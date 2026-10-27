//! Schema history. The core's steps are keyed on PRAGMA user_version and
//! run once, in order, each in a transaction; an extension's steps are
//! keyed by name in `schema_migrations`, which every open creates. Never
//! edit a shipped step. Append.
//!
//! A database from before the registry existed (the single-binary era,
//! schema 14) is recognised by its version and an empty registry: its
//! rows are recorded as applied without running anything, under the
//! marker `('core', "monolith_v14")`, and each extension then adopts its
//! own leading `legacy_baseline()` steps the same way. An extension's
//! step whose `adopt_if` query finds what it makes already there is
//! adopted too.

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::{params, Connection};

use crate::ext::{Migration, StoreExtension};
use crate::{Result, StoreError};

/// The registry row a pre-registry database receives when first opened.
pub const MONOLITH_MARKER: &str = "monolith_v14";
/// The only pre-registry schema a database may arrive at.
const LAST_MONOLITH_VERSION: i64 = 14;

const CORE: &[Migration] = &[
    Migration {
        name: "v001_initial",
        sql: V001_INITIAL,
    },
    Migration {
        name: "v002_password_auth",
        sql: V002_PASSWORD_AUTH,
    },
    Migration {
        name: "v003_reserved",
        sql: V003_RESERVED,
    },
    Migration {
        name: "v004_media",
        sql: V004_MEDIA,
    },
    Migration {
        name: "v005_rich_cards",
        sql: V005_RICH_CARDS,
    },
    Migration {
        name: "v006_limits",
        sql: V006_LIMITS,
    },
    Migration {
        name: "v007_connect_cta",
        sql: V007_CONNECT_CTA,
    },
    Migration {
        name: "v008_account_deletion",
        sql: V008_ACCOUNT_DELETION,
    },
    Migration {
        name: "v009_notes",
        sql: V009_NOTES,
    },
    Migration {
        name: "v010_reserved",
        sql: V010_RESERVED,
    },
    Migration {
        name: "v011_reserved",
        sql: V011_RESERVED,
    },
    Migration {
        name: "v012_mobile",
        sql: V012_MOBILE,
    },
    Migration {
        name: "v013_reserved",
        sql: V013_RESERVED,
    },
    Migration {
        name: "v014_reserved",
        sql: V014_RESERVED,
    },
    Migration {
        name: "v015_review_client",
        sql: V015_REVIEW_CLIENT,
    },
    Migration {
        name: "v016_ai_generation",
        sql: V016_AI_GENERATION,
    },
    Migration {
        name: "v017_ai_source_sets",
        sql: V017_AI_SOURCE_SETS,
    },
    Migration {
        name: "v018_ai_turns",
        sql: V018_AI_TURNS,
    },
    Migration {
        name: "v019_voice",
        sql: V019_VOICE,
    },
    Migration {
        name: "v020_import_history",
        sql: V020_IMPORT_HISTORY,
    },
    Migration {
        name: "v021_ai_ledger_detail",
        sql: V021_AI_LEDGER_DETAIL,
    },
    Migration {
        name: "v022_deck_archive",
        sql: V022_DECK_ARCHIVE,
    },
    Migration {
        name: "v023_mcp_confirmations",
        sql: "CREATE TABLE mcp_confirmations (
          token_hash TEXT PRIMARY KEY,
          user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
          client_id TEXT NOT NULL,
          target TEXT NOT NULL,
          deck_id INTEGER,
          card_ids TEXT NOT NULL,
          expires_ms INTEGER NOT NULL,
          invalidated INTEGER NOT NULL DEFAULT 0 CHECK (invalidated IN (0, 1)),
          result TEXT
        );
        CREATE INDEX mcp_confirmations_expiry ON mcp_confirmations(expires_ms);
        CREATE INDEX mcp_confirmations_pending ON mcp_confirmations(user_id)
          WHERE result IS NULL AND invalidated = 0;
        CREATE TRIGGER mcp_confirmation_card_deleted AFTER DELETE ON cards BEGIN
          UPDATE mcp_confirmations SET invalidated = 1
          WHERE user_id = OLD.user_id AND result IS NULL AND invalidated = 0
            AND EXISTS (SELECT 1 FROM json_each(card_ids) WHERE value = OLD.id);
        END;
        CREATE TRIGGER mcp_confirmation_deck_deleted AFTER DELETE ON decks BEGIN
          UPDATE mcp_confirmations SET invalidated = 1
          WHERE user_id = OLD.user_id AND result IS NULL AND invalidated = 0
            AND deck_id = OLD.id;
        END;",
    },
    // A deck deletion covers the decks under the deck, so its approval
    // pins that set of decks beside the cards: a subdeck made or moved in
    // after the person said yes holds no pinned card, and only the deck
    // set can show it was not part of what they approved. `deck_id` stays
    // the deck that was named. A row staged before this step has no list
    // and is read as the named deck alone.
    Migration {
        name: "v024_mcp_confirmation_subtree",
        sql: "ALTER TABLE mcp_confirmations ADD COLUMN deck_ids TEXT;
        DROP TRIGGER mcp_confirmation_deck_deleted;
        CREATE TRIGGER mcp_confirmation_deck_deleted AFTER DELETE ON decks BEGIN
          UPDATE mcp_confirmations SET invalidated = 1
          WHERE user_id = OLD.user_id AND result IS NULL AND invalidated = 0
            AND (deck_id = OLD.id
                 OR EXISTS (SELECT 1 FROM json_each(deck_ids) WHERE value = OLD.id));
        END;",
    },
    Migration {
        name: "v025_speech",
        sql: V025_SPEECH,
    },
    Migration {
        // Listing tags with their counts and filtering a queue by tag
        // name both go tag -> cards; the primary key only serves the
        // other direction.
        name: "v026_card_tags_by_tag",
        sql: "CREATE INDEX idx_card_tags_tag ON card_tags(tag_id);",
    },
    Migration {
        name: "v027_devices",
        sql: V027_DEVICES,
    },
    Migration {
        // A browser session opened by an emailed sign-in link rather than
        // a password or passkey: it may study, and must sign in again
        // before anything that changes how the account is reached
        // (`auth::FullUser`).
        name: "v028_link_sessions",
        sql: "ALTER TABLE sessions ADD COLUMN via_link INTEGER NOT NULL DEFAULT 0;",
    },
    Migration {
        name: "v029_jobs",
        sql: V029_JOBS,
    },
    Migration {
        name: "v030_import_commits",
        sql: V030_IMPORT_COMMITS,
    },
    Migration {
        name: "v031_durable_ceremonies",
        sql: V031_DURABLE_CEREMONIES,
    },
    Migration {
        // Review cards are due for their whole study day unless the
        // account asks for exact times (`UserSettings::due_by_day`).
        // Every account starts by day, the ones already here included.
        name: "v032_due_by_day",
        sql: "ALTER TABLE user_settings ADD COLUMN due_by_day INTEGER NOT NULL DEFAULT 1;",
    },
    Migration {
        name: "v033_reserved",
        sql: RESERVED,
    },
    Migration {
        name: "v034_note_revisions",
        sql: V034_NOTE_REVISIONS,
    },
    Migration {
        name: "v035_study_measures",
        sql: V035_STUDY_MEASURES,
    },
    Migration {
        name: "v036_id_floor",
        sql: V036_ID_FLOOR,
    },
    Migration {
        name: "v037_client_errors",
        sql: V037_CLIENT_ERRORS,
    },
    Migration {
        name: "v038_speech_blocks",
        sql: V038_SPEECH_BLOCKS,
    },
    Migration {
        name: "v039_reserved",
        sql: RESERVED,
    },
    // The language a person picked for each field of a card they wrote
    // by hand (a tag like "ja" or "es-MX"). NULL: nobody said.
    Migration {
        name: "v040_note_langs",
        sql: "ALTER TABLE notes ADD COLUMN front_lang TEXT;
              ALTER TABLE notes ADD COLUMN back_lang TEXT;",
    },
    Migration {
        name: "v041_reserved",
        sql: RESERVED,
    },
    Migration {
        name: "v042_reserved",
        sql: RESERVED,
    },
    Migration {
        name: "v043_reserved",
        sql: RESERVED,
    },
    Migration {
        name: "v044_account_locale",
        sql: V044_ACCOUNT_LOCALE,
    },
    // A deck's goal date (`YYYY-MM-DD` on the account's calendar) and the
    // plan it was set with: the new cards a day that keep it on track
    // (`goal_pace`) and how many cards the deck had then (`goal_cards`),
    // so adding cards re-plans and only skipped days put it behind.
    Migration {
        name: "v045_deck_goal",
        sql: "ALTER TABLE decks ADD COLUMN goal_date TEXT;
              ALTER TABLE decks ADD COLUMN goal_pace INTEGER;
              ALTER TABLE decks ADD COLUMN goal_cards INTEGER;",
    },
    Migration {
        name: "v046_user_id_floor",
        sql: V046_USER_ID_FLOOR,
    },
    Migration {
        name: "v047_refresh_rotation",
        sql: V047_REFRESH_ROTATION,
    },
    Migration {
        name: "v048_note_dropped_slots",
        sql: V048_NOTE_DROPPED_SLOTS,
    },
    // An account's live cards by whether they are suspended, then by
    // deck: everything a study queue, a deck's counts and a goal's
    // counts ask of a card, so walking an account of tens of thousands
    // of cards never reads their text.
    Migration {
        name: "v049_study_index",
        sql: "CREATE INDEX idx_cards_study ON cards(user_id, suspended, deck_id)
                WHERE deleted_at IS NULL;",
    },
    Migration {
        name: "v050_external_media",
        sql: V050_EXTERNAL_MEDIA,
    },
];

