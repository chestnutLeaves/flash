use crate::common;

use axum::http::StatusCode;
use common::*;
use flash_server::service::now_ms;

fn number_after(html: &str, key: &str) -> i64 {
    let rest = &html[html.find(key).unwrap_or_else(|| panic!("{key} in {html}")) + key.len()..];
    rest.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn note_body(kind: &str, front: &str, back: &str) -> String {
    format!(
        "note_type={kind}&front_html={}&back_html={}&tags=",
        enc(front),
        enc(back)
    )
}

#[tokio::test]
async fn edit_from_study_saves_without_a_reload_and_the_card_shows_again() {
    let t = AppBuilder::new("study-edit").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[
                ("Nerve to the deltoid?".into(), "Axilary".into(), vec![]),
                ("Nerve to biceps?".into(), "Musculocutaneous".into(), vec![]),
            ],
            now_ms(),
        )
        .unwrap();

    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    assert!(html.contains("data-study-edit-open"), "{html}");
    assert!(html.contains(r#"id="editor-dialog" data-editor-after="study""#));
    assert!(html.contains("/static/editor.js?v="));
    let session = number_after(&html, "data-session=");
    let card = number_after(&html, "data-card-id=");
    assert!(
        html.contains("data-remaining=\"2\" data-total=\"2\""),
        "{html}"
    );
    assert!(!html.contains("data-revealed"));

    let dialog = send(
        &t.app,
        get(&format!("/cards/{card}/edit?after=study"), Some(&cookie)),
    )
    .await;
    assert_eq!(dialog.status, StatusCode::OK);
    let dialog = dialog.text();
    assert!(
        dialog.contains(&format!(r#"hx-post="/cards/{card}/edit?after=study""#)),
        "{dialog}"
    );

    let saved = send(
        &t.app,
        form_post(
            &format!("/cards/{card}/edit?after=study"),
            Some(&cookie),
            &note_body("basic", "Which nerve? (edited)", "Axillary"),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.text());
    assert_eq!(saved.header("hx-trigger"), Some("flash:card-saved"));
    assert!(saved.header("hx-refresh").is_none());

    let again = send(
        &t.app,
        get(
            &format!("/study/card?session={session}&card={card}&remaining=2&total=2&shown=1234"),
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK);
    let again = again.text();
    assert!(again.contains("(edited)"), "{again}");
    assert!(
        again.contains(&format!("data-card-id=\"{card}\"")),
        "{again}"
    );
    assert!(again.contains("data-shown=\"1234\""), "{again}");
    assert!(again.contains("2 left"), "{again}");
    assert!(
        again.contains("reveal-btn"),
        "the front, as it was: {again}"
    );

    let answer = send(
        &t.app,
        get(
            &format!(
                "/study/card?session={session}&card={card}&remaining=2&total=2&shown=1234&revealed=true"
            ),
            Some(&cookie),
        ),
    )
    .await
    .text();
    assert!(answer.contains("Axillary"), "{answer}");
    assert!(answer.contains("data-revealed=\"1\""), "{answer}");
    assert!(answer.contains("grade-1"), "{answer}");
    assert!(
        answer.contains(&format!(r#""session": {session}"#)),
        "{answer}"
    );

    let refused = send(
        &t.app,
        form_post(
            &format!("/cards/{card}/edit?after=study"),
            Some(&cookie),
            &note_body("cloze", "no blanks", ""),
        ),
    )
    .await;
    assert_eq!(refused.status, StatusCode::OK);
    let refused = refused.text();
    assert!(refused.contains("editor-error"), "{refused}");
    assert!(
        refused.contains(&format!(r#"hx-post="/cards/{card}/edit?after=study""#)),
        "{refused}"
    );

    let reopened = send(
        &t.app,
        get(&format!("/cards/{card}/edit?after=study"), Some(&cookie)),
    )
    .await
    .text();
    let revision = number_after(&reopened, &format!("/cards/{card}/restore/"));
    assert!(
        reopened.contains(&format!("/cards/{card}/restore/{revision}?after=study")),
        "{reopened}"
    );
}

#[tokio::test]
async fn a_card_the_edit_took_away_gives_way_to_the_sessions_next() {
    let t = AppBuilder::new("study-edit-gone").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let deck = t.store.create_deck(user, "Vocab", "", now_ms()).unwrap();
    let made = send(
        &t.app,
        form_post(
            &format!("/decks/{}/notes", deck.0),
            Some(&cookie),
            &note_body("basic_reversed", "gato", "cat"),
        ),
    )
    .await;
    assert!(made.status.is_redirection() || made.status.is_success());
    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    let session = number_after(&html, "data-session=");
    let cards = t.store.list_cards(user, Some(deck), None, 5).unwrap();
    assert_eq!(cards.len(), 2);
    let (forward, reverse) = (cards[0].id.0, cards[1].id.0);

    let saved = send(
        &t.app,
        form_post(
            &format!("/cards/{forward}/edit?after=study"),
            Some(&cookie),
            &note_body("basic", "gato", "cat"),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.text());
    let kept = t.store.list_cards(user, Some(deck), None, 5).unwrap();
    assert_eq!(kept.len(), 1, "{kept:?}");
    let (gone, left) = if kept[0].id.0 == forward {
        (reverse, forward)
    } else {
        (forward, reverse)
    };

    let again = send(
        &t.app,
        get(
            &format!("/study/card?session={session}&card={gone}&remaining=2&total=2&shown=1"),
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK);
    let again = again.text();
    assert!(
        again.contains(&format!("data-card-id=\"{left}\"")),
        "{again}"
    );
    assert!(again.contains("1 left"), "the queue as it stands: {again}");

    t.services
        .delete_card(user, flash_core::CardId(left), now_ms())
        .unwrap();
    let done = send(
        &t.app,
        get(
            &format!("/study/card?session={session}&card={left}&remaining=1&total=2&shown=1"),
            Some(&cookie),
        ),
    )
    .await
    .text();
    assert!(done.contains("study-done"), "{done}");
    assert!(!done.contains("data-card-id"), "{done}");
}

#[tokio::test]
async fn someone_elses_session_shows_no_card() {
    let t = AppBuilder::new("study-edit-stranger").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let (_, stranger) = signed_in(&t.store, "Other", "other@example.com");
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[("Nerve to the deltoid?".into(), "Axillary".into(), vec![])],
            now_ms(),
        )
        .unwrap();
    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    let session = number_after(&html, "data-session=");
    let card = number_after(&html, "data-card-id=");
    let reply = send(
        &t.app,
        get(
            &format!("/study/card?session={session}&card={card}&remaining=1&total=1"),
            Some(&stranger),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND, "{}", reply.text());
    assert!(!reply.text().contains("Nerve to the deltoid?"));
}
