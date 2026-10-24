//! What breaks in the browser, as the pages and browsers report it:
//! counted without saying who, new once a week, our own scripts only,
//! never from another site, and every page says where policy violations
//! go.

use std::any::Any;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use flash_server::ext::{AccountHooks, ServerExtension};
use flash_server::testing::{send, AppBuilder, TestApp, BASE};
use flash_store::{ClientErrorNew, Store};
use parking_lot::Mutex;

const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) \
     AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

/// A deployment that keeps every error it is told is new.
#[derive(Default)]
struct Hears {
    new: Mutex<Vec<String>>,
}

impl AccountHooks for Hears {}

impl ServerExtension for Hears {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn client_error_new(&self, _store: &Store, error: &ClientErrorNew<'_>, _now_ms: i64) {
        self.new.lock().push(error.message.to_string());
    }
}

fn app() -> (TestApp, Arc<Hears>) {
    let hears = Arc::new(Hears::default());
    let app = AppBuilder::new("client-errors").ext(hears.clone()).build();
    (app, hears)
}

/// A page's report as `report.js` sends it: a beacon of JSON as text.
fn beacon(origin: &str, body: &str) -> Request<Body> {
    Request::post("/client-errors")
        .header(header::ORIGIN, origin)
        .header(header::USER_AGENT, IPHONE)
        .header(header::CONTENT_TYPE, "text/plain;charset=UTF-8")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn error(message: &str, file: &str) -> String {
    serde_json::json!({
        "kind": "error",
        "message": message,
        "file": file,
        "line": 120,
        "page": "/decks/42?page=2",
    })
    .to_string()
}

#[tokio::test]
async fn a_page_reports_its_own_error_and_it_is_new_once() {
    let (t, hears) = app();
    for _ in 0..2 {
        let reply = send(
            &t.app,
            beacon(BASE, &error("TypeError: x is null", "/static/study.js")),
        )
        .await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT);
    }
    let errors = t.store.client_errors_since(0).unwrap();
    assert_eq!(errors.len(), 1);
    let e = &errors[0];
    assert_eq!(e.count, 2);
    assert_eq!(
        (e.kind.as_str(), e.file.as_str(), e.line),
        ("error", "/static/study.js", 120)
    );
    assert_eq!(e.message, "TypeError: x is null");
    assert_eq!(e.pages, ["/decks/{id}"]);
    assert_eq!(e.browsers, ["Safari (mobile)"]);
    // Told once, however often it happens.
    assert_eq!(*hears.new.lock(), ["TypeError: x is null"]);
}

#[tokio::test]
async fn what_is_not_ours_or_not_a_report_is_dropped() {
    let (t, hears) = app();
    let dropped = [
        // An extension's script, another site's, none at all.
        error("boom", "chrome-extension://abc/content.js"),
        error("boom", "https://cdn.example/lib.js"),
        error("boom", ""),
        // A kind the page never sends.
        serde_json::json!({ "kind": "warning", "file": "/static/ui.js" }).to_string(),
        // Past the field's bound.
        error(&"x".repeat(5_000), "/static/ui.js"),
        // Not JSON.
        "not json".to_string(),
    ];
    for body in &dropped {
        let reply = send(&t.app, beacon(BASE, body)).await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{body:.60}");
    }
    assert!(t.store.client_errors_since(0).unwrap().is_empty());
    assert!(hears.new.lock().is_empty());
}

#[tokio::test]
async fn another_site_cannot_report_for_our_pages() {
    let (t, _) = app();
    let reply = send(
        &t.app,
        beacon("https://elsewhere.example", &error("boom", "/static/ui.js")),
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert!(t.store.client_errors_since(0).unwrap().is_empty());
}

fn csp_report(blocked: &str, source: &str) -> Request<Body> {
    let body = serde_json::json!({ "csp-report": {
        "document-uri": format!("{BASE}/study"),
        "blocked-uri": blocked,
        "effective-directive": "script-src-elem",
        "disposition": "enforce",
        "source-file": source,
        "line-number": 3,
    }});
    // A browser sends it on its own: no Origin of the page's.
    Request::post("/client-errors/csp")
        .header(header::CONTENT_TYPE, "application/csp-report")
        .header(header::USER_AGENT, IPHONE)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn a_policy_violation_on_our_page_is_counted_and_an_extensions_is_not() {
    let (t, hears) = app();
    let kept = send(&t.app, csp_report("https://cdn.example/x.js?k=1", "")).await;
    assert_eq!(kept.status, StatusCode::NO_CONTENT);
    let ext = send(
        &t.app,
        csp_report("inline", "chrome-extension://abc/inject.js"),
    )
    .await;
    assert_eq!(ext.status, StatusCode::NO_CONTENT);
    let errors = t.store.client_errors_since(0).unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, "csp");
    assert_eq!(
        errors[0].message,
        "script-src-elem blocked https://cdn.example"
    );
    assert_eq!(errors[0].pages, ["/study"]);
    assert_eq!(hears.new.lock().len(), 1);
}

#[tokio::test]
async fn every_page_says_where_its_policy_reports_go() {
    let (t, _) = app();
    let page = send(&t.app, Request::get("/login").body(Body::empty()).unwrap()).await;
    assert_eq!(
        page.header("reporting-endpoints"),
        Some(flash_server::state::REPORTING_ENDPOINTS)
    );
    let csp = page.header("content-security-policy").unwrap_or_default();
    assert!(csp.ends_with(flash_server::state::CSP_REPORTING), "{csp}");
}
