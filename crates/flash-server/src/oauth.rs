//! OAuth 2.1 authorization server + resource server, hand-rolled.
//! Public clients only (PKCE S256 mandatory), dynamic client registration
//! (RFC 7591), server metadata (RFC 8414), protected-resource metadata
//! (RFC 9728). Tokens are opaque 256-bit values stored as SHA-256:
//! randoms, or for a refresh derived from the token it replaces
//! (`refresh::rotated_pair`).

mod refresh;

pub(crate) use refresh::exchange_refresh;

use crate::i18n::filters;
use std::borrow::Cow;

use askama::Template;
use axum::extract::rejection::FormRejection;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use flash_core::UserId;
use flash_store::CodeConsume;
use serde::{Deserialize, Serialize};

use crate::auth::{hash_token, new_token, pkce_s256, FullUser};
use crate::ext::{AccountEvent, Surface};
use crate::service::now_ms;
use crate::state::AppState;

const CODE_TTL_MS: i64 = 60 * 1000;
/// How long after a code expires the check for it runs, and how long
/// housekeeping keeps an expired code (`CODE_KEPT_MS`), so the check
/// always finds the row it asks about.
const CODE_CHECK_AFTER_MS: i64 = 30 * 1000;
pub(crate) const CODE_KEPT_MS: i64 = 10 * 60 * 1000;

/// The job that notices an authorization code its client never came back
/// for: consent was given, the person was sent back to the client, and
/// the client never exchanged the code. Nothing else sees that happen.
pub const JOB_OAUTH_CODE_CHECK: &str = "oauth_code_check";

/// The `oauth_code_check` job: a code still unused after it expired goes
/// on its account as `connect_failed` (`code_never_exchanged`), beside
/// the consent and device that preceded it.
pub fn oauth_code_check_job(
    state: &AppState,
    job: &flash_store::JobRow,
    _now: i64,
) -> crate::jobs::Outcome {
    use crate::jobs::Outcome;
    let code_hash = serde_json::from_str::<serde_json::Value>(&job.payload)
        .ok()
        .and_then(|v| v.get("code_hash")?.as_str().map(str::to_string));
    let (Some(user), Some(code_hash)) = (job.user, code_hash) else {
        return Outcome::Fail("a code check with no account or code".into());
    };
    match state.services.store().oauth_code_used(user, &code_hash) {
        Ok(Some(false)) => {
            tracing::info!(
                user = user.raw(),
                "oauth code never exchanged by its client"
            );
            crate::web::account_event(
                state,
                user,
                "connect_failed",
                Some("code_never_exchanged"),
                None,
            );
            Outcome::Done
        }
        // Redeemed, or gone: nothing to say.
        Ok(_) => Outcome::Done,
        Err(e) => Outcome::Retry(e.to_string()),
    }
}

/// Token lifetimes, shared with the app's first-party grant flow.
pub(crate) const ACCESS_TTL_MS: i64 = 60 * 60 * 1000;
pub(crate) const REFRESH_TTL_MS: i64 = 60 * 24 * 60 * 60 * 1000;
const CONSENT_TTL_MS: i64 = 10 * 60 * 1000;
const DEFAULT_SCOPE: &str = "flashcards";

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource_metadata),
        )
        // MCP clients may request the path-suffixed form for a resource
        // living at /mcp (RFC 9728 section 3).
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource_metadata),
        )
        .route(
            "/oauth/register",
            post(register).layer(axum::extract::DefaultBodyLimit::max(REGISTER_BODY_LIMIT)),
        )
        .route("/oauth/token", post(token))
}

/// The two endpoints a browser drives: the authorization request, which
/// renders the consent page to the signed-in user, and the consent form
/// itself. They mount in the browser band, so the consent POST sits
/// behind the same-origin guard as well as its single-use ceremony token.
pub fn browser_router() -> Router<AppState> {
    Router::new()
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/consent", post(consent))
}

// ---- metadata ----

async fn authorization_server_metadata(State(state): State<AppState>) -> Response {
    let base = &state.config.base_url;
    Json(serde_json::json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": [DEFAULT_SCOPE],
        // RFC 9207: every authorization response names its issuer
        // (`with_iss`), which protects clients against mix-up attacks.
        "authorization_response_iss_parameter_supported": true,
    }))
    .into_response()
}

/// `to` with `iss` (RFC 9207) appended: the issuer every authorization
/// response names, exactly the metadata's `issuer`, which clients compare
/// as a string.
fn with_iss(mut to: String, base_url: &str) -> String {
    to.push_str(&format!("&iss={}", crate::auth::urlencode(base_url)));
    to
}

