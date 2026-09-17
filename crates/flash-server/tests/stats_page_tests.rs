//! The stats page over HTTP: the heatmap's year picker appears once the
//! history spans more than one year, and `?year=` shows that calendar
//! year as the heading with its link marked current.

mod common;

use common::*;
use flash_core::Rating;
use flash_server::service::now_ms;
use flash_store::Store;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn seed_review(store: &Store, user: flash_core::UserId, ts: i64) {
    let deck = store
        .find_deck_by_name(user, "D")
        .unwrap()
        .unwrap_or_else(|| store.create_deck(user, "D", "", ts).unwrap());
    let card = store
        .create_cards(
            user,
            deck,
            &[(flash_core::validate_card_text("f", "b").unwrap(), vec![])],
            None,
            ts,
        )
        .unwrap()[0];
    let scheduler = flash_core::Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(user, card).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, ts).unwrap();
    store
        .record_review(user, card, &outcome, ts, "web", None, None)
        .unwrap();
}

#[tokio::test]
async fn the_year_picker_appears_with_more_than_a_year_of_history() {
    let t = AppBuilder::new("stats-page").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");

    // One year of history: the past year, no picker.
    seed_review(&t.store, user, now_ms());
    let reply = send(&t.app, get("/stats", Some(&cookie))).await;
    assert_eq!(reply.status, 200);
    let page = reply.text();
    assert!(page.contains("Past year"), "{page}");
    assert!(!page.contains("hm-years"), "{page}");

    // A review two years back: links for each year, the past year last.
    seed_review(&t.store, user, now_ms() - 2 * 365 * DAY);
    let page = send(&t.app, get("/stats", Some(&cookie))).await.text();
    assert!(page.contains("hm-years"), "{page}");
    let year = jiff::Timestamp::from_millisecond(now_ms() - 2 * 365 * DAY)
        .unwrap()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .year();
    assert!(
        page.contains(&format!("href=\"/stats?year={year}\"")),
        "{page}"
    );
    assert!(
        page.contains("href=\"/stats\" aria-current=\"true\">Past year"),
        "{page}"
    );

    // That year's view: its heading, its link current, no future cells.
    let page = send(&t.app, get(&format!("/stats?year={year}"), Some(&cookie)))
        .await
        .text();
    assert!(page.contains(&format!("<h2>{year}</h2>")), "{page}");
    assert!(
        page.contains(&format!(
            "href=\"/stats?year={year}\" aria-current=\"true\""
        )),
        "{page}"
    );
    assert!(!page.contains("data-future"), "{page}");
    assert!(!page.contains("data-today"), "{page}");
}
