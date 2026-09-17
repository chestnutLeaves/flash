//! Natural voice: clips are made ahead of the study queue by the worker
//! and served by card and side; identical words share one recording;
//! a voice change starts a cold cache and queues the phrase set; a
//! sidecar that stops answering never blocks a request; settings round
//! trip over the API and the web; housekeeping sweeps what nobody's
//! card names.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use flash_core::{DeckId, UserId};
use flash_server::ai::fake::{FakeProvider, FakeReply};
use flash_server::policy::AiInput;
use flash_server::service::now_ms;
use flash_server::tts::fake::{FakeTts, FAKE_MODEL};
use flash_server::tts::worker::{run_once, warm_pass, WARM_BATCH};
use flash_server::tts::{clip_key, phrase_text, TtsError, PHRASES};
use flash_store::{VoiceClipKind, VoiceClipNew};
use serde_json::json;

/// Whether a sidecar call was a voice's demo line, in that voice's words.
fn is_sample(voice: &str, text: &str) -> bool {
    phrase_text("sample", voice).as_deref() == Some(text)
}

struct Voiced {
    t: TestApp,
    fake: Arc<FakeTts>,
}

fn voiced(tag: &str) -> Voiced {
    let fake = Arc::new(FakeTts::default());
    let t = AppBuilder::new(tag).tts(fake.clone()).build();
    Voiced { t, fake }
}

const CARDS: &[(&str, &str)] = &[
    ("How many chambers does the heart have?", "Four."),
    ("What carries blood away from the heart?", "Arteries."),
    ("What is the largest artery?", "The aorta."),
    (
        "Which valve sits between the left atrium and ventricle?",
        "The mitral valve.",
    ),
    (
        "What is the resting heart rate range?",
        "Sixty to one hundred.",
    ),
    ("Which chamber pumps to the lungs?", "The right ventricle."),
];

fn seeded(t: &TestApp, name: &str) -> (UserId, String, DeckId) {
    let user = member(&t.store, name, &format!("{name}@example.com"));
    let deck = t.services.create_deck(user, "Heart", "", now_ms()).unwrap();
    t.services
        .create_cards_in_deck(
            user,
            deck,
            &CARDS
                .iter()
                .map(|(f, b)| (f.to_string(), b.to_string(), vec![]))
                .collect::<Vec<_>>(),
            now_ms(),
        )
        .unwrap();
    (user, api_bearer(&t.store, user), deck)
}

