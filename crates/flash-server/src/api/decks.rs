//! Decks: list, create, detail (settings), rename, move, limits, boost,
//! archive and unarchive, delete, the paged card list, quick-add, and the
//! rich editor's create path.
//!
//! A deck's `name` is its whole path (`Bio::Cells`), so a create or a
//! rename that names a path nests the deck. `parent_id` on create and the
//! move route are the same thing said with an id, for a client that shows
//! a picker and never has a path typed into it.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use flash_core::DeckId;
use flash_store::notes::NoteType;
use flash_store::CardStatus;
use serde::{Deserialize, Serialize};

use super::dto::{Card, Deck, DeckDetail};
use super::today::{BoostBody, Boosted};
use super::{call, ApiError, ApiJson, ApiResult, BearerUser, Page};
use crate::ext::{AccountEvent, Surface};
use crate::service::deck_cards::DeckFilter;
use crate::service::{now_ms, NoteInput};
use crate::state::AppState;

/// The web's page size; the app may ask for up to 100.
const DEFAULT_PER_PAGE: u32 = 25;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/decks", get(list).post(create))
        .route("/decks/{id}", get(detail).patch(rename).delete(delete_deck))
        .route("/decks/{id}/move", post(move_deck))
        .route("/decks/{id}/limits", axum::routing::put(set_limits))
        .route("/decks/{id}/goal", axum::routing::put(set_goal))
        .route("/decks/{id}/boost", post(boost))
        .route("/decks/{id}/archive", post(archive))
        .route("/decks/{id}/unarchive", post(unarchive))
        .route("/decks/{id}/cards", get(cards).post(quick_add))
        .route("/decks/{id}/notes", post(create_note))
}

#[derive(Serialize)]
pub struct Decks {
    pub items: Vec<Deck>,
}

async fn list(State(state): State<AppState>, user: BearerUser) -> ApiResult<Decks> {
    let decks = call(state.services.clone(), move |s| {
        s.list_decks(user.id, now_ms())
    })
    .await?;
    Ok(Json(Decks {
        items: decks.into_iter().map(Into::into).collect(),
    }))
}

#[derive(Deserialize)]
struct NewDeckBody {
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    name: String,
    #[serde(default, deserialize_with = "crate::bounds::deck_description")]
    description: String,
    /// The deck to make this one inside; absent or null for where `name`
    /// itself says.
    #[serde(default)]
    parent_id: Option<i64>,
}

async fn create(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<NewDeckBody>,
) -> Result<(StatusCode, Json<Deck>), ApiError> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::invalid(crate::i18n::t("deck-error-empty")));
    }
    let deck = call(state.services.clone(), move |s| {
        let now = now_ms();
        let description = body.description.trim();
        let id = match body.parent_id {
            Some(parent) => s.create_subdeck(user.id, DeckId(parent), &name, description, now)?,
            None => s.create_deck(user.id, &name, description, now)?,
        };
        s.deck_by_id(user.id, id, now)
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("deck_created", Surface::Mobile),
    );
    Ok((StatusCode::CREATED, Json(deck.into())))
}

async fn detail(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<DeckDetail> {
    let detail = call(state.services.clone(), move |s| {
        s.deck_detail(user.id, DeckId(id), now_ms())
    })
    .await?;
    Ok(Json(detail.into()))
}

#[derive(Deserialize)]
struct RenameBody {
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    name: String,
}

async fn rename(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<RenameBody>,
) -> ApiResult<Deck> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::invalid(crate::i18n::t("deck-error-empty")));
    }
    let deck = call(state.services.clone(), move |s| {
        s.rename_deck(user.id, DeckId(id), &name)?;
        s.deck_by_id(user.id, DeckId(id), now_ms())
    })
    .await?;
    Ok(Json(deck.into()))
}

#[derive(Deserialize)]
struct MoveBody {
    /// The deck to move into; null for the top level.
    parent_id: Option<i64>,
}

/// Moves the deck, with the decks under it, and answers with it as it is
/// now: a new `name`, `depth` and `parent_id`.
async fn move_deck(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<MoveBody>,
) -> ApiResult<Deck> {
    let deck = call(state.services.clone(), move |s| {
        s.move_deck(user.id, DeckId(id), body.parent_id.map(DeckId))?;
        s.deck_by_id(user.id, DeckId(id), now_ms())
    })
    .await?;
    Ok(Json(deck.into()))
}

/// Null (or absent) = inherit the account default.
#[derive(Deserialize)]
struct LimitsBody {
    #[serde(default)]
    new_per_day: Option<u32>,
    #[serde(default)]
    reviews_per_day: Option<u32>,
}