/// ChatGPT registers a callback of its own for each connector
/// (`/connector/oauth/{id}`), and once the server names its issuer in
/// authorization responses it sends its shared callback instead, still
/// under the client it registered. A client holding a per-connector
/// ChatGPT callback may therefore use the shared one: both are
/// chatgpt.com's, so no code can be sent anywhere its registration did
/// not already allow.
pub const CHATGPT_CONNECTOR_CALLBACK: &str = "https://chatgpt.com/connector/oauth/";
pub const CHATGPT_SHARED_CALLBACK: &str = "https://chatgpt.com/connector_platform_oauth_redirect";

/// ChatGPT's mobile website lives on its own host and names its callbacks
/// there, under the client registered with the `chatgpt.com` ones; its
/// users were refused until it counted as the same site. Matched as a
/// prefix ending in `/`, so a port, userinfo or a longer host never
/// qualifies.
const CHATGPT_MOBILE_WEB: &str = "https://m.chatgpt.com/";
const CHATGPT_WEB: &str = "https://chatgpt.com/";

/// `uri` as ChatGPT's main site would name it: a mobile-web callback with
/// its host put back to `chatgpt.com`, anything else as it is.
pub fn as_chatgpt_web(uri: &str) -> Cow<'_, str> {
    match uri.strip_prefix(CHATGPT_MOBILE_WEB) {
        Some(rest) => Cow::Owned(format!("{CHATGPT_WEB}{rest}")),
        None => Cow::Borrowed(uri),
    }
}

/// Whether `uri` is one `registered` allows (exactly, or ChatGPT's shared
/// callback for a client registered with one of its own), counting a
/// ChatGPT mobile-web callback as its `chatgpt.com` twin. The code still
/// goes to `uri` as asked, and the token exchange must name it again.
fn redirect_registered(registered: &[String], uri: &str) -> bool {
    let web = as_chatgpt_web(uri);
    registered.iter().any(|u| u == uri || *u == web)
        || (web == CHATGPT_SHARED_CALLBACK
            && registered
                .iter()
                .any(|u| u.starts_with(CHATGPT_CONNECTOR_CALLBACK)))
}

/// The assistants' callback hosts: a refused redirect to one is logged
/// with its path (`log_path`).
pub const KNOWN_HOSTS: [&str; 8] = [
    "chatgpt.com",
    "m.chatgpt.com",
    "claude.ai",
    "grok.com",
    "callback.mistral.ai",
    "oauth-redirect.googleusercontent.com",
    "localhost",
    "127.0.0.1",
];

async fn protected_resource_metadata(State(state): State<AppState>) -> Response {
    let base = &state.config.base_url;
    Json(serde_json::json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
        "scopes_supported": [DEFAULT_SCOPE],
    }))
    .into_response()
}

// ---- dynamic client registration ----

/// The two fields this server acts on. Anything else a client sends
/// (RFC 7591 allows a lot) is accepted and dropped: this endpoint is
/// unauthenticated, so nothing it receives may be stored unbounded.
#[derive(Deserialize)]
struct RegisterRequest {
    #[serde(deserialize_with = "crate::bounds::redirect_uris")]
    redirect_uris: Vec<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_client_name")]
    client_name: Option<String>,
}

/// More than enough for ten redirect URIs and a name; the endpoint is
/// unauthenticated, so the default 2 MiB body limit was a write amplifier.
const REGISTER_BODY_LIMIT: usize = 16 * 1024;

/// A redirect URI a client may register: an absolute `https` URL, or
/// plain `http` to a loopback *host* (local tooling, RFC 8252 §7.3).
/// Parsed as a URL, not matched as a prefix: `http://localhost@evil`
/// has host `evil`, `http://localhost.evil` is not loopback, and a
/// fragment or userinfo has no place in a redirect target.
fn valid_redirect_uri(uri: &str) -> bool {
    let Ok(url) = url::Url::parse(uri) else {
        return false;
    };
    if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if uri.chars().any(|c| c.is_control()) {
        return false;
    }
    match url.scheme() {
        "https" => url.host().is_some(),
        "http" => matches!(
            url.host(),
            Some(url::Host::Domain("localhost"))
                | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
                | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
        ),
        _ => false,
    }
}

/// The page an authorize request gets when it is malformed after the
/// client and redirect URI checked out. Never a redirect: anyone can
/// register a client with any `https` URI, so bouncing an error to the
/// registered URI without a click would make the authorize endpoint an
/// open redirector from this origin. The redirect happens only after
/// the person presses Approve or Deny.
#[derive(Template)]
#[template(path = "oauth_error.html")]
struct OAuthErrorPage {
    /// The OAuth error code, for the person to quote back to the app.
    code: &'static str,
    theme: &'static str,
}

