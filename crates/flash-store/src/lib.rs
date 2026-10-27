//! Persistence layer: SQLite via rusqlite. All SQL lives here; callers get
//! typed methods over domain types from flash-core.
//!
//! Concurrency: one connection behind a Mutex. A handful of users on a
//! small host; WAL keeps the rare concurrent read cheap. The async server calls
//! these sync methods inside spawn_blocking.

pub mod export;
pub mod ext;
pub mod fold;
pub mod import;
pub mod labels;
pub mod media;
mod migrations;
pub mod notes;
mod pragmas;
mod repo;
pub mod richtext;
mod tmp;

use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, MutexGuard};

/// The connection the `_tx` functions and the extension hooks write on:
/// the store's own, or a transaction from `Store::with_tx`.
pub use rusqlite::Connection;
/// What `Store::with_tx` hands its closure.
pub use rusqlite::Transaction;

use ext::StoreExtension;

pub use migrations::MONOLITH_MARKER;
pub use repo::auth::{
    add_passkey_tx, consume_invite_tx, consume_password_reset_tx, delete_unused_password_resets_tx,
    lookup_invite_tx, lookup_password_reset_tx, InviteInfo, StoredPasskey, WebSessionHit,
    WebSessionRow,
};
pub use repo::card_bulk::{CardFilter, CardStatus, DeckFacet, MAX_BULK_CARDS};
pub use repo::cards::{
    check_tags, CardExtras, CardRow, ImportedCard, MAX_TAGS_PER_CARD, MAX_TAG_LEN,
};
pub use repo::client_errors::{
    ClientErrorNew, ClientErrorRecorded, ClientErrorSummary, CLIENT_ERROR_QUIET_MS,
    CLIENT_ERROR_ROWS_PER_DAY,
};
pub use repo::deck_goals::{DeckGoal, GoalCounts};
pub use repo::decks::{require_deck, DeckLimits, DeckSummary, DeletedDeck, MAX_DECKS_PER_USER};
pub use repo::ids::{next_id, IdTable};
pub use repo::import_commits::ImportClaim;
pub use repo::jobs::{
    enqueue_job, job_counts_tx, owe_blob_deletes, JobCounts, JobRow, JobState, NewJob,
    JOB_DELETE_BLOBS, JOB_EXTERNAL_MEDIA,
};
pub use repo::mcp::{McpDeletionPreview, McpDeletionReceipt, McpMutationReceipt, SourceEdit};
pub use repo::media::MediaRow;
pub use repo::notes::{
    CardSide, NoteLangs, NoteRevision, NoteRow, NoteSave, NoteSpeech, NoteUpdate, RestoredRevision,
    RevisionSource, REVISIONS_KEPT, REVISION_MAX_AGE_MS, SPEECH_BY_ASSISTANT,
};
pub use repo::oauth::{
    revoke_all_tokens_for_user_tx, revoke_tokens_for_user_tx, ApiTokenInfo, CodeConsume, CodeGrant,
    GrantRow, NextPair, OAuthClient, RefreshGrant, RefreshOutcome, TokenRow, REFRESH_GRACE_MS,
};
pub use repo::reviews::{ImportedReview, SessionStats};
pub use repo::speech::parse_speech_hints;
pub use repo::tags::TagCount;
pub use repo::users::{
    account_row_tx, clear_password_failures_tx, create_user_tx, email_taken_tx,
    set_password_hash_tx, AccountRow, AdminUserRow, DeletedAccount, LocaleSignal, LocaleSource,
    PasswordLogin, PW_LOCKOUT_THRESHOLD, PW_LOCKOUT_WINDOW_MS,
};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(&'static str),
    #[error("card limit reached: {current} of {cap} cards used")]
    CapExceeded { current: u32, cap: u32 },
}

