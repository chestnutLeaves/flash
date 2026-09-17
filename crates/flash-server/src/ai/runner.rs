//! The one loop that takes queued generation jobs through the model.
//! A job is read (the source becomes a cached extraction, figures are
//! cut from a PDF and stored as media) and then written (the model's
//! cards become drafts on the job's review), with the job row telling
//! the client where it stands after each stage. Every model call is a
//! ledger row, answered or not; a failure leaves the job failed with a
//! sentence a person can act on, and the provider's own words only in
//! the journal.
//!
//! Two gates bound the work: the process-wide heavy-job permit (one job
//! per account at a time, a few across the process, shared with imports
//! and exports) and the runner's own `CONCURRENT_JOBS`, so two long
//! video readings cannot take every blocking thread. The loop wakes
//! when a job is queued and every `IDLE_TICK` regardless, and a job
//! that was mid-way when the process died is queued again at boot.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use flash_core::{MediaId, UserId};
use flash_store::richtext;
use flash_store::{
    AiCallStep, AiDraftNew, AiDraftRow, AiDraftStatus, AiExtractionNew, AiExtractionRow, AiJobOp,
    AiJobRow, AiJobState, AiSourceKind, AiSourceRow, AiTurnNew, AiTurnRole, StoreError,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

use super::ledger;
use super::prompts::PROMPT_VERSION;
use super::provider::{
    DraftNote, EditInput, ExtractInput, ExtractSource, Extraction, FigureRef, LlmError, Reply,
    SourcePart, Turn,
};
use super::sources;
use super::validate::{self, FigureMap};
use crate::media_store::{MediaStore, MediaStoreError};
use crate::service::{now_ms, SOURCE_EXPIRED};
use crate::state::{AiRuntime, AppState, HeavyBusy, HeavyPermit};

/// Jobs the runner works on at once.
pub const CONCURRENT_JOBS: usize = 2;
/// How long the loop sleeps between looks at the queue when nothing
/// wakes it.
const IDLE_TICK: Duration = Duration::from_secs(5);
/// How far down the queue one pass looks for a job it can start.
const QUEUE_BATCH: u32 = 50;
/// Blobs at or above this size take one of the server's blob-read
/// permits while they are in memory.
const LARGE_BLOB_BYTES: u64 = 4 * 1024 * 1024;

/// Progress a client sees while the source is being read, and once the
/// reading is in hand and the cards are being written.
const PROGRESS_EXTRACTING: u8 = 5;
const PROGRESS_GENERATING: u8 = 40;

/// The boot step: every job the previous process was working on goes
/// back to the queue (or fails, the second time). Returns how many.
pub fn requeue_interrupted(state: &AppState) -> u32 {
    match state.services.store().requeue_interrupted_ai_jobs(now_ms()) {
        Ok(n) => {
            if n > 0 {
                tracing::info!(jobs = n, "generation jobs requeued after restart");
            }
            n
        }
        Err(e) => {
            tracing::warn!("requeue generation jobs: {e}");
            0
        }
    }
}

/// The loop every binary spawns when a model is configured. Never
/// returns.
pub async fn run_jobs(state: AppState) {
    let Some(ai) = state.ai.clone() else {
        return;
    };
    let _ = tokio::task::spawn_blocking({
        let state = state.clone();
        move || requeue_interrupted(&state)
    })
    .await;
    let slots = Arc::new(Semaphore::new(CONCURRENT_JOBS));
    loop {
        // The handles are not awaited: each job holds its own permits
        // and wakes the loop when it is done.
        let _ = start_jobs(&state, &slots).await;
        tokio::select! {
            _ = ai.wake.notified() => {}
            _ = tokio::time::sleep(IDLE_TICK) => {}
        }
    }
}

/// One pass over the queue for tests: starts what can start, waits for
/// those jobs to finish, and returns how many were started.
pub async fn run_once(state: &AppState) -> u32 {
    let slots = Arc::new(Semaphore::new(CONCURRENT_JOBS));
    let handles = start_jobs(state, &slots).await;
    let started = handles.len() as u32;
    for handle in handles {
        let _ = handle.await;
    }
    started
}

/// A job taken off the queue with the two permits it runs under.
struct Claimed {
    user: UserId,
    job: AiJobRow,
    heavy: HeavyPermit,
    slot: OwnedSemaphorePermit,
}

async fn start_jobs(state: &AppState, slots: &Arc<Semaphore>) -> Vec<JoinHandle<()>> {
    let claimed = {
        let state = state.clone();
        let slots = slots.clone();
        tokio::task::spawn_blocking(move || claim_jobs(&state, &slots))
            .await
            .unwrap_or_default()
    };
    claimed
        .into_iter()
        .map(|claimed| tokio::spawn(run_claimed(state.clone(), claimed)))
        .collect()
}

/// Takes jobs off the head of the queue while there are slots and
/// permits for them. An account already running a heavy job is stepped
/// over; a process at its heavy-job limit ends the pass. A job is
/// claimed by moving it out of `queued` here, so no later pass takes
/// it again.
fn claim_jobs(state: &AppState, slots: &Arc<Semaphore>) -> Vec<Claimed> {
    let store = state.services.store();
    let queued = match store.queued_ai_jobs(QUEUE_BATCH) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("read generation queue: {e}");
            return Vec::new();
        }
    };
    let mut claimed = Vec::new();
    let mut busy: Vec<UserId> = Vec::new();
    for (user, job) in queued {
        if busy.contains(&user) {
            continue;
        }
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            break;
        };
        let heavy = match state.heavy.acquire(user) {
            Ok(permit) => permit,
            Err(HeavyBusy::Yours) => {
                busy.push(user);
                continue;
            }
            Err(HeavyBusy::Everyone) => break,
        };
        let taken = store.set_ai_job_state(
            user,
            job.id,
            AiJobState::Extracting,
            PROGRESS_EXTRACTING,
            None,
            now_ms(),
        );
        if let Err(e) = taken {
            tracing::warn!(job = job.id.0, "claim generation job: {e}");
            continue;
        }
        claimed.push(Claimed {
            user,
            job,
            heavy,
            slot,
        });
    }
    claimed
}

