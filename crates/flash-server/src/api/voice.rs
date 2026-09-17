//! Voice study's model call: one spoken turn. The client owns the
//! dialog and says where it is (the front is read and an answer is
//! awaited, or the verdict was given); the server hands the card, that
//! state and what the student said to the model, which decides what was
//! meant and grades or answers when that is what it was. The reply is
//! tagged by intent so the client can act on it in the same switch its
//! own keywords go through.
//!
//! A turn needs a model (404 with the sentence when none is configured),
//! a per-user budget (a scripted loop of spoken turns is throttled before
//! it reaches the model), and one of the process's model-call permits,
//! held for the whole blocking call. It never writes a review: the
//! client posts that through the reviews route once its undo window has
//! passed, so `review_log` keeps its study-path writer (the only other
//! is the import replay, whose rows carry source `import`).
//!
//! The gate and the blocking call are shared with the browser's twin in
//! `crate::web_voice`, which takes the same fields as a form.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use flash_core::{CardId, SessionId, UserId};
use flash_store::VoiceSide;
use serde::{Deserialize, Serialize};

use super::dto::Reveal;
use super::{ApiError, ApiJson, ApiResult, BearerUser};
use crate::ai::provider::DialogState;
use crate::service::{
    now_ms, spoken_payload, ServiceError, SpokenOutcome, SpokenPayload, VoiceError,
};
use crate::state::{AiOff, AiRuntime, AppState};
use crate::tts::serve::{self, Resolved};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/study/sessions/{sid}/turn",
            // A recording rides in the body as base64, past the default
            // limit once form encoding is counted.
            post(spoken_turn).layer(DefaultBodyLimit::max(crate::bounds::VOICE_BODY_BYTES)),
        )
        .route("/study/cards/{card}/audio", get(card_audio))
        .route("/voice/phrases", get(phrases))
        .route("/voice/phrases/{voice}/{key}/audio", get(phrase_audio))
        .route("/voice/answers/{token}/audio", get(answer_audio))
        .route("/voice/voices", get(voices))
}

/// Which side of the card to hear.
#[derive(Deserialize)]
pub struct SideQuery {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub side: String,
}

/// Which voice's phrase set; the account's own when absent.
#[derive(Deserialize)]
pub struct VoiceQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub voice: Option<String>,
}

/// How an audio route answers what was not the bytes: not made yet,
/// nothing by that name, no sidecar.
pub(crate) struct AudioMisses {
    pub not_ready: fn() -> Response,
    pub not_found: fn(&'static str) -> Response,
    pub off: fn() -> Response,
}

/// The audio routes' shared ending: the bytes, a 304, or why not. The
/// API's own replies are marked no-store by its layer; the page's
/// carry `cache_control`. A side that will never have a clip is a 204
/// on both: nothing to play, nothing to wait for.
pub(crate) fn audio_response(
    resolved: Resolved,
    cache_control: &str,
    what: &'static str,
    misses: AudioMisses,
) -> Response {
    match resolved {
        Resolved::Ready { row, blob, etag } => {
            serve::clip_response(&row, blob, &etag, cache_control)
        }
        Resolved::NotModified { etag } => serve::not_modified(&etag, cache_control),
        Resolved::Fresh { mime, bytes } => serve::fresh_response(&mime, bytes),
        Resolved::NotReady => (misses.not_ready)(),
        Resolved::Unvoiced => StatusCode::NO_CONTENT.into_response(),
        Resolved::NotFound => (misses.not_found)(what),
        Resolved::Off => (misses.off)(),
    }
}

const API_MISSES: AudioMisses = AudioMisses {
    not_ready: || {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_ready",
            "that clip is not made yet",
        )
        .into_response()
    },
    not_found: |what| ApiError::not_found(what).into_response(),
    off: || ApiError::not_available("voice").into_response(),
};

async fn card_audio(
    State(state): State<AppState>,
    user: BearerUser,
    Path(card): Path<i64>,
    Query(query): Query<SideQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(side) = VoiceSide::from_str(&query.side) else {
        return ApiError::invalid("side must be front or back").into_response();
    };
    let app = state.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        serve::resolve_card_clip(&app, user.id, CardId(card), side, &headers)
    })
    .await
    .unwrap_or(Resolved::NotReady);
    audio_response(resolved, serve::CARD_CACHE, "card", API_MISSES)
}

async fn phrase_audio(
    State(state): State<AppState>,
    _user: BearerUser,
    Path((voice, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if voice.len() > crate::bounds::KEYWORD || key.len() > crate::bounds::KEYWORD {
        return ApiError::not_found("phrase").into_response();
    }
    let app = state.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        serve::resolve_phrase_clip(&app, &voice, &key, &headers)
    })
    .await
    .unwrap_or(Resolved::NotReady);
    audio_response(resolved, serve::PHRASE_CACHE, "phrase", API_MISSES)
}

