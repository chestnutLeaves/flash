//! The schema registry: a fresh database records the core's steps and
//! reopens as a no-op; an extension's steps run once, in order, and are
//! recorded under its name; a pre-registry database at the last
//! single-binary schema is adopted, any other pre-registry version is
//! refused.

use std::sync::Arc;

use flash_store::ext::{Migration, StoreExtension};
use flash_store::{NextPair, RefreshOutcome, Store, MONOLITH_MARKER, REFRESH_GRACE_MS};
use rusqlite::Connection;

fn temp_db(tag: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("flash-migrations-{tag}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

struct Widgets;

const WIDGET_STEPS: &[Migration] = &[
    Migration {
        name: "0001_widgets",
        sql: "CREATE TABLE widgets (id INTEGER PRIMARY KEY, name TEXT NOT NULL) STRICT;",
    },
    Migration {
        name: "0002_widget_color",
        sql: "ALTER TABLE widgets ADD COLUMN color TEXT;",
    },
];

impl StoreExtension for Widgets {
    fn name(&self) -> &'static str {
        "widgets"
    }
    fn migrations(&self) -> &'static [Migration] {
        WIDGET_STEPS
    }
}

fn names(store: &Store, extension: &str) -> Vec<String> {
    store
        .applied_migrations()
        .unwrap()
        .into_iter()
        .filter(|(e, _, _)| e == extension)
        .map(|(_, n, _)| n)
        .collect()
}

#[test]
fn fresh_database_records_the_core_steps_and_reopens_quietly() {
    let path = temp_db("fresh");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 49);
    let core = names(&store, "core");
    assert_eq!(core.len(), 49);
    assert_eq!(core[46], "v047_refresh_rotation");
    assert_eq!(core[47], "v048_note_dropped_slots");
    assert_eq!(core[48], "v049_study_index");
    assert_eq!(core[40], "v041_reserved");
    assert_eq!(core[41], "v042_reserved");
    assert_eq!(core[42], "v043_reserved");
    assert_eq!(core[43], "v044_account_locale");
    assert_eq!(core[44], "v045_deck_goal");
    assert_eq!(core[45], "v046_user_id_floor");
    assert_eq!(core[23], "v024_mcp_confirmation_subtree");
    assert_eq!(core[24], "v025_speech");
    assert_eq!(core[25], "v026_card_tags_by_tag");
    assert_eq!(core[26], "v027_devices");
    assert_eq!(core[28], "v029_jobs");
    assert_eq!(core[29], "v030_import_commits");
    assert_eq!(core[30], "v031_durable_ceremonies");
    assert_eq!(core[31], "v032_due_by_day");
    assert_eq!(core[32], "v033_reserved");
    assert_eq!(core[33], "v034_note_revisions");
    assert_eq!(core[34], "v035_study_measures");
    assert_eq!(core[35], "v036_id_floor");
    assert_eq!(core[36], "v037_client_errors");
    assert_eq!(core[0], "v001_initial");
    assert_eq!(core[13], "v014_reserved");
    assert_eq!(core[14], "v015_review_client");
    assert_eq!(core[15], "v016_ai_generation");
    assert_eq!(core[16], "v017_ai_source_sets");
    assert_eq!(core[17], "v018_ai_turns");
    assert!(!core.iter().any(|n| n == MONOLITH_MARKER));
    let before = store.applied_migrations().unwrap();
    drop(store);

    let again = Store::open(&path).unwrap();
    assert_eq!(again.applied_migrations().unwrap(), before);
}

/// What a command run beside the server opens: a database the server
/// already upgraded. A missing file or an older schema is refused and
/// left exactly as it was.
#[test]
fn open_current_refuses_a_missing_or_older_database() {
    let path = temp_db("current");
    assert!(Store::open_current(&path).is_err());
    assert!(!path.exists(), "a missing database is not created");

    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", 22)
        .unwrap();
    let err = Store::open_current(&path).err().unwrap().to_string();
    assert!(err.contains("schema 22"), "{err}");
    let version: i64 = Connection::open(&path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 22, "an older database is not migrated");

    std::fs::remove_file(&path).unwrap();
    drop(Store::open(&path).unwrap());
    assert!(Store::open_current(&path).is_ok());
}

