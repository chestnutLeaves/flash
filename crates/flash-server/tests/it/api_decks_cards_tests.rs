//! Deck management over the API: create/rename/limits/delete, the paged
//! and searchable card list, quick-add, the rich editor's
//! create/seed/save, and a deck's goal date.

use crate::common;

use axum::http::StatusCode;
use common::*;
use flash_core::UserId;
use flash_server::service::now_ms;
use serde_json::json;

fn signed_up(t: &TestApp, name: &str) -> (UserId, String) {
    let user = member(&t.store, name, &format!("{name}@example.com"));
    (user, api_bearer(&t.store, user))
}

/// Subdecks over the API: made by path or by `parent_id`, listed in tree
/// order with `leaf`, `depth` and `parent_id` and with counts that hold
/// the decks under them, moved by id, and counted by the detail.
#[tokio::test]
async fn subdecks_nest_by_path_or_parent_and_move_by_id() {
    let t = AppBuilder::new("api-subdecks").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let create = |body: serde_json::Value| {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move { send(&app, json_post("/api/v1/decks", Some(&bearer), &body)).await }
    };

    // A path makes the decks on the way; a parent id says the same thing
    // for a client that shows a picker.
    let arrhythmias = create(json!({ "name": "Pharm::Cardio::Arrhythmias" })).await;
    assert_eq!(
        arrhythmias.status,
        StatusCode::CREATED,
        "{}",
        arrhythmias.text()
    );
    let arrhythmias = arrhythmias.json();
    assert_eq!(arrhythmias["leaf"], "Arrhythmias");
    assert_eq!(arrhythmias["depth"], 2);
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();
    let renal = create(json!({ "name": "Renal", "parent_id": pharm.0 })).await;
    assert_eq!(renal.status, StatusCode::CREATED, "{}", renal.text());
    let renal = renal.json();
    assert_eq!(renal["name"], "Pharm::Renal");
    assert_eq!(renal["parent_id"], pharm.0);
    let zebra = create(json!({ "name": "Zebra" })).await.json();

    let card = flash_core::validate_card_text("f", "b").unwrap();
    for deck in [
        arrhythmias["id"].as_i64().unwrap(),
        renal["id"].as_i64().unwrap(),
    ] {
        t.store
            .create_cards(
                user,
                flash_core::DeckId(deck),
                &[(card.clone(), Vec::new())],
                None,
                now_ms(),
            )
            .unwrap();
    }

    let list = send(&t.app, json_get("/api/v1/decks", Some(&bearer)))
        .await
        .json();
    let rows: Vec<(String, u64, u64)> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["leaf"].as_str().unwrap().to_string(),
                d["depth"].as_u64().unwrap(),
                d["new"].as_u64().unwrap(),
            )
        })
        .collect();
    let row = |leaf: &str, depth, new| (leaf.to_string(), depth, new);
    assert_eq!(
        rows,
        vec![
            row("Pharm", 0, 2),
            row("Cardio", 1, 1),
            row("Arrhythmias", 2, 1),
            row("Renal", 1, 1),
            row("Zebra", 0, 0),
        ],
        "tree order, and a deck's count holds the decks under it"
    );
    assert!(list["items"][0]["parent_id"].is_null());

    let detail = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{}", pharm.0), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(detail["subdeck_count"], 3);
    assert_eq!(detail["card_count"], 2);

    // A move keeps the deck's name and carries what is under it.
    let cardio = t
        .store
        .find_deck_by_name(user, "Pharm::Cardio")
        .unwrap()
        .unwrap();
    let moved = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/move", cardio.0),
            Some(&bearer),
            &json!({ "parent_id": zebra["id"] }),
        ),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    assert_eq!(moved.json()["name"], "Zebra::Cardio");
    assert!(t
        .store
        .find_deck_by_name(user, "Zebra::Cardio::Arrhythmias")
        .unwrap()
        .is_some());
    let out = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/move", cardio.0),
            Some(&bearer),
            &json!({ "parent_id": null }),
        ),
    )
    .await
    .json();
    assert_eq!((&out["name"], &out["depth"]), (&json!("Cardio"), &json!(0)));

    // Into itself, or into someone else's deck: refused, nothing moves.
    let inside = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/move", cardio.0),
            Some(&bearer),
            &json!({ "parent_id": arrhythmias["id"] }),
        ),
    )
    .await;
    // A store refusal is a conflict carrying its sentence, which is what
    // the app shows.
    assert_eq!(inside.status, StatusCode::CONFLICT, "{}", inside.text());
    assert!(inside.text().contains("into itself"), "{}", inside.text());
    let (stranger, _) = signed_up(&t, "other");
    let theirs = t
        .store
        .create_deck(stranger, "Theirs", "", now_ms())
        .unwrap();
    let foreign = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/move", cardio.0),
            Some(&bearer),
            &json!({ "parent_id": theirs.0 }),
        ),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deck_lifecycle_create_detail_rename_limits_delete() {
    let t = AppBuilder::new("api-decks").build();
    let (_, bearer) = signed_up(&t, "flashtester");

    let blank = send(
        &t.app,
        json_post("/api/v1/decks", Some(&bearer), &json!({ "name": "  " })),
    )
    .await;
    assert_eq!(blank.status, StatusCode::BAD_REQUEST);

    let created = send(
        &t.app,
        json_post(
            "/api/v1/decks",
            Some(&bearer),
            &json!({ "name": "Pharm", "description": "NCLEX" }),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let deck = created.json();
    assert_eq!(deck["name"], "Pharm");
    assert_eq!(deck["description"], "NCLEX");
    let id = deck["id"].as_i64().unwrap();

    // A second deck with the same name is a conflict, blank names a 400.
    let dup = send(
        &t.app,
        json_post("/api/v1/decks", Some(&bearer), &json!({ "name": "Pharm" })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let list = send(&t.app, json_get("/api/v1/decks", Some(&bearer)))
        .await
        .json();
    assert_eq!(list["items"][0]["id"], id);

    let detail = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(detail.status, StatusCode::OK);
    let d = detail.json();
    assert_eq!(d["deck"]["name"], "Pharm");
    assert!(d["limits"]["new_per_day"].is_null(), "inherits by default");
    assert_eq!(d["limits"]["default_new"], 20);
    assert_eq!(d["limits"]["default_reviews"], 200);
    assert_eq!(d["card_count"], 0);

    let renamed = send(
        &t.app,
        json_patch(
            &format!("/api/v1/decks/{id}"),
            Some(&bearer),
            &json!({ "name": "Pharmacology" }),
        ),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(renamed.json()["name"], "Pharmacology");

    let limits = send(
        &t.app,
        json_put(
            &format!("/api/v1/decks/{id}/limits"),
            Some(&bearer),
            &json!({ "new_per_day": 5, "reviews_per_day": null }),
        ),
    )
    .await;
    assert_eq!(limits.status, StatusCode::NO_CONTENT);
    let d = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(d["limits"]["new_per_day"], 5);
    assert!(d["limits"]["reviews_per_day"].is_null());
    let too_big = send(
        &t.app,
        json_put(
            &format!("/api/v1/decks/{id}/limits"),
            Some(&bearer),
            &json!({ "new_per_day": 99999 }),
        ),
    )
    .await;
    assert_eq!(too_big.status, StatusCode::BAD_REQUEST);

    let boosted = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{id}/boost"),
            Some(&bearer),
            &json!({ "extra": 10 }),
        ),
    )
    .await;
    assert_eq!(boosted.json()["boost_today"], 10);

    // Archive: the deck is still there to read, marked, and out of today;
    // unarchive clears the mark. Neither takes a body.
    let archived = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{id}/archive"),
            Some(&bearer),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(
        archived.status,
        StatusCode::NO_CONTENT,
        "{}",
        archived.text()
    );
    let list = send(&t.app, json_get("/api/v1/decks", Some(&bearer)))
        .await
        .json();
    assert!(list["items"][0]["archived_at"].is_i64(), "{list}");
    let today = send(&t.app, json_get("/api/v1/today", Some(&bearer)))
        .await
        .json();
    assert_eq!(today["decks"].as_array().unwrap().len(), 0, "{today}");
    let d = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(d.status, StatusCode::OK);
    assert!(d.json()["deck"]["archived_at"].is_i64());
    let restored = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{id}/unarchive"),
            Some(&bearer),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(restored.status, StatusCode::NO_CONTENT);
    let list = send(&t.app, json_get("/api/v1/decks", Some(&bearer)))
        .await
        .json();
    assert!(list["items"][0]["archived_at"].is_null(), "{list}");
    let missing = send(
        &t.app,
        json_post("/api/v1/decks/999999/archive", Some(&bearer), &json!({})),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let deleted = send(
        &t.app,
        json_delete(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK);
    assert_eq!(deleted.json()["cards_deleted"], 0);
    let gone = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cards_page_search_quick_add_and_delete() {
    let t = AppBuilder::new("api-cards").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let deck = t.services.create_deck(user, "Pharm", "", now_ms()).unwrap();
    let base = format!("/api/v1/decks/{}", deck.0);

    // Quick-add with tags, then 29 more so the list paginates.
    let added = send(
        &t.app,
        json_post(
            &format!("{base}/cards"),
            Some(&bearer),
            &json!({ "front": "Warfarin antidote?", "back": "Vitamin K", "tags": ["exam-2", "cardiac"] }),
        ),
    )
    .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    let first_id = added.json()["card_ids"][0].as_i64().unwrap();
    let rows: Vec<(String, String, Vec<String>)> = (1..30)
        .map(|i| (format!("Question {i}"), format!("Answer {i}"), vec![]))
        .collect();
    t.services
        .create_cards_in_deck(user, deck, &rows, now_ms())
        .unwrap();

    let page1 = send(&t.app, json_get(&format!("{base}/cards"), Some(&bearer))).await;
    assert_eq!(page1.status, StatusCode::OK);
    let p = page1.json();
    assert_eq!(p["total"], 30);
    assert_eq!(p["pages"], 2);
    assert_eq!(p["page"], 1);
    assert_eq!(p["items"].as_array().unwrap().len(), 25);
    // Newest first, like the web; the quick-added card is on page 2.
    assert_eq!(p["items"][0]["front"], "Question 29");
    let page2 = send(
        &t.app,
        json_get(&format!("{base}/cards?page=2"), Some(&bearer)),
    )
    .await
    .json();
    let first = page2["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == first_id)
        .expect("the quick-added card is listed");
    assert_eq!(first["front"], "Warfarin antidote?");
    assert_eq!(first["tags"], json!(["cardiac", "exam-2"]));
    assert!(first["kind"].is_null());

    let page2 = send(
        &t.app,
        json_get(&format!("{base}/cards?page=2"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(page2["items"].as_array().unwrap().len(), 5);
    let beyond = send(
        &t.app,
        json_get(&format!("{base}/cards?page=9"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(beyond["page"], 2, "clamped to the last page");

    let found = send(
        &t.app,
        json_get(&format!("{base}/cards?q=warfarin"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(found["total"], 1);
    assert_eq!(found["items"][0]["id"], first_id);

    let one = send(
        &t.app,
        json_get(&format!("/api/v1/cards/{first_id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(one.status, StatusCode::OK);
    assert_eq!(one.json()["deck_id"], deck.0);

    let updated = send(
        &t.app,
        json_put(
            &format!("/api/v1/cards/{first_id}"),
            Some(&bearer),
            &json!({ "front": "Warfarin reversal?", "back": "Vitamin K" }),
        ),
    )
    .await;
    assert_eq!(updated.status, StatusCode::NO_CONTENT);

    let removed = send(
        &t.app,
        json_delete(&format!("/api/v1/cards/{first_id}"), Some(&bearer)),
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let after = send(&t.app, json_get(&format!("{base}/cards"), Some(&bearer)))
        .await
        .json();
    assert_eq!(after["total"], 29);

    // Someone else's deck is simply not found: cards, quick-add, detail.
    let (_, ada) = signed_up(&t, "ada");
    for reply in [
        send(&t.app, json_get(&format!("{base}/cards"), Some(&ada))).await,
        send(
            &t.app,
            json_post(
                &format!("{base}/cards"),
                Some(&ada),
                &json!({ "front": "x", "back": "y" }),
            ),
        )
        .await,
        send(&t.app, json_get(&base, Some(&ada))).await,
    ] {
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
    }
}

/// A deck's card list leaves out its archived subdecks' cards, as the
/// web deck page does; its own archived cards are still listed.
#[tokio::test]
async fn cards_page_leaves_out_archived_subdecks() {
    let t = AppBuilder::new("api-cards-archived-sub").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let deck = t.services.create_deck(user, "Pharm", "", now_ms()).unwrap();
    let old = t
        .services
        .create_deck(user, "Pharm::Old", "", now_ms())
        .unwrap();
    let card = |front: &str| (front.to_string(), "b".to_string(), vec![]);
    let own = t
        .services
        .create_cards_in_deck(user, deck, &[card("kept"), card("shelved")], now_ms())
        .unwrap();
    t.services
        .create_cards_in_deck(user, old, &[card("in old")], now_ms())
        .unwrap();
    t.services
        .archive_cards(user, deck, &own[1..], true, now_ms())
        .unwrap();
    let shelved = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/archive", old.0),
            Some(&bearer),
            &json!({}),
        ),
    )
    .await;
    assert_eq!(shelved.status, StatusCode::NO_CONTENT, "{}", shelved.text());

    let p = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{}/cards", deck.0), Some(&bearer)),
    )
    .await
    .json();
    let fronts: Vec<&str> = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["front"].as_str().unwrap())
        .collect();
    assert_eq!(fronts, vec!["shelved", "kept"], "{p}");
    assert_eq!(p["total"], 2);
}

#[tokio::test]
async fn editor_creates_seeds_and_saves_notes() {
    let t = AppBuilder::new("api-editor").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let deck = t
        .services
        .create_deck(user, "Anatomy", "", now_ms())
        .unwrap();

    let meta = send(&t.app, json_get("/api/v1/editor/meta", Some(&bearer)))
        .await
        .json();
    assert_eq!(meta["note_types"].as_array().unwrap().len(), 4);
    assert_eq!(meta["hues"][0], "red");

    let created = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/notes", deck.0),
            Some(&bearer),
            &json!({
                "note_type": "cloze",
                "front_html": "The {{c1::heart}} pumps {{c2::blood}}",
                "back_html": "<b>Extra</b> notes",
                "tags": ["anatomy"]
            }),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let ids = created.json()["card_ids"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 2, "one card per cloze index");
    let card_id = ids[0].as_i64().unwrap();

    let card = send(
        &t.app,
        json_get(&format!("/api/v1/cards/{card_id}"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(card["kind"], "cloze 1");
    assert!(card["front_html"].as_str().unwrap().contains("cloze-blank"));

    let seed = send(
        &t.app,
        json_get(&format!("/api/v1/cards/{card_id}/editor"), Some(&bearer)),
    )
    .await;
    assert_eq!(seed.status, StatusCode::OK);
    let seed = seed.json();
    assert_eq!(seed["note_type"], "cloze");
    assert_eq!(seed["sibling_count"], 2);
    assert_eq!(seed["tags"], json!(["anatomy"]));

    let saved = send(
        &t.app,
        json_put(
            &format!("/api/v1/cards/{card_id}/editor"),
            Some(&bearer),
            &json!({
                "note_type": "cloze",
                "front_html": "The {{c1::heart}} pumps <script>x</script>{{c2::blood}}",
                "back_html": "",
                "tags": []
            }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let s = saved.json();
    assert_eq!(s["card_ids"].as_array().unwrap().len(), 2);
    assert!(
        !s["front_html"].as_str().unwrap().contains("script"),
        "sanitized on the way in"
    );

    let bad = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/notes", deck.0),
            Some(&bearer),
            &json!({ "note_type": "haiku", "front_html": "x", "back_html": "y" }),
        ),
    )
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let nocloze = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/notes", deck.0),
            Some(&bearer),
            &json!({ "note_type": "cloze", "front_html": "no deletions here", "back_html": "" }),
        ),
    )
    .await;
    assert_eq!(nocloze.status, StatusCode::BAD_REQUEST);
    assert_eq!(nocloze.json()["error"]["code"], "invalid");
}

#[tokio::test]
async fn the_editor_keeps_the_language_picked_for_each_field() {
    let t = AppBuilder::new("api-editor-langs").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let deck = t
        .services
        .create_deck(user, "Japanese", "", now_ms())
        .unwrap();
    let created = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/notes", deck.0),
            Some(&bearer),
            &json!({
                "note_type": "basic",
                "front_html": "cat",
                "back_html": "ねこ",
                "langs": { "front": "en", "back": "ja" }
            }),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let card_id = created.json()["card_ids"][0].as_i64().unwrap();
    let seed = || {
        let t = &t;
        let bearer = &bearer;
        async move {
            send(
                &t.app,
                json_get(&format!("/api/v1/cards/{card_id}/editor"), Some(bearer)),
            )
            .await
            .json()["langs"]
                .clone()
        }
    };
    assert_eq!(seed().await, json!({ "front": "en", "back": "ja" }));
    let save = |body: serde_json::Value| {
        let t = &t;
        let bearer = &bearer;
        async move {
            send(
                &t.app,
                json_put(
                    &format!("/api/v1/cards/{card_id}/editor"),
                    Some(bearer),
                    &body,
                ),
            )
            .await
        }
    };
    let plain =
        save(json!({ "note_type": "basic", "front_html": "cat!", "back_html": "ねこ" })).await;
    assert_eq!(plain.status, StatusCode::OK, "{}", plain.text());
    assert_eq!(seed().await, json!({ "front": null, "back": "ja" }));
    let changed = save(json!({
        "note_type": "basic", "front_html": "cat!", "back_html": "ねこ",
        "langs": { "front": null, "back": "ja-JP" }
    }))
    .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert_eq!(seed().await, json!({ "front": null, "back": "ja-JP" }));
    let bad = save(json!({
        "note_type": "basic", "front_html": "cat!", "back_html": "ねこ",
        "langs": { "front": "klingon" }
    }))
    .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.text());
}

#[tokio::test]
async fn a_goal_date_is_set_shown_and_cleared() {
    let t = AppBuilder::new("api-goal").build();
    let (user, bearer) = signed_up(&t, "flashtester");
    let made = send(
        &t.app,
        json_post("/api/v1/decks", Some(&bearer), &json!({ "name": "Boards" })),
    )
    .await
    .json();
    let id = made["id"].as_i64().unwrap();
    let today = t.store.get_settings(user).unwrap().study_date(now_ms());
    let goal = today.checked_add(jiff::Span::new().days(20)).unwrap();
    let put = |body: serde_json::Value| {
        let (app, bearer) = (t.app.clone(), bearer.clone());
        async move {
            send(
                &app,
                json_put(&format!("/api/v1/decks/{id}/goal"), Some(&bearer), &body),
            )
            .await
        }
    };
    assert_eq!(
        put(json!({ "goal_date": goal.to_string() })).await.status,
        StatusCode::NO_CONTENT
    );
    let d = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await
    .json();
    assert_eq!(d["deck"]["goal"]["date"], goal.to_string());
    assert_eq!(d["deck"]["goal"]["days_left"], 20);
    for bad in [
        json!({ "goal_date": "soon" }),
        json!({ "goal_date": today.to_string() }),
    ] {
        let reply = put(bad).await;
        assert!(reply.status.is_client_error(), "{}", reply.text());
    }
    assert_eq!(
        put(json!({ "goal_date": null })).await.status,
        StatusCode::NO_CONTENT
    );
    let d = send(
        &t.app,
        json_get(&format!("/api/v1/decks/{id}"), Some(&bearer)),
    )
    .await
    .json();
    assert!(d["deck"]["goal"].is_null());
}
