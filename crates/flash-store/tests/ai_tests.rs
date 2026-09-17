//! Card generation storage: sources, cached readings and their figure
//! pins, jobs and their state, drafts and the user's decisions on them,
//! the call ledger a quota is derived from, the runner's and the
//! scheduler's cross-account sweeps, the lineage from a kept note back
//! to its reading, and the two rules every table here shares: a user
//! never sees another's rows, and deleting the account takes them all.

use flash_core::{AiJobId, AiSourceId, DeckId, MediaId, NoteId, UserId};
use flash_store::media::MediaKind;
use flash_store::notes::NoteType;
use flash_store::richtext::sanitize_with_media;
use flash_store::{
    AiCallNew, AiCallStep, AiDraftNew, AiDraftStatus, AiExtractionNew, AiJobNew, AiJobOp,
    AiJobState, AiSourceKind, AiSourceNew, AiTurnNew, AiTurnRole, NoteSave, Store, StoreError,
};

const NOW: i64 = 1_700_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

fn store_with_user() -> (Store, UserId) {
    let store = Store::open_in_memory().expect("open + migrate");
    let user = store
        .create_user("Ada", Some("ada@example.com"), "member", NOW)
        .unwrap();
    (store, user)
}

fn source(store: &Store, user: UserId, sha: &str, at: i64) -> AiSourceId {
    store
        .create_ai_source(
            user,
            &AiSourceNew {
                kind: AiSourceKind::Pdf,
                sha256: sha,
                mime: "application/pdf",
                size: 1234,
                filename: "notes.pdf",
                url: None,
                text: None,
                pages: Some(3),
                duration_s: None,
                est_words: Some(900),
                image_count: None,
                origin_mime: None,
                now_ms: at,
            },
        )
        .unwrap()
}

fn extraction(store: &Store, user: UserId, src: AiSourceId, sha: &str, at: i64) -> i64 {
    store
        .insert_ai_extraction(
            user,
            &AiExtractionNew {
                source_id: src,
                sha256: sha,
                model: "m1",
                prompt_version: 1,
                text: "The heart has four chambers.",
                figures: r#"[{"page":1,"caption":"the heart"}]"#,
                est_words: 5,
                now_ms: at,
            },
        )
        .unwrap()
}

fn job(store: &Store, user: UserId, src: AiSourceId, deck: Option<DeckId>, at: i64) -> AiJobId {
    store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[src],
                deck_id: deck,
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "focus on anatomy",
                now_ms: at,
            },
        )
        .unwrap()
}

fn draft(front: &str) -> AiDraftNew {
    AiDraftNew {
        note_type: NoteType::Basic,
        front_html: sanitize_with_media(front),
        back_html: sanitize_with_media("<b>answer</b>"),
        tags: vec!["anatomy".to_string()],
        figure_media_ids: vec![],
    }
}

fn media(store: &Store, user: UserId, sha: &str, at: i64) -> MediaId {
    store
        .create_media(user, sha, "fig.png", "image/png", MediaKind::Image, 10, at)
        .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn call(
    store: &Store,
    user: UserId,
    job: Option<AiJobId>,
    step: AiCallStep,
    kind: AiSourceKind,
    seconds: u32,
    ok: bool,
    at: i64,
) {
    store
        .insert_ai_call(
            user,
            &AiCallNew {
                job_id: job,
                step,
                model: "m1",
                input_kind: kind,
                input_tokens: 100,
                output_tokens: 50,
                media_seconds: seconds,
                ms: 800,
                ok,
                error: (!ok).then_some("upstream said no"),
                started_at: at,
                ..Default::default()
            },
        )
        .unwrap();
}

fn count(store: &Store, sql: &str) -> i64 {
    store
        .with_conn(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

/// A note kept from `job`, the way the keep path creates one.
fn keep_note(store: &Store, user: UserId, deck: DeckId, job: Option<AiJobId>) -> NoteId {
    let front = sanitize_with_media("Q");
    let back = sanitize_with_media("A");
    let generated = flash_store::notes::generate_cards(NoteType::Basic, &front, &back).unwrap();
    let save = NoteSave {
        note_type: NoteType::Basic,
        front_html: &front,
        back_html: &back,
        generated: &generated,
        tags: &[],
        cap: None,
        ai_job_id: job,
        now_ms: NOW,
    };
    store.create_note(user, deck, &save).unwrap().0
}

#[test]
fn the_schema_reaches_22_with_the_generation_tables() {
    let (store, _) = store_with_user();
    assert_eq!(store.schema_version().unwrap(), 22);
    for table in [
        "ai_sources",
        "ai_extractions",
        "ai_figures",
        "ai_jobs",
        "ai_job_sources",
        "ai_drafts",
        "ai_calls",
        "ai_turns",
    ] {
        assert_eq!(count(&store, &format!("SELECT COUNT(*) FROM {table}")), 0);
    }
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM pragma_table_info('notes') WHERE name = 'ai_job_id'"
        ),
        1
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM pragma_table_info('ai_sources') WHERE name = 'origin_mime'"
        ),
        1
    );
}

