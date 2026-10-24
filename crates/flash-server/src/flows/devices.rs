//! The devices list: every place the account is signed in, browsers and
//! the app together, the same on the settings page and in the app. A
//! browser is a session cookie, the app a grant of the first-party
//! client; connectors are not devices and stay under Connected apps.
//! Each row can be signed out, and "sign out everywhere else" spares
//! the device asking, as a password change does.

use flash_core::UserId;
use flash_store::Store;
use serde::{Deserialize, Serialize};

use crate::api::MOBILE_CLIENT_ID;

use super::account::KeepSession;

/// Where a sign-in lives. The two kinds are numbered apart, so a row is
/// always named by both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Browser,
    App,
}

impl DeviceKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "browser" => Some(DeviceKind::Browser),
            "app" => Some(DeviceKind::App),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DeviceKind::Browser => "browser",
            DeviceKind::App => "app",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub kind: DeviceKind,
    pub id: i64,
    /// "Chrome on Windows", "iPhone": what the browser or the app said
    /// it was, or its kind when it said nothing.
    pub label: crate::i18n::Words,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    /// The device asking.
    pub current: bool,
    /// A browser signed in by an emailed link rather than by the person.
    pub via_link: bool,
}

impl Device {
    /// When it was last seen: its last use, or its sign-in.
    pub fn active_at(&self) -> i64 {
        self.last_used_at.unwrap_or(self.created_at)
    }
}

/// Every device signed in to the account: the one asking first, then
/// the most recently active. `this` names the one asking.
pub fn list(
    store: &Store,
    user: UserId,
    this: &KeepSession,
    now_ms: i64,
) -> flash_store::Result<Vec<Device>> {
    let browsers = store
        .list_web_sessions(user, this.web_session_hash.as_deref(), now_ms)?
        .into_iter()
        .map(|s| Device {
            kind: DeviceKind::Browser,
            id: s.id,
            label: named(s.label, "settings-device-browser-unknown"),
            created_at: s.created_at,
            last_used_at: s.last_used_at,
            current: s.current,
            via_link: s.via_link,
        });
    let apps = store
        .list_tokens_for_user(user, MOBILE_CLIENT_ID, now_ms)?
        .into_iter()
        .map(|t| Device {
            kind: DeviceKind::App,
            current: this.api_token_id == Some(t.id),
            id: t.id,
            label: named(t.label, "settings-device-phone-unknown"),
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            via_link: false,
        });
    let mut devices: Vec<Device> = browsers.chain(apps).collect();
    devices.sort_by_key(|d| (!d.current, std::cmp::Reverse(d.active_at())));
    Ok(devices)
}

/// A device that never said what it is (a session from before names
/// were kept, a browser that sends no User-Agent) is named by its kind,
/// the message `fallback` in the reader's language.
fn named(label: String, fallback: &str) -> crate::i18n::Words {
    if label.trim().is_empty() {
        crate::i18n::t(fallback)
    } else {
        // theirs: the name the device gave itself.
        crate::i18n::Words::theirs(label)
    }
}

/// Signs one of the user's devices out. Another account's id, or one
/// already gone, is `NotFound`.
pub fn sign_out(
    store: &Store,
    user: UserId,
    kind: DeviceKind,
    id: i64,
    now_ms: i64,
) -> flash_store::Result<()> {
    match kind {
        DeviceKind::Browser => store.delete_web_session_by_id(user, id),
        DeviceKind::App => store.revoke_token(user, MOBILE_CLIENT_ID, id, now_ms),
    }
}

/// Signs out every device but the one asking; returns how many. Unlike
/// a password change it leaves connectors alone: they are not devices.
pub fn sign_out_others(
    store: &Store,
    user: UserId,
    this: &KeepSession,
    now_ms: i64,
) -> flash_store::Result<usize> {
    // One transaction: a failure between the two must not leave the
    // browsers signed out and the phones still in.
    store.with_tx(|tx| {
        let browsers =
            store.delete_sessions_for_user_tx(tx, user, this.web_session_hash.as_deref())?;
        let apps = flash_store::revoke_tokens_for_user_tx(
            tx,
            user,
            MOBILE_CLIENT_ID,
            this.api_token_id,
            now_ms,
        )?;
        Ok(browsers + apps)
    })
}
