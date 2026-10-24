//! Session tokens and the authenticated-user extractor.
//!
//! Tokens are 256-bit randoms; only their SHA-256 hex ever reaches the
//! database, so a DB leak leaks nothing usable.

use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::CookieJar;
use base64::Engine;
use flash_core::UserId;
use sha2::{Digest, Sha256};

use crate::service::now_ms;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "__Host-flash";
pub const CEREMONY_COOKIE: &str = "__Host-flash-ceremony";
pub const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
pub const CEREMONY_TTL_MS: i64 = 5 * 60 * 1000;

/// 256-bit random token, base64url.
pub fn new_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// base64url(SHA256(verifier)): the PKCE S256 code challenge (RFC 7636),
/// used both when Flash is the client (of an external identity provider)
/// and the server (MCP OAuth).
pub fn pkce_s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `Set-Cookie` value for a host-locked, HttpOnly session cookie.
/// SameSite=Lax: the OAuth authorize redirect arrives as a top-level
/// cross-site GET and must still see the session.
pub fn cookie_header(name: &str, token: &str, max_age_secs: i64) -> String {
    format!("{name}={token}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}")
}

pub fn clear_cookie_header(name: &str) -> String {
    format!("{name}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0")
}

pub const THEME_COOKIE: &str = "flash-theme";

/// `Set-Cookie` value for a preference the page's own script may also
/// write: how the deck list is shown, which holds nothing secret and must
/// change the moment it is clicked, without a request. So it is not
/// HttpOnly, where the session cookie is. `None` forgets it. `static/ui.js`
/// writes the same attributes.
pub fn pref_cookie_header(name: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{name}={value}; Path=/; Secure; SameSite=Lax; Max-Age=31536000"),
        None => format!("{name}=; Path=/; Secure; SameSite=Lax; Max-Age=0"),
    }
}

pub const COLLAPSED_DECKS_COOKIE: &str = "flash-decks-folded";

/// The decks this browser has folded shut in its deck lists, by id. Every
/// deck starts open, so the cookie names only the ones that were closed,
/// and they stay closed until they are opened again: the same kind of
/// memory as `ShowArchived`, and as far from the address.
///
/// The value is ids joined by dots. Whatever else turns up in it is
/// dropped, and the list is held to `MAX`, so a cookie someone wrote by
/// hand costs nothing to read.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CollapsedDecks(pub Vec<i64>);

impl CollapsedDecks {
    /// Far more than anyone folds, and well inside a cookie's 4 KB.
    pub const MAX: usize = 200;

    pub fn parse(value: &str) -> Self {
        let mut ids: Vec<i64> = value
            .split('.')
            .filter_map(|id| id.parse().ok())
            .filter(|id| *id > 0)
            .take(Self::MAX)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        Self(ids)
    }

    pub fn contains(&self, deck: i64) -> bool {
        self.0.binary_search(&deck).is_ok()
    }

    /// With `deck` folded or not, and only the decks in `existing` kept,
    /// so a deleted deck's id does not ride along for a year.
    pub fn with(&self, deck: i64, collapsed: bool, existing: &[i64]) -> Self {
        let mut ids: Vec<i64> = self
            .0
            .iter()
            .copied()
            .filter(|id| *id != deck && existing.contains(id))
            .collect();
        if collapsed && existing.contains(&deck) {
            ids.push(deck);
        }
        ids.sort_unstable();
        ids.truncate(Self::MAX);
        Self(ids)
    }

    /// The `Set-Cookie` value that remembers this, or forgets it when
    /// nothing is folded.
    pub fn cookie(&self) -> String {
        if self.0.is_empty() {
            return pref_cookie_header(COLLAPSED_DECKS_COOKIE, None);
        }
        let ids: Vec<String> = self.0.iter().map(i64::to_string).collect();
        pref_cookie_header(COLLAPSED_DECKS_COOKIE, Some(&ids.join(".")))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for CollapsedDecks {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        Ok(jar
            .get(COLLAPSED_DECKS_COOKIE)
            .map(|c| Self::parse(c.value()))
            .unwrap_or_default())
    }
}

pub const SHOW_ARCHIVED_COOKIE: &str = "flash-decks-archived";

/// Whether the deck list shows archived decks. Like the theme it is how
/// this browser likes its pages, so it lives in a cookie: it outlasts a
/// refresh and a visit, stays on until it is turned off, and keeps out of
/// the address, which is `/decks` either way.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShowArchived(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for ShowArchived {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        Ok(ShowArchived(
            jar.get(SHOW_ARCHIVED_COOKIE)
                .is_some_and(|c| c.value() == "1"),
        ))
    }
}

