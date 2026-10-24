//! What breaks in the browser. The website's pages report errors in their
//! own scripts (`static/report.js`), and browsers report what the
//! Content-Security-Policy blocked (`state::CSP_REPORTING`). Both are
//! counted (`flash_store::repo::client_errors`), never logged one by one:
//! an error new after a quiet week is written to the log and handed to
//! the extension (`ServerExtension::client_error_new`), and the rest only
//! counted.
//!
//! Nothing here says who saw it: the report carries no account, the row
//! no address and no user agent. The browser is a family, the page a path
//! with its numbers and tokens taken out. Anything that is not about our
//! own scripts (an extension's error, another site's) is dropped.
//!
//! Both endpoints answer 204 whatever they were sent (429 past an
//! address's budget), so a page learns nothing from them and a broken
//! report breaks nothing.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use flash_store::{ClientErrorNew, ClientErrorRecorded};
use serde::Deserialize;

use crate::bounds;
use crate::service::now_ms;
use crate::state::AppState;

/// A message is kept to this many characters.
const MESSAGE_CHARS: usize = 300;
/// A script's or page's path is kept to this many characters.
const PATH_CHARS: usize = 200;
/// A path segment at least this long is a token or a slug, not a word.
const TOKEN_SEGMENT: usize = 16;
/// Violations taken from one report request (the Reporting API batches).
const VIOLATIONS_PER_REQUEST: usize = 5;
/// Schemes of browser extensions: what they inject is not our page's.
const EXTENSION_SCHEMES: &[&str] = &[
    "chrome-extension:",
    "moz-extension:",
    "safari-extension:",
    "safari-web-extension:",
    "ms-browser-extension:",
];

/// `POST /client-errors`: a page's report of its own error. In the
/// browser band: the same-origin guard and the web budget apply.
pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/client-errors",
            post(report).layer(DefaultBodyLimit::max(bounds::CLIENT_ERROR_BODY_BYTES)),
        )
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), budget))
}

/// `POST /client-errors/csp`: a browser's report of what a policy
/// blocked. Outside the same-origin guard: a browser sends these on its
/// own, with no Origin of the page's.
pub fn csp_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/client-errors/csp",
            post(csp_report).layer(DefaultBodyLimit::max(bounds::CSP_REPORT_BODY_BYTES)),
        )
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), budget))
}

/// Reports past an address's budget are refused unread: a page that
/// throws in a loop, or someone posting reports, costs one map entry.
/// A browser does nothing with the answer either way.
async fn budget(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let ip = crate::middleware::client_ip(&request, state.config.client_ip_header.as_deref());
    if !state.rate_limiter.allow(&format!("{ip}:client-errors")) {
        return (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response();
    }
    next.run(request).await
}

/// What `static/report.js` sends.
#[derive(Deserialize)]
struct Report {
    #[serde(default, deserialize_with = "bounds::opt_keyword")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "bounds::opt_client_error_text")]
    message: Option<String>,
    #[serde(default, deserialize_with = "bounds::opt_client_error_text")]
    file: Option<String>,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default, deserialize_with = "bounds::opt_client_error_text")]
    page: Option<String>,
}

/// A page's own error. The body is JSON sent as text (a beacon), so it is
/// read whatever its type says.
async fn report(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let Ok(r) = serde_json::from_slice::<Report>(&body) else {
        return StatusCode::NO_CONTENT;
    };
    let kind = match r.kind.as_deref() {
        Some("error") => "error",
        Some("rejection") => "rejection",
        _ => return StatusCode::NO_CONTENT,
    };
    // Only our own scripts: the page sends nothing else, and anything
    // else is not ours to fix.
    let Some(file) = r.file.as_deref().and_then(script_path) else {
        return StatusCode::NO_CONTENT;
    };
    let message = clean_message(r.message.as_deref().unwrap_or_default());
    let page = page_path(r.page.as_deref().unwrap_or_default());
    let browser = browser_family(user_agent(&headers));
    record(
        state,
        vec![Owned {
            kind,
            file,
            line: r.line.unwrap_or(0),
            message,
            page,
            browser,
        }],
    )
    .await;
    StatusCode::NO_CONTENT
}

