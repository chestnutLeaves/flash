//! A language picker's order and search (`i18n::picker`), written here
//! and again by a downstream extension. Every copy is held to
//! `picker_vectors.json`, the server's answer for each case, written here
//! and failing while stale:
//!
//!   FLASH_WRITE_PICKER_VECTORS=1 cargo test -p flash-server --test it picker_vectors_tests::

use std::path::PathBuf;

use flash_core::locale::UiLocale;
use flash_server::i18n::picker;

/// What browsers and phones ask for, most wanted first, and where they
/// are: none, one the server answers, one only a stand-in answers, one
/// none answers, several, and a Spanish whose region is its time zone.
const WANTED: &[(&[&str], Option<&str>)] = &[
    (&[], None),
    (&["en-US"], None),
    (&["es-MX", "en-US"], None),
    (&["fr-FR"], None),
    (&["ca"], Some("Europe/Madrid")),
    (&["es"], Some("Europe/Madrid")),
    (&["es"], Some("America/Bogota")),
    (&["en-GB", "es-ES", "es-AR"], None),
    (&["de-DE", "es-419", "en"], None),
];

/// What a reader types into a picker's search.
const QUERIES: &[&str] = &[
    "", "esp", "ESPAÑOL", "espanol", "spanish", "eng", "latino", "xyz", " es ",
];

fn vectors() -> String {
    let all: Vec<UiLocale> = UiLocale::all().collect();
    let real: Vec<UiLocale> = UiLocale::real().collect();
    let tags = |list: &[UiLocale]| list.iter().map(|l| l.tag()).collect::<Vec<_>>();
    let mut order = Vec::new();
    for shipped in [&all, &real] {
        for (wanted, zone) in WANTED {
            order.push(serde_json::json!({
                "shipped": tags(shipped),
                "wanted": wanted,
                "zone": zone,
                "order": tags(&picker::order(shipped, wanted, *zone)),
            }));
        }
    }
    let mut matches = Vec::new();
    for query in QUERIES {
        let found: Vec<&str> = all
            .iter()
            .filter(|l| picker::matches(**l, query))
            .map(|l| l.tag())
            .collect();
        matches.push(serde_json::json!({ "query": query, "found": found }));
    }
    let json = serde_json::json!({
        "note": "A language picker's order and search as the server writes them (crates/flash-server/src/i18n/picker.rs). Written by crates/flash-server/tests/it/picker_vectors_tests.rs; rerun with FLASH_WRITE_PICKER_VECTORS=1.",
        "search_from": picker::SEARCH_FROM,
        "order": order,
        "matches": matches,
    });
    serde_json::to_string_pretty(&json).unwrap() + "\n"
}

#[test]
fn the_apps_vectors_are_the_servers_picker() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/picker_vectors.json");
    let fresh = vectors();
    if std::env::var("FLASH_WRITE_PICKER_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        held.replace("\r\n", "\n") == fresh,
        "{} is stale. Rerun with FLASH_WRITE_PICKER_VECTORS=1",
        path.display()
    );
}

/// The reader's own languages come first, in the order asked, each as
/// the server answers it; the rest follow by name; nothing is listed
/// twice and nothing shipped is left out.
#[test]
fn a_picker_lists_the_readers_own_languages_first() {
    let real: Vec<UiLocale> = UiLocale::real().collect();
    let spain = flash_server::testing::spain();
    let latin_america = flash_server::testing::latin_america();
    let order = picker::order(&real, &["es-MX", "en-US"], None);
    assert_eq!(&order[..2], &[latin_america, UiLocale::EN]);
    assert_eq!(order.len(), real.len());
    let stand_in = picker::order(&real, &["ca"], Some("Europe/Madrid"));
    assert_eq!(stand_in[0], spain);
    let none = picker::order(&real, &["de-DE"], None);
    let mut by_name = none.clone();
    by_name.sort_by_key(|l| flash_store::fold::fold(l.native_name()));
    assert_eq!(none, by_name);
}

/// The website's pickers (the Language card in Settings, the signed-out
/// switch) list every language with no search yet, which serves a table
/// as short as today's. The day the table reaches the number of
/// languages people read from which a picker offers a search, this fails
/// until the website's pickers offer one too, as the app's does. The
/// pseudo-languages are left out: only a test server lists them.
#[test]
fn the_websites_pickers_need_a_search_before_the_table_needs_one() {
    let languages = UiLocale::real().count();
    assert!(
        languages < picker::SEARCH_FROM,
        "{languages} languages: give the website's language pickers a search \
         (i18n::picker::matches) before shipping this many"
    );
}