#[test]
fn a_job_reads_a_set_of_sources_in_order() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let a = source(&store, user, &"a".repeat(64), NOW);
    let b = source(&store, user, &"b".repeat(64), NOW);
    let c = source(&store, user, &"c".repeat(64), NOW);
    assert_eq!(store.count_unattached_ai_sources(user).unwrap(), 3);
    let newest_first: Vec<AiSourceId> = store
        .list_unattached_ai_sources(user, 10)
        .unwrap()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(newest_first, [c, b, a]);

    let j = store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[b, a],
                deck_id: Some(deck),
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: NOW,
            },
        )
        .unwrap();
    // The first member is the job's own source id; the set is the set.
    assert_eq!(store.get_ai_job(user, j).unwrap().unwrap().source_id, b);
    assert_eq!(store.list_ai_job_source_ids(user, j).unwrap(), [b, a]);
    let rows = store.list_ai_job_sources(user, j).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, b);
    assert_eq!(rows[0].1.as_ref().map(|s| s.id), Some(b));
    assert_eq!(
        rows[1].1.as_ref().map(|s| s.sha256.as_str()),
        Some("a".repeat(64).as_str())
    );
    assert_eq!(store.count_unattached_ai_sources(user).unwrap(), 1);
    assert_eq!(
        store
            .list_unattached_ai_sources(user, 10)
            .unwrap()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        [c]
    );

    // An empty set is refused, and so is one naming another's source.
    assert!(matches!(
        store.create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: NOW,
            },
        ),
        Err(StoreError::Invalid(_))
    ));
    let bob = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let bob_src = source(&store, bob, &"z".repeat(64), NOW);
    assert!(matches!(
        store.create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[c, bob_src],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai source"))
    ));
    assert!(store.list_ai_job_source_ids(bob, j).unwrap().is_empty());
    assert!(store.list_ai_job_sources(bob, j).unwrap().is_empty());
    assert_eq!(store.count_unattached_ai_sources(bob).unwrap(), 1);

    // A member housekeeping took is a hole in the list, not a missing
    // entry.
    store
        .with_conn(|conn| {
            conn.execute("DELETE FROM ai_sources WHERE id = ?1", [a.0])?;
            Ok(())
        })
        .unwrap();
    let rows = store.list_ai_job_sources(user, j).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].1.is_some());
    assert_eq!(rows[1], (a, None));
}

fn turn(
    store: &Store,
    user: UserId,
    root: AiJobId,
    role: AiTurnRole,
    text: &str,
    child: Option<AiJobId>,
) -> i64 {
    store
        .add_ai_turn(
            user,
            &AiTurnNew {
                job_id: root,
                role,
                text,
                child_job_id: child,
                now_ms: NOW,
            },
        )
        .unwrap()
}