/// UI theme, read from a cookie and rendered as `data-theme` on <html>.
/// Server-side so pages never flash the wrong palette.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    Clay,
    Mocha,
    Taupe,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Clay => "clay",
            Theme::Mocha => "mocha",
            Theme::Taupe => "taupe",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Theme> {
        match s {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "clay" => Some(Theme::Clay),
            "mocha" => Some(Theme::Mocha),
            "taupe" => Some(Theme::Taupe),
            _ => None,
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Theme {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        Ok(jar
            .get(THEME_COOKIE)
            .and_then(|c| Theme::from_str(c.value()))
            .unwrap_or(Theme::Dark))
    }
}

/// Extractor: the logged-in user, or a redirect to /login for pages.
pub struct AuthUser {
    pub id: UserId,
    pub is_admin: bool,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match session_user(parts, state).await {
            Some((id, is_admin)) => Ok(AuthUser { id, is_admin }),
            None => Err(login_redirect(parts)),
        }
    }
}

/// Extractor for what changes how an account is reached or leaves it:
/// passwords, deleting the account, other devices, connected apps,
/// sign-in providers, exports, approving an assistant. A
/// session opened by an emailed link is asked to sign in again, the way
/// a stranger is asked to sign in; whoever can read the mail could
/// already reset the password, but should not be one click from these.
pub struct FullUser {
    pub id: UserId,
    pub is_admin: bool,
}

impl FromRequestParts<AppState> for FullUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match session(parts, state).await {
            Some(s) if !s.via_link => Ok(FullUser {
                id: s.user,
                is_admin: s.is_admin,
            }),
            Some(_) => Err(sign_in_again(parts)),
            None => Err(login_redirect(parts)),
        }
    }
}

/// Extractor variant for JSON/htmx endpoints: 401 instead of redirect.
pub struct ApiUser(pub UserId);

impl FromRequestParts<AppState> for ApiUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match session_user(parts, state).await {
            Some((user, _)) => Ok(ApiUser(user)),
            None => Err((StatusCode::UNAUTHORIZED, "not signed in").into_response()),
        }
    }
}

/// Extractor for pages that render either way: the session if there is
/// one, None otherwise. Never rejects.
pub struct OptionalUser(pub Option<(UserId, bool)>);

impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalUser(session_user(parts, state).await))
    }
}

/// Extractor for pages that care how a session was opened (the sign-in
/// page): the session if there is one. Never rejects.
pub struct MaybeSession(pub Option<SessionUser>);

impl FromRequestParts<AppState> for MaybeSession {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeSession(session(parts, state).await))
    }
}

/// Extractor for admin-only surfaces. Signed-out visitors are redirected
/// to /login like any page; signed-in members get a plain 404 so the
/// surface's existence isn't advertised.
pub struct AdminUser(pub UserId);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // No link is ever minted for an admin; a link session asking is
        // sent to sign in all the same.
        match session(parts, state).await {
            Some(s) if s.is_admin && !s.via_link => Ok(AdminUser(s.user)),
            Some(s) if s.is_admin => Err(sign_in_again(parts)),
            Some(_) => Err((StatusCode::NOT_FOUND, "not found").into_response()),
            None => Err(login_redirect(parts)),
        }
    }
}

pub(crate) fn login_redirect(parts: &Parts) -> Response {
    let next = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let to = format!("/login?next={}", urlencode(next));
    Redirect::to(&to).into_response()
}

/// To the sign-in page, saying why: this session came from an emailed
/// link and this page wants a real sign-in. A form post comes back to
/// the page it was posted from, not to the post itself.
fn sign_in_again(parts: &Parts) -> Response {
    let next = if parts.method == axum::http::Method::GET {
        parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str().to_string())
            .unwrap_or_else(|| "/".into())
    } else {
        parts
            .headers
            .get(header::REFERER)
            .and_then(|v| v.to_str().ok())
            .and_then(|r| r.split_once("://").map(|(_, rest)| rest))
            .and_then(|rest| rest.find('/').map(|i| rest[i..].to_string()))
            .filter(|p| crate::web::is_local_path(p))
            .unwrap_or_else(|| "/settings".into())
    };
    let to = format!("/login?again=1&next={}", urlencode(&next));
    Redirect::to(&to).into_response()
}

