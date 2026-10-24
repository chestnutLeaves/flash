//! What a person is in the middle of survives a restart (a deploy): a
//! connect waiting on the consent screen, an assistant's MCP session, and
//! the signed media links in a study panel. Each test starts it on one
//! app and finishes it on another built over the same database, as the
//! next process would be.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_core::MediaId;
use flash_server::service::now_ms;

use crate::mcp_tests::{call_tool, initialize, Harness};

/// The consent screen a connector's authorize request opened on one
/// process is approved on the next.
#[tokio::test]
async fn a_consent_shown_before_a_restart_is_approved_after_it() {
    let before = AppBuilder::new("restart-consent").build();
    let user = member(&before.store, "FlashTester", "flashtester@example.com");
    let cookie = web_session(&before.store, user);
    before
        .store
        .create_oauth_client(
            "restart-client",
            "Claude",
            &["https://client.example.com/callback".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let page = send(
        &before.app,
        Request::get(
            "/oauth/authorize?response_type=code&client_id=restart-client\
             &redirect_uri=https%3A%2F%2Fclient.example.com%2Fcallback\
             &state=xyz&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM\
             &code_challenge_method=S256",
        )
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    let token = page
        .text()
        .split("name=\"token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("consent token in page")
        .to_string();

    let after = AppBuilder::new("restart-consent-2")
        .store(before.store.clone())
        .build();
    let approved = send(
        &after.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={token}&decision=approve")))
            .unwrap(),
    )
    .await;
    assert_eq!(
        approved.status,
        StatusCode::SEE_OTHER,
        "{}",
        approved.text()
    );
    let location = approved.location();
    assert!(location.starts_with("https://client.example.com/callback?code="));
    assert!(location.contains("state=xyz"));

    // Answered once: approving the same screen again (a second tap) gets
    // a redirect too, with a fresh code in place of the first.
    let again = send(
        &after.app,
        Request::post("/oauth/consent")
            .header(header::COOKIE, &cookie)
            .header(header::ORIGIN, BASE)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("token={token}&decision=approve")))
            .unwrap(),
    )
    .await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.text());
    assert!(again
        .location()
        .starts_with("https://client.example.com/callback?code="));
    assert_ne!(again.location(), location);
}

/// An assistant's MCP session opened on one process answers on the next:
/// the client does not have to start over after a deploy.
#[tokio::test]
async fn an_mcp_session_outlives_the_process_that_opened_it() {
    let before = crate::mcp_tests::harness_named("restart-mcp");
    let session = initialize(&before).await;
    let stored: i64 = before
        .store
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM mcp_sessions", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(stored, 1);

    let next = AppBuilder::new("restart-mcp-2")
        .store(before.store.clone())
        .build();
    let after = Harness {
        app: next.app,
        bearer: before.bearer.clone(),
        store: before.store.clone(),
        services: next.services,
        data_dir: next.data_dir,
        user: before.user,
    };
    let decks = call_tool(&after, &session, 2, "list_decks", serde_json::json!({})).await;
    assert!(decks.get("decks").is_some(), "{decks}");
}

/// A panel's signed media link made before a restart still loads after
/// it: the key that signs them is the deployment's, not the process's.
#[test]
fn a_panel_media_link_signed_before_a_restart_verifies_after_it() {
    let before = AppBuilder::new("restart-links").build();
    let owner = member(&before.store, "FlashTester", "flashtester@example.com");
    let now = now_ms();
    let url = before.state.panel_links.url(BASE, owner, MediaId(7), now);
    let query = url.split('?').nth(1).unwrap();
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
            .unwrap()
            .to_string()
    };
    let (expires, sig) = (param("e").parse::<i64>().unwrap(), param("s"));

    let after = AppBuilder::new("restart-links-2")
        .store(before.store.clone())
        .build();
    assert!(after
        .state
        .panel_links
        .verify(owner, MediaId(7), expires, &sig, now));
    assert!(!after
        .state
        .panel_links
        .verify(owner, MediaId(8), expires, &sig, now));
}