#[test]
fn extension_steps_run_once_in_order_and_are_recorded() {
    let path = temp_db("widgets");
    let store = Store::open_with(&path, vec![Arc::new(Widgets)]).unwrap();
    assert_eq!(
        names(&store, "widgets"),
        ["0001_widgets", "0002_widget_color"]
    );
    store
        .with_conn(|conn| {
            conn.execute("INSERT INTO widgets (name, color) VALUES ('a', 'red')", [])?;
            Ok(())
        })
        .unwrap();
    let before = store.applied_migrations().unwrap();
    drop(store);

    // Reopening with the extension changes nothing; opening without it
    // leaves its tables alone.
    let again = Store::open_with(&path, vec![Arc::new(Widgets)]).unwrap();
    assert_eq!(again.applied_migrations().unwrap(), before);
    drop(again);
    let core_only = Store::open(&path).unwrap();
    let n: i64 = core_only
        .with_conn(|conn| Ok(conn.query_row("SELECT COUNT(*) FROM widgets", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(n, 1);
}

/// A database that predates the registry, at the last single-binary
/// schema: its core steps are adopted, not run, an extension's baseline
/// is adopted the same way, and the core steps added since then run.
#[test]
fn a_schema_14_database_without_a_registry_is_adopted() {
    let path = temp_db("adopt");
    // The core's own first 14 steps stand in for the monolith here (the
    // exact legacy history is exercised where that history lives):
    // build a fresh database, then wind it back to what a schema-14
    // monolith left behind.
    {
        let first = Store::open(&path).unwrap();
        drop(first);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "DROP TABLE schema_migrations;
             DROP INDEX idx_cards_study;
             ALTER TABLE notes DROP COLUMN dropped_slots;
             DROP TABLE oauth_spent_refresh;
             DROP TRIGGER users_id_floor;
             DROP TRIGGER users_id_fresh;
             ALTER TABLE decks DROP COLUMN goal_cards;
             ALTER TABLE decks DROP COLUMN goal_pace;
             ALTER TABLE decks DROP COLUMN goal_date;
             DROP TABLE account_locales;
             ALTER TABLE user_settings DROP COLUMN locale_source;
             ALTER TABLE user_settings DROP COLUMN locale;
             ALTER TABLE notes DROP COLUMN front_lang;
             ALTER TABLE notes DROP COLUMN back_lang;
             ALTER TABLE notes DROP COLUMN front_speech;
             ALTER TABLE notes DROP COLUMN back_speech;
             ALTER TABLE notes DROP COLUMN speech_by;
             ALTER TABLE cards DROP COLUMN front_speech;
             ALTER TABLE cards DROP COLUMN back_speech;
             ALTER TABLE cards DROP COLUMN speech_by;
             DROP TABLE client_errors;
             DROP TABLE note_revisions;
             ALTER TABLE user_settings DROP COLUMN due_by_day;
             DROP TABLE server_state;
             DROP TABLE mcp_sessions;
             DROP TABLE ceremonies;
             DROP TABLE import_commits;
             DROP TABLE jobs;
             ALTER TABLE oauth_tokens DROP COLUMN install_id;
             DROP INDEX idx_sessions_user;
             ALTER TABLE sessions DROP COLUMN label;
             ALTER TABLE sessions DROP COLUMN last_used_at;
             DROP INDEX idx_card_tags_tag;
             ALTER TABLE decks DROP COLUMN speech_language_hints;
             DROP TRIGGER mcp_confirmation_card_deleted;
             DROP TRIGGER mcp_confirmation_deck_deleted;
             DROP TABLE mcp_confirmations;
             DROP INDEX idx_review_import_once;
             DROP TABLE voice_clip_refs;
             DROP TABLE voice_clips;
             ALTER TABLE user_settings DROP COLUMN voice_id;
             ALTER TABLE user_settings DROP COLUMN speech_rate;
             ALTER TABLE user_settings DROP COLUMN voice_apply;
             ALTER TABLE user_settings DROP COLUMN sound_cues;
             ALTER TABLE review_log DROP COLUMN client;
             ALTER TABLE review_log DROP COLUMN answer_ms;
             ALTER TABLE study_sessions DROP COLUMN surface;
             ALTER TABLE study_sessions DROP COLUMN reviews;
             ALTER TABLE study_sessions DROP COLUMN last_review_at;
             ALTER TABLE user_settings DROP COLUMN timezone_source;
             ALTER TABLE notes DROP COLUMN ai_job_id;
             ALTER TABLE decks DROP COLUMN archived_at;
             DROP TABLE ai_calls;
             DROP TABLE ai_drafts;
             DROP TABLE ai_turns;
             DROP TABLE ai_job_sources;
             DROP TABLE ai_jobs;
             DROP TABLE ai_figures;
             DROP TABLE ai_extractions;
             DROP TABLE ai_sources;
             DROP TRIGGER decks_id_floor;
             DROP TRIGGER notes_id_floor;
             DROP TRIGGER cards_id_floor;
             DROP TRIGGER decks_id_fresh;
             DROP TRIGGER notes_id_fresh;
             DROP TRIGGER cards_id_fresh;
             DROP TABLE id_floor;
             PRAGMA user_version = 14;",
        )
        .unwrap();
    }
    struct Baseline;
    const BASELINE: &[Migration] = &[Migration {
        name: "0001_baseline",
        sql: "CREATE TABLE never_created (id INTEGER PRIMARY KEY) STRICT;",
    }];
    impl StoreExtension for Baseline {
        fn name(&self) -> &'static str {
            "ext"
        }
        fn migrations(&self) -> &'static [Migration] {
            BASELINE
        }
        fn legacy_baseline(&self) -> usize {
            1
        }
    }
    let store = Store::open_with(&path, vec![Arc::new(Baseline)]).unwrap();
    let core = names(&store, "core");
    assert!(core.iter().any(|n| n == MONOLITH_MARKER));
    // The marker, the 14 adopted steps, and v015 through v049, each
    // applied once.
    assert_eq!(core.len(), 50);
    assert_eq!(core.iter().filter(|n| *n == "v049_study_index").count(), 1);
    assert_eq!(
        core.iter().filter(|n| *n == "v015_review_client").count(),
        1
    );
    assert_eq!(
        core.iter().filter(|n| *n == "v016_ai_generation").count(),
        1
    );
    assert_eq!(
        core.iter().filter(|n| *n == "v017_ai_source_sets").count(),
        1
    );
    assert_eq!(core.iter().filter(|n| *n == "v018_ai_turns").count(), 1);
    assert_eq!(core.iter().filter(|n| *n == "v019_voice").count(), 1);
    assert_eq!(
        core.iter().filter(|n| *n == "v020_import_history").count(),
        1
    );
    assert_eq!(
        core.iter()
            .filter(|n| *n == "v021_ai_ledger_detail")
            .count(),
        1
    );
    assert_eq!(core.iter().filter(|n| *n == "v022_deck_archive").count(), 1);
    assert_eq!(
        core.iter()
            .filter(|n| *n == "v024_mcp_confirmation_subtree")
            .count(),
        1
    );
    assert_eq!(store.schema_version().unwrap(), 49);
    assert_eq!(names(&store, "ext"), ["0001_baseline"]);
    // Adopted, not executed.
    let exists: bool = store
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'never_created')",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(!exists);
}

