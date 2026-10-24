//! The advanced card editor over HTTP: the notes create route (every note
//! type and validation), the edit dialog partial and its save, the
//! editor media upload, and deck-page pagination.

use crate::common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_server::service::now_ms;
use flash_store::Store;

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 1, 2, 3,
];

struct Harness {
    app: axum::Router,
    store: Arc<Store>,
    user: flash_core::UserId,
    session_cookie: String,
}

fn harness(tag: &str) -> Harness {
    let t = AppBuilder::new(&format!("editor-{tag}")).build();
    let (user, session_cookie) = signed_in(&t.store, "Ed", "ed@example.com");
    Harness {
        app: t.app,
        store: t.store,
        user,
        session_cookie,
    }
}

fn deck(h: &Harness) -> flash_core::DeckId {
    h.store.create_deck(h.user, "Ed", "", now_ms()).unwrap()
}

fn seed_cards(h: &Harness, deck: flash_core::DeckId, n: usize) {
    let cards: Vec<_> = (0..n)
        .map(|i| {
            (
                flash_core::validate_card_text(&format!("front {i:03}"), "b").unwrap(),
                Vec::new(),
            )
        })
        .collect();
    h.store
        .create_cards(h.user, deck, &cards, None, now_ms())
        .unwrap();
}

async fn send(h: &Harness, req: Request<Body>) -> (StatusCode, Vec<(String, String)>, String) {
    let r = common::send(&h.app, req).await;
    let text = r.text();
    (r.status, r.headers, text)
}

/// Signed-in form post.
fn form_post(h: &Harness, path: &str, body: &str) -> Request<Body> {
    common::form_post(path, Some(&h.session_cookie), body)
}