/// A browser's policy violation reports: one object under `csp-report`
/// (`report-uri`, `application/csp-report`), or a list of reports
/// (`report-to`, `application/reports+json`). Only enforced violations
/// on our own pages count, and none an extension caused.
async fn csp_report(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return StatusCode::NO_CONTENT;
    };
    let origin = state.config.base_url.trim_end_matches('/').to_string();
    let browser = browser_family(user_agent(&headers));
    let errors: Vec<Owned> = violations(&value)
        .into_iter()
        .take(VIOLATIONS_PER_REQUEST)
        .filter_map(|v| v.counted(&origin, &browser))
        .collect();
    if !errors.is_empty() {
        record(state, errors).await;
    }
    StatusCode::NO_CONTENT
}

/// A report, cleaned and owned, on its way to the store.
struct Owned {
    kind: &'static str,
    file: String,
    line: u32,
    message: String,
    page: String,
    browser: String,
}

/// Counts `errors`; one new after a quiet week is logged and handed to
/// the extension.
async fn record(state: AppState, errors: Vec<Owned>) {
    let done = tokio::task::spawn_blocking(move || {
        let store = state.services.store();
        let now = now_ms();
        for e in &errors {
            let error = ClientErrorNew {
                kind: e.kind,
                file: &e.file,
                line: e.line,
                message: &e.message,
                page: &e.page,
                browser: &e.browser,
            };
            match store.record_client_error(&error, now) {
                Ok(ClientErrorRecorded::New) => {
                    tracing::warn!(
                        kind = e.kind,
                        file = ?e.file,
                        line = e.line,
                        message = ?e.message,
                        page = ?e.page,
                        browser = ?e.browser,
                        "a page reported a new browser error"
                    );
                    state.ext.client_error_new(store, &error, now);
                }
                Ok(ClientErrorRecorded::Again | ClientErrorRecorded::Dropped) => {}
                Err(e) => tracing::warn!("count a browser error: {e}"),
            }
        }
    })
    .await;
    if let Err(e) = done {
        tracing::warn!("count a browser error: {e}");
    }
}

/// One violation as a browser described it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Violation {
    document: String,
    blocked: String,
    directive: String,
    disposition: String,
    source: String,
    line: u32,
}

impl Violation {
    /// What is counted of it: none for a report-only policy, another
    /// site's page or an extension's doing.
    fn counted(&self, origin: &str, browser: &str) -> Option<Owned> {
        if !self.disposition.is_empty() && self.disposition != "enforce" {
            return None;
        }
        let page = self.document.strip_prefix(origin)?;
        if !page.starts_with('/') {
            return None;
        }
        let extension = |s: &str| EXTENSION_SCHEMES.iter().any(|x| s.starts_with(x));
        if extension(&self.blocked) || extension(&self.source) {
            return None;
        }
        // The script that tripped it, when it is one of ours.
        let file = self
            .source
            .strip_prefix(origin)
            .and_then(script_path)
            .unwrap_or_default();
        if !self.source.is_empty() && file.is_empty() {
            return None;
        }
        let directive: String = self
            .directive
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(40)
            .collect();
        let directive = if directive.is_empty() {
            "policy".to_string()
        } else {
            directive
        };
        Some(Owned {
            kind: "csp",
            line: if file.is_empty() { 0 } else { self.line },
            file,
            message: format!(
                "{directive} blocked {}",
                blocked_what(&self.blocked, origin)
            ),
            page: page_path(page),
            browser: browser.to_string(),
        })
    }
}

