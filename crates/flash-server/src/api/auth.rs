//! Signing in from the app. Every path ends in `issue_tokens`, the
//! mobile twin of `auth::issue_session`: a 1 h access token plus a 60 d
//! rotating refresh token, stored (hashed) in `oauth_tokens` under the
//! first-party client id. Credentials are exchanged natively (a password,
//! or an external identity provider's token) and resolved by the same
//! `flows` the web uses, so the rules never fork.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use flash_core::UserId;
use flash_store::Store;
use serde::{Deserialize, Serialize};

use super::dto::{Me, TokenPair};
use super::{call, ApiError, ApiJson, ApiResult, BearerUser, MOBILE_CLIENT_ID, MOBILE_SCOPE};
use crate::auth::{hash_token, new_token};
use crate::ext::{AccountEvent, AccountView, Surface};
use crate::flows::account::KeepSession;
use crate::flows::devices::{self, DeviceKind};
use crate::flows::{enroll, login, reset};
use crate::middleware;
use crate::oauth::{exchange_refresh, ACCESS_TTL_MS, REFRESH_TTL_MS};
use crate::password;
use crate::service::{now_ms, Services};
use crate::state::AppState;
use crate::webauthn::{self, normalize_email, CeremonyError, EnrollError};

pub fn router(state: &AppState) -> Router<AppState> {
    let strict =
        axum::middleware::from_fn_with_state(state.clone(), middleware::api_auth_rate_limit);
    Router::new()
        // Credential exchanges: the strict per-IP+path budget.
        .route("/auth/password", post(password_login))
        .route("/auth/refresh", post(refresh))
        // Onboarding without the website: mail loops and invite links.
        .route("/auth/reset", post(reset_request))
        .route("/auth/reset/{token}", post(reset_confirm))
        .route("/auth/enroll/{token}", get(enroll_info))
        .route("/auth/enroll/{token}/password", post(enroll_password))
        .route(
            "/auth/enroll/{token}/passkey/start",
            post(enroll_passkey_start),
        )
        .route(
            "/auth/enroll/{token}/passkey/finish",
            post(enroll_passkey_finish),
        )
        .route("/auth/passkey/login/start", post(passkey_login_start))
        .route("/auth/passkey/login/finish", post(passkey_login_finish))
        .layer(strict)
        // Signed-in grant management.
        .route("/auth/logout", post(logout))
        .route("/auth/sessions", get(sessions))
        .route("/auth/sessions/revoke_others", post(revoke_others))
        .route("/auth/sessions/{kind}/{id}", delete(revoke_session))
        .route("/me", get(me))
}

/// The phone a sign-in comes from, as its request names it: a display
/// name for the devices list, and the id the app keeps for its install.
/// A sign-in carrying an install id replaces that install's earlier
/// grant, so one phone is one row however often it signs in.
pub struct AppDevice {
    label: String,
    install_id: Option<String>,
}

impl AppDevice {
    pub fn new(device_label: &str, install_id: Option<String>) -> Self {
        AppDevice {
            label: clean_label(device_label),
            install_id: install_id
                .map(|id| id.trim().to_string())
                .filter(|id| !id.is_empty()),
        }
    }
}

/// Mints the app's access/refresh pair for `user`; only hashes reach
/// the store. The device names the grant in the devices list.
pub(crate) fn issue_tokens(
    store: &Store,
    user: UserId,
    device: &AppDevice,
    now_ms: i64,
) -> flash_store::Result<(String, String)> {
    if let Some(install) = &device.install_id {
        store.revoke_install_tokens(user, MOBILE_CLIENT_ID, install, now_ms)?;
    }
    let access = new_token();
    let refresh = new_token();
    store.insert_oauth_token(
        &hash_token(&access),
        &hash_token(&refresh),
        MOBILE_CLIENT_ID,
        user,
        MOBILE_SCOPE,
        &device.label,
        device.install_id.as_deref(),
        now_ms + ACCESS_TTL_MS,
        now_ms + REFRESH_TTL_MS,
        now_ms,
    )?;
    Ok((access, refresh))
}

