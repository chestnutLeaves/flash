//! The browser's twins of the voice routes: the same fields as a form
//! from the study page's session, the same JSON back, behind the
//! session cookie and the same-origin guard like every other study
//! mutation. The gate and the blocking call are `api::voice`'s; only the
//! request shape and the error rendering differ. `static/voice.js` posts
//! the study page's spoken turns here.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use flash_core::{CardId, SessionId};
use flash_store::VoiceSide;
use serde::Deserialize;

use crate::api::voice::{
    audio_response, dialog_state, turn, voice_for, AudioMisses, Refused, SideQuery, TurnReply,
    VoiceQuery,
};
use crate::auth::{AnyUser, ApiUser};
use crate::service::{spoken_payload, VoiceError};
use crate::state::{AiOff, AppState, VoiceOff};
use crate::tts::serve::{self, Resolved};

// ---- natural voice: the page's audio ----

const PAGE_MISSES: AudioMisses = AudioMisses {
    not_ready: || (StatusCode::NOT_FOUND, "that clip is not made yet").into_response(),
    not_found: |what| (StatusCode::NOT_FOUND, format!("not found: {what}")).into_response(),
    off: || VoiceOff.into_response(),
};

pub async fn card_audio(
    State(state): State<AppState>,
    AnyUser { id: user, .. }: AnyUser,
    Path(card): Path<i64>,
    Query(query): Query<SideQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(side) = VoiceSide::from_str(&query.side) else {
        return (StatusCode::BAD_REQUEST, "side must be front or back").into_response();
    };
    let app = state.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        serve::resolve_card_clip(&app, user, CardId(card), side, &headers)
    })
    .await
    .unwrap_or(Resolved::NotReady);
    audio_response(resolved, serve::CARD_CACHE, "card", PAGE_MISSES)
}

pub async fn phrase_audio(
    State(state): State<AppState>,
    AnyUser { .. }: AnyUser,
    Path((voice, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if voice.len() > crate::bounds::KEYWORD || key.len() > crate::bounds::KEYWORD {
        return (StatusCode::NOT_FOUND, "not found: phrase").into_response();
    }
    let app = state.clone();
    let resolved = tokio::task::spawn_blocking(move || {
        serve::resolve_phrase_clip(&app, &voice, &key, &headers)
    })
    .await
    .unwrap_or(Resolved::NotReady);
    audio_response(resolved, serve::PHRASE_CACHE, "phrase", PAGE_MISSES)
}

pub async fn answer_audio(
    State(state): State<AppState>,
    AnyUser { id: user, .. }: AnyUser,
    Path(token): Path<String>,
) -> Response {
    if token.len() > crate::bounds::KEYWORD {
        return (StatusCode::NOT_FOUND, "not found: answer").into_response();
    }
    let app = state.clone();
    let resolved =
        tokio::task::spawn_blocking(move || serve::resolve_answer_clip(&app, user, &token))
            .await
            .unwrap_or(Resolved::NotReady);
    audio_response(resolved, "no-store", "answer", PAGE_MISSES)
}

pub async fn phrases(
    State(state): State<AppState>,
    AnyUser { id: user, .. }: AnyUser,
    Query(query): Query<VoiceQuery>,
) -> Response {
    if state.tts.is_none() {
        return VoiceOff.into_response();
    }
    let app = state.clone();
    let set = tokio::task::spawn_blocking(move || {
        let voice = voice_for(&app, user, query.voice)?;
        Ok(serve::phrase_set(&app, &voice, |key| {
            format!("/voice/phrases/{voice}/{key}/audio")
        }))
    })
    .await;
    match set {
        Ok(Ok(Some(set))) => Json(set).into_response(),
        Ok(Ok(None)) => (StatusCode::NOT_FOUND, "not found: voice").into_response(),
        Ok(Err(e)) => crate::web::err_response(e),
        Err(e) => {
            tracing::error!("join: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn voices(State(state): State<AppState>, AnyUser { .. }: AnyUser) -> Response {
    if state.tts.is_none() {
        return VoiceOff.into_response();
    }
    let app = state.clone();
    let listing = tokio::task::spawn_blocking(move || {
        serve::voice_listing(&app, |voice| format!("/voice/phrases/{voice}/sample/audio"))
    })
    .await;
    match listing {
        Ok(Some(listing)) => Json(listing).into_response(),
        Ok(None) => VoiceOff.into_response(),
        Err(e) => {
            tracing::error!("join: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

// ---- the spoken turn ----

/// The session rides in the body, as the study page's other forms carry
/// it. `state` is where the conversation is (`front` or `window`); what
/// was said is the transcript, or the recording's type and base64.
#[derive(Deserialize)]
pub struct TurnForm {
    pub session: i64,
    pub card: i64,
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub state: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_transcript")]
    pub transcript: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_audio_mime")]
    pub audio_mime: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_audio_b64")]
    pub audio_b64: Option<String>,
}

impl From<Refused> for Response {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Off => AiOff.into_response(),
            Refused::Throttled => (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response(),
        }
    }
}

pub async fn spoken_turn(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Form(form): Form<TurnForm>,
) -> Response {
    let dialog = match dialog_state(&form.state) {
        Ok(dialog) => dialog,
        Err(e) => return crate::web::err_response(e),
    };
    let spoken = match spoken_payload(form.transcript, form.audio_mime, form.audio_b64) {
        Ok(spoken) => spoken,
        Err(e) => return crate::web::err_response(e),
    };
    match turn(
        &state,
        user,
        SessionId(form.session),
        CardId(form.card),
        dialog,
        spoken,
    )
    .await
    {
        Ok(Ok(outcome)) => Json(TurnReply::new(outcome, |token| {
            format!("/voice/answers/{token}/audio")
        }))
        .into_response(),
        Ok(Err(VoiceError::Service(e))) => crate::web::err_response(e),
        Ok(Err(VoiceError::Model(e))) => {
            tracing::warn!("voice model call failed: {e}");
            (StatusCode::BAD_GATEWAY, e.user_message()).into_response()
        }
        Err(refused) => refused.into(),
    }
}