async fn answer_audio(
    State(state): State<AppState>,
    user: BearerUser,
    Path(token): Path<String>,
) -> Response {
    if token.len() > crate::bounds::KEYWORD {
        return ApiError::not_found("answer").into_response();
    }
    let app = state.clone();
    let resolved =
        tokio::task::spawn_blocking(move || serve::resolve_answer_clip(&app, user.id, &token))
            .await
            .unwrap_or(Resolved::NotReady);
    audio_response(resolved, "no-store", "answer", API_MISSES)
}

/// The account's voice, or the one asked for.
pub(crate) fn voice_for(
    state: &AppState,
    user: UserId,
    asked: Option<String>,
) -> Result<String, ServiceError> {
    match asked {
        Some(voice) => Ok(voice),
        None => Ok(state.services.voice_settings(user)?.voice_id),
    }
}

async fn phrases(
    State(state): State<AppState>,
    user: BearerUser,
    Query(query): Query<VoiceQuery>,
) -> Result<Json<serve::PhraseSet>, ApiError> {
    state.tts().map_err(|_| ApiError::not_available("voice"))?;
    let app = state.clone();
    let set = tokio::task::spawn_blocking(move || {
        let voice = voice_for(&app, user.id, query.voice)?;
        Ok::<_, ServiceError>(serve::phrase_set(&app, &voice, |key| {
            format!("/api/v1/voice/phrases/{voice}/{key}/audio")
        }))
    })
    .await
    .map_err(|_| ApiError::internal())??;
    set.map(Json).ok_or_else(|| ApiError::not_found("voice"))
}

async fn voices(
    State(state): State<AppState>,
    _user: BearerUser,
) -> Result<Json<serve::VoiceListing>, ApiError> {
    state.tts().map_err(|_| ApiError::not_available("voice"))?;
    let app = state.clone();
    let listing = tokio::task::spawn_blocking(move || {
        serve::voice_listing(&app, |voice| {
            format!("/api/v1/voice/phrases/{voice}/sample/audio")
        })
    })
    .await
    .map_err(|_| ApiError::internal())?;
    listing
        .map(Json)
        .ok_or_else(|| ApiError::not_available("voice"))
}

/// One spoken turn: the card, where the conversation is (`front` while
/// the answer is awaited, `window` once the verdict was given), and what
/// the student said as the transcript or as the recording. One or the
/// other.
#[derive(Deserialize)]
pub struct TurnBody {
    pub card_id: i64,
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub state: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_transcript")]
    pub transcript: Option<String>,
    #[serde(default)]
    pub audio: Option<AudioBody>,
}

/// A recording of what was said, as the client captured it.
#[derive(Deserialize)]
pub struct AudioBody {
    #[serde(deserialize_with = "crate::bounds::audio_mime")]
    pub mime: String,
    #[serde(deserialize_with = "crate::bounds::audio_b64")]
    pub data_b64: String,
}

/// What the turn came to, tagged by `intent`. An `answer` carries the
/// verdict and the reveal to show with it (1 Again, 2 Hard, 3 Good, 4
/// Easy: what the client posts as the review unless the student undoes
/// it); a `question` its answer, and where its clip in the account's
/// voice will be once made (null when the server makes none); a `grade`
/// the rating the student named; `repeat`, `skip`, `undo` and `unclear`
/// carry nothing else.
#[derive(Serialize)]
#[serde(tag = "intent", rename_all = "snake_case")]
pub enum TurnReply {
    Answer {
        rating: i64,
        feedback: String,
        spoken_feedback: String,
        /// Boxed: the reveal is most of the enum, and the bare intents
        /// should not pay for it.
        reveal: Box<Reveal>,
    },
    Question {
        answer: String,
        spoken_answer: String,
        spoken_answer_url: Option<String>,
    },
    Grade {
        rating: i64,
    },
    Repeat,
    Skip,
    Undo,
    Unclear,
}

impl TurnReply {
    /// `answer_url` turns an answer clip's token into the route the
    /// surface serves it at.
    pub(crate) fn new(o: SpokenOutcome, answer_url: impl Fn(&str) -> String) -> Self {
        match o {
            SpokenOutcome::Graded(g) => TurnReply::Answer {
                rating: g.rating as i64,
                feedback: g.feedback,
                spoken_feedback: g.spoken_feedback,
                reveal: Box::new(g.reveal.into()),
            },
            SpokenOutcome::Answered(c) => TurnReply::Question {
                answer: c.answer,
                spoken_answer: c.spoken_answer,
                spoken_answer_url: c.clip_token.as_deref().map(answer_url),
            },
            SpokenOutcome::Rated(rating) => TurnReply::Grade {
                rating: rating as i64,
            },
            SpokenOutcome::Repeat => TurnReply::Repeat,
            SpokenOutcome::Skip => TurnReply::Skip,
            SpokenOutcome::Undo => TurnReply::Undo,
            SpokenOutcome::Unclear => TurnReply::Unclear,
        }
    }
}