#[test]
fn a_review_keeps_its_thread_in_order_with_each_pass_state() {
    let (store, user) = store_with_user();
    let bob = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let src = source(&store, user, &"a".repeat(64), NOW);
    let root = job(&store, user, src, None, NOW);
    let pass = store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[src],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::Revise,
                parent_job_id: Some(root),
                instruction: "shorter",
                now_ms: NOW + 1,
            },
        )
        .unwrap();
    let first = turn(&store, user, root, AiTurnRole::User, "shorter", Some(pass));
    let second = turn(
        &store,
        user,
        root,
        AiTurnRole::Model,
        "Rewrote 2 cards.",
        Some(pass),
    );
    let loose = turn(
        &store,
        user,
        root,
        AiTurnRole::Model,
        "Added 3 cards.",
        None,
    );
    assert!(first < second && second < loose);

    let thread = store.list_ai_turns(user, root).unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(
        (
            thread[0].role,
            thread[0].text.as_str(),
            thread[0].child_job_id
        ),
        (AiTurnRole::User, "shorter", Some(pass))
    );
    assert_eq!(thread[0].child_state, Some(AiJobState::Queued));
    assert_eq!(thread[1].role, AiTurnRole::Model);
    assert_eq!(thread[2].child_job_id, None);
    assert_eq!(thread[2].child_state, None);
    // The pass fails: the turns that ran it say so.
    store
        .set_ai_job_state(user, pass, AiJobState::Failed, 0, Some("no"), NOW + 2)
        .unwrap();
    let thread = store.list_ai_turns(user, root).unwrap();
    assert_eq!(thread[1].child_state, Some(AiJobState::Failed));

    // Another account's root or pass cannot be written to or read.
    assert!(matches!(
        store.add_ai_turn(
            bob,
            &AiTurnNew {
                job_id: root,
                role: AiTurnRole::User,
                text: "x",
                child_job_id: None,
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai job"))
    ));
    let bob_src = source(&store, bob, &"b".repeat(64), NOW);
    let bob_root = job(&store, bob, bob_src, None, NOW);
    assert!(matches!(
        store.add_ai_turn(
            bob,
            &AiTurnNew {
                job_id: bob_root,
                role: AiTurnRole::User,
                text: "x",
                child_job_id: Some(pass),
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai job"))
    ));
    assert!(store.list_ai_turns(bob, root).unwrap().is_empty());
    assert_eq!(store.list_ai_turns(user, root).unwrap().len(), 3);
}

#[test]
fn a_source_can_be_taken_back_until_a_job_reads_it() {
    let (store, user) = store_with_user();
    let bob = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let alone = source(&store, user, &"a".repeat(64), NOW);
    let shared = source(&store, user, &"b".repeat(64), NOW);
    source(&store, bob, &"b".repeat(64), NOW);
    let read = source(&store, user, &"c".repeat(64), NOW);
    let ext = extraction(&store, user, alone, &"a".repeat(64), NOW);
    let fig = media(&store, user, &"f".repeat(64), NOW);
    store.add_ai_figure(user, ext, fig).unwrap();
    job(&store, user, read, None, NOW);

    // The blob nobody else holds comes back as an orphan; the reading
    // and its pins go with the source.
    assert_eq!(
        store.delete_ai_source(user, alone).unwrap(),
        Some("a".repeat(64))
    );
    assert!(store.get_ai_source(user, alone).unwrap().is_none());
    assert!(store.get_ai_extraction(user, ext).unwrap().is_none());
    assert!(store.get_media(user, fig).unwrap().is_some());
    // A blob another account also holds is not an orphan.
    assert_eq!(store.delete_ai_source(user, shared).unwrap(), None);
    // A member of a job stays.
    assert!(matches!(
        store.delete_ai_source(user, read),
        Err(StoreError::Invalid(_))
    ));
    assert!(store.get_ai_source(user, read).unwrap().is_some());
    // Not yours: not found, and untouched.
    let bob_src = source(&store, bob, &"z".repeat(64), NOW);
    assert!(matches!(
        store.delete_ai_source(user, bob_src),
        Err(StoreError::NotFound("ai source"))
    ));
    assert!(store.get_ai_source(bob, bob_src).unwrap().is_some());
    assert!(matches!(
        store.delete_ai_source(user, alone),
        Err(StoreError::NotFound("ai source"))
    ));
}

#[test]
fn source_extraction_job_and_draft_round_trip() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "Cardio", "", NOW).unwrap();
    let sha = "a".repeat(64);

    let src = source(&store, user, &sha, NOW);
    let row = store.get_ai_source(user, src).unwrap().expect("source");
    assert_eq!(row.kind, AiSourceKind::Pdf);
    assert_eq!(
        (row.pages, row.est_words, row.size),
        (Some(3), Some(900), 1234)
    );
    assert_eq!(row.last_used_at, NOW);
    store.touch_ai_source(user, src, NOW + 1).unwrap();
    assert_eq!(
        store
            .get_ai_source(user, src)
            .unwrap()
            .unwrap()
            .last_used_at,
        NOW + 1
    );

    assert!(store
        .find_ai_extraction(user, &sha, "m1", 1)
        .unwrap()
        .is_none());
    let ext = extraction(&store, user, src, &sha, NOW);
    let found = store
        .find_ai_extraction(user, &sha, "m1", 1)
        .unwrap()
        .expect("cache hit");
    assert_eq!(found.id, ext);
    assert_eq!(found.source_id, src);
    assert!(found.figures.contains("the heart"), "figures come back raw");
    assert!(store
        .find_ai_extraction(user, &sha, "m1", 2)
        .unwrap()
        .is_none());
    assert!(store
        .find_ai_extraction(user, &sha, "m2", 1)
        .unwrap()
        .is_none());
    store.touch_ai_extraction(user, ext, NOW + 2).unwrap();
    assert_eq!(
        store
            .get_ai_extraction(user, ext)
            .unwrap()
            .unwrap()
            .last_used_at,
        NOW + 2
    );
    // The same reading twice is a constraint error, not a second row.
    let dup = store.insert_ai_extraction(
        user,
        &AiExtractionNew {
            source_id: src,
            sha256: &sha,
            model: "m1",
            prompt_version: 1,
            text: "again",
            figures: "[]",
            est_words: 1,
            now_ms: NOW,
        },
    );
    assert!(dup.unwrap_err().is_constraint());

    let fig = media(&store, user, &"f".repeat(64), NOW);
    store.add_ai_figure(user, ext, fig).unwrap();
    store.add_ai_figure(user, ext, fig).unwrap();
    assert_eq!(store.ai_figure_media_ids(user, ext).unwrap(), vec![fig]);
    assert!(matches!(
        store.add_ai_figure(user, ext, MediaId(999)),
        Err(StoreError::NotFound("media"))
    ));

    let j = job(&store, user, src, Some(deck), NOW);
    let row = store.get_ai_job(user, j).unwrap().expect("job");
    assert_eq!(row.state, AiJobState::Queued);
    assert_eq!(row.op, AiJobOp::Generate);
    assert_eq!(
        (row.deck_id, row.attempts, row.progress),
        (Some(deck), 0, 0)
    );
    assert_eq!(row.instruction, "focus on anatomy");
    assert_eq!(store.active_ai_job(user).unwrap().unwrap().id, j);
    assert_eq!(store.list_ai_jobs(user, 10).unwrap().len(), 1);

    store.set_ai_job_extraction(user, j, ext).unwrap();
    store
        .set_ai_job_state(user, j, AiJobState::Generating, 40, None, NOW + 3)
        .unwrap();
    let row = store.get_ai_job(user, j).unwrap().unwrap();
    assert_eq!((row.state, row.progress), (AiJobState::Generating, 40));
    assert_eq!(row.extraction_id, Some(ext));
    assert!(row.finished_at.is_none());

    let ids = store
        .replace_ai_drafts(user, j, &[draft("Q1"), draft("Q2")], true, NOW + 4)
        .unwrap();
    assert_eq!(ids.len(), 2);
    let drafts = store.list_ai_drafts(user, j).unwrap();
    assert_eq!(drafts.iter().map(|d| d.ord).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(drafts[0].tags, ["anatomy"]);
    assert_eq!(drafts[0].status, AiDraftStatus::Pending);
    assert_eq!(drafts[0].back_html, "<b>answer</b>");

    let edited = sanitize_with_media("Q1 <i>edited</i>");
    store
        .update_ai_draft_fields(
            user,
            ids[0],
            NoteType::BasicReversed,
            &edited,
            &edited,
            &["x".to_string(), "y".to_string()],
            NOW + 5,
        )
        .unwrap();
    let d = store.get_ai_draft(user, ids[0]).unwrap().unwrap();
    assert_eq!(d.status, AiDraftStatus::Edited);
    assert_eq!(d.note_type, NoteType::BasicReversed);
    assert_eq!(d.tags, ["x", "y"]);
    assert_eq!(d.updated_at, NOW + 5);

    let note = keep_note(&store, user, deck, Some(j));
    store
        .set_ai_draft_status(user, ids[0], AiDraftStatus::Kept, Some(note), NOW + 6)
        .unwrap();
    store
        .set_ai_draft_status(user, ids[1], AiDraftStatus::Discarded, None, NOW + 6)
        .unwrap();
    let counts = store.count_ai_drafts(user, j).unwrap();
    assert_eq!((counts.pending, counts.kept, counts.discarded), (0, 1, 1));
    assert_eq!(
        store.get_ai_draft(user, ids[0]).unwrap().unwrap().note_id,
        Some(note)
    );
    // Editing a decided draft leaves its status alone.
    store
        .update_ai_draft_fields(
            user,
            ids[0],
            NoteType::Basic,
            &edited,
            &edited,
            &[],
            NOW + 7,
        )
        .unwrap();
    assert_eq!(
        store.get_ai_draft(user, ids[0]).unwrap().unwrap().status,
        AiDraftStatus::Kept
    );

    store
        .set_ai_job_state(user, j, AiJobState::Done, 100, None, NOW + 8)
        .unwrap();
    let row = store.get_ai_job(user, j).unwrap().unwrap();
    assert_eq!(row.finished_at, Some(NOW + 8));
    assert!(store.active_ai_job(user).unwrap().is_none());
    store
        .set_ai_job_state(
            user,
            j,
            AiJobState::Failed,
            0,
            Some("the model did not answer"),
            NOW + 9,
        )
        .unwrap();
    assert_eq!(
        store.get_ai_job(user, j).unwrap().unwrap().error.as_deref(),
        Some("the model did not answer")
    );
}

#[test]
fn replacing_drafts_keeps_the_decided_ones_and_appending_continues_the_order() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let src = source(&store, user, &"a".repeat(64), NOW);
    let j = job(&store, user, src, Some(deck), NOW);
    let ids = store
        .replace_ai_drafts(user, j, &[draft("a"), draft("b"), draft("c")], true, NOW)
        .unwrap();
    store
        .set_ai_draft_status(user, ids[0], AiDraftStatus::Kept, None, NOW)
        .unwrap();
    store
        .set_ai_draft_status(user, ids[1], AiDraftStatus::Discarded, None, NOW)
        .unwrap();

    // Regenerate: the pending draft goes, the two decided ones stay, the
    // new ones are numbered after the highest that remains.
    let new = store
        .replace_ai_drafts(user, j, &[draft("d"), draft("e")], true, NOW + 1)
        .unwrap();
    let rows = store.list_ai_drafts(user, j).unwrap();
    assert_eq!(
        rows.iter()
            .map(|d| (d.ord, d.status, d.front_html.as_str()))
            .collect::<Vec<_>>(),
        [
            (0, AiDraftStatus::Kept, "a"),
            (1, AiDraftStatus::Discarded, "b"),
            (2, AiDraftStatus::Pending, "d"),
            (3, AiDraftStatus::Pending, "e"),
        ]
    );
    assert_eq!(new.len(), 2);
    assert!(
        rows.iter().all(|d| d.front_html != "c"),
        "the replaced pending draft is gone"
    );

    // Ask for more: appended after everything.
    store
        .append_ai_drafts(user, j, &[draft("f")], NOW + 2)
        .unwrap();
    let rows = store.list_ai_drafts(user, j).unwrap();
    assert_eq!(
        rows.last().map(|d| (d.ord, d.front_html.as_str())),
        Some((4, "f"))
    );
    let counts = store.count_ai_drafts(user, j).unwrap();
    assert_eq!((counts.pending, counts.kept, counts.discarded), (3, 1, 1));

    // Without keep_kept everything goes.
    store
        .replace_ai_drafts(user, j, &[draft("g")], false, NOW + 3)
        .unwrap();
    let rows = store.list_ai_drafts(user, j).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].ord, rows[0].front_html.as_str()), (0, "g"));
}

