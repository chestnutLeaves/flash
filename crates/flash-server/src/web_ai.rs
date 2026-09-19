//! Card generation in the browser: the composer that takes material in
//! (each file answered with its reading as JSON, so the page's script
//! can show it on a chip), the set's preflight before anything is
//! generated, the job as it runs, and the review of what the model
//! wrote. Every domain call is `Services`'; this module only turns a
//! request into one and the answer into markup or JSON.
//!
//! The core never learns what a plan sells. It offers every input this
//! server has enabled, marks a kind the plan withholds with the plan's
//! own sentence, and the quota line beside the composer is whatever the
//! extension renders into `Slot::AiQuota`.

use std::sync::Arc;

use askama::Template;
use axum::extract::{FromRequest, Multipart, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::{Form, Json};
use flash_core::{AiDraftId, AiJobId, AiSourceId, DeckId, UserId};
use flash_store::notes::NoteType;
use flash_store::{AiDraftRow, AiDraftStatus, AiJobState, AiTurnRow};
use serde::Deserialize;

use crate::ai::caps::Caps;
use crate::ai::runner::source_label;
use crate::api::ai::{busy, ingest, with_retry_after, SetPreflight, Upload};
use crate::api::{ApiError, ApiJson};
use crate::auth::{ApiUser, AuthUser, Theme};
use crate::ext::Slot;
use crate::service::{facts_line, now_ms, DeckRow, DeckTarget, ServiceError, Services};
use crate::state::{AiRuntime, AppState};
use crate::web::{act, respond, EditorView, NoteForm, Shell};

/// The runtime, or the answer a server with no model configured gives.
fn require_ai(state: &AppState) -> Result<Arc<AiRuntime>, Response> {
    state
        .ai()
        .map(Arc::clone)
        .map_err(IntoResponse::into_response)
}

/// Whether htmx sent this request, so a handler can answer with a
/// fragment or a swap instruction rather than a page.
fn from_htmx(headers: &HeaderMap) -> bool {
    headers.contains_key("hx-request")
}

fn render<T: Template>(page: T) -> Response {
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("template render: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render error").into_response()
        }
    }
}

// ---- the generate page ----

/// What the composer may take, as the template prints it: the whole
/// picture as JSON for the script (escaped into a data attribute), and
/// the few values the markup itself needs.
struct CapsView {
    json: String,
    /// The file input's `accept` attribute.
    accept: String,
    /// "Drop PDFs, photos, video or audio, or paste a YouTube link."
    hint: String,
    youtube: bool,
    instruction_max: usize,
    url_max: usize,
}

impl CapsView {
    fn new(caps: &Caps) -> Self {
        CapsView {
            json: serde_json::to_string(caps).unwrap_or_else(|_| "{}".to_string()),
            accept: caps.accept_attr(),
            hint: caps.hint(),
            youtube: caps.youtube,
            instruction_max: caps.instruction_max,
            url_max: caps.url_max,
        }
    }
}

#[derive(Template)]
#[template(path = "ai_generate.html")]
struct GeneratePage {
    shell: Shell,
    decks: Vec<DeckRow>,
    /// The deck the entry point named, preselected; 0 when the page was
    /// opened from the deck list.
    deck_id: i64,
    caps: CapsView,
    /// The extension's quota line; empty on the open server.
    quota_html: String,
    /// A generation already running for this account: only one at a time.
    active_job: Option<i64>,
}

pub async fn generate_page(
    State(state): State<AppState>,
    user: AuthUser,
    theme: Theme,
) -> Response {
    generate(state, user, theme, 0).await
}

pub async fn deck_generate_page(
    State(state): State<AppState>,
    user: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
) -> Response {
    generate(state, user, theme, id).await
}

async fn generate(state: AppState, auth: AuthUser, theme: Theme, deck_id: i64) -> Response {
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let AuthUser { id: user, is_admin } = auth;
    let shell = Shell::new(&state, theme, "decks", is_admin);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        // A deck named in the path is the user's or the page is not
        // theirs to open.
        if deck_id != 0 {
            s.deck_by_id(user, DeckId(deck_id), now_ms())?;
        }
        let admissions = s.ai_kind_admissions(user, &ai, now_ms())?;
        let caps = Caps::of(&ai, &admissions);
        Ok(GeneratePage {
            shell,
            decks: active_decks(&s, user)?,
            deck_id,
            caps: CapsView::new(&caps),
            quota_html: app.ext.render(&app, Slot::AiQuota { user })?,
            active_job: s.ai_active_job(user)?.map(|job| job.id.0),
        })
    })
    .await
}