/// The model's failure is the provider's sentence at 502: the request
/// was fine, the service behind it was not. Everything else is the
/// service error's usual mapping.
impl From<VoiceError> for ApiError {
    fn from(err: VoiceError) -> Self {
        match err {
            VoiceError::Service(e) => e.into(),
            VoiceError::Model(e) => {
                tracing::warn!("voice model call failed: {e}");
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "model_unavailable",
                    e.user_message(),
                )
            }
        }
    }
}

/// Why a spoken turn was refused before the model was asked.
pub(crate) enum Refused {
    /// No model on this server.
    Off,
    /// The user's spoken-turn budget for the minute is spent.
    Throttled,
}

impl From<Refused> for ApiError {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Off => ApiError::new(StatusCode::NOT_FOUND, "not_available", AiOff::MESSAGE),
            Refused::Throttled => ApiError::rate_limited(),
        }
    }
}

/// The runtime a spoken turn may use, once the server has one and the
/// user is within their budget. The permit is taken by the caller: it
/// is a wait, and belongs around the blocking call only.
fn voice_runtime(state: &AppState, user: UserId) -> Result<Arc<AiRuntime>, Refused> {
    let ai = state.ai().map_err(|AiOff| Refused::Off)?.clone();
    if !state.rate_limiter.allow(&format!("voice:{}", user.raw())) {
        return Err(Refused::Throttled);
    }
    Ok(ai)
}

/// The dialog state a request named, or the sentence for a bad one.
pub(crate) fn dialog_state(state: &str) -> Result<DialogState, ServiceError> {
    DialogState::parse(state)
        .ok_or_else(|| ServiceError::Invalid("state must be front or window".into()))
}

/// One turn: the budget checks, then the service call on a blocking
/// thread under a model-call permit. A graded answer's reveal gets its
/// clip attached the way the reveal route's does; a question's answer
/// is queued for a clip of its own, first in line.
pub(crate) async fn turn(
    state: &AppState,
    user: UserId,
    session: SessionId,
    card: CardId,
    dialog: DialogState,
    spoken: SpokenPayload,
) -> Result<Result<SpokenOutcome, VoiceError>, Refused> {
    let ai = voice_runtime(state, user)?;
    // Held across the await below, so the permit covers the whole
    // blocking call and is released when the reply is back.
    let _permit = ai.calls.acquire().await;
    let services = state.services.clone();
    let runtime = Arc::clone(&ai);
    let tts = state.tts.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let mut outcome =
            services.spoken_turn(user, session, card, dialog, &spoken, &runtime, now_ms())?;
        match &mut outcome {
            SpokenOutcome::Graded(graded) => {
                crate::tts::serve::attach_reveal(
                    tts.as_deref(),
                    &services,
                    user,
                    &mut graded.reveal,
                );
            }
            SpokenOutcome::Answered(clarify) => {
                if let Some(tts) = tts.as_deref() {
                    let voice = services.voice_settings(user)?.voice_id;
                    clarify.clip_token = tts.enqueue_answer(user, &voice, &clarify.spoken_answer);
                }
            }
            _ => {}
        }
        Ok(outcome)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!("join: {e}");
        Err(ServiceError::Internal("join".into()).into())
    });
    Ok(outcome)
}

async fn spoken_turn(
    State(state): State<AppState>,
    user: BearerUser,
    Path(sid): Path<i64>,
    ApiJson(body): ApiJson<TurnBody>,
) -> ApiResult<TurnReply> {
    let dialog = dialog_state(&body.state).map_err(ApiError::from)?;
    let (mime, data) = match body.audio {
        Some(audio) => (Some(audio.mime), Some(audio.data_b64)),
        None => (None, None),
    };
    let spoken = spoken_payload(body.transcript, mime, data).map_err(ApiError::from)?;
    let outcome = turn(
        &state,
        user.id,
        SessionId(sid),
        CardId(body.card_id),
        dialog,
        spoken,
    )
    .await
    .map_err(ApiError::from)?;
    Ok(Json(TurnReply::new(outcome?, |token| {
        format!("/api/v1/voice/answers/{token}/audio")
    })))
}
