//! FLASH_ACCESS_LOG: one line per request, naming the route by its
//! template. A path's values and the query string carry tokens, codes
//! and OAuth state, so neither may reach the log.

use crate::common;

use std::io::Write;
use std::sync::Arc;

use parking_lot::Mutex;

use axum::body::Body;
use axum::http::{header, Request};
use common::*;
use flash_server::service::now_ms;

/// Everything the log wrote while `run` ran, on this thread.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn logged(t: &TestApp, requests: Vec<Request<Body>>) -> String {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    for request in requests {
        send(&t.app, request).await;
    }
    let bytes = captured.0.lock().clone();
    String::from_utf8(bytes).unwrap()
}

fn with_agent(path: &str) -> Request<Body> {
    Request::get(path)
        .header(header::USER_AGENT, "TestBrowser/1.0")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn a_request_is_logged_by_its_route_never_its_values() {
    let t = AppBuilder::new("access-log").access_log().build();
    let log = logged(
        &t,
        vec![
            with_agent("/enroll/secret-token-value?code=secret-code"),
            with_agent("/static/app.css"),
            with_agent("/no-such-page-anywhere?state=secret-state"),
        ],
    )
    .await;
    let lines: Vec<&str> = log
        .lines()
        .filter(|l| l.contains("flash::access"))
        .collect();
    assert_eq!(lines.len(), 2, "static files are not logged: {log}");
    for secret in [
        "secret-token-value",
        "secret-code",
        "secret-state",
        "no-such-page",
    ] {
        assert!(!log.contains(secret), "{secret} leaked: {log}");
    }
    assert!(lines[0].contains("route=\"/enroll/{token}\""), "{log}");
    assert!(lines[1].contains("route=\"unmatched\""), "{log}");
    assert!(lines.iter().all(|l| l.contains("TestBrowser/1.0")), "{log}");
}

/// The first authorize request of a connect names the page it came from
/// by origin only: enough to tell an assistant's web app from its desktop
/// or phone app, and nothing of the path, query or state behind it.
#[tokio::test]
async fn an_oauth_request_logs_where_it_came_from_by_origin_only() {
    let t = AppBuilder::new("access-log-referer").access_log().build();
    let from = |referer: Option<&str>| {
        let mut request =
            Request::get("/oauth/authorize?response_type=code&client_id=c&state=secret-state")
                .header(header::USER_AGENT, "TestBrowser/1.0");
        if let Some(r) = referer {
            request = request.header(header::REFERER, r);
        }
        request.body(Body::empty()).unwrap()
    };
    let log = logged(
        &t,
        vec![
            from(Some("https://chatgpt.com/secret-path?x=secret-query")),
            from(None),
            with_agent("/no-such-page-anywhere"),
        ],
    )
    .await;
    let lines: Vec<&str> = log
        .lines()
        .filter(|l| l.contains("flash::access"))
        .collect();
    assert_eq!(lines.len(), 3, "{log}");
    assert!(
        lines[0].contains("referer_origin=\"https://chatgpt.com\""),
        "{log}"
    );
    assert!(lines[1].contains("referer_origin=\"-\""), "{log}");
    assert!(!lines[2].contains("referer_origin"), "{log}");
    for secret in ["secret-path", "secret-query", "secret-state"] {
        assert!(!log.contains(secret), "{secret} leaked: {log}");
    }
}

/// A refused authorize request names, in the log only, the client that
/// asked and the host its redirect pointed at, so a connect that fails
/// for one kind of client can be traced to it. The rest of the URI and
/// the state stay out.
#[tokio::test]
async fn a_refused_authorize_names_its_client_and_redirect_host() {
    let t = AppBuilder::new("refused-authorize-log").build();
    let (_, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.store
        .create_oauth_client(
            "chatgpt-client",
            "ChatGPT",
            &["https://chatgpt.com/connector/oauth/abc123".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let uri = "/oauth/authorize?response_type=code&client_id=chatgpt-client\
               &redirect_uri=https%3A%2F%2Fother.example.com%2Fsecret-path\
               &state=secret-state&code_challenge=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopq\
               &code_challenge_method=S256";
    let log = logged(&t, vec![get(uri, Some(&cookie))]).await;
    let line = log
        .lines()
        .find(|l| l.contains("oauth authorize rejected"))
        .unwrap_or_else(|| panic!("no rejection logged: {log}"));
    assert!(
        line.contains("reason=\"redirect_not_registered\""),
        "{line}"
    );
    assert!(line.contains("client=\"ChatGPT\""), "{line}");
    assert!(
        line.contains("redirect_host=\"other.example.com\""),
        "{line}"
    );
    for secret in ["secret-path", "secret-state"] {
        assert!(!log.contains(secret), "{secret} leaked: {log}");
    }
}

/// A refused callback on an assistant's host logs its path too, the one
/// thing that tells a new route of theirs; the query, which carries their
/// state, stays out.
#[tokio::test]
async fn a_refused_assistant_callback_logs_its_path_but_not_its_query() {
    let t = AppBuilder::new("refused-assistant-path-log").build();
    let (_, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.store
        .create_oauth_client(
            "chatgpt-client",
            "ChatGPT",
            &["https://chatgpt.com/connector/oauth/abc123".to_string()],
            "{}",
            now_ms(),
        )
        .unwrap();
    let uri = "/oauth/authorize?response_type=code&client_id=chatgpt-client\
               &redirect_uri=https%3A%2F%2Fchatgpt.com%2Fnew_callback%3Fx%3Dsecret-query\
               &state=secret-state&code_challenge=abcdefghijklmnopqrstuvwxyzabcdefghijklmnopq\
               &code_challenge_method=S256";
    let log = logged(&t, vec![get(uri, Some(&cookie))]).await;
    let line = log
        .lines()
        .find(|l| l.contains("oauth authorize rejected"))
        .unwrap_or_else(|| panic!("no rejection logged: {log}"));
    assert!(line.contains("redirect_host=\"chatgpt.com\""), "{line}");
    assert!(line.contains("redirect_path=\"/new_callback\""), "{line}");
    for secret in ["secret-query", "secret-state"] {
        assert!(!log.contains(secret), "{secret} leaked: {log}");
    }
}

#[tokio::test]
async fn nothing_is_logged_unless_the_log_is_on() {
    let t = AppBuilder::new("access-log-off").build();
    let log = logged(&t, vec![with_agent("/no-such-page-anywhere")]).await;
    assert!(!log.contains("flash::access"), "{log}");
}