async fn run_claimed(state: AppState, claimed: Claimed) {
    let Claimed {
        user,
        job,
        heavy,
        slot,
    } = claimed;
    let id = job.id;
    let root = job.parent_job_id;
    if let Err(failure) = run_job(&state, user, job).await {
        // The provider's words go to the journal; the person gets a
        // sentence that says what to do next: on the job row, and, for
        // a pass of a review's thread, as the model's turn in it.
        tracing::warn!(
            user = user.raw(),
            job = id.0,
            "generation failed: {failure}"
        );
        let message = failure.user_message();
        let services = state.services.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let store = services.store();
            let now = now_ms();
            store.set_ai_job_state(user, id, AiJobState::Failed, 0, Some(&message), now)?;
            if let Some(root) = root {
                store.add_ai_turn(
                    user,
                    &AiTurnNew {
                        job_id: root,
                        role: AiTurnRole::Model,
                        text: &message,
                        child_job_id: Some(id),
                        now_ms: now,
                    },
                )?;
            }
            Ok::<(), StoreError>(())
        })
        .await;
        if let Ok(Err(e)) = outcome {
            tracing::warn!(job = id.0, "mark generation job failed: {e}");
        }
    }
    drop(heavy);
    drop(slot);
    if let Some(ai) = &state.ai {
        ai.wake.notify_one();
    }
}

/// Why a job stopped. `Display` is for the journal and may carry the
/// provider's excerpt; `user_message` is what the job row shows.
#[derive(Debug)]
enum JobFailure {
    Model(LlmError),
    Store(StoreError),
    /// Housekeeping took the source, or its blob is gone.
    Expired,
    /// The model answered but nothing in the answer could become a card.
    Empty,
    /// The model answered a message of the thread with no cards at
    /// all; the drafts were left alone rather than emptied.
    EmptyRevision,
    Internal(String),
}

impl JobFailure {
    fn user_message(&self) -> String {
        match self {
            JobFailure::Model(e) => e.user_message().to_string(),
            JobFailure::Expired => SOURCE_EXPIRED.to_string(),
            JobFailure::Empty => {
                "No cards could be made from this source. Try a different source or add an \
                 instruction."
                    .to_string()
            }
            JobFailure::EmptyRevision => {
                "The model returned no cards, so the drafts were left as they were. Try saying \
                 it differently."
                    .to_string()
            }
            JobFailure::Store(_) | JobFailure::Internal(_) => {
                "Something went wrong while saving the cards. Please try again.".to_string()
            }
        }
    }
}