// ---- the sources, and their readings ----

/// Text or a link typed into the composer, sent as an ordinary form.
#[derive(Deserialize)]
pub struct SourceForm {
    #[serde(default, deserialize_with = "crate::bounds::text_source")]
    text: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::youtube_url")]
    youtube_url: Option<String>,
}

/// `POST /ai/sources`: one piece of material from the composer, as a
/// multipart body with one `file`, or an ordinary form with `text` or
/// `youtube_url`. Answers JSON, the same shape the API gives, so the
/// page's script fills the chip; a refusal is the API's error envelope.
pub async fn create_source(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    req: Request,
) -> Response {
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let multipart = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
        .starts_with("multipart/form-data");
    // Held until the bytes are stored: the body limit is per request,
    // and a handful of concurrent uploads must fit the process. Only a
    // body that can carry a file needs it.
    let mut upload_permit = None;
    let upload = if multipart {
        match state.upload_semaphore.clone().try_acquire_owned() {
            Ok(permit) => upload_permit = Some(permit),
            Err(_) => return with_retry_after(busy().into_response()),
        }
        let parts = match Multipart::from_request(req, &state).await {
            Ok(parts) => parts,
            Err(e) => return ApiError::invalid(e.body_text()).into_response(),
        };
        let fields = match crate::api::ai::read_multipart(parts).await {
            Ok(fields) => fields,
            Err(e) => return e.into_response(),
        };
        match (fields.file, fields.text, fields.youtube_url) {
            (Some((name, bytes)), _, _) => Upload::File(name, bytes),
            (None, Some(text), _) if !text.trim().is_empty() => Upload::Text(text),
            (None, _, Some(url)) if !url.trim().is_empty() => Upload::Youtube(url),
            _ => {
                return ApiError::invalid("Add something for the model to read first.")
                    .into_response()
            }
        }
    } else {
        match Form::<SourceForm>::from_request(req, &state).await {
            Ok(Form(form)) => match (
                form.text.filter(|t| !t.trim().is_empty()),
                form.youtube_url.filter(|u| !u.trim().is_empty()),
            ) {
                (Some(text), _) => Upload::Text(text),
                (None, Some(url)) => Upload::Youtube(url),
                (None, None) => {
                    return ApiError::invalid("Add something for the model to read first.")
                        .into_response()
                }
            },
            Err(e) => return ApiError::invalid(e.body_text()).into_response(),
        }
    };
    let media = state.media.clone();
    let created = tokio::task::spawn_blocking({
        let services = state.services.clone();
        move || {
            let created = ingest(&services, user, upload, media.as_ref(), &ai);
            drop(upload_permit);
            created
        }
    })
    .await;
    match created {
        Ok(Ok(created)) => (StatusCode::CREATED, Json(created)).into_response(),
        Ok(Err(e)) => ApiError::from(e).into_response(),
        Err(e) => {
            tracing::error!("join: {e}");
            ApiError::internal().into_response()
        }
    }
}

/// `POST /ai/preflight`: what one generation over the chips would be,
/// as JSON, so the page enables Generate and prints the set's line.
#[derive(Deserialize)]
pub struct PreflightBody {
    #[serde(deserialize_with = "crate::bounds::source_ids")]
    source_ids: Vec<i64>,
}

pub async fn preflight_set(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    ApiJson(body): ApiJson<PreflightBody>,
) -> Response {
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let ids: Vec<AiSourceId> = body.source_ids.into_iter().map(AiSourceId).collect();
    let outcome = tokio::task::spawn_blocking({
        let services = state.services.clone();
        move || {
            let p = services.ai_preflight_set(user, &ids, &ai, now_ms())?;
            let quota = crate::api::ai::quota_of(&services, user, &p.usage)?;
            Ok::<_, ServiceError>(SetPreflight::new(p, quota))
        }
    })
    .await;
    match outcome {
        Ok(Ok(preflight)) => Json(preflight).into_response(),
        Ok(Err(e)) => ApiError::from(e).into_response(),
        Err(e) => {
            tracing::error!("join: {e}");
            ApiError::internal().into_response()
        }
    }
}

