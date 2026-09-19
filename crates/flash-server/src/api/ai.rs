//! Card generation over the API: a source goes up, its preflight comes
//! back, a job is queued and polled, and the drafts it wrote are kept,
//! edited or discarded. Every route answers `not_available` when no
//! model is configured, so the app hides the whole feature from one
//! probe of `/meta`.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use flash_core::{AiDraftId, AiJobId, AiSourceId, DeckId};
use flash_store::notes::NoteType;
use flash_store::{AiDraftCounts, AiDraftRow, AiSourceRow, AiTurnRow};
use jiff::{tz::TimeZone, Timestamp};
use serde::{Deserialize, Serialize};

use super::{call, ApiError, ApiJson, ApiResult, BearerUser};
use crate::policy::AiUsage;
use crate::service::{
    facts_line, now_ms, AiJobDetail, AiPreflight, AiSetPreflight, AiThread, DeckTarget,
    ServiceError, Services,
};
use crate::state::AppState;
use crate::web::MEDIA_UPLOAD_MAX_BYTES;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/ai/sources",
            // One file at the media cap plus multipart framing.
            post(create_source)
                .layer(DefaultBodyLimit::max(MEDIA_UPLOAD_MAX_BYTES))
                .get(list_sources),
        )
        .route("/ai/sources/{id}", delete(delete_source))
        .route("/ai/sources/{id}/preflight", get(preflight))
        .route("/ai/preflight", post(preflight_set))
        .route("/ai/jobs", post(create_job))
        .route("/ai/jobs/{id}", get(job))
        .route("/ai/jobs/{id}/drafts", get(drafts))
        .route("/ai/jobs/{id}/keep", post(keep))
        .route("/ai/jobs/{id}/discard", post(discard_job))
        .route("/ai/jobs/{id}/regenerate", post(regenerate))
        .route("/ai/jobs/{id}/more", post(more))
        .route("/ai/jobs/{id}/edit", post(edit))
        .route("/ai/jobs/{id}/turns", get(turns))
        .route("/ai/drafts/{id}", patch(patch_draft))
        .route("/ai/quota", get(quota))
}

// ---- wire shapes ----

/// The account's standing this month, as the app's meter shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Quota {
    /// How much of the month's allowance is used, 0 to 100; null when
    /// nothing is metered.
    pub allowance_used_pct: Option<u8>,
    /// The day the month starts over, as an ISO date in the account's
    /// own time zone; null when nothing is metered.
    pub resets_at: Option<String>,
    /// The inputs the plan allows, before this server's own list.
    pub inputs: Vec<&'static str>,
}

impl Quota {
    fn from_usage(usage: &AiUsage, timezone: &str) -> Self {
        Quota {
            allowance_used_pct: usage.allowance_used_pct,
            resets_at: usage.resets_at_ms.map(|ms| iso_date(ms, timezone)),
            inputs: usage.inputs.iter().map(|i| i.as_str()).collect(),
        }
    }
}

/// `ms` as a calendar date in `timezone` (UTC for a zone the server
/// does not know).
fn iso_date(ms: i64, timezone: &str) -> String {
    let tz = TimeZone::get(timezone).unwrap_or(TimeZone::UTC);
    Timestamp::from_millisecond(ms)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_zoned(tz)
        .date()
        .to_string()
}

pub fn quota_of(
    s: &Services,
    user: flash_core::UserId,
    usage: &AiUsage,
) -> Result<Quota, ServiceError> {
    let timezone = s.store().get_settings(user)?.timezone;
    Ok(Quota::from_usage(usage, &timezone))
}

/// What a generation from a source would be, before it is asked for.
#[derive(Debug, Serialize)]
pub struct Preflight {
    pub source_id: i64,
    pub kind: &'static str,
    pub pages: Option<u32>,
    pub est_words: Option<u32>,
    pub image_count: Option<u32>,
    pub duration_s: Option<u32>,
    pub allowed: bool,
    /// Why not: a cap's token, `input_disabled`, or the plan's own.
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    pub cost_class: &'static str,
    pub quota: Quota,
}

