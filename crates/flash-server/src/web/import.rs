use askama::Template;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;

use crate::auth::{AuthUser, Theme};
use crate::flows::{import_flow, Failure};
use crate::i18n::filters;
use crate::state::AppState;

use super::{account_event, respond, Shell};

struct ImportPreview {
    total: usize,
    /// What was uploaded, echoed back since a file input can't be re-filled.
    file_name: String,
    file_size: crate::i18n::Words,
    /// Prefill for the required destination field (may be empty).
    deck: String,
    /// A file that names several decks keeps them, so the field asks for
    /// an optional deck to put them under (prefilled with `parent`) and
    /// the page lists the first of them, `more_decks` standing for the
    /// rest.
    keeps_decks: bool,
    parent: String,
    file_decks: Vec<(String, usize)>,
    more_decks: usize,
    warnings: u32,
    sample: Vec<(String, String)>,
    messages: Vec<crate::i18n::Words>,
    token: String,
    /// Total Anki reviews found: >0 offers the bring-your-progress box.
    progress_reviews: usize,
    /// Human summary of importable Anki FSRS settings, when found.
    settings_desc: Option<String>,
    /// Cards whose deck colors were remapped to the Flash palette: >0
    /// offers the keep-colors/clean-look choice.
    colored_cards: usize,
}

#[derive(Template)]
#[template(path = "import.html")]
struct ImportPage {
    shell: Shell,
    preview: Option<ImportPreview>,
    imported: Option<usize>,
}

pub(super) async fn import_page(
    State(state): State<AppState>,
    AuthUser { is_admin, .. }: AuthUser,
    theme: Theme,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    respond(state.services.clone(), move |_| {
        Ok(ImportPage {
            shell,
            preview: None,
            imported: None,
        })
    })
    .await
}

/// How many of a file's decks the preview names before "+N more".
const PREVIEW_DECKS: usize = 12;

impl From<import_flow::ImportPreview> for ImportPreview {
    fn from(p: import_flow::ImportPreview) -> Self {
        // Read as a person reads a path, not as the file spells it.
        let mut file_decks: Vec<(String, usize)> = p
            .file_decks
            .into_iter()
            .map(|(name, cards)| (flash_core::deck_path::display(&name), cards))
            .collect();
        let more_decks = file_decks.len().saturating_sub(PREVIEW_DECKS);
        let keeps_decks = file_decks.len() > 1;
        file_decks.truncate(PREVIEW_DECKS);
        ImportPreview {
            total: p.total,
            file_name: p.file_name,
            file_size: human_size(p.file_size_bytes),
            deck: p.deck,
            keeps_decks,
            parent: p.parent,
            file_decks,
            more_decks,
            warnings: p.warnings,
            sample: p.sample,
            messages: p.messages,
            token: p.token,
            progress_reviews: p.progress_reviews,
            settings_desc: p.settings_desc,
            colored_cards: p.colored_cards,
        }
    }
}

/// "8.2 MB", "168 KB", "512 B".
fn human_size(bytes: usize) -> crate::i18n::Words {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    use crate::i18n::ta;
    if b >= MB {
        let mb = import_flow::one_decimal(b / MB);
        ta("import-size-mb", &[("n", mb.into())])
    } else if b >= 1024.0 {
        ta(
            "import-size-kb",
            &[("n", format!("{:.0}", b / 1024.0).into())],
        )
    } else {
        ta("import-size-b", &[("n", bytes.to_string().into())])
    }
}

