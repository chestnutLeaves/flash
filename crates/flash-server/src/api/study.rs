//! The study loop: start a session, reveal, grade, end. Every call is
//! the same `Services` method the web page and the MCP tools use; the
//! app never decides what comes next.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, Rating, SessionId};
use serde::{Deserialize, Serialize};

use super::dto::{Reveal, SessionSummary, StudyCard};
use super::{app_platform, call, ApiError, ApiJson, ApiResult, BearerUser};
use crate::service::{
    now_ms, CardFront, NextDue, ReviewOrigin, ServiceError, Services, SessionSurface,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/study/sessions", post(start))
        .route("/study/sessions/{sid}/reveal", post(reveal))
        .route("/study/sessions/{sid}/reviews", post(review))
        .route("/study/sessions/{sid}/end", post(end))
}

/// What to study: everything, one deck, one tag, or the cards carrying
/// every one of several tags, inside a deck when one is named.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Scope {
    All,
    Deck {
        id: i64,
    },
    Tag {
        #[serde(deserialize_with = "crate::bounds::tag")]
        tag: String,
    },
    Tags {
        #[serde(deserialize_with = "crate::bounds::scope_tags")]
        tags: Vec<String>,
        #[serde(default)]
        deck: Option<i64>,
    },
}

impl From<Scope> for StudyScope {
    fn from(s: Scope) -> Self {
        match s {
            Scope::All => StudyScope::all(),
            Scope::Deck { id } => StudyScope::deck(DeckId(id)),
            Scope::Tag { tag } => StudyScope::tagged([tag]),
            Scope::Tags { tags, deck } => StudyScope::new(deck.map(DeckId), tags),
        }
    }
}

#[derive(Deserialize)]
struct StartBody {
    #[serde(default = "default_scope")]
    scope: Scope,
}

fn default_scope() -> Scope {
    Scope::All
}

#[derive(Serialize)]
pub struct Session {
    pub session_id: i64,
    pub grading_mode: &'static str,
    /// Queue length at the start; the app keeps it for the progress bar.
    pub total: u32,
    pub remaining: u32,
    /// Eligible cards today's limits kept out of the queue.
    pub held_by_limit: u32,
    /// The deck when the session is deck-scoped, so an empty queue can
    /// offer that deck's boost.
    pub scope_deck: Option<i64>,
    pub card: Option<StudyCard>,
    /// When the next cards come, once the queue is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due: Option<NextDue>,
}

async fn start(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<StartBody>,
) -> ApiResult<Session> {
    let scope: StudyScope = body.scope.into();
    let scope_deck = scope.deck.map(|d| d.0);
    let app = state.clone();
    call(state.services.clone(), move |s| {
        let now = now_ms();
        let info = s.start_session_from(user.id, scope.clone(), SessionSurface::Mobile, now)?;
        let next_due = if info.cards_due == 0 && info.scope_has_cards {
            s.next_due(user.id, &scope, info.held_by_limit, now)?
        } else {
            None
        };
        let card = info
            .first_card
            .map(|c| study_card(&app, &s, user, c, info.next_card_id))
            .transpose()?;
        Ok::<_, ServiceError>(Session {
            session_id: info.session_id.0,
            grading_mode: info.grading_mode.as_str(),
            total: info.cards_due,
            remaining: info.cards_due,
            held_by_limit: info.held_by_limit,
            scope_deck,
            card,
            next_due,
        })
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
struct RevealBody {
    card_id: i64,
    /// What the user typed, when the card asks for it.
    #[serde(default, deserialize_with = "crate::bounds::opt_typed_answer")]
    typed: Option<String>,
}

async fn reveal(
    State(state): State<AppState>,
    user: BearerUser,
    Path(_sid): Path<i64>,
    ApiJson(body): ApiJson<RevealBody>,
) -> ApiResult<Reveal> {
    let reveal = call(state.services.clone(), move |s| {
        let reveal = s.reveal_card(user.id, CardId(body.card_id), body.typed.as_deref())?;
        let extra = state.ext.reveal_json(&state, &s, user.id, &reveal);
        let mut reveal = Reveal::from(reveal);
        reveal.extra = extra;
        Ok::<_, ServiceError>(reveal)
    })
    .await?;
    Ok(Json(reveal))
}

/// A card as the app is sent it, with the extension's fields.
fn study_card(
    state: &AppState,
    s: &Services,
    user: BearerUser,
    front: CardFront,
    next: Option<CardId>,
) -> Result<StudyCard, ServiceError> {
    let card = s.study_card(user.id, front, next)?;
    let extra = state.ext.study_card_json(state, s, user.id, &card);
    let mut card = StudyCard::from(card);
    card.extra = extra;
    Ok(card)
}

#[derive(Deserialize)]
struct ReviewBody {
    card_id: i64,
    rating: i64,
    /// How long the card was on screen before this grade, as the app
    /// measured it; optional, so an app that does not send it still
    /// grades.
    #[serde(default)]
    answer_ms: Option<i64>,
}

#[derive(Serialize)]
pub struct Reviewed {
    pub next_card: Option<StudyCard>,
    pub remaining: u32,
    pub held_by_limit: u32,
    pub scope_deck: Option<i64>,
    /// When the next cards come, once the queue is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due: Option<NextDue>,
}

async fn review(
    State(state): State<AppState>,
    user: BearerUser,
    Path(sid): Path<i64>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<ReviewBody>,
) -> ApiResult<Reviewed> {
    let rating =
        Rating::from_i64(body.rating).ok_or_else(|| ApiError::malformed("rating must be 1-4"))?;
    let platform = app_platform(&headers);
    let app = state.clone();
    call(state.services.clone(), move |s| {
        let session = SessionId(sid);
        let now = now_ms();
        let result = s.submit_review(
            user.id,
            session,
            CardId(body.card_id),
            rating,
            ReviewOrigin::new("mobile", platform).answered_in(body.answer_ms),
            now,
        )?;
        let scope = s.session_scope(user.id, session)?;
        let next_due = if result.next_card.is_none() {
            s.next_due(user.id, &scope, result.held_by_limit, now)?
        } else {
            None
        };
        let next_card = result
            .next_card
            .map(|c| study_card(&app, &s, user, c, result.next_card_id))
            .transpose()?;
        Ok::<_, ServiceError>(Reviewed {
            next_card,
            remaining: result.remaining,
            held_by_limit: result.held_by_limit,
            scope_deck: scope.deck.map(|d| d.0),
            next_due,
        })
    })
    .await
    .map(Json)
}

async fn end(
    State(state): State<AppState>,
    user: BearerUser,
    Path(sid): Path<i64>,
) -> ApiResult<SessionSummary> {
    let stats = call(state.services.clone(), move |s| {
        s.end_session(user.id, SessionId(sid), now_ms())
    })
    .await?;
    Ok(Json(stats.into()))
}