/// Mints the app's tokens and the `Me` they come with; runs on a
/// blocking thread. Public for the extension's own sign-in routes.
pub fn signed_in(
    app: &AppState,
    user: UserId,
    device: &AppDevice,
    now_ms: i64,
) -> Result<TokenPair, ApiError> {
    let s = &app.services;
    let (access, refresh) = issue_tokens(s.store(), user, device, now_ms)?;
    Ok(TokenPair::new(
        access,
        refresh,
        ACCESS_TTL_MS / 1000,
        me_of(app, s, user)?,
    ))
}

/// The one way to build `Me`: the account plus the extension's fields.
/// Sign-in, refresh and `/me` all answer with it, so the app sees one
/// shape; runs on a blocking thread.
fn me_of(app: &AppState, s: &Services, user: UserId) -> Result<Me, ApiError> {
    let mut me: Me = s.account_overview(user)?.into();
    me.language = super::settings::chosen_language(s.store(), user, app.i18n.shipped())?;
    me.extra = app.ext.account_json(app, s, user, AccountView::Me)?;
    Ok(me)
}

/// Device names are display-only; keep them short and trimmed.
fn clean_label(raw: &str) -> String {
    raw.trim().chars().take(80).collect()
}

// ---- password ----

#[derive(Deserialize)]
struct PasswordBody {
    #[serde(deserialize_with = "crate::bounds::email")]
    email: String,
    #[serde(deserialize_with = "crate::bounds::password")]
    password: String,
    #[serde(default, deserialize_with = "crate::bounds::device_label")]
    device_label: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_opaque")]
    install_id: Option<String>,
}