/// `DELETE /ai/sources/{id}`: a chip taken off the composer.
pub async fn delete_source(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let media = state.media.clone();
    let outcome = tokio::task::spawn_blocking({
        let services = state.services.clone();
        move || services.ai_delete_source(user, AiSourceId(id), media.as_ref())
    })
    .await;
    match outcome {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => ApiError::from(e).into_response(),
        Err(e) => {
            tracing::error!("join: {e}");
            ApiError::internal().into_response()
        }
    }
}

// ---- the job ----

#[derive(Deserialize)]
pub struct JobForm {
    /// The chips' source ids, comma-joined, in composer order.
    #[serde(deserialize_with = "crate::bounds::source_id_list")]
    source_ids: String,
    /// The deck the switcher names, or 0 for the deck `deck_name` names.
    deck_id: i64,
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    deck_name: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_instruction")]
    instruction: Option<String>,
}

/// What the deck switchers post: an id, or the 0 that stands for "a new
/// deck" and the name typed beside it.
fn deck_target(deck_id: i64, name: Option<String>) -> DeckTarget {
    if deck_id == 0 {
        DeckTarget::New(name.unwrap_or_default())
    } else {
        DeckTarget::Existing(DeckId(deck_id))
    }
}

/// "12,15,19" as ids; anything that is not a number is refused with a
/// sentence, since the page never writes one.
fn parse_source_ids(list: &str) -> Result<Vec<AiSourceId>, ServiceError> {
    let mut ids = Vec::new();
    for piece in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let id: i64 = piece
            .parse()
            .map_err(|_| ServiceError::Invalid("That source list is not valid.".into()))?;
        if ids.len() >= crate::bounds::AI_SOURCES_PER_JOB {
            return Err(ServiceError::Invalid(crate::service::too_many_sources()));
        }
        ids.push(AiSourceId(id));
    }
    Ok(ids)
}

pub async fn create_job(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<JobForm>,
) -> Response {
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let runtime = ai.clone();
    match act(state.services.clone(), move |s| {
        let ids = parse_source_ids(&form.source_ids)?;
        s.ai_create_job(
            user,
            &ids,
            deck_target(form.deck_id, form.deck_name),
            form.instruction.as_deref().unwrap_or(""),
            &runtime,
            now_ms(),
        )
    })
    .await
    {
        Ok(queued) => {
            ai.wake.notify_one();
            if queued.deck_created {
                crate::web::account_event(&state, user, "deck_created", None, None);
            }
            crate::web::account_event(&state, user, "ai_job", Some("generate"), None);
            Redirect::to(&format!("/ai/jobs/{}", queued.job)).into_response()
        }
        Err(response) => response,
    }
}

/// The progress block, shared by the job page and the poll that replaces
/// it. Both contexts carry the same names so one template renders from
/// either.
struct JobProgress {
    job_id: i64,
    /// Whether the block asks for itself again in two seconds; a
    /// finished job renders without the trigger, so polling stops.
    polling: bool,
    state_label: &'static str,
    progress: u8,
    /// The runner's user-safe sentence, when the job failed.
    error: Option<String>,
    /// Where "Try again" and "Back to deck" go.
    retry_href: String,
    back_href: String,
}

impl JobProgress {
    fn of(detail: &crate::service::AiJobDetail) -> Self {
        let deck = detail.job.deck_id.map(|d| d.0);
        JobProgress {
            job_id: detail.job.id.0,
            polling: detail.job.state.is_active(),
            state_label: match detail.job.state {
                AiJobState::Queued => "Queued…",
                AiJobState::Extracting => "Reading the source…",
                AiJobState::Generating => "Writing cards…",
                AiJobState::Done => "Done.",
                AiJobState::Failed => "Stopped.",
            },
            progress: detail.job.progress,
            error: detail.job.error.clone(),
            retry_href: match deck {
                Some(id) => format!("/decks/{id}/generate"),
                None => "/generate".to_string(),
            },
            back_href: match deck {
                Some(id) => format!("/decks/{id}"),
                None => "/decks".to_string(),
            },
        }
    }
}

