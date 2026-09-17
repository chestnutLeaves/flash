//! The mobile app's JSON API under /api/v1. Handlers are thin: one
//! extractor, one Services call inside spawn_blocking, one DTO, the
//! same discipline as the web and MCP surfaces, so no business rule
//! lives here. Mounted outside the same-origin guard (native clients
//! send no Origin) and authenticated with bearer tokens minted by the
//! first-party grant flow, which stores them in the same `oauth_tokens`
//! table the MCP authorization server uses.

pub mod admin;
pub mod ai;
pub mod auth;
pub mod cards;
pub mod decks;
pub mod dto;
pub mod import;
pub mod media;
pub mod settings;
pub mod stats;
pub mod study;
pub mod today;
pub mod voice;

use axum::extract::{FromRequest, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use flash_core::UserId;
use flash_store::ApiTokenInfo;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::auth::hash_token;
use crate::middleware;
use crate::service::{now_ms, ServiceError, Services};
use crate::state::AppState;

/// The reserved client id of the first-party app. Dynamic registration
/// mints 256-bit random ids, so this name cannot collide with an MCP
/// client, and only tokens it minted reach account-management routes.
pub const MOBILE_CLIENT_ID: &str = "flash-mobile";
pub const MOBILE_SCOPE: &str = "mobile";
/// The oldest app build the API still speaks to; bump on a breaking
/// change and the app asks for an update. 1.0.1: the plan's usage meter
/// became the generation quota (`/usage` is gone).
pub const MIN_APP_VERSION: &str = "1.0.1";

pub fn router(state: &AppState) -> Router<AppState> {
    let api_limit = axum::middleware::from_fn_with_state(state.clone(), middleware::api_rate_limit);
    let v1 = Router::new()
        .route("/meta", get(meta))
        .merge(auth::router(state))
        .merge(today::router())
        .merge(decks::router())
        .merge(cards::router())
        .merge(study::router())
        .merge(voice::router())
        .merge(settings::router())
        .merge(stats::router())
        .merge(import::router())
        .merge(admin::router())
        .merge(media::router())
        .merge(ai::router())
        .merge(state.ext.api_routes(state))
        .fallback(not_found)
        .layer(api_limit)
        .layer(axum::middleware::from_fn(no_store));
    Router::new().nest("/api/v1", v1)
}

/// Every API reply is per-user and live: never cache it (tokens in
/// particular must not land in a shared cache).
async fn no_store(request: Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

// ---- errors ----

/// The one error shape the app sees: `{"error": {"code", "message", …}}`.
/// `code` is stable and machine-readable; `message` is for logs and
/// fallback copy; extra fields (`current`/`cap` on `over_cap`, `what` on
/// `not_found`) ride alongside.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            extra: serde_json::Map::new(),
        }
    }

    /// Attaches a structured detail next to `code` and `message`.
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.extra.insert(key.to_string(), v);
        }
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid", message)
    }

    /// Missing, expired, or foreign bearer token.
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "sign in required")
    }

    /// Wrong email/password or a failed identity exchange, deliberately
    /// uniform so nothing leaks about which part was wrong.
    pub fn invalid_credentials() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "those details didn't match",
        )
    }

    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "not allowed")
    }

    pub fn not_found(what: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("not found: {what}"),
        )
        .with("what", what)
    }

    /// The feature isn't configured on this server (signup, Google,
    /// Apple, billing, push): the app hides the affordance.
    pub fn not_available(what: &'static str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_available",
            format!("{what} is not available on this server"),
        )
        .with("what", what)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn payload_too_large() -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "that upload is too large",
        )
    }

    pub fn rate_limited() -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "slow down")
    }
}

/// A heavy job that could not start: the app shows the message and the
/// user tries again in a moment.
impl From<crate::state::HeavyBusy> for ApiError {
    fn from(busy: crate::state::HeavyBusy) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "busy", busy.message())
    }
}

