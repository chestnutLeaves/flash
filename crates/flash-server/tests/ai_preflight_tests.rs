//! The preflight's caps, one sentence each, and that a job cannot be
//! queued past them. The facts are written straight into the source
//! row, the way ingest would have written them for a file of that size.

mod common;

use std::sync::Arc;

use axum::http::StatusCode;
use common::*;
use flash_core::{AiSourceId, UserId};
use flash_server::ai::fake::FakeProvider;
use flash_server::bounds;
use flash_server::policy::ALL_AI_INPUTS;
use flash_server::service::now_ms;
use flash_store::{AiSourceKind, AiSourceNew, Store};
use serde_json::json;

fn source(store: &Store, user: UserId, kind: AiSourceKind, facts: Facts) -> AiSourceId {
    store
        .create_ai_source(
            user,
            &AiSourceNew {
                kind,
                sha256: &"ab".repeat(32),
                mime: "",
                size: 10,
                filename: "f",
                url: None,
                text: None,
                pages: facts.pages,
                duration_s: facts.duration_s,
                est_words: facts.est_words,
                image_count: facts.image_count,
                origin_mime: None,
                now_ms: now_ms(),
            },
        )
        .unwrap()
}

#[derive(Default)]
struct Facts {
    pages: Option<u32>,
    est_words: Option<u32>,
    image_count: Option<u32>,
    duration_s: Option<u32>,
}