/// One member of the set a job reads, as the job page lists it.
struct SourceLine {
    name: String,
    /// "12 pages · ~4,800 words", or the reason it cannot be read.
    line: String,
}

#[derive(Template)]
#[template(path = "ai_job.html")]
struct JobPage {
    shell: Shell,
    job_id: i64,
    polling: bool,
    state_label: &'static str,
    progress: u8,
    error: Option<String>,
    retry_href: String,
    back_href: String,
    sources: Vec<SourceLine>,
    sources_expired: u32,
}

#[derive(Template)]
#[template(path = "ai_job_progress.html")]
struct JobProgressPartial {
    job_id: i64,
    polling: bool,
    state_label: &'static str,
    progress: u8,
    error: Option<String>,
    retry_href: String,
    back_href: String,
}

pub async fn job_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let shell = Shell::new(&state, theme, "decks", is_admin);
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let (detail, sources) = match act(state.services.clone(), move |s| {
        let detail = s.ai_job_detail(user, AiJobId(id))?;
        let now = now_ms();
        let mut sources = Vec::with_capacity(detail.sources.len());
        for row in &detail.sources {
            let preflight = s.ai_preflight(user, row.id, &ai, now)?;
            sources.push(SourceLine {
                name: source_label(row),
                line: facts_line(&preflight),
            });
        }
        Ok((detail, sources))
    })
    .await
    {
        Ok(pair) => pair,
        Err(response) => return response,
    };
    let review = format!("/ai/jobs/{id}/review");
    // A finished job has nothing left to show here: the poll is told to
    // go to the review, and so is anyone who opens the page late.
    if detail.job.state == AiJobState::Done {
        return if from_htmx(&headers) {
            (StatusCode::OK, [("HX-Redirect", review)]).into_response()
        } else {
            Redirect::to(&review).into_response()
        };
    }
    let p = JobProgress::of(&detail);
    if from_htmx(&headers) {
        render(JobProgressPartial {
            job_id: p.job_id,
            polling: p.polling,
            state_label: p.state_label,
            progress: p.progress,
            error: p.error,
            retry_href: p.retry_href,
            back_href: p.back_href,
        })
    } else {
        render(JobPage {
            shell,
            job_id: p.job_id,
            polling: p.polling,
            state_label: p.state_label,
            progress: p.progress,
            error: p.error,
            retry_href: p.retry_href,
            back_href: p.back_href,
            sources,
            sources_expired: detail.sources_expired,
        })
    }
}

// ---- the review ----

/// One draft as the review shows it.
struct DraftRowView {
    id: i64,
    front_html: String,
    back_html: String,
    tags: String,
    /// The crops this draft placed, as thumbnails beside its front.
    figures: Vec<i64>,
    status: &'static str,
    status_label: &'static str,
    badge_class: &'static str,
    /// Whether keep, edit and discard still apply.
    open: bool,
    /// Why the last decision on this row did not go through.
    notice: Option<String>,
}

impl DraftRowView {
    fn new(draft: AiDraftRow, notice: Option<String>) -> Self {
        let (status_label, badge_class) = match draft.status {
            AiDraftStatus::Pending => ("new", "fl-badge"),
            AiDraftStatus::Edited => ("edited", "fl-badge"),
            AiDraftStatus::Kept => ("kept", "fl-badge fl-badge-good"),
            AiDraftStatus::Discarded => ("discarded", "fl-badge"),
        };
        DraftRowView {
            id: draft.id.0,
            front_html: draft.front_html,
            back_html: draft.back_html,
            tags: draft.tags.join(", "),
            figures: draft.figure_media_ids.into_iter().map(|m| m.0).collect(),
            status: draft.status.as_str(),
            status_label,
            badge_class,
            open: draft.status.is_open(),
            notice,
        }
    }
}

/// The drafts of one note type, under the label the editor uses for it.
struct DraftGroup {
    label: &'static str,
    rows: Vec<DraftRowView>,
}