pub(super) async fn import_preview(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    mut multipart: Multipart,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    // Bound how many upload bodies are in memory before reading this one:
    // the body limit is per request, and a handful of concurrent 100 MB
    // uploads would otherwise exceed the whole process's budget.
    let Ok(_upload) = state.upload_semaphore.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            crate::i18n::t("import-busy-uploads"),
        )
            .into_response();
    };
    let mut file: Option<(String, axum::body::Bytes)> = None;
    let mut deck = String::new();
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("file") => {
                let name = field.file_name().unwrap_or("upload").to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, bytes)),
                    // Includes the over-limit case: the body cap bites while
                    // streaming the field, so oversized uploads land here too.
                    Err(e) => {
                        tracing::warn!(user = user.raw(), file = ?name, "import upload failed: {e}");
                        return (e.status(), format!("upload: {e}")).into_response();
                    }
                }
            }
            Some("deck") => {
                deck = field.text().await.unwrap_or_default().trim().to_string();
                // The one text field of the form, bounded at the edge like
                // a request struct's field would be.
                if deck.len() > crate::bounds::DECK_NAME {
                    return (
                        StatusCode::BAD_REQUEST,
                        crate::i18n::t("import-deck-too-long"),
                    )
                        .into_response();
                }
            }
            _ => {}
        }
    }
    let Some((filename, bytes)) = file else {
        tracing::warn!(user = user.raw(), "import rejected: no file in upload");
        return (StatusCode::BAD_REQUEST, crate::i18n::t("import-no-file")).into_response();
    };
    if filename.len() > crate::bounds::FILENAME {
        return (
            StatusCode::BAD_REQUEST,
            crate::i18n::t("import-name-too-long"),
        )
            .into_response();
    }
    // Kept for the failure log after the closure takes ownership.
    let (log_file, log_bytes) = (filename.clone(), bytes.len());
    let Ok(_permit) = state.import_semaphore.clone().try_acquire_owned() else {
        return (StatusCode::TOO_MANY_REQUESTS, crate::i18n::t("import-busy")).into_response();
    };
    let heavy = match state.heavy(user) {
        Ok(permit) => permit,
        Err(busy) => return busy.into_response(),
    };

    let dir = import_flow::dir(&state.config);
    let response = crate::i18n::blocking(move || -> Result<ImportPage, Failure> {
        let preview = import_flow::preview(&heavy, &dir, &filename, &bytes, &deck)?;
        Ok(ImportPage {
            shell,
            preview: Some(preview.into()),
            imported: None,
        })
    })
    .await;
    match response {
        Ok(Ok(page)) => {
            account_event(&state, user, "import_preview", None, None);
            Html(crate::i18n::render_current(&page).unwrap_or_default()).into_response()
        }
        Ok(Err(Failure::User(msg))) => {
            tracing::warn!(
                user = user.raw(),
                file = ?log_file,
                bytes = log_bytes,
                "import preview rejected: {msg}"
            );
            (StatusCode::BAD_REQUEST, msg).into_response()
        }
        Ok(Err(Failure::Internal(detail))) => {
            tracing::error!(
                user = user.raw(),
                file = ?log_file,
                "import preview failed: {detail}"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(user = user.raw(), file = ?log_file, "import join: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct CommitForm {
    #[serde(deserialize_with = "crate::bounds::token")]
    token: String,
    /// The deck named on the preview: where the cards go, or, with
    /// `keep_decks`, the optional deck to put the file's decks under.
    #[serde(default, deserialize_with = "crate::bounds::deck_name")]
    deck: String,
    /// "on" from a preview of a file that names several decks.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    keep_decks: Option<String>,
    /// "on" when the user chose to bring their Anki study progress.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    progress: Option<String>,
    /// "on" when the user chose to adopt the package's FSRS settings.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    settings: Option<String>,
    /// "keep" (default) leaves remapped deck colors; "clean" strips them.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    colors: Option<String>,
}

pub(super) async fn import_commit(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Form(form): Form<CommitForm>,
) -> Response {
    let Ok(_permit) = state.import_semaphore.clone().try_acquire_owned() else {
        return (StatusCode::TOO_MANY_REQUESTS, crate::i18n::t("import-busy")).into_response();
    };
    let heavy = match state.heavy(user) {
        Ok(permit) => permit,
        Err(busy) => return busy.into_response(),
    };
    let dir = import_flow::dir(&state.config);
    let media_store = state.media.clone();
    let services = state.services.clone();
    let opts = import_flow::CommitOptions {
        token: form.token,
        deck: form.deck,
        keep_decks: form.keep_decks.is_some(),
        progress: form.progress.is_some(),
        settings: form.settings.is_some(),
        clean_colors: form.colors.as_deref() == Some("clean"),
    };
    let result = crate::i18n::blocking(move || -> Result<usize, Failure> {
        import_flow::commit(
            &heavy,
            &services,
            media_store.as_ref(),
            &dir,
            is_admin,
            &opts,
        )
    })
    .await;
    match result {
        Ok(Ok(n)) => {
            account_event(&state, user, "import_commit", None, Some(n as i64));
            Html(
                ImportPage {
                    shell: Shell::new(&state, theme, "decks", is_admin),
                    preview: None,
                    imported: Some(n),
                }
                .render_with_values(&crate::i18n::reader_values())
                .unwrap_or_default(),
            )
            .into_response()
        }
        Ok(Err(Failure::User(msg))) => {
            tracing::warn!(user = user.raw(), "import commit rejected: {msg}");
            (StatusCode::BAD_REQUEST, msg).into_response()
        }
        Ok(Err(Failure::Internal(detail))) => {
            tracing::error!(user = user.raw(), "import commit failed: {detail}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(user = user.raw(), "import commit join: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
    }
}
