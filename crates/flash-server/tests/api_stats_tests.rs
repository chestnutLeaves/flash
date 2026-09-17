//! Stats over the API: the same numbers the web page renders, shaped
//! for drawing (53 heat columns of 7, 30 bars, 7 upcoming days).

mod common;

use axum::http::StatusCode;
use common::*;
use flash_core::Rating;
use flash_server::service::{now_ms, ReviewOrigin};

#[tokio::test]
async fn stats_reflect_reviews() {
    let t = AppBuilder::new("api-stats").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, flashtester);

    // Empty account: zeros, null retention, full-shape arrays.
    let reply = send(&t.app, json_get("/api/v1/stats", Some(&bearer))).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let s = reply.json();
    assert_eq!(s["total_reviews"], 0);
    assert_eq!(s["retention_pct"], serde_json::Value::Null);
    assert_eq!(s["heat_weeks"].as_array().unwrap().len(), 53);
    assert_eq!(s["heat_weeks"][0].as_array().unwrap().len(), 7);
    assert_eq!(s["days"].as_array().unwrap().len(), 30);
    assert_eq!(s["upcoming"].as_array().unwrap().len(), 7);
    assert!(s["heat_weeks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|w| w.as_array().unwrap())
        .any(|c| c["today"] == true));

    // One study session: the counters move.
    let now = now_ms();
    t.services.create_deck(flashtester, "Bio", "", now).unwrap();
    t.services
        .create_cards(
            flashtester,
            "Bio",
            &[
                ("Cell".to_string(), "Unit of life".to_string(), vec![]),
                ("ATP".to_string(), "Energy".to_string(), vec![]),
            ],
            now,
        )
        .unwrap();
    let session = t
        .services
        .start_session(flashtester, flash_core::queue::StudyScope::All, now)
        .unwrap();
    let mut card = session.first_card;
    while let Some(front) = card {
        let r = t
            .services
            .submit_review(
                flashtester,
                session.session_id,
                front.card_id,
                Rating::Good,
                ReviewOrigin::WEB,
                now,
            )
            .unwrap();
        card = r.next_card;
    }
    let s = send(&t.app, json_get("/api/v1/stats", Some(&bearer)))
        .await
        .json();
    assert_eq!(s["total_reviews"], 2);
    assert_eq!(s["reviews_30d"], 2);
    assert_eq!(s["streak"], 1);
    assert_eq!(s["active_cards"], 2);
    assert_eq!(s["days"][29]["count"], 2);
    assert_eq!(s["days"][29]["pct"], 100);
    let today = s["heat_weeks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|w| w.as_array().unwrap())
        .find(|c| c["today"] == true)
        .unwrap();
    assert_eq!(today["count"], 2);
    assert_eq!(today["level"], 4);
    // One year of history: no years to pick from, the trailing view.
    assert_eq!(s["heat_years"], serde_json::json!([]));
    assert_eq!(s["heat_year"], serde_json::Value::Null);

    // A review two years back opens the picker; `?year=` shows that
    // calendar year, with no today or future cells.
    let two_years_ago = now - 2 * 365 * 24 * 60 * 60 * 1000;
    let deck = t
        .store
        .find_deck_by_name(flashtester, "Bio")
        .unwrap()
        .unwrap();
    let cards = t
        .store
        .list_cards(flashtester, Some(deck), None, 1)
        .unwrap();
    let scheduler = flash_core::Scheduler::new(None, 0.9).unwrap();
    let state = t.store.get_card_state(flashtester, cards[0].id).unwrap();
    let outcome = scheduler
        .review(&state, Rating::Good, two_years_ago)
        .unwrap();
    t.store
        .record_review(
            flashtester,
            cards[0].id,
            &outcome,
            two_years_ago,
            "web",
            None,
            None,
        )
        .unwrap();
    let year = jiff::Timestamp::from_millisecond(two_years_ago)
        .unwrap()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .year();
    let s = send(&t.app, json_get("/api/v1/stats", Some(&bearer)))
        .await
        .json();
    assert_eq!(s["heat_years"].as_array().unwrap().len(), 3);
    assert_eq!(s["heat_years"][0], year);
    assert_eq!(s["heat_year"], serde_json::Value::Null);
    let s = send(
        &t.app,
        json_get(&format!("/api/v1/stats?year={year}"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(s["heat_year"], year);
    assert_eq!(s["total_reviews"], 3);
    let cells: Vec<_> = s["heat_weeks"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|w| w.as_array().unwrap())
        .collect();
    assert!(cells
        .iter()
        .all(|c| c["today"] == false && c["future"] == false));
    assert_eq!(
        cells
            .iter()
            .filter(|c| c["level"].as_u64() > Some(0))
            .count(),
        1
    );
}
