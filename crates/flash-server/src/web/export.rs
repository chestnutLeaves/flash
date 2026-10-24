//! Exports are never gated: your cards are always yours.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use flash_core::{DeckId, UserId};

use super::{account_event, act};
use crate::auth::FullUser;
use crate::flows::export_flow;
use crate::service::now_ms;
use crate::state::AppState;

pub(super) async fn export_apkg(State(state): State<AppState>, user: FullUser) -> Response {
    apkg_of(state, user.id, None).await
}

pub(super) async fn export_csv(State(state): State<AppState>, user: FullUser) -> Response {
    csv_of(state, user.id, None).await
}

/// A deck's own Anki package: the deck and its subdecks, named from the
/// deck.
pub(super) async fn deck_apkg(
    State(state): State<AppState>,
    user: FullUser,
    Path(id): Path<i64>,
) -> Response {
    apkg_of(state, user.id, Some(DeckId(id))).await
}

/// A deck's own cards as a spreadsheet.
pub(super) async fn deck_csv(
    State(state): State<AppState>,
    user: FullUser,
    Path(id): Path<i64>,
) -> Response {
    csv_of(state, user.id, Some(DeckId(id))).await
}

async fn apkg_of(state: AppState, user: UserId, deck: Option<DeckId>) -> Response {
    let heavy = match state.heavy(user) {
        Ok(permit) => permit,
        Err(busy) => return busy.into_response(),
    };
    let media_store = state.media.clone();
    let config = state.config.clone();
    let now = now_ms();
    let result = act(state.services.clone(), move |s| {
        let name = match deck {
            Some(deck) => Some(s.deck_by_id(user, deck, now)?.leaf),
            None => None,
        };
        let file = export_flow::apkg_in(&heavy, &s, media_store.as_ref(), &config, deck, now)?;
        Ok((file, name))
    })
    .await;
    match result {
        Ok((export, name)) => {
            let (detail, file) = match name {
                Some(name) => ("deck-apkg", export_flow::deck_file_name(&name, "apkg", now)),
                None => ("apkg", export_flow::file_name("apkg", now)),
            };
            account_event(&state, user, "export", Some(detail), None);
            export_flow::attachment_stream(export, "application/octet-stream", file).await
        }
        Err(response) => response,
    }
}

async fn csv_of(state: AppState, user: UserId, deck: Option<DeckId>) -> Response {
    let heavy = match state.heavy(user) {
        Ok(permit) => permit,
        Err(busy) => return busy.into_response(),
    };
    let now = now_ms();
    let result = act(state.services.clone(), move |s| match deck {
        Some(deck) => {
            let name = s.deck_by_id(user, deck, now)?.leaf;
            Ok((export_flow::deck_csv(&heavy, &s, deck)?, Some(name)))
        }
        None => Ok((export_flow::csv(&heavy, &s)?, None)),
    })
    .await;
    match result {
        Ok((csv, name)) => {
            let (detail, disposition) = match name {
                Some(name) => (
                    "deck-csv",
                    export_flow::disposition(&export_flow::deck_file_name(&name, "csv", now)),
                ),
                None => (
                    "csv",
                    format!(
                        "attachment; filename=\"{}\"",
                        export_flow::file_name("csv", now)
                    ),
                ),
            };
            account_event(&state, user, "export", Some(detail), None);
            (
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                csv,
            )
                .into_response()
        }
        Err(response) => response,
    }
}
