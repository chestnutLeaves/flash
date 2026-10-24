//! End-to-end MCP tests over the real Streamable HTTP endpoint: initialize,
//! tool discovery, and a complete study loop, authenticated with a real
//! bearer token.

use crate::common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_server::service::now_ms;
use tower::ServiceExt;

pub(crate) struct Harness {
    pub(crate) app: axum::Router,
    pub(crate) bearer: String,
    pub(crate) store: std::sync::Arc<flash_store::Store>,
    pub(crate) services: flash_server::service::Services,
    pub(crate) data_dir: std::path::PathBuf,
    pub(crate) user: flash_core::UserId,
}

pub(crate) fn harness() -> Harness {
    harness_named("mcp")
}

/// A harness with its own data directory, for a test that writes media
/// files (tests sharing a directory race to create it).
pub(crate) fn harness_named(tag: &str) -> Harness {
    harness_built(AppBuilder::new(tag))
}

/// A harness over the server `builder` makes (with an extension plugged
/// in, say).
pub(crate) fn harness_built(builder: AppBuilder) -> Harness {
    let t = builder.build();
    let user = t
        .store
        .create_user("Tester", None, "member", now_ms())
        .unwrap();
    // The bearer helper mints tokens for "test-client"; register it under
    // the name a real MCP client would carry.
    t.store
        .create_oauth_client("test-client", "Claude", &[], "{}", now_ms())
        .unwrap();
    Harness {
        bearer: oauth_bearer(&t.store, user),
        store: t.store.clone(),
        services: t.services,
        data_dir: t.data_dir,
        app: t.app,
        user,
    }
}

async fn tool_error(h: &Harness, session: &str, name: &str, args: serde_json::Value) -> String {
    let (_,_,body)=rpc(h,Some(session),serde_json::json!({"jsonrpc":"2.0","id":990,"method":"tools/call","params":{"name":name,"arguments":args}})).await;
    let result = result_of(&body);
    assert!(
        result["isError"] == true || result.is_null(),
        "expected failure: {body}"
    );
    body
}

#[tokio::test]
async fn rich_batch_edits_preserve_media_siblings_and_history_and_rollback() {
    let h = harness();
    let session = initialize(&h).await;
    let media = h
        .store
        .create_media(
            h.user,
            "image-hash",
            "image.png",
            "image/png",
            flash_store::media::MediaKind::Image,
            10,
            now_ms(),
        )
        .unwrap();
    let created=call_tool(&h,&session,800,"create_cards",serde_json::json!({"deck":"Rich","cards":[
        {"front_html":format!("<b>Question</b><img src=\"/media/{}\">",media.0),"back_html":"<i>Answer</i>","tags":["keep"]},
        {"note_type":"cloze","front_html":"<b>{{c1::Paris}}</b> and {{c2::Rome}}","back_html":"<i>Cities</i>"},
        {"note_type":"basic_reversed","front_html":"<b>Left</b>","back_html":"<i>Right</i>"},
        {"note_type":"basic_typed","front_html":"Type this","back_html":"Answer"}
    ]})).await;
    assert_eq!(created["created"], 6);
    let first = created["card_ids"][0].as_i64().unwrap();
    let cloze = created["card_ids"][1].as_i64().unwrap();
    let reversed = created["card_ids"][3].as_i64().unwrap();
    let typed = created["card_ids"][5].as_i64().unwrap();
    let listed = call_tool(
        &h,
        &session,
        801,
        "list_cards",
        serde_json::json!({"deck":"Rich"}),
    )
    .await;
    assert_eq!(listed["total_count"], 6);
    let card = listed["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["card_id"] == first)
        .unwrap();
    let original_front = card["front_html"].clone();
    assert!(original_front.as_str().unwrap().contains("<b>"));
    assert!(card["media"][0]["url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/mcp-media/{}", media.0)));
    let services = flash_server::service::Services::new(h.store.clone());
    let started = services
        .start_session(h.user, flash_core::queue::StudyScope::all(), now_ms())
        .unwrap();
    services
        .submit_review(
            h.user,
            started.session_id,
            flash_core::CardId(first),
            flash_core::Rating::Good,
            flash_server::service::ReviewOrigin::new("mcp", None),
            now_ms(),
        )
        .unwrap();
    let before = h
        .store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM review_log WHERE card_id=?1",
                [first],
                |r| r.get::<_, u32>(0),
            )?)
        })
        .unwrap();
    let updated=call_tool(&h,&session,802,"update_cards",serde_json::json!({"cards":[{"card_id":first,"back_html":"<u>New answer</u>"},{"card_id":typed,"front_html":"Type the answer"}]})).await;
    assert_eq!(updated["updated"], 2);
    assert_eq!(updated["created"], 0);
    let row = h
        .store
        .get_card(h.user, flash_core::CardId(first))
        .unwrap()
        .unwrap();
    assert_eq!(row.front_html.as_deref(), original_front.as_str());
    assert_eq!(row.tags, vec!["keep"]);
    assert_eq!(
        h.store
            .get_card(h.user, flash_core::CardId(typed))
            .unwrap()
            .unwrap()
            .type_answer
            .as_deref(),
        Some("Answer")
    );
    assert_eq!(
        h.store
            .with_conn(|conn| Ok(conn.query_row(
                "SELECT COUNT(*) FROM review_log WHERE card_id=?1",
                [first],
                |r| r.get::<_, u32>(0)
            )?))
            .unwrap(),
        before
    );
    let cloze_siblings = serde_json::json!([cloze, created["card_ids"][2]]);
    tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":cloze,"back_html":"Changed"}]}),
    )
    .await;
    let expanded=call_tool(&h,&session,803,"update_cards",serde_json::json!({"cards":[{"card_id":cloze,"sibling_card_ids":cloze_siblings,"front_html":"<b>{{c1::Paris}}</b> and {{c2::Rome}} and {{c3::Oslo}}"}]})).await;
    assert_eq!(expanded["created"], 1);
    assert_eq!(expanded["updated"], 2);
    let rev_siblings = serde_json::json!([reversed, created["card_ids"][4]]);
    let rev=call_tool(&h,&session,804,"update_cards",serde_json::json!({"cards":[{"card_id":reversed,"sibling_card_ids":rev_siblings,"front_html":"<b>Updated left</b>"}]})).await;
    assert_eq!(rev["updated"], 2);
    // A later update would remove a cloze slot: the earlier valid edit rolls back too.
    let error=tool_error(&h,&session,"update_cards",serde_json::json!({"cards":[
        {"card_id":first,"back_html":"MUST ROLL BACK"},
        {"card_id":cloze,"sibling_card_ids":expanded["card_ids"],"front_html":"{{c1::Paris}} only"}
    ]})).await;
    assert!(error.contains("implicit_card_removal"));
    assert_eq!(
        h.store
            .get_card(h.user, flash_core::CardId(first))
            .unwrap()
            .unwrap(),
        row
    );
    // Invalid media anywhere also rejects the whole creation, including a new deck.
    tool_error(&h,&session,"create_cards",serde_json::json!({"deck":"Must not exist","cards":[{"front_html":"ok","back_html":"ok"},{"front_html":"<img src=\"/media/999999\">","back_html":"bad"}]})).await;
    assert!(h
        .store
        .find_deck_by_name(h.user, "Must not exist")
        .unwrap()
        .is_none());
}

/// A wrong `sibling_card_ids` is refused with the ids the source has, this
/// card included, and the entry it was in, so the assistant can send them
/// at once instead of reading the cards again and again.
#[tokio::test]
async fn a_wrong_sibling_list_is_refused_with_the_ids_to_send() {
    let h = harness();
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        830,
        "create_cards",
        serde_json::json!({"deck":"Pairs","cards":[
            {"front_html":"Alone","back_html":"Single"},
            {"note_type":"basic_reversed","front_html":"Left","back_html":"Right"}
        ]}),
    )
    .await;
    let single = created["card_ids"][0].as_i64().unwrap();
    let (left, right) = (
        created["card_ids"][1].as_i64().unwrap(),
        created["card_ids"][2].as_i64().unwrap(),
    );
    let pair = format!("[{left}, {right}]");

    for sent in [serde_json::json!(null), serde_json::json!([right])] {
        let mut patch = serde_json::json!({"card_id":left,"back_html":"Changed"});
        if !sent.is_null() {
            patch["sibling_card_ids"] = sent;
        }
        let error = tool_error(
            &h,
            &session,
            "update_cards",
            serde_json::json!({"cards":[patch]}),
        )
        .await;
        assert!(error.contains(&format!("card {left} ")), "{error}");
        assert!(
            error.contains(&format!(
                "send sibling_card_ids {pair} (this card included)"
            )),
            "{error}"
        );
    }

    // The entry that failed is named, and the batch's other edit is undone.
    let error = tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":single,"back_html":"Must not stick"},
            {"card_id":right,"sibling_card_ids":[right],"back_html":"Changed"}
        ]}),
    )
    .await;
    assert!(error.contains(&format!("card {right} ")), "{error}");
    assert!(error.contains(&pair), "{error}");
    let single_row = h
        .store
        .get_card(h.user, flash_core::CardId(single))
        .unwrap()
        .unwrap();
    assert_eq!(single_row.back, "Single");

    let error = tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":single,"sibling_card_ids":[single, left],"back_html":"Changed"}
        ]}),
    )
    .await;
    assert!(
        error.contains(&format!(
            "card {single} has no other cards: omit sibling_card_ids or send [{single}]"
        )),
        "{error}"
    );

    // Both cards of the pair as two edits name both.
    let error = tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":left,"sibling_card_ids":[left, right],"back_html":"One"},
            {"card_id":right,"sibling_card_ids":[left, right],"front_html":"Two"}
        ]}),
    )
    .await;
    assert!(
        error.contains(&format!("cards {left} and {right} share one source")),
        "{error}"
    );

    // Sent as the error says, the edit goes through.
    let updated = call_tool(
        &h,
        &session,
        831,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":right,"sibling_card_ids":[left, right],"back_html":"Changed"},
            {"card_id":single,"back_html":"Edited"}
        ]}),
    )
    .await;
    assert_eq!(updated["updated"], 3);
}