async fn password_login(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PasswordBody>,
) -> ApiResult<TokenPair> {
    let Ok(email) = normalize_email(&body.email) else {
        return Err(ApiError::invalid_credentials());
    };
    let login = {
        let email = email.clone();
        call(state.services.clone(), move |s| {
            s.store().user_by_email(&email)
        })
        .await?
    };
    let now = now_ms();

    // Unknown email, no password, or locked out: burn a dummy verify
    // where a real one would have run, then answer uniformly.
    let internal = |_| ApiError::internal();
    let Some(login) = login else {
        let _ = password::verify_blocking(&state, password::dummy_phc().to_string(), body.password)
            .await;
        return Err(ApiError::invalid_credentials());
    };
    let Some(phc) = login.password_hash.clone() else {
        let _ = password::verify_blocking(&state, password::dummy_phc().to_string(), body.password)
            .await;
        return Err(ApiError::invalid_credentials());
    };
    if login::is_locked(&login, now) {
        return Err(ApiError::invalid_credentials());
    }

    let outcome = password::verify_blocking(&state, phc, body.password.clone())
        .await
        .map_err(internal)?;
    let user = login.user;
    if !outcome.ok {
        call(state.services.clone(), move |s| {
            s.store().record_password_failure(user, now)
        })
        .await?;
        return Err(ApiError::invalid_credentials());
    }

    // On error the login still succeeds; rehash next time.
    let rehashed = if outcome.needs_rehash {
        password::hash_blocking(&state, body.password).await.ok()
    } else {
        None
    };
    let device = AppDevice::new(&body.device_label, body.install_id);
    let app = state.clone();
    let pair = call(state.services.clone(), move |s| {
        login::finish(s.store(), user, rehashed.as_deref())?;
        signed_in(&app, user, &device, now)
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user,
        AccountEvent::new("login", Surface::Mobile).detail(Some("password")),
    );
    Ok(Json(pair))
}

// ---- enroll / reset ----

/// The uniform "a mail went out (or deliberately didn't)" answer of the
/// mail loops; shared with the extension's signup route.
#[derive(Serialize)]
pub struct Sent {
    pub sent: bool,
}

#[derive(Deserialize)]
struct ResetRequestBody {
    #[serde(deserialize_with = "crate::bounds::email")]
    email: String,
}

async fn reset_request(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ResetRequestBody>,
) -> Result<(StatusCode, Json<Sent>), ApiError> {
    let Some(mailer) = state.mailer.clone() else {
        return Err(ApiError::not_available("reset"));
    };
    let site = state.site.clone();
    call(state.services.clone(), move |s| {
        reset::request(s.store(), mailer.as_ref(), &site, &body.email, now_ms())
    })
    .await?;
    state
        .ext
        .count_event(&state.services, "reset_request", None);
    Ok((StatusCode::ACCEPTED, Json(Sent { sent: true })))
}

fn link_invalid(what: &'static str) -> ApiError {
    let id = match what {
        "reset" => "auth-reset-spent-line",
        _ => "auth-invite-spent-line",
    };
    ApiError::new(StatusCode::FORBIDDEN, "link_invalid", crate::i18n::t(id))
}

#[derive(Deserialize)]
struct NewPasswordBody {
    #[serde(deserialize_with = "crate::bounds::password")]
    password: String,
    #[serde(default, deserialize_with = "crate::bounds::device_label")]
    device_label: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_opaque")]
    install_id: Option<String>,
}

async fn reset_confirm(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<NewPasswordBody>,
) -> ApiResult<TokenPair> {
    if !password::token_shape_ok(&token) {
        return Err(link_invalid("reset"));
    }
    password::validate_new_password(&body.password)
        .map_err(|id| ApiError::invalid(crate::i18n::t(id)))?;
    // Look the token up before hashing: a stranger with a made-up link
    // must not be able to spend the Argon2 permits. The reset is not
    // consumed here, so a hiccup between the steps burns nothing.
    let hash = hash_token(&token);
    {
        let hash = hash.clone();
        call(state.services.clone(), move |s| {
            s.store()
                .lookup_password_reset(&hash, now_ms())
                .map_err(crate::service::ServiceError::from)
        })
        .await?
        .ok_or_else(|| link_invalid("reset"))?;
    }
    let phc = password::hash_blocking(&state, body.password)
        .await
        .map_err(|_| ApiError::internal())?;
    let device = AppDevice::new(&body.device_label, body.install_id);
    let app = state.clone();
    call(state.services.clone(), move |s| {
        let now = now_ms();
        match reset::confirm(s.store(), &hash, &phc, now)? {
            Some(user) => signed_in(&app, user, &device, now),
            None => Err(link_invalid("reset")),
        }
    })
    .await
    .map(Json)
}

#[derive(Serialize)]
struct EnrollInfo {
    display_name: String,
    /// Set for signup invites: the verified address the account will get.
    email: Option<String>,
    role: String,
}

async fn enroll_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> ApiResult<EnrollInfo> {
    if !password::token_shape_ok(&token) {
        return Err(link_invalid("invite"));
    }
    let hash = hash_token(&token);
    let hooks = state.ext.clone();
    let enrollment = call(state.services.clone(), move |s| {
        enroll::load(s.store(), hooks.as_ref(), &hash, now_ms())
    })
    .await?;
    match enrollment {
        Some(enrollment) => Ok(Json(EnrollInfo {
            email: enrollment.preset_email().map(str::to_string),
            display_name: enrollment.invite.display_name,
            role: enrollment.invite.role,
        })),
        None => Err(link_invalid("invite")),
    }
}

#[derive(Deserialize)]
struct EnrollPasswordBody {
    #[serde(deserialize_with = "crate::bounds::password")]
    password: String,
    /// Required for admin invites (signup invites carry their address).
    #[serde(default, deserialize_with = "crate::bounds::email")]
    email: String,
    #[serde(default, deserialize_with = "crate::bounds::device_label")]
    device_label: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_opaque")]
    install_id: Option<String>,
}

async fn enroll_password(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<EnrollPasswordBody>,
) -> ApiResult<TokenPair> {
    if !password::token_shape_ok(&token) {
        return Err(link_invalid("invite"));
    }
    password::validate_new_password(&body.password)
        .map_err(|id| ApiError::invalid(crate::i18n::t(id)))?;
    let hash = hash_token(&token);
    let now = now_ms();
    let enrollment = {
        let hash = hash.clone();
        let hooks = state.ext.clone();
        call(state.services.clone(), move |s| {
            enroll::load(s.store(), hooks.as_ref(), &hash, now)
        })
        .await?
        .ok_or_else(|| link_invalid("invite"))?
    };
    enroll::resolve_email(&enrollment, &body.email)
        .map_err(|id| ApiError::invalid(crate::i18n::t(id)))?;
    let phc = password::hash_blocking(&state, body.password)
        .await
        .map_err(|_| ApiError::internal())?;
    let device = AppDevice::new(&body.device_label, body.install_id);
    let app = state.clone();
    call(state.services.clone(), move |s| {
        let outcome = enroll::with_password(
            s.store(),
            app.ext.as_ref(),
            &hash,
            &enrollment,
            &body.email,
            &phc,
            now,
        )?;
        match outcome {
            enroll::EnrollOutcome::Created(user) => signed_in(&app, user, &device, now),
            enroll::EnrollOutcome::EmailTaken => Err(ApiError::conflict(
                "email_taken",
                crate::i18n::t("auth-email-taken"),
            )),
            enroll::EnrollOutcome::BadEmail(id) => Err(ApiError::invalid(crate::i18n::t(id))),
            enroll::EnrollOutcome::InviteGone => Err(link_invalid("invite")),
        }
    })
    .await
    .map(Json)
}

// ---- enroll with a passkey ----

fn enroll_error(e: EnrollError) -> ApiError {
    match e {
        EnrollError::BadEmail(id) => ApiError::invalid(crate::i18n::t(id)),
        EnrollError::NoInvite => link_invalid("invite"),
        EnrollError::EmailTaken => {
            ApiError::conflict("email_taken", crate::i18n::t("auth-email-taken"))
        }
        EnrollError::Ceremony(c) => ceremony_error(c),
    }
}

#[derive(Deserialize)]
struct EnrollPasskeyStartBody {
    /// Required for admin invites (signup invites carry their address).
    #[serde(default, deserialize_with = "crate::bounds::email")]
    email: String,
}

async fn enroll_passkey_start(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<EnrollPasskeyStartBody>,
) -> ApiResult<Ceremony<webauthn_rs::prelude::CreationChallengeResponse>> {
    if !password::token_shape_ok(&token) {
        return Err(link_invalid("invite"));
    }
    let (ceremony_id, options) =
        webauthn::start_enroll_ceremony(&state, &token, &body.email, "/".to_string())
            .await
            .map_err(enroll_error)?;
    Ok(Json(Ceremony {
        ceremony_id,
        options,
    }))
}

#[derive(Deserialize)]
struct EnrollPasskeyFinishBody {
    #[serde(deserialize_with = "crate::bounds::token")]
    ceremony_id: String,
    credential: webauthn_rs::prelude::RegisterPublicKeyCredential,
    #[serde(default, deserialize_with = "crate::bounds::device_label")]
    device_label: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_opaque")]
    install_id: Option<String>,
}

async fn enroll_passkey_finish(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ApiJson(body): ApiJson<EnrollPasskeyFinishBody>,
) -> ApiResult<TokenPair> {
    if !password::token_shape_ok(&token) {
        return Err(link_invalid("invite"));
    }
    let enrolled =
        webauthn::finish_enroll_ceremony(&state, &hash_token(&body.ceremony_id), body.credential)
            .await
            .map_err(enroll_error)?;
    let device = AppDevice::new(&body.device_label, body.install_id);
    let app = state.clone();
    call(state.services.clone(), move |_| {
        signed_in(&app, enrolled.user, &device, now_ms())
    })
    .await
    .map(Json)
}

// ---- passkeys ----

/// A ceremony as the app carries it: the key rides in the body instead
/// of a cookie. `options` is the WebAuthn JSON (base64url fields) the
/// platform authenticator consumes.
#[derive(Serialize)]
pub struct Ceremony<T: Serialize> {
    pub ceremony_id: String,
    pub options: T,
}

/// `CeremonyError` in the envelope; shared with the settings routes.
pub(super) fn ceremony_error(e: CeremonyError) -> ApiError {
    match e {
        CeremonyError::NoPasskeys => ApiError::not_available("passkeys"),
        CeremonyError::Expired => ApiError::new(
            StatusCode::BAD_REQUEST,
            "ceremony_expired",
            crate::i18n::t("api-passkey-expired"),
        ),
        CeremonyError::Rejected | CeremonyError::UnknownCredential => {
            ApiError::invalid_credentials()
        }
        CeremonyError::Internal(msg) => {
            tracing::error!("passkey ceremony: {msg}");
            ApiError::internal()
        }
    }
}

async fn passkey_login_start(
    State(state): State<AppState>,
) -> ApiResult<Ceremony<webauthn_rs::prelude::RequestChallengeResponse>> {
    let (token, options) = webauthn::start_login_ceremony(&state)
        .await
        .map_err(ceremony_error)?;
    Ok(Json(Ceremony {
        ceremony_id: token,
        options,
    }))
}

#[derive(Deserialize)]
struct PasskeyLoginBody {
    #[serde(deserialize_with = "crate::bounds::token")]
    ceremony_id: String,
    credential: webauthn_rs::prelude::PublicKeyCredential,
    #[serde(default, deserialize_with = "crate::bounds::device_label")]
    device_label: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_opaque")]
    install_id: Option<String>,
}

async fn passkey_login_finish(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PasskeyLoginBody>,
) -> ApiResult<TokenPair> {
    let user =
        webauthn::finish_login_ceremony(&state, &hash_token(&body.ceremony_id), body.credential)
            .await
            .map_err(ceremony_error)?;
    let device = AppDevice::new(&body.device_label, body.install_id);
    let app = state.clone();
    let pair = call(state.services.clone(), move |_| {
        signed_in(&app, user, &device, now_ms())
    })
    .await?;
    state.ext.account_event(
        &state.services,
        user,
        AccountEvent::new("login", Surface::Mobile).detail(Some("passkey")),
    );
    Ok(Json(pair))
}

// ---- refresh / logout / sessions ----

#[derive(Deserialize)]
struct RefreshBody {
    #[serde(deserialize_with = "crate::bounds::token")]
    refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RefreshBody>,
) -> ApiResult<TokenPair> {
    let app = state.clone();
    call(state.services.clone(), move |s| {
        // The client is named so another client's token is refused
        // before anything about it changes.
        let refreshed = exchange_refresh(
            s.store(),
            &body.refresh_token,
            Some(MOBILE_CLIENT_ID),
            now_ms(),
        )?;
        match refreshed {
            Some(r) => Ok(TokenPair::new(
                r.access,
                r.refresh,
                r.expires_in,
                me_of(&app, &s, r.grant.user_id)?,
            )),
            None => Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_grant",
                // for machines: OAuth's own refusal, for the app's code.
                crate::i18n::Words::for_machines("refresh token invalid"),
            )),
        }
    })
    .await
    .map(Json)
}