fn authorize_error(code: &'static str, theme: crate::auth::Theme) -> Response {
    let page = OAuthErrorPage {
        code,
        theme: theme.as_str(),
    };
    (
        StatusCode::BAD_REQUEST,
        Html(crate::i18n::render_current(&page).unwrap_or_default()),
    )
        .into_response()
}

async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> Response {
    if req.redirect_uris.is_empty() || req.redirect_uris.len() > 10 {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "1-10 redirect_uris required",
        );
    }
    for uri in &req.redirect_uris {
        if !valid_redirect_uri(uri) || uri.len() > 500 {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_redirect_uri",
                "redirect_uris must be https:// (or loopback http)",
            );
        }
    }
    let client_name = req
        .client_name
        .as_deref()
        .unwrap_or("Unnamed client")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect::<String>();
    let client_name = if client_name.trim().is_empty() {
        "Unnamed client".to_string()
    } else {
        client_name
    };

    let client_id = new_token();
    let now = now_ms();
    let metadata = serde_json::json!({
        "client_name": client_name,
        "redirect_uris": req.redirect_uris,
    })
    .to_string();
    let services = state.services.clone();
    let redirect_uris = req.redirect_uris.clone();
    let stored_id = client_id.clone();
    let stored_name = client_name.clone();
    let result = crate::i18n::blocking(move || {
        services.store().create_oauth_client(
            &stored_id,
            &stored_name,
            &redirect_uris,
            &metadata,
            now,
        )
    })
    .await;
    match result {
        Ok(Ok(())) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "client_id": client_id,
                "client_name": client_name,
                "redirect_uris": req.redirect_uris,
                "token_endpoint_auth_method": "none",
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "client_id_issued_at": now / 1000,
            })),
        )
            .into_response(),
        other => {
            tracing::error!("client registration failed: {other:?}");
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "registration failed",
            )
        }
    }
}

// ---- authorize + consent ----

#[derive(Deserialize, Clone)]
pub struct AuthorizeParams {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub response_type: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    pub client_id: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_redirect_uri")]
    pub redirect_uri: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_oauth_state")]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_code_challenge")]
    pub code_challenge: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub code_challenge_method: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_oauth_scope")]
    pub scope: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_oauth_resource")]
    pub resource: Option<String>,
}

/// Fully-validated authorize request, held server-side while the user
/// looks at the consent screen: a durable ceremony (`CONSENT_KIND`), so a
/// restart while someone reads the screen does not break their connect.
/// The account it is for is parked beside it.
#[derive(Clone, Serialize, Deserialize)]
pub struct PendingConsent {
    pub client_id: String,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: String,
    pub scope: String,
    pub resource: Option<String>,
    /// Once approved: the hash of the code last sent back for it, which
    /// a second tap on the same screen withdraws before sending another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_hash: Option<String>,
}

const CONSENT_KIND: &str = "oauth_consent";

/// What a pending consent becomes once answered, for as long as a code
/// lives: a second tap on the same screen (a phone where the hand-off
/// back to the app takes a moment, so the page still shows) finds it and
/// gets the first tap's answer again, never "consent expired" in place
/// of the redirect it replaced.
const CONSENT_APPROVED: &str = "oauth_consent_approved";
const CONSENT_DENIED: &str = "oauth_consent_denied";

/// A second Approve on a screen whose code the client already redeemed:
/// the connection is made, and a new code would only be a replay away
/// from revoking it.
#[derive(Template)]
#[template(path = "consent_done.html")]
struct ConsentDonePage {
    client_name: String,
    theme: &'static str,
}

#[derive(Template)]
#[template(path = "consent.html")]
struct ConsentPage {
    client_name: String,
    /// Where the authorization code will be sent. Anyone can register a
    /// client under any name, so the name alone is not something to
    /// approve on; the destination host is.
    redirect_host: String,
    /// The extension's own content for a host it knows, in place of the
    /// page's (`Slot::ConsentBody`); empty for every other host.
    body_html: String,
    token: String,
    theme: &'static str,
    /// Loads `static/timezone.js`: an account made from an assistant may
    /// see no other page of the server.
    ask_timezone: bool,
}

/// `host[:port]` of an absolute URL, for display. The URI was validated
/// at registration and matched exactly here, so this is presentation.
/// The parsed host is the ASCII (punycode) form a browser's address bar
/// shows, so a look-alike Unicode host cannot pass for a real one.
fn display_host(uri: &str) -> String {
    let Ok(url) = url::Url::parse(uri) else {
        return String::new();
    };
    let host = url.host_str().unwrap_or("");
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    }
}