async fn set_limits(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<LimitsBody>,
) -> Result<StatusCode, ApiError> {
    call(state.services.clone(), move |s| {
        s.set_deck_limits(user.id, DeckId(id), body.new_per_day, body.reviews_per_day)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A deck's goal date, `YYYY-MM-DD`; null (or absent) clears it.
#[derive(Deserialize)]
struct GoalBody {
    #[serde(default, deserialize_with = "crate::bounds::opt_goal_date")]
    goal_date: Option<String>,
}

async fn set_goal(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<GoalBody>,
) -> Result<StatusCode, ApiError> {
    let date = match body.goal_date.as_deref() {
        None => None,
        Some(text) => Some(flash_core::goal::parse_date(text).ok_or_else(|| {
            crate::service::ServiceError::Malformed("goal_date must be YYYY-MM-DD".into())
        })?),
    };
    call(state.services.clone(), move |s| {
        s.set_deck_goal(user.id, DeckId(id), date, now_ms())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn boost(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<BoostBody>,
) -> ApiResult<Boosted> {
    let boost_today = call(state.services.clone(), move |s| {
        s.boost_new_today(user.id, Some(DeckId(id)), body.extra, now_ms())
    })
    .await?;
    Ok(Json(Boosted { boost_today }))
}

/// Archive: the deck keeps every card and its history but leaves the
/// study queue and the due counts. Both calls are idempotent.
async fn archive(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    call(state.services.clone(), move |s| {
        s.archive_deck(user.id, DeckId(id), now_ms())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unarchive(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    call(state.services.clone(), move |s| {
        s.unarchive_deck(user.id, DeckId(id))
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct DeckDeleted {
    cards_deleted: u32,
}

async fn delete_deck(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
) -> ApiResult<DeckDeleted> {
    let deleted = call(state.services.clone(), move |s| {
        s.delete_deck(user.id, DeckId(id))
    })
    .await?;
    if deleted.cards > 0 {
        state.ext.account_event(
            &state.services,
            user.id,
            AccountEvent::new("cards_deleted", Surface::Mobile)
                .detail(Some("deck"))
                .amount(Some(deleted.cards as i64)),
        );
    }
    // The deletion owes its orphaned blobs to the job worker.
    crate::jobs::wake(&state);
    Ok(Json(DeckDeleted {
        cards_deleted: deleted.cards,
    }))
}

#[derive(Deserialize)]
struct CardsQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_search")]
    q: Option<String>,
    page: Option<u32>,
    per_page: Option<u32>,
}

async fn cards(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    Query(query): Query<CardsQuery>,
) -> ApiResult<Page<Card>> {
    let per_page = query.per_page.unwrap_or(DEFAULT_PER_PAGE).clamp(1, 100);
    // The web deck page's list, archived cards included.
    let filter = DeckFilter {
        search: DeckFilter::search_from(query.q.as_deref()),
        status: CardStatus::All,
        ..DeckFilter::default()
    };
    let page = call(state.services.clone(), move |s| {
        s.deck_cards_filtered(
            user.id,
            DeckId(id),
            &filter,
            query.page.unwrap_or(1),
            per_page,
        )
    })
    .await?;
    Ok(Json(Page {
        items: page.rows.into_iter().map(Into::into).collect(),
        page: page.page,
        pages: page.pages,
        total: page.total,
    }))
}

#[derive(Deserialize)]
struct QuickAddBody {
    #[serde(deserialize_with = "crate::bounds::card_side")]
    front: String,
    #[serde(deserialize_with = "crate::bounds::card_side")]
    back: String,
    #[serde(default, deserialize_with = "crate::bounds::tags")]
    tags: Vec<String>,
}

#[derive(Serialize)]
pub struct CardsCreated {
    pub card_ids: Vec<i64>,
}

async fn quick_add(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<QuickAddBody>,
) -> Result<(StatusCode, Json<CardsCreated>), ApiError> {
    let ids = call(state.services.clone(), move |s| {
        s.create_cards_in_deck(
            user.id,
            DeckId(id),
            &[(body.front, body.back, body.tags)],
            now_ms(),
        )
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("cards_created", Surface::Mobile)
            .detail(Some("manual"))
            .amount(Some(ids.len() as i64)),
    );
    Ok((
        StatusCode::CREATED,
        Json(CardsCreated {
            card_ids: ids.into_iter().map(|c| c.0).collect(),
        }),
    ))
}

/// The editor's submission, shared with `PUT /cards/{id}/editor`.
#[derive(Deserialize)]
pub struct NoteBody {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub note_type: String,
    #[serde(default, deserialize_with = "crate::bounds::field_html")]
    pub front_html: String,
    #[serde(default, deserialize_with = "crate::bounds::field_html")]
    pub back_html: String,
    #[serde(default, deserialize_with = "crate::bounds::tags")]
    pub tags: Vec<String>,
    /// The language picked for each field; absent (an app from before
    /// the picker) leaves the note's as they are, a null clears one.
    #[serde(default)]
    pub langs: Option<LangsBody>,
}

/// The language picked for each field of a card, as a tag ("ja",
/// "es-MX"); null where none is.
#[derive(Deserialize)]
pub struct LangsBody {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub front: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub back: Option<String>,
}

impl NoteBody {
    pub fn into_input(self) -> Result<NoteInput, ApiError> {
        let note_type = NoteType::parse(&self.note_type)
            .ok_or_else(|| ApiError::malformed("unknown note type"))?;
        Ok(NoteInput {
            note_type,
            front_html: self.front_html,
            back_html: self.back_html,
            tags: self.tags,
            speech: Default::default(),
            langs: self.langs.map(|l| flash_store::NoteLangs {
                front: l.front,
                back: l.back,
            }),
            allow_external_images: false,
        })
    }
}

#[derive(Serialize)]
pub struct NoteSaved {
    pub note_id: i64,
    pub card_ids: Vec<i64>,
}

async fn create_note(
    State(state): State<AppState>,
    user: BearerUser,
    Path(id): Path<i64>,
    ApiJson(body): ApiJson<NoteBody>,
) -> Result<(StatusCode, Json<NoteSaved>), ApiError> {
    let input = body.into_input()?;
    let (note, ids) = call(state.services.clone(), move |s| {
        s.create_note(user.id, DeckId(id), &input, now_ms())
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("note_created", Surface::Mobile),
    );
    Ok((
        StatusCode::CREATED,
        Json(NoteSaved {
            note_id: note.0,
            card_ids: ids.into_iter().map(|c| c.0).collect(),
        }),
    ))
}
