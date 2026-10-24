//! Password reset by mailed link. `request` is uniform (unknown address,
//! cooldown, and cap all look like a sent mail); `confirm` burns the
//! token, stores the new hash, and signs out every session and device:
//! the reset's premise is a possibly-stolen password.

use flash_core::UserId;
use flash_store::Store;

use super::account::{replace_password_tx, KeepSession};
use crate::auth::{hash_token, new_token};
use crate::email::{self, Mailer};
use crate::service::Result;
use crate::state::Site;

pub const TTL_MS: i64 = 60 * 60 * 1000;
pub const COOLDOWN_MS: i64 = 60_000;
pub const DAILY_CAP: u32 = 5;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Mails a reset link when the address has an account and isn't rate
/// limited; always `Ok(())` so callers answer uniformly.
pub fn request(
    store: &Store,
    mailer: &dyn Mailer,
    site: &Site,
    email: &str,
    now_ms: i64,
) -> Result<()> {
    let Some((email, user)) = account(store, email)? else {
        return Ok(());
    };
    if store.recent_password_resets(user, now_ms - COOLDOWN_MS)? > 0
        || store.recent_password_resets(user, now_ms - DAY_MS)? >= DAILY_CAP
    {
        return Ok(());
    }
    let token = mint(store, user, now_ms)?;
    // In the account's language; one it never set reads in the language
    // of the page that asked.
    let reader = crate::i18n::for_account(store, user, &crate::i18n::tr());
    let mut mail = crate::i18n::scoped(reader, || email::password_reset_email(site, &token));
    mail.to = email;
    if let Err(e) = mailer.send(&mail) {
        tracing::error!("reset mail: {e}");
        let _ = store.delete_password_reset(&hash_token(&token));
    }
    Ok(())
}

/// The link `flash-server reset-link` prints, for an operator whose
/// instance sends no mail: the mailed link and token, without the mail's
/// rate limits (the operator already holds the database). `None` when no
/// account has the address.
pub fn link_for(store: &Store, base_url: &str, email: &str, now_ms: i64) -> Result<Option<String>> {
    let Some((_, user)) = account(store, email)? else {
        return Ok(None);
    };
    Ok(Some(link(base_url, &mint(store, user, now_ms)?)))
}

pub fn link(base_url: &str, token: &str) -> String {
    format!("{base_url}/reset/{token}")
}

/// The normalized address and its account, when it has one.
fn account(store: &Store, email: &str) -> Result<Option<(String, UserId)>> {
    let Ok(email) = crate::webauthn::normalize_email(email) else {
        return Ok(None);
    };
    Ok(store
        .user_by_email(&email)?
        .map(|login| (email, login.user)))
}

fn mint(store: &Store, user: UserId, now_ms: i64) -> Result<String> {
    let token = new_token();
    store.create_password_reset(&hash_token(&token), user, now_ms, now_ms + TTL_MS)?;
    Ok(token)
}

/// Consumes the reset token and installs `phc`. `Ok(None)` when the
/// token is invalid, expired, or already used.
///
/// One transaction: the new password and the end of every old session
/// and grant land together. A reset is how someone locks an intruder out,
/// and a crash between the two must not leave the intruder signed in.
pub fn confirm(store: &Store, token_hash: &str, phc: &str, now_ms: i64) -> Result<Option<UserId>> {
    Ok(store.with_tx(|tx| {
        let Some(user) = flash_store::lookup_password_reset_tx(tx, token_hash, now_ms)? else {
            return Ok(None);
        };
        flash_store::consume_password_reset_tx(tx, token_hash, now_ms)?;
        // Nothing kept: the reset's premise is a possibly-stolen password.
        replace_password_tx(store, tx, user, phc, &KeepSession::default(), now_ms)?;
        Ok(Some(user))
    })?)
}