/// What a refused authorize request asked for, as far as it got: the
/// name its client registered under, and the redirect URI it sent (not
/// necessarily a registered one). Only the log sees these; the account
/// event keeps the fixed reason.
#[derive(Clone, Copy, Default)]
struct Asked<'a> {
    client: Option<&'a str>,
    redirect_uri: Option<&'a str>,
}

/// The host of a redirect URI for the log, never the rest of it: a
/// refused URI is the requester's own text, so it is parsed, cut short
/// and reduced to its host.
fn log_host(uri: &str) -> String {
    url::Url::parse(uri)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.chars().take(64).collect()))
        .unwrap_or_else(|| "unparsed".to_string())
}

/// The path of a redirect URI for the log, when its host is one of the
/// assistants' (`KNOWN_HOSTS`): a refused callback of theirs is a route
/// this server does not know yet, and the path is what tells it. Never the
/// query, which carries their state; any other host's path stays out.
fn log_path(uri: &str) -> String {
    url::Url::parse(uri)
        .ok()
        .filter(|u| u.host_str().is_some_and(|h| KNOWN_HOSTS.contains(&h)))
        .map(|u| u.path().chars().take(80).collect())
        .unwrap_or_else(|| "-".to_string())
}

/// An authorize request or consent answer turned away. The person is
/// signed in, so the refusal goes on their account as `connect_failed`
/// with the reason, next to the consent views it interrupts. The log line
/// also names the client and the redirect host (and an assistant's
/// redirect path), so a connection that fails for one kind of client can
/// be told apart.
fn refuse(
    state: &AppState,
    user: UserId,
    reason: &'static str,
    asked: Asked,
    response: Response,
) -> Response {
    tracing::info!(
        reason,
        user = user.raw(),
        client = ?asked.client.unwrap_or("-"),
        redirect_host = ?asked.redirect_uri.map(log_host).unwrap_or_else(|| "-".into()),
        redirect_path = ?asked.redirect_uri.map(log_path).unwrap_or_else(|| "-".into()),
        "oauth authorize rejected"
    );
    crate::web::account_event(state, user, "connect_failed", Some(reason), None);
    response
}