/// A durable reference to an external image which is being localized by the
/// job worker. The URL is retained only while the job is pending or retrying;
/// successful work records the local media id and failed work removes it.
const V050_EXTERNAL_MEDIA: &str = r#"
CREATE TABLE external_media_refs (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  side TEXT NOT NULL CHECK (side IN ('front','back')),
  source_url TEXT NOT NULL,
  media_id INTEGER REFERENCES media(id),
  status TEXT NOT NULL CHECK (status IN ('pending','imported','failed')),
  last_error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE (user_id, note_id, side, source_url)
) STRICT;
CREATE INDEX idx_external_media_pending ON external_media_refs(status, updated_at);
"#;

/// Refresh tokens rotate with reuse detection (`repo::oauth`).
/// `oauth_spent_refresh` keeps every refresh token a grant rotated away,
/// until it would have expired anyway (`expires_at`), with when it was
/// rotated (`spent_at`) and the random salt the pair that replaced it was
/// derived from with it (`salt`): the newest one, presented again for a
/// moment, is given the same pair again (its reply may have been lost);
/// any other one revokes the grant. Hashes and a salt only: no pair can
/// be derived without the token itself.
const V047_REFRESH_ROTATION: &str = "
CREATE TABLE oauth_spent_refresh (
  refresh_hash TEXT PRIMARY KEY,
  token_id INTEGER NOT NULL REFERENCES oauth_tokens(id) ON DELETE CASCADE,
  expires_at INTEGER NOT NULL,
  spent_at INTEGER NOT NULL,
  salt TEXT NOT NULL
) STRICT;
CREATE INDEX idx_spent_refresh_token ON oauth_spent_refresh(token_id);
";

/// The slots of a note whose card a person deleted, a JSON array of
/// ords: a later save of the note makes every other slot its text has,
/// but not these. Filled from the cards already deleted from live notes,
/// where the note's text still makes that slot (a slot its text lost was
/// taken away by an edit, not deleted, and comes back if written again).
const V048_NOTE_DROPPED_SLOTS: &str = "
ALTER TABLE notes ADD COLUMN dropped_slots TEXT NOT NULL DEFAULT '[]';
WITH gone AS (
    SELECT DISTINCT c.note_id, c.ord FROM cards c JOIN notes n ON n.id = c.note_id
    WHERE n.deleted_at IS NULL AND c.deleted_at IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM cards l
                      WHERE l.note_id = c.note_id AND l.ord = c.ord AND l.deleted_at IS NULL)
      AND CASE n.note_type
            WHEN 'cloze' THEN instr(n.front_html, '{{c' || (c.ord + 1) || '::') > 0
            WHEN 'basic_reversed' THEN c.ord IN (0, 1)
            ELSE c.ord = 0
          END)
UPDATE notes SET dropped_slots = (
    SELECT json_group_array(ord) FROM (SELECT ord FROM gone WHERE note_id = notes.id ORDER BY ord))
WHERE id IN (SELECT note_id FROM gone);
";

/// The language an account reads Flash in (`locale`, a BCP 47 tag such
/// as "es-AR") and where it came from (`locale_source`): the browser it
/// signs in with, the assistant it studies in, its phone, or its own
/// choice. A learned language replaces only another learned one while
/// nobody chose; a choice is never replaced.
///
/// `account_locales` keeps each of the three signals as first and last
/// seen, with a browser's whole list of preferences, so what people read
/// in can be counted apart from what Flash picked for them.
const V044_ACCOUNT_LOCALE: &str = "
ALTER TABLE user_settings ADD COLUMN locale TEXT;
ALTER TABLE user_settings ADD COLUMN locale_source TEXT NOT NULL DEFAULT 'default'
  CHECK (locale_source IN ('default', 'browser', 'host', 'app', 'chosen'));
CREATE TABLE account_locales (
  user_id INTEGER NOT NULL REFERENCES users(id),
  source TEXT NOT NULL CHECK (source IN ('browser', 'host', 'app')),
  tag TEXT NOT NULL,
  prefs TEXT,
  first_at INTEGER NOT NULL,
  last_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, source)
) STRICT;
";

/// Slots kept so the numbering stays stable: what these steps made is a
/// downstream extension's, and its own migrations make it.
const RESERVED: &str = "-- reserved for a downstream extension";

/// How each side reads aloud and in which language
/// (`flash_core::speech`): JSON block lists, NULL while a side has none.
/// A note holds the blocks given for its fields; each of its cards
/// holds the blocks derived for that card (a reversed card's sides
/// swapped, a cloze card's gap a cue), and a card without a note holds
/// its own. `speech_by` is `assistant` for the assistant that wrote the
/// card; the other value is a downstream extension's. Writing them never
/// moves `updated_at`, so they can never win a sync conflict over an edit.
const V038_SPEECH_BLOCKS: &str = "
ALTER TABLE notes ADD COLUMN front_speech TEXT;
ALTER TABLE notes ADD COLUMN back_speech TEXT;
ALTER TABLE notes ADD COLUMN speech_by TEXT CHECK (speech_by IN ('assistant', 'labeler'));
ALTER TABLE cards ADD COLUMN front_speech TEXT;
ALTER TABLE cards ADD COLUMN back_speech TEXT;
ALTER TABLE cards ADD COLUMN speech_by TEXT CHECK (speech_by IN ('assistant', 'labeler'));
";

