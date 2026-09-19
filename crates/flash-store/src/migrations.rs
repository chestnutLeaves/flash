//! Schema history. The core's steps are keyed on PRAGMA user_version and
//! run once, in order, each in a transaction; an extension's steps are
//! keyed by name in `schema_migrations`, which every open creates. Never
//! edit a shipped step. Append.
//!
//! A database from before the registry existed (the single-binary era,
//! schema 14) is recognised by its version and an empty registry: its
//! rows are recorded as applied without running anything, under the
//! marker `('core', "monolith_v14")`, and each extension then adopts its
//! own leading `legacy_baseline()` steps the same way.

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
];

fn now_ms() -> i64 {
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
        for m in steps {
            if !done.contains(m.name) {
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

/// Card generation from a user's own material. A *source* is what the
/// user handed over (its blob lives in the media store under its
/// sha256, like card media; the row carries the facts the preflight
/// shows). An *extraction* is the model's reading of a source, cached
/// per user by content hash, model and prompt version so re-runs over
/// the same file cost nothing; the figure crops it produced are ordinary
/// media rows, pinned by `ai_figures` so the unlinked-media sweep leaves
/// them alone until a card keeps them or the extraction goes. A *job* is
/// one run of the pipeline: a small state machine the runner advances
/// and the clients poll. Its `source_id` and `extraction_id` carry no
/// foreign key on purpose: sources and extractions are housekept after
/// a month unused, and a job whose source has gone still renders, as
/// "source expired". *Drafts* are the generated cards awaiting keep or
/// discard (invisible to study, MCP and export) and remember the note
/// a keep created. *Calls* are the ledger every model call writes, ok or
/// not; a quota is derived from it rather than kept in a counter. A
/// note remembers the job that generated it, so voice study can ground
/// a question in the extraction the card came from.
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

/// A job reads a *set* of sources, in order: a PDF, some photos and a
/// recording in one request. `ai_job_sources` is the set; the job's
/// own `source_id` stays as the first member for readers written before
/// sets existed, and every job that exists is given a one-member set.
/// The junction carries no foreign key to `ai_sources` for the same
/// reason the job's column never did: a member may be housekept away,
/// and the job still renders as expired. A source made from a
/// converted document keeps the type it was uploaded as in
/// `origin_mime`, so the person sees "slides.pptx" and not a PDF they
/// never made.
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

/// The thread under a review: what the person asked the model to do
/// to the drafts, and the model's one-sentence answers. `job_id` is the
/// root job (the review the thread belongs to); `child_job_id` is the
/// pass that ran a person's turn or wrote the model's, so a failed
/// pass can be shown as such. Order is by id, since two turns can
/// share a millisecond.
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

/// Voice study on every surface. The account chooses a voice, a speed,
/// how grades apply and whether the sound cues play, and web and app
/// read the same row. A synthesized clip is
/// one row per (spoken text, voice, model) with its bytes in the media
/// store under `key` (64 hex, a hash of the three with its own prefix,
/// never a media hash), shared by every account whose card says the
/// same words; `envelope` is the loudness curve the apps animate with,
/// one byte per 50 ms. A ref is which account's card resolved to which
/// clip: no foreign key on the card on purpose, since cards are hard
/// deleted with their deck and the sweep drops refs whose card is gone.
/// A card clip nobody references is an orphan; phrase clips (`kind =
/// 'phrase'`) have no refs and stay.
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

/// The ledger is rebuilt (the old table renamed aside, copied, dropped)
/// to carry everything the reply's usage record says (cached, tool-use
/// and thinking tokens, the model version that answered, the response
/// id, and the record itself as sent) and to admit the voice `turn`
/// step. What a call costs is no concern of the core's: a downstream
/// extension may keep its own account of each row.
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

    #[test]
    fn the_ledger_detail_step_keeps_every_old_call_and_admits_the_voice_turn() {
        let conn = core_up_to(20);
        conn.execute_batch(
            "INSERT INTO users (id, display_name, role, created_at) VALUES (7, 'Ada', 'member', 1);
             INSERT INTO ai_calls (id, user_id, step, model, input_kind, input_tokens, output_tokens,
               media_seconds, ms, ok, error, started_at)
             VALUES (1, 7, 'generate', 'gemini-3.8-flash', 'pdf', 1000, 200, 0, 900, 1, NULL, 5),
                    (2, 7, 'grade', 'gemini-3.5-flash-lite', 'audio', 1200, 300, 4, 300, 1, NULL, 6),
                    (3, 7, 'extract', 'gemini-3.8-flash', 'video', 0, 0, 0, 10, 0, 'http 503', 7);",
        )
        .unwrap();
        apply(&conn, "core", &CORE[20], Some(21)).unwrap();

        let totals: (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), SUM(input_tokens), SUM(ok = 0) FROM ai_calls WHERE user_id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(totals, (3, 2200, 1), "every call survives with its counts");
        // The detail the old rows never had reads as nothing.
        let detail: (i64, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT cached_tokens, model_version, usage_json FROM ai_calls WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(detail, (0, None, None));
        // The rebuilt table admits the voice turn step and keeps its
        // index.
        conn.execute(
            "INSERT INTO ai_calls (user_id, step, model, input_kind, started_at) \
             VALUES (7, 'turn', 'gemini-3.5-flash-lite', 'text', 8)",
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
}
