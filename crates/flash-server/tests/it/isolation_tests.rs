//! Cross-user isolation, exhaustively: every `Services` method that takes
//! a resource id is called by one user with another user's ids and must
//! refuse, leaving the owner's rows untouched. A source scan over
//! `service.rs` lists every such method and fails when one is missing
//! from the coverage list below, so a new method cannot skip the check.
//! The same is then confirmed at the HTTP surface for the API.

use std::path::Path;

use axum::http::StatusCode;
use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, Rating, SessionId};
use flash_scan::{compact, crate_at, parse_source, FnKind, Vis};
use flash_server::api::MOBILE_CLIENT_ID;
use flash_server::auth::hash_token;
use flash_server::service::{now_ms, CardFront, NoteInput, ReviewOrigin, Services};
use flash_server::testing::{
    api_bearer, json_delete, json_get, json_post, member, send, AppBuilder,
};
use flash_store::notes::NoteType;
use flash_store::RevisionSource;

/// Every `Services` method with a resource-id parameter, each exercised
/// below as the wrong user.
const COVERED: &[&str] = &[
    "deck_limits",
    "set_deck_limits",
    "set_deck_goal",
    "goal_preview",
    "goal_window",
    "session_goal",
    "set_deck_speech_hints",
    "declare_deck_languages",
    "rename_deck",
    "rename_deck_leaf",
    "move_deck",
    "create_subdeck",
    "archive_deck",
    "unarchive_deck",
    "boost_new_today",
    "boost_today",
    "start_session",
    "start_session_from",
    "submit_review",
    "submit_review_if_queued",
    "session_head",
    "study_front",
    "save_panel_edit",
    "reveal",
    "study_card",
    "reveal_card",
    "session_scope",
    "list_tags",
    "end_session",
    "queue_counts",
    "next_due",
    "week_ahead",
    "preview_cards",
    "preview_outlook",
    "finished_session",
    "grade_preview",
    "deck_by_id",
    "deck_detail",
    "create_cards_in_deck",
    "deck_cards_filtered",
    "deck_card_ids",
    "deck_card_counts",
    "archive_cards",
    "restore_archived",
    "delete_cards",
    "move_cards",
    "return_cards",
    "tag_cards",
    "restore_tag_links",
    "export_deck_cards",
    "set_card_archived",
    "card",
    "import_cards",
    "update_card",
    "update_mcp_sources",
    "delete_card",
    "create_note",
    "save_card_editor",
    "card_revisions",
    "restore_card_revision",
    "editor_seed",
    "delete_deck",
    "list_cards",
    "create_note_from",
];

const ID_TYPES: &[&str] = &[
    "DeckId",
    "CardId",
    "McpSourcePatch",
    "NoteId",
    "MediaId",
    "SessionId",
    "StudyScope",
    "RevisionId",
];