#[tokio::test]
async fn update_cards_keeps_the_old_text_and_restore_cards_puts_it_back() {
    let h = harness();
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        820,
        "create_cards",
        serde_json::json!({"deck":"Undo","cards":[{"front_html":"Capital of Peru?","back_html":"Lima","tags":["geo"]}]}),
    )
    .await;
    let card = created["card_ids"][0].as_i64().unwrap();
    let source = created["source_ids"][0].as_i64().unwrap();
    // A note-less card, as the quick-add form and older clients made them.
    let deck = h.store.find_deck_by_name(h.user, "Undo").unwrap().unwrap();
    let plain = h
        .services
        .create_cards_in_deck(
            h.user,
            deck,
            &[("Two & two?".into(), "Four".into(), vec!["math".into()])],
            now_ms(),
        )
        .unwrap()[0]
        .0;
    let services = flash_server::service::Services::new(h.store.clone());
    let started = services
        .start_session(h.user, flash_core::queue::StudyScope::all(), now_ms())
        .unwrap();
    services
        .submit_review(
            h.user,
            started.session_id,
            flash_core::CardId(card),
            flash_core::Rating::Good,
            flash_server::service::ReviewOrigin::new("mcp", None),
            now_ms(),
        )
        .unwrap();
    let state = h
        .store
        .get_card_state(h.user, flash_core::CardId(card))
        .unwrap();

    let updated = call_tool(
        &h,
        &session,
        821,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":card,"back_html":"Cusco","tags":["wrong"]},
            {"card_id":plain,"front_html":"<b>Two plus two?</b>"}
        ]}),
    )
    .await;
    assert_eq!(updated["updated"], 2);
    let revisions = updated["revision_ids"].as_array().unwrap();
    assert_eq!(revisions.len(), 2, "one per source: {updated}");
    assert!(revisions.iter().all(|r| r.is_i64()), "{updated}");
    // Saving what the card already says keeps nothing.
    let again = call_tool(
        &h,
        &session,
        822,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":card,"back_html":"Cusco"}]}),
    )
    .await;
    assert_eq!(again["revision_ids"], serde_json::json!([null]));

    // The note-less card's first edit kept its own text.
    let listed = call_tool(
        &h,
        &session,
        823,
        "restore_cards",
        serde_json::json!({"card_id":plain}),
    )
    .await;
    let versions = listed["revisions"].as_array().unwrap();
    assert_eq!(versions.len(), 1, "{listed}");
    assert_eq!(versions[0]["front_preview"], "Two & two?");
    assert_eq!(versions[0]["back_preview"], "Four");
    assert_eq!(versions[0]["tags"], serde_json::json!(["math"]));
    assert_eq!(versions[0]["replaced_by"], "mcp");
    assert!(listed.get("card_ids").is_none(), "listing restores nothing");

    // Listing the edited card, then restoring what update_cards kept.
    let listed = call_tool(
        &h,
        &session,
        824,
        "restore_cards",
        serde_json::json!({"card_id":card}),
    )
    .await;
    let versions = listed["revisions"].as_array().unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["revision_id"], revisions[0]);
    assert_eq!(versions[0]["back_preview"], "Lima");
    let restored = call_tool(
        &h,
        &session,
        825,
        "restore_cards",
        serde_json::json!({"card_id":card,"revision_id":revisions[0]}),
    )
    .await;
    assert_eq!(restored["card_ids"], serde_json::json!([card]));
    assert_eq!(restored["source_ids"], serde_json::json!([source]));
    assert_eq!(
        (restored["updated"].as_i64(), restored["created"].as_i64()),
        (Some(1), Some(0))
    );
    assert!(restored["revision_ids"][0].is_i64(), "{restored}");
    assert!(restored["revisions"].as_array().unwrap().is_empty());
    let row = h
        .store
        .get_card(h.user, flash_core::CardId(card))
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.back.as_str(), row.tags.clone()),
        ("Lima", vec!["geo".to_string()])
    );
    assert_eq!(
        h.store
            .get_card_state(h.user, flash_core::CardId(card))
            .unwrap(),
        state,
        "the schedule is untouched"
    );
    let reviews = h
        .store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM review_log WHERE card_id = ?1",
                [card],
                |r| r.get::<_, u32>(0),
            )?)
        })
        .unwrap();
    assert_eq!(reviews, 1, "the review history is untouched");
    // The restore is undoable too: the wrong text is now a version.
    let listed = call_tool(
        &h,
        &session,
        826,
        "restore_cards",
        serde_json::json!({"card_id":card}),
    )
    .await;
    assert_eq!(listed["revisions"][0]["back_preview"], "Cusco");
    assert_eq!(listed["revisions"][0]["replaced_by"], "restore");

    // A version of one card is not another card's to restore.
    let error = tool_error(
        &h,
        &session,
        "restore_cards",
        serde_json::json!({"card_id":plain,"revision_id":revisions[0]}),
    )
    .await;
    assert!(error.contains("not found"), "{error}");
    assert_eq!(
        h.store
            .get_card(h.user, flash_core::CardId(plain))
            .unwrap()
            .unwrap()
            .front,
        "Two plus two?"
    );
}

#[tokio::test]
async fn batch_delete_confirms_across_sessions_and_retries_without_expanding_targets() {
    let h = harness();
    let session = initialize(&h).await;
    let made=call_tool(&h,&session,810,"create_cards",serde_json::json!({"deck":"Delete","cards":[{"front_html":"one","back_html":"a"},{"front_html":"two","back_html":"b"}]})).await;
    let asked = call_tool(
        &h,
        &session,
        811,
        "delete_cards",
        serde_json::json!({"card_ids":made["card_ids"]}),
    )
    .await;
    assert_eq!(asked["pending"], true);
    assert_eq!(asked["cards_affected"], 2);
    assert!(asked["expires_ms"].as_i64().unwrap() > now_ms() + 500_000);
    let second = initialize(&h).await;
    let args =
        serde_json::json!({"card_ids":made["card_ids"],"confirm_token":asked["confirm_token"]});
    let (done, retry) = tokio::join!(
        call_tool(&h, &second, 812, "delete_cards", args.clone()),
        call_tool(&h, &session, 813, "delete_cards", args)
    );
    assert_eq!(done["deleted"], true);
    assert_eq!(done, retry);
    assert_eq!(
        call_tool(
            &h,
            &second,
            814,
            "list_cards",
            serde_json::json!({"deck":"Delete"})
        )
        .await["total_count"],
        0
    );
    let staged = call_tool(
        &h,
        &second,
        815,
        "delete_deck",
        serde_json::json!({"deck":"Delete"}),
    )
    .await;
    call_tool(
        &h,
        &second,
        816,
        "create_cards",
        serde_json::json!({"deck":"Delete","cards":[{"front_html":"new","back_html":"a"}]}),
    )
    .await;
    let error = tool_error(
        &h,
        &second,
        "delete_deck",
        serde_json::json!({"deck":"Delete","confirm_token":staged["confirm_token"]}),
    )
    .await;
    assert!(error.contains("confirmation_target_changed"));
    // The code alone left a model guessing; the message says what happened
    // to this call and what to do next, and the code stays machine-readable.
    assert!(error.contains("This call deleted nothing"), "{error}");
    assert!(
        error.contains(r#""code":"confirmation_target_changed""#),
        "{error}"
    );
    assert!(h
        .store
        .find_deck_by_name(h.user, "Delete")
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn keyset_pagination_reads_every_card_beyond_one_batch() {
    let h = harness();
    let session = initialize(&h).await;
    let cards = (0..500)
        .map(|i| serde_json::json!({"front_html":format!("Q{i}"),"back_html":"A"}))
        .collect::<Vec<_>>();
    call_tool(
        &h,
        &session,
        820,
        "create_cards",
        serde_json::json!({"deck":"Many","cards":cards}),
    )
    .await;
    call_tool(
        &h,
        &session,
        821,
        "create_cards",
        serde_json::json!({"deck":"Many","cards":[{"front_html":"last","back_html":"A"}]}),
    )
    .await;
    let first = call_tool(
        &h,
        &session,
        822,
        "list_cards",
        serde_json::json!({"deck":"Many","limit":500}),
    )
    .await;
    assert_eq!(first["total_count"], 501);
    assert_eq!(first["has_more"], true);
    // Even the smallest cards fill the page's byte budget before 500.
    let mut seen: Vec<i64> = Vec::new();
    let mut page = first;
    for id in 823..840 {
        let rows = page["cards"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.len() <= 500);
        seen.extend(rows.iter().map(|c| c["card_id"].as_i64().unwrap()));
        if page["has_more"] == false {
            assert!(page["next_cursor"].is_null());
            break;
        }
        assert_eq!(page["next_cursor"], *seen.last().unwrap());
        page = call_tool(
            &h,
            &session,
            id,
            "list_cards",
            serde_json::json!({"deck":"Many","limit":500,"before_card_id":page["next_cursor"]}),
        )
        .await;
    }
    assert_eq!(seen.len(), 501);
    assert!(
        seen.windows(2).all(|w| w[0] > w[1]),
        "newest first, no repeats"
    );
}

#[tokio::test]
async fn full_sources_round_trip_without_losing_audio_video_or_images() {
    let h = harness();
    let session = initialize(&h).await;
    let mut refs = Vec::new();
    for (kind, mime, name) in [
        (
            flash_store::media::MediaKind::Image,
            "image/png",
            "image.png",
        ),
        (
            flash_store::media::MediaKind::Audio,
            "audio/mpeg",
            "audio.mp3",
        ),
        (
            flash_store::media::MediaKind::Video,
            "video/mp4",
            "video.mp4",
        ),
    ] {
        let id = h
            .store
            .create_media(h.user, name, name, mime, kind, 12, now_ms())
            .unwrap();
        refs.push(id.0);
    }
    let html=format!("<b>Rich question</b><img src=\"/media/{}\"><audio controls src=\"/media/{}\"></audio><video controls src=\"/media/{}\"></video>",refs[0],refs[1],refs[2]);
    let made=call_tool(&h,&session,850,"create_cards",serde_json::json!({"deck":"Media","cards":[{"front_html":html,"back_html":"<ul><li>One</li><li>Two</li></ul>","tags":["media"]}]})).await;
    let before = call_tool(
        &h,
        &session,
        851,
        "list_cards",
        serde_json::json!({"deck":"Media"}),
    )
    .await;
    let card = &before["cards"][0];
    let source = &card["source"];
    assert_eq!(card["media"].as_array().unwrap().len(), 3);
    let saved=call_tool(&h,&session,852,"update_cards",serde_json::json!({"cards":[{"card_id":made["card_ids"][0],"front_html":source["front_html"],"back_html":source["back_html"],"tags":source["tags"]}]})).await;
    assert_eq!(saved["updated"], 1);
    assert_eq!(saved["created"], 0);
    let after = call_tool(
        &h,
        &session,
        853,
        "list_cards",
        serde_json::json!({"deck":"Media"}),
    )
    .await;
    assert_eq!(before, after);
    let id = made["card_ids"][0].as_i64().unwrap();
    let links = h
        .store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM card_media WHERE card_id=?1",
                [id],
                |r| r.get::<_, u32>(0),
            )?)
        })
        .unwrap();
    assert_eq!(links, 3);
    let started = call_tool(
        &h,
        &session,
        854,
        "start_study_session",
        serde_json::json!({"deck":"Media"}),
    )
    .await;
    assert_eq!(started["first_card"]["media"].as_array().unwrap().len(), 3);
    assert_eq!(started["first_card"]["front_html"], card["front_html"]);
    assert!(started["first_card"].get("source").is_none());
    assert!(started["first_card"].get("back_html").is_none());
}