/// v047: a grant from before refresh tokens were remembered rotates as
/// before, and the token it rotated from is caught when it comes back.
#[test]
fn v047_a_grant_from_before_it_rotates_with_reuse_detection() {
    const NOW: i64 = 1_700_000_000_000;
    let path = temp_db("v047");
    {
        let store = Store::open(&path).unwrap();
        let user = store
            .create_user("FlashTester", None, "member", NOW)
            .unwrap();
        store
            .insert_oauth_token(
                "a0",
                "r0",
                "flash-mobile",
                user,
                "mobile",
                "iPhone",
                None,
                NOW + 3_600_000,
                NOW + 86_400_000,
                NOW,
            )
            .unwrap();
        drop(store);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "DROP INDEX idx_cards_study;
             DELETE FROM schema_migrations WHERE name = 'v049_study_index';
             DROP TABLE oauth_spent_refresh;
             DELETE FROM schema_migrations WHERE name = 'v047_refresh_rotation';
             ALTER TABLE notes DROP COLUMN dropped_slots;
             DELETE FROM schema_migrations WHERE name = 'v048_note_dropped_slots';
             PRAGMA user_version = 46;",
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), 49);
    let present = |old: &str, new: &str, now: i64| {
        store
            .rotate_refresh_token(
                old,
                None,
                &NextPair {
                    access_hash: "a1",
                    refresh_hash: new,
                    salt: "s1",
                    access_expires_ms: now + 1000,
                    refresh_expires_ms: now + 2000,
                },
                now,
            )
            .unwrap()
    };
    let rotated = present("r0", "r1", NOW + 10);
    assert!(matches!(rotated, RefreshOutcome::Rotated(_)), "{rotated:?}");
    let reused = present("r0", "rx", NOW + 10 + REFRESH_GRACE_MS + 1);
    assert!(matches!(reused, RefreshOutcome::Reused(_)), "{reused:?}");
}

#[test]
fn an_older_pre_registry_database_is_refused() {
    let path = temp_db("refuse");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY); PRAGMA user_version = 12;")
        .unwrap();
    drop(conn);
    let err = Store::open(&path).err().expect("refused");
    assert!(err.to_string().contains("schema version 12"), "{err}");
}