/// The public `Services` methods, in any file of the crate, whose
/// parameters name a resource id (a returned id is not a resource the
/// caller chose: `create_deck` returns one).
fn methods_with_resource_ids() -> Vec<String> {
    let mut names = Vec::new();
    for file in crate_at(Path::new(env!("CARGO_MANIFEST_DIR"))).source_files() {
        for function in &file.functions {
            let on_services = matches!(&function.kind, FnKind::Method { owner, trait_: None } if owner == "Services");
            let takes_id = function
                .params
                .iter()
                .any(|p| p.ty.idents.iter().any(|i| ID_TYPES.contains(&i.as_str())));
            if on_services && function.vis == Vis::Pub && takes_id {
                names.push(function.name.clone());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_service_method_with_a_resource_id_is_covered() {
    // One scan of the crate's source serves both directions of the check.
    let methods = methods_with_resource_ids();
    let missing: Vec<&String> = methods
        .iter()
        .filter(|name| !COVERED.contains(&name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "service methods taking a resource id that the isolation test does not call:\n{}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let stale: Vec<&&str> = COVERED
        .iter()
        .filter(|name| !methods.iter().any(|m| m == *name))
        .collect();
    assert!(
        stale.is_empty(),
        "covered names that are no longer methods: {stale:?}"
    );
    // A name on the list is a claim that the test below calls it as the
    // wrong user; the claim is checked against this file's own source,
    // so listing a method is not the same as covering it.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/isolation_tests.rs");
    let this_file = parse_source(
        "isolation_tests.rs",
        &std::fs::read_to_string(path).expect("this test's source"),
    );
    // A call sits either in parsed code or inside an `assert!`, whose
    // body is tokens; both are read.
    let called_with_b = |name: &str| {
        this_file
            .method_calls
            .iter()
            .any(|m| m.method == name && m.args.first().map(String::as_str) == Some("b"))
            || this_file
                .macros
                .iter()
                .any(|m| compact(&m.tokens).contains(&format!(".{name}(b,")))
    };
    let unexercised: Vec<&&str> = COVERED.iter().filter(|name| !called_with_b(name)).collect();
    assert!(
        unexercised.is_empty(),
        "covered names never called with the wrong user first (`s.<name>(b, …)`): {unexercised:?}"
    );
}

struct Owned {
    deck: DeckId,
    card: CardId,
    session: SessionId,
}

fn seed(services: &Services, owner: flash_core::UserId) -> Owned {
    let now = now_ms();
    let deck = services.create_deck(owner, "Owned", "", now).unwrap();
    let cards = services
        .create_cards_in_deck(
            owner,
            deck,
            &[("front".into(), "back".into(), vec!["tag".into()])],
            now,
        )
        .unwrap();
    let session = services
        .start_session(owner, StudyScope::deck(deck), now)
        .unwrap()
        .session_id;
    Owned {
        deck,
        card: cards[0],
        session,
    }
}

fn note_input() -> NoteInput {
    NoteInput {
        note_type: NoteType::Basic,
        front_html: "<p>x</p>".into(),
        back_html: "<p>y</p>".into(),
        tags: vec![],
        speech: Default::default(),
        langs: None,
    }
}

#[tokio::test]
async fn another_user_cannot_touch_owned_resources_through_services() {
    let t = AppBuilder::new("isolation-services").build();
    let a = member(&t.store, "FlashTester", "flashtester@example.com");
    let b = member(&t.store, "Other", "other@example.com");
    let s = &t.services;
    let now = now_ms();
    let owned = seed(s, a);
    let in_a_month = t
        .store
        .get_settings(a)
        .unwrap()
        .study_date(now)
        .checked_add(jiff::Span::new().days(30))
        .unwrap();
    s.set_deck_goal(a, owned.deck, Some(in_a_month), now)
        .unwrap();
    // An edit by A, so A's card has an earlier version to guard.
    s.save_card_editor(a, owned.card, &note_input(), RevisionSource::Web, now)
        .unwrap();
    let kept = s.card_revisions(a, owned.card).unwrap()[0].id;
    let before_cards = s.list_cards(a, Some(owned.deck), None, 500).unwrap();
    let before_decks = s.list_decks(a, now).unwrap();
    let before_state = t.store.get_card_state(a, owned.card).unwrap();

    // Decks.
    assert!(s.deck_limits(b, owned.deck).is_err());
    assert!(s.set_deck_limits(b, owned.deck, Some(1), Some(1)).is_err());
    let settings = s.store().get_settings(b).unwrap();
    assert!(s
        .set_deck_goal(b, owned.deck, Some(in_a_month), now)
        .is_err());
    assert!(s.set_deck_goal(b, owned.deck, None, now).is_err());
    assert!(s.goal_preview(b, owned.deck, in_a_month, now).is_err());
    assert!(matches!(
        s.goal_window(b, &settings, owned.deck, now),
        Ok(None)
    ));
    assert!(matches!(
        s.session_goal(b, &StudyScope::deck(owned.deck), now),
        Ok(None)
    ));
    assert!(s
        .set_deck_speech_hints(b, owned.deck, &["fr".to_string()])
        .is_err());
    assert!(s
        .declare_deck_languages(b, owned.deck, &["fr".to_string()])
        .is_err());
    assert!(s.rename_deck(b, owned.deck, "stolen").is_err());
    assert!(s.rename_deck_leaf(b, owned.deck, "stolen").is_err());
    // A deck is no more a place to put things than a thing to take: not
    // moved, not moved into, and no subdeck made inside it.
    let theirs = s.create_deck(b, "Theirs", "", now).unwrap();
    assert!(s.move_deck(b, owned.deck, None).is_err());
    assert!(s.move_deck(b, owned.deck, Some(theirs)).is_err());
    assert!(s.move_deck(b, theirs, Some(owned.deck)).is_err());
    assert!(s.create_subdeck(b, owned.deck, "inside", "", now).is_err());
    let names: Vec<String> = s
        .list_decks(b, now)
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, vec!["Theirs"], "nothing moved or was made");
    assert!(s.archive_deck(b, owned.deck, now).is_err());
    assert!(s.unarchive_deck(b, owned.deck).is_err());
    assert!(s.boost_new_today(b, Some(owned.deck), 5, now).is_err());
    assert!(s.boost_today(b, Some(owned.deck), now).is_err());
    assert!(s.deck_by_id(b, owned.deck, now).is_err());
    assert!(s.deck_detail(b, owned.deck, now).is_err());
    assert!(s
        .create_cards_in_deck(b, owned.deck, &[("f".into(), "b".into(), vec![])], now)
        .is_err());
    assert!(s.create_note(b, owned.deck, &note_input(), now).is_err());
    assert!(s.delete_deck(b, owned.deck).is_err());
    // Listing and counting under another user's deck yields nothing.
    assert!(s
        .list_cards(b, Some(owned.deck), None, 50)
        .unwrap()
        .is_empty());
    assert!(
        s.list_tags(b, Some(owned.deck)).is_err(),
        "another user's deck has no tags to offer"
    );
    let counts = s
        .queue_counts(b, &StudyScope::deck(owned.deck), now)
        .unwrap();
    assert_eq!((counts.due, counts.new_available), (0, 0));
    assert!(
        !matches!(
            s.next_due(b, &StudyScope::deck(owned.deck), 0, now),
            Ok(Some(_))
        ),
        "another user's deck has nothing coming"
    );
    assert!(
        s.week_ahead(b, &StudyScope::deck(owned.deck), 0, now)
            .unwrap()
            .iter()
            .all(|day| day.cards == 0),
        "nor any week ahead"
    );
    assert!(
        s.preview_cards(b, owned.deck, now).is_err(),
        "another user's deck has nothing to preview"
    );
    assert!(
        matches!(s.preview_outlook(b, owned.deck, now), Ok((0, None))),
        "nor anything left to study after one"
    );
    assert!(s
        .start_session(b, StudyScope::deck(owned.deck), now)
        .is_err());
    assert!(s
        .start_session_from(
            b,
            StudyScope::deck(owned.deck),
            flash_server::service::SessionSurface::Web,
            now
        )
        .is_err());

    // Cards.
    assert!(s.card(b, owned.card).is_err());
    assert!(s.grade_preview(b, owned.card, now).is_err());
    assert!(s.reveal(b, owned.card).is_err());
    assert!(s.reveal_card(b, owned.card, None).is_err());
    assert!(s.study_front(b, owned.card).unwrap().is_none());
    assert!(s
        .save_panel_edit(b, owned.card, "theirs".into(), "now".into(), now)
        .is_err());
    assert!(s
        .study_card(
            b,
            CardFront {
                card_id: owned.card,
                front: String::new(),
            },
            Some(owned.card)
        )
        .is_err());
    assert!(s.update_card(b, owned.card, "x", "y", now).is_err());
    assert!(s
        .save_card_editor(b, owned.card, &note_input(), RevisionSource::Web, now)
        .is_err());
    assert!(s.editor_seed(b, owned.card).is_err());
    // B can neither list A's earlier version nor put it back, by A's
    // card or through one of B's own.
    assert!(s.card_revisions(b, owned.card).is_err());
    assert!(s.restore_card_revision(b, owned.card, kept, now).is_err());
    let mine = s
        .create_cards_in_deck(b, theirs, &[("f".into(), "b".into(), vec![])], now)
        .unwrap()[0];
    assert!(s.restore_card_revision(b, mine, kept, now).is_err());
    assert_eq!(s.card_revisions(a, owned.card).unwrap().len(), 1);
    assert!(s.set_card_archived(b, owned.card, true, now).is_err());
    assert!(!s.card(a, owned.card).unwrap().suspended);
    let before = s.card(a, owned.card).unwrap();
    let filter = flash_server::deck_cards::DeckFilter::default();
    assert!(s
        .deck_cards_filtered(b, owned.deck, &filter, 1, 25)
        .is_err());
    assert!(s.deck_card_ids(b, owned.deck, &filter).is_err());
    assert!(s.deck_card_counts(b, owned.deck).is_err());
    assert!(s
        .archive_cards(b, owned.deck, &[owned.card], true, now)
        .is_err());
    assert!(s
        .archive_cards(b, theirs, &[owned.card], true, now)
        .is_err());
    assert!(s.restore_archived(b, &[owned.card], true, now).is_err());
    assert!(s
        .move_cards(b, owned.deck, &[owned.card], theirs, now)
        .is_err());
    assert!(s.move_cards(b, theirs, &[owned.card], theirs, now).is_err());
    assert!(s.return_cards(b, &[(owned.card, theirs)], now).is_err());
    assert!(s
        .tag_cards(b, theirs, &[owned.card], &["x".into()], true, now)
        .is_err());
    assert!(s
        .restore_tag_links(b, &[(owned.card, "tag".into())], false, now)
        .is_err());
    assert!(s.export_deck_cards(b, owned.deck).is_err());
    assert!(s.delete_cards(b, theirs, &[owned.card], now).is_err());
    assert_eq!(s.card(a, owned.card).unwrap(), before);
    assert!(s.delete_card(b, owned.card, now).is_err());
    assert!(s
        .update_mcp_sources(
            b,
            &[flash_server::service::McpSourcePatch {
                card_id: owned.card,
                front_html: None,
                back_html: Some("foreign rewrite".into()),
                tags: None,
                sibling_card_ids: vec![],
                speech: Default::default(),
            }],
            now
        )
        .is_err());

    // Sessions and reviews.
    assert!(s.session_scope(b, owned.session).is_err());
    assert!(s.finished_session(b, owned.session, 0, now).is_err());
    assert!(s
        .submit_review(
            b,
            owned.session,
            owned.card,
            Rating::Good,
            ReviewOrigin::WEB,
            now
        )
        .is_err());
    assert!(s
        .submit_review_if_queued(
            b,
            owned.session,
            owned.card,
            Rating::Good,
            ReviewOrigin::WEB,
            now
        )
        .is_err());
    assert!(s.session_head(b, owned.session, now).is_err());
    assert!(s.end_session(b, owned.session, now).is_err());

    // Nor may B make a note in A's deck.
    assert!(s
        .create_note_from(b, owned.deck, &note_input(), None, now)
        .is_err());

    // Import with a foreign media id: the id must not link.
    let foreign_media = t
        .store
        .create_media(
            a,
            "0000000000000000000000000000000000000000000000000000000000000000",
            "pic.png",
            "image/png",
            flash_store::media::MediaKind::Image,
            4,
            now,
        )
        .unwrap();
    let media_map = [("pic.png".to_string(), foreign_media)]
        .into_iter()
        .collect();
    let row = flash_store::import::ImportRow {
        anki_card_id: None,
        media: vec![],
        front: "front".into(),
        back: "b".into(),
        front_html: Some(flash_store::richtext::sanitize_with_media(
            r#"<p><img src="pic.png"></p>"#,
        )),
        back_html: None,
        tags: vec![],
        deck: None,
        suspended: false,
        cloze_text: None,
        cloze_index: None,
        type_answer: None,
    };
    let _ = s.import_cards(b, "Imported", vec![row], vec![], Some(&media_map), now);
    let linked: i64 = t
        .store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM card_media cm JOIN cards c ON c.id = cm.card_id
                 WHERE c.user_id = ?1 AND cm.media_id = ?2",
                rusqlite::params![b.raw(), foreign_media.0],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        linked, 0,
        "another user's media must not link into B's cards"
    );

    // Nothing of A's moved.
    assert_eq!(
        s.list_cards(a, Some(owned.deck), None, 500).unwrap(),
        before_cards
    );
    assert_eq!(s.list_decks(a, now).unwrap(), before_decks);
    assert_eq!(t.store.get_card_state(a, owned.card).unwrap(), before_state);
}

#[tokio::test]
async fn another_users_bearer_is_turned_away_by_the_api() {
    let t = AppBuilder::new("isolation-api").build();
    let a = member(&t.store, "FlashTester", "flashtester@example.com");
    let b = member(&t.store, "Other", "other@example.com");
    let owned = seed(&t.services, a);
    let a_bearer = api_bearer(&t.store, a);
    let b_bearer = api_bearer(&t.store, b);

    for path in [
        format!("/api/v1/decks/{}", owned.deck.0),
        format!("/api/v1/decks/{}/cards", owned.deck.0),
        format!("/api/v1/cards/{}", owned.card.0),
        format!("/api/v1/cards/{}/editor", owned.card.0),
        format!("/api/v1/tags?deck={}", owned.deck.0),
    ] {
        let reply = send(&t.app, json_get(&path, Some(&b_bearer))).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
    }
    let review = send(
        &t.app,
        json_post(
            &format!("/api/v1/study/sessions/{}/reviews", owned.session.0),
            Some(&b_bearer),
            &serde_json::json!({"card_id": owned.card.0, "rating": 3}),
        ),
    )
    .await;
    assert!(review.status.is_client_error(), "{}", review.status);
    let deleted = send(
        &t.app,
        json_delete(&format!("/api/v1/cards/{}", owned.card.0), Some(&b_bearer)),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND);
    assert!(t.services.card(a, owned.card).is_ok());
    // Nor archive A's deck: it stays in A's study queue.
    let archived = send(
        &t.app,
        json_post(
            &format!("/api/v1/decks/{}/archive", owned.deck.0),
            Some(&b_bearer),
            &serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(archived.status, StatusCode::NOT_FOUND);
    assert_eq!(
        t.services
            .deck_by_id(a, owned.deck, now_ms())
            .unwrap()
            .archived_at,
        None
    );

    // B cannot revoke A's app session by id; A's bearer keeps working.
    let a_token = t
        .store
        .list_tokens_for_user(a, MOBILE_CLIENT_ID, now_ms())
        .unwrap()[0]
        .id;
    let revoke = send(
        &t.app,
        json_delete(&format!("/api/v1/auth/sessions/{a_token}"), Some(&b_bearer)),
    )
    .await;
    assert_eq!(revoke.status, StatusCode::NOT_FOUND);
    assert!(t
        .store
        .lookup_api_token(&hash_token(&a_bearer), now_ms())
        .unwrap()
        .is_some());
}