/// How stale a session's "last used" may get before a request refreshes
/// it. Writing it on every request would put a write behind every page
/// on the one SQLite connection; the devices list needs minutes, not
/// milliseconds.
pub const SESSION_TOUCH_EVERY_MS: i64 = 5 * 60 * 1000;

/// The signed-in browser user behind the session cookie, if any. Shared
/// with the API's cookie-or-bearer extractor.
pub(crate) async fn session_user(parts: &Parts, state: &AppState) -> Option<(UserId, bool)> {
    session(parts, state).await.map(|s| (s.user, s.is_admin))
}

/// Who a session cookie names, and how that session was opened.
#[derive(Debug, Clone, Copy)]
pub struct SessionUser {
    pub user: UserId,
    pub is_admin: bool,
    /// Opened by an emailed sign-in link (`FullUser`).
    pub via_link: bool,
}

/// The session a request's context (`i18n::context`) already looked up:
/// every extractor after it reads this rather than the store.
#[derive(Debug, Clone, Copy)]
pub struct CachedSession(pub Option<SessionUser>);

/// The signed-in account's language and learned time zone, as the
/// session lookup read them.
#[derive(Debug, Clone, Default)]
pub struct AccountLang {
    pub locale: Option<String>,
    /// The person chose it (never replaced by a signal).
    pub chosen: bool,
    pub zone: Option<String>,
}

/// The session behind the request's cookie, if valid.
pub async fn session(parts: &Parts, state: &AppState) -> Option<SessionUser> {
    if let Some(cached) = parts.extensions.get::<CachedSession>() {
        return cached.0;
    }
    lookup_session(parts, state).await.map(|(user, _)| user)
}

/// `session`, read from the store, with the account's language.
pub async fn lookup_session(parts: &Parts, state: &AppState) -> Option<(SessionUser, AccountLang)> {
    let hash = session_hash(&parts.headers)?;
    let services = state.services.clone();
    tokio::task::spawn_blocking(move || {
        let store = services.store();
        let now = now_ms();
        let hit = store.lookup_web_session(&hash, now).ok().flatten()?;
        if hit
            .last_used_at
            .is_none_or(|at| now - at >= SESSION_TOUCH_EVERY_MS)
        {
            // Best effort: a missed write only leaves "last used" older.
            let _ = store.touch_web_session(hit.id, now);
        }
        Some(hit)
    })
    .await
    .ok()
    .flatten()
    .map(|hit| {
        (
            SessionUser {
                user: hit.user,
                is_admin: hit.role == "admin",
                via_link: hit.via_link,
            },
            AccountLang {
                locale: hit.locale,
                chosen: hit.locale_chosen,
                zone: hit.zone,
            },
        )
    })
}

/// SHA-256 of the request's session cookie, if it carries one: how the
/// store knows which session is the browser asking.
pub fn session_hash(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get(SESSION_COOKIE).map(|c| hash_token(c.value()))
}

/// The browser a web sign-in comes from: its name for the devices list,
/// the session it already held, which the new one replaces, so one
/// browser is one row however often it signs in, and the languages it
/// says its person reads, most wanted first.
#[derive(Debug, Clone, Default)]
pub struct Browser {
    label: String,
    previous: Option<String>,
    languages: Vec<String>,
}

impl Browser {
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        Browser {
            label: browser_name(user_agent),
            previous: session_hash(headers),
            languages: accept_languages(headers),
        }
    }

    /// "Chrome on Windows", or empty when the User-Agent says neither.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The languages the browser asks for, normalized, most wanted first
    /// (`flash_core::locale::parse_accept_language`); empty when it names
    /// none.
    pub fn languages(&self) -> &[String] {
        &self.languages
    }
}

/// A request's `Accept-Language` preferences, normalized and capped; the
/// header itself is never kept or logged.
pub fn accept_languages(headers: &HeaderMap) -> Vec<String> {
    headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .map(flash_core::locale::parse_accept_language)
        .unwrap_or_default()
}

/// Keeps what a signed-in browser says about its person's language (at
/// a sign-in, or on the page that connects an assistant).
pub fn note_browser_languages(
    store: &flash_store::Store,
    user: UserId,
    languages: &[String],
    now_ms: i64,
) {
    crate::account_locale::keep(
        store,
        user,
        flash_store::LocaleSource::Browser,
        languages,
        now_ms,
    );
}