impl StoreError {
    /// A UNIQUE/CHECK/FK violation from SQLite: "already exists" or "not
    /// allowed", as opposed to the database being broken. Callers turn
    /// it into a conflict answer instead of an internal error.
    pub fn is_constraint(&self) -> bool {
        matches!(
            self,
            StoreError::Db(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation
        )
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Runs a mutation that must touch a row: a zero row count is
/// `NotFound(what)`. Ownership-scoped UPDATEs rely on this. The WHERE
/// carries the user id, so "not yours" and "doesn't exist" both land
/// here. Takes any connection so transactions use it too.
pub fn exec_expect_row<P: rusqlite::Params>(
    conn: &Connection,
    what: &'static str,
    sql: &str,
    params: P,
) -> Result<()> {
    if conn.execute(sql, params)? == 0 {
        return Err(StoreError::NotFound(what));
    }
    Ok(())
}

/// Told after each write the store commits (`Store::on_commit`).
pub type CommitListener = Box<dyn Fn() + Send + Sync>;

pub struct Store {
    conn: Mutex<Connection>,
    extensions: Vec<Arc<dyn StoreExtension>>,
    commit_listeners: Arc<Mutex<Vec<CommitListener>>>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, Vec::new())
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::open_in_memory_with(Vec::new())
    }

    /// Opens a database the server has already brought up to this
    /// binary's schema, for a command run beside a live server: a missing
    /// file or an older schema is refused, never created or migrated
    /// under a server that may still be running the old code.
    pub fn open_current(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(StoreError::Invalid(format!(
                "no database at {}",
                path.display()
            )));
        }
        let conn = Connection::open(path)?;
        let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let expected = migrations::core_version();
        if found < expected {
            return Err(StoreError::Invalid(format!(
                "the database is at schema {found} and this binary needs {expected}: \
                 start the server once to upgrade it, then run this again"
            )));
        }
        Self::init(conn, Vec::new())
    }

    /// Opens with extensions, which are consulted by the core's
    /// deletions from then on.
    pub fn open_with(path: &Path, extensions: Vec<Arc<dyn StoreExtension>>) -> Result<Self> {
        Self::init(Connection::open(path)?, extensions)
    }

    pub fn open_in_memory_with(extensions: Vec<Arc<dyn StoreExtension>>) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, extensions)
    }

    fn init(conn: Connection, extensions: Vec<Arc<dyn StoreExtension>>) -> Result<Self> {
        pragmas::apply(&conn)?;
        migrations::run(&conn, &extensions)?;
        let commit_listeners: Arc<Mutex<Vec<CommitListener>>> = Arc::default();
        let listeners = commit_listeners.clone();
        // One hook on the one connection sees every write, whichever
        // path made it; returning false lets the commit go ahead.
        conn.commit_hook(Some(move || {
            for listener in listeners.lock().iter() {
                listener();
            }
            false
        }))?;
        Ok(Self {
            conn: Mutex::new(conn),
            extensions,
            commit_listeners,
        })
    }

    /// Calls `listener` whenever a write commits (a transaction, or a
    /// statement outside one), for a downstream extension that reacts to
    /// changes wherever they were made. It runs inside the commit, with
    /// the connection locked: it must only signal (wake a task, set a
    /// flag), never touch the store.
    pub fn on_commit(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.commit_listeners.lock().push(Box::new(listener));
    }

    /// The core's schema cursor (PRAGMA user_version).
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn()
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    /// Every recorded migration as `(extension, name, applied_at)`, in
    /// the order they were recorded.
    pub fn applied_migrations(&self) -> Result<Vec<(String, String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT extension, name, applied_at FROM schema_migrations
             ORDER BY applied_at, rowid",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The one connection, locked. A panic while the guard is held cannot
    /// poison it (parking_lot has no such state); an open transaction is
    /// rolled back as its guard unwinds, so the next caller sees a clean
    /// connection rather than a permanently panicking lock. A store
    /// extension's queries take it as the store's own do.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Runs `f` with the connection locked, for an extension's own
    /// queries. `f` must not call any other `Store` method: the mutex is
    /// not reentrant.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R> {
        let conn = self.conn();
        f(&conn)
    }

    /// Same, inside one transaction: commits on Ok, rolls back on Err.
    pub fn with_tx<R>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<R>) -> Result<R> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// A real database round-trip, for health checks.
    pub fn health_check(&self) -> Result<()> {
        let one: i64 = self.conn().query_row("SELECT 1", [], |r| r.get(0))?;
        if one != 1 {
            return Err(StoreError::Invalid("SELECT 1 != 1".into()));
        }
        Ok(())
    }

    /// Test hook: raw SQL, used to prove triggers hold even outside the
    /// typed API. Not for production paths.
    #[doc(hidden)]
    pub fn raw_execute_for_tests(&self, sql: &str) -> Result<usize> {
        Ok(self.conn().execute(sql, [])?)
    }
}
