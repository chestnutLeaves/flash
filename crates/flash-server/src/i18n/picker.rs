//! The order a language picker lists the languages a server ships, the
//! same on the website and in the phone app: first the ones the reader's
//! own browser or phone asks for, in the order it asks (each as the
//! server's negotiation answers it, so "es-MX" brings Latin America's
//! Spanish), then the rest by their names in themselves, compared as card
//! search compares text (case and accents folded, `flash_store::fold`).
//! "Automatic" sits above them all, and is the picker's own.
//!
//! Shared vectors hold the app's twin to this order
//! (tests/picker_vectors.json).

use flash_core::locale::{negotiate_tags, UiLocale};

/// `shipped` in the order a picker lists them for a reader whose browser
/// or phone asks for `wanted` (most wanted first) in time zone `zone`.
pub fn order<S: AsRef<str>>(
    shipped: &[UiLocale],
    wanted: &[S],
    zone: Option<&str>,
) -> Vec<UiLocale> {
    let mut out: Vec<UiLocale> = Vec::new();
    for tag in wanted {
        if let Some(answer) = negotiate_tags(&[tag.as_ref()], zone, shipped) {
            if !out.contains(&answer.locale) {
                out.push(answer.locale);
            }
        }
    }
    let mut rest: Vec<UiLocale> = shipped
        .iter()
        .copied()
        .filter(|l| !out.contains(l))
        .collect();
    rest.sort_by_cached_key(|l| (flash_store::fold::fold(l.native_name()), l.tag()));
    out.extend(rest);
    out
}

/// Whether `locale` answers what a reader typed into a picker's search:
/// its name in itself or in English holds the text, folded alike.
#[cfg(feature = "test-support")]
pub fn matches(locale: UiLocale, query: &str) -> bool {
    let query = flash_store::fold::fold(query.trim());
    query.is_empty()
        || flash_store::fold::fold(locale.native_name()).contains(&query)
        || flash_store::fold::fold(locale.english_name()).contains(&query)
}

/// The number of languages from which a picker offers a search (more
/// than eight: a short list is read at a glance).
#[cfg(feature = "test-support")]
pub const SEARCH_FROM: usize = 9;
