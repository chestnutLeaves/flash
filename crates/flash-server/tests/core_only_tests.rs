//! The open server: `build_app` with `CoreOnly` installed. Everything the
//! hosted extension adds must be absent, and the core's own site root
//! must stand on its own.

mod common;

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
    let body = r.text();
    assert!(body.contains("href=\"/decks\""), "the dashboard renders");
    assert!(
        !body.contains("href=\"/community\""),
        "no Community item without the extension"
    );
}

#[tokio::test]
async fn pages_render_without_the_extension_fragments() {
    let h = harness();
    let r = send(&h.app, get("/login", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    let body = r.text();
    assert!(body.contains("Sign in with passkey"));
    assert!(!body.contains("Continue with Google"));
    assert!(!body.contains("Create an account"));

    let (user, cookie) = signed_in(&h.store, "Ada", "ada@example.com");
    let r = send(&h.app, get("/settings", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    let body = r.text();
    assert!(body.contains("Your cards"), "the core's own card");
    assert!(!body.contains("Plan &amp; usage"));
    assert!(!body.contains("Connected accounts"));
    assert!(body.contains("Delete account"));

    let r = send(&h.app, get("/decks", Some(&cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(!r.text().contains("free cards used"));

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
    let body = r.text();
    assert!(body.contains("Daily limits"));
    assert!(!body.contains("Create share link"));
}

/// The features the README says the hosted version adds (self-serve
/// signup, a pricing page, the community library) have no route here.
/// The extension's own tests probe its whole surface against this
/// build; this keeps the promise the README makes.
#[tokio::test]
async fn the_hosted_features_the_readme_names_are_absent() {
    let h = harness();
    for path in ["/signup", "/pricing", "/community"] {
        let r = send(&h.app, get(path, None)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
    }
    let user = member(&h.store, "Bob", "bob@example.com");
    let bearer = api_bearer(&h.store, user);
    let r = send(&h.app, json_get("/api/v1/community", Some(&bearer))).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