async fn authorize(
    State(state): State<AppState>,
    FullUser { id: user, .. }: FullUser,
    theme: crate::auth::Theme,
    headers: HeaderMap,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    let bad = |text: &'static str| (StatusCode::BAD_REQUEST, text).into_response();
    // Connecting an assistant is often an account's first contact: what
    // its browser says about its language is kept here too.
    let languages = crate::auth::accept_languages(&headers);
    if !languages.is_empty() {
        let services = state.services.clone();
        let _ = crate::i18n::blocking(move || {
            crate::auth::note_browser_languages(services.store(), user, &languages, now_ms());
        })
        .await;
    }
    // Before the client is known, the redirect it sent is all that says
    // who asked.
    let asked_by_redirect = Asked {
        client: None,
        redirect_uri: params.redirect_uri.as_deref(),
    };
    // Client + redirect must validate before anything is redirected.
    let Some(client_id) = params.client_id.clone() else {
        return refuse(
            &state,
            user,
            "missing_client_id",
            asked_by_redirect,
            bad("missing client_id"),
        );
    };
    let services = state.services.clone();
    let lookup_id = client_id.clone();
    let client =
        match crate::i18n::blocking(move || services.store().get_oauth_client(&lookup_id)).await {
            Ok(Ok(Some(client))) => client,
            Ok(Ok(None)) => {
                return refuse(
                    &state,
                    user,
                    "unknown_client",
                    asked_by_redirect,
                    bad("unknown client_id"),
                )
            }
            other => {
                tracing::error!("client lookup: {other:?}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
            }
        };
    let Some(redirect_uri) = params.redirect_uri.clone() else {
        return refuse(
            &state,
            user,
            "missing_redirect_uri",
            Asked {
                client: Some(&client.client_name),
                redirect_uri: None,
            },
            bad("missing redirect_uri"),
        );
    };
    let asked = Asked {
        client: Some(&client.client_name),
        redirect_uri: Some(&redirect_uri),
    };
    if !redirect_registered(&client.redirect_uris, &redirect_uri) {
        return refuse(
            &state,
            user,
            "redirect_not_registered",
            asked,
            bad("redirect_uri not registered"),
        );
    }

    // A malformed request is a page, never a redirect: the registered
    // URI is the client's own choice, and the person has not clicked
    // anything yet (see `authorize_error`).
    let page_error = |reason: &'static str, code: &'static str| {
        refuse(&state, user, reason, asked, authorize_error(code, theme))
    };
    if params.response_type.as_deref() != Some("code") {
        return page_error("unsupported_response_type", "unsupported_response_type");
    }
    let Some(code_challenge) = params.code_challenge.clone() else {
        return page_error("missing_code_challenge", "invalid_request");
    };
    if params.code_challenge_method.as_deref() != Some("S256") {
        return page_error("unsupported_challenge_method", "invalid_request");
    }
    // RFC 8707: a resource indicator, when given, must be this server's
    // MCP endpoint; a token for somewhere else is not something to mint.
    // An origin the server moved from still names this server, since a
    // client may keep sending the resource it first discovered.
    if let Some(resource) = params.resource.as_deref() {
        let resource = resource.trim_end_matches('/');
        let ours = std::iter::once(&state.config.base_url)
            .chain(&state.config.previous_base_urls)
            .any(|origin| resource == format!("{origin}/mcp"));
        if !ours {
            return page_error("invalid_target", "invalid_target");
        }
    }
    if params
        .scope
        .as_deref()
        .is_some_and(|s| s.split_whitespace().any(|w| w != DEFAULT_SCOPE))
    {
        return page_error("invalid_scope", "invalid_scope");
    }

    let asking = Some(client.client_name.as_str()).filter(|n| !n.is_empty());
    crate::web::record_event(
        &state,
        user,
        AccountEvent::new("view", Surface::Web)
            .detail(Some("consent"))
            .client(asking),
    );
    state.ext.consent_shown(
        &state,
        user,
        &crate::ext::ConsentShown {
            client: asking,
            headers: &headers,
            redirect_uri: &redirect_uri,
            oauth_state: params.state.as_deref(),
        },
    );
    let consent_token = new_token();
    let redirect_host = display_host(&redirect_uri);
    let pending = PendingConsent {
        client_id,
        redirect_uri,
        state: params.state.clone(),
        code_challenge,
        scope: params.scope.unwrap_or_else(|| DEFAULT_SCOPE.into()),
        resource: params.resource.clone(),
        code_hash: None,
    };
    if !state
        .put_durable_ceremony(
            hash_token(&consent_token),
            CONSENT_KIND,
            Some(user),
            &pending,
            now_ms() + CONSENT_TTL_MS,
        )
        .await
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "couldn't start the connection just now; retry from the app",
        )
            .into_response();
    }
    let services = state.services.clone();
    // A failed read only means the page doesn't ask; consent goes on.
    let ask_timezone = crate::i18n::blocking(move || services.timezone_is_default(user))
        .await
        .is_ok_and(|r| r.unwrap_or(false));
    // Content that fails to render falls back to the core's page.
    let body_html = state
        .ext
        .render(
            &state,
            crate::ext::Slot::ConsentBody {
                redirect_host: &redirect_host,
                token: &consent_token,
            },
        )
        .inspect_err(|e| tracing::warn!("consent body: {e}"))
        .unwrap_or_default();
    let page = ConsentPage {
        client_name: client.client_name,
        redirect_host,
        body_html,
        token: consent_token,
        theme: theme.as_str(),
        ask_timezone,
    };
    let mut response = Html(crate::i18n::render_current(&page).unwrap_or_default()).into_response();
    // No form-action here. Chrome checks it against every hop of the
    // consent POST's redirect chain, and the chain does not end at the
    // callback we validated: ChatGPT's Android app has chatgpt.com answer
    // it with a 302 to `chatgpt://`, and any client's callback may hand
    // off to its own app the same way. With form-action set, Chrome
    // cancelled that hop without a word and the code was never exchanged.
    // The one form here posts to us, and the redirect URI is checked
    // against the client's registration server-side.
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONSENT_CSP),
    );
    response
}

/// The consent page's policy (no `form-action`, above). It reports what
/// it blocks as every page does: it ends with `state::CSP_REPORTING`.
pub const CONSENT_CSP: &str = "default-src 'self'; script-src 'self'; \
     style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; \
     report-uri /client-errors/csp; report-to csp";

#[derive(Deserialize)]
struct ConsentForm {
    #[serde(deserialize_with = "crate::bounds::token")]
    token: String,
    #[serde(deserialize_with = "crate::bounds::keyword")]
    decision: String,
}

/// What an approval sends back: a code, or (on a repeat whose code was
/// redeemed) the client's name for the page saying it is connected.
enum Approved {
    Code(String),
    Connected(String),
}

