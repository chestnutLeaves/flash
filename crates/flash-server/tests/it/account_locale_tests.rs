//! An account learns the language its person reads from what their own
//! software says: the browser that signs in or connects an assistant,
//! the assistant on each tool call or through the panel, and the phone
//! on each request. A choice is never replaced.

use crate::common;
use crate::mcp_tests::{harness_named, initialize, rpc};

use axum::body::Body;
use axum::http::{header, HeaderValue, Request, StatusCode};
use common::*;
use flash_server::auth::{hash_token, new_token, urlencode};
use flash_server::service::now_ms;
use flash_store::LocaleSource;

fn with_languages(mut req: Request<Body>, header_value: &'static str) -> Request<Body> {
    req.headers_mut().insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static(header_value),
    );
    req
}

#[tokio::test]
async fn a_sign_in_keeps_the_browsers_languages() {
    let t = AppBuilder::new("locale-signin").build();
    let token = new_token();
    t.store
        .create_invite(
            &hash_token(&token),
            "Tester",
            "member",
            None,
            now_ms() + 86_400_000,
            now_ms(),
        )
        .unwrap();
    let body = format!(
        "email={}&password={}&confirm={}",
        urlencode("lang@example.com"),
        urlencode("hunter42hunter42"),
        urlencode("hunter42hunter42")
    );
    let reply = send(
        &t.app,
        with_languages(
            form_post(&format!("/auth/enroll/{token}/password"), None, &body),
            "es-MX,es;q=0.9,en;q=0.8",
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.text());
    let user = t
        .store
        .user_by_email("lang@example.com")
        .unwrap()
        .unwrap()
        .user;
    assert_eq!(
        t.store.account_locale(user).unwrap(),
        Some(("es-MX".into(), LocaleSource::Browser))
    );
    let signals = t.store.locale_signals(user).unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].prefs.as_deref(), Some("es-MX,es,en"));

    // A language the person chose outlives the next sign-in's browser.
    t.store.set_chosen_locale(user, "en").unwrap();
    let reply = send(
        &t.app,
        with_languages(
            form_post(
                "/auth/login/password",
                None,
                "email=lang%40example.com&password=hunter42hunter42",
            ),
            "es-ES",
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.store.account_locale(user).unwrap(),
        Some(("en".into(), LocaleSource::Chosen))
    );
    // The browser itself is still kept as seen.
    let browser = t.store.locale_signals(user).unwrap();
    assert_eq!(browser[0].tag, "es-ES");
}

#[tokio::test]
async fn the_page_that_connects_an_assistant_keeps_the_browsers_languages() {
    let t = AppBuilder::new("locale-consent").build();
    let (user, cookie) = signed_in(&t.store, "Tester", "consent@example.com");
    // Whatever the request asks for, the browser has said its language.
    let reply = send(
        &t.app,
        with_languages(
            get("/oauth/authorize?client_id=nobody", Some(&cookie)),
            "pt-BR,pt;q=0.9",
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        t.store.account_locale(user).unwrap(),
        Some(("pt-BR".into(), LocaleSource::Browser))
    );
}

#[tokio::test]
async fn an_assistant_says_the_language_with_its_calls() {
    let h = harness_named("locale-host");
    let session = initialize(&h).await;
    let (status, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {
                "name": "list_decks", "arguments": {},
                "_meta": {"openai/locale": "es-ES", "openai/userAgent": "test"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        h.store.account_locale(h.user).unwrap(),
        Some(("es-ES".into(), LocaleSource::Host))
    );

    // The panel tells what the host told it, with a page's first grade.
    // The grade itself may fail (no such session); the language is kept.
    let (status, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 8, "method": "tools/call",
            "params": {
                "name": "panel_grade",
                "arguments": {"session_id": 999, "card_id": 1, "rating": 3, "ui_locale": "es-CL"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        h.store.account_locale(h.user).unwrap(),
        Some(("es-CL".into(), LocaleSource::Host))
    );
}

#[tokio::test]
async fn the_app_says_the_phones_language_with_its_requests() {
    let t = AppBuilder::new("locale-app").build();
    let user = member(&t.store, "Tester", "app@example.com");
    let bearer = api_bearer(&t.store, user);
    let reply = send(
        &t.app,
        with_languages(json_get("/api/v1/me", Some(&bearer)), "es-AR,es;q=0.9"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(
        t.store.account_locale(user).unwrap(),
        Some(("es-AR".into(), LocaleSource::App))
    );
}

#[tokio::test]
async fn nonsense_is_never_kept() {
    let t = AppBuilder::new("locale-junk").build();
    let user = member(&t.store, "Tester", "junk@example.com");
    let bearer = api_bearer(&t.store, user);
    let reply = send(
        &t.app,
        with_languages(
            json_get("/api/v1/me", Some(&bearer)),
            "*, <script>, x-klingon",
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(t.store.account_locale(user).unwrap(), None);
    assert!(t.store.locale_signals(user).unwrap().is_empty());
}