#[test]
fn usage_counts_only_successful_generation_steps() {
    let (store, user) = store_with_user();
    let src = source(&store, user, &"a".repeat(64), NOW);
    let j = job(&store, user, src, None, NOW);
    let since = NOW - 7 * DAY;

    call(
        &store,
        user,
        Some(j),
        AiCallStep::Extract,
        AiSourceKind::Youtube,
        120,
        true,
        NOW,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Extract,
        AiSourceKind::Audio,
        60,
        true,
        NOW,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Generate,
        AiSourceKind::Youtube,
        0,
        true,
        NOW,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Revise,
        AiSourceKind::Youtube,
        0,
        true,
        NOW + 1,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::More,
        AiSourceKind::Youtube,
        0,
        false,
        NOW + 2,
    );
    call(
        &store,
        user,
        None,
        AiCallStep::Grade,
        AiSourceKind::Text,
        0,
        true,
        NOW + 3,
    );
    call(
        &store,
        user,
        None,
        AiCallStep::Clarify,
        AiSourceKind::Text,
        0,
        true,
        NOW + 3,
    );
    // A failed video read is free too; last month's work is outside the window.
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Extract,
        AiSourceKind::Video,
        300,
        false,
        NOW,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Generate,
        AiSourceKind::Pdf,
        0,
        true,
        since - 1,
    );
    call(
        &store,
        user,
        Some(j),
        AiCallStep::Extract,
        AiSourceKind::Video,
        30,
        true,
        since - 1,
    );

    let usage = store.ai_usage_since(user, since).unwrap();
    assert_eq!(
        (usage.generations, usage.video_seconds, usage.audio_seconds),
        (2, 120, 60)
    );
    let all = store.ai_usage_since(user, 0).unwrap();
    assert_eq!((all.generations, all.video_seconds), (3, 150));

    // A call may not be booked against someone else's job.
    let other = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let err = store.insert_ai_call(
        other,
        &AiCallNew {
            job_id: Some(j),
            model: "m1",
            started_at: NOW,
            ..Default::default()
        },
    );
    assert!(matches!(err, Err(StoreError::NotFound("ai job"))));
}

