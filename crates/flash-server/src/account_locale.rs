//! What a person's own software says about the language they read: the
//! browser that signs in, the assistant they study in, their phone.
//! Each signal is kept with the account (`account_locales`) and taken as
//! its language while nobody chose one (`user_settings.locale`). Nothing
//! here ever fails a request.

use std::collections::HashMap;

use parking_lot::Mutex;

use flash_core::UserId;
use flash_store::{LocaleSource, Store};

use crate::service::Services;

/// Keeps a signal and takes it as the account's language while nobody
/// chose one: `languages` is most wanted first, and a browser's whole
/// list is kept with it. Runs on a blocking thread or inside one.
pub fn keep(store: &Store, user: UserId, source: LocaleSource, languages: &[String], now_ms: i64) {
    let Some(first) = languages.first() else {
        return;
    };
    let prefs = (source == LocaleSource::Browser).then(|| languages.join(","));
    let kept = store
        .observe_locale(user, source, first, prefs.as_deref(), now_ms)
        .and_then(|()| store.adopt_locale(user, source, first));
    if let Err(e) = kept {
        tracing::warn!(error = %e, source = source.as_str(), "could not keep a language");
    }
}

/// `keep` for a signal every request carries (the assistant's, the
/// phone's): written once a day per account and source, or at once when
/// it changes, so a chat's every call is not a write. True when it was
/// news and kept.
pub async fn keep_often_seen(
    services: &Services,
    user: UserId,
    source: LocaleSource,
    languages: Vec<String>,
    now_ms: i64,
) -> bool {
    let Some(first) = languages.first() else {
        return false;
    };
    if !services.seen_locales.is_news(user, source, first, now_ms) {
        return false;
    }
    let services = services.clone();
    let _ = tokio::task::spawn_blocking(move || {
        keep(services.store(), user, source, &languages, now_ms);
    })
    .await;
    true
}

/// The language each account's assistant or phone last said, and when
/// it was kept, for one store (`Services` holds it: a process-wide one
/// would let one server's accounts silence another's that share an id).
/// Bounded: cleared when it grows past what one server sees in a day.
#[derive(Default)]
pub struct Seen(Mutex<HashMap<SeenKey, (String, i64)>>);

/// An account and the source (`LocaleSource::as_str`) it was heard from.
type SeenKey = (UserId, &'static str);

impl Seen {
    fn is_news(&self, user: UserId, source: LocaleSource, tag: &str, now_ms: i64) -> bool {
        const DAY_MS: i64 = 86_400_000;
        let mut seen = self.0.lock();
        if seen.len() > 50_000 {
            seen.clear();
        }
        let key = (user, source.as_str());
        match seen.get(&key) {
            Some((last, at)) if last == tag && now_ms - at < DAY_MS => false,
            _ => {
                seen.insert(key, (tag.to_string(), now_ms));
                true
            }
        }
    }
}