/// The drafts of one review as the page shows them: the header, the
/// counts, the toolbar and the rows. Rendered on the page, and again
/// out of band when a pass of the thread has changed the drafts.
struct ReviewBody {
    job_id: i64,
    /// The deck the generation was queued into, preselected on the
    /// switcher; 0 once that deck has been deleted.
    deck_id: i64,
    decks: Vec<DeckRow>,
    pending: u32,
    kept: u32,
    discarded: u32,
    groups: Vec<DraftGroup>,
    /// What the source was, for the header line.
    source_name: String,
    /// The card policy's sentence when a keep stopped early.
    notice: Option<String>,
}

/// One turn of the thread as a bubble.
struct TurnView {
    /// `user` or `model`: the bubble's side.
    role: &'static str,
    text: String,
    /// The model's sentence for a pass that failed.
    failed: bool,
}

impl TurnView {
    fn of(turn: AiTurnRow) -> Self {
        TurnView {
            role: turn.role.as_str(),
            text: turn.text,
            failed: turn.child_state == Some(AiJobState::Failed),
        }
    }
}

/// The thread block: the turns, whether a pass is running (the block
/// then polls and the box is closed), and what the box shows.
struct ThreadView {
    /// The root job: what the box posts to and the poll asks for.
    job_id: i64,
    turns: Vec<TurnView>,
    active: bool,
    /// Why the last message was not taken.
    error: Option<String>,
    /// The message echoed back into the box when it was not taken.
    draft: String,
    message_max: usize,
}

#[derive(Template)]
#[template(path = "ai_review.html")]
struct ReviewPage {
    shell: Shell,
    review: ReviewBody,
    thread: ThreadView,
}

/// What a send or a poll of the thread answers with: the block, and the
/// review body out of band once no pass is running.
#[derive(Template)]
#[template(path = "ai_thread_reply.html")]
struct ThreadReply {
    thread: ThreadView,
    review: Option<ReviewBody>,
}

/// The set as the review's header names it: "notes.pdf", "notes.pdf
/// and 6 photos", "notes.pdf, 6 photos and 1 link", or what is left
/// of it once housekeeping has taken some.
/// The decks a generation can be filed into: an archived deck is not
/// offered, though a job already aimed at one keeps its name.
fn active_decks(s: &Services, user: UserId) -> Result<Vec<DeckRow>, ServiceError> {
    Ok(s.list_decks(user, now_ms())?
        .into_iter()
        .filter(|d| d.archived_at.is_none())
        .collect())
}