#[test]
fn a_call_is_written_and_read_back_newest_first() {
    let (store, user) = store_with_user();
    let first = store
        .insert_ai_call(
            user,
            &AiCallNew {
                step: AiCallStep::Turn,
                model: "gemini-3.5-flash-lite",
                model_version: Some("gemini-3.5-flash-lite-001"),
                response_id: Some("resp-1"),
                input_kind: AiSourceKind::Audio,
                input_tokens: 1_000,
                cached_tokens: 50,
                output_tokens: 200,
                thoughts_tokens: 120,
                media_seconds: 3,
                ms: 400,
                usage_json: Some(r#"{"promptTokenCount":1000}"#),
                started_at: NOW,
                ..Default::default()
            },
        )
        .unwrap();
    let second = store
        .insert_ai_call(
            user,
            &AiCallNew {
                model: "gemini-3.8-flash",
                started_at: NOW + 1,
                ok: false,
                error: Some("http 503"),
                ..Default::default()
            },
        )
        .unwrap();

    let rows = store.ai_calls_for_user(user, 0, NOW + 2, 10, 0).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, second);
    assert_eq!(rows[1].id, first);
    let call = &rows[1];
    assert_eq!(call.step, AiCallStep::Turn);
    assert_eq!(
        call.model_version.as_deref(),
        Some("gemini-3.5-flash-lite-001")
    );
    assert_eq!(call.response_id.as_deref(), Some("resp-1"));
    assert_eq!(
        (
            call.input_tokens,
            call.cached_tokens,
            call.output_tokens,
            call.thoughts_tokens
        ),
        (1_000, 50, 200, 120)
    );
    assert_eq!(
        call.usage_json.as_deref(),
        Some(r#"{"promptTokenCount":1000}"#)
    );
    assert_eq!(store.ai_call_count_for_user(user, 0, NOW + 2).unwrap(), 2);
    assert_eq!(
        store.ai_calls_for_user(user, 0, NOW + 2, 1, 1).unwrap()[0].id,
        first
    );
    // The window's end is exclusive.
    assert_eq!(store.ai_call_count_for_user(user, 0, NOW + 1).unwrap(), 1);

    // Another account sees none of it, and the rows go with the account.
    let bob = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    assert!(store
        .ai_calls_for_user(bob, 0, NOW + 2, 10, 0)
        .unwrap()
        .is_empty());
    store.delete_user_account(user).unwrap();
    let left: i64 = store
        .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM ai_calls", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn the_runner_takes_the_oldest_queued_job_and_requeues_what_a_restart_interrupted() {
    let (store, user) = store_with_user();
    let src = source(&store, user, &"a".repeat(64), NOW);
    let first = job(&store, user, src, None, NOW);
    let second = job(&store, user, src, None, NOW + 1);
    let done = job(&store, user, src, None, NOW + 2);
    store
        .set_ai_job_state(user, done, AiJobState::Done, 100, None, NOW + 3)
        .unwrap();

    assert_eq!(store.next_queued_ai_job().unwrap().unwrap().id, first);
    store
        .set_ai_job_state(user, first, AiJobState::Extracting, 10, None, NOW + 4)
        .unwrap();
    assert_eq!(store.next_queued_ai_job().unwrap().unwrap().id, second);

    // The process restarts while `first` is extracting: back to the queue.
    assert_eq!(store.requeue_interrupted_ai_jobs(NOW + 5).unwrap(), 1);
    let row = store.get_ai_job(user, first).unwrap().unwrap();
    assert_eq!(
        (row.state, row.attempts, row.progress),
        (AiJobState::Queued, 1, 0)
    );
    assert_eq!(store.get_ai_job(user, second).unwrap().unwrap().attempts, 0);
    assert_eq!(
        store.get_ai_job(user, done).unwrap().unwrap().state,
        AiJobState::Done
    );

    // Interrupted again: that is the second time, and the job fails.
    store
        .set_ai_job_state(user, first, AiJobState::Generating, 60, None, NOW + 6)
        .unwrap();
    assert_eq!(store.requeue_interrupted_ai_jobs(NOW + 7).unwrap(), 1);
    let row = store.get_ai_job(user, first).unwrap().unwrap();
    assert_eq!((row.state, row.attempts), (AiJobState::Failed, 2));
    assert_eq!(row.error.as_deref(), Some("interrupted twice"));
    assert_eq!(row.finished_at, Some(NOW + 7));
    assert_eq!(store.requeue_interrupted_ai_jobs(NOW + 8).unwrap(), 0);
    assert_eq!(store.next_queued_ai_job().unwrap().unwrap().id, second);
}

#[test]
fn the_stale_source_sweep_returns_only_hashes_nothing_else_holds() {
    let (store, user) = store_with_user();
    let cutoff = NOW - 30 * DAY;
    let old = cutoff - DAY;

    // Stale, and nothing else names its blob: the hash is an orphan.
    let alone = source(&store, user, &"a".repeat(64), old);
    let alone_ext = extraction(&store, user, alone, &"a".repeat(64), old);
    let fig = media(&store, user, &"f".repeat(64), old);
    store.add_ai_figure(user, alone_ext, fig).unwrap();
    // Stale, but a card's media row shares the blob.
    let shared = source(&store, user, &"b".repeat(64), old);
    media(&store, user, &"b".repeat(64), old);
    // Stale, but another source of the same content is still in use.
    let twin = source(&store, user, &"c".repeat(64), old);
    let twin_live = source(&store, user, &"c".repeat(64), NOW);
    // Recently used itself.
    let fresh = source(&store, user, &"d".repeat(64), NOW);
    // Old itself, but its reading was used from the cache since.
    let cached = source(&store, user, &"e".repeat(64), old);
    let cached_ext = extraction(&store, user, cached, &"e".repeat(64), old);
    store.touch_ai_extraction(user, cached_ext, NOW).unwrap();
    // A job that named the swept source keeps its id.
    let j = job(&store, user, alone, None, old);
    store
        .set_ai_job_state(user, j, AiJobState::Done, 100, None, old)
        .unwrap();
    // Old, but the runner still has to read it.
    let queued = source(&store, user, &"q".repeat(64), old);
    let queued_job = job(&store, user, queued, None, old);
    // A member of a set whose reading was used recently.
    let member = source(&store, user, &"m".repeat(64), old);
    let lead = source(&store, user, &"l".repeat(64), old);
    let set_job = store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[lead, member],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: old,
            },
        )
        .unwrap();
    let set_ext = extraction(&store, user, lead, &"lm".repeat(32), NOW);
    store.set_ai_job_extraction(user, set_job, set_ext).unwrap();
    store
        .set_ai_job_state(user, set_job, AiJobState::Done, 100, None, old)
        .unwrap();
    // Never attached to a job: a shorter leash.
    let leftover = source(&store, user, &"n".repeat(64), NOW - 3 * DAY);
    let recent_leftover = source(&store, user, &"o".repeat(64), NOW - DAY);

    let mut orphans = store.sweep_stale_ai_sources(cutoff, NOW - 2 * DAY).unwrap();
    orphans.sort();
    assert_eq!(orphans, vec!["a".repeat(64), "n".repeat(64)]);

    for gone in [alone, shared, twin, leftover] {
        assert!(store.get_ai_source(user, gone).unwrap().is_none());
    }
    for kept in [
        twin_live,
        fresh,
        cached,
        queued,
        member,
        lead,
        recent_leftover,
    ] {
        assert!(
            store.get_ai_source(user, kept).unwrap().is_some(),
            "{kept:?} was swept"
        );
    }
    assert!(store.get_ai_extraction(user, alone_ext).unwrap().is_none());
    assert!(store.get_ai_extraction(user, cached_ext).unwrap().is_some());
    assert_eq!(store.get_ai_job(user, j).unwrap().unwrap().source_id, alone);
    assert_eq!(store.list_ai_job_source_ids(user, j).unwrap(), [alone]);
    // Once the runner is done with it, the old queued source goes too.
    store
        .set_ai_job_state(user, queued_job, AiJobState::Done, 100, None, NOW)
        .unwrap();
    assert_eq!(
        store.sweep_stale_ai_sources(cutoff, NOW - 2 * DAY).unwrap(),
        vec!["q".repeat(64)]
    );

    // The figure crop lost its pin: it is still a media row, and the
    // unlinked-media sweep takes it from here.
    assert!(store.get_media(user, fig).unwrap().is_some());
    // (The old media row that shared "b" with the swept source is
    // unlinked too, so the sweep takes both.)
    let mut swept = store.sweep_unlinked_media(NOW).unwrap();
    swept.sort();
    assert_eq!(swept, vec!["b".repeat(64), "f".repeat(64)]);
    assert!(store.get_media(user, fig).unwrap().is_none());
    // Nothing left to do.
    assert!(store
        .sweep_stale_ai_sources(cutoff, NOW - 2 * DAY)
        .unwrap()
        .is_empty());
}