impl std::fmt::Display for JobFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobFailure::Model(e) => write!(f, "model: {e}"),
            JobFailure::Store(e) => write!(f, "store: {e}"),
            JobFailure::Expired => f.write_str("source expired"),
            JobFailure::Empty => f.write_str("no usable drafts"),
            JobFailure::EmptyRevision => f.write_str("no drafts in the revision"),
            JobFailure::Internal(detail) => write!(f, "internal: {detail}"),
        }
    }
}

impl From<StoreError> for JobFailure {
    fn from(e: StoreError) -> Self {
        JobFailure::Store(e)
    }
}

impl From<MediaStoreError> for JobFailure {
    fn from(e: MediaStoreError) -> Self {
        match e {
            MediaStoreError::NotFound => JobFailure::Expired,
            other => JobFailure::Internal(format!("media store: {other}")),
        }
    }
}

/// The job, start to finish. The store work and the model calls are
/// all blocking, so the job runs as one blocking task under a model
/// permit; only the permits are taken here, on the async side.
async fn run_job(state: &AppState, user: UserId, job: AiJobRow) -> Result<(), JobFailure> {
    let ai = state
        .ai
        .clone()
        .ok_or_else(|| JobFailure::Internal("no model configured".into()))?;
    let (model, _) = ai.provider.model_names();
    let (sources, cached) = {
        let services = state.services.clone();
        let model = model.clone();
        let job = job.clone();
        join(
            tokio::task::spawn_blocking(move || load_reading(services.store(), user, &job, &model))
                .await,
        )??
    };
    let total: u64 = sources.iter().map(|s| s.size).sum();
    let _read = if total >= LARGE_BLOB_BYTES {
        Some(
            state
                .media_reads
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| JobFailure::Internal("media reads closed".into()))?,
        )
    } else {
        None
    };
    // The permit borrows the runtime for as long as the job runs; the
    // blocking task takes its own handle to the same runtime.
    let _call = ai
        .calls
        .acquire()
        .await
        .map_err(|_| JobFailure::Internal("model calls closed".into()))?;
    let runtime = ai.clone();
    let state = state.clone();
    join(
        tokio::task::spawn_blocking(move || {
            work(&state, &runtime, user, &job, &model, sources, cached)
        })
        .await,
    )?
}

/// A blocking task's result, or the panic it ended in as a failure.
fn join<T>(joined: Result<T, tokio::task::JoinError>) -> Result<T, JobFailure> {
    joined.map_err(|e| JobFailure::Internal(format!("join: {e}")))
}

/// The set's rows in order and, when there is one, the reading to
/// reuse: the job's own (a child pass) or the cache's for this set.
/// Any member housekeeping has taken means the whole set has expired.
fn load_reading(
    store: &flash_store::Store,
    user: UserId,
    job: &AiJobRow,
    model: &str,
) -> Result<(Vec<AiSourceRow>, Option<AiExtractionRow>), JobFailure> {
    let mut sources = Vec::new();
    for (_, row) in store.list_ai_job_sources(user, job.id)? {
        sources.push(row.ok_or(JobFailure::Expired)?);
    }
    if sources.is_empty() {
        return Err(JobFailure::Expired);
    }
    let cached = match job.extraction_id {
        Some(id) => store.get_ai_extraction(user, id)?,
        None => {
            let hash = sources::set_hash(&sources);
            store.find_ai_extraction(user, &hash, model, PROMPT_VERSION)?
        }
    };
    Ok((sources, cached))
}

