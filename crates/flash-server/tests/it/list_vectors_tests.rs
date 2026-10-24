//! Lists (`i18n::list_in`, CLDR's patterns through ICU4X), written here
//! and again by a downstream extension from the joiners `i18n::lists`
//! gives it. Every copy is held to `list_vectors.json`, the server's
//! answer for each case, written here and failing while stale:
//!
//!   FLASH_WRITE_LIST_VECTORS=1 cargo test -p flash-server --test it list_vectors_tests::

use std::path::PathBuf;

use flash_core::locale::UiLocale;
use flash_server::i18n::{list_in, ListKind};

/// The lists each language is asked for: none to four items, and the
/// words a joiner changes for (and the near misses it does not).
const ITEMS: &[&[&str]] = &[
    &[],
    &["Biology"],
    &["Biology", "Chemistry"],
    &["Biology", "Chemistry", "Physics"],
    &["Biology", "Chemistry", "Physics", "Maths"],
    &["Arte", "inglés"],
    &["Arte", "Historia", "Hilo"],
    &["Arte", "hielo"],
    &["Arte", "hiato"],
    &["Arte", "chico"],
    &["Arte", "hora"],
    &["Arte", "choco"],
    &["Arte", "Música", "ocho"],
    &["Arte", "8"],
    &["Arte", "11"],
    &["Arte", "11.000"],
    &["Arte", "11\u{a0}000"],
    &["Arte", "110"],
    &["Arte", "11 gatos"],
];

const KINDS: [(&str, ListKind); 3] = [
    ("and", ListKind::And),
    ("or", ListKind::Or),
    ("unit", ListKind::Unit),
];

/// Every invisible character written as its escape, so the file shows
/// what it holds.
fn visible(json: &str) -> String {
    json.chars()
        .map(|c| match c {
            '\u{a0}' | '\u{200E}'..='\u{200F}' | '\u{2011}' | '\u{202A}'..='\u{202E}' => {
                format!("\\u{:04X}", c as u32)
            }
            _ => c.to_string(),
        })
        .collect()
}

fn vectors() -> String {
    let mut cases = Vec::new();
    for locale in UiLocale::all() {
        for (name, kind) in KINDS {
            for items in ITEMS {
                cases.push(serde_json::json!({
                    "tag": locale.tag(),
                    "kind": name,
                    "items": items,
                    "out": list_in(locale, items, kind),
                }));
            }
        }
    }
    let json = serde_json::json!({
        "note": "Each list as the server writes it (crates/flash-server/src/i18n.rs, list_in). Written by crates/flash-server/tests/it/list_vectors_tests.rs; rerun with FLASH_WRITE_LIST_VECTORS=1.",
        "cases": cases,
    });
    visible(&serde_json::to_string_pretty(&json).unwrap()) + "\n"
}

#[test]
fn the_apps_vectors_are_the_servers_lists() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/list_vectors.json");
    let fresh = vectors();
    if std::env::var("FLASH_WRITE_LIST_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        held.replace("\r\n", "\n") == fresh,
        "{} is stale. Rerun with FLASH_WRITE_LIST_VECTORS=1",
        path.display()
    );
}

/// English writes a plain enumeration as it always has ("a, b, c"), so
/// its pages read as they did; Spanish turns its joiners where CLDR does.
#[test]
fn english_enumerates_as_it_did_and_spanish_turns_its_joiners() {
    let spain = flash_server::testing::spain();
    assert_eq!(
        list_in(UiLocale::EN, &["a", "b", "c"], ListKind::Unit),
        ["a", "b", "c"].join(", ")
    );
    assert_eq!(list_in(UiLocale::EN, &["a", "b"], ListKind::Unit), "a, b");
    assert_eq!(
        list_in(spain, &["arte", "inglés"], ListKind::And),
        "arte e inglés"
    );
    assert_eq!(
        list_in(spain, &["arte", "hora"], ListKind::Or),
        "arte u hora"
    );
    assert_eq!(
        list_in(spain, &["arte", "hielo"], ListKind::And),
        "arte y hielo"
    );
}

/// Every language's joiners come out of its lists whole, and a condition
/// is attached exactly where ICU4X applies it.
#[test]
fn every_languages_joiners_are_read_from_its_lists() {
    use flash_server::i18n::lists::joiners;
    for locale in UiLocale::all() {
        for (_, kind) in KINDS {
            let j = joiners(locale, kind);
            assert!(
                !j.pair.is_empty() || locale.tag().starts_with("zh"),
                "{j:?}"
            );
        }
    }
    let spain = flash_server::testing::spain();
    assert_eq!(joiners(spain, ListKind::And).special.unwrap().then, " e ");
    assert_eq!(joiners(spain, ListKind::Or).special.unwrap().then, " u ");
    assert!(joiners(UiLocale::EN, ListKind::And).special.is_none());
}