/// "Chrome on Windows" from a User-Agent: the browser and the system it
/// runs on, as a person would name them. Only this name is stored, never
/// the User-Agent itself. Empty when neither can be told.
pub fn browser_name(user_agent: &str) -> String {
    let has = |needle: &str| user_agent.contains(needle);
    // Order matters: Edge, Opera and Samsung's browser all claim Chrome,
    // and Chrome claims Safari.
    let browser = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        Some("Edge")
    } else if has("OPR/") || has("Opera") {
        Some("Opera")
    } else if has("SamsungBrowser/") {
        Some("Samsung Internet")
    } else if has("Firefox/") || has("FxiOS/") {
        Some("Firefox")
    } else if has("Chrome/") || has("CriOS/") || has("Chromium/") {
        Some("Chrome")
    } else if has("Safari/") {
        Some("Safari")
    } else {
        None
    };
    // An iPhone or iPad says "like Mac OS X" and an Android says Linux,
    // so they come first.
    let system = if has("iPhone") {
        Some("iPhone")
    } else if has("iPad") {
        Some("iPad")
    } else if has("Android") {
        Some("Android")
    } else if has("CrOS") {
        Some("ChromeOS")
    } else if has("Windows") {
        Some("Windows")
    } else if has("Macintosh") || has("Mac OS X") {
        Some("macOS")
    } else if has("Linux") {
        Some("Linux")
    } else {
        None
    };
    match (browser, system) {
        (Some(b), Some(s)) => format!("{b} on {s}"),
        (Some(b), None) => b.to_string(),
        (None, Some(s)) => format!("Browser on {s}"),
        (None, None) => String::new(),
    }
}

pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Mints a web session for `user` inside a store call and returns the
/// cookie token (the store only ever sees its hash). Pair with
/// [`with_session_cookie`] on the response. Every login path (password,
/// passkey, reset, enrollment, an external identity provider) ends here.
/// The session the browser held before, whoever's it was, ends with it:
/// the new cookie overwrites it anyway, and a row nobody can use again
/// would sit in the devices list.
pub fn issue_session(
    store: &flash_store::Store,
    user: UserId,
    browser: &Browser,
    now_ms: i64,
) -> flash_store::Result<String> {
    if let Some(previous) = &browser.previous {
        store.delete_web_session(previous)?;
    }
    let token = new_token();
    store.create_web_session(
        &hash_token(&token),
        user,
        &browser.label,
        now_ms,
        now_ms + SESSION_TTL_MS,
    )?;
    note_browser_languages(store, user, &browser.languages, now_ms);
    Ok(token)
}

/// `issue_session` for someone who arrived by an emailed sign-in link: a
/// session that can study, and signs in again for what `FullUser`
/// guards. Signing in any other way later replaces it with a full one.
pub fn issue_link_session(
    store: &flash_store::Store,
    user: UserId,
    browser: &Browser,
    now_ms: i64,
) -> flash_store::Result<String> {
    if let Some(previous) = &browser.previous {
        store.delete_web_session(previous)?;
    }
    let token = new_token();
    store.create_link_session(
        &hash_token(&token),
        user,
        &browser.label,
        now_ms,
        now_ms + SESSION_TTL_MS,
    )?;
    note_browser_languages(store, user, &browser.languages, now_ms);
    Ok(token)
}

/// Helper: respond with a session cookie set.
pub fn with_session_cookie(token: &str, response: Response) -> Response {
    let mut response = response;
    if let Ok(value) = cookie_header(SESSION_COOKIE, token, SESSION_TTL_MS / 1000).parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// Cookie *or* bearer, for the two web media routes both surfaces share
/// (upload and serving). A request that presented a bearer gets the JSON
/// 401; a plain browser request keeps the login redirect it always had.
/// It lives here with the cookie extractors, never under `api`, whose
/// handlers must accept bearers only.
#[derive(Debug, Clone, Copy)]
pub struct AnyUser {
    pub id: UserId,
    pub is_admin: bool,
}

impl FromRequestParts<AppState> for AnyUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if crate::api::bearer_token(parts).is_some() {
            return match crate::api::mobile_user(parts, state).await {
                Some(u) => Ok(AnyUser {
                    id: u.id,
                    is_admin: u.is_admin,
                }),
                None => Err(crate::api::ApiError::unauthorized().into_response()),
            };
        }
        match session_user(parts, state).await {
            Some((id, is_admin)) => Ok(AnyUser { id, is_admin }),
            None => Err(login_redirect(parts)),
        }
    }
}