#[test]
fn a_pinned_figure_survives_the_unlinked_media_sweep_and_a_deck_deletion() {
    let (store, user) = store_with_user();
    let old = NOW - 2 * DAY;
    let src = source(&store, user, &"a".repeat(64), old);
    let ext = extraction(&store, user, src, &"a".repeat(64), old);
    let fig = media(&store, user, &"f".repeat(64), old);
    let loose = media(&store, user, &"g".repeat(64), old);
    store.add_ai_figure(user, ext, fig).unwrap();

    assert_eq!(
        store.sweep_unlinked_media(NOW).unwrap(),
        vec!["g".repeat(64)]
    );
    assert!(store.get_media(user, fig).unwrap().is_some());
    assert!(store.get_media(user, loose).unwrap().is_none());

    // Deleting the deck a job targets nulls the job's deck and the
    // kept draft's note, and leaves the pinned crop alone.
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let j = job(&store, user, src, Some(deck), NOW);
    let ids = store
        .replace_ai_drafts(user, j, &[draft("q")], true, NOW)
        .unwrap();
    let note = keep_note(&store, user, deck, Some(j));
    store
        .set_ai_draft_status(user, ids[0], AiDraftStatus::Kept, Some(note), NOW)
        .unwrap();
    store.delete_deck(user, deck).unwrap();
    assert_eq!(store.get_ai_job(user, j).unwrap().unwrap().deck_id, None);
    assert_eq!(
        store.get_ai_draft(user, ids[0]).unwrap().unwrap().note_id,
        None
    );
    assert!(store.get_media(user, fig).unwrap().is_some());

    // A user deleting the crop's media row unpins it first.
    assert_eq!(store.delete_media(user, fig).unwrap(), 0);
    assert!(store.ai_figure_media_ids(user, ext).unwrap().is_empty());
}