impl Preflight {
    fn new(source: AiSourceId, p: AiPreflight, quota: Quota) -> Self {
        Preflight {
            source_id: source.0,
            kind: p.kind.as_str(),
            pages: p.pages,
            est_words: p.est_words,
            image_count: p.image_count,
            duration_s: p.duration_s,
            allowed: p.allowed,
            code: p.code,
            reason: p.reason,
            cost_class: p.cost_class,
            quota,
        }
    }
}

/// A source as the client sees it after upload.
#[derive(Debug, Serialize)]
pub struct Source {
    pub id: i64,
    pub kind: &'static str,
    pub filename: String,
    /// The canonical link of a video-link source.
    pub url: Option<String>,
    pub size: u64,
    /// The type the file was uploaded as when the stored bytes are a
    /// conversion of it.
    pub origin_mime: Option<String>,
}

impl Source {
    pub fn of(row: AiSourceRow) -> Self {
        Source {
            id: row.id.0,
            kind: row.kind.as_str(),
            filename: row.filename,
            url: row.url,
            size: row.size,
            origin_mime: row.origin_mime,
        }
    }
}

/// A source with its preflight and the facts line a chip shows.
#[derive(Debug, Serialize)]
pub struct SourceCreated {
    #[serde(flatten)]
    pub source: Source,
    /// "12 pages · ~4,800 words · 3 figures".
    pub line: String,
    pub preflight: Preflight,
}