/// Everything after the permits: the reading, then the writing.
fn work(
    state: &AppState,
    ai: &AiRuntime,
    user: UserId,
    job: &AiJobRow,
    model: &str,
    sources: Vec<AiSourceRow>,
    cached: Option<AiExtractionRow>,
) -> Result<(), JobFailure> {
    let store = state.services.store();
    let now = now_ms();
    let charged_as =
        AiSourceKind::costliest(sources.iter().map(|s| s.kind)).unwrap_or(AiSourceKind::Text);
    let extraction_row = match cached {
        Some(row) => {
            store.touch_ai_extraction(user, row.id, now)?;
            row
        }
        None => extract(state, ai, user, job, model, &sources)?,
    };
    store.set_ai_job_extraction(user, job.id, extraction_row.id)?;
    for source in &sources {
        store.touch_ai_source(user, source.id, now)?;
    }
    store.set_ai_job_state(
        user,
        job.id,
        AiJobState::Generating,
        PROGRESS_GENERATING,
        None,
        now,
    )?;

    let extraction = extraction_of(&extraction_row);
    let figures = figure_map(store, user, &extraction_row)?;
    let by_media: HashMap<i64, u32> = figures.iter().map(|(n, id)| (id.0, *n)).collect();
    let root = job.parent_job_id.unwrap_or(job.id);
    let rows = store.list_ai_drafts(user, root)?;
    let started = Instant::now();
    let started_at = now_ms();
    // The sentence the model wrote about an edit, once it has.
    let mut said: Option<String> = None;
    let (step, reply) = match job.op {
        AiJobOp::Generate => (
            AiCallStep::Generate,
            ai.provider.generate(&extraction, &job.instruction, &[]),
        ),
        AiJobOp::More => {
            // The model sees every card the person has not thrown away,
            // so it adds to the set rather than repeating it.
            let existing: Vec<DraftNote> = rows
                .iter()
                .filter(|r| r.status != AiDraftStatus::Discarded)
                .map(|r| note_of_row(r, &by_media))
                .collect();
            (
                AiCallStep::More,
                ai.provider
                    .generate(&extraction, &job.instruction, &existing),
            )
        }
        AiJobOp::Revise => {
            // One message of the review's thread: the model sees the
            // turns before it (not this pass's own, and not the
            // sentence of a pass that failed), the kept cards as
            // context, and the open drafts it may change.
            let thread: Vec<Turn> = store
                .list_ai_turns(user, root)?
                .into_iter()
                .filter(|t| t.child_job_id != Some(job.id))
                .filter(|t| {
                    !(t.role == AiTurnRole::Model && t.child_state == Some(AiJobState::Failed))
                })
                .map(|t| Turn {
                    role: t.role,
                    text: t.text,
                })
                .collect();
            let open: Vec<DraftNote> = rows
                .iter()
                .filter(|r| r.status.is_open())
                .map(|r| note_of_row(r, &by_media))
                .collect();
            let kept: Vec<DraftNote> = rows
                .iter()
                .filter(|r| r.status == AiDraftStatus::Kept)
                .map(|r| note_of_row(r, &by_media))
                .collect();
            let edit = EditInput {
                extraction: &extraction,
                thread: &thread,
                message: &job.instruction,
                open: &open,
                kept: &kept,
            };
            let reply = ai.provider.revise(&edit).map(|r| {
                said = Some(r.value.reply);
                Reply::new(r.value.cards, r.usage)
            });
            (AiCallStep::Revise, reply)
        }
    };
    record_call(
        store,
        state.services.ai_policy(),
        user,
        job,
        step,
        model,
        charged_as,
        &reply,
        started,
        started_at,
    )?;
    let reply = reply.map_err(JobFailure::Model)?;
    let drafts: Vec<AiDraftNew> = validate::drafts(reply.value, &figures)
        .into_iter()
        .map(|d| AiDraftNew {
            note_type: d.note_type,
            front_html: richtext::sanitize_with_media(&d.front_html).trimmed(),
            back_html: richtext::sanitize_with_media(&d.back_html).trimmed(),
            tags: d.tags,
            figure_media_ids: d.figure_media_ids.into_iter().map(MediaId).collect(),
        })
        .collect();
    if drafts.is_empty() {
        return Err(match job.op {
            AiJobOp::Revise => JobFailure::EmptyRevision,
            AiJobOp::Generate | AiJobOp::More => JobFailure::Empty,
        });
    }
    let now = now_ms();
    let turn: Option<String> = match job.op {
        AiJobOp::Generate => {
            store.replace_ai_drafts(user, root, &drafts, false, now)?;
            None
        }
        AiJobOp::Revise => {
            store.replace_ai_drafts(user, root, &drafts, true, now)?;
            let fallback = format!("Rewrote {}.", count_cards(drafts.len()));
            Some(validate::reply_sentence(
                said.as_deref().unwrap_or(""),
                &fallback,
            ))
        }
        AiJobOp::More => {
            store.append_ai_drafts(user, root, &drafts, now)?;
            Some(format!("Added {}.", count_cards(drafts.len())))
        }
    };
    if let Some(text) = turn {
        store.add_ai_turn(
            user,
            &AiTurnNew {
                job_id: root,
                role: AiTurnRole::Model,
                text: &text,
                child_job_id: Some(job.id),
                now_ms: now,
            },
        )?;
    }
    store.set_ai_job_state(user, job.id, AiJobState::Done, 100, None, now)?;
    Ok(())
}