#[test]
fn a_kept_note_leads_back_to_its_reading() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let src = source(&store, user, &"a".repeat(64), NOW);
    let ext = extraction(&store, user, src, &"a".repeat(64), NOW);
    let j = job(&store, user, src, Some(deck), NOW);
    store.set_ai_job_extraction(user, j, ext).unwrap();

    let generated = keep_note(&store, user, deck, Some(j));
    let by_hand = keep_note(&store, user, deck, None);
    assert_eq!(store.note_ai_job(user, generated).unwrap(), Some(j));
    assert_eq!(store.note_ai_job(user, by_hand).unwrap(), None);
    assert_eq!(
        store.get_note(user, generated).unwrap().unwrap().ai_job_id,
        Some(j)
    );
    let reading = store
        .ai_extraction_for_note(user, generated)
        .unwrap()
        .expect("lineage");
    assert_eq!(reading.id, ext);
    assert_eq!(reading.text, "The heart has four chambers.");
    assert!(store
        .ai_extraction_for_note(user, by_hand)
        .unwrap()
        .is_none());

    // The lineage is the job's; a job not the user's cannot be claimed.
    let other = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let other_deck = store.create_deck(other, "D", "", NOW).unwrap();
    let front = sanitize_with_media("Q");
    let generated_cards =
        flash_store::notes::generate_cards(NoteType::Basic, &front, &front).unwrap();
    let save = NoteSave {
        note_type: NoteType::Basic,
        front_html: &front,
        back_html: &front,
        generated: &generated_cards,
        tags: &[],
        cap: None,
        ai_job_id: Some(j),
        now_ms: NOW,
    };
    assert!(matches!(
        store.create_note(other, other_deck, &save),
        Err(StoreError::NotFound("ai job"))
    ));
}

#[test]
fn deleting_the_account_takes_every_generation_row_and_reports_the_source_blob() {
    let (store, user) = store_with_user();
    let other = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let src = source(&store, user, &"a".repeat(64), NOW);
    // A second source whose blob the survivor also holds.
    source(&store, user, &"s".repeat(64), NOW);
    let ext = extraction(&store, user, src, &"a".repeat(64), NOW);
    let fig = media(&store, user, &"f".repeat(64), NOW);
    store.add_ai_figure(user, ext, fig).unwrap();
    let j = job(&store, user, src, Some(deck), NOW);
    store.set_ai_job_extraction(user, j, ext).unwrap();
    let child = store
        .create_ai_job(
            user,
            &AiJobNew {
                source_ids: &[src],
                deck_id: Some(deck),
                extraction_id: Some(ext),
                op: AiJobOp::More,
                parent_job_id: Some(j),
                instruction: "",
                now_ms: NOW + 1,
            },
        )
        .unwrap();
    let ids = store
        .replace_ai_drafts(user, child, &[draft("q")], true, NOW)
        .unwrap();
    let note = keep_note(&store, user, deck, Some(child));
    store
        .set_ai_draft_status(user, ids[0], AiDraftStatus::Kept, Some(note), NOW)
        .unwrap();
    call(
        &store,
        user,
        Some(child),
        AiCallStep::Generate,
        AiSourceKind::Pdf,
        0,
        true,
        NOW,
    );
    turn(&store, user, j, AiTurnRole::User, "shorter", Some(child));
    turn(
        &store,
        user,
        j,
        AiTurnRole::Model,
        "Rewrote 1 card.",
        Some(child),
    );
    // The survivor holds one of the hashes as a source of their own.
    source(&store, other, &"s".repeat(64), NOW);
    let bob_src = source(&store, other, &"b".repeat(64), NOW);
    let bob_job = job(&store, other, bob_src, None, NOW);
    turn(&store, other, bob_job, AiTurnRole::User, "mine", None);

    let mut deleted = store.delete_user_account(user).unwrap();
    deleted.orphan_blobs.sort();
    assert_eq!(deleted.orphan_blobs, vec!["a".repeat(64), "f".repeat(64)]);

    for table in [
        "ai_sources",
        "ai_extractions",
        "ai_figures",
        "ai_jobs",
        "ai_job_sources",
        "ai_turns",
        "ai_drafts",
        "ai_calls",
        "notes",
    ] {
        assert_eq!(
            count(
                &store,
                &format!(
                    "SELECT COUNT(*) FROM {table} WHERE user_id = {}",
                    user.raw()
                )
            ),
            0,
            "{table} still has the deleted user's rows"
        );
    }
    assert_eq!(count(&store, "SELECT COUNT(*) FROM ai_sources"), 2);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM ai_job_sources"), 1);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM ai_turns"), 1);
    assert_eq!(
        store.get_ai_job(other, bob_job).unwrap().unwrap().source_id,
        bob_src
    );
}

