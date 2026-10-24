//! Changes of several steps land whole or not at all: an enrollment that
//! fails part way leaves the invite working and no account behind, and an
//! import commit is claimed so a retry never adds its cards twice.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_core::UserId;
use flash_server::auth::hash_token;
use flash_server::ext::{AccountHooks, SignupInvite};
use flash_server::flows::enroll;
use flash_server::service::now_ms;
use flash_store::{Connection, NewJob, StoreError};
use serde_json::json;

/// An extension whose enrollment hook writes what a real one would (a job
/// it owes), then fails, as a crash part way through would stop it.
struct FailingHooks;

impl AccountHooks for FailingHooks {
    fn user_enrolled(
        &self,
        conn: &Connection,
        user: UserId,
        _signup: Option<&SignupInvite>,
        _invite_token_hash: &str,
        now_ms: i64,
    ) -> flash_store::Result<()> {
        flash_store::enqueue_job(
            conn,
            &NewJob {
                kind: "task",
                dedupe_key: None,
                user: Some(user),
                payload: "{}",
                run_at: now_ms,
            },
            now_ms,
        )?;
        Err(StoreError::Invalid("the hook fell over".into()))
    }
}

#[test]
fn an_enrollment_that_fails_part_way_leaves_the_invite_and_no_account() {
    let t = AppBuilder::new("atomic-enroll").build();
    let token_hash = hash_token("invite-token");
    let now = now_ms();
    t.store
        .create_invite(
            &token_hash,
            "FlashTester",
            "member",
            None,
            now + 60_000,
            now,
        )
        .unwrap();
    let enrollment = enroll::load(&t.store, &FailingHooks, &token_hash, now)
        .unwrap()
        .unwrap();

    let outcome = enroll::with_password(
        &t.store,
        &FailingHooks,
        &token_hash,
        &enrollment,
        "flashtester@example.com",
        "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNoaGFzaA",
        now,
    );
    assert!(outcome.is_err());
    assert!(
        !t.store.email_taken("flashtester@example.com").unwrap(),
        "no account"
    );
    assert!(
        t.store.lookup_invite(&token_hash, now).unwrap().is_some(),
        "the invite still works"
    );
    let owed = t.store.job_counts(now + 1).unwrap();
    assert_eq!(owed.due, 0, "nothing owed to an account that never was");
}

fn upload(bearer: &str) -> Request<Body> {
    let boundary = "flashboundary";
    let csv = "front,back\nCell,Unit of life\nATP,Energy\n";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"deck\"\r\n\r\nBio\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"c.csv\"\r\n\
         Content-Type: text/csv\r\n\r\n{csv}\r\n--{boundary}--\r\n"
    );
    Request::post("/api/v1/import/preview")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn card_count(t: &TestApp, user: UserId) -> i64 {
    t.store
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM cards WHERE user_id = ?1",
                [user.raw()],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}

/// The commit a crash cut off before it could clean up, retried: one
/// that finished answers what it did and adds nothing; one cut off part
/// way says so and adds nothing either.
#[tokio::test]
async fn a_retried_import_commit_never_adds_its_cards_twice() {
    let t = AppBuilder::new("atomic-import").build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let commit = |token: String| {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            send(
                &app,
                json_post(
                    "/api/v1/import/commit",
                    Some(&bearer),
                    &json!({"token": token, "deck": "Bio"}),
                ),
            )
            .await
        }
    };
    let preview = || {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            send(&app, upload(&bearer)).await.json()["token"]
                .as_str()
                .unwrap()
                .to_string()
        }
    };

    // Finished before, its reply lost, its preview still on disk.
    let token = preview().await;
    t.store
        .claim_import(user, &hash_token(&token), now_ms())
        .unwrap();
    t.store
        .finish_import(user, &hash_token(&token), 2, now_ms())
        .unwrap();
    let reply = commit(token).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(card_count(&t, user), 0, "nothing added a second time");

    // Cut off part way.
    let token = preview().await;
    t.store
        .claim_import(user, &hash_token(&token), now_ms())
        .unwrap();
    let reply = commit(token).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert!(
        reply.text().contains("cut off part way"),
        "{}",
        reply.text()
    );
    assert_eq!(card_count(&t, user), 0);

    // A fresh preview imports as ever, once.
    let token = preview().await;
    assert_eq!(commit(token.clone()).await.status, StatusCode::OK);
    assert_eq!(card_count(&t, user), 2);
}