/// "1 card" or "N cards".
fn count_cards(n: usize) -> String {
    if n == 1 {
        "1 card".to_string()
    } else {
        format!("{n} cards")
    }
}

/// The model's reading of a set that has none cached yet: the bytes go
/// to the model as numbered sources, the reply is validated, a PDF's
/// figures are cut from the member they name and stored as media, and
/// the extraction row is written under the set's hash with the figure
/// list carrying each crop's media id.
fn extract(
    state: &AppState,
    ai: &AiRuntime,
    user: UserId,
    job: &AiJobRow,
    model: &str,
    sources: &[AiSourceRow],
) -> Result<AiExtractionRow, JobFailure> {
    let store = state.services.store();
    let input = extract_input(state.media.as_ref(), sources)?;
    let started = Instant::now();
    let started_at = now_ms();
    let reply = ai.provider.extract(&input);
    record_call(
        store,
        state.services.ai_policy(),
        user,
        job,
        AiCallStep::Extract,
        model,
        input.costliest(),
        &reply,
        started,
        started_at,
    )?;
    let extraction = validate::extraction(reply.map_err(JobFailure::Model)?.value);
    // The PDFs a figure may be cut from, by the number the model knows
    // them as.
    let pdf_bytes: HashMap<u32, &[u8]> = input
        .sources
        .iter()
        .filter(|s| s.kind == AiSourceKind::Pdf)
        .filter_map(|s| match s.parts.first() {
            Some(SourcePart::Inline { bytes, .. }) => Some((s.index, bytes.as_slice())),
            _ => None,
        })
        .collect();
    let now = now_ms();
    let stored: Vec<StoredFigure> = extraction
        .figures
        .iter()
        .map(|figure| StoredFigure {
            media_id: pdf_bytes
                .get(&figure.source)
                .and_then(|bytes| crop_figure(state, ai, user, bytes, figure, now))
                .map(|id| id.0),
            figure: figure.clone(),
        })
        .collect();
    let figures_json = serde_json::to_string(&stored)
        .map_err(|e| JobFailure::Internal(format!("figures json: {e}")))?;
    let hash = sources::set_hash(sources);
    let id = store.insert_ai_extraction(
        user,
        &AiExtractionNew {
            source_id: sources[0].id,
            sha256: &hash,
            model,
            prompt_version: PROMPT_VERSION,
            text: &extraction.text,
            figures: &figures_json,
            est_words: extraction.est_words,
            now_ms: now,
        },
    )?;
    for figure in &stored {
        if let Some(media) = figure.media_id {
            store.add_ai_figure(user, id, MediaId(media))?;
        }
    }
    store
        .get_ai_extraction(user, id)?
        .ok_or_else(|| JobFailure::Internal("extraction row vanished after insert".into()))
}