#[tokio::test]
async fn legacy_imports_are_readable_and_lossy_source_rewrites_are_rejected() {
    let h = harness();
    let session = initialize(&h).await;
    let services = flash_server::service::Services::new(h.store.clone());
    let ids = services
        .create_cards(
            h.user,
            "Imported",
            &[
                ("[...] is a city".into(), "Paris".into(), vec![]),
                ("Rich basic".into(), "Answer".into(), vec![]),
            ],
            now_ms(),
        )
        .unwrap();
    let front = flash_store::richtext::sanitize_with_media(
        "<b><span class=\"cloze-blank\">[...]</span> is a city</b>",
    );
    let back = flash_store::richtext::sanitize_with_media(
        "<b><span class=\"cloze-answer\">Paris</span> is a city</b>",
    );
    h.store
        .set_card_extras(
            h.user,
            &[
                (
                    ids[0],
                    flash_store::CardExtras {
                        front_html: Some(front),
                        back_html: Some(back),
                        cloze_text: Some("{{c1::Paris}} is a city\u{1f}".into()),
                        cloze_index: Some(1),
                        type_answer: None,
                    },
                ),
                (
                    ids[1],
                    flash_store::CardExtras {
                        front_html: Some(flash_store::richtext::sanitize_with_media(
                            "<b>Rich basic</b>",
                        )),
                        ..Default::default()
                    },
                ),
            ],
        )
        .unwrap();
    let before = call_tool(
        &h,
        &session,
        860,
        "list_cards",
        serde_json::json!({"deck":"Imported"}),
    )
    .await;
    let cloze = before["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["card_id"] == ids[0].0)
        .unwrap();
    assert!(cloze["front_html"].as_str().unwrap().contains("<b>"));
    assert!(cloze["source"]["read_only_reason"]
        .as_str()
        .unwrap()
        .contains("source_unavailable"));
    let error = tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":ids[0].0,"back_html":"Changed"}]}),
    )
    .await;
    assert!(error.contains("source_unavailable"));
    assert_eq!(
        before,
        call_tool(
            &h,
            &session,
            861,
            "list_cards",
            serde_json::json!({"deck":"Imported"})
        )
        .await
    );
    let basic = call_tool(
        &h,
        &session,
        862,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":ids[1].0,"back_html":"<i>New answer</i>"}]}),
    )
    .await;
    assert_eq!(basic["updated"], 1);
    assert_eq!(
        h.store
            .get_card(h.user, ids[1])
            .unwrap()
            .unwrap()
            .front_html
            .as_deref(),
        Some("<b>Rich basic</b>")
    );
}

/// An Anki import leaves a cloze note's cards without a note row; they
/// share their `cloze_text`. They must still read as one source and edit
/// together, in place.
#[tokio::test]
async fn imported_cloze_siblings_are_one_source_and_edit_together() {
    let h = harness();
    let session = initialize(&h).await;
    // What the generator renders for this source, taken from a real note.
    let reference = call_tool(
        &h,
        &session,
        870,
        "create_cards",
        serde_json::json!({"deck":"Reference","cards":[{"note_type":"cloze",
            "front_html":"{{c1::Paris}} is in {{c2::France}}","back_html":"Extra"}]}),
    )
    .await;
    let rendered: Vec<_> = reference["card_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            h.store
                .get_card(h.user, flash_core::CardId(id.as_i64().unwrap()))
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_eq!(rendered.len(), 2);

    let services = flash_server::service::Services::new(h.store.clone());
    let ids = services
        .create_cards(
            h.user,
            "Imported",
            &[
                (rendered[0].front.clone(), rendered[0].back.clone(), vec![]),
                (rendered[1].front.clone(), rendered[1].back.clone(), vec![]),
            ],
            now_ms(),
        )
        .unwrap();
    let extras = |i: usize| flash_store::CardExtras {
        front_html: rendered[i]
            .front_html
            .as_deref()
            .map(flash_store::richtext::sanitize_with_media),
        back_html: rendered[i]
            .back_html
            .as_deref()
            .map(flash_store::richtext::sanitize_with_media),
        cloze_text: Some("{{c1::Paris}} is in {{c2::France}}\u{1f}Extra".into()),
        cloze_index: Some(i as u32 + 1),
        type_answer: None,
    };
    h.store
        .set_card_extras(h.user, &[(ids[0], extras(0)), (ids[1], extras(1))])
        .unwrap();

    let listed = call_tool(
        &h,
        &session,
        871,
        "list_cards",
        serde_json::json!({"deck":"Imported"}),
    )
    .await;
    let both = serde_json::json!([ids[0].0, ids[1].0]);
    for card in listed["cards"].as_array().unwrap() {
        assert!(card["source"]["note_id"].is_null());
        assert!(card["source"]["read_only_reason"].is_null(), "{card}");
        assert_eq!(card["source"]["sibling_card_ids"], both);
    }

    let error = tool_error(
        &h,
        &session,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":ids[0].0,"back_html":"Alone"}]}),
    )
    .await;
    assert!(error.contains("sibling_card_ids"), "{error}");

    let updated = call_tool(
        &h,
        &session,
        872,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":ids[0].0,"sibling_card_ids":both,
            "back_html":"<b>Geography</b>"}]}),
    )
    .await;
    assert_eq!(updated["updated"], 2);
    assert_eq!(updated["created"], 0);
    let after = call_tool(
        &h,
        &session,
        873,
        "list_cards",
        serde_json::json!({"deck":"Imported"}),
    )
    .await;
    let after = after["cards"].as_array().unwrap();
    assert_eq!(after.len(), 2);
    assert!(after[0]["source"]["note_id"].is_i64());
    assert_eq!(after[0]["source"]["note_id"], after[1]["source"]["note_id"]);
    for card in after {
        assert!(both.as_array().unwrap().contains(&card["card_id"]));
        assert!(card["back_html"]
            .as_str()
            .unwrap()
            .contains("<b>Geography</b>"));
        assert!(card["source"]["front_html"]
            .as_str()
            .unwrap()
            .contains("{{c1::Paris}}"));
    }
}

