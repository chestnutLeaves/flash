use crate::common;

use axum::http::StatusCode;
use common::*;
use flash_core::queue::StudyScope;
use flash_server::service::now_ms;

fn cards(names: &[&str]) -> Vec<(String, String, Vec<String>)> {
    names
        .iter()
        .map(|n| ((*n).to_string(), format!("{n} answer"), vec![]))
        .collect()
}

#[tokio::test]
async fn a_preview_walks_what_study_could_ask_oldest_first() {
    let t = AppBuilder::new("preview-cards").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let now = now_ms();
    let made = t
        .services
        .create_cards(user, "Pharm", &cards(&["first", "second", "shelved"]), now)
        .unwrap();
    t.services
        .create_cards(user, "Pharm::Sub", &cards(&["nested"]), now)
        .unwrap();
    t.services
        .create_cards(user, "Pharm::Old", &cards(&["retired"]), now)
        .unwrap();
    t.store.suspend_cards(user, &made[2..], now).unwrap();
    let old = t
        .store
        .find_deck_by_name(user, "Pharm::Old")
        .unwrap()
        .unwrap();
    t.services.archive_deck(user, old, now).unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();

    let page = send(
        &t.app,
        get(&format!("/decks/{}/preview", pharm.0), Some(&cookie)),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    let html = page.text();
    let at = |front: &str| html.find(&format!(">{front}<"));
    let (first, second, nested) = (at("first"), at("second"), at("nested"));
    assert!(
        first.is_some() && second.is_some() && nested.is_some(),
        "{html}"
    );
    assert!(first < second && second < nested, "oldest first: {html}");
    assert!(
        html.contains(">first answer<"),
        "each card carries its answer"
    );
    assert_eq!(at("shelved"), None, "an archived card is out of study");
    assert_eq!(at("retired"), None, "so is an archived subdeck's");
    assert_eq!(html.matches("data-preview-card").count(), 3);
}

#[tokio::test]
async fn opening_a_preview_schedules_nothing() {
    let t = AppBuilder::new("preview-reads").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(user, "Pharm", &cards(&["a", "b"]), now_ms())
        .unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();
    let scope = StudyScope::deck(pharm);
    let before = t.services.queue_counts(user, &scope, now_ms()).unwrap();

    let page = send(
        &t.app,
        get(&format!("/decks/{}/preview", pharm.0), Some(&cookie)),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK);
    let after = t.services.queue_counts(user, &scope, now_ms()).unwrap();
    assert_eq!(
        (after.due, after.new_available, after.reviewed_today),
        (before.due, before.new_available, before.reviewed_today)
    );
    assert!(
        page.text().contains("2 cards are ready to study now."),
        "the end says what is left to study"
    );
}

#[tokio::test]
async fn with_nothing_left_today_the_end_says_when_the_next_come() {
    let t = AppBuilder::new("preview-next").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let now = now_ms();
    t.services
        .create_cards(user, "Pharm", &cards(&["a", "b", "c"]), now)
        .unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();
    t.services
        .set_deck_limits(user, pharm, Some(1), None)
        .unwrap();
    let session = t
        .services
        .start_session(user, StudyScope::deck(pharm), now)
        .unwrap();
    let card = session.first_card.unwrap().card_id;
    t.services
        .submit_review(
            user,
            session.session_id,
            card,
            flash_core::Rating::Easy,
            flash_server::service::ReviewOrigin::new("web", None),
            now,
        )
        .unwrap();

    let html = send(
        &t.app,
        get(&format!("/decks/{}/preview", pharm.0), Some(&cookie)),
    )
    .await
    .text();
    assert!(
        html.contains("Your next review is tomorrow: 1 card."),
        "the deck's own limit, not the account's 20: {html}"
    );
    assert_eq!(
        html.matches("data-preview-card").count(),
        3,
        "the preview still goes through every card"
    );
}

#[tokio::test]
async fn a_deck_with_nothing_to_preview_says_so() {
    let t = AppBuilder::new("preview-empty").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let made = t
        .services
        .create_cards(user, "Pharm", &cards(&["only"]), now_ms())
        .unwrap();
    t.store.suspend_cards(user, &made, now_ms()).unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();

    let html = send(
        &t.app,
        get(&format!("/decks/{}/preview", pharm.0), Some(&cookie)),
    )
    .await
    .text();
    assert!(
        html.contains("This deck has no cards to preview."),
        "{html}"
    );
    assert!(!html.contains("data-preview-card"));
    assert!(!html.contains("data-preview-end"), "nothing to end");
}

#[tokio::test]
async fn another_accounts_deck_is_not_found() {
    let t = AppBuilder::new("preview-owner").build();
    let (owner, _) = signed_in(&t.store, "Owner", "owner@example.com");
    let (_, stranger) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(owner, "Private", &cards(&["secret"]), now_ms())
        .unwrap();
    let deck = t
        .store
        .find_deck_by_name(owner, "Private")
        .unwrap()
        .unwrap();

    let page = send(
        &t.app,
        get(&format!("/decks/{}/preview", deck.0), Some(&stranger)),
    )
    .await;
    assert_eq!(page.status, StatusCode::NOT_FOUND);
    assert!(!page.text().contains("secret"));
}

#[tokio::test]
async fn every_deck_menu_offers_the_preview() {
    let t = AppBuilder::new("preview-menu").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let made = t
        .services
        .create_cards(user, "Pharm", &cards(&["a", "b", "c"]), now_ms())
        .unwrap();
    t.services
        .create_cards(user, "Pharm::Sub", &cards(&["d"]), now_ms())
        .unwrap();
    t.store.suspend_cards(user, &made[..1], now_ms()).unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();
    let link = format!("href=\"/decks/{}/preview\"", pharm.0);
    for page in [
        "/".to_string(),
        "/decks".to_string(),
        format!("/decks/{}", pharm.0),
    ] {
        let html = send(&t.app, get(&page, Some(&cookie))).await.text();
        assert!(html.contains(&link), "{page} offers the preview: {html}");
        assert!(
            html.contains("Preview all 3"),
            "{page} counts what it goes through: two left here, one below"
        );
        assert!(html.contains("Spaced repetition unaffected"), "{page}");
    }
}

#[tokio::test]
async fn a_deck_with_no_cards_offers_no_preview() {
    let t = AppBuilder::new("preview-menu-empty").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let deck = t.store.create_deck(user, "Empty", "", now_ms()).unwrap();
    let html = send(&t.app, get("/decks", Some(&cookie))).await.text();
    assert!(html.contains("Empty"));
    assert!(
        !html.contains(&format!("/decks/{}/preview", deck.0)),
        "{html}"
    );
}