/// The largest id each of decks, notes and cards has held, so a deleted
/// row's id is never given to a new one (`repo::ids`). A card id the
/// review log still names was a card's once, so it counts too. An insert
/// that names no id, or one at or under the floor, is refused: a new
/// insert that forgot `next_id` fails at once instead of reusing an id
/// after the next deletion.
const V036_ID_FLOOR: &str = "
CREATE TABLE id_floor (
  tbl TEXT PRIMARY KEY,
  max_id INTEGER NOT NULL
) WITHOUT ROWID;
INSERT INTO id_floor (tbl, max_id) VALUES
  ('decks', COALESCE((SELECT MAX(id) FROM decks), 0)),
  ('notes', COALESCE((SELECT MAX(id) FROM notes), 0)),
  ('cards', MAX(COALESCE((SELECT MAX(id) FROM cards), 0),
                COALESCE((SELECT MAX(card_id) FROM review_log), 0)));
CREATE TRIGGER decks_id_floor AFTER DELETE ON decks BEGIN
  UPDATE id_floor SET max_id = OLD.id WHERE tbl = 'decks' AND max_id < OLD.id;
END;
CREATE TRIGGER notes_id_floor AFTER DELETE ON notes BEGIN
  UPDATE id_floor SET max_id = OLD.id WHERE tbl = 'notes' AND max_id < OLD.id;
END;
CREATE TRIGGER cards_id_floor AFTER DELETE ON cards BEGIN
  UPDATE id_floor SET max_id = OLD.id WHERE tbl = 'cards' AND max_id < OLD.id;
END;
CREATE TRIGGER decks_id_fresh BEFORE INSERT ON decks
WHEN NEW.id IS NULL OR NEW.id <= (SELECT max_id FROM id_floor WHERE tbl = 'decks') BEGIN
  SELECT RAISE(ABORT, 'a new deck takes its id from next_id');
END;
CREATE TRIGGER notes_id_fresh BEFORE INSERT ON notes
WHEN NEW.id IS NULL OR NEW.id <= (SELECT max_id FROM id_floor WHERE tbl = 'notes') BEGIN
  SELECT RAISE(ABORT, 'a new note takes its id from next_id');
END;
CREATE TRIGGER cards_id_fresh BEFORE INSERT ON cards
WHEN NEW.id IS NULL OR NEW.id <= (SELECT max_id FROM id_floor WHERE tbl = 'cards') BEGIN
  SELECT RAISE(ABORT, 'a new card takes its id from next_id');
END;
";

/// Accounts join v036's floor: an emailed link or a phone's folder may
/// name an account by its id alone, so a deleted account's id is never
/// given to a new one. An id freed before this step is known to nothing
/// and can be given once more.
const V046_USER_ID_FLOOR: &str = "
INSERT INTO id_floor (tbl, max_id) VALUES ('users', COALESCE((SELECT MAX(id) FROM users), 0));
CREATE TRIGGER users_id_floor AFTER DELETE ON users BEGIN
  UPDATE id_floor SET max_id = OLD.id WHERE tbl = 'users' AND max_id < OLD.id;
END;
CREATE TRIGGER users_id_fresh BEFORE INSERT ON users
WHEN NEW.id IS NULL OR NEW.id <= (SELECT max_id FROM id_floor WHERE tbl = 'users') BEGIN
  SELECT RAISE(ABORT, 'a new account takes its id from next_id');
END;
";

/// Script errors the website's pages reported about themselves, and the
/// Content-Security-Policy violations browsers reported about them
/// (`repo::client_errors`). Counted, not logged: one row per day, kind,
/// script and line, message, page and browser family, with how often
/// and when first and last. No account, no address, no user agent: the
/// browser is a family ("Safari (mobile)"), the page a path with its
/// numbers taken out. `new_error` marks the row whose report was the
/// error's first in a week, as decided when it came in, which is what an
/// alert names. Rows go after 30 days.
const V037_CLIENT_ERRORS: &str = "
CREATE TABLE client_errors (
  id INTEGER PRIMARY KEY,
  day INTEGER NOT NULL,
  kind TEXT NOT NULL,
  file TEXT NOT NULL,
  line INTEGER NOT NULL,
  message TEXT NOT NULL,
  page TEXT NOT NULL,
  browser TEXT NOT NULL,
  count INTEGER NOT NULL,
  first_at INTEGER NOT NULL,
  last_at INTEGER NOT NULL,
  new_error INTEGER NOT NULL,
  UNIQUE (day, kind, file, line, message, page, browser)
);
CREATE INDEX idx_client_errors_error ON client_errors(kind, file, line, message);
CREATE INDEX idx_client_errors_last ON client_errors(last_at);
CREATE INDEX idx_client_errors_new ON client_errors(first_at) WHERE new_error = 1;
";

/// What the review log and sessions could not say before.
/// `review_log.answer_ms`: how long the card was on screen before the
/// grade, as the page that showed it measured (NULL where nobody did:
/// an assistant's own grading, an old app, reviews from before this).
/// `elapsed_ms` is the scheduler's time since the card's previous
/// review, which was being read as answer time.
/// `study_sessions.surface`: where the session was started (web, mcp,
/// panel, mobile, or one an extension names); `reviews` and
/// `last_review_at` give its length without relying on `ended_at`,
/// which only some clients set.
/// `user_settings.timezone_source`: how the zone was learned (default,
/// browser, chosen), so an account still on the default can be
/// told from one that really is on UTC; `unknown` for zones learned
/// before this was kept.
const V035_STUDY_MEASURES: &str = "
ALTER TABLE review_log ADD COLUMN answer_ms INTEGER;
ALTER TABLE study_sessions ADD COLUMN surface TEXT;
ALTER TABLE study_sessions ADD COLUMN reviews INTEGER NOT NULL DEFAULT 0;
ALTER TABLE study_sessions ADD COLUMN last_review_at INTEGER;
UPDATE study_sessions SET
  reviews = (SELECT COUNT(*) FROM review_log r WHERE r.session_id = study_sessions.id),
  last_review_at = (SELECT MAX(reviewed_at) FROM review_log r
                    WHERE r.session_id = study_sessions.id);
ALTER TABLE user_settings ADD COLUMN timezone_source TEXT NOT NULL DEFAULT 'default';
UPDATE user_settings SET timezone_source = 'unknown' WHERE timezone <> 'UTC';
";

/// What a note said before each edit, so an edit (the web editor's, the
/// app's, an assistant's) can be undone. A revision holds the
/// note's whole source as it was: type, both fields, and the tag set as
/// a JSON array; `source` is who made the edit that replaced it. A
/// revision belongs to its note: it lives while the note row does (a
/// soft-deleted note keeps its history) and goes with it when the row is
/// deleted, by cascade, which a deck or account deletion does. The few
/// newest are kept (`repo::notes::REVISIONS_KEPT`).
const V034_NOTE_REVISIONS: &str = "
CREATE TABLE note_revisions (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  at INTEGER NOT NULL,
  note_type TEXT NOT NULL,
  front_html TEXT NOT NULL,
  back_html TEXT NOT NULL,
  tags TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('web','mcp','api','sync','restore'))
) STRICT;
CREATE INDEX idx_note_revisions_note ON note_revisions(note_id, at);
";