/// A page is bounded by bytes as well as by count, and the cursor reads
/// the rest: no card is skipped or repeated.
#[tokio::test]
async fn a_page_of_large_cards_stops_at_the_byte_budget_and_the_cursor_reads_the_rest() {
    let h = harness();
    let session = initialize(&h).await;
    let back = "lorem ipsum ".repeat(1700); // about 20 KB
    let cards = (0..30)
        .map(|i| serde_json::json!({"front_html":format!("Big {i}"),"back_html":back}))
        .collect::<Vec<_>>();
    let created = call_tool(
        &h,
        &session,
        880,
        "create_cards",
        serde_json::json!({"deck":"Heavy","cards":cards}),
    )
    .await;
    assert_eq!(created["created"], 30);

    let mut seen = Vec::new();
    let mut cursor = serde_json::Value::Null;
    for page in 0..30 {
        let mut args = serde_json::json!({"deck":"Heavy","limit":500});
        if !cursor.is_null() {
            args["before_card_id"] = cursor.clone();
        }
        let out = call_tool(&h, &session, 881 + page, "list_cards", args).await;
        let rows = out["cards"].as_array().unwrap();
        assert!(!rows.is_empty());
        assert!(
            rows.len() < 30,
            "one page carried {} large cards",
            rows.len()
        );
        assert!(serde_json::to_vec(rows).unwrap().len() < 130 * 1024);
        assert_eq!(out["total_count"], 30);
        seen.extend(rows.iter().map(|c| c["card_id"].as_i64().unwrap()));
        if out["has_more"] == false {
            assert!(out["next_cursor"].is_null());
            break;
        }
        cursor = out["next_cursor"].clone();
    }
    let mut expected: Vec<i64> = created["card_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_i64().unwrap())
        .collect();
    expected.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(seen, expected);
}

/// The `client` column of every review, oldest first.
fn review_clients(store: &flash_store::Store) -> Vec<Option<String>> {
    store
        .with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT client FROM review_log ORDER BY id")?;
            let rows = stmt.query_map([], |r| r.get::<_, Option<String>>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
        .unwrap()
}

/// Sends one JSON-RPC message; returns (status, session_id_header, body_text).
pub(crate) async fn rpc(
    h: &Harness,
    session: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, Option<String>, String) {
    rpc_with(h, session, body, &[]).await
}

/// `rpc` with extra request headers (`MCP-Protocol-Version`, say).
pub(crate) async fn rpc_with(
    h: &Harness,
    session: Option<&str>,
    body: serde_json::Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Option<String>, String) {
    let mut request = Request::post("/mcp")
        .header(header::HOST, "localhost:8437")
        .header(header::AUTHORIZATION, format!("Bearer {}", h.bearer))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = h
        .app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(response.into_body(), 10_000_000)
        .await
        .unwrap();
    (status, session, String::from_utf8_lossy(&bytes).to_string())
}

/// Extracts the JSON-RPC result from a plain-JSON or SSE-framed body.
pub(crate) fn result_of(body: &str) -> serde_json::Value {
    // SSE streams open with a priming frame (`data:` + retry); take the
    // first data line that actually carries JSON.
    let json_part = if body.contains("data:") {
        body.lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .find(|payload| payload.starts_with('{'))
            .unwrap_or("{}")
            .to_string()
    } else {
        body.to_string()
    };
    let value: serde_json::Value = serde_json::from_str(&json_part).unwrap_or_default();
    value["result"].clone()
}

/// The tool result's text content, parsed as JSON.
pub(crate) fn tool_json(result: &serde_json::Value) -> serde_json::Value {
    let text = result["content"][0]["text"].as_str().unwrap_or("{}");
    serde_json::from_str(text).unwrap_or_default()
}

pub(crate) async fn initialize(h: &Harness) -> String {
    initialize_with(h, serde_json::json!({})).await
}

/// `initialize` as a client with these capabilities (a host that renders
/// MCP Apps names the extension here).
pub(crate) async fn initialize_with(h: &Harness, capabilities: serde_json::Value) -> String {
    initialize_as(h, "test", capabilities).await
}

/// `initialize_with`, introducing the client by `name`.
pub(crate) async fn initialize_as(
    h: &Harness,
    name: &str,
    capabilities: serde_json::Value,
) -> String {
    let (status, session, body) = rpc(
        h,
        None,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
                "clientInfo": {"name": name, "version": "0"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = result_of(&body);
    assert!(
        result["instructions"]
            .as_str()
            .unwrap_or("")
            .contains("study"),
        "server instructions present; raw body: {body}"
    );
    let session = session.expect("session id assigned");
    // Complete the handshake.
    let (status, _, _) = rpc(
        h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    assert!(status.is_success() || status == StatusCode::ACCEPTED);
    session
}

pub(crate) async fn call_tool(
    h: &Harness,
    session: &str,
    id: i64,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let (status, _, body) = rpc(
        h,
        Some(session),
        serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = result_of(&body);
    assert_ne!(result, serde_json::Value::Null, "rpc error: {body}");
    tool_json(&result)
}

/// TEMPORARY, goes with the `front`/`back` aliases on `NewCard`.
#[tokio::test]
async fn create_cards_still_takes_the_earlier_front_and_back_names() {
    let h = harness();
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        820,
        "create_cards",
        serde_json::json!({"deck":"Earlier","cards":[{"front":"Capital of Peru?","back":"Lima","tags":["geo"]}]}),
    )
    .await;
    assert_eq!(created["created"], 1, "{created}");
    let listed = call_tool(
        &h,
        &session,
        821,
        "list_cards",
        serde_json::json!({"deck":"Earlier"}),
    )
    .await;
    assert_eq!(listed["cards"][0]["front_html"], "Capital of Peru?");
    assert_eq!(listed["cards"][0]["back_html"], "Lima");

    let (_, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "id": 822, "method": "tools/list"}),
    )
    .await;
    let tools = result_of(&body)["tools"].clone();
    let create = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "create_cards")
        .unwrap();
    let schema = create["inputSchema"].to_string();
    assert!(schema.contains("front_html"), "{schema}");
    assert!(
        !schema.contains("\"front\""),
        "alias leaked into the schema: {schema}"
    );
    assert!(
        !schema.contains("\"back\""),
        "alias leaked into the schema: {schema}"
    );
}

#[tokio::test]
async fn an_assistant_gives_each_side_its_speech_and_language() {
    use flash_core::CardId;
    let h = harness_named("mcp_speech");
    let session = initialize(&h).await;
    // The schema asks every new card for its speech, and teaches it.
    let (_, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "id": 900, "method": "tools/list"}),
    )
    .await;
    let tools = result_of(&body)["tools"].clone();
    let create = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "create_cards")
        .unwrap()
        .clone();
    let card = &create["inputSchema"]["$defs"]["NewCard"];
    let required: Vec<&str> = card["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        required.contains(&"front_speech") && required.contains(&"back_speech"),
        "{card}"
    );
    assert!(card["properties"]["front_speech"]["description"]
        .as_str()
        .unwrap()
        .contains("ねこ"));

    let made = call_tool(
        &h,
        &session,
        901,
        "create_cards",
        serde_json::json!({
            "deck": "Japanese",
            "languages": ["ja", "en"],
            "cards": [
                {
                    "note_type": "basic_reversed",
                    "front_html": "How do you say cat in Japanese?",
                    "back_html": "猫（ねこ）",
                    "front_speech": [{"lang": "en", "text": "How do you say cat in Japanese?"}],
                    "back_speech": [{"lang": "ja", "text": "ねこ"}]
                },
                // Without speech the card is saved all the same.
                {"front_html": "dog", "back_html": "犬"}
            ]
        }),
    )
    .await;
    assert_eq!(made["created"], 3, "{made}");
    let ids: Vec<i64> = made["card_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let reversed = h.store.get_card(h.user, CardId(ids[1])).unwrap().unwrap();
    assert_eq!(
        reversed.front_speech.unwrap(),
        vec![flash_core::speech::SpeechBlock::Say {
            lang: "ja".into(),
            text: "ねこ".into()
        }]
    );
    let plain = h.store.get_card(h.user, CardId(ids[2])).unwrap().unwrap();
    assert_eq!((plain.front_speech, plain.back_speech), (None, None));
    // The deck is declared Japanese and English until its cards speak.
    let deck = h
        .store
        .find_deck_by_name(h.user, "Japanese")
        .unwrap()
        .unwrap();
    assert_eq!(
        h.store.deck_speech_hints(h.user, deck).unwrap(),
        vec!["ja", "en"]
    );

    // A cue nobody knows, or a language nobody speaks, is refused.
    let err = tool_error(
        &h,
        &session,
        "create_cards",
        serde_json::json!({"deck": "Japanese", "cards": [{
            "front_html": "x", "back_html": "y",
            "front_speech": [{"say": "sing"}], "back_speech": []
        }]}),
    )
    .await;
    assert!(err.contains("unknown speech cue"), "{err}");
    let err = tool_error(
        &h,
        &session,
        "create_cards",
        serde_json::json!({"deck": "Japanese", "cards": [{
            "front_html": "x", "back_html": "y",
            "front_speech": [{"lang": "klingon", "text": "x"}], "back_speech": []
        }]}),
    )
    .await;
    assert!(err.contains("unknown speech language"), "{err}");

    // An edit that rewrites a side brings its new blocks.
    let edited = call_tool(
        &h,
        &session,
        902,
        "update_cards",
        serde_json::json!({"cards": [{
            "card_id": ids[2],
            "back_html": "犬（いぬ）",
            "back_speech": [{"lang": "ja", "text": "いぬ"}]
        }]}),
    )
    .await;
    assert_eq!(edited["updated"], 1, "{edited}");
    let dog = h.store.get_card(h.user, CardId(ids[2])).unwrap().unwrap();
    assert_eq!(
        dog.back_speech.unwrap(),
        vec![flash_core::speech::SpeechBlock::Say {
            lang: "ja".into(),
            text: "いぬ".into()
        }]
    );
}

