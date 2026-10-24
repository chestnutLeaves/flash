//! Passkey ceremonies: enrollment via single-use invite links, and login.
//! The user row is only created when enrollment *finishes* successfully.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use webauthn_rs::prelude::*;

use crate::auth::{
    self, clear_cookie_header, cookie_header, hash_token, new_token, with_session_cookie,
    CEREMONY_COOKIE, CEREMONY_TTL_MS,
};
use crate::flows::enroll;
use crate::service::now_ms;
use crate::state::{AppState, Ceremony};

type ApiResult<T> = Result<T, Response>;

fn bad(status: StatusCode, msg: &str) -> Response {
    (status, msg.to_string()).into_response()
}

fn internal(context: &str, err: impl std::fmt::Display) -> Response {
    tracing::error!("{context}: {err}");
    bad(
        StatusCode::INTERNAL_SERVER_ERROR,
        &crate::i18n::t("error-internal"),
    )
}

async fn blocking<T, F>(f: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, flash_store::StoreError> + Send + 'static,
{
    crate::i18n::blocking(f)
        .await
        .map_err(|e| internal("join", e))?
        .map_err(|e| internal("store", e))
}

fn ceremony_key(jar: &CookieJar) -> ApiResult<String> {
    jar.get(CEREMONY_COOKIE)
        .map(|c| hash_token(c.value()))
        .ok_or_else(|| bad(StatusCode::BAD_REQUEST, &crate::i18n::t("auth-no-ceremony")))
}

// ---- Enrollment ----

/// Light-touch normalization: enough to keep the UNIQUE column coherent
/// (it compares case-sensitively) and reject obvious typos. Ownership is
/// proven by a mailed link, not by parsing.
pub fn normalize_email(raw: &str) -> Result<String, &'static str> {
    let email = raw.trim().to_lowercase();
    if email.is_empty() {
        return Err("auth-email-required");
    }
    if email.len() > 254 || email.chars().any(char::is_whitespace) {
        return Err("auth-email-invalid");
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err("auth-email-invalid");
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err("auth-email-invalid");
    }
    Ok(email)
}

#[derive(serde::Deserialize)]
pub struct EnrollStartBody {
    #[serde(deserialize_with = "crate::bounds::email")]
    email: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_next_path")]
    next: Option<String>,
}