fn sources_phrase(sources: &[flash_store::AiSourceRow], expired: u32) -> String {
    use flash_store::AiSourceKind;
    let mut parts: Vec<String> = Vec::new();
    let photos = sources
        .iter()
        .filter(|s| s.kind == AiSourceKind::Image)
        .count();
    let links = sources
        .iter()
        .filter(|s| s.kind == AiSourceKind::Youtube)
        .count();
    for row in sources {
        match row.kind {
            AiSourceKind::Image if photos > 1 => {}
            AiSourceKind::Youtube if links > 1 => {}
            _ => parts.push(source_label(row)),
        }
    }
    if photos > 1 {
        parts.push(format!("{photos} photos"));
    }
    if links > 1 {
        parts.push(format!("{links} links"));
    }
    if expired > 0 {
        parts.push(match expired {
            1 => "1 expired source".to_string(),
            n => format!("{n} expired sources"),
        });
    }
    match parts.len() {
        0 => String::new(),
        1 => parts.remove(0),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

fn review_page_of(
    s: &Services,
    shell: Shell,
    user: UserId,
    job: AiJobId,
    notice: Option<String>,
) -> Result<ReviewPage, ServiceError> {
    Ok(ReviewPage {
        shell,
        review: review_body_of(s, user, job, notice)?,
        thread: thread_view_of(s, user, job, None, String::new())?,
    })
}

fn thread_view_of(
    s: &Services,
    user: UserId,
    job: AiJobId,
    error: Option<String>,
    draft: String,
) -> Result<ThreadView, ServiceError> {
    let thread = s.ai_thread(user, job)?;
    Ok(ThreadView {
        job_id: thread.root.id.0,
        turns: thread.turns.into_iter().map(TurnView::of).collect(),
        active: thread.active_job_id.is_some(),
        error,
        draft,
        message_max: crate::bounds::AI_INSTRUCTION,
    })
}

fn review_body_of(
    s: &Services,
    user: UserId,
    job: AiJobId,
    notice: Option<String>,
) -> Result<ReviewBody, ServiceError> {
    let detail = s.ai_job_detail(user, job)?;
    let drafts = s.ai_drafts(user, job)?;
    // Grouped in the editor's own order, so the review reads like the
    // type picker rather than like the model's output order.
    let groups: Vec<DraftGroup> = NoteType::ALL
        .iter()
        .filter_map(|kind| {
            let rows: Vec<DraftRowView> = drafts
                .iter()
                .filter(|d| d.note_type == *kind)
                .cloned()
                .map(|d| DraftRowView::new(d, None))
                .collect();
            (!rows.is_empty()).then(|| DraftGroup {
                label: kind.label(),
                rows,
            })
        })
        .collect();
    Ok(ReviewBody {
        job_id: detail.root.0,
        deck_id: detail.job.deck_id.map(|d| d.0).unwrap_or(0),
        decks: active_decks(s, user)?,
        pending: detail.counts.pending,
        kept: detail.counts.kept,
        discarded: detail.counts.discarded,
        groups,
        source_name: sources_phrase(&detail.sources, detail.sources_expired),
        notice,
    })
}

pub async fn review_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let shell = Shell::new(&state, theme, "decks", is_admin);
    respond(state.services.clone(), move |s| {
        review_page_of(&s, shell, user, AiJobId(id), None)
    })
    .await
}

// ---- one draft at a time ----

#[derive(Template)]
#[template(path = "ai_draft_row.html")]
struct DraftRowPartial {
    row: DraftRowView,
}

/// The same row, plus the out-of-band swap that closes the editor: what
/// a save from the dialog answers with.
#[derive(Template)]
#[template(path = "ai_draft_saved.html")]
struct DraftSavedPartial {
    row: DraftRowView,
}

/// The deck the review's switcher currently names, sent along by every
/// per-row keep. A `deck_id` of 0 is the switcher's "new deck" entry,
/// and `deck_name` is what was typed beside it; the second row of a keep
/// finds the deck the first row made, so one name is one deck.
#[derive(Deserialize)]
pub struct DeckForm {
    #[serde(default)]
    deck_id: Option<i64>,
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    deck_name: Option<String>,
}

impl DeckForm {
    fn target(self) -> Option<DeckTarget> {
        self.deck_id.map(|id| deck_target(id, self.deck_name))
    }
}

pub async fn keep_draft(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Form(form): Form<DeckForm>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let draft = AiDraftId(id);
    match act(state.services.clone(), move |s| {
        let job = s.ai_draft(user, draft)?.job_id;
        let report = s.ai_keep_drafts(user, job, Some(&[draft]), form.target(), now_ms())?;
        Ok(DraftRowPartial {
            row: DraftRowView::new(s.ai_draft(user, draft)?, report.stopped_by),
        })
    })
    .await
    {
        Ok(partial) => render(partial),
        Err(response) => response,
    }
}

pub async fn discard_draft(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let draft = AiDraftId(id);
    match act(state.services.clone(), move |s| {
        s.ai_discard_draft(user, draft, now_ms())?;
        Ok(DraftRowPartial {
            row: DraftRowView::new(s.ai_draft(user, draft)?, None),
        })
    })
    .await
    {
        Ok(partial) => render(partial),
        Err(response) => response,
    }
}

#[derive(Template)]
#[template(path = "ai_draft_editor.html")]
struct DraftEditorDialog {
    draft_id: i64,
    editor: EditorView,
    error: Option<String>,
}

/// The card editor, seeded from a draft: the same dialog the deck page
/// opens over a card, posting to the draft instead of a note.
pub async fn draft_editor(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    respond(state.services.clone(), move |s| {
        let draft = s.ai_draft(user, AiDraftId(id))?;
        Ok(DraftEditorDialog {
            draft_id: draft.id.0,
            editor: EditorView::from_draft(&draft),
            error: None,
        })
    })
    .await
}

/// Saves the dialog. Success swaps the row and empties the dialog in one
/// reply; a refusal re-renders the dialog with the reason, retargeted so
/// nothing typed is lost.
pub async fn save_draft(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Form(form): Form<NoteForm>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let draft = AiDraftId(id);
    let saved = act(state.services.clone(), move |s| {
        let input = form.into_input()?;
        let current = s.ai_draft(user, draft)?;
        match s.ai_update_draft(
            user,
            draft,
            input.note_type,
            &input.front_html,
            &input.back_html,
            &input.tags,
            now_ms(),
        ) {
            Ok(()) => Ok(Ok(DraftSavedPartial {
                row: DraftRowView::new(s.ai_draft(user, draft)?, None),
            })),
            Err(err @ ServiceError::Invalid(_)) => {
                let mut editor = EditorView::from_draft(&current);
                // Echo what was typed, not what is stored.
                editor.note_type = input.note_type.as_str();
                editor.front_html =
                    flash_store::richtext::sanitize_with_media(&input.front_html).into_string();
                editor.back_html =
                    flash_store::richtext::sanitize_with_media(&input.back_html).into_string();
                editor.tags = input.tags.join(", ");
                Ok(Err(DraftEditorDialog {
                    draft_id: draft.0,
                    editor,
                    error: Some(err.to_string()),
                }))
            }
            Err(other) => Err(other),
        }
    })
    .await;
    match saved {
        Ok(Ok(row)) => render(row),
        Ok(Err(dialog)) => {
            let mut response = render(dialog);
            let headers = response.headers_mut();
            headers.insert("hx-retarget", "#editor-dialog".parse().unwrap());
            headers.insert("hx-reswap", "innerHTML".parse().unwrap());
            response
        }
        Err(response) => response,
    }
}

// ---- the whole review at once ----

pub async fn keep_job(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    Form(form): Form<DeckForm>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let shell = Shell::new(&state, theme, "decks", is_admin);
    let job = AiJobId(id);
    let kept = act(state.services.clone(), move |s| {
        let report = s.ai_keep_drafts(user, job, None, form.target(), now_ms())?;
        // A policy that stopped the loop leaves drafts behind: the
        // review comes back with its sentence rather than the deck.
        match report.stopped_by {
            Some(reason) => Ok(Err(review_page_of(&s, shell, user, job, Some(reason))?)),
            None => {
                // The report's deck is the one the cards actually went
                // into, a deck the switcher just named included.
                let deck = report.deck.map(|d| d.0).or_else(|| {
                    s.ai_job(user, job)
                        .ok()
                        .and_then(|j| j.deck_id.map(|d| d.0))
                });
                Ok(Ok((deck, report.kept)))
            }
        }
    })
    .await;
    match kept {
        Ok(Ok((deck, kept))) => {
            crate::web::account_event(&state, user, "ai_kept", None, Some(i64::from(kept)));
            match deck {
                Some(deck) => Redirect::to(&format!("/decks/{deck}")).into_response(),
                None => Redirect::to("/decks").into_response(),
            }
        }
        Ok(Err(page)) => render(page),
        Err(response) => response,
    }
}

pub async fn discard_job(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    match act(state.services.clone(), move |s| {
        s.ai_discard_job(user, AiJobId(id), now_ms())
    })
    .await
    {
        Ok(_) => Redirect::to(&format!("/ai/jobs/{id}/review")).into_response(),
        Err(response) => response,
    }
}

// ---- the thread ----

#[derive(Deserialize)]
pub struct MessageForm {
    #[serde(default, deserialize_with = "crate::bounds::opt_instruction")]
    message: Option<String>,
}

/// What a send answers with.
enum EditReply {
    /// Queued, for a browser without htmx: back to the review.
    Queued,
    /// The thread block, queued or not; `bool` says which.
    Partial(Box<ThreadReply>, bool),
    /// A refusal for a browser without htmx: the whole page, the reason
    /// in the thread.
    Page(Box<ReviewPage>),
}

/// `POST /ai/jobs/{id}/edit`: one message about the review's cards.
/// From htmx the answer is the thread block (polling, since a pass is
/// queued; or carrying the reason and the message when it was not
/// taken, so nothing typed is lost). Without htmx a queued message
/// sends the browser back to the review, where the thread polls.
pub async fn edit_job(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<MessageForm>,
) -> Response {
    let ai = match require_ai(&state) {
        Ok(ai) => ai,
        Err(response) => return response,
    };
    let htmx = from_htmx(&headers);
    let shell = Shell::new(&state, theme, "decks", is_admin);
    let job = AiJobId(id);
    let message = form.message.unwrap_or_default();
    let reply = act(state.services.clone(), move |s| {
        match s.ai_edit_job(user, job, &message, now_ms()) {
            Ok(_) if htmx => Ok(EditReply::Partial(
                Box::new(ThreadReply {
                    thread: thread_view_of(&s, user, job, None, String::new())?,
                    review: None,
                }),
                true,
            )),
            Ok(_) => Ok(EditReply::Queued),
            Err(ServiceError::Invalid(reason))
            | Err(ServiceError::AiDenied {
                message: reason, ..
            }) => {
                if htmx {
                    Ok(EditReply::Partial(
                        Box::new(ThreadReply {
                            thread: thread_view_of(&s, user, job, Some(reason), message)?,
                            review: None,
                        }),
                        false,
                    ))
                } else {
                    let mut page = review_page_of(&s, shell, user, job, None)?;
                    page.thread.error = Some(reason);
                    page.thread.draft = message;
                    Ok(EditReply::Page(Box::new(page)))
                }
            }
            Err(other) => Err(other),
        }
    })
    .await;
    let queued = matches!(
        reply,
        Ok(EditReply::Queued) | Ok(EditReply::Partial(_, true))
    );
    if queued {
        ai.wake.notify_one();
        crate::web::account_event(&state, user, "ai_job", Some("edit"), None);
    }
    match reply {
        Ok(EditReply::Queued) => Redirect::to(&format!("/ai/jobs/{id}/review")).into_response(),
        Ok(EditReply::Partial(partial, _)) => render(*partial),
        Ok(EditReply::Page(page)) => render(*page),
        Err(response) => response,
    }
}

/// `GET /ai/jobs/{id}/thread`: the thread block, polled while a pass
/// runs. The answer that first finds none running also carries the
/// review body out of band, so the changed drafts land with the reply.
pub async fn thread_partial(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    if let Err(response) = require_ai(&state) {
        return response;
    }
    let job = AiJobId(id);
    respond(state.services.clone(), move |s| {
        let thread = thread_view_of(&s, user, job, None, String::new())?;
        let review = if thread.active {
            None
        } else {
            Some(review_body_of(&s, user, job, None)?)
        };
        Ok(ThreadReply { thread, review })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_store::{AiSourceKind, AiSourceRow};

    fn row(kind: AiSourceKind, name: &str) -> AiSourceRow {
        AiSourceRow {
            id: AiSourceId(1),
            kind,
            sha256: String::new(),
            mime: String::new(),
            size: 0,
            filename: name.into(),
            url: (kind == AiSourceKind::Youtube).then(|| "https://youtu.be/x".into()),
            text: None,
            pages: None,
            duration_s: None,
            est_words: None,
            image_count: None,
            origin_mime: None,
            created_at: 0,
            last_used_at: 0,
        }
    }

    #[test]
    fn the_review_names_the_set_the_way_a_person_would() {
        let pdf = row(AiSourceKind::Pdf, "notes.pdf");
        let photo = row(AiSourceKind::Image, "a.png");
        let link = row(AiSourceKind::Youtube, "");
        assert_eq!(sources_phrase(std::slice::from_ref(&pdf), 0), "notes.pdf");
        assert_eq!(
            sources_phrase(&[pdf.clone(), photo.clone()], 0),
            "notes.pdf and a.png"
        );
        assert_eq!(
            sources_phrase(
                &[pdf.clone(), photo.clone(), photo.clone(), link.clone()],
                0
            ),
            "notes.pdf, https://youtu.be/x and 2 photos"
        );
        assert_eq!(
            sources_phrase(&[row(AiSourceKind::Text, "")], 1),
            "Pasted text and 1 expired source"
        );
        assert_eq!(sources_phrase(&[], 2), "2 expired sources");
        assert_eq!(parse_source_ids("12, 15,19").unwrap().len(), 3);
        assert!(parse_source_ids("12,x").is_err());
        assert!(parse_source_ids("").unwrap().is_empty());
    }
}
