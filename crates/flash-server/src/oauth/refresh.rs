//! Refresh-token exchange, shared by both token endpoints (the
//! connectors' and the app's).

use base64::Engine;
use flash_store::{NextPair, RefreshGrant, RefreshOutcome};
use hmac::{Hmac, KeyInit, Mac};

use super::{ACCESS_TTL_MS, REFRESH_TTL_MS};
use crate::auth::{hash_token, new_token};

/// A refresh token exchanged: whose grant, the pair it answers with, and
/// the seconds until that pair's access token expires.
pub(crate) struct Refreshed {
    pub grant: RefreshGrant,
    pub access: String,
    pub refresh: String,
    pub expires_in: i64,
}

/// Exchanges a refresh token for a pair: rotation with reuse detection
/// (`Store::rotate_refresh_token`). `None` is a refusal: unknown,
/// expired, revoked, another client's than `client_id`, or a reuse,
/// which has just revoked the grant. Runs on a blocking thread.
pub(crate) fn exchange_refresh(
    store: &flash_store::Store,
    refresh_token: &str,
    client_id: Option<&str>,
    now: i64,
) -> flash_store::Result<Option<Refreshed>> {
    let salt = new_token();
    let (access, refresh) = rotated_pair(refresh_token, &salt);
    let outcome = store.rotate_refresh_token(
        &hash_token(refresh_token),
        client_id,
        &NextPair {
            access_hash: &hash_token(&access),
            refresh_hash: &hash_token(&refresh),
            salt: &salt,
            access_expires_ms: now + ACCESS_TTL_MS,
            refresh_expires_ms: now + REFRESH_TTL_MS,
        },
        now,
    )?;
    Ok(match outcome {
        RefreshOutcome::Rotated(grant) => Some(Refreshed {
            grant,
            access,
            refresh,
            expires_in: ACCESS_TTL_MS / 1000,
        }),
        RefreshOutcome::Again {
            grant,
            salt,
            access_expires_at,
        } => {
            let (access, refresh) = rotated_pair(refresh_token, &salt);
            Some(Refreshed {
                grant,
                access,
                refresh,
                expires_in: (access_expires_at - now) / 1000,
            })
        }
        RefreshOutcome::Reused(grant) => {
            tracing::warn!(
                client_id = %grant.client_id,
                user = grant.user_id.raw(),
                "refresh token reuse detected; grant revoked"
            );
            None
        }
        RefreshOutcome::Invalid => None,
    })
}

/// The pair a refresh makes: HMAC-SHA-256 of a fresh random salt, keyed
/// by the refresh token it rotates away. The same token and salt make
/// the same pair again, which is how a client whose reply was lost gets
/// it during the grace; the store keeps the salt but only the tokens'
/// hashes, so it cannot make the pair itself.
fn rotated_pair(refresh_token: &str, salt: &str) -> (String, String) {
    let derive = |half: &str| {
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(refresh_token.as_bytes())
            .expect("hmac accepts any key length");
        mac.update(salt.as_bytes());
        mac.update(half.as_bytes());
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    };
    (derive("access"), derive("refresh"))
}
