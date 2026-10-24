//! A variable's isolation (`i18n::bidi`), written here, for the pages'
//! scripts (static/i18n.js) and by a downstream extension. Every copy is
//! held to `bidi_vectors.json`, the server's answer for each case,
//! written here and failing while stale:
//!
//!   FLASH_WRITE_BIDI_VECTORS=1 cargo test -p flash-server --test it bidi_vectors_tests::

use std::path::PathBuf;

use flash_server::i18n::bidi::{isolate, right_to_left};

/// Values at the edges of the rule: none, left to right, each
/// right-to-left block's first and last letters, mixed, digits, marks.
const VALUES: &[&str] = &[
    "",
    "Biology",
    "Biología 2",
    "123",
    "📚 Pharm",
    "\u{0590}",
    "שלום",
    "مرحبا",
    "\u{08FF}",
    "\u{0900}",
    "Bio 101 עברית",
    "\u{200E}x",
    "\u{200F}x",
    "\u{FB1C}",
    "\u{FB1D}",
    "\u{FDFF}",
    "\u{FE00}",
    "\u{FE70}",
    "\u{FEFF}",
    "\u{FF00}",
    "\u{107FF}",
    "\u{10800}",
    "\u{10900}",
    "\u{10FFF}",
    "\u{11000}",
    "\u{1E7FF}",
    "\u{1E800}",
    "\u{1E900}",
    "\u{1EFFF}",
    "\u{1F000}",
];

/// A value's text in the JSON, every invisible or private character
/// written as its escape, so the file shows what it holds.
fn visible(json: &str) -> String {
    json.chars()
        .map(|c| match c {
            '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{E000}'..='\u{F8FF}' => format!("\\u{:04X}", c as u32),
            _ => c.to_string(),
        })
        .collect()
}

fn vectors() -> String {
    let mut cases = Vec::new();
    for rtl in [false, true] {
        for value in VALUES {
            cases.push(serde_json::json!({
                "rtl": rtl,
                "value": value,
                "out": isolate(value, rtl),
            }));
        }
    }
    let json = serde_json::json!({
        "note": "Each value as a message sets it for a reader of a left-to-right (rtl false) or right-to-left (rtl true) language (crates/flash-server/src/i18n/bidi.rs). Written by crates/flash-server/tests/it/bidi_vectors_tests.rs; rerun with FLASH_WRITE_BIDI_VECTORS=1.",
        "cases": cases,
    });
    visible(&serde_json::to_string_pretty(&json).unwrap()) + "\n"
}

#[test]
fn the_scripts_vectors_are_the_servers_isolation() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/bidi_vectors.json");
    let fresh = vectors();
    if std::env::var("FLASH_WRITE_BIDI_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        held.replace("\r\n", "\n") == fresh,
        "{} is stale. Rerun with FLASH_WRITE_BIDI_VECTORS=1",
        path.display()
    );
}

/// The rule's blocks are Unicode's own: every letter of bidi class R or
/// AL lies in them, and none of class L does. Checked against the
/// strong letters of each right-to-left script and their neighbours.
#[test]
fn the_right_to_left_blocks_hold_the_right_to_left_letters() {
    for rtl in [
        "שלום",
        "مرحبا",
        "ܫܠܡܐ",
        "ހެލޯ",
        "ߒߞߏ",
        "ࠀ",
        "ࡀ",
        "𐤀",
        "𞤀",
        "ﷲ",
        "ﻻ",
    ] {
        assert!(right_to_left(rtl), "{rtl}");
    }
    for ltr in [
        "Hello",
        "Привет",
        "नमस्ते",
        "こんにちは",
        "안녕",
        "ภาษา",
        "123",
        "",
        "📚",
    ] {
        assert!(!right_to_left(ltr), "{ltr}");
    }
}
