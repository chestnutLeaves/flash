//! Card generation end to end against the scripted provider: a source
//! goes up, a job is queued, the runner takes it through the model, the
//! drafts are reviewed over the API, and what is kept is an ordinary
//! note with its lineage. Also the ways it stops: a model failure, a
//! restart, a disabled input, another user's ids, a server with no
//! model, and a plan that says no.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_core::{AiJobId, DeckId, NoteId, UserId};
use flash_server::ai::fake::{FakeProvider, FakeReply, Step};
use flash_server::ai::provider::{DraftNote, LlmError, Revision, SourceKind, Usage};
use flash_server::ai::runner;
use flash_server::policy::{
    AiAdmission, AiInput, AiPolicy, AiRequest, AiUsage, RecordedCall, ALL_AI_INPUTS,
};
use flash_server::service::now_ms;
use flash_store::notes::NoteType;
use flash_store::{AiJobState, Store, StoreError};
use serde_json::{json, Value};

const PASSAGE: &str = "The heart has four chambers: two atria and two ventricles. Blood enters \
                       the right atrium and leaves for the lungs through the pulmonary valve.";

struct Gen {
    t: TestApp,
    fake: Arc<FakeProvider>,
    user: UserId,
    bearer: String,
    deck: DeckId,
}

fn generation_app(tag: &str, fake: Arc<FakeProvider>) -> Gen {
    generation_app_with(tag, fake, ALL_AI_INPUTS.to_vec(), None)
}

fn generation_app_with(
    tag: &str,
    fake: Arc<FakeProvider>,
    inputs: Vec<AiInput>,
    policy: Option<Arc<dyn AiPolicy>>,
) -> Gen {
    let mut builder = AppBuilder::new(tag).ai(fake.clone(), inputs);
    if let Some(policy) = policy {
        builder = builder.ai_policy(policy);
    }
    let t = builder.build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let deck = t
        .services
        .create_deck(user, "Anatomy", "", now_ms())
        .unwrap();
    Gen {
        t,
        fake,
        user,
        bearer,
        deck,
    }
}

async fn post(g: &Gen, path: &str, body: Value) -> Reply {
    send(&g.t.app, json_post(path, Some(&g.bearer), &body)).await
}

async fn get_json(g: &Gen, path: &str) -> Reply {
    send(&g.t.app, json_get(path, Some(&g.bearer))).await
}

async fn text_source(g: &Gen, text: &str) -> Value {
    let r = post(g, "/api/v1/ai/sources", json!({ "text": text })).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    r.json()
}

async fn queue_job(g: &Gen, source_id: i64, instruction: &str) -> i64 {
    queue_set(g, &[source_id], instruction).await
}