/// What a person in the middle of something must not lose to a restart.
/// `ceremonies`: an OAuth consent screen or another sign-in round trip
/// in progress, keyed by the hash of the token its page or cookie holds,
/// used once and gone by `expires_at`; the account it is for is its own
/// column. `mcp_sessions`: an assistant's MCP session (the client's
/// `initialize`), keyed by the hash of the session id, so a session
/// outlives the process that opened it. `server_state`: small facts the
/// server keeps about itself (the key that signs panel media links, a
/// clean stop).
const V031_DURABLE_CEREMONIES: &str = "
CREATE TABLE ceremonies (
  key_hash TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  user_id INTEGER,
  payload TEXT NOT NULL,
  expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_ceremonies_expiry ON ceremonies(expires_at);
CREATE TABLE mcp_sessions (
  id_hash TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE server_state (
  name TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;
";

/// One row per import a preview token committed: claimed before the
/// first card goes in, finished with the count once they all have. A
/// retry of a commit that went through (the reply was lost) finds the
/// finished row and imports nothing twice; one that finds the row still
/// unfinished knows the first try was cut off part way.
const V030_IMPORT_COMMITS: &str = "
CREATE TABLE import_commits (
  user_id INTEGER NOT NULL,
  token_hash TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  finished_at INTEGER,
  cards INTEGER,
  PRIMARY KEY (user_id, token_hash)
) STRICT;
";

/// Work the server owes someone (a mail, a notice), written down before
/// it is done so a restart or a crash cannot lose it. A worker claims a
/// due job by taking a lease (`lease_until`); a job whose worker died is
/// claimed again once the lease runs out. `dedupe_key` makes enqueuing
/// the same promise twice a no-op. `attempts` counts claims; a job ends
/// `done_at` or, after its last retry, `failed_at` with `last_error`.
const V029_JOBS: &str = "
CREATE TABLE jobs (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  dedupe_key TEXT UNIQUE,
  user_id INTEGER,
  payload TEXT NOT NULL DEFAULT '{}',
  run_at INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  lease_until INTEGER,
  done_at INTEGER,
  failed_at INTEGER,
  last_error TEXT,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_jobs_due ON jobs(run_at) WHERE done_at IS NULL AND failed_at IS NULL;
CREATE INDEX idx_jobs_user ON jobs(user_id);
";

/// Every sign-in, browser or app, is listed and signed out one by one.
/// A browser session gains an id to name it by (its hash stays out of
/// forms), the name of the browser, and when it was last used; SQLite
/// adds no primary key to a table, so it is rebuilt, and the sessions
/// already open carry over. An app token gains the id of the install
/// that asked for it, so signing in again on the same phone replaces
/// that phone's grant instead of adding one.
const V027_DEVICES: &str = "
CREATE TABLE sessions_v027 (
  id INTEGER PRIMARY KEY,
  id_hash TEXT NOT NULL UNIQUE,
  user_id INTEGER NOT NULL REFERENCES users(id),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  label TEXT NOT NULL DEFAULT '',
  last_used_at INTEGER
) STRICT;
INSERT INTO sessions_v027 (id_hash, user_id, created_at, expires_at)
  SELECT id_hash, user_id, created_at, expires_at FROM sessions ORDER BY created_at;
DROP TABLE sessions;
ALTER TABLE sessions_v027 RENAME TO sessions;
CREATE INDEX idx_sessions_user ON sessions(user_id);

ALTER TABLE oauth_tokens ADD COLUMN install_id TEXT;
";

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn applied(conn: &Connection, extension: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT name FROM schema_migrations WHERE extension = ?1")?;
    let rows = stmt.query_map([extension], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn record(conn: &Connection, extension: &str, name: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_migrations (extension, name, applied_at) VALUES (?1, ?2, ?3)",
        params![extension, name, now],
    )?;
    Ok(())
}

/// Runs one step and records it, atomically.
fn apply(conn: &Connection, extension: &str, m: &Migration, version: Option<i64>) -> Result<()> {
    let bump = version
        .map(|v| format!("PRAGMA user_version = {v};\n"))
        .unwrap_or_default();
    conn.execute_batch(&format!(
        "BEGIN;\n{}\n{bump}INSERT INTO schema_migrations (extension, name, applied_at) \
         VALUES ('{extension}', '{}', {});\nCOMMIT;",
        m.sql,
        m.name,
        now_ms()
    ))?;
    Ok(())
}

/// The `user_version` a database has once every core step has run.
pub fn core_version() -> i64 {
    CORE.len() as i64
}

pub fn run(conn: &Connection, extensions: &[Arc<dyn StoreExtension>]) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           extension TEXT NOT NULL,
           name TEXT NOT NULL,
           applied_at INTEGER NOT NULL,
           PRIMARY KEY (extension, name)
         ) STRICT;",
    )?;
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let registered: i64 =
        conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))?;

    // A pre-registry database: adopt it whole, or refuse it.
    if current > 0 && registered == 0 {
        if current != LAST_MONOLITH_VERSION {
            return Err(StoreError::Invalid(format!(
                "database is at schema version {current}; upgrade it with the last \
                 single-binary release (schema {LAST_MONOLITH_VERSION}) first"
            )));
        }
        let tx = conn.unchecked_transaction()?;
        let now = now_ms();
        record(&tx, "core", MONOLITH_MARKER, now)?;
        // Only the steps the monolith already contains are adopted; anything
        // newer runs below like on any other database.
        for m in CORE.iter().take(LAST_MONOLITH_VERSION as usize) {
            record(&tx, "core", m.name, now)?;
        }
        tx.commit()?;
    }

    for (i, m) in CORE.iter().enumerate() {
        let version = (i + 1) as i64;
        if version > current {
            apply(conn, "core", m, Some(version))?;
        }
    }

    let adopted_monolith: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE extension = 'core' AND name = ?1)",
        [MONOLITH_MARKER],
        |r| r.get(0),
    )?;
    for ext in extensions {
        let name = ext.name();
        let steps = ext.migrations();
        if adopted_monolith && applied(conn, name)?.is_empty() {
            let tx = conn.unchecked_transaction()?;
            let now = now_ms();
            for m in steps.iter().take(ext.legacy_baseline()) {
                record(&tx, name, m.name, now)?;
            }
            tx.commit()?;
        }
        let done = applied(conn, name)?;
        for m in steps.iter().filter(|m| !done.contains(m.name)) {
            let present = match ext.adopt_if(m.name) {
                Some(query) => conn.query_row(query, [], |r| r.get::<_, bool>(0))?,
                None => false,
            };
            if present {
                record(conn, name, m.name, now_ms())?;
            } else {
                apply(conn, name, m, None)?;
            }
        }
    }
    Ok(())
}

/// Timestamps are epoch milliseconds (FSRS cares about sub-day elapsed time).
/// Secrets (session ids, invite/oauth tokens and codes) are stored only as
/// SHA-256 hex. review_log is append-only, enforced by triggers (the delete
/// trigger is relaxed in V008 for rows whose owner no longer exists, so an
/// account can be purged).
const V001_INITIAL: &str = r#"
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  display_name TEXT NOT NULL,
  email TEXT UNIQUE,
  role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('admin','member')),
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE user_settings (
  user_id INTEGER PRIMARY KEY REFERENCES users(id),
  grading_mode TEXT NOT NULL DEFAULT 'silent'
    CHECK (grading_mode IN ('silent','announce','self')),
  desired_retention REAL NOT NULL DEFAULT 0.9,
  new_per_day INTEGER NOT NULL DEFAULT 20,
  day_cutoff_hour INTEGER NOT NULL DEFAULT 4,
  timezone TEXT NOT NULL DEFAULT 'America/New_York',
  fsrs_params TEXT
) STRICT;

CREATE TABLE webauthn_credentials (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  cred_id BLOB NOT NULL UNIQUE,
  passkey TEXT NOT NULL,
  label TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  last_used_at INTEGER
) STRICT;

CREATE TABLE invites (
  id INTEGER PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  role TEXT NOT NULL DEFAULT 'member',
  display_name TEXT NOT NULL,
  created_by INTEGER REFERENCES users(id),
  expires_at INTEGER NOT NULL,
  used_at INTEGER
) STRICT;

CREATE TABLE sessions (
  id_hash TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
) STRICT;