async fn start(t: &TestApp, bearer: &str) -> serde_json::Value {
    let reply = send(
        &t.app,
        json_post(
            "/api/v1/study/sessions",
            Some(bearer),
            &json!({ "scope": { "type": "all" } }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    reply.json()
}

fn audio_path(card: i64, side: &str) -> String {
    format!("/api/v1/study/cards/{card}/audio?side={side}")
}

#[tokio::test]
async fn without_a_sidecar_the_voice_routes_say_so() {
    let t = AppBuilder::new("tts-off").build();
    let (_, bearer, _) = seeded(&t, "FlashTester");
    let meta = send(&t.app, json_get("/api/v1/meta", None)).await.json();
    assert_eq!(meta["features"]["voice"], json!(false));
    assert!(meta["voice"].is_null());
    for path in [
        audio_path(1, "front"),
        "/api/v1/voice/phrases".to_string(),
        "/api/v1/voice/phrases/af_heart/correct/audio".to_string(),
        "/api/v1/voice/voices".to_string(),
    ] {
        let reply = send(&t.app, json_get(&path, Some(&bearer))).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.json()["error"]["code"], "not_available", "{path}");
    }
    let session = start(&t, &bearer).await;
    assert!(session["card"]["audio_front"].is_null());
    // Settings still save: the voice orders the app's picker either way.
    let saved = send(
        &t.app,
        json_put(
            "/api/v1/settings/voice",
            Some(&bearer),
            &json!({ "voice_id": "bm_fable" }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn clips_are_made_ahead_and_served_by_card_and_side() {
    let v = voiced("tts-ahead");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    let meta = send(&v.t.app, json_get("/api/v1/meta", None)).await.json();
    assert_eq!(meta["features"]["voice"], json!(true));
    assert_eq!(meta["voice"]["default_voice"], "af_heart");
    assert_eq!(meta["voice"]["envelope_hz"], 20);
    assert!(meta["voice"]["phrases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k == "correct"));

    let session = start(&v.t, &bearer).await;
    let card = session["card"]["card_id"].as_i64().unwrap();
    assert!(
        session["card"]["audio_front"].is_null(),
        "nothing is made inside a request"
    );

    let made = run_once(&v.t.state).await;
    let calls = v.fake.calls();
    // The shown card and the three after it, both sides, plus the
    // phrase set for the account's voice.
    let card_calls = calls
        .iter()
        .filter(|(_, text)| CARDS.iter().any(|(f, b)| text == f || text == b))
        .count();
    assert_eq!(card_calls, 8, "{calls:?}");
    assert!(
        calls
            .iter()
            .any(|(voice, text)| voice == "af_heart" && text == "Correct."),
        "{calls:?}"
    );
    assert!(calls
        .iter()
        .any(|(_, text)| text.starts_with("Hi, I'm Heart.")));
    assert_eq!(made as usize, calls.len());
    // The shown card went first.
    assert_eq!(calls[0].1, CARDS[0].0);

    let reply = send(
        &v.t.app,
        json_get(&audio_path(card, "front"), Some(&bearer)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.header("content-type"), Some("audio/mp4"));
    // The API's layer marks every reply no-store; the page's twin
    // carries the revalidating policy (see the page test).
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.bytes, FakeTts::bytes_for(CARDS[0].0));
    let etag = reply.header("etag").unwrap().to_string();
    let mut revalidate = json_get(&audio_path(card, "front"), Some(&bearer));
    revalidate
        .headers_mut()
        .insert("if-none-match", etag.parse().unwrap());
    let cached = send(&v.t.app, revalidate).await;
    assert_eq!(cached.status, StatusCode::NOT_MODIFIED);
    assert_eq!(cached.header("etag"), Some(etag.as_str()));

    let bad = send(&v.t.app, json_get(&audio_path(card, "up"), Some(&bearer))).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // The next session start carries the descriptor; the reveal both.
    let session = start(&v.t, &bearer).await;
    let front = &session["card"]["audio_front"];
    assert_eq!(front["url"], audio_path(card, "front"));
    assert_eq!(
        front["duration_ms"],
        (CARDS[0].0.chars().count() * 60) as i64
    );
    assert_eq!(front["envelope"], "AECA/w==");
    let sid = session["session_id"].as_i64().unwrap();
    let reveal = send(
        &v.t.app,
        json_post(
            &format!("/api/v1/study/sessions/{sid}/reveal"),
            Some(&bearer),
            &json!({ "card_id": card }),
        ),
    )
    .await
    .json();
    assert_eq!(reveal["audio_back"]["url"], audio_path(card, "back"));
    let back = send(&v.t.app, json_get(&audio_path(card, "back"), Some(&bearer))).await;
    assert_eq!(back.bytes, FakeTts::bytes_for(CARDS[0].1));
}

#[tokio::test]
async fn a_clip_not_made_yet_is_a_404_that_queues_it() {
    let v = voiced("tts-miss");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    // Card 5 is past the lookahead of a fresh session.
    let reply = send(&v.t.app, json_get(&audio_path(5, "front"), Some(&bearer))).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.json()["error"]["code"], "not_ready");
    assert!(
        v.fake.calls().is_empty(),
        "nothing is made inside a request"
    );
    run_once(&v.t.state).await;
    let calls = v.fake.calls();
    assert!(
        calls.iter().any(|(_, text)| text == CARDS[4].0),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|(_, text)| text == CARDS[4].1),
        "both sides"
    );
    let reply = send(&v.t.app, json_get(&audio_path(5, "front"), Some(&bearer))).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        calls.iter().filter(|(_, text)| text == CARDS[4].0).count(),
        1,
        "one synthesis per side"
    );
}

#[tokio::test]
async fn identical_words_across_accounts_share_one_clip() {
    let v = voiced("tts-shared");
    let (a, bearer_a, _) = seeded(&v.t, "FlashTester");
    let (b, bearer_b, _) = seeded(&v.t, "FlashTwo");
    start(&v.t, &bearer_a).await;
    start(&v.t, &bearer_b).await;
    run_once(&v.t.state).await;
    let firsts = v
        .fake
        .calls()
        .iter()
        .filter(|(_, text)| text == CARDS[0].0)
        .count();
    assert_eq!(firsts, 1, "one recording for both accounts");
    let first_card = |user: UserId| {
        v.t.store
            .list_cards(user, None, None, 50)
            .unwrap()
            .into_iter()
            .find(|c| c.front == CARDS[0].0)
            .unwrap()
            .id
            .0
    };
    let card_a = first_card(a);
    let card_b = first_card(b);
    assert_ne!(card_a, card_b);
    for (card, bearer) in [(card_a, &bearer_a), (card_b, &bearer_b)] {
        let reply = send(&v.t.app, json_get(&audio_path(card, "front"), Some(bearer))).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.bytes, FakeTts::bytes_for(CARDS[0].0));
    }
    assert_eq!(
        v.t.store.voice_clip_count().unwrap() as usize,
        v.fake.calls().len()
    );
    // Not another user's card, whatever it says.
    let foreign = send(
        &v.t.app,
        json_get(&audio_path(card_b, "front"), Some(&bearer_a)),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(foreign.json()["error"]["code"], "not_found");
}

#[tokio::test]
async fn changing_the_voice_starts_cold_and_queues_its_phrases() {
    let v = voiced("tts-voice-change");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    start(&v.t, &bearer).await;
    run_once(&v.t.state).await;
    let before = v.fake.calls().len();
    let saved = send(
        &v.t.app,
        json_put(
            "/api/v1/settings/voice",
            Some(&bearer),
            &json!({ "voice_id": "bm_fable" }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT);
    let miss = send(&v.t.app, json_get(&audio_path(1, "front"), Some(&bearer))).await;
    assert_eq!(
        miss.status,
        StatusCode::NOT_FOUND,
        "a new voice is a cold cache"
    );
    run_once(&v.t.state).await;
    let calls = v.fake.calls();
    assert!(calls.len() > before);
    assert!(calls[before..].iter().all(|(voice, _)| voice == "bm_fable"));
    assert!(calls[before..].iter().any(|(_, text)| text == "Correct."));
    assert!(calls[before..]
        .iter()
        .any(|(_, text)| text.starts_with("Hi, I'm Fable.")));
    let hit = send(&v.t.app, json_get(&audio_path(1, "front"), Some(&bearer))).await;
    assert_eq!(hit.status, StatusCode::OK);
}

#[tokio::test]
async fn phrases_are_listed_and_served_by_key() {
    let v = voiced("tts-phrases");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    let listing = send(&v.t.app, json_get("/api/v1/voice/phrases", Some(&bearer))).await;
    assert_eq!(listing.status, StatusCode::OK, "{}", listing.text());
    let set = listing.json();
    assert_eq!(set["voice_id"], "af_heart");
    assert_eq!(set["model"], FAKE_MODEL);
    assert!(set["phrases"]["correct"].is_null(), "not made yet");
    run_once(&v.t.state).await;
    let set = send(&v.t.app, json_get("/api/v1/voice/phrases", Some(&bearer)))
        .await
        .json();
    assert_eq!(
        set["phrases"]["correct"]["url"],
        "/api/v1/voice/phrases/af_heart/correct/audio"
    );
    assert!(set["phrases"]["sample"]["duration_ms"].as_i64().unwrap() > 0);
    let clip = send(
        &v.t.app,
        json_get(
            "/api/v1/voice/phrases/af_heart/correct/audio",
            Some(&bearer),
        ),
    )
    .await;
    assert_eq!(clip.status, StatusCode::OK);
    assert_eq!(clip.bytes, FakeTts::bytes_for("Correct."));
    // Another voice on request, unknown keys and voices are not found.
    let other = send(
        &v.t.app,
        json_get("/api/v1/voice/phrases?voice=bm_fable", Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(other["voice_id"], "bm_fable");
    for path in [
        "/api/v1/voice/phrases/af_heart/nope/audio",
        "/api/v1/voice/phrases/zz_nobody/correct/audio",
        "/api/v1/voice/phrases?voice=zz_nobody",
    ] {
        let reply = send(&v.t.app, json_get(path, Some(&bearer))).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
    }
    let voices = send(&v.t.app, json_get("/api/v1/voice/voices", Some(&bearer)))
        .await
        .json();
    assert_eq!(voices["default"], "af_heart");
    let groups = voices["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 4);
    assert_eq!(groups[0]["label"], "American English");
    assert_eq!(groups[0]["voices"][3]["id"], "af_heart");
    assert_eq!(groups[0]["voices"][3]["name"], "Heart");
    let total: usize = groups
        .iter()
        .map(|g| g["voices"].as_array().unwrap().len())
        .sum();
    assert_eq!(total, 41);
}

#[tokio::test]
async fn a_failing_sidecar_never_blocks_a_request() {
    let v = voiced("tts-down");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    v.fake
        .set_failing(Some(TtsError::Unavailable("connection refused".into())));
    let started = std::time::Instant::now();
    let session = start(&v.t, &bearer).await;
    assert!(session["card"]["audio_front"].is_null());
    let reply = send(&v.t.app, json_get(&audio_path(1, "front"), Some(&bearer))).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    let made = run_once(&v.t.state).await;
    assert_eq!(made, 0);
    assert!(v.fake.calls().is_empty());
    assert_eq!(v.t.store.voice_clip_count().unwrap(), 0);
    // Inside the backoff nothing is tried again, even once it is back.
    v.fake.set_failing(None);
    let made = run_once(&v.t.state).await;
    assert_eq!(made, 0, "the worker backs off");
    assert!(v.fake.calls().is_empty());
}

#[tokio::test]
async fn settings_round_trip_over_the_api_and_the_web() {
    let v = voiced("tts-settings");
    let (user, bearer, _) = seeded(&v.t, "FlashTester");
    let settings = send(&v.t.app, json_get("/api/v1/settings", Some(&bearer)))
        .await
        .json();
    assert_eq!(
        settings["voice"],
        json!({
            "voice_id": "af_heart",
            "speech_rate": 1.0,
            "voice_apply": "auto",
            "sound_cues": true
        })
    );
    let saved = send(
        &v.t.app,
        json_put(
            "/api/v1/settings/voice",
            Some(&bearer),
            &json!({
                "speech_rate": 1.15,
                "voice_apply": "confirm",
                "sound_cues": false
            }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.text());
    let settings = send(&v.t.app, json_get("/api/v1/settings", Some(&bearer)))
        .await
        .json();
    assert_eq!(settings["voice"]["voice_id"], "af_heart", "untouched");
    assert_eq!(settings["voice"]["voice_apply"], "confirm");
    assert_eq!(settings["voice"]["sound_cues"], false);
    assert!(
        settings["voice"].get("voice_mode").is_none(),
        "no passive mode"
    );
    let rate = settings["voice"]["speech_rate"].as_f64().unwrap();
    assert!((rate - 1.15).abs() < 0.001);

    for (body, sentence) in [
        (json!({ "speech_rate": 1.5 }), "speech rate must be between"),
        (json!({ "voice_id": "zz_nobody" }), "unknown voice"),
        (json!({ "voice_apply": "maybe" }), "unknown grading setting"),
    ] {
        let reply = send(
            &v.t.app,
            json_put("/api/v1/settings/voice", Some(&bearer), &body),
        )
        .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}");
        assert!(reply.text().contains(sentence), "{body}: {}", reply.text());
    }

    // The web form saves the whole card and comes back to the page.
    let cookie = web_session(&v.t.store, user);
    let posted = send(
        &v.t.app,
        form_post(
            "/settings/voice",
            Some(&cookie),
            "voice_id=bm_george&speech_rate=0.8&voice_apply=auto",
        ),
    )
    .await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.text());
    assert_eq!(posted.location(), "/settings?saved=voice");
    let settings = v.t.services.voice_settings(user).unwrap();
    assert_eq!(settings.voice_id, "bm_george");
    assert!(!settings.sound_cues, "an absent checkbox is off");
    assert_eq!(settings.apply, flash_core::VoiceApply::Auto);
    // From the study page's dialog: nothing to navigate to.
    let mut dialog = form_post(
        "/settings/voice",
        Some(&cookie),
        "voice_id=bm_george&speech_rate=1.0&voice_apply=auto&sound_cues=on",
    );
    dialog
        .headers_mut()
        .insert("hx-request", "true".parse().unwrap());
    let posted = send(&v.t.app, dialog).await;
    assert_eq!(posted.status, StatusCode::NO_CONTENT);
    assert!(v.t.services.voice_settings(user).unwrap().sound_cues);
    let page = send(&v.t.app, get("/settings?saved=voice", Some(&cookie)))
        .await
        .text();
    assert!(page.contains("Saved"), "{page}");
    assert!(
        page.contains(r#"aria-hidden="true">American English</div>"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<input type="radio" name="voice_id" value="bm_george" checked> George"#),
        "{page}"
    );
    assert_eq!(page.matches(r#"class="voice-group-label""#).count(), 4);
    assert_eq!(page.matches(r#"data-voice-sample=""#).count(), 41);
    assert!(
        !page.contains("voice_mode") && !page.contains("assive"),
        "no passive mode: {page}"
    );
}

#[tokio::test]
async fn the_study_page_carries_the_clips_and_the_voice_settings() {
    let v = voiced("tts-page");
    let (user, _, _) = seeded(&v.t, "FlashTester");
    let cookie = web_session(&v.t.store, user);
    let page = send(&v.t.app, get("/study", Some(&cookie))).await.text();
    assert!(!page.contains("data-audio-front"), "not made yet: {page}");
    run_once(&v.t.state).await;
    let page = send(&v.t.app, get("/study", Some(&cookie))).await.text();
    assert!(
        page.contains(r#"data-audio-front="/study/cards/1/audio?side=front""#),
        "{page}"
    );
    let clip = send(
        &v.t.app,
        get("/study/cards/1/audio?side=front", Some(&cookie)),
    )
    .await;
    assert_eq!(clip.status, StatusCode::OK);
    assert_eq!(clip.header("content-type"), Some("audio/mp4"));
    assert_eq!(clip.header("cache-control"), Some("private, no-cache"));
    assert!(clip.header("etag").is_some());
    let phrase = send(
        &v.t.app,
        get("/voice/phrases/af_heart/correct/audio", Some(&cookie)),
    )
    .await;
    assert_eq!(phrase.status, StatusCode::OK);
    assert_eq!(
        phrase.header("cache-control"),
        Some("private, max-age=86400")
    );
    let phrases = send(&v.t.app, get("/voice/phrases", Some(&cookie))).await;
    assert_eq!(phrases.status, StatusCode::OK);
    assert_eq!(
        phrases.json()["phrases"]["correct"]["url"],
        "/voice/phrases/af_heart/correct/audio"
    );
    for path in [
        "/static/speech.js",
        "/static/glow.js",
        "/static/cue-tick.wav",
        "/static/cue-done.wav",
    ] {
        let reply = send(&v.t.app, get(path, None)).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
    }
    // The signed-out page does not hand out clips.
    let anon = send(&v.t.app, get("/study/cards/1/audio?side=front", None)).await;
    assert_ne!(anon.status, StatusCode::OK);
}

#[tokio::test]
async fn housekeeping_sweeps_clips_nobody_names_but_keeps_phrases() {
    let v = voiced("tts-sweep");
    let (user, bearer, deck) = seeded(&v.t, "FlashTester");
    start(&v.t, &bearer).await;
    run_once(&v.t.state).await;
    let before = v.t.store.voice_clip_count().unwrap();
    assert!(before > 8);
    let phrases = flash_server::tts::PHRASES.len() as u64;
    v.t.services.delete_deck(user, deck).unwrap();
    let far = now_ms() + 2 * 24 * 60 * 60 * 1000;
    flash_server::scheduler::housekeeping(
        &v.t.services,
        v.t.state.media.as_ref(),
        &v.t.data_dir,
        far,
    );
    assert_eq!(v.t.store.voice_clip_count().unwrap(), phrases);
    let key = flash_server::tts::clip_key(FAKE_MODEL, "af_heart", CARDS[0].0);
    assert!(matches!(
        v.t.state.media.get(&key, None),
        Err(flash_server::media_store::MediaStoreError::NotFound)
    ));
    let phrase_key = flash_server::tts::clip_key(FAKE_MODEL, "af_heart", "Correct.");
    assert!(v.t.state.media.get(&phrase_key, None).is_ok());
}

#[tokio::test]
async fn every_phrase_of_every_voice_is_made_once_by_the_warm_up() {
    let v = voiced("tts-warm");
    let (user, bearer, _) = seeded(&v.t, "FlashTester");
    let voices = flash_server::tts::voices::all().count();
    let total = voices * PHRASES.len();
    // Nothing exists yet: no sample in the listing, no address on a row.
    let listing = send(&v.t.app, json_get("/api/v1/voice/voices", Some(&bearer)))
        .await
        .json();
    assert!(listing["groups"][0]["voices"][0]["sample"].is_null());
    let cookie = web_session(&v.t.store, user);
    let page = send(&v.t.app, get("/settings", Some(&cookie))).await.text();
    assert!(page.contains(r#"data-voice-sample="af_heart""#), "{page}");
    assert!(!page.contains("data-sample-url"), "{page}");
    assert!(!page.contains("Play sample"), "{page}");
    // Pass and drain until nothing is missing: a batch at a time,
    // samples first.
    let mut passes = 0;
    loop {
        let pass = warm_pass(&v.t.state);
        assert!(pass.queued <= WARM_BATCH);
        run_once(&v.t.state).await;
        passes += 1;
        if pass.done {
            break;
        }
        assert!(passes <= total, "the warm-up never finishes");
    }
    let calls = v.fake.calls();
    assert_eq!(calls.len(), total);
    let mut distinct = calls.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), total, "no clip made twice");
    assert!(
        calls[..voices]
            .iter()
            .all(|(voice, text)| is_sample(voice, text)),
        "samples first"
    );
    // Each voice introduced itself in its own language.
    assert!(
        calls
            .iter()
            .any(|(voice, text)| voice == "jf_alpha"
                && text.starts_with("こんにちは、アルファです。"))
    );
    assert!(calls
        .iter()
        .any(|(voice, text)| voice == "zf_xiaobei" && text.starts_with("你好，我是晓北。")));
    assert!(calls
        .iter()
        .any(|(voice, text)| voice == "bm_lewis" && text.starts_with("Hi, I'm Lewis.")));
    assert!(
        !calls
            .iter()
            .any(|(voice, text)| voice.starts_with(['j', 'z']) && text.starts_with("Hi, I'm")),
        "{calls:?}"
    );
    // A later look finds nothing to do and asks the sidecar for nothing.
    let again = warm_pass(&v.t.state);
    assert!(again.done);
    assert_eq!(again.queued, 0);
    run_once(&v.t.state).await;
    assert_eq!(v.fake.calls().len(), total);
    // Every voice carries its sample in the listing, on both surfaces.
    let listing = send(&v.t.app, json_get("/api/v1/voice/voices", Some(&bearer)))
        .await
        .json();
    for group in listing["groups"].as_array().unwrap() {
        for voice in group["voices"].as_array().unwrap() {
            let id = voice["id"].as_str().unwrap();
            assert_eq!(
                voice["sample"]["url"],
                format!("/api/v1/voice/phrases/{id}/sample/audio")
            );
            assert!(voice["sample"]["duration_ms"].as_i64().unwrap() > 0, "{id}");
        }
    }
    let web = send(&v.t.app, get("/voice/voices", Some(&cookie)))
        .await
        .json();
    assert_eq!(
        web["groups"][0]["voices"][0]["sample"]["url"],
        "/voice/phrases/af_alloy/sample/audio"
    );
    let page = send(&v.t.app, get("/settings", Some(&cookie))).await.text();
    assert!(
        page.contains(r#"data-sample-url="/voice/phrases/jf_alpha/sample/audio""#),
        "{page}"
    );
    // A voice nobody picked has its whole set, so a first grade in it
    // never waits.
    let set = send(
        &v.t.app,
        json_get("/api/v1/voice/phrases?voice=jf_alpha", Some(&bearer)),
    )
    .await
    .json();
    assert!(set["phrases"]
        .as_object()
        .unwrap()
        .values()
        .all(|clip| !clip.is_null()));
}

#[tokio::test]
async fn live_work_outranks_the_warm_up() {
    let v = voiced("tts-warm-rank");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    let pass = warm_pass(&v.t.state);
    assert_eq!(pass.queued, WARM_BATCH);
    assert!(!pass.done);
    // A session starts: its cards and its voice's phrases are all made
    // before the batch of samples queued ahead of them.
    start(&v.t, &bearer).await;
    run_once(&v.t.state).await;
    let calls = v.fake.calls();
    let first_sample = calls
        .iter()
        .position(|(voice, text)| is_sample(voice, text))
        .expect("the batch was made");
    assert!(first_sample > 0);
    assert!(calls[..first_sample]
        .iter()
        .any(|(_, text)| CARDS.iter().any(|(front, _)| front == text)));
    assert!(calls[..first_sample]
        .iter()
        .any(|(_, text)| text == "Correct."));
    assert!(
        calls[first_sample..]
            .iter()
            .all(|(voice, text)| is_sample(voice, text)),
        "{calls:?}"
    );
    assert_eq!(calls.len() - first_sample, WARM_BATCH);
}

#[tokio::test]
async fn a_demo_line_that_changed_its_words_is_made_again() {
    let v = voiced("tts-demo-words");
    let (user, bearer, _) = seeded(&v.t, "FlashTester");
    // Yesterday's clip: the Japanese voice's demo in English, filed as
    // its sample, bytes and all.
    let old_text = "Hi, I'm Alpha. I've read all your cards. Twice. Shall we?";
    let old_key = clip_key(FAKE_MODEL, "jf_alpha", old_text);
    v.t.state.media.put(&old_key, "audio/mp4", b"old").unwrap();
    v.t.store
        .insert_voice_clip(&VoiceClipNew {
            key: &old_key,
            kind: VoiceClipKind::Phrase,
            voice: "jf_alpha",
            model: FAKE_MODEL,
            phrase: Some("sample"),
            mime: "audio/mp4",
            size: 3,
            duration_ms: 900,
            envelope: &[1, 2, 3],
            now_ms: now_ms(),
        })
        .unwrap();
    // The listing does not offer the old words, and looking dropped them.
    let listing = send(&v.t.app, json_get("/api/v1/voice/voices", Some(&bearer)))
        .await
        .json();
    let alpha = |listing: &serde_json::Value| {
        listing["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|g| g["voices"].as_array().unwrap().clone())
            .find(|voice| voice["id"] == "jf_alpha")
            .unwrap()
    };
    assert!(alpha(&listing)["sample"].is_null(), "{listing}");
    assert!(v.t.store.find_voice_clip(&old_key).unwrap().is_none());
    assert!(matches!(
        v.t.state.media.get(&old_key, None),
        Err(flash_server::media_store::MediaStoreError::NotFound)
    ));
    let cookie = web_session(&v.t.store, user);
    let reply = send(
        &v.t.app,
        get("/voice/phrases/jf_alpha/sample/audio", Some(&cookie)),
    )
    .await;
    assert_ne!(reply.status, StatusCode::OK);
    // The warm-up makes the sample in Japanese, never in English again.
    loop {
        let pass = warm_pass(&v.t.state);
        run_once(&v.t.state).await;
        if pass.done {
            break;
        }
    }
    let calls = v.fake.calls();
    let japanese = phrase_text("sample", "jf_alpha").unwrap();
    assert!(
        calls
            .iter()
            .any(|(voice, text)| voice == "jf_alpha" && *text == japanese),
        "{calls:?}"
    );
    assert!(!calls.iter().any(|(_, text)| text == old_text), "{calls:?}");
    let listing = send(&v.t.app, json_get("/api/v1/voice/voices", Some(&bearer)))
        .await
        .json();
    assert_eq!(
        alpha(&listing)["sample"]["url"],
        "/api/v1/voice/phrases/jf_alpha/sample/audio"
    );
    let reply = send(
        &v.t.app,
        get("/voice/phrases/jf_alpha/sample/audio", Some(&cookie)),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(v
        .t
        .store
        .find_voice_clip(&clip_key(FAKE_MODEL, "jf_alpha", &japanese))
        .unwrap()
        .is_some());
    // One row per voice's sample: the old one did not linger.
    let samples = v.t.store.find_phrase_clips(FAKE_MODEL, "sample").unwrap();
    assert_eq!(samples.len(), flash_server::tts::voices::all().count());
}

/// The clients wait for a clip rather than read with the device's own
/// voice, so the server's part is to queue what they are about to wait
/// on as early as it can, and to say when nothing will ever come.
#[tokio::test]
async fn picking_a_voice_queues_the_card_on_screen_in_it() {
    let v = voiced("tts-switch");
    let (_, bearer, _) = seeded(&v.t, "FlashTester");
    let session = start(&v.t, &bearer).await;
    let card = session["card"]["card_id"].as_i64().unwrap();
    // The session start queued the first cards in the default voice.
    run_once(&v.t.state).await;
    let before = v.fake.calls().len();
    let saved = send(
        &v.t.app,
        json_put(
            "/api/v1/settings/voice",
            Some(&bearer),
            &json!({ "voice_id": "bm_fable" }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT);
    // Not made inside the save or the first ask: a 404 the client
    // asks again after.
    let pending = send(
        &v.t.app,
        json_get(&audio_path(card, "front"), Some(&bearer)),
    )
    .await;
    assert_eq!(pending.status, StatusCode::NOT_FOUND);
    assert_eq!(pending.json()["error"]["code"], "not_ready");
    assert_eq!(v.fake.calls().len(), before);
    run_once(&v.t.state).await;
    // The card on screen came first in the new voice, both sides, ahead
    // of the voice's phrases.
    let calls: Vec<(String, String)> = v.fake.calls().into_iter().skip(before).collect();
    assert_eq!(
        calls.first(),
        Some(&("bm_fable".to_string(), CARDS[0].0.to_string())),
        "{calls:?}"
    );
    assert_eq!(
        calls.get(1),
        Some(&("bm_fable".to_string(), CARDS[0].1.to_string())),
        "{calls:?}"
    );
    let ready = send(
        &v.t.app,
        json_get(&audio_path(card, "front"), Some(&bearer)),
    )
    .await;
    assert_eq!(ready.status, StatusCode::OK);
    assert_eq!(ready.bytes, FakeTts::bytes_for(CARDS[0].0));
}

#[tokio::test]
async fn a_side_too_long_to_voice_is_a_204_nothing_waits_on() {
    let v = voiced("tts-long");
    let (user, bearer, deck) = seeded(&v.t, "FlashTester");
    let long = "word ".repeat(300);
    let ids =
        v.t.services
            .create_cards_in_deck(user, deck, &[(long, "Short.".into(), vec![])], now_ms())
            .unwrap();
    let card = ids[0].0;
    let api = send(
        &v.t.app,
        json_get(&audio_path(card, "front"), Some(&bearer)),
    )
    .await;
    assert_eq!(api.status, StatusCode::NO_CONTENT);
    assert!(api.bytes.is_empty());
    let cookie = web_session(&v.t.store, user);
    let page = send(
        &v.t.app,
        get(
            &format!("/study/cards/{card}/audio?side=front"),
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(page.status, StatusCode::NO_CONTENT);
    // The short side is an ordinary miss, queued and made.
    let back = send(&v.t.app, json_get(&audio_path(card, "back"), Some(&bearer))).await;
    assert_eq!(back.status, StatusCode::NOT_FOUND);
    assert_eq!(back.json()["error"]["code"], "not_ready");
    run_once(&v.t.state).await;
    assert_eq!(v.fake.calls().len(), 1, "{:?}", v.fake.calls());
    let back = send(&v.t.app, json_get(&audio_path(card, "back"), Some(&bearer))).await;
    assert_eq!(back.status, StatusCode::OK);
}

/// A server with a voice and a model that answers a question.
fn voiced_with_model(tag: &str, replies: Vec<FakeReply>) -> Voiced {
    let fake = Arc::new(FakeTts::default());
    let t = AppBuilder::new(tag)
        .tts(fake.clone())
        .ai(
            Arc::new(FakeProvider::scripted(replies)),
            vec![AiInput::Text],
        )
        .build();
    Voiced { t, fake }
}

#[tokio::test]
async fn the_answer_to_a_question_is_made_first_in_line_and_served_by_token() {
    let v = voiced_with_model(
        "tts-answer",
        vec![FakeReply::Turn(Ok(FakeProvider::canned_question()))],
    );
    let (user, bearer, _) = seeded(&v.t, "FlashTester");
    let session = start(&v.t, &bearer).await;
    let sid = session["session_id"].as_i64().unwrap();
    let card = session["card"]["card_id"].as_i64().unwrap();
    let asked = send(
        &v.t.app,
        json_post(
            &format!("/api/v1/study/sessions/{sid}/turn"),
            Some(&bearer),
            &json!({ "card_id": card, "state": "front", "transcript": "why" }),
        ),
    )
    .await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text());
    let json = asked.json();
    assert_eq!(json["intent"], "question");
    let spoken = json["spoken_answer"].as_str().unwrap().to_string();
    let url = json["spoken_answer_url"].as_str().unwrap().to_string();
    assert!(
        url.starts_with("/api/v1/voice/answers/") && url.ends_with("/audio"),
        "{url}"
    );
    // Not made inside the turn: a 404 until the worker gets to it.
    assert!(!v.fake.calls().iter().any(|(_, text)| *text == spoken));
    let pending = send(&v.t.app, json_get(&url, Some(&bearer))).await;
    assert_eq!(pending.status, StatusCode::NOT_FOUND);
    assert_eq!(pending.json()["error"]["code"], "not_ready");
    run_once(&v.t.state).await;
    // Made before the cards coming up and the phrases, right behind the
    // card on screen.
    let calls = v.fake.calls();
    let at = |text: &str| calls.iter().position(|(_, t)| t == text).unwrap();
    assert!(at(&spoken) < at(CARDS[1].0), "{calls:?}");
    assert!(at(&spoken) < at("Correct."), "{calls:?}");
    let made = send(&v.t.app, json_get(&url, Some(&bearer))).await;
    assert_eq!(made.status, StatusCode::OK);
    assert_eq!(made.bytes, FakeTts::bytes_for(&spoken));
    assert_eq!(made.header("content-type"), Some("audio/mp4"));
    assert_eq!(made.header("cache-control"), Some("no-store"));
    // Held in memory, not in the store: every other call became a row.
    assert_eq!(
        v.t.store.voice_clip_count().unwrap(),
        calls.len() as u64 - 1
    );
    // The page twin serves the same token to the same account; another
    // account gets nothing by it.
    let cookie = web_session(&v.t.store, user);
    let page = send(&v.t.app, get(&url.replace("/api/v1", ""), Some(&cookie))).await;
    assert_eq!(page.status, StatusCode::OK);
    assert_eq!(page.bytes, FakeTts::bytes_for(&spoken));
    let (_, other, _) = seeded(&v.t, "FlashOther");
    let foreign = send(&v.t.app, json_get(&url, Some(&other))).await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(foreign.json()["error"]["code"], "not_found");
}