#[tokio::test]
async fn each_cap_has_its_sentence_and_blocks_the_job() {
    let t = AppBuilder::new("ai-preflight")
        .ai(Arc::new(FakeProvider::default()), ALL_AI_INPUTS.to_vec())
        .build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let deck = t.services.create_deck(user, "D", "", now_ms()).unwrap();

    let cases = [
        (
            AiSourceKind::Pdf,
            Facts {
                pages: Some(220),
                est_words: Some(100),
                ..Facts::default()
            },
            "too_many_pages",
            "This PDF has 220 pages; the limit is 150.",
        ),
        (
            AiSourceKind::Pdf,
            Facts {
                pages: Some(12),
                est_words: Some(21_000),
                ..Facts::default()
            },
            "too_many_words",
            "That's about 21,000 words; the limit is 15,000.",
        ),
        (
            AiSourceKind::Text,
            Facts {
                est_words: Some(15_001),
                ..Facts::default()
            },
            "too_many_words",
            "That's about 15,001 words; the limit is 15,000.",
        ),
        (
            AiSourceKind::Image,
            Facts {
                image_count: Some(21),
                ..Facts::default()
            },
            "too_many_images",
            "Only 20 photos per generation.",
        ),
        (
            AiSourceKind::Video,
            Facts {
                duration_s: Some(112 * 60 + 5),
                ..Facts::default()
            },
            "too_long",
            "This recording is 1 h 52 min; the limit is 90 minutes.",
        ),
        (
            AiSourceKind::Audio,
            Facts {
                duration_s: Some(bounds::AI_MAX_MEDIA_SECONDS + 60),
                ..Facts::default()
            },
            "too_long",
            "This recording is 1 h 31 min; the limit is 90 minutes.",
        ),
    ];
    for (kind, facts, code, reason) in cases {
        let id = source(&t.store, user, kind, facts);
        let r = send(
            &t.app,
            json_get(
                &format!("/api/v1/ai/sources/{}/preflight", id.0),
                Some(&bearer),
            ),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
        let p = r.json();
        assert_eq!(p["allowed"], false, "{p}");
        assert_eq!(p["code"], code, "{p}");
        assert_eq!(p["reason"], reason, "{p}");
        assert_eq!(p["kind"], kind.as_str());
        let job = send(
            &t.app,
            json_post(
                "/api/v1/ai/jobs",
                Some(&bearer),
                &json!({ "source_ids": [id.0], "deck_id": deck.0 }),
            ),
        )
        .await;
        assert_eq!(job.status, StatusCode::BAD_REQUEST, "{}", job.text());
        assert_eq!(job.json()["error"]["message"], reason);
        // The set's preflight says the same, and names the member.
        let set = send(
            &t.app,
            json_post(
                "/api/v1/ai/preflight",
                Some(&bearer),
                &json!({ "source_ids": [id.0] }),
            ),
        )
        .await;
        assert_eq!(set.status, StatusCode::OK, "{}", set.text());
        let s = set.json();
        assert_eq!(s["allowed"], false);
        assert_eq!(s["code"], code);
        assert_eq!(s["reason"], reason);
        assert_eq!(s["refused_source_id"], id.0);
    }

    // Several sources that each fit can add up past a cap; the sentence
    // then speaks of the whole set.
    let a = source(
        &t.store,
        user,
        AiSourceKind::Pdf,
        Facts {
            pages: Some(100),
            est_words: Some(1_000),
            ..Facts::default()
        },
    );
    let b = source(
        &t.store,
        user,
        AiSourceKind::Pdf,
        Facts {
            pages: Some(60),
            est_words: Some(1_000),
            ..Facts::default()
        },
    );
    let set = send(
        &t.app,
        json_post(
            "/api/v1/ai/preflight",
            Some(&bearer),
            &json!({ "source_ids": [a.0, b.0] }),
        ),
    )
    .await;
    let s = set.json();
    assert_eq!(s["allowed"], false, "{s}");
    assert_eq!(s["code"], "too_many_pages");
    assert_eq!(
        s["reason"],
        "These files have 160 pages in all; one generation reads up to 150."
    );
    assert!(s["refused_source_id"].is_null());
    assert_eq!(s["totals"]["pages"], 160);
    assert_eq!(s["totals"]["sources"], 2);
    assert_eq!(s["line"], "2 sources · 160 pages · ~2,000 words");
    assert_eq!(s["sources"].as_array().unwrap().len(), 2);
    assert_eq!(s["sources"][0]["allowed"], true);
    let job = send(
        &t.app,
        json_post(
            "/api/v1/ai/jobs",
            Some(&bearer),
            &json!({ "source_ids": [a.0, b.0], "deck_id": deck.0 }),
        ),
    )
    .await;
    assert_eq!(job.status, StatusCode::BAD_REQUEST, "{}", job.text());
    // The same source twice, an empty set, and a set of someone else's
    // are refused before anything is read.
    for (body, status) in [
        (json!({ "source_ids": [a.0, a.0] }), StatusCode::BAD_REQUEST),
        (json!({ "source_ids": [] }), StatusCode::BAD_REQUEST),
        (
            json!({ "source_ids": [a.0, 999_999] }),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let r = send(
            &t.app,
            json_post("/api/v1/ai/preflight", Some(&bearer), &body),
        )
        .await;
        assert_eq!(r.status, status, "{body}: {}", r.text());
    }

    // At the cap exactly, and with facts ingest could not read, the
    // source passes.
    for (kind, facts) in [
        (
            AiSourceKind::Pdf,
            Facts {
                pages: Some(bounds::AI_MAX_PAGES),
                est_words: Some(bounds::AI_MAX_WORDS),
                ..Facts::default()
            },
        ),
        (
            AiSourceKind::Image,
            Facts {
                image_count: Some(bounds::AI_MAX_IMAGES),
                ..Facts::default()
            },
        ),
        (
            AiSourceKind::Video,
            Facts {
                duration_s: Some(bounds::AI_MAX_MEDIA_SECONDS),
                ..Facts::default()
            },
        ),
        (AiSourceKind::Audio, Facts::default()),
        (AiSourceKind::Youtube, Facts::default()),
    ] {
        let id = source(&t.store, user, kind, facts);
        let r = send(
            &t.app,
            json_get(
                &format!("/api/v1/ai/sources/{}/preflight", id.0),
                Some(&bearer),
            ),
        )
        .await;
        let p = r.json();
        assert_eq!(p["allowed"], true, "{p}");
        assert_eq!(p["cost_class"], kind.as_str());
    }

    // An unknown source is not found, and reading one marks it used.
    let r = send(
        &t.app,
        json_get("/api/v1/ai/sources/999999/preflight", Some(&bearer)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_preflight_marks_the_source_used() {
    let t = AppBuilder::new("ai-preflight-touch")
        .ai(Arc::new(FakeProvider::default()), ALL_AI_INPUTS.to_vec())
        .build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let bearer = api_bearer(&t.store, user);
    let id = source(&t.store, user, AiSourceKind::Text, Facts::default());
    let before = t
        .store
        .get_ai_source(user, id)
        .unwrap()
        .unwrap()
        .last_used_at;
    // Move the row into the past, then read it.
    t.store
        .touch_ai_source(user, id, before - 40 * 24 * 60 * 60 * 1000)
        .unwrap();
    send(
        &t.app,
        json_get(
            &format!("/api/v1/ai/sources/{}/preflight", id.0),
            Some(&bearer),
        ),
    )
    .await;
    let after = t
        .store
        .get_ai_source(user, id)
        .unwrap()
        .unwrap()
        .last_used_at;
    assert!(after >= before, "the preflight refreshed last_used_at");
}
