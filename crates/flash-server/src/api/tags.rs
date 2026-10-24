//! The tags a user's cards carry, so a client can offer them as a study
//! filter. Names come back sorted, with the number of live cards each
//! one is on.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flash_core::DeckId;
use serde::Deserialize;

use super::dto::Tag;
use super::{call, ApiResult, BearerUser};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/tags", get(list))
}

#[derive(serde::Serialize)]
pub struct Tags {
    pub items: Vec<Tag>,
}

#[derive(Deserialize)]
struct TagsQuery {
    /// Only the tags inside this deck and its subdecks; omit for every
    /// deck. A deck that is not the user's is not found.
    deck: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    user: BearerUser,
    Query(query): Query<TagsQuery>,
) -> ApiResult<Tags> {
    let tags = call(state.services.clone(), move |s| {
        s.list_tags(user.id, query.deck.map(DeckId))
    })
    .await?;
    Ok(Json(Tags {
        items: tags.into_iter().map(Into::into).collect(),
    }))
}
