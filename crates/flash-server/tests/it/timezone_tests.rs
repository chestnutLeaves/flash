//! The web learns a person's time zone from their browser: the dashboard
//! and the connector consent page load a small script while the account
//! still has the default zone, and the route it posts to only ever
//! replaces that default.

use crate::common;

use axum::http::StatusCode;
use common::*;

fn zone(t: &TestApp, user: flash_core::UserId) -> String {
    t.store.get_settings(user).unwrap().timezone
}

#[tokio::test]
async fn the_browser_zone_replaces_only_the_default() {
    let t = AppBuilder::new("tz-adopt").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    assert_eq!(zone(&t, user), "UTC");

    let html = send(&t.app, get("/", Some(&cookie))).await.text();
    assert!(html.contains("/static/timezone.js"), "the dashboard asks");

    let r = send(
        &t.app,
        form_post(
            "/settings/timezone",
            Some(&cookie),
            "timezone=America%2FChicago",
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(zone(&t, user), "America/Chicago");

    let html = send(&t.app, get("/", Some(&cookie))).await.text();
    assert!(
        !html.contains("/static/timezone.js"),
        "asked once is enough"
    );

    // A second browser somewhere else leaves the zone alone.
    let r = send(
        &t.app,
        form_post(
            "/settings/timezone",
            Some(&cookie),
            "timezone=Europe%2FBerlin",
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(zone(&t, user), "America/Chicago");
}

/// Chrome and Hermes still report India as `Asia/Calcutta`, a name some
/// hosts no longer ship; it is taken and kept as the browser sent it, so
/// the page's own zone check keeps matching. The lookup's fallback for a
/// host without the name is `flash_core::day`'s own test.
#[tokio::test]
async fn an_old_zone_name_is_taken_as_sent() {
    let t = AppBuilder::new("tz-legacy").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");

    let r = send(
        &t.app,
        form_post(
            "/settings/timezone",
            Some(&cookie),
            "timezone=Asia%2FCalcutta",
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(zone(&t, user), "Asia/Calcutta");
}

#[tokio::test]
async fn a_chosen_zone_is_never_replaced_and_nonsense_is_refused() {
    let t = AppBuilder::new("tz-chosen").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");

    let r = send(
        &t.app,
        form_post("/settings/timezone", Some(&cookie), "timezone=Not%2FA_Zone"),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(zone(&t, user), "UTC");

    t.store
        .set_study_settings(user, Some("Asia/Tokyo"), None, None, None)
        .unwrap();
    let r = send(
        &t.app,
        form_post(
            "/settings/timezone",
            Some(&cookie),
            "timezone=America%2FChicago",
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(zone(&t, user), "Asia/Tokyo");
}

#[tokio::test]
async fn it_takes_a_signed_in_same_origin_post() {
    let t = AppBuilder::new("tz-guard").build();
    let (user, _) = signed_in(&t.store, "FlashTester", "flashtester@example.com");

    let r = send(
        &t.app,
        form_post("/settings/timezone", None, "timezone=America%2FChicago"),
    )
    .await;
    assert!(
        r.status == StatusCode::UNAUTHORIZED || r.status.is_redirection(),
        "{}",
        r.status
    );
    assert_eq!(zone(&t, user), "UTC");
}

#[tokio::test]
async fn the_static_script_is_served() {
    let t = AppBuilder::new("tz-static").build();
    let r = send(&t.app, get("/static/timezone.js", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.text().contains("/settings/timezone"));
}
