//! Import over the API: the same two steps as the web page (multipart
//! preview, JSON commit), the same parked-file handoff, the same guard
//! against two imports at once.

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{call, ApiError, ApiJson, ApiResult, BearerUser};
use crate::ext::{AccountEvent, Surface};
use crate::flows::import_flow;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/import/preview",
            // Multipart overhead on top of the largest accepted package.
            post(preview).layer(DefaultBodyLimit::max(import_flow::MAX_BYTES + 64 * 1024)),
        )
        .route("/import/commit", post(commit))
}

fn busy() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "busy",
        crate::i18n::t("import-busy"),
    )
}

#[derive(Serialize)]
struct Preview {
    total: usize,
    file_name: String,
    file_size_bytes: usize,
    /// Prefill for the one destination, and the line about decks that
    /// will merge into it: what a client that does not send `keep_decks`
    /// shows.
    deck: String,
    deck_note: Option<crate::i18n::Words>,
    /// The decks the file names, in its order. More than one, and a client
    /// can send `keep_decks` to keep them, with `deck` as an optional deck
    /// to put them under, prefilled from `parent`.
    file_decks: Vec<FileDeck>,
    parent: String,
    warnings: u32,
    sample: Vec<SampleRow>,
    messages: Vec<crate::i18n::Words>,
    token: String,
    progress_reviews: usize,
    settings_desc: Option<String>,
    colored_cards: usize,
}

#[derive(Serialize)]
struct FileDeck {
    /// The whole path, as the file names it.
    name: String,
    cards: usize,
}

#[derive(Serialize)]
struct SampleRow {
    front: String,
    back: String,
}

async fn preview(
    State(state): State<AppState>,
    user: BearerUser,
    mut multipart: Multipart,
) -> ApiResult<Preview> {
    // Bound how many upload bodies are in memory before reading this one:
    // the body limit is per request, and a handful of concurrent 100 MB
    // uploads would otherwise exceed the whole process's budget.
    let Ok(_upload) = state.upload_semaphore.clone().try_acquire_owned() else {
        return Err(busy());
    };
    let mut file: Option<(String, axum::body::Bytes)> = None;
    let mut deck = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("file") => {
                let name = field.file_name().unwrap_or("upload").to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, bytes)),
                    Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                        return Err(ApiError::payload_too_large());
                    }
                    Err(e) => return Err(ApiError::malformed(format!("upload: {e}"))),
                }
            }
            Some("deck") => {
                deck = field.text().await.unwrap_or_default().trim().to_string();
                // The one text field of the form, bounded at the edge like
                // a request struct's field would be.
                if deck.len() > crate::bounds::DECK_NAME {
                    return Err(ApiError::invalid(crate::i18n::t("api-deck-name-long")));
                }
            }
            _ => {}
        }
    }
    let Some((filename, bytes)) = file else {
        return Err(ApiError::malformed("no file"));
    };
    if filename.len() > crate::bounds::FILENAME {
        return Err(ApiError::invalid(crate::i18n::t("api-file-name-long")));
    }
    let Ok(_permit) = state.import_semaphore.clone().try_acquire_owned() else {
        return Err(busy());
    };
    let heavy = state.heavy(user.id)?;
    let dir = import_flow::dir(&state.config);
    let preview = call(state.services.clone(), move |_| {
        import_flow::preview(&heavy, &dir, &filename, &bytes, &deck).map_err(ApiError::from)
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("import_preview", Surface::Mobile),
    );
    Ok(Json(Preview {
        total: preview.total,
        file_name: preview.file_name,
        file_size_bytes: preview.file_size_bytes,
        deck: preview.deck,
        deck_note: preview.deck_note,
        file_decks: preview
            .file_decks
            .into_iter()
            .map(|(name, cards)| FileDeck { name, cards })
            .collect(),
        parent: preview.parent,
        warnings: preview.warnings,
        sample: preview
            .sample
            .into_iter()
            .map(|(front, back)| SampleRow { front, back })
            .collect(),
        messages: preview.messages,
        token: preview.token,
        progress_reviews: preview.progress_reviews,
        settings_desc: preview.settings_desc,
        colored_cards: preview.colored_cards,
    }))
}

#[derive(Deserialize)]
struct CommitBody {
    #[serde(deserialize_with = "crate::bounds::token")]
    token: String,
    /// Where the cards go; with `keep_decks`, the deck to put the file's
    /// decks under, which may be blank.
    #[serde(default, deserialize_with = "crate::bounds::deck_name")]
    deck: String,
    /// Keep the decks a file names instead of merging them into `deck`.
    /// Absent from a client built before decks nested, which gets the
    /// merge it was built around.
    #[serde(default)]
    keep_decks: bool,
    #[serde(default)]
    progress: bool,
    #[serde(default)]
    settings: bool,
    /// "keep" (default) or "clean".
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    colors: Option<String>,
}

#[derive(Serialize)]
struct Committed {
    imported: usize,
}

async fn commit(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<CommitBody>,
) -> ApiResult<Committed> {
    let Ok(_permit) = state.import_semaphore.clone().try_acquire_owned() else {
        return Err(busy());
    };
    let heavy = state.heavy(user.id)?;
    let dir = import_flow::dir(&state.config);
    let media = state.media.clone();
    let opts = import_flow::CommitOptions {
        token: body.token,
        deck: body.deck,
        keep_decks: body.keep_decks,
        progress: body.progress,
        settings: body.settings,
        clean_colors: body.colors.as_deref() == Some("clean"),
    };
    let committed = call(state.services.clone(), move |s| {
        let imported = import_flow::commit(&heavy, &s, media.as_ref(), &dir, user.is_admin, &opts)
            .map_err(ApiError::from)?;
        Ok::<_, ApiError>(Committed { imported })
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("import_commit", Surface::Mobile).amount(Some(committed.imported as i64)),
    );
    Ok(Json(committed))
}