CREATE TABLE oauth_clients (
  id INTEGER PRIMARY KEY,
  client_id TEXT NOT NULL UNIQUE,
  client_name TEXT NOT NULL,
  redirect_uris TEXT NOT NULL,
  metadata TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE oauth_codes (
  code_hash TEXT PRIMARY KEY,
  client_id TEXT NOT NULL,
  user_id INTEGER NOT NULL REFERENCES users(id),
  redirect_uri TEXT NOT NULL,
  code_challenge TEXT NOT NULL,
  scope TEXT NOT NULL,
  resource TEXT,
  expires_at INTEGER NOT NULL,
  used_at INTEGER
) STRICT;

CREATE TABLE oauth_tokens (
  id INTEGER PRIMARY KEY,
  access_hash TEXT NOT NULL UNIQUE,
  refresh_hash TEXT UNIQUE,
  client_id TEXT NOT NULL,
  user_id INTEGER NOT NULL REFERENCES users(id),
  scope TEXT NOT NULL,
  access_expires_at INTEGER NOT NULL,
  refresh_expires_at INTEGER,
  created_at INTEGER NOT NULL,
  revoked_at INTEGER
) STRICT;
CREATE INDEX idx_tokens_user ON oauth_tokens(user_id);

CREATE TABLE decks (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  UNIQUE (user_id, name)
) STRICT;

CREATE TABLE cards (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  deck_id INTEGER NOT NULL REFERENCES decks(id),
  front TEXT NOT NULL,
  back TEXT NOT NULL,
  suspended INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
) STRICT;
CREATE INDEX idx_cards_deck ON cards(deck_id) WHERE deleted_at IS NULL;

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  name TEXT NOT NULL,
  UNIQUE (user_id, name)
) STRICT;

CREATE TABLE card_tags (
  card_id INTEGER NOT NULL REFERENCES cards(id),
  tag_id INTEGER NOT NULL REFERENCES tags(id),
  PRIMARY KEY (card_id, tag_id)
) WITHOUT ROWID;

CREATE TABLE card_state (
  card_id INTEGER PRIMARY KEY REFERENCES cards(id),
  phase INTEGER NOT NULL DEFAULT 0,
  stability REAL,
  difficulty REAL,
  due INTEGER NOT NULL,
  last_review INTEGER,
  reps INTEGER NOT NULL DEFAULT 0,
  lapses INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX idx_state_due ON card_state(due);

CREATE TABLE review_log (
  id INTEGER PRIMARY KEY,
  card_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  reviewed_at INTEGER NOT NULL,
  rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 4),
  phase_before INTEGER NOT NULL,
  elapsed_ms INTEGER NOT NULL,
  stability_after REAL NOT NULL,
  difficulty_after REAL NOT NULL,
  due_after INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('mcp','web','import')),
  session_id INTEGER
) STRICT;
CREATE INDEX idx_review_card ON review_log(card_id, reviewed_at);
CREATE INDEX idx_review_user_time ON review_log(user_id, reviewed_at);

CREATE TRIGGER review_log_no_update BEFORE UPDATE ON review_log
BEGIN SELECT RAISE(ABORT, 'review_log is immutable'); END;
CREATE TRIGGER review_log_no_delete BEFORE DELETE ON review_log
BEGIN SELECT RAISE(ABORT, 'review_log is immutable'); END;

CREATE TABLE study_sessions (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  scope TEXT NOT NULL
) STRICT;
"#;

/// Password auth (optional alongside passkeys). password_hash is a PHC
/// string (Argon2id), a verifier, not a recoverable secret. The pw_failed
/// pair implements a small DB-backed lockout window. Invites learn when
/// they were minted. (Columns a downstream extension added in this slot
/// live in that extension's own baseline migration.)
const V002_PASSWORD_AUTH: &str = r#"
ALTER TABLE invites ADD COLUMN created_at INTEGER;

ALTER TABLE users ADD COLUMN password_hash TEXT;
ALTER TABLE users ADD COLUMN pw_failed_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN pw_failed_at INTEGER;

CREATE TABLE password_resets (
  token_hash TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  used_at INTEGER
) STRICT;
CREATE INDEX idx_password_resets_user ON password_resets(user_id);

CREATE INDEX idx_cards_user ON cards(user_id) WHERE deleted_at IS NULL;
"#;

/// Slot kept so the numbering stays stable: what shipped here in the
/// single-binary history belongs to a downstream extension.
const V003_RESERVED: &str = "-- reserved for a downstream extension";

/// Media metadata. Blobs live on disk under data_dir/media, named by
/// sha256 (content-addressed: hostile filenames never touch the
/// filesystem, identical files dedupe). Rows are per-user references to a
/// blob; a blob is deletable only when no row across any user names its
/// hash. `filename` is sanitized display metadata, never a path.
const V004_MEDIA: &str = r#"
CREATE TABLE media (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  sha256 TEXT NOT NULL,
  filename TEXT NOT NULL,
  mime TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('image','audio','video')),
  size INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (user_id, sha256, filename)
) STRICT;
CREATE INDEX idx_media_user ON media(user_id);
CREATE INDEX idx_media_sha ON media(sha256);

CREATE TABLE card_media (
  card_id INTEGER NOT NULL REFERENCES cards(id),
  media_id INTEGER NOT NULL REFERENCES media(id),
  PRIMARY KEY (card_id, media_id)
) WITHOUT ROWID;
"#;

/// Dual card representation and Anki fidelity. front/back stay the plain
/// text every surface (MCP, CSV, search) uses; *_html is the sanitized
/// semantic-HTML view for the web UI (NULL = plain card). cloze_text +
/// cloze_index preserve the original cloze source for round-trip export;
/// type_answer drives the interactive typing UI. Editing a card clears
/// all five (a hand-edited card is plain by definition).
const V005_RICH_CARDS: &str = r#"
ALTER TABLE cards ADD COLUMN front_html TEXT;
ALTER TABLE cards ADD COLUMN back_html TEXT;
ALTER TABLE cards ADD COLUMN cloze_text TEXT;
ALTER TABLE cards ADD COLUMN cloze_index INTEGER;
ALTER TABLE cards ADD COLUMN type_answer TEXT;
"#;

/// Daily limits, Anki-style: the account carries defaults (new/day,
/// reviews/day), each deck may override either (NULL = inherit), and both
/// levels carry a "more new cards today" boost keyed to the study-day
/// start it was granted in (stale boost_day = no boost).
const V006_LIMITS: &str = r#"
ALTER TABLE user_settings ADD COLUMN reviews_per_day INTEGER NOT NULL DEFAULT 200;
ALTER TABLE user_settings ADD COLUMN boost_new INTEGER NOT NULL DEFAULT 0;
ALTER TABLE user_settings ADD COLUMN boost_day INTEGER NOT NULL DEFAULT 0;
ALTER TABLE decks ADD COLUMN new_per_day INTEGER;
ALTER TABLE decks ADD COLUMN reviews_per_day INTEGER;
ALTER TABLE decks ADD COLUMN boost_new INTEGER NOT NULL DEFAULT 0;
ALTER TABLE decks ADD COLUMN boost_day INTEGER NOT NULL DEFAULT 0;
"#;

/// The Today page's "Connect your AI" card can be dismissed for good.
const V007_CONNECT_CTA: &str = r#"
ALTER TABLE user_settings ADD COLUMN hide_connect_cta INTEGER NOT NULL DEFAULT 0;
"#;

/// Self-serve account deletion. review_log stays immutable for every live
/// account; only rows whose user row is already gone may be deleted, which
/// is exactly the last step of Store::delete_user_account.
const V008_ACCOUNT_DELETION: &str = r#"
DROP TRIGGER review_log_no_delete;
CREATE TRIGGER review_log_no_delete BEFORE DELETE ON review_log
WHEN EXISTS (SELECT 1 FROM users WHERE id = OLD.user_id)
BEGIN SELECT RAISE(ABORT, 'review_log is immutable'); END;
"#;