/// The set as the model takes it: each member numbered from 1 and
/// named, with its text, its link, its blob, or (a photo set stored
/// before every photo was its own source) the photos unpacked from
/// their blob.
fn extract_input(media: &dyn MediaStore, rows: &[AiSourceRow]) -> Result<ExtractInput, JobFailure> {
    let mut out = Vec::with_capacity(rows.len());
    for (i, source) in rows.iter().enumerate() {
        let parts = match source.kind {
            AiSourceKind::Text => vec![SourcePart::Text(source.text.clone().unwrap_or_default())],
            AiSourceKind::Youtube => vec![SourcePart::YoutubeUrl(
                source.url.clone().ok_or(JobFailure::Expired)?,
            )],
            AiSourceKind::Pdf | AiSourceKind::Video | AiSourceKind::Audio => {
                let blob = media.get(&source.sha256, None)?;
                vec![SourcePart::Inline {
                    mime: source.mime.clone(),
                    bytes: blob.bytes,
                }]
            }
            AiSourceKind::Image if source.mime == "application/octet-stream" => {
                let blob = media.get(&source.sha256, None)?;
                sources::unpack_images(&blob.bytes)
                    .map_err(JobFailure::Internal)?
                    .into_iter()
                    .map(|f| SourcePart::Inline {
                        mime: f.mime,
                        bytes: f.bytes,
                    })
                    .collect()
            }
            AiSourceKind::Image => {
                let blob = media.get(&source.sha256, None)?;
                vec![SourcePart::Inline {
                    mime: source.mime.clone(),
                    bytes: blob.bytes,
                }]
            }
        };
        out.push(ExtractSource {
            index: (i + 1) as u32,
            kind: source.kind,
            label: source_label(source),
            parts,
            duration_s: source.duration_s,
        });
    }
    Ok(ExtractInput { sources: out })
}

/// What the model is told a source is called: the file's name, the
/// link, or "Pasted text".
pub fn source_label(row: &AiSourceRow) -> String {
    if let Some(url) = &row.url {
        return url.clone();
    }
    if row.filename.is_empty() {
        "Pasted text".to_string()
    } else {
        row.filename.clone()
    }
}

/// A figure cut from the PDF and stored as one of the user's media
/// rows, so a kept card can carry it. A crop that fails (too small, a
/// page the model imagined, the media cap) loses that figure and
/// nothing else.
#[cfg(feature = "pdf")]
fn crop_figure(
    state: &AppState,
    ai: &AiRuntime,
    user: UserId,
    bytes: &[u8],
    figure: &FigureRef,
    now_ms: i64,
) -> Option<MediaId> {
    let png = {
        let pdf = ai.pdf()?;
        match pdf.crop_figure_png(bytes, figure.page, figure.bbox) {
            Ok(png) => png,
            Err(e) => {
                tracing::warn!(figure = figure.n, "figure crop: {e}");
                return None;
            }
        }
    };
    let name = format!(
        "figure-s{}-p{}-{}.png",
        figure.source, figure.page, figure.n
    );
    match crate::media::ingest_media(
        &state.services,
        state.media.as_ref(),
        user,
        false,
        &name,
        &png,
        now_ms,
    ) {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(figure = figure.n, "figure ingest: {e}");
            None
        }
    }
}

#[cfg(not(feature = "pdf"))]
fn crop_figure(
    _state: &AppState,
    _ai: &AiRuntime,
    _user: UserId,
    _bytes: &[u8],
    _figure: &FigureRef,
    _now_ms: i64,
) -> Option<MediaId> {
    None
}

/// A figure as the extraction row keeps it: the model's description
/// plus the media row its crop became, when it did.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredFigure {
    #[serde(flatten)]
    figure: FigureRef,
    #[serde(default)]
    media_id: Option<i64>,
}

fn stored_figures(row: &AiExtractionRow) -> Vec<StoredFigure> {
    serde_json::from_str(&row.figures).unwrap_or_default()
}

fn extraction_of(row: &AiExtractionRow) -> Extraction {
    Extraction {
        text: row.text.clone(),
        figures: stored_figures(row).into_iter().map(|s| s.figure).collect(),
        est_words: row.est_words,
    }
}

/// Figure number to media id, for the crops that were made and are
/// still pinned to the extraction: the only images a draft may place.
fn figure_map(
    store: &flash_store::Store,
    user: UserId,
    row: &AiExtractionRow,
) -> Result<FigureMap, JobFailure> {
    let pinned = store.ai_figure_media_ids(user, row.id)?;
    Ok(stored_figures(row)
        .into_iter()
        .filter_map(|s| {
            let id = MediaId(s.media_id?);
            pinned.contains(&id).then_some((s.figure.n, id))
        })
        .collect())
}

/// A stored draft as the model wrote it, for a pass that sees the
/// existing cards: the sides back to text with each figure image as
/// its `{{figure:N}}` token.
fn note_of_row(row: &AiDraftRow, by_media: &HashMap<i64, u32>) -> DraftNote {
    DraftNote {
        note_type: row.note_type,
        front: text_with_figure_tokens(&row.front_html, by_media),
        back: text_with_figure_tokens(&row.back_html, by_media),
        tags: row.tags.clone(),
        figures: row
            .figure_media_ids
            .iter()
            .filter_map(|m| by_media.get(&m.0).copied())
            .collect(),
    }
}

