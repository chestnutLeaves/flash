//! Import over the API: a CSV preview (parked on disk under a token),
//! the commit that inserts it, the deck-name and expired-token refusals.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use serde_json::json;

fn upload(path: &str, bearer: &str, filename: &str, bytes: &[u8], deck: &str) -> Request<Body> {
    let boundary = "flashboundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"deck\"\r\n\r\n{deck}\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: text/csv\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::post(path)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

const CSV: &[u8] =
    b"front,back,tags,deck\nCell,Unit of life,bio,Bio::Cells\nATP,Energy,bio,Bio::Energy\n";

#[tokio::test]
async fn a_new_preview_replaces_the_previous_one() {
    // One pending import per user: repeated previews cannot pile packages
    // up on the disk that also holds the database.
    let t = AppBuilder::new("api-import-replace").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, flashtester);

    let first = send(
        &t.app,
        upload("/api/v1/import/preview", &bearer, "one.csv", CSV, ""),
    )
    .await
    .json()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let second = send(
        &t.app,
        upload("/api/v1/import/preview", &bearer, "two.csv", CSV, ""),
    )
    .await
    .json()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first, second);

    let stale = send(
        &t.app,
        json_post(
            "/api/v1/import/commit",
            Some(&bearer),
            &json!({"token": first, "deck": "Biology"}),
        ),
    )
    .await;
    assert_eq!(stale.status, StatusCode::BAD_REQUEST, "{}", stale.text());
    let live = send(
        &t.app,
        json_post(
            "/api/v1/import/commit",
            Some(&bearer),
            &json!({"token": second, "deck": "Biology"}),
        ),
    )
    .await;
    assert_eq!(live.status, StatusCode::OK, "{}", live.text());
}

/// A file that names several decks keeps them when the client asks: the
/// preview lists them with their cards, each arrives as the path it names
/// with the decks on the way made, and the deck named at commit is an
/// optional place to put the lot. A path that cannot be made stops the
/// import before the first card.
#[tokio::test]
async fn a_file_with_several_decks_keeps_them_nested() {
    let t = AppBuilder::new("api-import-nested").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, flashtester);
    let preview = |deck: &'static str| {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            send(
                &app,
                upload("/api/v1/import/preview", &bearer, "c.csv", CSV, deck),
            )
            .await
            .json()
        }
    };
    let commit = |body: serde_json::Value| {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            send(
                &app,
                json_post("/api/v1/import/commit", Some(&bearer), &body),
            )
            .await
        }
    };
    let names = || {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            let decks = send(&app, json_get("/api/v1/decks", Some(&bearer)))
                .await
                .json();
            decks["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| (d["name"].as_str().unwrap().to_string(), d["new"].clone()))
                .collect::<Vec<_>>()
        }
    };

    let p = preview("").await;
    assert_eq!(
        p["file_decks"],
        json!([{"name": "Bio::Cells", "cards": 1}, {"name": "Bio::Energy", "cards": 1}])
    );
    assert_eq!(p["parent"], "");
    // No deck named: the file's decks land at the top level, and "Bio",
    // which no row names, is made to hold them.
    let reply = commit(json!({"token": p["token"], "keep_decks": true})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json()["imported"], 2);
    assert_eq!(
        names().await,
        vec![
            ("Bio".to_string(), json!(2)),
            ("Bio::Cells".to_string(), json!(1)),
            ("Bio::Energy".to_string(), json!(1)),
        ]
    );

    // A deck named: the whole tree goes under it, and what was typed at
    // upload comes back as the prefill.
    let p = preview("School").await;
    assert_eq!(p["parent"], "School");
    let reply = commit(json!({"token": p["token"], "deck": " School ", "keep_decks": true})).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let all: Vec<String> = names().await.into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        all,
        vec![
            "Bio",
            "Bio::Cells",
            "Bio::Energy",
            "School",
            "School::Bio",
            "School::Bio::Cells",
            "School::Bio::Energy"
        ]
    );

    // A parent that pushes a path past the bound: nothing is made, not
    // even the decks that would have fit.
    let p = preview("").await;
    let long = "x".repeat(195);
    let reply = commit(json!({"token": p["token"], "deck": long, "keep_decks": true})).await;
    assert_ne!(reply.status, StatusCode::OK);
    assert!(
        reply.text().contains("longer than 200 bytes"),
        "{}",
        reply.text()
    );
    assert_eq!(names().await.len(), 7, "nothing was made");
}

/// The merge, which is also the contract with an app built before decks
/// nested: it sends no `keep_decks`, is shown one destination with a note
/// about the decks that merge into it, and gets exactly that.
#[tokio::test]
async fn preview_then_commit() {
    let t = AppBuilder::new("api-import").build();
    let flashtester = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, flashtester);

    let reply = send(
        &t.app,
        upload("/api/v1/import/preview", &bearer, "cards.csv", CSV, ""),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let p = reply.json();
    assert_eq!(p["total"], 2);
    assert_eq!(p["file_name"], "cards.csv");
    assert_eq!(p["deck"], "Bio");
    assert!(p["deck_note"].as_str().unwrap().contains("2 decks"));
    assert_eq!(p["sample"][0]["front"], "Cell");
    assert_eq!(p["progress_reviews"], 0);
    let token = p["token"].as_str().unwrap().to_string();

    // A blank deck name is refused; the preview survives the refusal.
    let reply = send(
        &t.app,
        json_post(
            "/api/v1/import/commit",
            Some(&bearer),
            &json!({"token": token, "deck": "  "}),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);

    let reply = send(
        &t.app,
        json_post(
            "/api/v1/import/commit",
            Some(&bearer),
            &json!({"token": token, "deck": "Biology", "colors": "keep"}),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json()["imported"], 2);
    let decks = send(&t.app, json_get("/api/v1/decks", Some(&bearer)))
        .await
        .json();
    assert_eq!(decks["items"][0]["name"], "Biology");

    // The parked preview is gone after commit.
    let reply = send(
        &t.app,
        json_post(
            "/api/v1/import/commit",
            Some(&bearer),
            &json!({"token": token, "deck": "Biology"}),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(reply.json()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("expired"));

    // Garbage files say why.
    let reply = send(
        &t.app,
        upload("/api/v1/import/preview", &bearer, "x.apkg", b"nope", ""),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(reply.json()["error"]["code"], "invalid");
}