impl ApiError {
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "internal error",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = serde_json::Map::new();
        error.insert("code".into(), self.code.into());
        error.insert("message".into(), self.message.into());
        error.extend(self.extra);
        let mut response =
            (self.status, Json(serde_json::json!({ "error": error }))).into_response();
        let h = response.headers_mut();
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if self.status == StatusCode::UNAUTHORIZED {
            h.insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Invalid(msg) => ApiError::invalid(msg),
            ServiceError::Store(flash_store::StoreError::NotFound(what)) => {
                ApiError::not_found(what)
            }
            ServiceError::Store(flash_store::StoreError::Invalid(msg)) => {
                ApiError::conflict("conflict", msg)
            }
            ServiceError::Store(e) if e.is_constraint() => {
                ApiError::conflict("conflict", "that already exists")
            }
            ServiceError::OverCap { current, cap, .. } => {
                ApiError::new(StatusCode::FORBIDDEN, "over_cap", err.to_string())
                    .with("current", current)
                    .with("cap", cap)
            }
            // The policy's own code rides as the envelope's, so the app
            // branches on `pro_required` or `monthly_limit` directly.
            ServiceError::AiDenied { code, message } => {
                ApiError::new(StatusCode::FORBIDDEN, code, message)
            }
            other => {
                tracing::error!("api service error: {other:?}");
                ApiError::internal()
            }
        }
    }
}

/// No model on this server: the same answer every AI route gives, in
/// the envelope, so the app hides the affordance.
impl From<crate::state::AiOff> for ApiError {
    fn from(_: crate::state::AiOff) -> Self {
        ApiError::not_available("card generation")
    }
}

/// A flow's failure: the person's sentence as 400, ours as a logged 500.
impl From<crate::flows::Failure> for ApiError {
    fn from(failure: crate::flows::Failure) -> Self {
        match failure {
            crate::flows::Failure::User(msg) => ApiError::invalid(msg),
            crate::flows::Failure::Internal(detail) => {
                tracing::error!("api flow failure: {detail}");
                ApiError::internal()
            }
        }
    }
}

impl From<flash_store::StoreError> for ApiError {
    fn from(err: flash_store::StoreError) -> Self {
        ServiceError::from(err).into()
    }
}

/// Runs a blocking call against the store and maps its error into the
/// envelope. The API twin of `web::act`.
pub async fn call<T, E, F>(services: Services, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    E: Into<ApiError> + Send + 'static,
    F: FnOnce(Services) -> Result<T, E> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || f(services)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(err.into()),
        Err(e) => {
            tracing::error!("join: {e}");
            Err(ApiError::internal())
        }
    }
}

/// Handlers return this: a JSON body or the envelope.
pub type ApiResult<T> = Result<Json<T>, ApiError>;

/// A page of a list endpoint.
#[derive(Debug, Serialize)]
pub struct Page<T: Serialize> {
    pub items: Vec<T>,
    pub page: u32,
    pub pages: u32,
    pub total: u32,
}

/// `axum::Json` whose rejection speaks the envelope instead of plain text.
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            // A body past its route's limit is not malformed, it is too
            // big, and the app says so differently (it did not send
            // anything wrong, it sent too much).
            Err(rejection) if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                Err(ApiError::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "payload_too_large",
                    rejection.body_text(),
                ))
            }
            Err(rejection) => Err(ApiError::invalid(rejection.body_text())),
        }
    }
}

/// The platform behind an app request, from the `X-Client` header the
/// app sends (`flash-ios/1.2.0 (42)`): `ios` or `android`, None for
/// anything else. Inspected, never stored as-is.
pub fn app_platform(headers: &axum::http::HeaderMap) -> Option<&'static str> {
    let value = headers.get("x-client")?.to_str().ok()?;
    let name = value.split(['/', ' ']).next()?;
    match name {
        "flash-ios" => Some("ios"),
        "flash-android" => Some("android"),
        _ => None,
    }
}

// ---- bearer extractors ----

pub(crate) fn bearer_token(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
}

async fn token_info(parts: &Parts, state: &AppState) -> Option<ApiTokenInfo> {
    let token = bearer_token(parts)?;
    let hash = hash_token(&token);
    let services = state.services.clone();
    tokio::task::spawn_blocking(move || {
        services
            .store()
            .lookup_api_token(&hash, now_ms())
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}

/// Only a token minted for the app counts: MCP grants (scope
/// `flashcards`) must never reach account management.
pub(crate) async fn mobile_user(parts: &Parts, state: &AppState) -> Option<BearerUser> {
    token_info(parts, state)
        .await
        .filter(|t| t.client_id == MOBILE_CLIENT_ID)
        .map(|t| BearerUser {
            id: t.user,
            is_admin: t.role == "admin",
            token_id: t.id,
        })
}

/// The app user behind a valid first-party bearer token.
#[derive(Debug, Clone, Copy)]
pub struct BearerUser {
    pub id: UserId,
    pub is_admin: bool,
    /// The grant itself, so "sign out other devices" can keep this one.
    pub token_id: i64,
}

impl FromRequestParts<AppState> for BearerUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        mobile_user(parts, state)
            .await
            .ok_or_else(ApiError::unauthorized)
    }
}