fn text_with_figure_tokens(html: &str, by_media: &HashMap<i64, u32>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find("<img") {
        out.push_str(&rest[..at]);
        let tag_end = rest[at..]
            .find('>')
            .map(|i| at + i + 1)
            .unwrap_or(rest.len());
        if let Some(n) = media_id_in_img(&rest[at..tag_end]).and_then(|id| by_media.get(&id)) {
            out.push_str(&format!(" {{{{figure:{n}}}}} "));
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    richtext::html_to_text(&out)
}

/// The id in an `<img src="/media/{id}">` tag.
fn media_id_in_img(tag: &str) -> Option<i64> {
    let at = tag.find("/media/")?;
    let digits: String = tag[at + "/media/".len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// One ledger row per model call, priced as it lands; see `ledger`.
#[allow(clippy::too_many_arguments)]
fn record_call<T>(
    store: &flash_store::Store,
    policy: &dyn crate::policy::AiPolicy,
    user: UserId,
    job: &AiJobRow,
    step: AiCallStep,
    model: &str,
    kind: AiSourceKind,
    reply: &Result<Reply<T>, LlmError>,
    started: Instant,
    started_at: i64,
) -> Result<(), JobFailure> {
    ledger::record(
        store,
        policy,
        user,
        &ledger::Call {
            job: Some(job.id),
            step,
            model,
            kind,
            started,
            started_at,
        },
        reply,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_core::AiJobId;
    use flash_store::notes::NoteType;

    #[test]
    fn a_draft_goes_back_to_the_model_with_its_figure_tokens() {
        let by_media: HashMap<i64, u32> = [(41, 1), (42, 2)].into_iter().collect();
        let row = AiDraftRow {
            id: flash_core::AiDraftId(1),
            job_id: AiJobId(1),
            ord: 0,
            note_type: NoteType::Basic,
            front_html: r#"<p>Label the valves <img src="/media/41"></p>"#.into(),
            back_html: r#"See <img src="/media/42" alt="x"> and <img src="/media/99">"#.into(),
            tags: vec!["anatomy".into()],
            figure_media_ids: vec![MediaId(41), MediaId(42), MediaId(99)],
            status: AiDraftStatus::Pending,
            note_id: None,
            created_at: 0,
            updated_at: 0,
        };
        let note = note_of_row(&row, &by_media);
        assert!(note.front.contains("{{figure:1}}"), "{}", note.front);
        assert!(note.front.starts_with("Label the valves"));
        assert!(note.back.contains("{{figure:2}}"), "{}", note.back);
        assert!(!note.back.contains("99"));
        assert_eq!(note.figures, vec![1, 2]);
        assert_eq!(note.tags, vec!["anatomy"]);
    }

    #[test]
    fn stored_figures_round_trip_with_their_media() {
        let stored = vec![StoredFigure {
            figure: FigureRef {
                n: 1,
                source: 2,
                page: 2,
                bbox: [0.1, 0.2, 0.3, 0.4],
                caption: "c".into(),
            },
            media_id: Some(7),
        }];
        let json = serde_json::to_string(&stored).unwrap();
        let back: Vec<StoredFigure> = serde_json::from_str(&json).unwrap();
        assert_eq!(back[0].media_id, Some(7));
        assert_eq!((back[0].figure.source, back[0].figure.page), (2, 2));
        // The plain model shape, without media ids, still reads; a
        // reading from before sets names the only source.
        let plain: Vec<StoredFigure> =
            serde_json::from_str(r#"[{"n":1,"page":1,"bbox":[0,0,1,1],"caption":"x"}]"#).unwrap();
        assert_eq!(plain[0].media_id, None);
        assert_eq!(plain[0].figure.source, 1);
    }

    #[test]
    fn a_pass_counts_its_cards_in_words() {
        assert_eq!(count_cards(1), "1 card");
        assert_eq!(count_cards(0), "0 cards");
        assert_eq!(count_cards(12), "12 cards");
    }
}