async fn logout(State(state): State<AppState>, user: BearerUser) -> Result<StatusCode, ApiError> {
    call(state.services.clone(), move |s| {
        s.store()
            .revoke_token(user.id, MOBILE_CLIENT_ID, user.token_id, now_ms())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn this_device(user: &BearerUser) -> KeepSession {
    KeepSession {
        web_session_hash: None,
        api_token_id: Some(user.token_id),
    }
}

#[derive(Serialize)]
struct SessionRow {
    kind: DeviceKind,
    id: i64,
    label: crate::i18n::Words,
    created_at: i64,
    last_used_at: Option<i64>,
    /// The grant making this request.
    current: bool,
}

#[derive(Serialize)]
struct Sessions {
    items: Vec<SessionRow>,
}

/// Every device signed in to the account, browsers included.
async fn sessions(State(state): State<AppState>, user: BearerUser) -> ApiResult<Sessions> {
    let this = this_device(&user);
    let rows = call(state.services.clone(), move |s| {
        devices::list(s.store(), user.id, &this, now_ms())
    })
    .await?;
    Ok(Json(Sessions {
        items: rows
            .into_iter()
            .map(|d| SessionRow {
                kind: d.kind,
                id: d.id,
                label: d.label,
                created_at: d.created_at,
                last_used_at: d.last_used_at,
                current: d.current,
            })
            .collect(),
    }))
}

async fn revoke_session(
    State(state): State<AppState>,
    user: BearerUser,
    Path((kind, id)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    let kind = DeviceKind::parse(&kind).ok_or(ApiError::not_found("session"))?;
    call(state.services.clone(), move |s| {
        devices::sign_out(s.store(), user.id, kind, id, now_ms())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct Revoked {
    revoked: usize,
}

async fn revoke_others(State(state): State<AppState>, user: BearerUser) -> ApiResult<Revoked> {
    let this = this_device(&user);
    let revoked = call(state.services.clone(), move |s| {
        devices::sign_out_others(s.store(), user.id, &this, now_ms())
    })
    .await?;
    Ok(Json(Revoked { revoked }))
}

async fn me(State(state): State<AppState>, user: BearerUser) -> ApiResult<Me> {
    let app = state.clone();
    let me = call(state.services.clone(), move |s| me_of(&app, &s, user.id)).await?;
    Ok(Json(me))
}