#[derive(Debug)]
pub(crate) enum EnrollError {
    BadEmail(&'static str),
    NoInvite,
    EmailTaken,
    Ceremony(CeremonyError),
}

impl From<flash_store::StoreError> for EnrollError {
    fn from(e: flash_store::StoreError) -> Self {
        EnrollError::Ceremony(CeremonyError::Internal(e.to_string()))
    }
}

/// What the finished ceremony yields: the new user and where the web
/// wanted to land afterwards.
pub(crate) struct Enrolled {
    pub user: flash_core::UserId,
    pub next: String,
}

/// Begins enrolling a new account with a passkey against an invite.
/// Signup invites lock the account to the address the verification link
/// was mailed to; `typed_email` is only trusted for classic admin
/// invites. Returns the raw ceremony key and the challenge.
pub(crate) async fn start_enroll_ceremony(
    state: &AppState,
    invite_token: &str,
    typed_email: &str,
    next: String,
) -> Result<(String, CreationChallengeResponse), EnrollError> {
    let invite_hash = hash_token(invite_token);
    let now = now_ms();
    let services = state.services.clone();
    let hooks = state.ext.clone();
    let typed = typed_email.to_string();
    let (enrollment, email) = {
        let invite_hash = invite_hash.clone();
        crate::i18n::blocking(move || -> Result<_, EnrollError> {
            let store = services.store();
            let Some(enrollment) = enroll::load(store, hooks.as_ref(), &invite_hash, now)? else {
                return Err(EnrollError::NoInvite);
            };
            let email =
                enroll::resolve_email(&enrollment, &typed).map_err(EnrollError::BadEmail)?;
            if store.email_taken(&email)? {
                return Err(EnrollError::EmailTaken);
            }
            Ok((enrollment, email))
        })
        .await
        .map_err(|e| EnrollError::Ceremony(CeremonyError::Internal(e.to_string())))??
    };
    let (challenge, reg_state) = state
        .webauthn
        .start_passkey_registration(
            Uuid::new_v4(),
            &enrollment.invite.display_name,
            &enrollment.invite.display_name,
            None,
        )
        .map_err(|e| EnrollError::Ceremony(CeremonyError::Internal(e.to_string())))?;
    let challenge = discoverable(challenge);
    let token = new_token();
    state.put_ceremony(
        hash_token(&token),
        Ceremony::Registration {
            state: reg_state,
            invite_token_hash: invite_hash,
            display_name: enrollment.invite.display_name,
            email,
            signup: enrollment.signup,
            role: enrollment.invite.role,
            next,
            expires_ms: now + CEREMONY_TTL_MS,
        },
    );
    Ok((token, challenge))
}

/// Finishes enrollment: verifies the registration, burns the single-use
/// invite, creates the user, and stores the passkey.
pub(crate) async fn finish_enroll_ceremony(
    state: &AppState,
    key_hash: &str,
    credential: RegisterPublicKeyCredential,
) -> Result<Enrolled, EnrollError> {
    let now = now_ms();
    let Some(Ceremony::Registration {
        state: reg_state,
        invite_token_hash,
        display_name,
        email,
        signup,
        role,
        next,
        ..
    }) = state.take_ceremony(key_hash, now)
    else {
        return Err(EnrollError::Ceremony(CeremonyError::Expired));
    };
    let passkey = state
        .webauthn
        .finish_passkey_registration(&credential, &reg_state)
        .map_err(|e| {
            tracing::warn!("passkey registration rejected: {e}");
            EnrollError::Ceremony(CeremonyError::Rejected)
        })?;
    let passkey_json = serde_json::to_string(&passkey)
        .map_err(|e| EnrollError::Ceremony(CeremonyError::Internal(e.to_string())))?;
    let cred_id = AsRef::<[u8]>::as_ref(passkey.cred_id()).to_vec();
    let services = state.services.clone();
    let hooks = state.ext.clone();
    crate::i18n::blocking(move || -> Result<Enrolled, EnrollError> {
        // One transaction: the invite is burned only if the account, what
        // the extension records about it and its passkey all land. A crash
        // part way leaves the link working, not an account nobody can
        // sign in to.
        let user = services.store().with_tx(|tx| {
            // Re-check before burning the single-use invite: if the email
            // was claimed since start, the user can fix it and retry.
            if flash_store::email_taken_tx(tx, &email)? {
                return Ok(None);
            }
            flash_store::consume_invite_tx(tx, &invite_token_hash, now)?;
            let user = flash_store::create_user_tx(tx, &display_name, Some(&email), &role, now)?;
            hooks.user_enrolled(tx, user, signup.as_ref(), &invite_token_hash, now)?;
            flash_store::add_passkey_tx(tx, user, &cred_id, &passkey_json, "enrolled", now)?;
            Ok(Some(user))
        })?;
        let user = user.ok_or(EnrollError::EmailTaken)?;
        Ok(Enrolled { user, next })
    })
    .await
    .map_err(|e| EnrollError::Ceremony(CeremonyError::Internal(e.to_string())))?
}

fn enroll_response(err: EnrollError) -> Response {
    match err {
        EnrollError::BadEmail(id) => bad(StatusCode::BAD_REQUEST, &crate::i18n::t(id)),
        EnrollError::NoInvite => bad(
            StatusCode::FORBIDDEN,
            &crate::i18n::t("auth-invite-expired"),
        ),
        EnrollError::EmailTaken => {
            bad(StatusCode::BAD_REQUEST, &crate::i18n::t("auth-email-taken"))
        }
        EnrollError::Ceremony(e) => ceremony_response(e),
    }
}

pub async fn enroll_start(
    State(state): State<AppState>,
    Path(invite_token): Path<String>,
    Json(body): Json<EnrollStartBody>,
) -> Response {
    let next = crate::web::clean_next(body.next);
    match start_enroll_ceremony(&state, &invite_token, &body.email, next).await {
        Ok((token, challenge)) => {
            let mut response = Json(challenge).into_response();
            if let Ok(value) =
                cookie_header(CEREMONY_COOKIE, &token, CEREMONY_TTL_MS / 1000).parse()
            {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            response
        }
        Err(e) => enroll_response(e),
    }
}

pub async fn enroll_finish(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    Json(credential): Json<RegisterPublicKeyCredential>,
) -> Response {
    let browser = auth::Browser::from_headers(&headers);
    let key = match ceremony_key(&jar) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let enrolled = match finish_enroll_ceremony(&state, &key, credential).await {
        Ok(enrolled) => enrolled,
        Err(e) => return enroll_response(e),
    };
    let services = state.services.clone();
    let now = now_ms();
    let user = enrolled.user;
    match blocking(move || auth::issue_session(services.store(), user, &browser, now)).await {
        Ok(session_token) => with_session_cookie(
            &session_token,
            Json(serde_json::json!({"ok": true, "next": enrolled.next})).into_response(),
        ),
        Err(response) => response,
    }
}

// ---- Ceremonies shared by the web (cookie-keyed) and the API (body-keyed) ----
//
// Both surfaces run the same WebAuthn steps; they differ only in how the
// ceremony key travels back (a cookie vs. a `ceremony_id` field). These
// functions take and return the raw key so each caller can carry it its
// own way.

#[derive(Debug)]
pub(crate) enum CeremonyError {
    /// Login: nobody has enrolled a passkey on this server.
    NoPasskeys,
    /// The key didn't match a live ceremony of the expected kind.
    Expired,
    /// The authenticator's answer failed verification.
    Rejected,
    /// An answer from a credential we don't hold (or can't read).
    UnknownCredential,
    Internal(String),
}

impl From<flash_store::StoreError> for CeremonyError {
    fn from(e: flash_store::StoreError) -> Self {
        CeremonyError::Internal(e.to_string())
    }
}

/// The stored passkeys that parse. One that doesn't is left out, loudly:
/// its owner can no longer sign in with it, and a quiet skip would hide a
/// format change in webauthn-rs until someone reported being locked out.
fn parse_passkeys(rows: &[flash_store::StoredPasskey]) -> Vec<Passkey> {
    rows.iter().filter_map(|p| parse_passkey(p).ok()).collect()
}

fn parse_passkey(row: &flash_store::StoredPasskey) -> Result<Passkey, serde_json::Error> {
    serde_json::from_str(&row.passkey_json).inspect_err(|e| {
        tracing::error!(
            "stored passkey {} of user {} does not parse: {e}",
            row.id,
            row.user_id
        );
    })
}

/// Registration options that ask for a discoverable credential where the
/// authenticator can make one: sign-in names no account, so only a
/// passkey its authenticator can find by itself signs in.
fn discoverable(mut challenge: CreationChallengeResponse) -> CreationChallengeResponse {
    if let Some(selection) = challenge.public_key.authenticator_selection.as_mut() {
        selection.resident_key = Some(webauthn_rs_proto::ResidentKeyRequirement::Preferred);
    }
    challenge
}

/// Begins a login: the challenge for the caller to hand to the
/// authenticator plus the raw ceremony key. Nobody is known yet, so the
/// challenge lists no credentials and the authenticator offers its own.
pub(crate) async fn start_login_ceremony(
    state: &AppState,
) -> Result<(String, RequestChallengeResponse), CeremonyError> {
    let services = state.services.clone();
    if !crate::i18n::blocking(move || services.store().any_passkey())
        .await
        .map_err(|e| CeremonyError::Internal(e.to_string()))??
    {
        return Err(CeremonyError::NoPasskeys);
    }
    // The library's discoverable flow refuses a passkey whose
    // backup-eligible flag turned on after registration; such a user signs
    // in another way and adds it again, until the library exposes the
    // setting.
    let (mut challenge, auth_state) = state
        .webauthn
        .start_discoverable_authentication()
        .map_err(|e| CeremonyError::Internal(e.to_string()))?;
    // Both clients open a sign-in prompt with these options, as before:
    // not conditional (autofill) mediation, and no extensions.
    challenge.mediation = None;
    challenge.public_key.extensions = None;
    let token = new_token();
    state.put_ceremony(
        hash_token(&token),
        Ceremony::Authentication {
            state: auth_state,
            expires_ms: now_ms() + CEREMONY_TTL_MS,
        },
    );
    Ok((token, challenge))
}

/// Finishes a login: finds the passkey the authenticator named, verifies
/// the assertion against it alone, and returns its owner, with its
/// counter/last-used bookkeeping done.
pub(crate) async fn finish_login_ceremony(
    state: &AppState,
    key_hash: &str,
    credential: PublicKeyCredential,
) -> Result<flash_core::UserId, CeremonyError> {
    let now = now_ms();
    let Some(Ceremony::Authentication {
        state: auth_state, ..
    }) = state.take_ceremony(key_hash, now)
    else {
        return Err(CeremonyError::Expired);
    };
    let services = state.services.clone();
    let webauthn = state.webauthn.clone();
    crate::i18n::blocking(move || -> Result<flash_core::UserId, CeremonyError> {
        let store = services.store();
        // By the credential id, not `identify_discoverable_authentication`:
        // that also wants the user handle, which a passkey answering from
        // an allow list may leave out, and the id alone finds the row.
        let Some((row, mut passkey)) = store
            .passkey_by_cred_id(credential.get_credential_id())?
            .and_then(|row| parse_passkey(&row).ok().map(|passkey| (row, passkey)))
        else {
            return Err(CeremonyError::UnknownCredential);
        };
        let result = webauthn
            .finish_discoverable_authentication(&credential, auth_state, &[(&passkey).into()])
            .map_err(|e| {
                tracing::warn!("passkey login rejected: {e}");
                CeremonyError::Rejected
            })?;
        // Persist counter/backup-state updates when present; either way
        // stamp last_used_at: most authenticators keep their sign counter
        // at 0, and the login still happened.
        if passkey.update_credential(&result) == Some(true) {
            if let Ok(json) = serde_json::to_string(&passkey) {
                store.update_passkey(row.user_id, row.id, &json, now)?;
            }
        } else {
            store.touch_passkey(row.user_id, row.id, now)?;
        }
        Ok(row.user_id)
    })
    .await
    .map_err(|e| CeremonyError::Internal(e.to_string()))?
}

/// Begins adding a passkey to a signed-in account; the user's existing
/// credentials are excluded so the same authenticator isn't enrolled
/// twice.
pub(crate) async fn start_add_ceremony(
    state: &AppState,
    user: flash_core::UserId,
) -> Result<(String, CreationChallengeResponse), CeremonyError> {
    let services = state.services.clone();
    let (display_name, existing) = crate::i18n::blocking(move || {
        let store = services.store();
        let (name, _) = store.get_user_info(user)?;
        let existing: Vec<CredentialID> = parse_passkeys(&store.passkeys_for_user(user)?)
            .iter()
            .map(|p| p.cred_id().clone())
            .collect();
        Ok::<_, flash_store::StoreError>((name, existing))
    })
    .await
    .map_err(|e| CeremonyError::Internal(e.to_string()))??;
    let (challenge, reg_state) = state
        .webauthn
        .start_passkey_registration(
            Uuid::new_v4(),
            &display_name,
            &display_name,
            (!existing.is_empty()).then_some(existing),
        )
        .map_err(|e| CeremonyError::Internal(e.to_string()))?;
    let challenge = discoverable(challenge);
    let token = new_token();
    state.put_ceremony(
        hash_token(&token),
        Ceremony::AddPasskey {
            state: reg_state,
            user,
            expires_ms: now_ms() + CEREMONY_TTL_MS,
        },
    );
    Ok((token, challenge))
}

/// Finishes adding a passkey; returns the new row's id. `user` must be
/// the account that started the ceremony.
pub(crate) async fn finish_add_ceremony(
    state: &AppState,
    key_hash: &str,
    user: flash_core::UserId,
    credential: RegisterPublicKeyCredential,
    label: &str,
) -> Result<i64, CeremonyError> {
    let now = now_ms();
    let Some(Ceremony::AddPasskey {
        state: reg_state,
        user: owner,
        ..
    }) = state.take_ceremony(key_hash, now)
    else {
        return Err(CeremonyError::Expired);
    };
    if owner != user {
        return Err(CeremonyError::Expired);
    }
    let passkey = state
        .webauthn
        .finish_passkey_registration(&credential, &reg_state)
        .map_err(|e| {
            tracing::warn!("passkey registration rejected: {e}");
            CeremonyError::Rejected
        })?;
    let json =
        serde_json::to_string(&passkey).map_err(|e| CeremonyError::Internal(e.to_string()))?;
    let cred_id = AsRef::<[u8]>::as_ref(passkey.cred_id()).to_vec();
    let label = label.trim().chars().take(80).collect::<String>();
    let label = if label.is_empty() { "passkey" } else { &label };
    let label = label.to_string();
    let services = state.services.clone();
    crate::i18n::blocking(move || {
        services
            .store()
            .add_passkey(user, &cred_id, &json, &label, now)
    })
    .await
    .map_err(|e| CeremonyError::Internal(e.to_string()))?
    .map_err(Into::into)
}

// ---- Login (web) ----

fn ceremony_response(err: CeremonyError) -> Response {
    match err {
        CeremonyError::NoPasskeys => {
            bad(StatusCode::FORBIDDEN, &crate::i18n::t("auth-no-passkeys"))
        }
        CeremonyError::Expired => bad(
            StatusCode::BAD_REQUEST,
            "ceremony expired; reload and retry",
        ),
        CeremonyError::Rejected => bad(
            StatusCode::UNAUTHORIZED,
            &crate::i18n::t("auth-passkey-failed"),
        ),
        CeremonyError::UnknownCredential => bad(
            StatusCode::UNAUTHORIZED,
            &crate::i18n::t("auth-passkey-unknown"),
        ),
        CeremonyError::Internal(e) => internal("passkey ceremony", e),
    }
}

pub async fn login_start(State(state): State<AppState>) -> Response {
    match start_login_ceremony(&state).await {
        Ok((token, challenge)) => {
            let mut response = Json(challenge).into_response();
            if let Ok(value) =
                cookie_header(CEREMONY_COOKIE, &token, CEREMONY_TTL_MS / 1000).parse()
            {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            response
        }
        Err(e) => ceremony_response(e),
    }
}

pub async fn login_finish(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    Json(credential): Json<PublicKeyCredential>,
) -> Response {
    let browser = auth::Browser::from_headers(&headers);
    let key = match ceremony_key(&jar) {
        Ok(key) => key,
        Err(response) => return response,
    };
    let user = match finish_login_ceremony(&state, &key, credential).await {
        Ok(user) => user,
        Err(e) => return ceremony_response(e),
    };
    let services = state.services.clone();
    let now = now_ms();
    match blocking(move || auth::issue_session(services.store(), user, &browser, now)).await {
        Ok(session_token) => {
            crate::web::account_event(&state, user, "login", Some("passkey"), None);
            with_session_cookie(
                &session_token,
                Json(serde_json::json!({"ok": true})).into_response(),
            )
        }
        Err(response) => response,
    }
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get(auth::SESSION_COOKIE) {
        let hash = hash_token(cookie.value());
        let services = state.services.clone();
        let _ = blocking(move || services.store().delete_web_session(&hash)).await;
    }
    let mut response = axum::response::Redirect::to("/login").into_response();
    if let Ok(value) = clear_cookie_header(auth::SESSION_COOKIE).parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn normalize_email_accepts_and_lowercases() {
        assert_eq!(
            normalize_email("  Ada.Lovelace42@Example.com "),
            Ok("ada.lovelace42@example.com".to_string())
        );
        assert_eq!(normalize_email("a@b"), Ok("a@b".to_string()));
    }

    #[test]
    fn normalize_email_rejects_garbage() {
        assert!(normalize_email("").is_err());
        assert!(normalize_email("   ").is_err());
        assert!(normalize_email("no-at-sign").is_err());
        assert!(normalize_email("@nolocal.com").is_err());
        assert!(normalize_email("nodomain@").is_err());
        assert!(normalize_email("two@@ats.com").is_err());
        assert!(normalize_email("has space@x.com").is_err());
        assert!(normalize_email(&format!("{}@x.com", "a".repeat(260))).is_err());
    }
}