/// Signed-in GET.
fn get(h: &Harness, path: &str) -> Request<Body> {
    common::get(path, Some(&h.session_cookie))
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn note_body(kind: &str, front: &str, back: &str, tags: &str) -> String {
    format!(
        "note_type={kind}&front_html={}&back_html={}&tags={}",
        enc(front),
        enc(back),
        enc(tags)
    )
}

fn multipart(boundary: &str, filename: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

// ---- create via the notes route ----

#[tokio::test]
async fn notes_route_creates_every_type_and_redirects_home() {
    let h = harness("types");
    let deck = deck(&h);
    let cases = [
        ("basic", "<b>Q</b>", "A", 1),
        ("basic_reversed", "Q", "A", 2),
        ("basic_typed", "Q", "A", 1),
        ("cloze", "{{c1::one}} {{c2::two}} {{c3::three}}", "", 3),
    ];
    let mut expected = 0;
    for (kind, f, b, n) in cases {
        let (status, headers, _) = send(
            &h,
            form_post(
                &h,
                &format!("/decks/{deck}/notes"),
                &note_body(kind, f, b, "tag"),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{kind}");
        assert_eq!(
            header_of(&headers, "location"),
            Some(format!("/decks/{deck}").as_str())
        );
        expected += n;
        assert_eq!(
            h.store.count_cards(h.user, Some(deck), None).unwrap(),
            expected,
            "{kind}"
        );
    }
    let (status, _, body) = send(&h, get(&h, &format!("/decks/{deck}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("cloze 3") && body.contains("reversed") && body.contains("typed"),
        "{body}"
    );
    assert!(body.contains("<b>Q</b>"), "rich row renders the HTML");
    assert!(
        body.contains("data-editor-toggle"),
        "advanced toggle present"
    );
    assert!(body.contains("/static/editor.js"), "editor script loaded");
}

#[tokio::test]
async fn notes_route_refusals_land_as_banners() {
    let h = harness("refuse");
    let deck = deck(&h);
    let cases = [
        ("cloze", "no blanks", "", "err=nocloze", "cloze note needs"),
        ("basic", "", "A", "err=empty", "both sides"),
        (
            "basic",
            "<img src=\"/media/999\">",
            "A",
            "err=media",
            "isn&#39;t yours",
        ),
    ];
    for (kind, f, b, query, copy) in cases {
        let (status, headers, _) = send(
            &h,
            form_post(
                &h,
                &format!("/decks/{deck}/notes"),
                &note_body(kind, f, b, ""),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{kind} {f:?}");
        let location = header_of(&headers, "location").unwrap().to_string();
        assert_eq!(location, format!("/decks/{deck}?{query}"));
        let (_, _, body) = send(&h, get(&h, &location)).await;
        assert!(body.contains(copy), "{query}: {body}");
    }
    assert_eq!(h.store.count_cards(h.user, Some(deck), None).unwrap(), 0);
    // An unknown note type is a plain 400, not a banner.
    let (status, _, _) = send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("weird", "a", "b", ""),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn quick_add_still_works_without_the_editor() {
    let h = harness("quick");
    let deck = deck(&h);
    // Exactly what the JS-less form posts (plus the dormant editor's fields).
    let (status, headers, _) = send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/cards"),
            "front=plain+q&back=plain+a&tags=x&note_type=basic&front_html=&back_html=",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(format!("/decks/{deck}").as_str())
    );
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);
    assert_eq!((card.front.as_str(), card.note_id), ("plain q", None));
}

// ---- edit dialog ----

#[tokio::test]
async fn edit_dialog_prefills_and_saves_rich_html() {
    let h = harness("edit");
    let deck = deck(&h);
    send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("basic", "<b>bold</b> q", "a", "t1"),
        ),
    )
    .await;
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);

    let (status, _, body) = send(&h, get(&h, &format!("/cards/{}/edit", card.id))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("data-editor-overlay"), "{body}");
    assert!(
        body.contains("contenteditable=\"true\" dir=\"auto\" data-field=\"front\" data-placeholder=\"Question\" spellcheck=\"true\"><b>bold</b> q</div>"),
        "prefilled unescaped inside the field: {body}"
    );
    assert!(
        body.contains("name=\"front_html\" value=\"&#60;b&#62;bold&#60;/b&#62; q\""),
        "escaped inside the hidden input: {body}"
    );
    assert!(body.contains("value=\"t1\""));
    assert!(body.contains("<option value=\"basic\" selected>"));

    // Save with a highlight, reversed type, new tags.
    let (status, headers, body) = send(
        &h,
        form_post(
            &h,
            &format!("/cards/{}/edit", card.id),
            &note_body(
                "basic_reversed",
                "<span class=\"hl-teal\">teal</span> <i>q2</i>",
                "a2",
                "t2, T3",
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    assert_eq!(header_of(&headers, "hx-refresh"), Some("true"));
    let cards = h.store.list_cards(h.user, Some(deck), None, 5).unwrap();
    assert_eq!(cards.len(), 2, "reversed now");
    let same = cards.iter().find(|c| c.id == card.id).unwrap();
    assert_eq!(
        same.front_html.as_deref(),
        Some("<span class=\"hl-teal\">teal</span> <i>q2</i>")
    );
    assert_eq!(same.front, "teal q2");
    assert_eq!(same.tags, vec!["t2", "t3"]);
    assert!(same.note_id.is_some());

    // A refusal re-renders the dialog with the message, keeping the input.
    let (status, headers, body) = send(
        &h,
        form_post(
            &h,
            &format!("/cards/{}/edit", card.id),
            &note_body("cloze", "no blanks", "", ""),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(header_of(&headers, "hx-refresh").is_none());
    assert!(
        body.contains("editor-error") && body.contains("cloze deletion"),
        "{body}"
    );
    assert!(body.contains("<option value=\"cloze\" selected>"));
    assert!(
        body.contains(">no blanks</div>"),
        "typed text echoed: {body}"
    );
}

#[tokio::test]
async fn edit_routes_are_ownership_scoped() {
    let h = harness("own");
    let deck = deck(&h);
    seed_cards(&h, deck, 1);
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);
    // Another user's session sees nothing.
    let other = harness("own2");
    let (status, _, _) = send(&other, get(&other, &format!("/cards/{}/edit", card.id))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Anonymous is refused.
    let (status, _, _) = send(
        &h,
        Request::get(format!("/cards/{}/edit", card.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(status, StatusCode::OK);
    // A POST without Origin is refused by the same-origin guard.
    let (status, _, _) = send(
        &h,
        Request::post(format!("/cards/{}/edit", card.id))
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(note_body("basic", "x", "y", "")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn history_lists_earlier_versions_and_restore_puts_one_back() {
    let h = harness("history");
    let deck = deck(&h);
    send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("basic", "<b>first</b> q", "a", "t1"),
        ),
    )
    .await;
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);
    let edit = format!("/cards/{}/edit", card.id);
    let (_, _, body) = send(&h, get(&h, &edit)).await;
    assert!(!body.contains("History"), "nothing edited yet: {body}");

    let (status, _, _) = send(
        &h,
        form_post(&h, &edit, &note_body("basic", "second q", "a", "t2")),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let note = h.store.note_for_card(h.user, card.id).unwrap().unwrap().id;
    let revision = h.store.list_note_revisions(h.user, note).unwrap()[0].id;
    let restore = format!("/cards/{}/restore/{}", card.id, revision.0);

    let (_, _, body) = send(&h, get(&h, &edit)).await;
    assert!(body.contains("History (1 earlier version)"), "{body}");
    assert!(
        body.contains("first q") && body.contains("before an edit here"),
        "{body}"
    );
    assert!(body.contains(&format!("hx-post=\"{restore}\"")), "{body}");

    // Another user cannot restore it, and a post without Origin is
    // refused by the same-origin guard.
    let other = harness("history2");
    let (status, _, _) = send(&other, form_post(&other, &restore, "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = send(
        &h,
        Request::post(restore.clone())
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        h.store.get_card(h.user, card.id).unwrap().unwrap().front,
        "second q"
    );

    // Restore re-renders the dialog over the earlier text, says so, and
    // History now holds the text it replaced.
    let (status, _, body) = send(&h, form_post(&h, &restore, "")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Restored the earlier version."), "{body}");
    assert!(body.contains("data-editor-changed"), "{body}");
    assert!(
        body.contains("><b>first</b> q</div>"),
        "restored into the field: {body}"
    );
    assert!(body.contains("value=\"t1\""), "{body}");
    assert!(
        body.contains("second q") && body.contains("before a restore"),
        "{body}"
    );
    let restored = h.store.get_card(h.user, card.id).unwrap().unwrap();
    assert_eq!(restored.front_html.as_deref(), Some("<b>first</b> q"));
    assert_eq!(restored.tags, vec!["t1"]);
}

// ---- media upload ----

#[tokio::test]
async fn editor_upload_returns_json_and_serves_the_file() {
    let h = harness("upload");
    let boundary = "flashboundary";
    let (status, _, body) = send(
        &h,
        Request::post("/media")
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(multipart(boundary, "pic.png", PNG)))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["kind"], "image");
    let id = json["id"].as_i64().unwrap();
    let (status, headers, _) = send(&h, get(&h, &format!("/media/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header_of(&headers, "content-type"), Some("image/png"));

    // The upload is usable in a note and gets linked to the card.
    let deck = deck(&h);
    let (status, _, _) = send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("basic", &format!("<img src=\"/media/{id}\">"), "A", ""),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);
    assert_eq!(card.front, "[image]");
    assert_eq!(
        h.store.media_for_export_in(h.user, None).unwrap()[0].id.0,
        id
    );

    // Scriptable formats die at the gate; the answer is JSON too.
    let (status, _, body) = send(
        &h,
        Request::post("/media")
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(multipart(
                boundary,
                "evil.svg",
                b"<svg onload=alert(1)/>",
            )))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("\"error\""), "{body}");
    // No file field at all.
    let (status, _, _) = send(
        &h,
        Request::post("/media")
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(format!("--{boundary}--\r\n")))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---- pagination ----

#[tokio::test]
async fn deck_page_paginates_at_25() {
    let h = harness("pages");
    let deck = deck(&h);
    seed_cards(&h, deck, 30);
    let (status, _, body) = send(&h, get(&h, &format!("/decks/{deck}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.matches("<tr id=\"card-").count(), 25);
    assert!(body.contains("Page 1 of 2 · 30 cards"), "{body}");
    assert!(
        body.contains("front 029") && !body.contains("front 004"),
        "newest first"
    );
    assert!(body.contains("?page=2\""));

    let (_, _, body) = send(&h, get(&h, &format!("/decks/{deck}?page=2"))).await;
    assert_eq!(body.matches("<tr id=\"card-").count(), 5);
    assert!(body.contains("front 000") && !body.contains("front 005"));
    assert!(body.contains("Page 2 of 2"));

    // The htmx partial: search + page, out-of-range clamps.
    let (_, _, body) = send(
        &h,
        get(&h, &format!("/decks/{deck}/cards?q=front+00&page=1")),
    )
    .await;
    assert_eq!(body.matches("<tr id=\"card-").count(), 10);
    assert!(!body.contains("class=\"pager\""), "one page: no pager");
    let (_, _, body) = send(&h, get(&h, &format!("/decks/{deck}/cards?page=99"))).await;
    assert_eq!(body.matches("<tr id=\"card-").count(), 5);
    assert!(body.contains("Page 2 of 2"));
    let (_, _, body) = send(&h, get(&h, &format!("/decks/{deck}/cards?q=zzz"))).await;
    assert!(body.contains("No cards match"), "{body}");

    // A deck with few cards has no pager at all.
    let small = h.store.create_deck(h.user, "Small", "", now_ms()).unwrap();
    seed_cards(&h, small, 3);
    let (_, _, body) = send(&h, get(&h, &format!("/decks/{small}"))).await;
    assert!(!body.contains("class=\"pager\""));
    assert_eq!(body.matches("<tr id=\"card-").count(), 3);
}

#[tokio::test]
async fn the_card_page_is_the_whole_editor_and_a_save_stays_on_it() {
    let h = harness("card-page");
    let deck = deck(&h);
    send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("basic", "<b>Deltoid</b> nerve?", "Axillary", "arm"),
        ),
    )
    .await;
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);

    let (status, _, body) = send(&h, get(&h, &format!("/cards/{}?via=panel", card.id))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("<h1>Edit card</h1>"), "{body}");
    assert!(
        body.contains(&format!(r#"<a href="/decks/{deck}">Ed</a>"#)),
        "{body}"
    );
    assert!(body.contains("data-editor-toolbar") || body.contains("editor-toolbar"));
    assert!(body.contains("><b>Deltoid</b> nerve?</div>"), "{body}");
    assert!(
        !body.contains("data-editor-overlay"),
        "the page, not a dialog"
    );
    assert!(!body.contains("data-editor-close"), "{body}");
    assert!(
        body.contains(&format!(
            r#"<a class="fl-btn fl-btn-secondary" href="/decks/{deck}">Cancel</a>"#
        )),
        "{body}"
    );
    assert!(
        body.contains(&format!(r#"hx-post="/cards/{}/edit?after=page""#, card.id)),
        "{body}"
    );
    assert!(body.contains("/static/editor.js?v="));

    let (status, headers, body) = send(
        &h,
        form_post(
            &h,
            &format!("/cards/{}/edit?after=page", card.id),
            &note_body("basic", "<b>Deltoid</b> nerve?", "Axillary (C5-C6)", "arm"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(header_of(&headers, "hx-refresh").is_none());
    assert!(
        body.contains(
            r#"<div class="editor-notice" role="status" data-editor-changed>Saved.</div>"#
        ),
        "{body}"
    );
    assert!(body.contains(">Axillary (C5-C6)</div>"), "{body}");
    assert!(body.contains("History (1 earlier version)"), "{body}");
    assert!(!body.contains("data-editor-overlay"), "{body}");
    let stored = h.store.list_cards(h.user, Some(deck), None, 5).unwrap();
    assert_eq!(stored[0].back, "Axillary (C5-C6)");

    let (status, _, body) = send(
        &h,
        form_post(
            &h,
            &format!("/cards/{}/edit?after=page", card.id),
            &note_body("cloze", "no blanks", "", ""),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("editor-error"), "{body}");
    assert!(body.contains(">no blanks</div>"), "{body}");
    assert!(!body.contains("data-editor-overlay"), "{body}");
}

#[tokio::test]
async fn the_card_page_signs_a_stranger_in_and_keeps_others_out() {
    let h = harness("card-page-access");
    let deck = deck(&h);
    seed_cards(&h, deck, 1);
    let card = h
        .store
        .list_cards(h.user, Some(deck), None, 5)
        .unwrap()
        .remove(0);

    let r = common::send(
        &h.app,
        common::get(&format!("/cards/{}?via=panel", card.id), None),
    )
    .await;
    assert!(r.status.is_redirection(), "{}", r.status);
    assert_eq!(
        r.location(),
        format!("/login?next=/cards/{}%3Fvia%3Dpanel", card.id)
    );

    let (_, other) = signed_in(&h.store, "Other", "other@example.com");
    let r = common::send(
        &h.app,
        common::get(&format!("/cards/{}", card.id), Some(&other)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text());
}

#[tokio::test]
async fn a_card_deleted_from_a_note_stays_gone_after_an_edit() {
    let h = harness("deleted-slot");
    let deck = deck(&h);
    send(
        &h,
        form_post(
            &h,
            &format!("/decks/{deck}/notes"),
            &note_body("basic_reversed", "hola", "hello", ""),
        ),
    )
    .await;
    let mut cards = h.store.list_cards(h.user, Some(deck), None, 5).unwrap();
    cards.sort_by_key(|c| c.ord);
    h.store.delete_card(h.user, cards[1].id, now_ms()).unwrap();

    let (status, _, body) = send(
        &h,
        form_post(
            &h,
            &format!("/cards/{}/edit", cards[0].id),
            &note_body("basic_reversed", "hola!", "hello", ""),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
    let left = h.store.list_cards(h.user, Some(deck), None, 5).unwrap();
    assert_eq!(left.len(), 1, "the deleted reverse card stays gone");
    assert_eq!(left[0].front, "hola!");
}
