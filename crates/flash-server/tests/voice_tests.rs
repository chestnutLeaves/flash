//! Voice study's server half: one spoken turn. What the student said
//! goes to the model with the card and where the conversation is; an
//! answer comes back graded with the reveal, a question answered, a
//! named grade as its rating, and a request to hear the card again (or
//! to skip, undo, or nothing intelligible) as a bare intent that grades
//! nothing and reveals nothing. Turns are refused where they should be:
//! no model on the server, another user's session, an ended session, a
//! transcript past its bound, a user past their budget. The model sees
//! the source a card was generated from and nothing for a card that was
//! not.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use flash_core::{DeckId, UserId};
use flash_server::ai::fake::{FakeProvider, FakeReply, Step};
use flash_server::ai::provider::{DialogState, SpokenIntent, SpokenTurn};
use flash_server::bounds;
use flash_server::policy::{
    AiAdmission, AiInput, AiPolicy, AiRequest, AiUsage, RecordedCall, ALL_AI_INPUTS,
};
use flash_server::service::{now_ms, NoteInput};
use flash_store::notes::{generate_cards, NoteType};
use flash_store::richtext::sanitize_card_html;
use flash_store::{
    AiExtractionNew, AiJobNew, AiJobOp, AiSourceKind, AiSourceNew, NoteSave, Store, StoreError,
};
use serde_json::json;

struct Voice {
    t: TestApp,
    fake: Arc<FakeProvider>,
}

fn voice_app(tag: &str) -> Voice {
    voice_app_scripted(tag, Vec::new())
}

/// A server whose model answers from `replies`, in order; unscripted,
/// an answer on the front is right and the window hears a question.
fn voice_app_scripted(tag: &str, replies: Vec<FakeReply>) -> Voice {
    let fake = Arc::new(FakeProvider::scripted(replies));
    let t = AppBuilder::new(tag)
        .ai(fake.clone(), vec![AiInput::Text])
        .build();
    Voice { t, fake }
}

/// A member with a deck holding one plain card.
fn seeded(t: &TestApp, name: &str) -> (UserId, String, DeckId) {
    let user = member(&t.store, name, &format!("{name}@example.com"));
    let deck = t.services.create_deck(user, "Heart", "", now_ms()).unwrap();
    t.services
        .create_cards_in_deck(
            user,
            deck,
            &[(
                "How many chambers does the heart have?".into(),
                "Four".into(),
                vec![],
            )],
            now_ms(),
        )
        .unwrap();
    (user, api_bearer(&t.store, user), deck)
}

/// A note kept from a generation job: source → reading → job → note,
/// the chain `ai_extraction_for_note` walks.
fn generated_note(t: &TestApp, user: UserId, deck: DeckId) -> i64 {
    let now = now_ms();
    let source = t
        .store
        .create_ai_source(
            user,
            &AiSourceNew {
                kind: AiSourceKind::Text,
                sha256: "0000000000000000000000000000000000000000000000000000000000000001",
                mime: "text/plain",
                size: 40,
                filename: "notes.txt",
                url: None,
                text: Some("The heart has four chambers."),
                pages: None,
                duration_s: None,
                est_words: Some(5),
                image_count: None,
                origin_mime: None,
                now_ms: now,
            },
        )
        .unwrap();
    let extraction = t
        .store
        .insert_ai_extraction(
            user,
            &AiExtractionNew {
                source_id: source,
                sha256: "0000000000000000000000000000000000000000000000000000000000000001",
                model: "fake",
                prompt_version: 1,
                text: "The heart has four chambers: two atria and two ventricles.",
                figures: "[]",
                est_words: 10,
                now_ms: now,
            },
        )
        .unwrap();
    let job = t
        .store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[source],
                deck_id: Some(deck),
                extraction_id: Some(extraction),
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: now,
            },
        )
        .unwrap();
    let front = sanitize_card_html("Which valve leads from the right atrium?");
    let back = sanitize_card_html("The tricuspid valve");
    let generated = generate_cards(NoteType::Basic, &front, &back).unwrap();
    t.store
        .create_note(
            user,
            deck,
            &NoteSave {
                note_type: NoteType::Basic,
                front_html: &front,
                back_html: &back,
                generated: &generated,
                tags: &[],
                cap: None,
                ai_job_id: Some(job),
                now_ms: now,
            },
        )
        .unwrap();
    extraction
}