async fn consent(
    State(state): State<AppState>,
    FullUser { id: user, .. }: FullUser,
    theme: crate::auth::Theme,
    Form(form): Form<ConsentForm>,
) -> Response {
    let key = hash_token(&form.token);
    let answer = if form.decision == "approve" {
        CONSENT_APPROVED
    } else {
        CONSENT_DENIED
    };
    let now = now_ms();
    let services = state.services.clone();
    let lookup_key = key.clone();
    // The first answer claims the screen; a later one finds it answered
    // and gets that first answer, whatever it says itself.
    let found = crate::i18n::blocking(move || {
        let store = services.store();
        let claimed = store.claim_durable_ceremony(
            &lookup_key,
            CONSENT_KIND,
            answer,
            now + CODE_TTL_MS,
            now,
        )?;
        if let Some((payload, owner)) = claimed {
            return Ok(Some((true, answer.to_string(), payload, owner)));
        }
        Ok::<_, flash_store::StoreError>(
            store
                .answered_ceremony(&lookup_key, &[CONSENT_APPROVED, CONSENT_DENIED], now)?
                .map(|(kind, payload, owner)| (false, kind, payload, owner)),
        )
    })
    .await;
    let found = match found {
        Ok(Ok(found)) => found,
        other => {
            tracing::error!("consent lookup failed: {other:?}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };
    let Some((first, answered, pending, owner)) =
        found.and_then(|(first, kind, payload, owner)| {
            let pending = serde_json::from_str::<PendingConsent>(&payload).ok()?;
            Some((first, kind, pending, owner))
        })
    else {
        return refuse(
            &state,
            user,
            "consent_expired",
            Asked::default(),
            (
                StatusCode::BAD_REQUEST,
                "consent expired; retry from the app",
            )
                .into_response(),
        );
    };
    // The consent must be completed by the same signed-in user who saw it.
    if owner != Some(user) {
        return refuse(
            &state,
            user,
            "session_changed",
            Asked::default(),
            (StatusCode::FORBIDDEN, "session changed; retry").into_response(),
        );
    }
    let sep = if pending.redirect_uri.contains('?') {
        '&'
    } else {
        '?'
    };
    if answered != CONSENT_APPROVED {
        if first {
            crate::web::account_event(&state, user, "consent", Some("deny"), None);
        }
        let mut to = format!("{}{sep}error=access_denied", pending.redirect_uri);
        if let Some(s) = &pending.state {
            to.push_str(&format!("&state={}", crate::auth::urlencode(s)));
        }
        return Redirect::to(&with_iss(to, &state.config.base_url)).into_response();
    }

    // Every approval gets a fresh code. A repeat withdraws the one before
    // it, so only the newest works, and never resends it: if both
    // redirects reached the client, a code redeemed twice would revoke
    // the connection it made.
    let services = state.services.clone();
    let p = pending.clone();
    let result = crate::i18n::blocking(move || {
        let store = services.store();
        if let Some(earlier) = &p.code_hash {
            if !store.retire_oauth_code(user, earlier)?
                && store.oauth_code_used(user, earlier)? == Some(true)
            {
                let client_name = store
                    .get_oauth_client(&p.client_id)?
                    .map(|c| c.client_name)
                    .unwrap_or_else(|| "The app".to_string());
                return Ok(Approved::Connected(client_name));
            }
        }
        let code = new_token();
        let code_hash = hash_token(&code);
        store.create_oauth_code(
            &code_hash,
            &p.client_id,
            user,
            &p.redirect_uri,
            &p.code_challenge,
            &p.scope,
            p.resource.as_deref(),
            now + CODE_TTL_MS,
        )?;
        // Checked once it expires: a client that never comes back for
        // it is otherwise invisible.
        let dedupe = format!("{JOB_OAUTH_CODE_CHECK}:{code_hash}");
        store.enqueue_job(
            &flash_store::NewJob {
                kind: JOB_OAUTH_CODE_CHECK,
                dedupe_key: Some(&dedupe),
                user: Some(user),
                payload: &serde_json::json!({ "code_hash": &code_hash }).to_string(),
                run_at: now + CODE_TTL_MS + CODE_CHECK_AFTER_MS,
            },
            now,
        )?;
        store.update_durable_ceremony(&key, CONSENT_APPROVED, now, |payload| {
            let mut answered: PendingConsent = serde_json::from_str(payload).ok()?;
            answered.code_hash = Some(code_hash);
            serde_json::to_string(&answered).ok()
        })?;
        Ok::<_, flash_store::StoreError>(Approved::Code(code))
    })
    .await;
    let code = match result {
        Ok(Ok(Approved::Code(code))) => code,
        Ok(Ok(Approved::Connected(client_name))) => {
            let page = ConsentDonePage {
                client_name,
                theme: theme.as_str(),
            };
            return Html(crate::i18n::render_current(&page).unwrap_or_default()).into_response();
        }
        other => {
            tracing::error!("create code failed: {:?}", other.map(|r| r.map(|_| ())));
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };
    let detail = if first { "approve" } else { "repeat" };
    crate::web::account_event(&state, user, "consent", Some(detail), None);
    let mut to = format!(
        "{}{sep}code={}",
        pending.redirect_uri,
        crate::auth::urlencode(&code)
    );
    if let Some(s) = &pending.state {
        to.push_str(&format!("&state={}", crate::auth::urlencode(s)));
    }
    Redirect::to(&with_iss(to, &state.config.base_url)).into_response()
}

// ---- token ----

#[derive(Deserialize)]
struct TokenForm {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    grant_type: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    code: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_redirect_uri")]
    redirect_uri: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    client_id: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_code_verifier")]
    code_verifier: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    refresh_token: Option<String>,
}

fn oauth_error(status: StatusCode, code: &str, description: &str) -> Response {
    let mut response = (
        status,
        Json(serde_json::json!({"error": code, "error_description": description})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn token_response(access: &str, refresh: &str, scope: &str, expires_in: i64) -> Response {
    let mut response = Json(serde_json::json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": refresh,
        "scope": scope,
    }))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// A token request turned away: one log line naming why, and, when the
/// code named an account, a `connect_failed` event on it. The client sees
/// only the OAuth error; the reason is for whoever wonders later why a
/// connection never finished.
fn token_rejected(
    state: &AppState,
    reason: &'static str,
    user: Option<UserId>,
    client_id: Option<&str>,
) {
    tracing::warn!(
        reason,
        client_id = ?client_id,
        user = ?user.map(|u| u.raw()),
        "oauth token rejected"
    );
    // Surface `web` like every other connect_failed: the failure is the
    // connect's, wherever in it the client tripped.
    if let Some(user) = user {
        crate::web::account_event(state, user, "connect_failed", Some(reason), None);
    }
}

async fn token(
    State(state): State<AppState>,
    form: Result<Form<TokenForm>, FormRejection>,
) -> Response {
    let form = match form {
        Ok(Form(form)) => form,
        Err(rejection) => {
            // The rejection's own text can quote what was sent, so only
            // its status goes in the log; the client gets it unchanged.
            tracing::warn!(
                status = rejection.status().as_u16(),
                "oauth token rejected: unreadable form"
            );
            return rejection.into_response();
        }
    };
    match form.grant_type.as_deref() {
        Some("authorization_code") => {
            let (Some(code), Some(client_id), Some(verifier)) =
                (form.code, form.client_id.clone(), form.code_verifier)
            else {
                token_rejected(&state, "missing_fields", None, form.client_id.as_deref());
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "missing fields");
            };
            let asked_by = client_id.clone();
            let services = state.services.clone();
            let now = now_ms();
            let code_hash = hash_token(&code);
            let access = new_token();
            let refresh = new_token();
            let access_hash = hash_token(&access);
            let refresh_hash = hash_token(&refresh);
            let redirect_uri = form.redirect_uri;
            // On success: the scope, plus who authorised which client so
            // the connection can be counted per account.
            // A refusal carries why, and whose code it was when the code
            // named anyone. Every refusal is `invalid_grant` to the client.
            type Issued = (String, UserId, String);
            type Refused = (&'static str, Option<UserId>);
            let result = crate::i18n::blocking(
                move || -> Result<Result<Issued, Refused>, flash_store::StoreError> {
                    match services.store().consume_oauth_code(&code_hash, now)? {
                        CodeConsume::Invalid => Ok(Err(("unknown_code", None))),
                        CodeConsume::Expired { user_id, .. } => {
                            Ok(Err(("expired_code", Some(user_id))))
                        }
                        CodeConsume::Replayed { client_id, user_id } => {
                            // OAuth 2.1: code replay revokes the whole grant.
                            services.store().revoke_grant(&client_id, user_id, now)?;
                            tracing::warn!("authorization code replay detected; grant revoked");
                            Ok(Err(("replayed", Some(user_id))))
                        }
                        CodeConsume::Fresh(grant) => {
                            let user = Some(grant.user_id);
                            if grant.client_id != client_id {
                                return Ok(Err(("client_mismatch", user)));
                            }
                            if redirect_uri.as_deref() != Some(grant.redirect_uri.as_str()) {
                                return Ok(Err(("redirect_mismatch", user)));
                            }
                            if pkce_s256(&verifier) != grant.code_challenge {
                                return Ok(Err(("pkce_mismatch", user)));
                            }
                            services.store().insert_oauth_token(
                                &access_hash,
                                &refresh_hash,
                                &grant.client_id,
                                grant.user_id,
                                &grant.scope,
                                "",
                                None,
                                now + ACCESS_TTL_MS,
                                now + REFRESH_TTL_MS,
                                now,
                            )?;
                            let client_name = services
                                .store()
                                .get_oauth_client(&grant.client_id)?
                                .map(|c| c.client_name)
                                .unwrap_or_default();
                            Ok(Ok((grant.scope, grant.user_id, client_name)))
                        }
                    }
                },
            )
            .await;
            match result {
                Ok(Ok(Ok((scope, user, client_name)))) => {
                    state.ext.account_event(
                        &state.services,
                        user,
                        AccountEvent::new("connector_authorized", Surface::Mcp)
                            .client(Some(client_name.as_str()).filter(|n| !n.is_empty())),
                    );
                    token_response(&access, &refresh, &scope, ACCESS_TTL_MS / 1000)
                }
                Ok(Ok(Err((reason, user)))) => {
                    token_rejected(&state, reason, user, Some(&asked_by));
                    oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", "grant rejected")
                }
                other => {
                    tracing::error!("token exchange: {other:?}");
                    oauth_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "server_error",
                        "internal",
                    )
                }
            }
        }
        Some("refresh_token") => {
            let Some(refresh_token) = form.refresh_token else {
                token_rejected(&state, "missing_fields", None, form.client_id.as_deref());
                return oauth_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "missing refresh_token",
                );
            };
            let services = state.services.clone();
            let result = crate::i18n::blocking(move || {
                exchange_refresh(services.store(), &refresh_token, None, now_ms())
            })
            .await;
            match result {
                Ok(Ok(Some(r))) => {
                    token_response(&r.access, &r.refresh, &r.grant.scope, r.expires_in)
                }
                Ok(Ok(None)) => {
                    token_rejected(&state, "bad_refresh", None, form.client_id.as_deref());
                    oauth_error(
                        StatusCode::BAD_REQUEST,
                        "invalid_grant",
                        "refresh token invalid",
                    )
                }
                other => {
                    tracing::error!("refresh: {:?}", other.map(|r| r.map(|_| ())));
                    oauth_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "server_error",
                        "internal",
                    )
                }
            }
        }
        _ => {
            token_rejected(&state, "unsupported_grant", None, form.client_id.as_deref());
            oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "use authorization_code or refresh_token",
            )
        }
    }
}

// ---- resource-server middleware for /mcp ----

/// The registered name of the client behind a bearer request ("Claude",
/// "ChatGPT", …), alongside the `UserId` in request extensions.
#[derive(Debug, Clone)]
pub struct ConnectorClient(pub String);

/// Stable OAuth identity, unlike a display name or transport session.
#[derive(Debug, Clone)]
pub struct ConnectorClientId(pub String);

/// Validates `Authorization: Bearer` and injects UserId (and the client's
/// name) into request extensions (rmcp forwards them to tool handlers).
/// 401s carry the resource-metadata pointer that MCP clients use to
/// discover OAuth.
pub async fn require_bearer(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let info = match token {
        Some(token) => {
            let hash = hash_token(&token);
            let services = state.services.clone();
            crate::i18n::blocking(move || services.store().lookup_api_token(&hash, now_ms()))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                // A token minted for the first-party app drives the API,
                // not MCP; the two grants are different consents.
                .filter(|info| info.client_id != crate::api::MOBILE_CLIENT_ID)
        }
        None => None,
    };
    match info {
        Some(info) => {
            state.ext.account_event(
                &state.services,
                info.user,
                AccountEvent::new("connector_session", Surface::Mcp)
                    .client(Some(info.client_name.as_str()).filter(|name| !name.is_empty())),
            );
            request.extensions_mut().insert(info.user);
            request
                .extensions_mut()
                .insert(ConnectorClientId(info.client_id));
            request
                .extensions_mut()
                .insert(ConnectorClient(info.client_name));
            next.run(request).await
        }
        None => {
            let metadata_url = format!(
                "{}/.well-known/oauth-protected-resource",
                state.config.base_url
            );
            let mut response =
                (StatusCode::UNAUTHORIZED, "authentication required").into_response();
            if let Ok(value) =
                format!("Bearer resource_metadata=\"{metadata_url}\"").parse::<HeaderValue>()
            {
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, value);
            }
            response
        }
    }
}