/// The violations in a report body of either form.
fn violations(value: &serde_json::Value) -> Vec<Violation> {
    let text = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .chars()
            .take(bounds::CLIENT_ERROR_TEXT)
            .collect::<String>()
    };
    let number = |v: &serde_json::Value, key: &str| {
        v.get(key)
            .and_then(|x| x.as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    if let Some(r) = value.get("csp-report") {
        let directive = match text(r, "effective-directive") {
            d if d.is_empty() => text(r, "violated-directive"),
            d => d,
        };
        return vec![Violation {
            document: text(r, "document-uri"),
            blocked: text(r, "blocked-uri"),
            directive,
            disposition: text(r, "disposition"),
            source: text(r, "source-file"),
            line: number(r, "line-number"),
        }];
    }
    let Some(list) = value.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter(|r| r.get("type").and_then(|t| t.as_str()) == Some("csp-violation"))
        .filter_map(|r| r.get("body"))
        .map(|b| Violation {
            document: text(b, "documentURL"),
            blocked: text(b, "blockedURL"),
            directive: text(b, "effectiveDirective"),
            disposition: text(b, "disposition"),
            source: text(b, "sourceFile"),
            line: number(b, "lineNumber"),
        })
        .collect()
}

/// What a policy blocked, said without its path: `inline`, `eval`, a
/// scheme (`data`), or the origin it came from.
fn blocked_what(blocked: &str, origin: &str) -> String {
    match blocked {
        "" => "something".into(),
        "inline" | "eval" | "wasm-eval" | "trusted-types-policy" | "trusted-types-sink" => {
            blocked.into()
        }
        _ if blocked.starts_with(origin) => "this site".into(),
        _ => match url::Url::parse(blocked) {
            Ok(u) if u.has_host() => u.origin().ascii_serialization(),
            Ok(u) => u.scheme().to_string(),
            Err(_) => "something".into(),
        },
    }
}

/// `/static/<name>` of one of our scripts, without its query: none for
/// anything else.
fn script_path(file: &str) -> Option<String> {
    let path = file.split(['?', '#']).next().unwrap_or_default();
    let ok = path.starts_with("/static/")
        && path.len() <= PATH_CHARS
        && !path.contains("..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'));
    ok.then(|| path.to_string())
}

/// What `page_path` puts in place of what it takes out.
const PLACEHOLDERS: &[&str] = &["{id}", "{token}", "{other}"];

/// A page's path for counting: no query, and every number or token in it
/// taken out (`/decks/{id}`, `/reset/{token}`), so a row names a page,
/// never a deck or a sign-in link. Plain ASCII whatever it was given,
/// and a path it made comes back unchanged.
fn page_path(page: &str) -> String {
    let path = page.split(['?', '#']).next().unwrap_or_default();
    if !path.starts_with('/') {
        return "{other}".into();
    }
    let segments: Vec<&str> = path
        .split('/')
        .skip(1)
        .map(|s| {
            if PLACEHOLDERS.contains(&s) {
                s
            } else if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
                "{id}"
            } else if s.len() >= TOKEN_SEGMENT
                || !s
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                "{token}"
            } else {
                s
            }
        })
        .collect();
    // Whole segments only, so a placeholder is never cut in half.
    let mut out = String::new();
    for segment in segments {
        if out.len() + 1 + segment.len() > PATH_CHARS {
            break;
        }
        out.push('/');
        out.push_str(segment);
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

/// A message as counted: one line, no control characters, its first
/// `MESSAGE_CHARS` characters.
fn clean_message(message: &str) -> String {
    let one_line: String = message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MESSAGE_CHARS)
        .collect();
    if one_line.is_empty() {
        "(no message)".into()
    } else {
        one_line
    }
}

fn user_agent(headers: &HeaderMap) -> &str {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

/// The browser's family, and whether it is a phone's or tablet's: all a
/// count needs, and nothing that tells one person from another.
fn browser_family(ua: &str) -> String {
    let has = |s: &str| ua.contains(s);
    let family = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        "Edge"
    } else if has("Firefox/") || has("FxiOS/") {
        "Firefox"
    } else if has("OPR/") {
        "Opera"
    } else if has("SamsungBrowser/") {
        "Samsung Internet"
    } else if has("Chrome/") || has("CriOS/") {
        "Chrome"
    } else if has("Safari/") {
        "Safari"
    } else {
        "Other"
    };
    if has("Mobile") || has("Android") || has("iPhone") || has("iPad") {
        format!("{family} (mobile)")
    } else {
        family.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://flash.example";

    #[test]
    fn a_page_is_counted_by_its_shape_not_its_ids_or_tokens() {
        assert_eq!(page_path("/decks/42?page=2"), "/decks/{id}");
        assert_eq!(page_path("/reset/kq3Zr9vX0bT7mPw2Lc8s"), "/reset/{token}");
        assert_eq!(page_path("/study#top"), "/study");
        assert_eq!(page_path("/"), "/");
        assert_eq!(page_path("https://elsewhere/x"), "{other}");
        assert_eq!(page_path("/cards/7/edit"), "/cards/{id}/edit");
        assert_eq!(page_path("/a b/<x>"), "/{token}/{token}");
    }

    /// Paths from a seeded generator, heavy on the characters next to the
    /// separators (where such code breaks): whatever comes in, the path
    /// counted is short plain ASCII of words and placeholders, a second
    /// pass changes nothing, and no run of digits survives.
    #[test]
    fn any_path_comes_out_short_plain_and_settled() {
        const PIECES: &[&str] = &[
            "/",
            "/",
            "/",
            "decks",
            "study",
            "42",
            "007",
            "{id}",
            "{",
            "}",
            "?",
            "#",
            "%2F",
            ":",
            "::",
            ".",
            "..",
            "-",
            "_",
            " ",
            "é",
            "日本",
            "a",
            "Z",
            "{token}",
            "/static/x.js",
            "kq3Zr9vX0bT7mPw2",
            "\u{0}",
            "\n",
        ];
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % bound as u64) as usize
        };
        for _ in 0..20_000 {
            let mut input = String::new();
            for _ in 0..next(40) {
                input.push_str(PIECES[next(PIECES.len())]);
            }
            let out = page_path(&input);
            assert!(out.len() <= PATH_CHARS, "{input:?} -> {out:?} is too long");
            assert!(out.is_ascii(), "{input:?} -> {out:?} is not ASCII");
            assert_eq!(page_path(&out), out, "{input:?} -> {out:?} is not settled");
            assert!(
                out == "{other}" || out.starts_with('/'),
                "{input:?} -> {out:?}"
            );
            for segment in out.split('/').skip(1) {
                let word = segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
                assert!(
                    PLACEHOLDERS.contains(&segment) || word,
                    "{input:?} -> {out:?}: segment {segment:?}"
                );
                assert!(
                    segment.is_empty() || !segment.chars().all(|c| c.is_ascii_digit()),
                    "{input:?} -> {out:?}: a number survived"
                );
            }
        }
    }

    #[test]
    fn only_our_own_scripts_are_scripts() {
        assert_eq!(
            script_path("/static/sample.js?v=12").as_deref(),
            Some("/static/sample.js")
        );
        assert_eq!(script_path("/static/../secret"), None);
        assert_eq!(script_path("chrome-extension://abc/content.js"), None);
        assert_eq!(script_path("/elsewhere.js"), None);
    }

    #[test]
    fn a_message_is_one_short_line() {
        assert_eq!(clean_message("  a\n\tb  "), "a b");
        assert_eq!(clean_message(""), "(no message)");
        assert_eq!(clean_message(&"x".repeat(1000)).chars().count(), 300);
    }

    #[test]
    fn a_browser_is_a_family() {
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";
        let chrome = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                      (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                    (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0";
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 \
                       (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36";
        let firefox =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";
        assert_eq!(browser_family(iphone), "Safari (mobile)");
        assert_eq!(browser_family(chrome), "Chrome");
        assert_eq!(browser_family(edge), "Edge");
        assert_eq!(browser_family(android), "Chrome (mobile)");
        assert_eq!(browser_family(firefox), "Firefox");
        assert_eq!(browser_family(""), "Other");
    }

    fn csp_uri_report(blocked: &str, source: &str) -> serde_json::Value {
        serde_json::json!({ "csp-report": {
            "document-uri": format!("{ORIGIN}/study?x=1"),
            "blocked-uri": blocked,
            "effective-directive": "script-src-elem",
            "disposition": "enforce",
            "source-file": source,
            "line-number": 12,
        }})
    }

    #[test]
    fn a_violation_on_our_page_is_counted_without_its_path() {
        let v = violations(&csp_uri_report("https://cdn.example/lib.js?k=secret", ""));
        let counted = v[0].counted(ORIGIN, "Chrome").expect("counted");
        assert_eq!(counted.kind, "csp");
        assert_eq!(counted.page, "/study");
        assert_eq!(
            counted.message,
            "script-src-elem blocked https://cdn.example"
        );
        assert_eq!((counted.file.as_str(), counted.line), ("", 0));
        let inline = violations(&csp_uri_report(
            "inline",
            &format!("{ORIGIN}/static/sample.js?v=3"),
        ));
        let counted = inline[0].counted(ORIGIN, "Chrome").expect("counted");
        assert_eq!(counted.message, "script-src-elem blocked inline");
        assert_eq!(
            (counted.file.as_str(), counted.line),
            ("/static/sample.js", 12)
        );
    }

    #[test]
    fn an_extension_or_another_site_is_not_counted() {
        let ext = violations(&csp_uri_report(
            "inline",
            "chrome-extension://abc/inject.js",
        ));
        assert!(ext[0].counted(ORIGIN, "Chrome").is_none());
        let blocked_ext = violations(&csp_uri_report("moz-extension://abc/x.js", ""));
        assert!(blocked_ext[0].counted(ORIGIN, "Firefox").is_none());
        let elsewhere = Violation {
            document: "https://other.example/study".into(),
            blocked: "inline".into(),
            directive: "script-src".into(),
            ..Violation::default()
        };
        assert!(elsewhere.counted(ORIGIN, "Chrome").is_none());
        let report_only = Violation {
            document: format!("{ORIGIN}/study"),
            blocked: "inline".into(),
            directive: "script-src".into(),
            disposition: "report".into(),
            ..Violation::default()
        };
        assert!(report_only.counted(ORIGIN, "Chrome").is_none());
    }

    #[test]
    fn the_reporting_api_batch_is_read_too() {
        let batch = serde_json::json!([
            { "type": "csp-violation", "body": {
                "documentURL": format!("{ORIGIN}/decks/9"),
                "blockedURL": "eval",
                "effectiveDirective": "script-src",
                "disposition": "enforce",
                "sourceFile": format!("{ORIGIN}/static/ui.js"),
                "lineNumber": 3,
            }},
            { "type": "deprecation", "body": {} },
        ]);
        let v = violations(&batch);
        assert_eq!(v.len(), 1);
        let counted = v[0].counted(ORIGIN, "Chrome").expect("counted");
        assert_eq!(counted.page, "/decks/{id}");
        assert_eq!(counted.message, "script-src blocked eval");
    }

    #[test]
    fn every_policy_ends_by_saying_where_to_report() {
        use crate::state::{page_csp, CSP_REPORTING};
        assert!(page_csp(None).ends_with(CSP_REPORTING));
        assert!(page_csp(Some("https://media.example")).ends_with(CSP_REPORTING));
        assert!(crate::oauth::CONSENT_CSP.ends_with(CSP_REPORTING));
    }
}
