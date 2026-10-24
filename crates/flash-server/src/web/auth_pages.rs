use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::auth::{self, hash_token, Theme};
use crate::ext::Slot;
use crate::flows::enroll;
use crate::i18n::filters;
use crate::service::now_ms;
use crate::state::AppState;

use super::{
    act, clean_next, count_event, dead_link, err_response, is_probably_bot, onward, page_language,
};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage {
    next: String,
    theme: &'static str,
    /// The extension's external sign-in buttons (and their outcome line).
    providers_html: String,
    /// Where "Create an account" points, carrying `next` so a signup
    /// lands where the sign-in would have; None hides it.
    signup_href: Option<String>,
    email: String,
    /// The password form's own failure line.
    error: Option<String>,
    /// Neutral status line (e.g. after account deletion), not an error.
    notice: Option<crate::i18n::Words>,
    /// The server's own methods sit under "Other ways to continue",
    /// below the extension's buttons (`ServerExtension::fold_own_sign_in`).
    fold: bool,
    /// The fold starts open: the password form has something to show
    /// (its error, or the address just tried).
    open: bool,
}

fn render_login(
    state: &AppState,
    theme: Theme,
    next: String,
    provider_err: Option<&str>,
    email: String,
    error: Option<String>,
    notice: Option<crate::i18n::Words>,
) -> Response {
    let providers_html = match state.ext.render(
        state,
        Slot::LoginProviders {
            next: &next,
            err: provider_err,
            ref_code: None,
        },
    ) {
        Ok(html) => html,
        Err(e) => return err_response(e),
    };
    // `next` can carry its own query (a connector's authorize request),
    // so it goes in encoded or its `&` would split this link's query.
    let signup_href = state.ext.signup_path(state).map(|path| {
        if next == "/" {
            path
        } else {
            let sep = if path.contains('?') { '&' } else { '?' };
            format!("{path}{sep}next={}", auth::urlencode(&next))
        }
    });
    let fold = state.ext.fold_own_sign_in(state);
    let open = error.is_some() || !email.is_empty();
    let page = LoginPage {
        next,
        theme: theme.as_str(),
        providers_html,
        signup_href,
        email,
        error,
        notice,
        fold,
        open,
    };
    match crate::i18n::render_current(&page) {
        Ok(html) => Html(html).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::i18n::t("error-render"),
        )
            .into_response(),
    }
}

/// Re-renders the login page with the uniform password-failure message
/// (used by the password login handler in `password.rs`).
pub(crate) fn render_login_error(
    state: &AppState,
    theme: Theme,
    next: String,
    email: String,
    msg: &str,
) -> Response {
    render_login(state, theme, next, None, email, Some(msg.to_string()), None)
}

#[derive(Deserialize)]
pub(super) struct NextQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_next_path")]
    next: Option<String>,
    deleted: Option<u8>,
    /// External sign-in outcomes that need a word on the login page; the
    /// extension owns the codes and their copy.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    err: Option<String>,
    /// Sent by `FullUser`: a session from an emailed link asked for
    /// something that wants a real sign-in.
    again: Option<u8>,
}

pub(super) async fn login_page(
    State(state): State<AppState>,
    auth::MaybeSession(session): auth::MaybeSession,
    theme: Theme,
    Query(query): Query<NextQuery>,
    parts: axum::http::request::Parts,
) -> Response {
    page_language(&state, "login", &parts);
    // Signing in is for the signed out. A signed-in visitor goes on,
    // `err` and all: an external sign-in that ran twice sends its second
    // run here with an error, though the first already signed them in.
    // A session from an emailed link stays: it is here to sign in for
    // real, and sending it on would bring it straight back.
    if session.is_some_and(|s| !s.via_link) {
        return Redirect::to(&onward(query.next)).into_response();
    }
    let notice = if query.again == Some(1) {
        Some(crate::i18n::t("login-again"))
    } else {
        (query.deleted == Some(1)).then(|| crate::i18n::t("login-deleted"))
    };
    // `next` lands in a data attribute (HTML-escaped by askama); only
    // local paths are honored.
    let next = clean_next(query.next);
    render_login(
        &state,
        theme,
        next,
        query.err.as_deref(),
        String::new(),
        None,
        notice,
    )
}

#[derive(Template)]
#[template(path = "enroll.html")]
struct EnrollPage {
    display_name: String,
    token: String,
    theme: &'static str,
    /// The address an extension fixed the invite to, shown read-only.
    email: Option<String>,
    error: Option<crate::i18n::Words>,
    /// Where to land after enrolling; always a local path.
    next: String,
}

/// Re-renders the enroll page with an error (password-chooser failures).
pub(crate) fn render_enroll_error(
    theme: Theme,
    token: &str,
    enrollment: &enroll::Enrollment,
    msg: crate::i18n::Words,
    next: &str,
) -> Response {
    let page = EnrollPage {
        display_name: enrollment.invite.display_name.clone(),
        token: token.to_string(),
        theme: theme.as_str(),
        email: enrollment.preset_email().map(str::to_string),
        error: Some(msg),
        next: next.to_string(),
    };
    Html(crate::i18n::render_current(&page).unwrap_or_default()).into_response()
}

#[derive(Deserialize)]
pub(super) struct EnrollQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_next_path")]
    next: Option<String>,
}

pub(super) async fn enroll_page(
    State(state): State<AppState>,
    theme: Theme,
    Path(token): Path<String>,
    Query(query): Query<EnrollQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    let next = clean_next(query.next);
    if !crate::password::token_shape_ok(&token) {
        return dead_link(
            theme,
            StatusCode::BAD_REQUEST,
            crate::i18n::t("gone-link-body"),
        );
    }
    let hash = hash_token(&token);
    let now = now_ms();
    let hooks = state.ext.clone();
    let enrollment = act(state.services.clone(), move |s| {
        Ok(enroll::load(s.store(), hooks.as_ref(), &hash, now)?)
    })
    .await;
    match enrollment {
        Ok(Some(enrollment)) => {
            if !is_probably_bot(&headers) {
                let ref_code = enrollment
                    .signup
                    .as_ref()
                    .and_then(|s| s.ref_code.as_deref());
                count_event(&state, "enroll_view", ref_code);
            }
            let page = EnrollPage {
                email: enrollment.preset_email().map(str::to_string),
                display_name: enrollment.invite.display_name,
                token,
                theme: theme.as_str(),
                error: None,
                next,
            };
            Html(crate::i18n::render_current(&page).unwrap_or_default()).into_response()
        }
        Ok(None) => dead_link(
            theme,
            StatusCode::FORBIDDEN,
            crate::i18n::t("gone-link-body"),
        ),
        Err(response) => response,
    }
}
