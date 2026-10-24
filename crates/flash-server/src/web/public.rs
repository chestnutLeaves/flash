use crate::i18n::filters;
use std::sync::Arc;

use askama::Template;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use super::{count_event, is_probably_bot, page_language, view_event, Via};
use crate::auth::{OptionalUser, Theme};
use crate::state::{AppState, Site};

// ---- crawler surfaces ----

/// A private server has nothing for crawlers.
pub(super) async fn robots_private() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "max-age=3600"),
        ],
        "User-agent: *\nDisallow: /\n",
    )
}

#[derive(Template)]
#[template(path = "notfound.html")]
struct NotFoundPage {
    theme: &'static str,
}

/// Explicit app-wide fallback: without one, unknown paths fall through to
/// the bearer-auth layer's wrapped default fallback and 401 instead of 404.
pub async fn not_found(theme: Theme) -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(
            NotFoundPage {
                theme: theme.as_str(),
            }
            .render_with_values(&crate::i18n::reader_values())
            .unwrap_or_default(),
        ),
    )
        .into_response()
}

#[derive(Template)]
#[template(path = "gone.html")]
struct GonePage {
    theme: &'static str,
    title: crate::i18n::Words,
    heading: crate::i18n::Words,
    body: Option<crate::i18n::Words>,
}

/// A page for an address that answers nothing any more, with `status`:
/// a link that expired, was used or was turned off, or a feature this
/// server does not offer. A person who follows one sees why and a way
/// home, never an empty page.
pub fn gone(
    theme: Theme,
    status: StatusCode,
    title: crate::i18n::Words,
    heading: crate::i18n::Words,
    body: Option<crate::i18n::Words>,
) -> Response {
    (
        status,
        Html(
            GonePage {
                theme: theme.as_str(),
                title,
                heading,
                body,
            }
            .render_with_values(&crate::i18n::reader_values())
            .unwrap_or_default(),
        ),
    )
        .into_response()
}

pub fn dead_link(theme: Theme, status: StatusCode, body: crate::i18n::Words) -> Response {
    gone(
        theme,
        status,
        crate::i18n::t("gone-title"),
        crate::i18n::t("gone-heading"),
        Some(body),
    )
}

// ---- connect (public how-to for attaching an AI) ----

#[derive(Template)]
#[template(path = "connect.html")]
struct ConnectPage {
    site: Arc<Site>,
    theme: &'static str,
    /// Header shows "Back to Flash" instead of "Log in" for signed-in users.
    signed_in: bool,
    /// The extension's head tags (`Slot::PublicHead`).
    head_html: String,
    app_steps_html: String,
}

/// `/connect`, also under a language's prefix (`/es/connect`) where a
/// deployment serves its public pages per language.
pub async fn connect_page(
    State(state): State<AppState>,
    OptionalUser(session): OptionalUser,
    theme: Theme,
    headers: axum::http::HeaderMap,
    via: Via,
    parts: axum::http::request::Parts,
) -> Response {
    page_language(&state, "connect", &parts);
    match session {
        Some((user, _)) => view_event(&state, user, "connect", via),
        None if !is_probably_bot(&headers) => count_event(&state, "connect_load", None),
        None => {}
    }
    let page = ConnectPage {
        site: state.site.clone(),
        theme: theme.as_str(),
        signed_in: session.is_some(),
        head_html: state
            .ext
            .render(&state, crate::ext::Slot::PublicHead)
            .unwrap_or_default(),
        app_steps_html: state
            .ext
            .render(&state, crate::ext::Slot::ConnectAppSteps)
            .unwrap_or_default(),
    };
    match crate::i18n::render_current(&page) {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("connect render: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-render"),
            )
                .into_response()
        }
    }
}

/// A public page rendered whole; a template failure is logged and
/// answered with a plain 500 (public pages have no shell to fall back to).
pub fn render_public(name: &str, page: Result<String, askama::Error>) -> Response {
    match page {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(template = name, "render failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-render"),
            )
                .into_response()
        }
    }
}