/// Admin-only routes. Unlike the web (which 404s to hide the surface),
/// the app already knows the user's role and gets an honest 403.
#[derive(Debug, Clone, Copy)]
pub struct BearerAdmin(pub UserId);

impl FromRequestParts<AppState> for BearerAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match mobile_user(parts, state).await {
            Some(u) if u.is_admin => Ok(BearerAdmin(u.id)),
            Some(_) => Err(ApiError::forbidden()),
            None => Err(ApiError::unauthorized()),
        }
    }
}

/// Public routes that render differently for a signed-in viewer (share
/// pages, community browse). Never rejects.
#[derive(Debug, Clone, Copy)]
pub struct OptionalBearer(pub Option<BearerUser>);

impl FromRequestParts<AppState> for OptionalBearer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalBearer(mobile_user(parts, state).await))
    }
}

// ---- meta ----

/// What this server offers, so the app shows only affordances that work
/// here. `features` seeds every key the core knows as off and lets the
/// extension turn its own on; `extra` carries the extension's own
/// top-level fields (store product ids and the like).
#[derive(Serialize)]
struct Meta {
    version: &'static str,
    min_app_version: &'static str,
    features: crate::ext::JsonFields,
    /// What card generation accepts here; null when it is off.
    ai: Option<AiMeta>,
    /// The natural voice this server reads with; null when it has none.
    voice: Option<VoiceMeta>,
    #[serde(flatten)]
    extra: crate::ext::JsonFields,
}

/// What the app needs to play the server's clips: the voice an account
/// starts with, how long a side may be to get one, the envelope's rate,
/// and the phrase keys it may ask for.
#[derive(Serialize)]
struct VoiceMeta {
    default_voice: String,
    max_chars: usize,
    envelope_hz: u32,
    phrases: Vec<&'static str>,
}

/// The inputs this server reads and the caps a source must fit, so the
/// app can hide a kind and say why a file is too big before uploading.
/// `caps` is the whole picture (types, limits, sentences); the flat
/// fields beside it are what older apps read.
#[derive(Serialize)]
struct AiMeta {
    inputs: Vec<&'static str>,
    max_words: u32,
    max_images: u32,
    max_pages: u32,
    max_media_minutes: u32,
    caps: crate::ai::caps::Caps,
    /// What to send the grader for a spoken answer: `transcript` (the
    /// device's own recognition) or `audio` (the recording itself).
    voice_input: &'static str,
}

async fn meta(State(state): State<AppState>) -> Json<Meta> {
    let mut features = crate::ext::JsonFields::new();
    for (key, on) in [
        ("signup", false),
        ("google", false),
        ("apple", false),
        ("passkeys", true),
        ("billing", false),
        ("push", false),
        ("community", false),
        ("ai", state.ai.is_some()),
        ("voice", state.tts.is_some()),
    ] {
        features.insert(key.to_string(), on.into());
    }
    features.extend(state.ext.features(&state));
    let ai = state.ai.as_ref().map(|ai| AiMeta {
        inputs: ai.inputs.iter().map(|i| i.as_str()).collect(),
        max_words: crate::bounds::AI_MAX_WORDS,
        max_images: crate::bounds::AI_MAX_IMAGES,
        max_pages: crate::bounds::AI_MAX_PAGES,
        max_media_minutes: crate::bounds::AI_MAX_MEDIA_SECONDS / 60,
        caps: crate::ai::caps::Caps::of(ai, &[]),
        voice_input: ai.voice_input.as_str(),
    });
    let voice = state.tts.as_ref().map(|tts| VoiceMeta {
        default_voice: tts.default_voice.clone(),
        max_chars: crate::tts::MAX_CLIP_CHARS,
        envelope_hz: crate::tts::ENVELOPE_HZ,
        phrases: crate::tts::phrase_keys(),
    });
    Json(Meta {
        version: env!("CARGO_PKG_VERSION"),
        min_app_version: MIN_APP_VERSION,
        features,
        ai,
        voice,
        extra: state.ext.meta_extra(&state),
    })
}

async fn not_found() -> ApiError {
    ApiError::not_found("route")
}