/// Servers of the 0.2.0 and 0.3.0 releases wrote rows into these tables.
#[test]
fn rows_in_the_tables_of_steps_16_to_19_never_block_a_deletion() {
    use flash_store::notes::{generate_cards, NoteType};
    use flash_store::richtext::sanitize_with_media;
    const NOW: i64 = 1_700_000_000_000;
    let (figure, source) = ("f".repeat(64), "a".repeat(64));
    let store = Store::open(&temp_db("legacy-rows")).unwrap();
    let ada = store
        .create_user("FlashTester", None, "member", NOW)
        .unwrap();
    let deck = store.create_deck(ada, "D", "", NOW).unwrap();
    store
        .with_conn(|c| {
            c.execute_batch(&format!(
                "INSERT INTO media (id, user_id, sha256, filename, mime, kind, size, created_at)
                 VALUES (1, {u}, '{figure}', 'f.png', 'image/png', 'image', 3, 0);
                 INSERT INTO ai_sources (id, user_id, kind, sha256, created_at, last_used_at)
                 VALUES (1, {u}, 'text', '{source}', 0, 0);
                 INSERT INTO ai_extractions (id, user_id, source_id, sha256, model,
                   prompt_version, text, created_at, last_used_at)
                 VALUES (1, {u}, 1, '{source}', 'm', 1, 't', 0, 0);
                 INSERT INTO ai_figures (user_id, extraction_id, media_id) VALUES ({u}, 1, 1);
                 INSERT INTO ai_jobs (id, user_id, source_id, deck_id, op, created_at, updated_at)
                 VALUES (1, {u}, 1, {d}, 'revise', 0, 0);
                 INSERT INTO ai_job_sources (user_id, job_id, source_id, ord) VALUES ({u}, 1, 1, 0);
                 INSERT INTO ai_turns (user_id, job_id, role, text, created_at)
                 VALUES ({u}, 1, 'user', 't', 0);
                 INSERT INTO ai_calls (user_id, job_id, step, model, input_kind, started_at)
                 VALUES ({u}, 1, 'extract', 'm', 'text', 0);
                 INSERT INTO voice_clips (id, key, kind, voice, model, mime, size, duration_ms,
                   envelope, created_at)
                 VALUES (1, 'k', 'card', 'v', 'm', 'x', 1, 1, x'00', 0);
                 INSERT INTO voice_clip_refs (user_id, card_id, side, clip_id, created_at,
                   last_used_at)
                 VALUES ({u}, 1, 'front', 1, 0, 0);",
                u = ada.raw(),
                d = deck.0,
            ))?;
            Ok(())
        })
        .unwrap();
    let front = sanitize_with_media("Q");
    let generated = generate_cards(NoteType::Basic, &front, &front).unwrap();
    let (note, _) = store
        .create_note(
            ada,
            deck,
            &flash_store::NoteSave {
                note_type: NoteType::Basic,
                front_html: &front,
                back_html: &front,
                generated: &generated,
                tags: &[],
                cap: None,
                origin: Some(1),
                speech: Default::default(),
                langs: None,
                now_ms: NOW,
            },
        )
        .unwrap();
    let rows = |sql: &str| -> i64 {
        store
            .with_conn(|c| Ok(c.query_row(sql, [], |r| r.get(0))?))
            .unwrap()
    };
    store
        .with_conn(|c| {
            c.execute(
                "INSERT INTO ai_drafts (user_id, job_id, ord, note_type, front_html,
                   note_id, created_at, updated_at)
                 VALUES (?1, 1, 0, 'basic', 'Q', ?2, 0, 0)",
                rusqlite::params![ada.raw(), note.0],
            )?;
            Ok(())
        })
        .unwrap();

    store.delete_deck(ada, deck).unwrap();
    assert_eq!(
        rows("SELECT COUNT(*) FROM ai_jobs WHERE deck_id IS NULL"),
        1
    );
    assert_eq!(
        rows("SELECT COUNT(*) FROM ai_drafts WHERE note_id IS NULL"),
        1
    );
    store.sweep_unlinked_media(NOW).unwrap();
    assert_eq!(
        rows("SELECT COUNT(*) FROM media"),
        1,
        "a pinned row keeps it"
    );

    let gone = store.delete_user_account(ada).unwrap();
    assert_eq!(gone.orphan_blobs, [source, figure]);
    for table in [
        "ai_jobs",
        "ai_sources",
        "ai_figures",
        "voice_clip_refs",
        "media",
    ] {
        assert_eq!(rows(&format!("SELECT COUNT(*) FROM {table}")), 0, "{table}");
    }
}