impl SourceCreated {
    fn new(row: AiSourceRow, preflight: AiPreflight, quota: Quota) -> Self {
        let id = row.id;
        SourceCreated {
            line: facts_line(&preflight),
            source: Source::of(row),
            preflight: Preflight::new(id, preflight, quota),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Sources {
    pub items: Vec<SourceCreated>,
}

/// What a set adds up to.
#[derive(Debug, Serialize)]
pub struct SetTotals {
    pub sources: u32,
    pub pages: u32,
    pub est_words: u32,
    pub image_count: u32,
    pub duration_s: u32,
    pub bytes: u64,
}

/// What one generation over a set would be.
#[derive(Debug, Serialize)]
pub struct SetPreflight {
    pub allowed: bool,
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    /// The member a refusal is about, when it is about one member.
    pub refused_source_id: Option<i64>,
    pub cost_class: &'static str,
    /// "3 sources · 48 pages · 6 photos · 4:31 of media".
    pub line: String,
    pub totals: SetTotals,
    pub sources: Vec<Preflight>,
    pub quota: Quota,
}

impl SetPreflight {
    pub fn new(p: AiSetPreflight, quota: Quota) -> Self {
        SetPreflight {
            allowed: p.allowed,
            code: p.code,
            reason: p.reason,
            refused_source_id: p.refused_source_id.map(|s| s.0),
            cost_class: p.cost_class,
            line: p.line,
            totals: SetTotals {
                sources: p.totals.sources,
                pages: p.totals.pages,
                est_words: p.totals.est_words,
                image_count: p.totals.image_count,
                duration_s: p.totals.duration_s,
                bytes: p.totals.bytes,
            },
            sources: p
                .sources
                .into_iter()
                .map(|(id, member)| Preflight::new(id, member, quota.clone()))
                .collect(),
            quota,
        }
    }
}

/// A job as the client polls it.
#[derive(Debug, Serialize)]
pub struct Job {
    pub id: i64,
    pub state: &'static str,
    pub progress: u8,
    pub error: Option<String>,
    pub op: &'static str,
    pub deck_id: Option<i64>,
    /// The job whose drafts this pass wrote to; itself for a first pass.
    pub root_job_id: i64,
    pub parent_job_id: Option<i64>,
    pub instruction: String,
    /// The set the job reads, in order, less what housekeeping has
    /// removed.
    pub sources: Vec<Source>,
    /// How many members housekeeping has removed.
    pub sources_expired: u32,
    /// The standing of the drafts on the review.
    pub counts: AiDraftCounts,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

impl From<AiJobDetail> for Job {
    fn from(d: AiJobDetail) -> Self {
        Job {
            id: d.job.id.0,
            state: d.job.state.as_str(),
            progress: d.job.progress,
            error: d.job.error,
            op: d.job.op.as_str(),
            deck_id: d.job.deck_id.map(|x| x.0),
            root_job_id: d.root.0,
            parent_job_id: d.job.parent_job_id.map(|x| x.0),
            instruction: d.job.instruction,
            sources: d.sources.into_iter().map(Source::of).collect(),
            sources_expired: d.sources_expired,
            counts: d.counts,
            created_at: d.job.created_at,
            finished_at: d.job.finished_at,
        }
    }
}

/// A draft as the review shows it.
#[derive(Debug, Serialize)]
pub struct Draft {
    pub id: i64,
    pub job_id: i64,
    pub ord: u32,
    pub note_type: &'static str,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    pub figure_media_ids: Vec<i64>,
    pub status: &'static str,
    pub note_id: Option<i64>,
}

impl From<AiDraftRow> for Draft {
    fn from(d: AiDraftRow) -> Self {
        Draft {
            id: d.id.0,
            job_id: d.job_id.0,
            ord: d.ord,
            note_type: d.note_type.as_str(),
            front_html: d.front_html,
            back_html: d.back_html,
            tags: d.tags,
            figure_media_ids: d.figure_media_ids.into_iter().map(|m| m.0).collect(),
            status: d.status.as_str(),
            note_id: d.note_id.map(|n| n.0),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Queued {
    pub job_id: i64,
    /// The deck the cards will land in: the id a `deck_name` resolved
    /// to, so a client that named a deck learns which one it got. A
    /// further pass over a review keeps the deck it already has and
    /// leaves this null.
    pub deck_id: Option<i64>,
}

/// A message accepted into the review's thread: the pass to poll and
/// the turn it became.
#[derive(Debug, Serialize)]
pub struct EditQueued {
    pub job_id: i64,
    pub turn_id: i64,
}

/// One turn of a review's thread.
#[derive(Debug, Serialize)]
pub struct Turn {
    pub id: i64,
    /// `user` or `model`.
    pub role: &'static str,
    pub text: String,
    /// The pass that ran the turn or wrote the reply, when one did.
    pub child_job_id: Option<i64>,
    /// That pass's state; `failed` marks the sentence as a failure.
    pub child_state: Option<&'static str>,
    pub created_at: i64,
}

impl From<AiTurnRow> for Turn {
    fn from(t: AiTurnRow) -> Self {
        Turn {
            id: t.id,
            role: t.role.as_str(),
            text: t.text,
            child_job_id: t.child_job_id.map(|j| j.0),
            child_state: t.child_state.map(|s| s.as_str()),
            created_at: t.created_at,
        }
    }
}

/// The thread of a review, oldest first, and the pass still running.
#[derive(Debug, Serialize)]
pub struct Thread {
    pub items: Vec<Turn>,
    pub active_job_id: Option<i64>,
}

impl From<AiThread> for Thread {
    fn from(t: AiThread) -> Self {
        Thread {
            items: t.turns.into_iter().map(Into::into).collect(),
            active_job_id: t.active_job_id.map(|j| j.0),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct KeepReport {
    pub kept: u32,
    pub cards: u32,
    /// The deck the cards went into, a deck this keep named included.
    pub deck_id: Option<i64>,
    pub remaining_pending: u32,
    /// The card policy's sentence when it stopped the keep early.
    pub stopped_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Discarded {
    pub discarded: u32,
}

// ---- sources ----

/// `{"text": …}` or `{"youtube_url": …}`, one of the two.
#[derive(Deserialize)]
struct SourceBody {
    #[serde(default, deserialize_with = "crate::bounds::text_source")]
    text: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::youtube_url")]
    youtube_url: Option<String>,
}

/// One source on the way in: what the body carried, whichever of the
/// three shapes it had.
pub enum Upload {
    Text(String),
    Youtube(String),
    File(String, Bytes),
}

/// The 429 the upload gate answers with, telling the client when to
/// come back so a batch of chips can queue rather than fail.
pub fn busy() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "busy",
        "too many uploads at once; try again in a moment",
    )
}

/// The "try again in a second" header on a busy answer.
pub fn with_retry_after(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    response
}

/// `POST /ai/sources`: multipart with exactly one `file` field, or JSON
/// with `text` or `youtube_url`. One source per call, so one reply is
/// one chip: the source with its preflight and its facts line.
async fn create_source(
    State(state): State<AppState>,
    user: BearerUser,
    req: Request,
) -> Result<(StatusCode, Json<SourceCreated>), Response> {
    let ai = state
        .ai()
        .map_err(|e| ApiError::from(e).into_response())?
        .clone();
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Held until the bytes are stored: the body limit is per request,
    // and a handful of concurrent uploads must fit the process.
    let upload_permit = state.upload_semaphore.clone().try_acquire_owned();
    let upload = if content_type.starts_with("multipart/form-data") {
        if upload_permit.is_err() {
            return Err(with_retry_after(busy().into_response()));
        }
        let multipart = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiError::invalid(e.body_text()).into_response())?;
        let (name, bytes) = read_file(multipart)
            .await
            .map_err(IntoResponse::into_response)?;
        Upload::File(name, bytes)
    } else {
        let ApiJson(body) = ApiJson::<SourceBody>::from_request(req, &state)
            .await
            .map_err(IntoResponse::into_response)?;
        match (body.text, body.youtube_url) {
            (Some(text), None) => Upload::Text(text),
            (None, Some(url)) => Upload::Youtube(url),
            _ => {
                return Err(
                    ApiError::invalid("send either text or a youtube_url, not both")
                        .into_response(),
                )
            }
        }
    };
    let media = state.media.clone();
    let created = call(state.services.clone(), move |s| {
        let created = ingest(&s, user.id, upload, media.as_ref(), &ai)?;
        drop(upload_permit);
        Ok::<_, ServiceError>(created)
    })
    .await
    .map_err(IntoResponse::into_response)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// One upload into a source row, answered with its preflight. Shared
/// with the web router, whose page posts the same three shapes.
pub fn ingest(
    s: &Services,
    user: flash_core::UserId,
    upload: Upload,
    media: &dyn crate::media_store::MediaStore,
    ai: &crate::state::AiRuntime,
) -> Result<SourceCreated, ServiceError> {
    let now = now_ms();
    let id = match upload {
        Upload::Text(text) => s.ai_source_from_text(user, &text, "", "text/plain", now)?,
        Upload::Youtube(url) => s.ai_source_from_youtube(user, &url, ai, now)?,
        Upload::File(name, bytes) => {
            s.ai_source_from_upload(user, &name, &bytes, media, ai, now)?
        }
    };
    let row = s
        .store()
        .get_ai_source(user, id)?
        .ok_or(flash_store::StoreError::NotFound("ai source"))?;
    let preflight = s.ai_preflight(user, id, ai, now)?;
    let quota = quota_of(s, user, &preflight.usage)?;
    Ok(SourceCreated::new(row, preflight, quota))
}

/// Everything a multipart `POST /ai/sources` may carry: the one `file`
/// field, and the pasted text or the video link a form may send in the
/// same body instead.
pub struct MultipartSource {
    pub file: Option<(String, Bytes)>,
    pub text: Option<String>,
    pub youtube_url: Option<String>,
}

/// Reads that body. Every field is bounded here, at the edge, the way a
/// request struct's would be: one file, its name at the filename cap,
/// the text and the link at their own, so nothing past any of them is
/// held in memory.
pub(crate) async fn read_multipart(mut multipart: Multipart) -> Result<MultipartSource, ApiError> {
    let mut source = MultipartSource {
        file: None,
        text: None,
        youtube_url: None,
    };
    while let Ok(Some(field)) = multipart.next_field().await {
        let bounded = |value: String, max: usize| -> Result<Option<String>, ApiError> {
            if value.len() > max {
                return Err(ApiError::invalid("that field is too long"));
            }
            Ok((!value.trim().is_empty()).then_some(value))
        };
        match field.name() {
            Some("text") => {
                let value = field.text().await.unwrap_or_default();
                source.text = bounded(value, crate::bounds::AI_TEXT_SOURCE)?;
                continue;
            }
            Some("youtube_url") => {
                let value = field.text().await.unwrap_or_default();
                source.youtube_url = bounded(value, crate::bounds::YOUTUBE_URL)?;
                continue;
            }
            Some("file") => {}
            _ => continue,
        }
        if source.file.is_some() {
            return Err(ApiError::invalid(
                "one file per upload; send each file in its own request",
            ));
        }
        let name = field.file_name().unwrap_or("upload").to_string();
        if name.len() > crate::bounds::FILENAME {
            return Err(ApiError::invalid("the file's name is too long"));
        }
        match field.bytes().await {
            // An empty file field is what a browser sends for a picker
            // nobody chose anything with.
            Ok(bytes) if bytes.is_empty() => {}
            Ok(bytes) => source.file = Some((name, bytes)),
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                return Err(ApiError::payload_too_large());
            }
            Err(e) => return Err(ApiError::invalid(format!("upload: {e}"))),
        }
    }
    Ok(source)
}

/// The one `file` field of the form, for the API's own upload: there a
/// body that is multipart is a file upload and nothing else.
async fn read_file(multipart: Multipart) -> Result<(String, Bytes), ApiError> {
    read_multipart(multipart)
        .await?
        .file
        .ok_or_else(|| ApiError::invalid("no file in upload"))
}

/// `GET /ai/sources`: the sources no job has read yet, newest first,
/// each with its preflight.
async fn list_sources(State(state): State<AppState>, user: BearerUser) -> ApiResult<Sources> {
    let ai = state.ai()?.clone();
    let items = call(state.services.clone(), move |s| {
        let now = now_ms();
        let mut items = Vec::new();
        for (row, preflight) in s.ai_unattached_sources(user.id, &ai, now)? {
            let quota = quota_of(&s, user.id, &preflight.usage)?;
            items.push(SourceCreated::new(row, preflight, quota));
        }
        Ok::<_, ServiceError>(items)
    })
    .await?;
    Ok(Json(Sources { items }))
}

/// `DELETE /ai/sources/{id}`: a source taken back before any job read
/// it. A member of a job answers 409.
async fn delete_source(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.ai()?;
    let media = state.media.clone();
    call(state.services.clone(), move |s| {
        s.ai_delete_source(user.id, AiSourceId(id), media.as_ref())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PreflightBody {
    #[serde(deserialize_with = "crate::bounds::source_ids")]
    source_ids: Vec<i64>,
}

/// `POST /ai/preflight`: what one generation over the set would be.
async fn preflight_set(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<PreflightBody>,
) -> ApiResult<SetPreflight> {
    let ai = state.ai()?.clone();
    let ids: Vec<AiSourceId> = body.source_ids.into_iter().map(AiSourceId).collect();
    let preflight = call(state.services.clone(), move |s| {
        let p = s.ai_preflight_set(user.id, &ids, &ai, now_ms())?;
        let quota = quota_of(&s, user.id, &p.usage)?;
        Ok::<_, ServiceError>(SetPreflight::new(p, quota))
    })
    .await?;
    Ok(Json(preflight))
}

async fn preflight(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<Preflight> {
    let ai = state.ai()?.clone();
    let source = AiSourceId(id);
    let preflight = call(state.services.clone(), move |s| {
        let p = s.ai_preflight(user.id, source, &ai, now_ms())?;
        let quota = quota_of(&s, user.id, &p.usage)?;
        Ok::<_, ServiceError>(Preflight::new(source, p, quota))
    })
    .await?;
    Ok(Json(preflight))
}

// ---- jobs ----

#[derive(Deserialize)]
struct JobBody {
    #[serde(deserialize_with = "crate::bounds::source_ids")]
    source_ids: Vec<i64>,
    /// A deck the account already has. Exactly one of this and
    /// `deck_name` is given.
    #[serde(default)]
    deck_id: Option<i64>,
    /// A deck by name: an existing one of that name, or a new deck.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    deck_name: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_instruction")]
    instruction: Option<String>,
}

/// The deck a body names, as one target. Two ways of saying it, or
/// neither, is a refusal rather than a guess.
fn deck_target(deck_id: Option<i64>, deck_name: Option<String>) -> Result<DeckTarget, ApiError> {
    match (deck_id, deck_name) {
        (Some(id), None) => Ok(DeckTarget::Existing(DeckId(id))),
        (None, Some(name)) => Ok(DeckTarget::New(name)),
        (Some(_), Some(_)) => Err(ApiError::invalid("send deck_id or deck_name, not both")),
        (None, None) => Err(ApiError::invalid("send deck_id or deck_name")),
    }
}

async fn create_job(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<JobBody>,
) -> Result<(StatusCode, Json<Queued>), ApiError> {
    let ai = state.ai()?.clone();
    let runtime = ai.clone();
    let ids: Vec<AiSourceId> = body.source_ids.into_iter().map(AiSourceId).collect();
    let deck = deck_target(body.deck_id, body.deck_name)?;
    let queued = call(state.services.clone(), move |s| {
        s.ai_create_job(
            user.id,
            &ids,
            deck,
            body.instruction.as_deref().unwrap_or(""),
            &runtime,
            now_ms(),
        )
    })
    .await?;
    ai.wake.notify_one();
    Ok((
        StatusCode::CREATED,
        Json(Queued {
            job_id: queued.job.0,
            deck_id: Some(queued.deck.0),
        }),
    ))
}

async fn job(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<Job> {
    state.ai()?;
    let detail = call(state.services.clone(), move |s| {
        s.ai_job_detail(user.id, AiJobId(id))
    })
    .await?;
    Ok(Json(detail.into()))
}

#[derive(Debug, Serialize)]
pub struct Drafts {
    pub items: Vec<Draft>,
}

async fn drafts(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<Drafts> {
    state.ai()?;
    let rows = call(state.services.clone(), move |s| {
        s.ai_drafts(user.id, AiJobId(id))
    })
    .await?;
    Ok(Json(Drafts {
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Deserialize)]
struct KeepBody {
    /// Absent: every open draft.
    #[serde(default, deserialize_with = "crate::bounds::opt_draft_ids")]
    draft_ids: Option<Vec<i64>>,
    /// Absent, with `deck_name` absent too: the job's own deck.
    #[serde(default)]
    deck_id: Option<i64>,
    /// The deck by name: an existing one of that name, or a new deck.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    deck_name: Option<String>,
}

async fn keep(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<KeepBody>,
) -> ApiResult<KeepReport> {
    state.ai()?;
    // Neither field is the job's own deck; either one names a target.
    let deck = match (body.deck_id, body.deck_name) {
        (None, None) => None,
        (deck_id, deck_name) => Some(deck_target(deck_id, deck_name)?),
    };
    let report = call(state.services.clone(), move |s| {
        let ids: Option<Vec<AiDraftId>> = body
            .draft_ids
            .map(|ids| ids.into_iter().map(AiDraftId).collect());
        s.ai_keep_drafts(user.id, AiJobId(id), ids.as_deref(), deck, now_ms())
    })
    .await?;
    Ok(Json(KeepReport {
        kept: report.kept,
        cards: report.cards,
        deck_id: report.deck.map(|d| d.0),
        remaining_pending: report.remaining_pending,
        stopped_by: report.stopped_by,
    }))
}

async fn discard_job(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<Discarded> {
    state.ai()?;
    let discarded = call(state.services.clone(), move |s| {
        s.ai_discard_job(user.id, AiJobId(id), now_ms())
    })
    .await?;
    Ok(Json(Discarded { discarded }))
}

#[derive(Deserialize)]
struct InstructionBody {
    #[serde(default, deserialize_with = "crate::bounds::opt_instruction")]
    instruction: Option<String>,
}

async fn regenerate(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<InstructionBody>,
) -> Result<(StatusCode, Json<Queued>), ApiError> {
    let ai = state.ai()?.clone();
    let job = call(state.services.clone(), move |s| {
        s.ai_revise_job(
            user.id,
            AiJobId(id),
            body.instruction.as_deref().unwrap_or(""),
            now_ms(),
        )
    })
    .await?;
    ai.wake.notify_one();
    Ok((
        StatusCode::CREATED,
        Json(Queued {
            job_id: job.0,
            deck_id: None,
        }),
    ))
}

async fn more(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<InstructionBody>,
) -> Result<(StatusCode, Json<Queued>), ApiError> {
    let ai = state.ai()?.clone();
    let job = call(state.services.clone(), move |s| {
        s.ai_more_job(
            user.id,
            AiJobId(id),
            body.instruction.as_deref().unwrap_or(""),
            now_ms(),
        )
    })
    .await?;
    ai.wake.notify_one();
    Ok((
        StatusCode::CREATED,
        Json(Queued {
            job_id: job.0,
            deck_id: None,
        }),
    ))
}

#[derive(Deserialize)]
struct EditBody {
    #[serde(deserialize_with = "crate::bounds::instruction")]
    message: String,
}

/// `POST /ai/jobs/{id}/edit`: one message about the review's cards.
/// The pass it queues may rewrite, add, drop, retype or retag the open
/// drafts; the kept and discarded ones are never touched.
async fn edit(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<EditBody>,
) -> Result<(StatusCode, Json<EditQueued>), ApiError> {
    let ai = state.ai()?.clone();
    let queued = call(state.services.clone(), move |s| {
        s.ai_edit_job(user.id, AiJobId(id), &body.message, now_ms())
    })
    .await?;
    ai.wake.notify_one();
    Ok((
        StatusCode::CREATED,
        Json(EditQueued {
            job_id: queued.job_id.0,
            turn_id: queued.turn_id,
        }),
    ))
}

/// `GET /ai/jobs/{id}/turns`: the thread of the review `id` belongs
/// to (a root or any pass of it).
async fn turns(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<Thread> {
    state.ai()?;
    let thread = call(state.services.clone(), move |s| {
        s.ai_thread(user.id, AiJobId(id))
    })
    .await?;
    Ok(Json(thread.into()))
}

// ---- drafts ----

/// Any of the editor's fields, and/or `status: "discarded"`.
#[derive(Deserialize)]
struct DraftPatch {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    note_type: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_field_html")]
    front_html: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_field_html")]
    back_html: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_tags")]
    tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    status: Option<String>,
}

async fn patch_draft(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<DraftPatch>,
) -> ApiResult<Draft> {
    state.ai()?;
    let draft = AiDraftId(id);
    let note_type = match body.note_type.as_deref() {
        Some(raw) => {
            Some(NoteType::parse(raw).ok_or_else(|| ApiError::invalid("unknown note type"))?)
        }
        None => None,
    };
    let discard = match body.status.as_deref() {
        None => false,
        Some("discarded") => true,
        Some(_) => return Err(ApiError::invalid("status may only be \"discarded\"")),
    };
    let edits = note_type.is_some()
        || body.front_html.is_some()
        || body.back_html.is_some()
        || body.tags.is_some();
    let row = call(state.services.clone(), move |s| {
        let now = now_ms();
        if edits {
            let current = s.ai_draft(user.id, draft)?;
            s.ai_update_draft(
                user.id,
                draft,
                note_type.unwrap_or(current.note_type),
                body.front_html.as_deref().unwrap_or(&current.front_html),
                body.back_html.as_deref().unwrap_or(&current.back_html),
                body.tags.as_deref().unwrap_or(&current.tags),
                now,
            )?;
        }
        if discard {
            s.ai_discard_draft(user.id, draft, now)?;
        }
        s.ai_draft(user.id, draft)
    })
    .await?;
    Ok(Json(row.into()))
}

// ---- quota ----

async fn quota(State(state): State<AppState>, user: BearerUser) -> ApiResult<Quota> {
    state.ai()?;
    let quota = call(state.services.clone(), move |s| {
        let usage = s.ai_quota(user.id, now_ms())?;
        quota_of(&s, user.id, &usage)
    })
    .await?;
    Ok(Json(quota))
}