async fn queue_set(g: &Gen, source_ids: &[i64], instruction: &str) -> i64 {
    let r = post(
        g,
        "/api/v1/ai/jobs",
        json!({ "source_ids": source_ids, "deck_id": g.deck.0, "instruction": instruction }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    r.json()["job_id"].as_i64().unwrap()
}

async fn upload(g: &Gen, name: &str, bytes: &[u8]) -> Reply {
    send(
        &g.t.app,
        multipart("/api/v1/ai/sources", &g.bearer, &[(name, bytes)]),
    )
    .await
}

async fn uploaded(g: &Gen, name: &str, bytes: &[u8]) -> Value {
    let r = upload(g, name, bytes).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    r.json()
}

async fn job_json(g: &Gen, job: i64) -> Value {
    let r = get_json(g, &format!("/api/v1/ai/jobs/{job}")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    r.json()
}

async fn drafts_json(g: &Gen, job: i64) -> Vec<Value> {
    let r = get_json(g, &format!("/api/v1/ai/jobs/{job}/drafts")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    r.json()["items"].as_array().cloned().unwrap()
}

/// `{ items, active_job_id }` of the review `job` belongs to.
async fn turns_json(g: &Gen, job: i64) -> Value {
    let r = get_json(g, &format!("/api/v1/ai/jobs/{job}/turns")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    r.json()
}

/// One message on the review, accepted; the pass it queued.
async fn edit(g: &Gen, job: i64, message: &str) -> i64 {
    let r = post(
        g,
        &format!("/api/v1/ai/jobs/{job}/edit"),
        json!({ "message": message }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    assert!(r.json()["turn_id"].is_number(), "{}", r.text());
    r.json()["job_id"].as_i64().unwrap()
}

/// The turns as `(role, text, child_state)`.
fn turn_list(turns: &Value) -> Vec<(String, String, Option<String>)> {
    turns["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["role"].as_str().unwrap().to_string(),
                t["text"].as_str().unwrap().to_string(),
                t["child_state"].as_str().map(str::to_string),
            )
        })
        .collect()
}

fn card(front: &str, back: &str) -> DraftNote {
    DraftNote {
        note_type: NoteType::Basic,
        front: front.into(),
        back: back.into(),
        tags: vec!["valves".into()],
        figures: vec![],
    }
}

/// One pass of the runner, then the job polled the way a client does
/// until it is done or failed.
async fn run_to_end(g: &Gen, job: i64) -> Value {
    let started = runner::run_once(&g.t.state).await;
    assert!(started >= 1, "the runner started nothing");
    for _ in 0..50 {
        let j = job_json(g, job).await;
        if matches!(j["state"].as_str(), Some("done") | Some("failed")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("job {job} never reached a final state");
}

fn steps(fake: &FakeProvider) -> Vec<Step> {
    fake.calls().iter().map(|c| c.step).collect()
}

fn count(store: &Store, sql: &str) -> i64 {
    store
        .with_conn(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

fn multipart(path: &str, bearer: &str, files: &[(&str, &[u8])]) -> Request<Body> {
    let boundary = "flashboundary";
    let mut body = Vec::new();
    for (name, bytes) in files {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
                 filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Request::post(path)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .body(Body::from(body))
        .unwrap()
}

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 1, 2, 3,
];

#[tokio::test]
async fn pasted_text_becomes_drafts_that_keep_as_notes_with_lineage() {
    let g = generation_app("ai-gen-text", Arc::new(FakeProvider::default()));

    // The source answers with its preflight.
    let created = text_source(&g, PASSAGE).await;
    let source_id = created["id"].as_i64().unwrap();
    assert_eq!(created["kind"], "text");
    assert_eq!(created["line"], "~24 words");
    assert_eq!(created["size"], PASSAGE.len());
    assert!(created["origin_mime"].is_null());
    let preflight = &created["preflight"];
    assert_eq!(preflight["allowed"], true, "{preflight}");
    assert_eq!(preflight["kind"], "text");
    assert_eq!(preflight["cost_class"], "text");
    assert!(preflight["est_words"].as_u64().unwrap() > 10);
    assert!(preflight["code"].is_null() && preflight["reason"].is_null());
    assert!(preflight["quota"]["allowance_used_pct"].is_null());
    let again = get_json(&g, &format!("/api/v1/ai/sources/{source_id}/preflight")).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json()["allowed"], true);

    // Queued, then run.
    let job = queue_job(&g, source_id, "focus on the valves").await;
    let queued = job_json(&g, job).await;
    assert_eq!(queued["state"], "queued");
    assert_eq!(queued["sources"][0]["kind"], "text");
    assert_eq!(queued["sources_expired"], 0);
    assert_eq!(queued["deck_id"], g.deck.0);
    assert_eq!(queued["root_job_id"], job);
    // A second job while one is queued is refused.
    let second = post(
        &g,
        "/api/v1/ai/jobs",
        json!({ "source_ids": [source_id], "deck_id": g.deck.0 }),
    )
    .await;
    assert_eq!(second.status, StatusCode::BAD_REQUEST);
    assert!(
        second.text().contains("already running"),
        "{}",
        second.text()
    );

    let done = run_to_end(&g, job).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["progress"], 100);
    assert_eq!(done["counts"]["pending"], 3);
    assert!(done["error"].is_null());
    let calls = g.fake.calls();
    assert_eq!(steps(&g.fake), [Step::Extract, Step::Generate]);
    assert_eq!(calls[0].kind(), Some(SourceKind::Text));
    assert_eq!(calls[0].sources, ["Pasted text"]);
    assert_eq!(calls[1].instruction.as_deref(), Some("focus on the valves"));

    // The drafts: the canned three, sanitized, the unresolvable figure
    // token gone (a text source has no pages to cut from).
    let drafts = drafts_json(&g, job).await;
    assert_eq!(drafts.len(), 3);
    let types: Vec<&str> = drafts
        .iter()
        .map(|d| d["note_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["basic", "cloze", "basic_typed"]);
    assert!(drafts.iter().all(|d| d["status"] == "pending"));
    assert_eq!(
        drafts[0]["front_html"],
        "How many chambers does the heart have?"
    );
    assert_eq!(drafts[0]["tags"], json!(["anatomy", "heart"]));
    assert_eq!(drafts[0]["figure_media_ids"], json!([]));
    let ids: Vec<i64> = drafts.iter().map(|d| d["id"].as_i64().unwrap()).collect();

    // Keep two of them.
    let kept = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/keep"),
        json!({ "draft_ids": [ids[0], ids[1]] }),
    )
    .await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.text());
    let report = kept.json();
    assert_eq!(report["kept"], 2);
    assert_eq!(report["cards"], 2, "a basic and a one-blank cloze");
    assert_eq!(report["remaining_pending"], 1);
    assert!(report["stopped_by"].is_null());

    let drafts = drafts_json(&g, job).await;
    assert_eq!(drafts[0]["status"], "kept");
    assert_eq!(drafts[1]["status"], "kept");
    assert_eq!(drafts[2]["status"], "pending");
    for d in &drafts[..2] {
        let note =
            g.t.store
                .get_note(g.user, NoteId(d["note_id"].as_i64().unwrap()))
                .unwrap()
                .expect("the kept draft's note exists");
        assert_eq!(note.ai_job_id, Some(AiJobId(job)));
        assert_eq!(note.deck_id, g.deck);
    }
    assert_eq!(
        g.t.store.count_cards(g.user, Some(g.deck), None).unwrap(),
        2
    );
    assert_eq!(
        job_json(&g, job).await["counts"],
        json!({"pending": 1, "kept": 2, "discarded": 0})
    );

    // Edit, then discard, the third.
    let edited = send(
        &g.t.app,
        json_patch(
            &format!("/api/v1/ai/drafts/{}", ids[2]),
            Some(&g.bearer),
            &json!({ "back_html": "<p>The <b>pulmonary</b> valve</p><script>x</script>" }),
        ),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text());
    assert_eq!(edited.json()["status"], "edited");
    assert_eq!(
        edited.json()["back_html"],
        "<p>The <b>pulmonary</b> valve</p>"
    );
    let discarded = send(
        &g.t.app,
        json_patch(
            &format!("/api/v1/ai/drafts/{}", ids[2]),
            Some(&g.bearer),
            &json!({ "status": "discarded" }),
        ),
    )
    .await;
    assert_eq!(discarded.status, StatusCode::OK);
    assert_eq!(discarded.json()["status"], "discarded");
    let twice = send(
        &g.t.app,
        json_patch(
            &format!("/api/v1/ai/drafts/{}", ids[2]),
            Some(&g.bearer),
            &json!({ "front_html": "<p>x</p>" }),
        ),
    )
    .await;
    assert_eq!(twice.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        job_json(&g, job).await["counts"],
        json!({"pending": 0, "kept": 2, "discarded": 1})
    );

    // Two successful calls in the ledger, one of them a generation.
    assert_eq!(
        count(&g.t.store, "SELECT COUNT(*) FROM ai_calls WHERE ok = 1"),
        2
    );
    assert_eq!(g.t.store.ai_usage_since(g.user, 0).unwrap().generations, 1);
    let quota = get_json(&g, "/api/v1/ai/quota").await;
    assert_eq!(quota.status, StatusCode::OK);
    assert!(
        quota.json()["allowance_used_pct"].is_null(),
        "the open policy meters nothing"
    );
    assert_eq!(quota.json()["inputs"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn regenerate_keeps_the_kept_rows_and_more_appends() {
    let g = generation_app("ai-gen-children", Arc::new(FakeProvider::default()));
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    let first = drafts_json(&g, job).await;
    let keep_one = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/keep"),
        json!({ "draft_ids": [first[0]["id"]] }),
    )
    .await;
    assert_eq!(keep_one.status, StatusCode::OK);

    // Regenerate: a child job; the kept row stays, the two open ones are
    // rewritten under the instruction.
    let r = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/regenerate"),
        json!({ "instruction": "shorter" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let child = r.json()["job_id"].as_i64().unwrap();
    assert_ne!(child, job);
    let done = run_to_end(&g, child).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["op"], "revise");
    assert_eq!(done["parent_job_id"], job);
    assert_eq!(done["root_job_id"], job);
    let revised = drafts_json(&g, child).await;
    assert_eq!(revised.len(), 3);
    assert_eq!(revised[0]["status"], "kept");
    assert!(revised[0]["note_id"].is_number());
    assert_eq!(revised[1]["status"], "pending");
    assert!(
        revised[2]["back_html"]
            .as_str()
            .unwrap()
            .ends_with("(shorter)"),
        "{}",
        revised[2]["back_html"]
    );
    let calls = g.fake.calls();
    assert_eq!(calls[2].step, Step::Revise);
    assert_eq!(calls[2].existing, 2, "only the open drafts are revised");
    assert!(calls[2].extraction_present);
    assert_eq!(
        steps(&g.fake)
            .iter()
            .filter(|s| **s == Step::Extract)
            .count(),
        1,
        "the reading is reused"
    );

    // Ask for more: appended after the existing ones, the model having
    // seen every draft that was not discarded.
    let r = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/more"),
        json!({ "instruction": "the valves" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let more = r.json()["job_id"].as_i64().unwrap();
    let done = run_to_end(&g, more).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["op"], "more");
    let all = drafts_json(&g, job).await;
    assert_eq!(all.len(), 6);
    assert_eq!(all[0]["status"], "kept");
    assert!(all[3..].iter().all(|d| d["status"] == "pending"));
    let ords: Vec<u64> = all.iter().map(|d| d["ord"].as_u64().unwrap()).collect();
    assert_eq!(ords, [0, 1, 2, 3, 4, 5]);
    let calls = g.fake.calls();
    assert_eq!(calls[3].step, Step::More);
    assert_eq!(calls[3].existing, 3);
    assert_eq!(calls[3].instruction.as_deref(), Some("the valves"));

    // Both older passes show in the review's thread, so a person who
    // opens it in a newer client sees what was asked.
    let turns = turns_json(&g, job).await;
    assert_eq!(
        turn_list(&turns),
        [
            ("user".into(), "shorter".into(), Some("done".into())),
            (
                "model".into(),
                "Rewrote 2 cards.".into(),
                Some("done".into())
            ),
            ("user".into(), "the valves".into(), Some("done".into())),
            ("model".into(), "Added 3 cards.".into(), Some("done".into())),
        ]
    );
    assert!(turns["active_job_id"].is_null());

    // Discard all: the open five go, the kept one stays.
    let r = post(&g, &format!("/api/v1/ai/jobs/{job}/discard"), json!({})).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json()["discarded"], 5);
    assert_eq!(
        job_json(&g, job).await["counts"],
        json!({"pending": 0, "kept": 1, "discarded": 5})
    );
}

#[tokio::test]
async fn a_message_on_the_review_edits_the_open_drafts_and_keeps_the_thread() {
    let fake = Arc::new(FakeProvider::default());
    let g = generation_app("ai-gen-thread", fake.clone());
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    let first = drafts_json(&g, job).await;
    let kept_id = first[0]["id"].as_i64().unwrap();
    let keep_one = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/keep"),
        json!({ "draft_ids": [kept_id] }),
    )
    .await;
    assert_eq!(keep_one.status, StatusCode::OK);
    let kept_note = drafts_json(&g, job).await[0]["note_id"].clone();
    assert!(turn_list(&turns_json(&g, job).await).is_empty());

    // The first message: a user turn at once, the pass named as active.
    let pass = edit(&g, job, "shorter").await;
    assert_ne!(pass, job);
    let turns = turns_json(&g, job).await;
    assert_eq!(
        turn_list(&turns),
        [("user".into(), "shorter".into(), Some("queued".into()))]
    );
    assert_eq!(turns["active_job_id"], pass);
    assert_eq!(turns["items"][0]["child_job_id"], pass);
    // The thread reads the same by the pass's id.
    assert_eq!(turns_json(&g, pass).await["items"], turns["items"]);

    // A second message while it runs is refused; the thread is unchanged.
    let r = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/edit"),
        json!({ "message": "and longer" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(r.text().contains("already running"), "{}", r.text());
    assert_eq!(turn_list(&turns_json(&g, job).await).len(), 1);

    let done = run_to_end(&g, pass).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["op"], "revise");
    let revised = drafts_json(&g, job).await;
    assert_eq!(revised.len(), 3);
    assert_eq!(revised[0]["id"], kept_id, "the kept row keeps its id");
    assert_eq!(revised[0]["note_id"], kept_note);
    assert!(revised[1]["back_html"]
        .as_str()
        .unwrap()
        .ends_with("(shorter)"));
    let turns = turns_json(&g, job).await;
    assert_eq!(
        turn_list(&turns),
        [
            ("user".into(), "shorter".into(), Some("done".into())),
            (
                "model".into(),
                "Rewrote 2 cards.".into(),
                Some("done".into())
            ),
        ]
    );
    assert!(turns["active_job_id"].is_null());
    let call = &fake.calls()[2];
    assert_eq!(call.step, Step::Revise);
    assert_eq!(call.instruction.as_deref(), Some("shorter"));
    assert_eq!(
        (call.existing, call.kept, call.thread),
        (2, 1, 0),
        "the open drafts, the kept card as context, no earlier turns"
    );

    // The second message: the model drops both open drafts and adds
    // one, and the kept row still does not move.
    fake.script_push(FakeReply::Revision(Ok(Revision {
        cards: vec![card(
            "Which valve opens into the aorta?",
            "The aortic valve",
        )],
        reply: "Dropped two and added one on the aortic valve.".into(),
    })));
    let pass2 = edit(&g, job, "drop those and add one on the aortic valve").await;
    assert_eq!(run_to_end(&g, pass2).await["state"], "done");
    let after = drafts_json(&g, job).await;
    assert_eq!(after.len(), 2, "{after:?}");
    assert_eq!(after[0]["id"], kept_id);
    assert_eq!(after[0]["status"], "kept");
    assert_eq!(after[1]["status"], "pending");
    assert!(after[1]["back_html"]
        .as_str()
        .unwrap()
        .contains("aortic valve"));
    assert_eq!(after[1]["tags"], json!(["valves"]));
    let call = &fake.calls()[3];
    assert_eq!(call.thread, 2, "the earlier exchange travels with it");
    let turns = turn_list(&turns_json(&g, job).await);
    assert_eq!(turns.len(), 4);
    assert_eq!(turns[3].1, "Dropped two and added one on the aortic valve.");

    // A message is a generation the plan counts, like the first pass.
    assert_eq!(g.t.store.ai_usage_since(g.user, 0).unwrap().generations, 3);
    assert_eq!(
        count(
            &g.t.store,
            "SELECT COUNT(*) FROM ai_calls WHERE step = 'revise' AND ok = 1"
        ),
        2
    );

    // Blank messages are not taken.
    for blank in ["", "   \n"] {
        let r = post(
            &g,
            &format!("/api/v1/ai/jobs/{job}/edit"),
            json!({ "message": blank }),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
        assert!(r.text().contains("Say what"), "{}", r.text());
    }
    assert_eq!(turn_list(&turns_json(&g, job).await).len(), 4);
}

#[tokio::test]
async fn an_edit_that_fails_leaves_the_drafts_and_says_so_in_the_thread() {
    let fake = Arc::new(FakeProvider::default());
    let g = generation_app("ai-gen-thread-failure", fake.clone());
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    let before = drafts_json(&g, job).await;

    // The model declines: the pass fails, the drafts stay, and the
    // failure's sentence is the model's turn in the thread.
    fake.script_push(FakeReply::Revision(Err(LlmError::refused())));
    let pass = edit(&g, job, "make them rude").await;
    let failed = run_to_end(&g, pass).await;
    assert_eq!(failed["state"], "failed");
    assert_eq!(drafts_json(&g, job).await, before);
    let turns = turn_list(&turns_json(&g, job).await);
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[1].0, "model");
    assert_eq!(turns[1].1, LlmError::refused().user_message());
    assert_eq!(turns[1].2.as_deref(), Some("failed"));

    // The model answers with no cards at all: the drafts are left
    // alone rather than emptied, and the thread says so.
    fake.script_push(FakeReply::Revision(Ok(Revision {
        cards: vec![],
        reply: "Done.".into(),
    })));
    let pass = edit(&g, job, "remove everything").await;
    let failed = run_to_end(&g, pass).await;
    assert_eq!(failed["state"], "failed");
    assert!(
        failed["error"]
            .as_str()
            .unwrap()
            .contains("left as they were"),
        "{failed}"
    );
    assert_eq!(drafts_json(&g, job).await, before);
    let turns = turn_list(&turns_json(&g, job).await);
    assert_eq!(turns.len(), 4);
    assert_eq!(turns[3].2.as_deref(), Some("failed"));

    // The next message sees the person's earlier messages but not the
    // sentences of the passes that failed.
    let pass = edit(&g, job, "shorter").await;
    assert_eq!(run_to_end(&g, pass).await["state"], "done");
    let call = fake.calls().into_iter().last().unwrap();
    assert_eq!(call.step, Step::Revise);
    assert_eq!(call.thread, 2, "two user turns, no failed model turns");
    assert_eq!(turn_list(&turns_json(&g, job).await).len(), 6);
    assert_eq!(
        count(
            &g.t.store,
            "SELECT COUNT(*) FROM ai_calls WHERE step = 'revise' AND ok = 0"
        ),
        1,
        "only the refusal was a failed call"
    );
}

#[tokio::test]
async fn a_reply_the_provider_billed_and_then_declined_is_recorded_with_its_usage() {
    let fake = Arc::new(FakeProvider::default());
    let g = generation_app("ai-gen-billed-refusal", fake.clone());
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    // The provider read 5,000 tokens and wrote 40 before declining,
    // and says so: the ledger keeps the counts under the failed call.
    fake.script_push(FakeReply::Revision(Err(LlmError::refused().billed(
        Usage {
            response_id: Some("declined-1".into()),
            ..Usage::text(5_000, 40)
        },
    ))));
    let pass = edit(&g, job, "make them rude").await;
    assert_eq!(run_to_end(&g, pass).await["state"], "failed");
    let row: (i64, i64, i64, String, Option<String>) =
        g.t.store
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT input_tokens, output_tokens, ok, error, response_id FROM ai_calls \
                     WHERE step = 'revise'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?)
            })
            .unwrap();
    assert_eq!(
        row,
        (5_000, 40, 0, "refused".into(), Some("declined-1".into()))
    );
    // A transport failure billed nothing and records nothing.
    fake.script_push(FakeReply::Revision(Err(LlmError::Timeout)));
    let pass = edit(&g, job, "shorter").await;
    assert_eq!(run_to_end(&g, pass).await["state"], "failed");
    assert_eq!(
        count(
            &g.t.store,
            "SELECT COUNT(*) FROM ai_calls WHERE error = 'timeout' AND input_tokens = 0"
        ),
        1
    );
}

#[tokio::test]
async fn a_model_failure_leaves_a_failed_job_with_a_safe_sentence() {
    let fake = Arc::new(FakeProvider::scripted(vec![FakeReply::Extract(Err(
        LlmError::http(503, "upstream said: secret internal body"),
    ))]));
    let g = generation_app("ai-gen-failure", fake);
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    let failed = run_to_end(&g, job).await;
    assert_eq!(failed["state"], "failed");
    assert_eq!(failed["progress"], 0);
    let error = failed["error"].as_str().unwrap();
    assert_eq!(
        error,
        LlmError::http(503, "").user_message(),
        "the sentence, not the body"
    );
    assert!(!error.contains("secret") && !error.contains("503"));
    assert_eq!(
        count(&g.t.store, "SELECT COUNT(*) FROM ai_calls WHERE ok = 1"),
        0
    );
    assert_eq!(
        count(&g.t.store, "SELECT COUNT(*) FROM ai_calls WHERE ok = 0"),
        1
    );
    assert_eq!(
        count(
            &g.t.store,
            "SELECT COUNT(*) FROM ai_calls WHERE error LIKE '%secret%'"
        ),
        0
    );
    assert_eq!(g.t.store.ai_usage_since(g.user, 0).unwrap().generations, 0);
    // The failed job cannot be revised, but a new one can be queued.
    let r = post(&g, &format!("/api/v1/ai/jobs/{job}/regenerate"), json!({})).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let again = queue_job(&g, source, "").await;
    assert_eq!(run_to_end(&g, again).await["state"], "done");
}

#[tokio::test]
async fn interrupted_jobs_are_queued_again_at_boot_and_fail_the_second_time() {
    let g = generation_app("ai-gen-requeue", Arc::new(FakeProvider::default()));
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = AiJobId(queue_job(&g, source, "").await);
    let interrupt = |state: AiJobState| {
        g.t.store
            .set_ai_job_state(g.user, job, state, 40, None, now_ms())
            .unwrap();
    };
    interrupt(AiJobState::Extracting);
    assert_eq!(runner::requeue_interrupted(&g.t.state), 1);
    let row = g.t.store.get_ai_job(g.user, job).unwrap().unwrap();
    assert_eq!(
        (row.state, row.attempts, row.progress),
        (AiJobState::Queued, 1, 0)
    );
    // Once requeued it runs to the end like any other job.
    assert_eq!(run_to_end(&g, job.0).await["state"], "done");

    interrupt(AiJobState::Generating);
    assert_eq!(runner::requeue_interrupted(&g.t.state), 1);
    let row = g.t.store.get_ai_job(g.user, job).unwrap().unwrap();
    assert_eq!(row.state, AiJobState::Failed);
    assert_eq!(row.attempts, 2);
    assert_eq!(row.error.as_deref(), Some("interrupted twice"));
    assert_eq!(runner::requeue_interrupted(&g.t.state), 0);
}

#[tokio::test]
async fn the_same_text_is_read_once() {
    let g = generation_app("ai-gen-cache", Arc::new(FakeProvider::default()));
    let first = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, first, "").await;
    run_to_end(&g, job).await;
    let second = text_source(&g, &format!("  {PASSAGE}\n")).await["id"]
        .as_i64()
        .unwrap();
    assert_ne!(first, second, "a new source row");
    let job2 = queue_job(&g, second, "different instruction").await;
    assert_eq!(run_to_end(&g, job2).await["state"], "done");
    assert_eq!(
        steps(&g.fake),
        [Step::Extract, Step::Generate, Step::Generate],
        "no second extract call"
    );
    let a = g.t.store.get_ai_job(g.user, AiJobId(job)).unwrap().unwrap();
    let b =
        g.t.store
            .get_ai_job(g.user, AiJobId(job2))
            .unwrap()
            .unwrap();
    assert_eq!(a.extraction_id, b.extraction_id);
    assert!(a.extraction_id.is_some());
    assert_eq!(drafts_json(&g, job2).await.len(), 3);
}

#[tokio::test]
async fn uploads_are_classified_and_read_back_from_the_blob_store() {
    let g = generation_app("ai-gen-uploads", Arc::new(FakeProvider::default()));

    // A PDF, known by its bytes whatever it is called. No renderer in
    // the harness, so no page count and no figure crops.
    let pdf = uploaded(&g, "notes.bin", b"%PDF-1.7\n1 0 obj <<>> endobj\n%%EOF").await;
    assert_eq!(pdf["kind"], "pdf");
    assert_eq!(pdf["filename"], "notes.bin");
    assert!(pdf["preflight"]["pages"].is_null());
    assert_eq!(pdf["preflight"]["allowed"], true);
    let job = queue_job(&g, pdf["id"].as_i64().unwrap(), "").await;
    assert_eq!(run_to_end(&g, job).await["state"], "done");
    assert_eq!(g.fake.calls()[0].kind(), Some(SourceKind::Pdf));
    assert_eq!(g.fake.calls()[0].sources, ["notes.bin"]);
    let drafts = drafts_json(&g, job).await;
    assert_eq!(drafts.len(), 3);
    assert_eq!(
        drafts[0]["figure_media_ids"],
        json!([]),
        "no renderer, no crops, no images"
    );
    assert_eq!(
        count(&g.t.store, "SELECT COUNT(*) FROM media"),
        0,
        "a source is not a media row"
    );

    // Every photo is its own source; a job reads them together.
    let a = uploaded(&g, "a.png", PNG).await;
    let b = uploaded(&g, "b.png", PNG).await;
    assert_eq!(a["kind"], "image");
    assert_eq!(a["filename"], "a.png");
    assert_eq!(a["preflight"]["image_count"], 1);
    assert_eq!(a["line"], "1 photo");
    let job = queue_set(
        &g,
        &[a["id"].as_i64().unwrap(), b["id"].as_i64().unwrap()],
        "",
    )
    .await;
    assert_eq!(run_to_end(&g, job).await["state"], "done");
    assert_eq!(g.fake.calls()[2].kinds, [SourceKind::Image]);
    assert_eq!(g.fake.calls()[2].sources, ["a.png", "b.png"]);
    let detail = job_json(&g, job).await;
    assert_eq!(detail["sources"].as_array().unwrap().len(), 2);
    assert_eq!(detail["sources"][1]["filename"], "b.png");

    // Two files in one body is not how a set is made; junk is not a
    // source; an HTML page is text.
    let two = send(
        &g.t.app,
        multipart(
            "/api/v1/ai/sources",
            &g.bearer,
            &[("a.png", PNG), ("b.pdf", b"%PDF-1.7")],
        ),
    )
    .await;
    assert_eq!(two.status, StatusCode::BAD_REQUEST);
    assert!(two.text().contains("one file per upload"), "{}", two.text());
    let junk = upload(&g, "x.bin", b"\x00\x01\x02\x03 junk").await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    assert!(junk.text().contains("isn't supported"), "{}", junk.text());
    let html = uploaded(
        &g,
        "page.html",
        b"<html><body><p>Hello <b>there</b></p></body></html>",
    )
    .await;
    assert_eq!(html["kind"], "text");
    assert_eq!(html["filename"], "page.html");
    assert_eq!(html["preflight"]["est_words"], 2);
    let md = uploaded(&g, "notes.md", "# Heading\n\nOne two three.\n".as_bytes()).await;
    assert_eq!(md["kind"], "text");
    assert_eq!(md["preflight"]["est_words"], 5);
    let nul = upload(&g, "notes.txt", b"ab\x00cd").await;
    assert_eq!(nul.status, StatusCode::BAD_REQUEST);
    // An office file with no converter configured is refused with the
    // code a client hides the kind on.
    let docx = upload(&g, "slides.pptx", b"PK\x03\x04 pretend").await;
    assert_eq!(docx.status, StatusCode::FORBIDDEN, "{}", docx.text());
    assert_eq!(docx.json()["error"]["code"], "input_disabled");
    assert!(docx.json()["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Save it as a PDF"));

    // A video link, in its one form.
    let link = post(
        &g,
        "/api/v1/ai/sources",
        json!({ "youtube_url": "https://youtu.be/dQw4w9WgXcQ?si=share" }),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED, "{}", link.text());
    assert_eq!(link.json()["kind"], "youtube");
    assert_eq!(
        link.json()["url"],
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    );
    let elsewhere = post(
        &g,
        "/api/v1/ai/sources",
        json!({ "youtube_url": "https://vimeo.com/1234" }),
    )
    .await;
    assert_eq!(elsewhere.status, StatusCode::BAD_REQUEST);
    let both = post(
        &g,
        "/api/v1/ai/sources",
        json!({ "text": "x", "youtube_url": "https://youtu.be/dQw4w9WgXcQ" }),
    )
    .await;
    assert_eq!(both.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_set_of_several_kinds_is_read_once_in_order_and_charged_as_its_dearest() {
    let g = generation_app("ai-gen-set", Arc::new(FakeProvider::default()));
    let pdf = uploaded(&g, "chapter.pdf", b"%PDF-1.7 chapter").await;
    let text = text_source(&g, PASSAGE).await;
    let photo = uploaded(&g, "board.png", PNG).await;
    let ids = [
        pdf["id"].as_i64().unwrap(),
        text["id"].as_i64().unwrap(),
        photo["id"].as_i64().unwrap(),
    ];
    // The set's preflight adds the members up.
    let set = post(&g, "/api/v1/ai/preflight", json!({ "source_ids": ids })).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    let s = set.json();
    assert_eq!(s["allowed"], true, "{s}");
    assert_eq!(s["cost_class"], "pdf");
    assert_eq!(s["totals"]["sources"], 3);
    assert_eq!(s["totals"]["image_count"], 1);
    assert_eq!(s["totals"]["est_words"], 24);
    assert_eq!(s["line"], "3 sources · ~24 words · 1 photo");
    assert_eq!(s["sources"].as_array().unwrap().len(), 3);

    let job = queue_set(&g, &ids, "").await;
    assert_eq!(run_to_end(&g, job).await["state"], "done");
    let calls = g.fake.calls();
    assert_eq!(steps(&g.fake), [Step::Extract, Step::Generate]);
    assert_eq!(
        calls[0].kinds,
        [SourceKind::Pdf, SourceKind::Text, SourceKind::Image]
    );
    assert_eq!(
        calls[0].sources,
        ["chapter.pdf", "Pasted text", "board.png"]
    );
    let kinds: Vec<String> =
        g.t.store
            .with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT input_kind FROM ai_calls ORDER BY id")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .unwrap();
    assert_eq!(kinds, ["pdf", "pdf"], "the ledger charges the dearest kind");
    let detail = job_json(&g, job).await;
    let names: Vec<&str> = detail["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["filename"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["chapter.pdf", "", "board.png"]);
    assert_eq!(detail["sources"][1]["kind"], "text");

    // The same set again, in the same order, is read from the cache;
    // in another order it is read again, since the numbering the model
    // cited would be off.
    let pdf2 = uploaded(&g, "chapter.pdf", b"%PDF-1.7 chapter").await;
    let text2 = text_source(&g, PASSAGE).await;
    let photo2 = uploaded(&g, "board.png", PNG).await;
    let same = [
        pdf2["id"].as_i64().unwrap(),
        text2["id"].as_i64().unwrap(),
        photo2["id"].as_i64().unwrap(),
    ];
    let job2 = queue_set(&g, &same, "").await;
    assert_eq!(run_to_end(&g, job2).await["state"], "done");
    assert_eq!(
        steps(&g.fake),
        [Step::Extract, Step::Generate, Step::Generate],
        "the reading is reused"
    );
    let reversed = [same[2], same[1], same[0]];
    let job3 = queue_set(&g, &reversed, "").await;
    assert_eq!(run_to_end(&g, job3).await["state"], "done");
    assert_eq!(
        steps(&g.fake),
        [
            Step::Extract,
            Step::Generate,
            Step::Generate,
            Step::Extract,
            Step::Generate
        ]
    );
    // And a lone member keeps its own hash: the PDF alone is a fresh
    // reading the first time and a cached one the second.
    let job4 = queue_set(&g, &[same[0]], "").await;
    assert_eq!(run_to_end(&g, job4).await["state"], "done");
    let job5 = queue_set(&g, &[pdf["id"].as_i64().unwrap()], "").await;
    assert_eq!(run_to_end(&g, job5).await["state"], "done");
    let extracts = steps(&g.fake)
        .iter()
        .filter(|s| **s == Step::Extract)
        .count();
    assert_eq!(extracts, 3);
}

#[tokio::test]
async fn a_source_can_be_taken_back_and_the_composer_has_a_ceiling() {
    let g = generation_app("ai-gen-take-back", Arc::new(FakeProvider::default()));
    let a = text_source(&g, "alpha beta").await["id"].as_i64().unwrap();
    let b = uploaded(&g, "b.png", PNG).await["id"].as_i64().unwrap();
    let listed = get_json(&g, "/api/v1/ai/sources").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    let items = listed.json()["items"].as_array().cloned().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], b, "newest first");
    assert_eq!(items[1]["line"], "~2 words");

    let r = send(
        &g.t.app,
        json_delete(&format!("/api/v1/ai/sources/{b}"), Some(&g.bearer)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    assert_eq!(
        get_json(&g, "/api/v1/ai/sources").await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let r = send(
        &g.t.app,
        json_delete(&format!("/api/v1/ai/sources/{b}"), Some(&g.bearer)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // A member of a job stays with the job.
    let job = queue_job(&g, a, "").await;
    let r = send(
        &g.t.app,
        json_delete(&format!("/api/v1/ai/sources/{a}"), Some(&g.bearer)),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.text());
    assert_eq!(run_to_end(&g, job).await["state"], "done");
    assert!(get_json(&g, "/api/v1/ai/sources").await.json()["items"]
        .as_array()
        .unwrap()
        .is_empty());

    // Only so many may wait in the composer.
    for i in 0..flash_server::bounds::AI_UNATTACHED_SOURCES_PER_USER {
        text_source(&g, &format!("passage {i}")).await;
    }
    let r = post(&g, "/api/v1/ai/sources", json!({ "text": "one more" })).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(r.text().contains("waiting already"), "{}", r.text());
    let r = upload(&g, "z.png", PNG).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_picker_is_built_from_what_the_model_reads() {
    let g = generation_app("ai-gen-caps", Arc::new(FakeProvider::default()));
    let meta = send(&g.t.app, json_get("/api/v1/meta", None)).await.json();
    let caps = &meta["ai"]["caps"];
    let kinds: Vec<&str> = caps["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["text", "pdf", "image", "youtube", "video", "audio"]);
    assert_eq!(caps["youtube"], true);
    assert_eq!(caps["set"]["max_sources"], 40);
    assert_eq!(caps["lift"]["chars"], 1_000);
    assert!(caps["copy"]["document"]
        .as_str()
        .unwrap()
        .contains("Save it as a PDF"));
    assert!(caps["kinds"][0]["withheld"].is_null());
    assert!(caps["kinds"][2]["ext"]
        .as_array()
        .unwrap()
        .contains(&json!(".png")));

    // A model that reads only text and photos, on a server that lists
    // video, does not boot.
    let narrow =
        FakeProvider::default().with_capabilities(flash_server::ai::provider::Capabilities {
            kinds: vec![AiInput::Text, AiInput::Image],
            ..FakeProvider::everything()
        });
    let err = flash_server::state::AiRuntime::new(
        Arc::new(narrow),
        Some(vec![AiInput::Text, AiInput::Video]),
        #[cfg(feature = "pdf")]
        None,
        None,
        std::env::temp_dir(),
    )
    .err()
    .expect("a listed kind the model cannot read is a boot error");
    assert!(err.contains("video"), "{err}");
    // Unlisted, the server reads everything the model does, less pdf
    // when there is no renderer.
    let narrow =
        FakeProvider::default().with_capabilities(flash_server::ai::provider::Capabilities {
            kinds: vec![AiInput::Text, AiInput::Pdf, AiInput::Image],
            ..FakeProvider::everything()
        });
    let runtime = flash_server::state::AiRuntime::new(
        Arc::new(narrow),
        None,
        #[cfg(feature = "pdf")]
        None,
        None,
        std::env::temp_dir(),
    )
    .unwrap();
    assert_eq!(runtime.inputs, [AiInput::Text, AiInput::Image]);
}

#[tokio::test]
async fn an_input_this_server_does_not_read_is_refused_with_a_code() {
    let g = generation_app_with(
        "ai-gen-disabled",
        Arc::new(FakeProvider::default()),
        vec![AiInput::Text],
        None,
    );
    let link = post(
        &g,
        "/api/v1/ai/sources",
        json!({ "youtube_url": "https://youtu.be/dQw4w9WgXcQ" }),
    )
    .await;
    assert_eq!(link.status, StatusCode::FORBIDDEN, "{}", link.text());
    assert_eq!(link.json()["error"]["code"], "input_disabled");
    assert_eq!(
        link.json()["error"]["message"],
        "This server does not accept YouTube links."
    );
    let pdf = upload(&g, "x.pdf", b"%PDF-1.7").await;
    assert_eq!(pdf.status, StatusCode::FORBIDDEN);
    assert_eq!(
        pdf.json()["error"]["message"],
        "This server does not accept PDFs."
    );
    let meta = send(&g.t.app, json_get("/api/v1/meta", None)).await.json();
    assert_eq!(meta["features"]["ai"], true);
    assert_eq!(meta["ai"]["inputs"], json!(["text"]));
    assert_eq!(meta["ai"]["caps"]["kinds"].as_array().unwrap().len(), 1);
    assert_eq!(meta["ai"]["caps"]["youtube"], false);
    assert_eq!(meta["ai"]["max_words"], 15_000);
    assert_eq!(meta["ai"]["max_images"], 20);
    assert_eq!(meta["ai"]["max_pages"], 150);
    assert_eq!(meta["ai"]["max_media_minutes"], 90);
    // Clients send the device's transcript unless the operator says audio.
    assert_eq!(meta["ai"]["voice_input"], "transcript");
    // Text still works.
    assert_eq!(text_source(&g, PASSAGE).await["preflight"]["allowed"], true);
}

#[tokio::test]
async fn another_users_ids_are_not_found() {
    let g = generation_app("ai-gen-isolation", Arc::new(FakeProvider::default()));
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    let draft = drafts_json(&g, job).await[0]["id"].as_i64().unwrap();
    let other = member(&g.t.store, "Other", "other@example.com");
    let other_bearer = api_bearer(&g.t.store, other);
    let other_deck =
        g.t.services
            .create_deck(other, "Theirs", "", now_ms())
            .unwrap();

    for path in [
        format!("/api/v1/ai/sources/{source}/preflight"),
        format!("/api/v1/ai/jobs/{job}"),
        format!("/api/v1/ai/jobs/{job}/drafts"),
        format!("/api/v1/ai/jobs/{job}/turns"),
    ] {
        let r = send(&g.t.app, json_get(&path, Some(&other_bearer))).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
    }
    let r = send(
        &g.t.app,
        json_delete(&format!("/api/v1/ai/sources/{source}"), Some(&other_bearer)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    for (path, body) in [
        (
            "/api/v1/ai/jobs".to_string(),
            json!({ "source_ids": [source], "deck_id": other_deck.0 }),
        ),
        (
            "/api/v1/ai/preflight".to_string(),
            json!({ "source_ids": [source] }),
        ),
        (format!("/api/v1/ai/jobs/{job}/keep"), json!({})),
        (format!("/api/v1/ai/jobs/{job}/discard"), json!({})),
        (format!("/api/v1/ai/jobs/{job}/regenerate"), json!({})),
        (format!("/api/v1/ai/jobs/{job}/more"), json!({})),
        (
            format!("/api/v1/ai/jobs/{job}/edit"),
            json!({ "message": "shorter" }),
        ),
    ] {
        let r = send(&g.t.app, json_post(&path, Some(&other_bearer), &body)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}: {}", r.text());
    }
    assert!(
        turn_list(&turns_json(&g, job).await).is_empty(),
        "nothing was written to the thread"
    );
    let r = send(
        &g.t.app,
        json_patch(
            &format!("/api/v1/ai/drafts/{draft}"),
            Some(&other_bearer),
            &json!({ "status": "discarded" }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // A deck of the other user's cannot receive the drafts either.
    let r = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/keep"),
        json!({ "deck_id": other_deck.0 }),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(
        job_json(&g, job).await["counts"]["pending"],
        3,
        "nothing moved"
    );
}

#[tokio::test]
async fn every_route_says_not_available_when_no_model_is_configured() {
    let t = AppBuilder::new("ai-gen-off").build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let meta = send(&t.app, json_get("/api/v1/meta", None)).await.json();
    assert_eq!(meta["features"]["ai"], false);
    assert!(meta["ai"].is_null());
    let gets = [
        "/api/v1/ai/sources",
        "/api/v1/ai/sources/1/preflight",
        "/api/v1/ai/jobs/1",
        "/api/v1/ai/jobs/1/drafts",
        "/api/v1/ai/jobs/1/turns",
        "/api/v1/ai/quota",
    ];
    for path in gets {
        let r = send(&t.app, json_get(path, Some(&bearer))).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(r.json()["error"]["code"], "not_available", "{path}");
    }
    let r = send(&t.app, json_delete("/api/v1/ai/sources/1", Some(&bearer))).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.json()["error"]["code"], "not_available");
    let posts = [
        "/api/v1/ai/sources",
        "/api/v1/ai/preflight",
        "/api/v1/ai/jobs",
        "/api/v1/ai/jobs/1/keep",
        "/api/v1/ai/jobs/1/discard",
        "/api/v1/ai/jobs/1/regenerate",
        "/api/v1/ai/jobs/1/more",
        "/api/v1/ai/jobs/1/edit",
    ];
    // A body every route's extractor accepts, so the answer is the
    // handler's and not the parser's.
    let body = json!({ "text": "x", "source_ids": [1], "deck_id": 1, "message": "x" });
    for path in posts {
        let r = send(&t.app, json_post(path, Some(&bearer), &body)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(r.json()["error"]["code"], "not_available", "{path}");
    }
    let r = send(
        &t.app,
        json_patch(
            "/api/v1/ai/drafts/1",
            Some(&bearer),
            &json!({ "status": "discarded" }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // The runner has nothing to do and says so.
    assert_eq!(runner::run_once(&t.state).await, 0);
}

/// A plan that refuses everything with one code.
struct Denying;

impl AiPolicy for Denying {
    fn admit_generation(
        &self,
        _store: &Store,
        _user: UserId,
        request: AiRequest,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        let kinds: Vec<&str> = request.inputs.iter().map(|i| i.as_str()).collect();
        Ok(AiAdmission::Denied {
            code: "pro_required",
            message: format!("{} sources need Pro.", kinds.join("+")),
        })
    }

    fn usage(&self, _store: &Store, _user: UserId, _now_ms: i64) -> Result<AiUsage, StoreError> {
        Ok(AiUsage {
            allowance_used_pct: Some(100),
            resets_at_ms: Some(1_790_000_000_000),
            inputs: &[AiInput::Text],
        })
    }
}

#[tokio::test]
async fn the_plans_refusal_reaches_the_client_with_its_code() {
    let g = generation_app_with(
        "ai-gen-policy",
        Arc::new(FakeProvider::default()),
        ALL_AI_INPUTS.to_vec(),
        Some(Arc::new(Denying)),
    );
    let created = text_source(&g, PASSAGE).await;
    let preflight = &created["preflight"];
    assert_eq!(preflight["allowed"], false);
    assert_eq!(preflight["code"], "pro_required");
    assert_eq!(preflight["reason"], "text sources need Pro.");
    assert_eq!(preflight["quota"]["allowance_used_pct"], 100);
    assert_eq!(preflight["quota"]["resets_at"], "2026-09-21");
    assert_eq!(preflight["quota"]["inputs"], json!(["text"]));

    let r = post(
        &g,
        "/api/v1/ai/jobs",
        json!({ "source_ids": [created["id"]], "deck_id": g.deck.0 }),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.text());
    assert_eq!(r.json()["error"]["code"], "pro_required");
    assert_eq!(r.json()["error"]["message"], "text sources need Pro.");
    // The set's preflight names the member the plan refused.
    let set = post(
        &g,
        "/api/v1/ai/preflight",
        json!({ "source_ids": [created["id"]] }),
    )
    .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(set.json()["allowed"], false);
    assert_eq!(set.json()["code"], "pro_required");
    assert_eq!(set.json()["refused_source_id"], created["id"]);
    assert_eq!(runner::run_once(&g.t.state).await, 0, "nothing was queued");
    let quota = get_json(&g, "/api/v1/ai/quota").await.json();
    assert_eq!(quota["resets_at"], "2026-09-21");

    // A message on a review the plan will no longer run is refused the
    // same way, and writes nothing to the thread.
    let gate = Arc::new(Gate(std::sync::atomic::AtomicBool::new(false)));
    let g = generation_app_with(
        "ai-gen-policy-later",
        Arc::new(FakeProvider::default()),
        ALL_AI_INPUTS.to_vec(),
        Some(gate.clone()),
    );
    let source = text_source(&g, PASSAGE).await["id"].as_i64().unwrap();
    let job = queue_job(&g, source, "").await;
    run_to_end(&g, job).await;
    gate.0.store(true, std::sync::atomic::Ordering::SeqCst);
    let r = post(
        &g,
        &format!("/api/v1/ai/jobs/{job}/edit"),
        json!({ "message": "shorter" }),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.text());
    assert_eq!(r.json()["error"]["code"], "pro_required");
    assert!(turn_list(&turns_json(&g, job).await).is_empty());
    assert_eq!(runner::run_once(&g.t.state).await, 0, "nothing was queued");
}

/// A plan that admits everything until the test closes it, then refuses
/// the way `Denying` does.
struct Gate(std::sync::atomic::AtomicBool);

impl AiPolicy for Gate {
    fn admit_generation(
        &self,
        store: &Store,
        user: UserId,
        request: AiRequest,
        now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            return Denying.admit_generation(store, user, request, now_ms);
        }
        Ok(AiAdmission::Allowed)
    }

    fn usage(&self, store: &Store, user: UserId, now_ms: i64) -> Result<AiUsage, StoreError> {
        Denying.usage(store, user, now_ms)
    }
}

/// A policy that keeps its own account of every recorded call: (row id,
/// step, model, answered).
struct Accounting(parking_lot::Mutex<Vec<(i64, String, String, bool)>>);

impl AiPolicy for Accounting {
    fn admit_generation(
        &self,
        _store: &Store,
        _user: UserId,
        _request: AiRequest,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        Ok(AiAdmission::Allowed)
    }

    fn usage(&self, _store: &Store, _user: UserId, _now_ms: i64) -> Result<AiUsage, StoreError> {
        Ok(AiUsage {
            inputs: ALL_AI_INPUTS,
            ..AiUsage::default()
        })
    }

    fn call_recorded(&self, _store: &Store, call: &RecordedCall<'_>) -> Result<(), StoreError> {
        self.0.lock().push((
            call.id,
            call.step.as_str().to_string(),
            call.model.to_string(),
            call.ok,
        ));
        Ok(())
    }
}

#[tokio::test]
async fn the_policy_hears_every_call_the_runner_records() {
    let accounting = Arc::new(Accounting(parking_lot::Mutex::new(Vec::new())));
    let g = generation_app_with(
        "ai-gen-accounting",
        Arc::new(FakeProvider::default()),
        ALL_AI_INPUTS.to_vec(),
        Some(accounting.clone()),
    );
    let created = text_source(&g, PASSAGE).await;
    let job = queue_job(&g, created["id"].as_i64().unwrap(), "").await;
    let j = run_to_end(&g, job).await;
    assert_eq!(j["state"], "done", "{j}");

    // The reading and the writing, each with the row the ledger gave it.
    let ids: Vec<i64> =
        g.t.store
            .with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT id FROM ai_calls ORDER BY id")?;
                let rows = stmt.query_map([], |r| r.get(0))?;
                Ok(rows.collect::<Result<Vec<_>, _>>()?)
            })
            .unwrap();
    assert_eq!(ids.len(), 2);
    let heard = accounting.0.lock().clone();
    assert_eq!(
        heard,
        [
            (ids[0], "extract".to_string(), "fake".to_string(), true),
            (ids[1], "generate".to_string(), "fake".to_string(), true),
        ]
    );
}