#[tokio::test]
async fn tools_are_discoverable() {
    let h = harness();
    let session = initialize(&h).await;
    let (status, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tools = result_of(&body)["tools"].clone();
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "start_study_session",
        "submit_review",
        "show_answer",
        "end_session",
        "list_decks",
        "create_deck",
        "create_cards",
        "update_cards",
        "delete_cards",
        "delete_deck",
        "list_cards",
        "list_tags",
        "archive_deck",
        "unarchive_deck",
    ] {
        assert!(
            names.contains(&expected),
            "missing tool {expected}: {names:?}"
        );
    }

    // Every tool advertises an outputSchema (derived from its Json<T> return).
    for t in tools.as_array().unwrap() {
        let name = t["name"].as_str().unwrap_or("?");
        assert!(t["outputSchema"].is_object(), "{name} has no outputSchema");
    }
    let start = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "start_study_session")
        .unwrap();
    let schema = start["outputSchema"].to_string();
    assert!(schema.contains("cards_due"), "{schema}");
    assert!(schema.contains("first_card"), "{schema}");

    // A client grades from this description and nothing else, so it
    // carries the scale the scheduler means: only Again says the answer
    // did not come back, which makes half an answer Again and not Hard.
    let review = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "submit_review")
        .unwrap();
    let says = review["description"].as_str().unwrap_or_default();
    assert!(says.contains("Part of an answer is a miss"), "{says}");
    assert!(
        says.contains("2, 3 and 4 all record that the user knew it"),
        "{says}"
    );
}

