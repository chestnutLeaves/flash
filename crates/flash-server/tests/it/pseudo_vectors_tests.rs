//! The pseudo-languages' turn (`i18n::pseudo`), written here, for the
//! pages' scripts (static/i18n.js) and by a downstream extension. Every
//! copy is held to `pseudo_vectors.json`, the server's answer for each
//! case, written here and failing while stale:
//!
//!   FLASH_WRITE_PSEUDO_VECTORS=1 cargo test -p flash-server --test it pseudo_vectors_tests::
//!
//! A case is a message as pieces (`["text", "Hi "]`, `["var", "Ana"]`):
//! each implementation sets a variable between its own marks before it
//! turns the whole.

use std::path::PathBuf;

use flash_core::locale::Pseudo;
use flash_server::i18n::pseudo::{turn, CLOSE, OPEN};

/// The messages the vectors turn: words, markup, entities, variables,
/// numbers, and what sits at their edges.
const CASES: &[&[(&str, &str)]] = &[
    &[("text", "Hello")],
    &[("text", "Hi "), ("var", "Ana"), ("text", ", see you")],
    &[("text", "See <a href=\"/decks\">your decks</a> &amp; more")],
    &[("text", "3 cards due, 21 new")],
    &[("var", "Biology"), ("text", " · "), ("var", "12")],
    &[("text", "A & B, &nbsp;C &unknown D")],
    &[("text", "Tom's “quoted” word")],
    &[("text", "<b>"), ("var", "<i>x</i>"), ("text", "</b> done")],
    &[("text", "Line one\nline two")],
    &[("text", "")],
    // A variable inside a tag: an address, which keeps no mark.
    &[
        ("text", "Write to <a href=\"mailto:"),
        ("var", "a@b.co"),
        ("text", "\">"),
        ("var", "a@b.co"),
        ("text", "</a>"),
    ],
];

/// A piece's text in the JSON, every invisible character written as its
/// escape, so the file shows what it holds.
fn visible(json: &str) -> String {
    json.chars()
        .map(|c| match c {
            '\u{200E}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{E000}'..='\u{F8FF}' => {
                format!("\\u{:04X}", c as u32)
            }
            _ => c.to_string(),
        })
        .collect()
}

fn vectors() -> String {
    let mut cases = Vec::new();
    for how in [Pseudo::Accented, Pseudo::Bidi] {
        for pieces in CASES {
            let marked: String = pieces
                .iter()
                .map(|(kind, text)| match *kind {
                    "var" => format!("{OPEN}{text}{CLOSE}"),
                    _ => (*text).to_string(),
                })
                .collect();
            cases.push(serde_json::json!({
                "how": how,
                "pieces": pieces,
                "out": turn(&marked, how),
            }));
        }
    }
    let json = serde_json::json!({
        "note": "The pseudo-languages' turn of each message (crates/flash-server/src/i18n/pseudo.rs). Written by crates/flash-server/tests/it/pseudo_vectors_tests.rs; rerun with FLASH_WRITE_PSEUDO_VECTORS=1.",
        "cases": cases,
    });
    visible(&serde_json::to_string_pretty(&json).unwrap()) + "\n"
}

#[test]
fn the_scripts_vectors_are_the_servers_turn() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pseudo_vectors.json");
    let fresh = vectors();
    if std::env::var("FLASH_WRITE_PSEUDO_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        held.replace("\r\n", "\n") == fresh,
        "{} is stale. Rerun with FLASH_WRITE_PSEUDO_VECTORS=1",
        path.display()
    );
}