#[test]
fn deleting_an_account_with_hand_made_notes_works() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    keep_note(&store, user, deck, None);
    store.delete_user_account(user).unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM notes"), 0);
}

#[test]
fn one_user_never_sees_or_touches_anothers_rows() {
    let (store, ada) = store_with_user();
    let bob = store
        .create_user("Bob", Some("bob@example.com"), "member", NOW)
        .unwrap();
    let deck = store.create_deck(ada, "D", "", NOW).unwrap();
    let src = source(&store, ada, &"a".repeat(64), NOW);
    let ext = extraction(&store, ada, src, &"a".repeat(64), NOW);
    let fig = media(&store, ada, &"f".repeat(64), NOW);
    store.add_ai_figure(ada, ext, fig).unwrap();
    let j = job(&store, ada, src, Some(deck), NOW);
    let ids = store
        .replace_ai_drafts(ada, j, &[draft("q")], true, NOW)
        .unwrap();
    call(
        &store,
        ada,
        Some(j),
        AiCallStep::Generate,
        AiSourceKind::Pdf,
        0,
        true,
        NOW,
    );
    let note = keep_note(&store, ada, deck, Some(j));

    // Reads answer as if nothing existed.
    assert!(store.get_ai_source(bob, src).unwrap().is_none());
    assert!(store
        .find_ai_extraction(bob, &"a".repeat(64), "m1", 1)
        .unwrap()
        .is_none());
    assert!(store.get_ai_extraction(bob, ext).unwrap().is_none());
    assert!(store.ai_figure_media_ids(bob, ext).unwrap().is_empty());
    assert!(store.get_ai_job(bob, j).unwrap().is_none());
    assert!(store.list_ai_jobs(bob, 10).unwrap().is_empty());
    assert!(store.active_ai_job(bob).unwrap().is_none());
    assert!(store.list_ai_drafts(bob, j).unwrap().is_empty());
    assert!(store.get_ai_draft(bob, ids[0]).unwrap().is_none());
    assert_eq!(store.count_ai_drafts(bob, j).unwrap().pending, 0);
    assert_eq!(store.ai_usage_since(bob, 0).unwrap().generations, 0);
    assert!(store.note_ai_job(bob, note).unwrap().is_none());
    assert!(store.ai_extraction_for_note(bob, note).unwrap().is_none());

    // Writes refuse.
    assert!(matches!(
        store.touch_ai_source(bob, src, NOW),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.touch_ai_extraction(bob, ext, NOW),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.add_ai_figure(bob, ext, fig),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.insert_ai_extraction(
            bob,
            &AiExtractionNew {
                source_id: src,
                sha256: &"z".repeat(64),
                model: "m1",
                prompt_version: 1,
                text: "",
                figures: "[]",
                est_words: 0,
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai source"))
    ));
    assert!(matches!(
        store.create_ai_job(
            bob,
            &AiJobNew {
                source_ids: &[src],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai source"))
    ));
    let bob_src = source(&store, bob, &"b".repeat(64), NOW);
    assert!(matches!(
        store.create_ai_job(
            bob,
            &AiJobNew {
                source_ids: &[bob_src],
                deck_id: Some(deck),
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: "",
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("deck"))
    ));
    assert!(matches!(
        store.create_ai_job(
            bob,
            &AiJobNew {
                source_ids: &[bob_src],
                deck_id: None,
                extraction_id: None,
                op: AiJobOp::More,
                parent_job_id: Some(j),
                instruction: "",
                now_ms: NOW,
            },
        ),
        Err(StoreError::NotFound("ai job"))
    ));
    assert!(matches!(
        store.set_ai_job_state(bob, j, AiJobState::Done, 100, None, NOW),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.set_ai_job_extraction(bob, j, ext),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.replace_ai_drafts(bob, j, &[draft("x")], true, NOW),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.append_ai_drafts(bob, j, &[draft("x")], NOW),
        Err(StoreError::NotFound(_))
    ));
    let html = sanitize_with_media("x");
    assert!(matches!(
        store.update_ai_draft_fields(bob, ids[0], NoteType::Basic, &html, &html, &[], NOW),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.set_ai_draft_status(bob, ids[0], AiDraftStatus::Kept, None, NOW),
        Err(StoreError::NotFound(_))
    ));
    // Bob's own draft may not claim Ada's note.
    let bob_job = job(&store, bob, bob_src, None, NOW);
    let bob_ids = store
        .replace_ai_drafts(bob, bob_job, &[draft("y")], true, NOW)
        .unwrap();
    assert!(matches!(
        store.set_ai_draft_status(bob, bob_ids[0], AiDraftStatus::Kept, Some(note), NOW),
        Err(StoreError::NotFound("note"))
    ));

    // Ada's rows are exactly as she left them.
    assert_eq!(store.list_ai_drafts(ada, j).unwrap().len(), 1);
    assert_eq!(
        store.get_ai_draft(ada, ids[0]).unwrap().unwrap().status,
        AiDraftStatus::Pending
    );
    assert_eq!(
        store.get_ai_job(ada, j).unwrap().unwrap().state,
        AiJobState::Queued
    );
}
