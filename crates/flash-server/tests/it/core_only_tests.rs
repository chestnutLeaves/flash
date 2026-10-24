//! The open server: `build_app` with `CoreOnly` installed. The core's own
//! site root and pages must stand on their own.

use crate::common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use flash_server::ext::CoreOnly;

fn harness() -> TestApp {
    AppBuilder::new("core-only").ext(Arc::new(CoreOnly)).build()
}

#[tokio::test]
async fn root_is_login_or_dashboard_and_crawlers_are_kept_out() {
    let h = harness();
    let r = send(&h.app, get("/", None)).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), "/login");

    let r = send(&h.app, get("/robots.txt", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.text(), "User-agent: *\nDisallow: /\n");

    let (_, cookie) = signed_in(&h.store, "Ada", "ada@example.com");
    let r = send(&h.app, get("/", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(
        r.text().contains("href=\"/decks\""),
        "the dashboard renders"
    );
}

#[tokio::test]
async fn core_pages_render_on_their_own() {
    let h = harness();
    let r = send(&h.app, get("/login", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    let body = r.text();
    assert!(body.contains("Sign in with passkey"));
    // With no buttons of an extension's own to lead with, nothing folds:
    // the passkey stays the page's first choice, under its own title.
    assert!(body.contains("<title>Sign in - Flash</title>"));
    assert!(body.contains("Flashcards that schedule themselves."));
    assert!(body.contains(r#"class="fl-btn fl-btn-primary" id="login-btn""#));
    assert!(!body.contains("<details"));

    let r = send(&h.app, get("/connect", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.text().contains("custom MCP server"));

    let (user, cookie) = signed_in(&h.store, "Ada", "ada@example.com");
    let r = send(&h.app, get("/settings", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    let body = r.text();
    assert!(body.contains("Your cards"), "the core's own card");
    assert!(body.contains("Delete account"));

    let r = send(&h.app, get("/decks", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);

    let deck = h
        .services
        .create_deck(user, "Pharm", "", flash_server::service::now_ms())
        .unwrap();
    let r = send(
        &h.app,
        get(&format!("/decks/{}/settings", deck.0), Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.text().contains("Daily limits"));
}

#[tokio::test]
async fn meta_offers_only_the_core_features() {
    let h = harness();
    let meta = send(&h.app, json_get("/api/v1/meta", None)).await.json();
    assert_eq!(
        meta["features"],
        serde_json::json!({ "signup": false, "passkeys": true }),
        "{meta}"
    );
    let keys: Vec<&str> = meta
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["features", "min_app_version", "version"], "{meta}");
    assert_eq!(meta["min_app_version"], "1.0.1", "{meta}");
}