/// Notes: the editable unit behind sibling cards (Anki's model). A note
/// holds the editor source (Front/Back, or cloze Text/Extra with the raw
/// `{{cN::...}}` markup) and fans out into cards by `ord` (reversed: 0
/// and 1; cloze: index - 1). Editing a note regenerates its cards in
/// place, so review state survives. Cards created before this migration
/// (and by MCP or the quick-add form) have no note; they behave as
/// single-card Basic notes and are adopted into a real note on their
/// first rich edit.
const V009_NOTES: &str = r#"
CREATE TABLE notes (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  deck_id INTEGER NOT NULL REFERENCES decks(id),
  note_type TEXT NOT NULL CHECK (note_type IN ('basic','basic_reversed','basic_typed','cloze')),
  front_html TEXT NOT NULL,
  back_html TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
) STRICT;
CREATE INDEX idx_notes_deck ON notes(deck_id) WHERE deleted_at IS NULL;
ALTER TABLE cards ADD COLUMN note_id INTEGER REFERENCES notes(id);
ALTER TABLE cards ADD COLUMN ord INTEGER NOT NULL DEFAULT 0;
CREATE UNIQUE INDEX idx_cards_note_ord ON cards(note_id, ord) WHERE note_id IS NOT NULL AND deleted_at IS NULL;
"#;

/// Slot kept so the numbering stays stable (see V003).
const V010_RESERVED: &str = "-- reserved for a downstream extension";

/// Slot kept so the numbering stays stable (see V003).
const V011_RESERVED: &str = "-- reserved for a downstream extension";

/// The mobile app. Bearer tokens minted for the first-party client carry
/// a device label and a last-use time (Settings → "sign out other
/// devices"); the theme moves from a browser cookie into settings so the
/// app and the web agree. Finally `review_log.source` gains 'mobile'.
/// SQLite cannot alter a CHECK, so the table is rebuilt: same columns,
/// both indexes and both triggers recreated (the delete trigger in its
/// V008 form). DROP TABLE fires no row triggers, and nothing references
/// review_log by foreign key. (Tables a downstream extension added in
/// this slot live in that extension's own baseline migration.)
const V012_MOBILE: &str = r#"
ALTER TABLE oauth_tokens ADD COLUMN label TEXT NOT NULL DEFAULT '';
ALTER TABLE oauth_tokens ADD COLUMN last_used_at INTEGER;
ALTER TABLE user_settings ADD COLUMN theme TEXT NOT NULL DEFAULT 'dark';

CREATE TABLE review_log_v12 (
  id INTEGER PRIMARY KEY,
  card_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  reviewed_at INTEGER NOT NULL,
  rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 4),
  phase_before INTEGER NOT NULL,
  elapsed_ms INTEGER NOT NULL,
  stability_after REAL NOT NULL,
  difficulty_after REAL NOT NULL,
  due_after INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('mcp','web','import','mobile')),
  session_id INTEGER
) STRICT;
INSERT INTO review_log_v12
  SELECT id, card_id, user_id, reviewed_at, rating, phase_before, elapsed_ms,
         stability_after, difficulty_after, due_after, source, session_id
  FROM review_log;
DROP TRIGGER review_log_no_update;
DROP TRIGGER review_log_no_delete;
DROP TABLE review_log;
ALTER TABLE review_log_v12 RENAME TO review_log;
CREATE INDEX idx_review_card ON review_log(card_id, reviewed_at);
CREATE INDEX idx_review_user_time ON review_log(user_id, reviewed_at);
CREATE TRIGGER review_log_no_update BEFORE UPDATE ON review_log
BEGIN SELECT RAISE(ABORT, 'review_log is immutable'); END;
CREATE TRIGGER review_log_no_delete BEFORE DELETE ON review_log
WHEN EXISTS (SELECT 1 FROM users WHERE id = OLD.user_id)
BEGIN SELECT RAISE(ABORT, 'review_log is immutable'); END;
"#;

/// Slot kept so the numbering stays stable (see V003).
const V013_RESERVED: &str = "-- reserved for a downstream extension";

/// Slot kept so the numbering stays stable (see V003).
const V014_RESERVED: &str = "-- reserved for a downstream extension";

/// Which client a review came through when the surface has several: the
/// OAuth client's name for a review over MCP, the platform for the app.
/// NULL for the web. Adding a column does not fire review_log's
/// immutability triggers.
const V015_REVIEW_CLIENT: &str = "ALTER TABLE review_log ADD COLUMN client TEXT;";