/// The whole `tools/list` an assistant reads, byte for byte as JSON, so
/// moving tools between files cannot change a name, a schema or a
/// description unnoticed. The panel's URI is versioned with its script,
/// so it stands as `{PANEL_URI}`. A deliberate change to a tool rewrites
/// the file: run this test once with `FLASH_WRITE_TOOLS_LIST=1`.
#[tokio::test]
async fn the_tool_list_is_exactly_the_pinned_one() {
    let h = harness();
    let session = initialize(&h).await;
    let (_, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let listed = serde_json::to_string_pretty(&result_of(&body)["tools"])
        .unwrap()
        .replace(flash_server::mcp_panel::URI, "{PANEL_URI}");
    let pinned_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_tools_list.json");
    if std::env::var_os("FLASH_WRITE_TOOLS_LIST").is_some() {
        std::fs::write(&pinned_path, format!("{listed}\n")).unwrap();
    }
    let pinned = std::fs::read_to_string(&pinned_path)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(listed, pinned.trim_end(), "tools/list changed");
}

/// Every assistant picks a tool from its name, title, description and
/// parameter docs alone, and the plugin directories review exactly these
/// fields. So each tool a model can call says when to use it first, names
/// no host (the same text serves every assistant), carries a title and
/// explicit hints, and documents every parameter.
#[tokio::test]
async fn every_tool_says_when_to_use_it_for_any_assistant() {
    let h = harness_built(AppBuilder::new("mcp"));
    let session = initialize(&h).await;
    let (_, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let tools = result_of(&body)["tools"].as_array().unwrap().clone();

    fn undocumented(schema: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match schema {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Object(props)) = map.get("properties") {
                    for (name, prop) in props {
                        if prop.get("description").is_none() {
                            out.push(format!("{path}.{name}"));
                        }
                    }
                }
                for (key, value) in map {
                    undocumented(value, &format!("{path}/{key}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for value in items {
                    undocumented(value, path, out);
                }
            }
            _ => {}
        }
    }

    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        let description = tool["description"].as_str().unwrap();
        let hints = &tool["annotations"];
        assert!(
            tool["title"].is_string() && tool["title"] == hints["title"],
            "{name}: top-level title {} vs annotations {}",
            tool["title"],
            hints["title"]
        );
        for hint in ["readOnlyHint", "destructiveHint", "openWorldHint"] {
            assert!(hints[hint].is_boolean(), "{name}: {hint} not explicit");
        }
        assert_eq!(hints["openWorldHint"], false, "{name}");
        if name.starts_with("panel_") {
            continue;
        }
        assert!(
            description.starts_with("Use this "),
            "{name}: {description}"
        );
        for host in [
            "ChatGPT", "Claude", "Grok", "Mistral", "Le Chat", "Gemini", "@Flash", "panel",
            "widget",
        ] {
            assert!(!description.contains(host), "{name} names {host}");
        }
        let mut missing = Vec::new();
        undocumented(&tool["inputSchema"], name, &mut missing);
        assert!(missing.is_empty(), "undocumented parameters: {missing:?}");
    }

    // The hints a host acts on (confirmation prompts, auto-approval),
    // pinned so a change to one is a decision and not an accident:
    // (readOnly, destructive, idempotent).
    let expected: &[(&str, bool, bool, bool)] = &[
        ("list_decks", true, false, false),
        ("list_tags", true, false, false),
        ("list_cards", true, false, false),
        ("get_limits", true, false, false),
        ("show_answer", true, false, false),
        ("start_study_session", false, false, false),
        ("submit_review", false, false, false),
        ("end_session", false, false, true),
        ("create_deck", false, false, false),
        ("create_cards", false, false, false),
        ("update_cards", false, false, true),
        ("restore_cards", false, false, true),
        ("rename_deck", false, false, true),
        ("archive_deck", false, false, true),
        ("unarchive_deck", false, false, true),
        ("set_limits", false, false, true),
        ("boost_new_today", false, false, false),
        ("delete_cards", false, true, false),
        ("delete_deck", false, true, false),
    ];
    for (name, read_only, destructive, idempotent) in expected {
        let tool = tools
            .iter()
            .find(|t| t["name"] == *name)
            .unwrap_or_else(|| panic!("no tool {name}"));
        let hints = &tool["annotations"];
        assert_eq!(hints["readOnlyHint"], *read_only, "{name} readOnlyHint");
        assert_eq!(
            hints["destructiveHint"], *destructive,
            "{name} destructiveHint"
        );
        assert_eq!(
            hints["idempotentHint"].as_bool().unwrap_or(false),
            *idempotent,
            "{name} idempotentHint"
        );
    }
    let model_visible = tools
        .iter()
        .filter(|t| !t["name"].as_str().unwrap().starts_with("panel_"))
        .count();
    assert_eq!(
        model_visible,
        expected.len(),
        "a new tool needs its hints pinned here"
    );
}

#[tokio::test]
async fn a_whole_study_loop_over_mcp() {
    let h = harness();
    let session = initialize(&h).await;

    let created = call_tool(
        &h,
        &session,
        10,
        "create_cards",
        serde_json::json!({
            "deck": "Pharm",
            "cards": [
                {"front_html": "Antidote for warfarin?", "back_html": "Vitamin K", "tags": ["exam-2"]},
                {"front_html": "Antidote for heparin?", "back_html": "Protamine sulfate"}
            ]
        }),
    )
    .await;
    assert_eq!(created["created"], 2);

    // Start a session; grading mode defaults to silent.
    let started = call_tool(
        &h,
        &session,
        11,
        "start_study_session",
        serde_json::json!({"deck": "Pharm"}),
    )
    .await;
    assert_eq!(started["cards_due"], 2);
    assert!(started["grading_mode"]
        .as_str()
        .unwrap()
        .starts_with("silent"));
    let sid = started["session_id"].as_i64().unwrap();
    let card1 = started["first_card"]["card_id"].as_i64().unwrap();
    assert!(started["first_card"]["front_html"]
        .as_str()
        .unwrap()
        .contains("Antidote"));

    // She answers correctly -> silent Good; next card arrives in-call.
    let r1 = call_tool(
        &h,
        &session,
        12,
        "submit_review",
        serde_json::json!({"session_id": sid, "card_id": card1, "rating": 3}),
    )
    .await;
    let card2 = r1["next_card"]["card_id"].as_i64().unwrap();
    assert_ne!(card2, card1);

    // She gives up on card 2: show_answer, then Again.
    let answer = call_tool(
        &h,
        &session,
        13,
        "show_answer",
        serde_json::json!({"card_id": card2}),
    )
    .await;
    assert_eq!(answer["back_html"], "Protamine sulfate");
    let r2 = call_tool(
        &h,
        &session,
        14,
        "submit_review",
        serde_json::json!({"session_id": sid, "card_id": card2, "rating": 1}),
    )
    .await;
    // The failed card is in a learning step; it's the only thing left.
    assert_eq!(r2["next_card"]["card_id"].as_i64().unwrap(), card2);

    let summary = call_tool(
        &h,
        &session,
        15,
        "end_session",
        serde_json::json!({"session_id": sid}),
    )
    .await;
    assert_eq!(summary["reviewed"], 2);
    assert_eq!(summary["good"], 1);
    assert_eq!(summary["again"], 1);
    // It says where the cards can be studied next time.
    assert!(summary["note"]
        .as_str()
        .unwrap()
        .ends_with("/study?via=chat"));

    // Each review remembers which client it came through.
    assert_eq!(
        review_clients(&h.store),
        [Some("Claude".to_string()), Some("Claude".to_string())]
    );

    // Response hygiene: nothing scheduling-ish in the submit payloads.
    for payload in [&r1, &r2] {
        let text = payload.to_string();
        assert!(!text.contains("stability"));
        assert!(!text.contains("interval"));
        assert!(!text.contains("due"));
    }
}

/// A cloze card over MCP: the assistant is handed "blank" for the gap
/// on the front and the word itself with the answer.
#[tokio::test]
async fn cloze_cards_hand_the_assistant_blank_then_the_word() {
    let h = harness();
    let services = flash_server::service::Services::new(h.store.clone());
    let deck = services
        .create_deck(h.user, "Capitals", "", now_ms())
        .unwrap();
    services
        .create_note(
            h.user,
            deck,
            &flash_server::service::NoteInput {
                note_type: flash_store::notes::NoteType::Cloze,
                front_html: "{{c1::Paris}} is the capital of France".into(),
                back_html: String::new(),
                tags: vec![],
                speech: Default::default(),
                langs: None,
            },
            now_ms(),
        )
        .unwrap();
    let session = initialize(&h).await;

    let started = call_tool(
        &h,
        &session,
        20,
        "start_study_session",
        serde_json::json!({"deck": "Capitals"}),
    )
    .await;
    assert_eq!(
        started["first_card"]["front_html"],
        "<span class=\"cloze-blank\">[...]</span> is the capital of France"
    );
    let card = started["first_card"]["card_id"].as_i64().unwrap();

    let answer = call_tool(
        &h,
        &session,
        21,
        "show_answer",
        serde_json::json!({"card_id": card}),
    )
    .await;
    assert!(answer["back_html"].as_str().unwrap().contains("Paris"));
}

/// The "I just made 44 cards, quiz me" case: the daily new-card limit
/// caps the session, and the model must be told the rest are held back
/// (and how to release them) rather than left to declare the user done
/// or improvise a quiz from chat.
#[tokio::test]
async fn empty_queue_explains_daily_limit() {
    let h = harness();
    let session = initialize(&h).await;

    call_tool(
        &h,
        &session,
        20,
        "set_limits",
        serde_json::json!({"new_per_day": 1}),
    )
    .await;
    let created = call_tool(
        &h,
        &session,
        21,
        "create_cards",
        serde_json::json!({
            "deck": "Weak Spots",
            "cards": [
                {"front_html": "Q1", "back_html": "A1"},
                {"front_html": "Q2", "back_html": "A2"},
                {"front_html": "Q3", "back_html": "A3"}
            ]
        }),
    )
    .await;
    assert_eq!(created["created"], 3);

    let started = call_tool(
        &h,
        &session,
        22,
        "start_study_session",
        serde_json::json!({"deck": "Weak Spots"}),
    )
    .await;
    assert_eq!(started["cards_due"], 1);
    assert!(started["note"].is_null(), "no note while cards remain");
    let sid = started["session_id"].as_i64().unwrap();
    let card1 = started["first_card"]["card_id"].as_i64().unwrap();

    let r = call_tool(
        &h,
        &session,
        23,
        "submit_review",
        serde_json::json!({"session_id": sid, "card_id": card1, "rating": 3}),
    )
    .await;
    assert!(r["next_card"].is_null());
    assert_eq!(r["remaining"], 0);
    let note = r["note"].as_str().expect("note explains the empty queue");
    assert!(note.contains("2 more card"), "{note}");
    assert!(note.contains("boost_new_today"), "{note}");

    // Following the note releases the rest.
    call_tool(
        &h,
        &session,
        24,
        "boost_new_today",
        serde_json::json!({"deck": "Weak Spots", "extra": 5}),
    )
    .await;
    let again = call_tool(
        &h,
        &session,
        25,
        "start_study_session",
        serde_json::json!({"deck": "Weak Spots"}),
    )
    .await;
    assert_eq!(again["cards_due"], 2);
    assert!(again["note"].is_null());
}

#[tokio::test]
async fn delete_deck_removes_the_deck_and_reports_card_count() {
    let h = harness();
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        40,
        "create_cards",
        serde_json::json!({
            "deck": "Ephemeral",
            "cards": [
                {"front_html": "q1", "back_html": "a1"},
                {"front_html": "q2", "back_html": "a2"},
                {"front_html": "trashed", "back_html": "earlier"}
            ]
        }),
    )
    .await;
    assert_eq!(created["created"], 3);
    // A card trashed earlier goes with the deck but is not counted: the
    // person is asked about the two cards the deck shows.
    let trashed = created["card_ids"][2].as_i64().unwrap();
    h.store
        .delete_card(h.user, flash_core::CardId(trashed), now_ms())
        .unwrap();

    // Step one deletes nothing: it names what would go and hands back a
    // token the model can only spend after the person said yes.
    let asked = call_tool(
        &h,
        &session,
        41,
        "delete_deck",
        serde_json::json!({"deck": "Ephemeral"}),
    )
    .await;
    assert_eq!(asked["deleted"], false);
    assert_eq!(asked["pending"], true);
    assert_eq!(asked["cards_affected"], 2);
    assert!(!asked["card_ids"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(trashed)));
    let token = asked["confirm_token"]
        .as_str()
        .expect("confirm token")
        .to_string();
    let decks = call_tool(&h, &session, 42, "list_decks", serde_json::json!({})).await;
    assert!(decks["decks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["name"] == "Ephemeral"));

    // A token for one target does not spend on another.
    let (status, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 43, "method": "tools/call",
            "params": {"name": "delete_cards", "arguments": {"card_ids": [1], "confirm_token": token}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        result_of(&body) == serde_json::Value::Null || result_of(&body)["isError"] == true,
        "a deck token spent on a card: {body}"
    );

    // Step two, with the token, deletes; the token is then spent.
    let deleted = call_tool(
        &h,
        &session,
        44,
        "delete_deck",
        serde_json::json!({"deck": "Ephemeral", "confirm_token": token}),
    )
    .await;
    assert_eq!(deleted["deleted"], true);
    assert_eq!(deleted["cards_affected"], 2);

    let decks = call_tool(&h, &session, 45, "list_decks", serde_json::json!({})).await;
    let names: Vec<&str> = decks["decks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["name"].as_str())
        .collect();
    assert!(!names.contains(&"Ephemeral"), "{names:?}");
}

/// `rename_deck` renames and moves, because a name is a path: a deck made
/// at the top level goes under another, its subdeck follows, the parent's
/// row then counts both, and two decks are never merged.
#[tokio::test]
async fn rename_deck_moves_a_deck_with_what_is_under_it() {
    let h = harness();
    let session = initialize(&h).await;
    for (id, deck) in [(70, "Cardio"), (71, "Cardio::Arrhythmias"), (72, "Pharm")] {
        call_tool(
            &h,
            &session,
            id,
            "create_cards",
            serde_json::json!({"deck": deck, "cards": [{"front_html": "q", "back_html": "a"}]}),
        )
        .await;
    }
    let moved = call_tool(
        &h,
        &session,
        73,
        "rename_deck",
        serde_json::json!({"deck": "Cardio", "new_name": " Pharm :: Cardio "}),
    )
    .await;
    assert_eq!(moved["renamed"], true);
    assert_eq!(moved["name"], "Pharm::Cardio");
    assert_eq!(moved["decks_renamed"], 2);

    let decks = call_tool(&h, &session, 74, "list_decks", serde_json::json!({})).await;
    let rows: Vec<(&str, u64)> = decks["decks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["name"].as_str().unwrap(), d["new"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("Pharm", 3),
            ("Pharm::Cardio", 2),
            ("Pharm::Cardio::Arrhythmias", 1)
        ],
        "each deck's count holds the decks under it"
    );

    // Studying the parent serves the subdecks' cards too.
    let started = call_tool(
        &h,
        &session,
        75,
        "start_study_session",
        serde_json::json!({"deck": "Pharm"}),
    )
    .await;
    assert_eq!(started["cards_due"], 3, "{started}");

    // Onto a name that is taken, or into itself: refused, nothing moves.
    call_tool(
        &h,
        &session,
        76,
        "create_deck",
        serde_json::json!({"name": "Taken"}),
    )
    .await;
    for new_name in ["Taken", "Pharm::Cardio::Arrhythmias::Deeper"] {
        tool_error(
            &h,
            &session,
            "rename_deck",
            serde_json::json!({"deck": "Pharm::Cardio", "new_name": new_name}),
        )
        .await;
    }
    assert!(h
        .store
        .find_deck_by_name(h.user, "Pharm::Cardio::Arrhythmias")
        .unwrap()
        .is_some());
}

/// A deck name is a path. Saving cards into `Pharm::Cardio::Arrhythmias`
/// on an empty account makes the decks on the way; deleting `Pharm` asks
/// about every deck under it by name, does not stretch to a subdeck that
/// arrived after the question, and takes the whole subtree once asked again.
#[tokio::test]
async fn a_deck_path_makes_its_parents_and_a_deletion_names_the_subdecks() {
    let h = harness();
    let session = initialize(&h).await;
    for (id, deck) in [(60, "Pharm::Cardio::Arrhythmias"), (61, " Pharm :: Renal ")] {
        let created = call_tool(
            &h,
            &session,
            id,
            "create_cards",
            serde_json::json!({"deck": deck, "cards": [{"front_html": "q", "back_html": "a"}]}),
        )
        .await;
        assert_eq!(created["created"], 1, "{created}");
    }
    let names = |decks: &serde_json::Value| -> Vec<String> {
        decks["decks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect()
    };
    let whole = vec![
        "Pharm",
        "Pharm::Cardio",
        "Pharm::Cardio::Arrhythmias",
        "Pharm::Renal",
    ];
    let decks = call_tool(&h, &session, 62, "list_decks", serde_json::json!({})).await;
    assert_eq!(names(&decks), whole);

    let asked = call_tool(
        &h,
        &session,
        63,
        "delete_deck",
        serde_json::json!({"deck": "Pharm"}),
    )
    .await;
    assert_eq!(asked["pending"], true);
    assert_eq!(asked["decks_affected"], serde_json::json!(whole));
    assert_eq!(asked["cards_affected"], 2);

    // An empty subdeck made after the question changes no pinned card,
    // and still is not what the person said yes to.
    call_tool(
        &h,
        &session,
        64,
        "create_deck",
        serde_json::json!({"name": "Pharm::Neuro"}),
    )
    .await;
    let error = tool_error(
        &h,
        &session,
        "delete_deck",
        serde_json::json!({"deck": "Pharm", "confirm_token": asked["confirm_token"]}),
    )
    .await;
    assert!(error.contains("confirmation_target_changed"), "{error}");
    let decks = call_tool(&h, &session, 65, "list_decks", serde_json::json!({})).await;
    assert_eq!(names(&decks).len(), 5, "nothing was deleted");

    let asked = call_tool(
        &h,
        &session,
        66,
        "delete_deck",
        serde_json::json!({"deck": "Pharm"}),
    )
    .await;
    assert_eq!(asked["decks_affected"].as_array().unwrap().len(), 5);
    let deleted = call_tool(
        &h,
        &session,
        67,
        "delete_deck",
        serde_json::json!({"deck": "Pharm", "confirm_token": asked["confirm_token"]}),
    )
    .await;
    assert_eq!(deleted["deleted"], true);
    assert_eq!(deleted["cards_affected"], 2);
    assert_eq!(deleted["decks_affected"], asked["decks_affected"]);
    let decks = call_tool(&h, &session, 68, "list_decks", serde_json::json!({})).await;
    assert!(names(&decks).is_empty(), "{decks}");
}

/// Archiving needs no confirmation (nothing is deleted): the deck lists
/// as archived with zero counts, a session finds nothing, and
/// unarchiving hands the cards back.
#[tokio::test]
async fn archive_deck_parks_it_without_a_confirmation() {
    let h = harness();
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        50,
        "create_cards",
        serde_json::json!({
            "deck": "Passed exam",
            "cards": [
                {"front_html": "q1", "back_html": "a1"},
                {"front_html": "q2", "back_html": "a2"}
            ]
        }),
    )
    .await;
    assert_eq!(created["created"], 2);

    let archived = call_tool(
        &h,
        &session,
        51,
        "archive_deck",
        serde_json::json!({"deck": "Passed exam"}),
    )
    .await;
    assert_eq!(archived["saved"], true);
    let decks = call_tool(&h, &session, 52, "list_decks", serde_json::json!({})).await;
    let row = decks["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "Passed exam")
        .expect("still listed")
        .clone();
    assert_eq!(row["archived"], true);
    assert_eq!(
        (row["due"].as_u64(), row["new"].as_u64()),
        (Some(0), Some(0))
    );
    let study = call_tool(
        &h,
        &session,
        53,
        "start_study_session",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(study["cards_due"], 0, "{study}");

    let restored = call_tool(
        &h,
        &session,
        54,
        "unarchive_deck",
        serde_json::json!({"deck": "Passed exam"}),
    )
    .await;
    assert_eq!(restored["saved"], true);
    let study = call_tool(
        &h,
        &session,
        55,
        "start_study_session",
        serde_json::json!({"deck": "Passed exam"}),
    )
    .await;
    assert_eq!(study["cards_due"], 2, "{study}");

    // A deck that does not exist is an error, not a silent no-op.
    let (status, _, body) = rpc(
        &h,
        Some(&session),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 56, "method": "tools/call",
            "params": {"name": "archive_deck", "arguments": {"deck": "Nope"}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("no deck named"), "{body}");
}

/// A deck's card list leaves out its archived subdecks' cards, as the
/// API and the deck page do; the archived subdeck still lists its own.
#[tokio::test]
async fn list_cards_leaves_out_archived_subdecks() {
    let h = harness();
    let session = initialize(&h).await;
    for (id, deck, front) in [
        (60, "Pharm", "own"),
        (61, "Pharm::Live", "live"),
        (62, "Pharm::Old", "old"),
    ] {
        let card = serde_json::json!({"front_html": front, "back_html": "b"});
        let args = serde_json::json!({"deck": deck, "cards": [card]});
        let created = call_tool(&h, &session, id, "create_cards", args).await;
        assert_eq!(created["created"], 1, "{created}");
    }
    let archived = call_tool(
        &h,
        &session,
        63,
        "archive_deck",
        serde_json::json!({"deck": "Pharm::Old"}),
    )
    .await;
    assert_eq!(archived["saved"], true);

    let fronts = |out: &serde_json::Value| -> Vec<String> {
        out["cards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["front_html"].as_str().unwrap().to_string())
            .collect()
    };
    let pharm = call_tool(
        &h,
        &session,
        64,
        "list_cards",
        serde_json::json!({"deck": "Pharm"}),
    )
    .await;
    assert_eq!(fronts(&pharm), vec!["live", "own"], "{pharm}");
    assert_eq!(pharm["total_count"], 2);

    let old = call_tool(
        &h,
        &session,
        65,
        "list_cards",
        serde_json::json!({"deck": "Pharm::Old"}),
    )
    .await;
    assert_eq!(fronts(&old), vec!["old"], "{old}");
    assert_eq!(old["total_count"], 1);
}

/// A token minted for the first-party app is not an MCP grant: the two
/// are different consents, and the app's token must not drive tools.
#[tokio::test]
async fn a_mobile_app_token_does_not_authenticate_mcp() {
    let t = AppBuilder::new("mcp-token-class").build();
    let user = member(&t.store, "FlashTester", "flashtester@example.com");
    let h = Harness {
        bearer: api_bearer(&t.store, user),
        store: t.store.clone(),
        services: t.services,
        data_dir: t.data_dir,
        app: t.app,
        user,
    };
    let (status, _, _) = rpc(
        &h,
        None,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Ids in tool arguments are scoped by the token's user: another user's
/// session and card ids are unknown, and nothing of theirs moves.
#[tokio::test]
async fn tool_arguments_cannot_reach_another_users_cards() {
    let t = AppBuilder::new("mcp-isolation").build();
    let owner = member(&t.store, "FlashTester", "flashtester@example.com");
    let other = member(&t.store, "Other", "other@example.com");
    t.store
        .create_oauth_client("test-client", "Claude", &[], "{}", now_ms())
        .unwrap();
    let now = now_ms();
    let deck = t.services.create_deck(owner, "Owned", "", now).unwrap();
    let cards = t
        .services
        .create_cards_in_deck(owner, deck, &[("f".into(), "b".into(), vec![])], now)
        .unwrap();
    let session = t
        .services
        .start_session(owner, flash_core::queue::StudyScope::all(), now)
        .unwrap()
        .session_id;
    let h = Harness {
        bearer: oauth_bearer(&t.store, other),
        store: t.store.clone(),
        services: t.services.clone(),
        data_dir: t.data_dir,
        app: t.app,
        user: other,
    };
    let mcp = initialize(&h).await;

    for (id, name, args) in [
        (
            80,
            "submit_review",
            serde_json::json!({"session_id": session.0, "card_id": cards[0].0, "rating": 3}),
        ),
        (
            81,
            "show_answer",
            serde_json::json!({"card_id": cards[0].0}),
        ),
        (
            82,
            "update_cards",
            serde_json::json!({"cards":[{"card_id": cards[0].0, "front_html": "x", "back_html": "y"}]}),
        ),
        (
            83,
            "delete_cards",
            serde_json::json!({"card_id": cards[0].0}),
        ),
        (
            84,
            "end_session",
            serde_json::json!({"session_id": session.0}),
        ),
    ] {
        let (status, _, body) = rpc(
            &h,
            Some(&mcp),
            serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": {"name": name, "arguments": args}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {body}");
        let result = result_of(&body);
        let refused = result == serde_json::Value::Null || result["isError"] == true;
        assert!(refused, "{name} answered another user's ids: {body}");
    }
    let card = t.services.card(owner, cards[0]).unwrap();
    assert_eq!((card.front.as_str(), card.back.as_str()), ("f", "b"));
    assert!(review_clients(&t.store).is_empty());
}

#[tokio::test]
async fn a_tag_is_one_tag_however_its_accents_are_written() {
    let h = harness();
    let session = initialize(&h).await;
    call_tool(
        &h,
        &session,
        880,
        "create_cards",
        serde_json::json!({"deck": "French", "cards": [
            {"front_html": "decomposed", "back_html": "b", "tags": ["Cafe\u{301}"]},
            {"front_html": "composed", "back_html": "b", "tags": ["café"]}
        ]}),
    )
    .await;
    let all = call_tool(&h, &session, 881, "list_tags", serde_json::json!({})).await;
    assert_eq!(
        all["tags"],
        serde_json::json!([{"name": "café", "cards": 2}])
    );
    let scoped = call_tool(
        &h,
        &session,
        882,
        "start_study_session",
        serde_json::json!({"tags": ["cafe\u{301}"]}),
    )
    .await;
    assert_eq!(scoped["cards_due"], 2, "a decomposed scope finds the tag");
}

#[tokio::test]
async fn tags_name_a_study_scope_and_list_themselves() {
    let h = harness();
    let session = initialize(&h).await;
    call_tool(
        &h,
        &session,
        870,
        "create_cards",
        serde_json::json!({"deck": "Pharm", "cards": [
            {"front_html": "both", "back_html": "b", "tags": ["nclex", "cardio"]},
            {"front_html": "one", "back_html": "b", "tags": ["nclex"]}
        ]}),
    )
    .await;
    call_tool(
        &h,
        &session,
        871,
        "create_cards",
        serde_json::json!({"deck": "Anatomy", "cards": [
            {"front_html": "a1", "back_html": "b", "tags": ["cardio"]}
        ]}),
    )
    .await;

    let all = call_tool(&h, &session, 872, "list_tags", serde_json::json!({})).await;
    assert_eq!(
        all["tags"],
        serde_json::json!([
            {"name": "cardio", "cards": 2},
            {"name": "nclex", "cards": 2},
        ])
    );
    let in_deck = call_tool(
        &h,
        &session,
        873,
        "list_tags",
        serde_json::json!({"deck": "Anatomy"}),
    )
    .await;
    assert_eq!(
        in_deck["tags"],
        serde_json::json!([{"name": "cardio", "cards": 1}])
    );
    tool_error(
        &h,
        &session,
        "list_tags",
        serde_json::json!({"deck": "Nope"}),
    )
    .await;

    // Every tag must be on the card, and a deck narrows them rather than
    // being ignored as it once was.
    let both = call_tool(
        &h,
        &session,
        874,
        "start_study_session",
        serde_json::json!({"tags": ["nclex", "cardio"]}),
    )
    .await;
    assert_eq!(both["cards_due"], 1);
    let narrowed = call_tool(
        &h,
        &session,
        875,
        "start_study_session",
        serde_json::json!({"deck": "Anatomy", "tags": ["cardio"]}),
    )
    .await;
    assert_eq!(narrowed["cards_due"], 1);
    let missing = call_tool(
        &h,
        &session,
        876,
        "start_study_session",
        serde_json::json!({"deck": "Anatomy", "tags": ["nclex"]}),
    )
    .await;
    assert_eq!(missing["cards_due"], 0, "no card in that deck carries it");
    // The single tag argument is one more tag, not a rival to them.
    let merged = call_tool(
        &h,
        &session,
        877,
        "start_study_session",
        serde_json::json!({"tag": "nclex", "tags": ["cardio"]}),
    )
    .await;
    assert_eq!(merged["cards_due"], 1);
}

#[tokio::test]
async fn a_goal_date_is_set_shown_and_cleared_over_mcp() {
    let h = harness();
    let session = initialize(&h).await;
    let today = h.store.get_settings(h.user).unwrap().study_date(now_ms());
    let in_a_month = today.checked_add(jiff::Span::new().days(31)).unwrap();

    call_tool(
        &h,
        &session,
        30,
        "create_deck",
        serde_json::json!({"name": "Boards", "goal_date": in_a_month.to_string()}),
    )
    .await;
    call_tool(
        &h,
        &session,
        31,
        "create_cards",
        serde_json::json!({
            "deck": "Boards",
            "cards": (0..27).map(|i| serde_json::json!({"front_html": format!("Q{i}"), "back_html": "A"})).collect::<Vec<_>>()
        }),
    )
    .await;
    let decks = call_tool(&h, &session, 32, "list_decks", serde_json::json!({})).await;
    let goal = &decks["decks"][0]["goal"];
    assert_eq!(goal["date"], in_a_month.to_string());
    assert_eq!(goal["days_left"], 31);
    assert_eq!(goal["on_track"], true);
    assert_eq!(goal["new_per_day"], 1);
    assert_eq!(decks["decks"][0]["new"], 1);

    let in_a_week = today.checked_add(jiff::Span::new().days(8)).unwrap();
    call_tool(
        &h,
        &session,
        33,
        "set_limits",
        serde_json::json!({"deck": "Boards", "goal_date": in_a_week.to_string()}),
    )
    .await;
    let limits = call_tool(&h, &session, 34, "get_limits", serde_json::json!({})).await;
    let boards = &limits["decks"][0];
    assert_eq!(boards["goal"]["date"], in_a_week.to_string());
    assert_eq!(boards["goal"]["new_per_day"], 6);
    assert_eq!(boards["new_available"], 6);

    call_tool(
        &h,
        &session,
        35,
        "set_limits",
        serde_json::json!({"deck": "Boards", "goal_date": ""}),
    )
    .await;
    let decks = call_tool(&h, &session, 36, "list_decks", serde_json::json!({})).await;
    assert!(decks["decks"][0].get("goal").is_none(), "{decks}");

    tool_error(
        &h,
        &session,
        "set_limits",
        serde_json::json!({"deck": "Boards", "goal_date": "next friday"}),
    )
    .await;
    tool_error(
        &h,
        &session,
        "set_limits",
        serde_json::json!({"goal_date": in_a_week.to_string()}),
    )
    .await;
    // A refused call saves neither half: not the goal beside a limit out
    // of range, nor the limit beside a goal already past.
    tool_error(
        &h,
        &session,
        "set_limits",
        serde_json::json!({"deck": "Boards", "goal_date": in_a_week.to_string(), "new_per_day": 99999}),
    )
    .await;
    tool_error(
        &h,
        &session,
        "set_limits",
        serde_json::json!({"deck": "Boards", "goal_date": today.to_string(), "new_per_day": 5}),
    )
    .await;
    let boards = h
        .store
        .find_deck_by_name(h.user, "Boards")
        .unwrap()
        .unwrap();
    assert_eq!(
        h.store.get_deck_limits(h.user, boards).unwrap().new_per_day,
        None
    );
    let decks = call_tool(&h, &session, 37, "list_decks", serde_json::json!({})).await;
    assert!(decks["decks"][0].get("goal").is_none(), "{decks}");
    tool_error(
        &h,
        &session,
        "create_deck",
        serde_json::json!({"name": "Too late", "goal_date": today.to_string()}),
    )
    .await;
    assert_eq!(h.store.find_deck_by_name(h.user, "Too late").unwrap(), None);
}

#[tokio::test]
async fn a_card_deleted_from_a_note_stays_gone_after_update_cards() {
    let h = harness_named("mcp-deleted-slot");
    let session = initialize(&h).await;
    let created = call_tool(
        &h,
        &session,
        800,
        "create_cards",
        serde_json::json!({"deck":"Words","cards":[
            {"note_type":"basic_reversed","front_html":"hola","back_html":"hello"}
        ]}),
    )
    .await;
    let left = created["card_ids"][0].as_i64().unwrap();
    let right = created["card_ids"][1].as_i64().unwrap();
    h.store
        .delete_card(h.user, flash_core::CardId(right), now_ms())
        .unwrap();
    let updated = call_tool(
        &h,
        &session,
        801,
        "update_cards",
        serde_json::json!({"cards":[
            {"card_id":left,"sibling_card_ids":[left],"front_html":"hola!"}
        ]}),
    )
    .await;
    assert_eq!(updated["created"], 0, "{updated}");
    let deck = h.store.find_deck_by_name(h.user, "Words").unwrap().unwrap();
    assert_eq!(h.store.count_cards(h.user, Some(deck), None).unwrap(), 1);
}

/// Refuses every add, so an edit that would make a card fails loudly.
struct NoNewCards;

impl flash_server::policy::CardPolicy for NoNewCards {
    fn admit(
        &self,
        _store: &flash_store::Store,
        _user: flash_core::UserId,
        adding: u32,
        _source: flash_server::policy::AddSource,
        _now_ms: i64,
    ) -> Result<flash_server::policy::Admission, flash_store::StoreError> {
        Ok(if adding == 0 {
            flash_server::policy::Admission::Allowed { recheck_cap: None }
        } else {
            flash_server::policy::Admission::Denied { current: 0, cap: 0 }
        })
    }

    fn new_budget(
        &self,
        _store: &flash_store::Store,
        _user: flash_core::UserId,
        budget: u32,
        _now_ms: i64,
    ) -> Result<u32, flash_store::StoreError> {
        Ok(budget)
    }

    fn over_cap_message(&self, _current: u32, _cap: u32) -> flash_server::i18n::Words {
        flash_server::policy::Unlimited.over_cap_message(0, 0)
    }
}

#[tokio::test]
async fn editing_an_imported_cloze_with_a_deleted_sibling_counts_no_new_card() {
    let h = harness_built(
        AppBuilder::new("mcp-cloze-gone-cap").policy(std::sync::Arc::new(NoNewCards)),
    );
    let session = initialize(&h).await;
    let source = "{{c1::Paris}} is in {{c2::France}}";
    let services = flash_server::service::Services::new(h.store.clone());
    let reference = services
        .create_note(
            h.user,
            h.store
                .create_deck(h.user, "Reference", "", now_ms())
                .unwrap(),
            &flash_server::service::NoteInput {
                note_type: flash_store::notes::NoteType::Cloze,
                front_html: source.into(),
                back_html: String::new(),
                tags: vec![],
                speech: Default::default(),
                langs: None,
            },
            now_ms(),
        )
        .unwrap()
        .1;
    let rendered: Vec<_> = reference
        .iter()
        .map(|id| h.store.get_card(h.user, *id).unwrap().unwrap())
        .collect();
    let ids = services
        .create_cards(
            h.user,
            "Imported",
            &rendered
                .iter()
                .map(|c| (c.front.clone(), c.back.clone(), vec![]))
                .collect::<Vec<_>>(),
            now_ms(),
        )
        .unwrap();
    let extras = |i: usize| flash_store::CardExtras {
        front_html: rendered[i]
            .front_html
            .as_deref()
            .map(flash_store::richtext::sanitize_with_media),
        back_html: rendered[i]
            .back_html
            .as_deref()
            .map(flash_store::richtext::sanitize_with_media),
        cloze_text: Some(format!("{source}\u{1f}")),
        cloze_index: Some(i as u32 + 1),
        type_answer: None,
    };
    h.store
        .set_card_extras(h.user, &[(ids[0], extras(0)), (ids[1], extras(1))])
        .unwrap();
    h.store.delete_card(h.user, ids[1], now_ms()).unwrap();

    let updated = call_tool(
        &h,
        &session,
        900,
        "update_cards",
        serde_json::json!({"cards":[{"card_id":ids[0].0,"sibling_card_ids":[ids[0].0],
            "back_html":"Geography"}]}),
    )
    .await;
    assert_eq!(updated["created"], 0, "{updated}");
}
