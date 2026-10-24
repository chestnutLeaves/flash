//! Every string and list a request can carry is bounded before a handler
//! sees it. The limits live in `flash_server::bounds`; each request
//! struct's field names its bound with `deserialize_with`, and the scan
//! (`flash_server::testing::bounded`) walks every `Deserialize` type in
//! the open core's shipped source and fails the build for a field of an
//! unbounded type that names none. A type that is not a request (a file
//! the server wrote itself, a parser's output that its own caps bound) is
//! listed below with the reason, and the list is checked against the
//! scan so it cannot rot. A downstream extension runs the same scan over
//! its own crate.
//!
//! What this buys: a new endpoint cannot accept an unbounded field by
//! omission, and the number that bounds it is in one table rather than
//! wherever someone remembered to check.

use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use flash_scan::{crate_at, workspace_crates};
use flash_server::bounds;
use flash_server::testing::bounded::{assert_requests_bounded, derives_deserialize, OPEN_CORE};
use flash_server::testing::{send, AppBuilder};

const NOT_A_REQUEST: &[(&str, &str)] = &[
    ("McpDeletionReceipt", "server-written confirmation result, read from the database for retry safety; never request input"),
    (
        "SpeechBlock",
        "a side's speech blocks as stored; requests carry them through the MCP layer's own bounded block type, and `speech::validate` caps their count and length",
    ),
    (
        "PendingConsent",
        "a durable ceremony the server wrote from an authorize request it had already bounded and validated",
    ),
    (
        "PendingImport",
        "the parked preview the server wrote itself; its rows come from the parser, whose caps bound them",
    ),
    (
        "ImportRow",
        "the parser's output for one card; the parser caps rows and field sizes, and the rich sides are SanitizedHtml",
    ),
    (
        "MediaRef",
        "one media reference the parser found in a field; filenames are sanitized and capped by the media module",
    ),
    (
        "ParsedImport",
        "a parser's whole result (rows and messages), built from a file the parser itself caps and parked by the server",
    ),
    (
        "ImportedSettings",
        "FSRS parameters read from a package's deck options; validated by building a scheduler before use",
    ),
    (
        "ImportNote",
        "what a parser says about a file (a message id and its numbers or capped text), parked by the server with its preview",
    ),
    (
        "NoteArg",
        "one variable of an ImportNote, made by the parser from a number or text it already capped",
    ),
    (
        "UndoRecord",
        "the record an Undo carries back, which the server wrote into the toast; it arrives in one text field bounded by `bounds::UNDO_RECORD`, and every card and deck in it is checked again as the caller's",
    ),
];

#[test]
fn every_request_string_and_list_names_its_bound() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let crates: Vec<_> = OPEN_CORE
        .iter()
        .map(|name| crate_at(&crates_dir.join(name)))
        .collect();
    assert_requests_bounded(&crates, NOT_A_REQUEST);
}

/// The scan reads real declarations: it must find the server's own
/// request structs, or a broken parser would pass vacuously.
#[test]
fn the_scan_sees_the_request_structs() {
    let mut names = Vec::new();
    for file in crate_at(Path::new(env!("CARGO_MANIFEST_DIR"))).source_files() {
        names.extend(
            file.types
                .iter()
                .filter(|t| derives_deserialize(t))
                .map(|t| t.name.clone()),
        );
    }
    for expected in [
        "QuickAddBody",
        "AuthorizeParams",
        "CreateCardsArgs",
        "NoteForm",
    ] {
        assert!(names.contains(&expected.to_string()), "{expected} not seen");
    }
}

/// Request data that never passes through a `Deserialize` struct. Path
/// segments and raw queries are bounded for every handler at once by
/// `middleware::uri_length` (probed below). A multipart body is not:
/// each handler reading one must name a bound in its own body.
#[test]
fn every_handler_reading_a_multipart_body_names_a_bound() {
    let mut offenders = Vec::new();
    let mut readers = 0;
    for krate in workspace_crates() {
        for file in krate.source_files() {
            // A function names a bound when a `bounds::…` path sits in its
            // body; a thin wrapper that hands the body to such a function
            // in the same file is bounded by it.
            let names_bound = |name: &str| {
                file.paths.iter().any(|p| {
                    p.function.as_deref() == Some(name) && p.segments.iter().any(|s| s == "bounds")
                })
            };
            let delegates_to_bounded = |name: &str| {
                file.calls
                    .iter()
                    .filter(|c| c.function.as_deref() == Some(name))
                    .filter_map(|c| c.path.last())
                    .any(|callee| file.function(callee).is_some() && names_bound(callee))
            };
            for function in &file.functions {
                let reads = function
                    .params
                    .iter()
                    .any(|p| p.ty.idents.iter().any(|i| i == "Multipart"));
                if !reads {
                    continue;
                }
                readers += 1;
                if !names_bound(&function.name) && !delegates_to_bounded(&function.name) {
                    offenders.push(format!("{}/{}: {}", krate.name, file.rel, function.name));
                }
            }
        }
    }
    assert!(readers >= 2, "the scan found {readers} multipart readers");
    assert!(
        offenders.is_empty(),
        "handlers reading a multipart body with no bound in the body:\n{}",
        offenders.join("\n")
    );
}

/// The bound on every path segment and query string: a URL past
/// `bounds::URI` is refused before any handler sees it.
#[tokio::test]
async fn a_url_past_the_cap_is_refused_before_routing() {
    let t = AppBuilder::new("bounded-uri").build();
    let long = "a".repeat(bounds::URI + 1);
    for uri in [format!("/{long}"), format!("/?{long}")] {
        let res = send(&t.app, Request::get(&uri).body(Body::empty()).unwrap()).await;
        assert_eq!(res.status, StatusCode::URI_TOO_LONG, "{}", &uri[..20]);
    }
    let fine = "a".repeat(bounds::URI - 2);
    let res = send(
        &t.app,
        Request::get(format!("/{fine}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(res.status, StatusCode::URI_TOO_LONG);
}