/// Tables a downstream extension uses, and `notes.ai_job_id`, the note's
/// origin. `ai_figures` pins media rows so the unlinked-media sweep
/// leaves them alone.
const V016_AI_GENERATION: &str = r#"
CREATE TABLE ai_sources (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  kind TEXT NOT NULL CHECK (kind IN ('text','pdf','image','youtube','video','audio')),
  sha256 TEXT NOT NULL,
  mime TEXT NOT NULL DEFAULT '',
  size INTEGER NOT NULL DEFAULT 0,
  filename TEXT NOT NULL DEFAULT '',
  url TEXT,
  text TEXT,
  pages INTEGER,
  duration_s INTEGER,
  est_words INTEGER,
  image_count INTEGER,
  created_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_ai_sources_user ON ai_sources(user_id);
CREATE INDEX idx_ai_sources_sha ON ai_sources(sha256);
CREATE INDEX idx_ai_sources_last_used ON ai_sources(last_used_at);

CREATE TABLE ai_extractions (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  source_id INTEGER NOT NULL REFERENCES ai_sources(id),
  sha256 TEXT NOT NULL,
  model TEXT NOT NULL,
  prompt_version INTEGER NOT NULL,
  text TEXT NOT NULL,
  figures TEXT NOT NULL DEFAULT '[]',
  est_words INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL,
  UNIQUE (user_id, sha256, model, prompt_version)
) STRICT;
CREATE INDEX idx_ai_extractions_source ON ai_extractions(source_id);

CREATE TABLE ai_figures (
  user_id INTEGER NOT NULL REFERENCES users(id),
  extraction_id INTEGER NOT NULL REFERENCES ai_extractions(id),
  media_id INTEGER NOT NULL REFERENCES media(id),
  PRIMARY KEY (extraction_id, media_id)
) WITHOUT ROWID;
CREATE INDEX idx_ai_figures_media ON ai_figures(media_id);

CREATE TABLE ai_jobs (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  source_id INTEGER NOT NULL,
  deck_id INTEGER REFERENCES decks(id),
  extraction_id INTEGER,
  op TEXT NOT NULL CHECK (op IN ('generate','revise','more')),
  parent_job_id INTEGER REFERENCES ai_jobs(id),
  instruction TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT 'queued'
    CHECK (state IN ('queued','extracting','generating','done','failed')),
  progress INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  finished_at INTEGER
) STRICT;
CREATE INDEX idx_ai_jobs_user ON ai_jobs(user_id, created_at);
CREATE INDEX idx_ai_jobs_state ON ai_jobs(state, created_at);

CREATE TABLE ai_drafts (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  job_id INTEGER NOT NULL REFERENCES ai_jobs(id),
  ord INTEGER NOT NULL,
  note_type TEXT NOT NULL CHECK (note_type IN ('basic','basic_reversed','basic_typed','cloze')),
  front_html TEXT NOT NULL,
  back_html TEXT NOT NULL DEFAULT '',
  tags TEXT NOT NULL DEFAULT '[]',
  figure_media_ids TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending','edited','kept','discarded')),
  note_id INTEGER REFERENCES notes(id),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE (job_id, ord)
) STRICT;

CREATE TABLE ai_calls (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  job_id INTEGER REFERENCES ai_jobs(id),
  step TEXT NOT NULL CHECK (step IN ('extract','generate','revise','more','grade','clarify')),
  model TEXT NOT NULL,
  input_kind TEXT NOT NULL CHECK (input_kind IN ('text','pdf','image','youtube','video','audio')),
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  media_seconds INTEGER NOT NULL DEFAULT 0,
  ms INTEGER NOT NULL DEFAULT 0,
  ok INTEGER NOT NULL DEFAULT 1,
  error TEXT,
  started_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_ai_calls_user_time ON ai_calls(user_id, started_at);

ALTER TABLE notes ADD COLUMN ai_job_id INTEGER REFERENCES ai_jobs(id);
"#;

/// A table and a column a downstream extension uses, the table filled
/// from the rows already there.
const V017_AI_SOURCE_SETS: &str = r#"
CREATE TABLE ai_job_sources (
  user_id INTEGER NOT NULL REFERENCES users(id),
  job_id INTEGER NOT NULL REFERENCES ai_jobs(id),
  source_id INTEGER NOT NULL,
  ord INTEGER NOT NULL,
  PRIMARY KEY (job_id, ord)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_ai_job_sources_source ON ai_job_sources(source_id);
INSERT INTO ai_job_sources (user_id, job_id, source_id, ord)
  SELECT user_id, id, source_id, 0 FROM ai_jobs;

ALTER TABLE ai_sources ADD COLUMN origin_mime TEXT;
"#;

/// A table a downstream extension uses.
const V018_AI_TURNS: &str = r#"
CREATE TABLE ai_turns (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  job_id INTEGER NOT NULL REFERENCES ai_jobs(id),
  role TEXT NOT NULL CHECK (role IN ('user','model')),
  text TEXT NOT NULL,
  child_job_id INTEGER REFERENCES ai_jobs(id),
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_ai_turns_job ON ai_turns(job_id, id);
"#;

/// Settings columns and tables a downstream extension uses.
const V019_VOICE: &str = r#"
ALTER TABLE user_settings ADD COLUMN voice_id TEXT NOT NULL DEFAULT 'af_heart';
ALTER TABLE user_settings ADD COLUMN speech_rate REAL NOT NULL DEFAULT 1.0;
ALTER TABLE user_settings ADD COLUMN voice_apply TEXT NOT NULL DEFAULT 'auto'
  CHECK (voice_apply IN ('auto','confirm'));
ALTER TABLE user_settings ADD COLUMN sound_cues INTEGER NOT NULL DEFAULT 1;

CREATE TABLE voice_clips (
  id INTEGER PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL CHECK (kind IN ('card','phrase')),
  voice TEXT NOT NULL,
  model TEXT NOT NULL,
  phrase TEXT,
  mime TEXT NOT NULL,
  size INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  envelope BLOB NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_voice_clips_phrase ON voice_clips(voice, model, phrase) WHERE phrase IS NOT NULL;

CREATE TABLE voice_clip_refs (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  card_id INTEGER NOT NULL,
  side TEXT NOT NULL CHECK (side IN ('front','back')),
  clip_id INTEGER NOT NULL REFERENCES voice_clips(id),
  created_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL,
  UNIQUE (user_id, card_id, side, clip_id)
) STRICT;
CREATE INDEX idx_voice_clip_refs_clip ON voice_clip_refs(clip_id);
CREATE INDEX idx_voice_clip_refs_user ON voice_clip_refs(user_id);
"#;

/// Imported Anki history: one review_log row per revlog row, source
/// `import`. A revlog id is the review's timestamp and the collection's
/// primary key, so a user's imported timestamps identify the reviews;
/// the partial unique index lets a second import of the same collection
/// add nothing. No existing row has source `import` (nothing wrote it
/// before now), so the index builds empty.
const V020_IMPORT_HISTORY: &str = r#"
CREATE UNIQUE INDEX idx_review_import_once ON review_log(user_id, reviewed_at) WHERE source = 'import';
"#;

/// A downstream extension's table is rebuilt (the old table renamed
/// aside, copied, dropped) with more columns, every old row kept.
const V021_AI_LEDGER_DETAIL: &str = r#"
ALTER TABLE ai_calls RENAME TO ai_calls_before_detail;
CREATE TABLE ai_calls (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  job_id INTEGER REFERENCES ai_jobs(id),
  step TEXT NOT NULL CHECK (step IN ('extract','generate','revise','more','grade','clarify','turn')),
  model TEXT NOT NULL,
  model_version TEXT,
  response_id TEXT,
  input_kind TEXT NOT NULL CHECK (input_kind IN ('text','pdf','image','youtube','video','audio')),
  input_tokens INTEGER NOT NULL DEFAULT 0,
  cached_tokens INTEGER NOT NULL DEFAULT 0,
  tool_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  thoughts_tokens INTEGER NOT NULL DEFAULT 0,
  media_seconds INTEGER NOT NULL DEFAULT 0,
  ms INTEGER NOT NULL DEFAULT 0,
  usage_json TEXT,
  ok INTEGER NOT NULL DEFAULT 1,
  error TEXT,
  started_at INTEGER NOT NULL
) STRICT;
INSERT INTO ai_calls (id, user_id, job_id, step, model, input_kind, input_tokens,
  output_tokens, media_seconds, ms, ok, error, started_at)
SELECT id, user_id, job_id, step, model, input_kind, input_tokens,
  output_tokens, media_seconds, ms, ok, error, started_at
FROM ai_calls_before_detail;
DROP TABLE ai_calls_before_detail;
CREATE INDEX idx_ai_calls_user_time ON ai_calls(user_id, started_at);
CREATE INDEX idx_ai_calls_time ON ai_calls(started_at);
"#;

/// A deck can be archived: kept whole (cards, media, review history) but
/// out of every study queue and every due count until it is unarchived.
/// The column is the moment it was archived, NULL while the deck is
/// active; nothing on the cards changes either way, so unarchiving just
/// recomputes the queue from the review data that was there all along.
const V022_DECK_ARCHIVE: &str = "ALTER TABLE decks ADD COLUMN archived_at INTEGER;";

/// A deck may name the languages its cards are spoken in (a JSON array
/// of codes; empty: unknown).
const V025_SPEECH: &str =
    "ALTER TABLE decks ADD COLUMN speech_language_hints TEXT NOT NULL DEFAULT '[]';";

#[cfg(test)]
mod tests {
    use super::*;

    /// A database with the first `n` core steps applied the way `run`
    /// applies them.
    fn core_up_to(n: usize) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::pragmas::apply(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (extension TEXT NOT NULL, name TEXT NOT NULL, \
             applied_at INTEGER NOT NULL, PRIMARY KEY (extension, name)) STRICT;",
        )
        .unwrap();
        for (i, m) in CORE.iter().take(n).enumerate() {
            apply(&conn, "core", m, Some((i + 1) as i64)).unwrap();
        }
        conn
    }

    /// 22 steps is the 0.3.0 release's schema.
    #[test]
    fn a_schema_22_database_takes_every_later_step() {
        let old = core_up_to(22);
        old.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO user_settings (user_id) VALUES (7);
             INSERT INTO decks (id, user_id, name, created_at) VALUES (3, 7, 'Bio', 2);
             INSERT INTO notes (id, user_id, deck_id, note_type, front_html, back_html,
                                created_at, updated_at)
             VALUES (5, 7, 3, 'basic', 'Q', 'A', 2, 2);
             INSERT INTO cards (id, user_id, deck_id, note_id, front, back, created_at, updated_at)
             VALUES (10, 7, 3, 5, 'Q', 'A', 2, 2);
             INSERT INTO card_state (card_id, due) VALUES (10, 4);
             INSERT INTO review_log (card_id, user_id, reviewed_at, rating, phase_before,
               elapsed_ms, stability_after, difficulty_after, due_after, source)
             VALUES (10, 7, 3, 3, 0, 0, 1.0, 5.0, 4, 'web');
             INSERT INTO sessions (id_hash, user_id, created_at, expires_at)
             VALUES ('cookie', 7, 1, 9);",
        )
        .unwrap();
        run(&old, &[]).unwrap();

        let fresh = Connection::open_in_memory().unwrap();
        crate::pragmas::apply(&fresh).unwrap();
        run(&fresh, &[]).unwrap();
        let schema = |conn: &Connection| -> Vec<(String, String, Option<String>)> {
            conn.prepare(
                "SELECT type, name, sql FROM sqlite_master
                 WHERE tbl_name <> 'schema_migrations' ORDER BY type, name",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
        };
        assert_eq!(schema(&old), schema(&fresh));
        let version: i64 = old
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, core_version());
        let counts: (i64, i64, i64, i64, i64) = old
            .query_row(
                "SELECT (SELECT COUNT(*) FROM decks), (SELECT COUNT(*) FROM notes),
                        (SELECT COUNT(*) FROM cards), (SELECT COUNT(*) FROM review_log),
                        (SELECT COUNT(*) FROM sessions WHERE id_hash = 'cookie')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 1, 1, 1));
    }

    #[test]
    fn the_v021_rebuild_keeps_every_old_row_and_admits_its_new_step() {
        let conn = core_up_to(20);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'A', 'member', 1);
             INSERT INTO ai_calls (id, user_id, step, model, input_kind, input_tokens, ok, error,
               started_at)
             VALUES (1, 7, 'extract', 'm', 'text', 10, 0, 'e', 5);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[20], Some(21)).unwrap();

        let kept: (i64, i64, Option<String>) = conn
            .query_row(
                "SELECT input_tokens, ok, error FROM ai_calls WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(kept, (10, 0, Some("e".into())));
        let added: (i64, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT cached_tokens, model_version, usage_json FROM ai_calls WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(added, (0, None, None));
        conn.execute(
            "INSERT INTO ai_calls (user_id, step, model, input_kind, started_at) \
             VALUES (7, 'turn', 'm', 'text', 8)",
            [],
        )
        .unwrap();
        let index: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' \
                 AND name = 'idx_ai_calls_user_time'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index, 1);
    }

    #[test]
    fn the_devices_step_keeps_every_open_browser_session() {
        let conn = core_up_to(26);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO sessions (id_hash, user_id, created_at, expires_at)
             VALUES ('older', 7, 10, 5000), ('newer', 7, 20, 6000);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[26], Some(27)).unwrap();

        // Both cookies still resolve, now each with an id to be named by,
        // numbered in the order they were opened, and no name or use yet.
        let rows: Vec<(i64, String, i64, String, Option<i64>)> = conn
            .prepare(
                "SELECT id, id_hash, expires_at, label, last_used_at FROM sessions \
                 WHERE user_id = 7 ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (1, "older".to_string(), 5000, String::new(), None),
                (2, "newer".to_string(), 6000, String::new(), None),
            ]
        );
        // A second session with the same cookie hash is refused.
        assert!(conn
            .execute(
                "INSERT INTO sessions (id_hash, user_id, created_at, expires_at) \
                 VALUES ('older', 7, 30, 7000)",
                [],
            )
            .is_err());
        // An app grant made before the step has no install.
        let column: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('oauth_tokens') WHERE name = 'install_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(column, 1);
    }

    #[test]
    fn the_id_floor_starts_past_every_id_held_and_a_deleted_id_is_never_given_again() {
        let conn = core_up_to(35);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO decks (id, user_id, name, created_at) VALUES (3, 7, 'Bio', 2);
             INSERT INTO cards (id, user_id, deck_id, front, back, created_at, updated_at)
             VALUES (10, 7, 3, 'q', 'a', 2, 2);
             INSERT INTO review_log (card_id, user_id, reviewed_at, rating, phase_before,
               elapsed_ms, stability_after, difficulty_after, due_after, source)
             VALUES (40, 7, 3, 3, 0, 0, 1.0, 5.0, 4, 'web');",
        )
        .unwrap();
        apply(&conn, "core", &CORE[35], Some(36)).unwrap();

        // Card 40 was deleted before the step, and the review log remembers it.
        let next = |t| crate::repo::ids::next_id(&conn, t).unwrap();
        assert_eq!(next(crate::IdTable::Cards), 41);
        assert_eq!(next(crate::IdTable::Decks), 4);
        // The newest deck goes: its id is not the next one's.
        conn.execute_batch(
            "INSERT INTO decks (id, user_id, name, created_at) VALUES (4, 7, 'Chem', 2);
             DELETE FROM decks WHERE id = 4;",
        )
        .unwrap();
        assert_eq!(next(crate::IdTable::Decks), 5);
        // An insert that takes SQLite's own next id is refused.
        assert!(conn
            .execute(
                "INSERT INTO decks (user_id, name, created_at) VALUES (7, 'Geo', 2)",
                [],
            )
            .is_err());
    }

    #[test]
    fn the_user_id_floor_starts_past_every_account_and_a_deleted_id_is_never_given_again() {
        let conn = core_up_to(45);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at)
             VALUES (7, 'Ada', 'member', 1), (9, 'Bo', 'member', 1);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[45], Some(46)).unwrap();

        let next = || crate::repo::ids::next_id(&conn, crate::IdTable::Users).unwrap();
        assert_eq!(next(), 10);
        // The newest account goes: its id is not the next one's.
        conn.execute_batch("DELETE FROM users WHERE id = 9;")
            .unwrap();
        assert_eq!(next(), 10);
        // An insert that takes SQLite's own next id is refused.
        assert!(conn
            .execute(
                "INSERT INTO users (display_name, role, created_at) VALUES ('Cy', 'member', 2)",
                [],
            )
            .is_err());
    }

    #[test]
    fn the_archive_step_leaves_every_existing_deck_active() {
        let conn = core_up_to(21);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO decks (id, user_id, name, created_at) VALUES (3, 7, 'Old exam', 2);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[21], Some(22)).unwrap();

        let archived_at: Option<i64> = conn
            .query_row("SELECT archived_at FROM decks WHERE id = 3", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            archived_at, None,
            "a deck that predates the column is active"
        );
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 22);
    }

    #[test]
    fn the_dropped_slots_step_keeps_the_deleted_slots_the_text_still_makes() {
        let conn = core_up_to(47);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO decks (id, user_id, name, created_at) VALUES (3, 7, 'Words', 2);
             INSERT INTO notes (id, user_id, deck_id, note_type, front_html, back_html,
                                created_at, updated_at)
             VALUES (1, 7, 3, 'cloze', '{{c1::A}} B {{c3::C}}', '', 2, 2),
                    (2, 7, 3, 'basic_reversed', 'Q', 'A', 2, 2);
             INSERT INTO cards (id, user_id, deck_id, note_id, ord, front, back,
                                created_at, updated_at, deleted_at)
             VALUES (10, 7, 3, 1, 0, 'f', 'b', 2, 2, NULL),
                    (11, 7, 3, 1, 1, 'f', 'b', 2, 2, 5),
                    (12, 7, 3, 1, 2, 'f', 'b', 2, 2, 5),
                    (20, 7, 3, 2, 0, 'f', 'b', 2, 2, 5),
                    (21, 7, 3, 2, 0, 'f', 'b', 2, 2, NULL),
                    (22, 7, 3, 2, 1, 'f', 'b', 2, 2, NULL);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[47], Some(48)).unwrap();
        let dropped = |id: i64| {
            conn.query_row("SELECT dropped_slots FROM notes WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
        };
        // c2 left the text (an edit), c3 was deleted: only c3 stays out.
        assert_eq!(dropped(1), "[2]");
        // A deleted card whose slot has a live card again is no claim.
        assert_eq!(dropped(2), "[]");
    }
}