async fn start(t: &TestApp, bearer: &str, deck: DeckId) -> (i64, i64) {
    let reply = send(
        &t.app,
        json_post(
            "/api/v1/study/sessions",
            Some(bearer),
            &json!({ "scope": { "type": "deck", "id": deck.0 } }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let json = reply.json();
    (
        json["session_id"].as_i64().unwrap(),
        json["card"]["card_id"].as_i64().unwrap(),
    )
}

/// One spoken turn as a transcript, in `state` (`front` or `window`).
async fn turn(
    t: &TestApp,
    bearer: &str,
    sid: i64,
    card: i64,
    state: &str,
    transcript: &str,
) -> Reply {
    turn_with(
        t,
        bearer,
        sid,
        &json!({ "card_id": card, "state": state, "transcript": transcript }),
    )
    .await
}

/// A spoken turn as any JSON body, for the shapes `turn` does not send.
async fn turn_with(t: &TestApp, bearer: &str, sid: i64, body: &serde_json::Value) -> Reply {
    send(
        &t.app,
        json_post(
            &format!("/api/v1/study/sessions/{sid}/turn"),
            Some(bearer),
            body,
        ),
    )
    .await
}

/// `n` bytes of a recording, as a request carries them.
fn clip_b64(n: usize) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(vec![0x11u8; n])
}

/// What the ledger says one user's latest call read: (input_kind, media_seconds).
fn last_call_input(t: &TestApp, user: UserId) -> (String, i64) {
    t.store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT input_kind, media_seconds FROM ai_calls WHERE user_id = ?1 \
                 ORDER BY id DESC LIMIT 1",
                [user.raw()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap()
}

/// The ledger rows for a user: (step, model, ok).
fn ai_calls(t: &TestApp, user: UserId) -> Vec<(String, String, bool)> {
    t.store
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT step, model, ok FROM ai_calls WHERE user_id = ?1 ORDER BY id")?;
            let rows = stmt.query_map([user.raw()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? == 1))
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

fn reviewed(t: &TestApp, user: UserId, sid: i64) -> u32 {
    t.store
        .session_stats(user, flash_core::SessionId(sid))
        .unwrap()
        .reviewed
}

#[tokio::test]
async fn a_spoken_answer_is_graded_with_the_reveal_and_no_review_written() {
    let v = voice_app("voice-grade");
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn(&v.t, &bearer, sid, card, "front", "  four chambers ").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let json = reply.json();
    assert_eq!(json["intent"], "answer");
    assert_eq!(json["rating"], 3);
    assert_eq!(json["feedback"], "Correct: four chambers.");
    assert!(json["spoken_feedback"]
        .as_str()
        .unwrap()
        .starts_with("Good."));
    assert_eq!(json["reveal"]["back"], "Four");
    assert_eq!(json["reveal"]["spoken_back"], "Four");
    assert_eq!(
        json["reveal"]["spoken_front"],
        "How many chambers does the heart have?"
    );
    assert!(json["reveal"]["typed_diff"].is_null());

    // The model heard the trimmed transcript, in the front state, and
    // saw no source.
    let calls = v.fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].step, Step::Turn);
    assert_eq!(calls[0].state, Some(DialogState::Front));
    assert_eq!(calls[0].instruction.as_deref(), Some("four chambers"));
    assert!(!calls[0].extraction_present);

    // One ledger row with the provider's counts, and no review: the
    // client posts that itself.
    let ledger = ai_calls(&v.t, user);
    assert_eq!(ledger, [("grade".to_string(), "fake".to_string(), true)]);
    let counted: (i64, i64) =
        v.t.store
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT input_tokens, output_tokens FROM ai_calls",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })
            .unwrap();
    assert_eq!(counted, (1_200, 300));
    assert_eq!(reviewed(&v.t, user, sid), 0);
}

/// The point of the turn: a student asking to hear the card again, in
/// their own words, is not graded. Nothing is revealed, nothing is
/// reviewed, and the reply carries the intent alone. The model still
/// answered a call, so the ledger has its row.
#[tokio::test]
async fn a_request_to_hear_the_card_again_is_not_graded() {
    let v = voice_app_scripted(
        "voice-repeat",
        vec![
            FakeProvider::bare(SpokenIntent::Repeat),
            FakeProvider::bare(SpokenIntent::Repeat),
        ],
    );
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn(
        &v.t,
        &bearer,
        sid,
        card,
        "front",
        "sorry, I didn't catch that, could you say the question one more time?",
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json(), json!({ "intent": "repeat" }));

    // In the window too: the back is read again, the grade stays pending.
    let again = turn(
        &v.t,
        &bearer,
        sid,
        card,
        "window",
        "what was the answer again",
    )
    .await;
    assert_eq!(again.json(), json!({ "intent": "repeat" }));
    assert_eq!(v.fake.calls()[1].state, Some(DialogState::Window));

    assert_eq!(reviewed(&v.t, user, sid), 0);
    // The ledger keeps its two voice steps: a turn that was not a
    // question is recorded as a grade call.
    assert_eq!(
        ai_calls(&v.t, user),
        [
            ("grade".to_string(), "fake".to_string(), true),
            ("grade".to_string(), "fake".to_string(), true),
        ]
    );
}

/// The other bare intents, and a grade the student named: each comes
/// back as itself, with the rating where there is one and nothing else.
#[tokio::test]
async fn skip_undo_unclear_and_a_named_grade_come_back_as_themselves() {
    let v = voice_app_scripted(
        "voice-intents",
        vec![
            FakeProvider::bare(SpokenIntent::Unclear),
            FakeProvider::bare(SpokenIntent::Skip),
            FakeReply::Turn(Ok(SpokenTurn {
                rating: Some(2),
                ..SpokenTurn::bare(SpokenIntent::Grade)
            })),
            FakeProvider::bare(SpokenIntent::Undo),
        ],
    );
    let (_user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let mumble = turn(&v.t, &bearer, sid, card, "front", "mm hm").await;
    assert_eq!(mumble.json(), json!({ "intent": "unclear" }));
    let pass = turn(&v.t, &bearer, sid, card, "front", "no clue, next one").await;
    assert_eq!(pass.json(), json!({ "intent": "skip" }));
    let harder = turn(&v.t, &bearer, sid, card, "window", "hmm, that one was hard").await;
    assert_eq!(harder.json(), json!({ "intent": "grade", "rating": 2 }));
    let back = turn(
        &v.t,
        &bearer,
        sid,
        card,
        "window",
        "no wait, that's not right",
    )
    .await;
    assert_eq!(back.json(), json!({ "intent": "undo" }));
}

/// The service holds any provider to the state: an undo before a verdict
/// and an answer after one are unclear, whatever the model said.
#[tokio::test]
async fn an_intent_the_state_cannot_mean_is_unclear() {
    let v = voice_app_scripted(
        "voice-state",
        vec![
            FakeProvider::bare(SpokenIntent::Undo),
            FakeReply::Turn(Ok(FakeProvider::canned_answer())),
        ],
    );
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let undo = turn(&v.t, &bearer, sid, card, "front", "undo that").await;
    assert_eq!(undo.json(), json!({ "intent": "unclear" }));
    let answer = turn(&v.t, &bearer, sid, card, "window", "four").await;
    assert_eq!(
        answer.json(),
        json!({ "intent": "unclear" }),
        "no reveal after a verdict"
    );
    assert_eq!(reviewed(&v.t, user, sid), 0);
}

#[tokio::test]
async fn a_generated_card_grounds_the_model_in_its_source() {
    let v = voice_app("voice-grounded");
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let extraction = generated_note(&v.t, user, deck);
    let before =
        v.t.store
            .get_ai_extraction(user, extraction)
            .unwrap()
            .unwrap();
    let (sid, _first) = start(&v.t, &bearer, deck).await;
    let cards = v.t.services.list_cards(user, Some(deck), None, 10).unwrap();
    let plain = cards.iter().find(|c| c.note_id.is_none()).unwrap().id.0;
    let generated = cards.iter().find(|c| c.note_id.is_some()).unwrap().id;

    assert_eq!(
        turn(&v.t, &bearer, sid, plain, "front", "four")
            .await
            .status,
        StatusCode::OK
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    assert_eq!(
        turn(&v.t, &bearer, sid, generated.0, "front", "tricuspid")
            .await
            .status,
        StatusCode::OK
    );

    let calls = v.fake.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0].extraction_present, "a plain card has no source");
    assert!(
        calls[1].extraction_present,
        "a kept draft carries its reading"
    );

    // Reading the source for a study turn keeps housekeeping off it.
    let after =
        v.t.store
            .get_ai_extraction(user, extraction)
            .unwrap()
            .unwrap();
    assert!(after.last_used_at > before.last_used_at);
}

#[tokio::test]
async fn a_question_in_the_window_gets_the_canned_answer() {
    let v = voice_app("voice-question");
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn(&v.t, &bearer, sid, card, "window", "what do the atria do?").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let json = reply.json();
    assert_eq!(json["intent"], "question");
    assert_eq!(
        json["answer"],
        "The atria receive blood; the ventricles pump it out."
    );
    assert_eq!(
        json["spoken_answer"],
        "The atria receive blood, and the ventricles pump it out."
    );
    assert!(json.get("reveal").is_none(), "a question reveals nothing");
    let calls = v.fake.calls();
    assert_eq!(calls[0].step, Step::Turn);
    assert_eq!(calls[0].state, Some(DialogState::Window));
    assert_eq!(
        calls[0].instruction.as_deref(),
        Some("what do the atria do?")
    );
    assert_eq!(
        ai_calls(&v.t, user),
        [("clarify".to_string(), "fake".to_string(), true)]
    );
}

#[tokio::test]
async fn spoken_turns_are_refused_for_foreign_ended_empty_and_misplaced() {
    let v = voice_app("voice-refused");
    let (_owner, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;
    let other = member(&v.t.store, "Other", "other@example.com");
    let other_bearer = api_bearer(&v.t.store, other);

    // Another user's session is not found, on either side of the verdict.
    let foreign = turn(&v.t, &other_bearer, sid, card, "front", "four").await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(foreign.json()["error"]["code"], "not_found");
    let stranger = turn(&v.t, &other_bearer, sid, card, "window", "why").await;
    assert_eq!(stranger.status, StatusCode::NOT_FOUND);

    // Silence is not a turn.
    let empty = turn(&v.t, &bearer, sid, card, "front", "   ").await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    assert!(
        empty.text().contains("nothing was said"),
        "{}",
        empty.text()
    );

    // The state has to be one the dialog has.
    let nowhere = turn(&v.t, &bearer, sid, card, "limbo", "four").await;
    assert_eq!(nowhere.status, StatusCode::BAD_REQUEST);
    assert!(
        nowhere.text().contains("state must be front or window"),
        "{}",
        nowhere.text()
    );

    // An ended session is a stale client.
    let end = send(
        &v.t.app,
        json_post(
            &format!("/api/v1/study/sessions/{sid}/end"),
            Some(&bearer),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(end.status, StatusCode::OK);
    let ended = turn(&v.t, &bearer, sid, card, "front", "four").await;
    assert_eq!(ended.status, StatusCode::BAD_REQUEST);
    assert!(ended.text().contains("has ended"), "{}", ended.text());

    // None of those reached the model.
    assert!(v.fake.calls().is_empty());
}

#[tokio::test]
async fn without_a_model_the_route_says_so() {
    let t = AppBuilder::new("voice-off").build();
    let (_user, bearer, deck) = seeded(&t, "flashtester");
    let (sid, card) = start(&t, &bearer, deck).await;
    for state in ["front", "window"] {
        let reply = turn(&t, &bearer, sid, card, state, "four").await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{state}");
        let json = reply.json();
        assert_eq!(json["error"]["code"], "not_available");
        assert_eq!(
            json["error"]["message"],
            "card generation is not set up on this server"
        );
    }
}

/// The recording path: the model is handed the bytes and their type,
/// nothing is transcribed on the way, and the ledger meters the call as
/// audio with the seconds the provider reported.
#[tokio::test]
async fn a_recording_is_heard_as_audio_and_metered_in_seconds() {
    let v = voice_app("voice-audio");
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn_with(
        &v.t,
        &bearer,
        sid,
        &json!({
            "card_id": card,
            "state": "front",
            "audio": { "mime": "audio/webm;codecs=opus", "data_b64": clip_b64(96_000) }
        }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let json = reply.json();
    assert_eq!(json["intent"], "answer");
    assert_eq!(json["rating"], 3);
    assert_eq!(json["reveal"]["back"], "Four");

    let calls = v.fake.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].step, Step::Turn);
    assert_eq!(calls[0].instruction, None, "nothing was transcribed");
    // The type reaches the model bare: parameters are the browser's business.
    assert_eq!(calls[0].audio, Some(("audio/webm".to_string(), 96_000)));
    assert_eq!(last_call_input(&v.t, user), ("audio".to_string(), 3));
}

/// Every malformed recording is refused at the edge with a sentence,
/// before the model is asked: it costs nothing and the fake sees nothing.
#[tokio::test]
async fn a_malformed_recording_is_refused_before_the_model() {
    let v = voice_app("voice-audio-bad");
    let (_user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;
    let cases: Vec<(serde_json::Value, &str)> = vec![
        (
            json!({ "card_id": card, "state": "front", "transcript": "four",
                    "audio": { "mime": "audio/wav", "data_b64": clip_b64(10) } }),
            "not both",
        ),
        (
            json!({ "card_id": card, "state": "front" }),
            "say something or send a recording",
        ),
        (
            json!({ "card_id": card, "state": "front",
                    "audio": { "mime": "video/mp4", "data_b64": clip_b64(10) } }),
            "not one the grader takes",
        ),
        (
            json!({ "card_id": card, "state": "front",
                    "audio": { "mime": "audio/wav", "data_b64": "not*base64" } }),
            "not valid base64",
        ),
        (
            json!({ "card_id": card, "state": "front",
                    "audio": { "mime": "audio/wav", "data_b64": "" } }),
            "recording is empty",
        ),
        (
            json!({ "card_id": card, "state": "front", "audio": {
                "mime": "audio/wav", "data_b64": clip_b64(bounds::VOICE_AUDIO_BYTES + 1) } }),
            "larger than",
        ),
    ];
    for (body, expected) in cases {
        let reply = turn_with(&v.t, &bearer, sid, &body).await;
        assert_eq!(
            reply.status,
            StatusCode::BAD_REQUEST,
            "{expected}: {}",
            reply.text()
        );
        let json = reply.json();
        assert_eq!(json["error"]["code"], "invalid", "{expected}");
        assert!(
            json["error"]["message"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{expected}: {}",
            json["error"]["message"]
        );
    }
    // Past the whole request's budget the body is never read.
    let huge = "A".repeat(bounds::VOICE_BODY_BYTES + 1);
    let reply = turn_with(
        &v.t,
        &bearer,
        sid,
        &json!({ "card_id": card, "state": "front",
                 "audio": { "mime": "audio/wav", "data_b64": huge } }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(v.fake.calls().is_empty());
}

#[tokio::test]
async fn a_transcript_past_the_bound_is_refused_at_the_edge() {
    let v = voice_app("voice-bound");
    let (_user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;
    let long = "a".repeat(bounds::TRANSCRIPT + 1);
    let reply = turn(&v.t, &bearer, sid, card, "front", &long).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let json = reply.json();
    assert_eq!(json["error"]["code"], "invalid");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("transcript is longer than 2000 bytes"),
        "{json}"
    );
    assert!(v.fake.calls().is_empty());
}

#[tokio::test]
async fn each_user_has_a_budget_of_spoken_turns() {
    let fake = Arc::new(FakeProvider::default());
    let t = AppBuilder::new("voice-budget")
        .ai(fake.clone(), vec![AiInput::Text])
        .tight_rate_limits()
        .build();
    let (_user, bearer, deck) = seeded(&t, "flashtester");
    let (sid, card) = start(&t, &bearer, deck).await;
    for _ in 0..TIGHT_BURST {
        let reply = turn(&t, &bearer, sid, card, "front", "four").await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    }
    let refused = turn(&t, &bearer, sid, card, "front", "four").await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    // The user's own bucket answered, in the API's envelope, not the
    // per-address band in front of every route.
    assert_eq!(refused.json()["error"]["code"], "rate_limited");
    assert_eq!(fake.calls().len(), TIGHT_BURST as usize);
}

/// The same turns through the browser's route: a form in, the same
/// JSON out, behind the session cookie and the same-origin guard.
#[tokio::test]
async fn the_web_twin_takes_a_form_and_answers_the_same_json() {
    let v = voice_app_scripted(
        "voice-web",
        vec![
            FakeReply::Turn(Ok(FakeProvider::canned_answer())),
            FakeReply::Turn(Ok(FakeProvider::canned_question())),
            FakeProvider::bare(SpokenIntent::Repeat),
            FakeReply::Turn(Ok(FakeProvider::canned_answer())),
        ],
    );
    let (user, cookie) = signed_in(&v.t.store, "FlashTester", "flashtester@example.com");
    let deck =
        v.t.services
            .create_deck(user, "Heart", "", now_ms())
            .unwrap();
    v.t.services
        .create_note(
            user,
            deck,
            &NoteInput {
                note_type: NoteType::Basic,
                front_html: "How many chambers?".into(),
                back_html: "Four".into(),
                tags: vec![],
            },
            now_ms(),
        )
        .unwrap();
    let session =
        v.t.services
            .start_session(user, flash_core::queue::StudyScope::Deck(deck), now_ms())
            .unwrap();
    let card = session.first_card.unwrap().card_id.0;
    let sid = session.session_id.0;
    let post = |body: String| form_post("/study/turn", Some(&cookie), &body);

    let graded = send(
        &v.t.app,
        post(format!(
            "session={sid}&card={card}&state=front&transcript=four"
        )),
    )
    .await;
    assert_eq!(graded.status, StatusCode::OK, "{}", graded.text());
    let json = graded.json();
    assert_eq!(json["intent"], "answer");
    assert_eq!(json["rating"], 3);
    assert_eq!(json["reveal"]["spoken_back"], "Four");

    let asked = send(
        &v.t.app,
        post(format!(
            "session={sid}&card={card}&state=window&transcript=why"
        )),
    )
    .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text());
    assert_eq!(asked.json()["intent"], "question");
    assert!(asked.json()["spoken_answer"]
        .as_str()
        .unwrap()
        .contains("ventricles"));

    let again = send(
        &v.t.app,
        post(format!(
            "session={sid}&card={card}&state=front&transcript=could+you+say+that+again"
        )),
    )
    .await;
    assert_eq!(again.json(), json!({ "intent": "repeat" }));

    // A recording travels as two form fields; the base64 is urlencoded
    // like any other value.
    let clip = clip_b64(32_000)
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D");
    let heard = send(
        &v.t.app,
        post(format!(
            "session={sid}&card={card}&state=front&audio_mime=audio%2Fwav&audio_b64={clip}"
        )),
    )
    .await;
    assert_eq!(heard.status, StatusCode::OK, "{}", heard.text());
    assert_eq!(heard.json()["rating"], 3);
    let last = v.fake.calls().last().cloned().unwrap();
    assert_eq!(last.audio, Some(("audio/wav".to_string(), 32_000)));

    // Neither field, or a state the dialog has not, is a 400 with the
    // sentence, not a 500.
    let empty = send(
        &v.t.app,
        post(format!("session={sid}&card={card}&state=front")),
    )
    .await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    assert!(empty.text().contains("say something or send a recording"));
    let nowhere = send(
        &v.t.app,
        post(format!(
            "session={sid}&card={card}&state=limbo&transcript=four"
        )),
    )
    .await;
    assert_eq!(nowhere.status, StatusCode::BAD_REQUEST);
    assert!(nowhere.text().contains("state must be front or window"));
}

/// The number after `"key": ` in the reveal button's hx-vals.
fn hx_value(html: &str, key: &str) -> String {
    let marker = format!("\"{key}\": ");
    let at = html
        .find(&marker)
        .unwrap_or_else(|| panic!("{key} in {html}"))
        + marker.len();
    html[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect()
}

/// While the front shows, the card carries the answer's words for the
/// recognizer, spelled out where the answer is a symbol: on the page as
/// a data attribute and in the API session. It is there before the
/// reveal on purpose (a recognizer cannot lean toward words it has not
/// seen); the revealed card carries none.
#[tokio::test]
async fn the_front_carries_the_answers_words_for_the_recognizer() {
    let v = voice_app("voice-hints");
    let (user, cookie) = signed_in(&v.t.store, "FlashTester", "flashtester@example.com");
    let deck =
        v.t.services
            .create_deck(user, "Chem", "", now_ms())
            .unwrap();
    v.t.services
        .create_cards_in_deck(
            user,
            deck,
            &[("Chemical symbol for gold?".into(), "Au.".into(), vec![])],
            now_ms(),
        )
        .unwrap();

    let page = send(&v.t.app, get("/study", Some(&cookie))).await.text();
    // The template escapes the JSON's quotes; the browser hands them back.
    assert!(
        page.contains(r#"data-answer-hints="[&#34;Au&#34;,&#34;A U&#34;]""#),
        "{page}"
    );
    assert!(!page.contains("data-spoken-back"), "{page}");
    let session = hx_value(&page, "session");
    let card = hx_value(&page, "card");
    let revealed = send(
        &v.t.app,
        form_post(
            "/study/reveal",
            Some(&cookie),
            &format!("session={session}&card={card}&remaining=1&total=1"),
        ),
    )
    .await;
    assert_eq!(revealed.status, StatusCode::OK);
    let revealed = revealed.text();
    assert!(revealed.contains(r#"data-spoken-back="Au.""#), "{revealed}");
    assert!(!revealed.contains("data-answer-hints"), "{revealed}");

    let bearer = api_bearer(&v.t.store, user);
    let reply = send(
        &v.t.app,
        json_post(
            "/api/v1/study/sessions",
            Some(&bearer),
            &json!({ "scope": { "type": "deck", "id": deck.0 } }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json()["card"]["answer_hints"], json!(["Au", "A U"]));
}

/// The study page tells its script what to send and how the user wants
/// grading, and marks each card so the script can tell a new card from
/// the same one revealed. A server without a model says none of it.
#[tokio::test]
async fn the_study_page_carries_the_voice_settings_when_there_is_a_model() {
    let v = voice_app("voice-page");
    let (user, cookie) = signed_in(&v.t.store, "FlashTester", "flashtester@example.com");
    let deck =
        v.t.services
            .create_deck(user, "Heart", "", now_ms())
            .unwrap();
    v.t.services
        .create_cards_in_deck(user, deck, &[("Q".into(), "A".into(), vec![])], now_ms())
        .unwrap();
    let page = send(&v.t.app, get("/study", Some(&cookie))).await;
    assert_eq!(page.status, StatusCode::OK);
    let html = page.text();
    assert!(html.contains(r#"data-voice-input="transcript""#), "{html}");
    assert!(html.contains(r#"data-grading-mode="silent""#), "{html}");
    assert!(
        html.contains(r#"class="fl-card study-card" data-card-id=""#),
        "{html}"
    );
    assert!(html.contains(r#"id="voice-toggle""#) && html.contains("/static/voice.js"));
    // The dock, the glow, the speech and glow scripts, and the account's
    // voice settings as data attributes.
    for needle in [
        r#"id="voice-dock""#,
        r#"id="voice-glow""#,
        r#"id="voice-mic""#,
        "/static/speech.js",
        "/static/glow.js",
        r#"data-voice-apply="auto""#,
        r#"data-speech-rate="1.0""#,
        r#"data-voice-cues="on""#,
    ] {
        assert!(html.contains(needle), "{needle} missing: {html}");
    }
    assert!(!html.contains("data-voice-audio"), "no sidecar: {html}");
    assert!(!html.contains("data-voice-mode"), "no passive mode: {html}");
    // The script the page names is served as a script, posts its turns
    // to the twin above, and acts on every intent the twin can answer.
    let script = send(&v.t.app, get("/static/voice.js", None)).await;
    assert_eq!(script.status, StatusCode::OK);
    assert_eq!(
        script.header("content-type"),
        Some("application/javascript")
    );
    let js = script.text();
    assert!(js.contains("/study/turn"), "{js}");
    for intent in SpokenIntent::ALL {
        let needle = format!("\"{}\"", intent.as_str());
        assert!(js.contains(&needle), "voice.js does not handle {needle}");
    }
    // The parser is the mobile app's, ported by hand: the pick among the
    // recognizer's alternatives by the card's answer words is here too.
    assert!(
        js.contains("pickTranscript") && js.contains("maxAlternatives"),
        "{js}"
    );
    assert!(js.contains("answerHints"), "{js}");

    v.t.services.set_grading_mode(user, "self").unwrap();
    let again = send(&v.t.app, get("/study", Some(&cookie))).await.text();
    assert!(again.contains(r#"data-grading-mode="self""#), "{again}");

    let off = AppBuilder::new("voice-page-off").build();
    let (user, cookie) = signed_in(&off.store, "FlashTester", "flashtester@example.com");
    off.services
        .create_deck(user, "Heart", "", now_ms())
        .unwrap();
    let html = send(&off.app, get("/study", Some(&cookie))).await.text();
    assert!(!html.contains("data-voice-input"), "{html}");
    assert!(!html.contains("voice-toggle") && !html.contains("/static/voice.js"));
}

// ---- the policy's say ----

/// A policy that keeps its own account of every recorded call: (row id,
/// model, answered).
struct Accounting(parking_lot::Mutex<Vec<(i64, String, bool)>>);

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
        self.0
            .lock()
            .push((call.id, call.model.to_string(), call.ok));
        Ok(())
    }
}

/// A policy whose account-keeping fails every time.
struct Fumbling;

impl AiPolicy for Fumbling {
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

    fn call_recorded(&self, _store: &Store, _call: &RecordedCall<'_>) -> Result<(), StoreError> {
        Err(StoreError::Invalid("no account today".into()))
    }
}

const BROKE: &str = "You've used this month's AI allowance. Voice grading resumes Oct 1.";

/// A policy that refuses every spoken turn.
struct Broke;

impl AiPolicy for Broke {
    fn admit_generation(
        &self,
        _store: &Store,
        _user: UserId,
        _request: AiRequest,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        Ok(AiAdmission::Allowed)
    }

    fn admit_voice(
        &self,
        _store: &Store,
        _user: UserId,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        Ok(AiAdmission::Denied {
            code: "monthly_limit",
            message: BROKE.to_string(),
        })
    }

    fn usage(&self, _store: &Store, _user: UserId, _now_ms: i64) -> Result<AiUsage, StoreError> {
        Ok(AiUsage {
            inputs: ALL_AI_INPUTS,
            ..AiUsage::default()
        })
    }
}

/// A voice server under `policy`.
fn voice_app_with(tag: &str, policy: Arc<dyn AiPolicy>) -> Voice {
    let fake = Arc::new(FakeProvider::default());
    let t = AppBuilder::new(tag)
        .ai(fake.clone(), vec![AiInput::Text])
        .ai_policy(policy)
        .build();
    Voice { t, fake }
}

/// The ledger's row ids for a user, oldest first.
fn ledger_ids(t: &TestApp, user: UserId) -> Vec<i64> {
    t.store
        .with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id FROM ai_calls WHERE user_id = ?1 ORDER BY id")?;
            let rows = stmt.query_map([user.raw()], |r| r.get(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

#[tokio::test]
async fn the_policy_hears_every_recorded_turn_with_its_row() {
    let accounting = Arc::new(Accounting(parking_lot::Mutex::new(Vec::new())));
    let v = voice_app_with("voice-accounting", accounting.clone());
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn(&v.t, &bearer, sid, card, "front", "four").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    let reply = turn(&v.t, &bearer, sid, card, "window", "why four").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());

    let ids = ledger_ids(&v.t, user);
    assert_eq!(ids.len(), 2);
    let heard = accounting.0.lock().clone();
    assert_eq!(
        heard,
        [
            (ids[0], "fake".to_string(), true),
            (ids[1], "fake".to_string(), true)
        ]
    );
}

#[tokio::test]
async fn a_policy_that_cannot_keep_its_account_loses_nothing_of_the_turn() {
    let v = voice_app_with("voice-fumbling", Arc::new(Fumbling));
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    let reply = turn(&v.t, &bearer, sid, card, "front", "four").await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json()["intent"], "answer");
    assert_eq!(
        ai_calls(&v.t, user),
        [("grade".to_string(), "fake".to_string(), true)]
    );
}

#[tokio::test]
async fn a_policy_may_refuse_a_spoken_turn_on_both_surfaces() {
    let v = voice_app_with("voice-broke", Arc::new(Broke));
    let (user, bearer, deck) = seeded(&v.t, "flashtester");
    let (sid, card) = start(&v.t, &bearer, deck).await;

    // The app: the code and the sentence, and the model never called.
    let reply = turn(&v.t, &bearer, sid, card, "front", "four").await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.text());
    assert_eq!(reply.json()["error"]["code"], "monthly_limit");
    assert_eq!(reply.json()["error"]["message"], BROKE);
    assert!(v.fake.calls().is_empty());
    assert!(ai_calls(&v.t, user).is_empty());

    // The web: the same sentence, for the notice the voice mode shows.
    let cookie = web_session(&v.t.store, user);
    let refused = send(
        &v.t.app,
        form_post(
            "/study/turn",
            Some(&cookie),
            &format!("session={sid}&card={card}&state=front&transcript=four"),
        ),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert!(refused.text().contains(BROKE), "{}", refused.text());
    assert!(v.fake.calls().is_empty());
}
