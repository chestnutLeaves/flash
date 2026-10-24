//! Text in any script as the store keeps it: one Unicode form for every
//! letter (NFC), and a block's direction and language kept when they are
//! ones a browser reads.

use flash_store::richtext::{html_to_text, sanitize_with_media};
use flash_store::Store;

const NOW: i64 = 1_700_000_000_000;

#[test]
fn a_letter_is_kept_in_one_form() {
    // "é" as "e" and an accent, from a PDF or a phone, is kept composed.
    let html = sanitize_with_media("<p>cafe\u{301}</p>");
    assert_eq!(html.as_str(), "<p>café</p>");
    assert_eq!(html_to_text("<p>cafe\u{301}</p>"), "café");
    // An entity that spells the accent ends up composed too.
    assert_eq!(sanitize_with_media("e&#x301;").as_str(), "é");
    // A document holding U+0338, which composes with `>`, is left alone
    // rather than risk a tag.
    let slash = sanitize_with_media("<b>a</b>\u{338}");
    assert!(slash.as_str().contains("</b>\u{338}"), "{}", slash.as_str());

    let store = Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    let card = flash_core::validate_card_text("n\u{303}andu\u{301}", "cafe\u{301}").unwrap();
    assert_eq!((card.front.as_str(), card.back.as_str()), ("ñandú", "café"));
    assert_eq!(
        flash_core::deck_path::normalise("Espan\u{303}ol::Le\u{301}xico").as_deref(),
        Some("Español::Léxico")
    );
    let deck = store.create_deck(user, "Words", "", NOW).unwrap();
    store
        .create_cards(user, deck, &[(card, vec![])], None, NOW)
        .unwrap();
    assert_eq!(
        store
            .list_cards(user, None, Some("ñandú"), 5)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn direction_and_language_survive_when_a_browser_reads_them() {
    let html = sanitize_with_media(
        r#"<p dir="rtl" lang="ar">كتاب</p><div dir="AUTO" lang="ja-jp">猫</div><span lang="zh-Hant-TW">貓</span>"#,
    );
    assert_eq!(
        html.as_str(),
        r#"<p dir="rtl" lang="ar">كتاب</p><div dir="auto" lang="ja-JP">猫</div><span lang="zh-TW">貓</span>"#
    );
    // Anything else is dropped: a direction no browser reads, a language
    // that is not one, and a direction on a run of bold (a language may
    // sit on any element, checked like the rest).
    let html = sanitize_with_media(
        r#"<p dir="sideways" lang="javascript:alert(1)">a</p><b dir="rtl" lang="AR">b</b><p lang="">c</p>"#,
    );
    assert_eq!(html.as_str(), r#"<p>a</p><b lang="ar">b</b><p>c</p>"#);
}
