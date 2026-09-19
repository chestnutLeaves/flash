//! Card scheduling state, the immutable review log, and study sessions.

use std::collections::HashMap;

use flash_core::queue::{QueueEntry, StudyScope};
use flash_core::{CardId, CardState, DeckId, Phase, ReviewOutcome, SessionId, UserId};
use rusqlite::{params, OptionalExtension};

use crate::{exec_expect_row, Result, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionStats {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

/// One replayed Anki review, ready for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedReview {
    pub card: CardId,
    pub reviewed_at_ms: i64,
    pub outcome: ReviewOutcome,
}

/// The one INSERT into review_log. The row lands only when the card is
/// the user's; the conflict target is the import index (user, time,
/// source `import`), so a replayed review the user already holds is
/// skipped while Flash's own reviews never collide. True when written.
#[allow(clippy::too_many_arguments)]
fn insert_review(
    conn: &rusqlite::Connection,
    user: UserId,
    card: CardId,
    outcome: &ReviewOutcome,
    reviewed_at_ms: i64,
    source: &str,
    client: Option<&str>,
    session: Option<SessionId>,
) -> Result<bool> {
    let n = conn.execute(
        "INSERT INTO review_log (card_id, user_id, reviewed_at, rating, phase_before,
           elapsed_ms, stability_after, difficulty_after, due_after, source, session_id,
           client)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
         WHERE EXISTS (SELECT 1 FROM cards c WHERE c.id = ?1 AND c.user_id = ?2)
         ON CONFLICT (user_id, reviewed_at) WHERE source = 'import' DO NOTHING",
        params![
            card.0,
            user.raw(),
            reviewed_at_ms,
            outcome.rating.as_i64(),
            outcome.phase_before.as_i64(),
            outcome.elapsed_ms,
            outcome.stability_after as f64,
            outcome.difficulty_after as f64,
            outcome.due_after_ms,
            source,
            session.map(|s| s.0),
            client,
        ],
    )?;
    Ok(n == 1)
}

/// The one UPDATE of a card's scheduling state, for the user's own card.
fn write_card_state(
    conn: &rusqlite::Connection,
    user: UserId,
    card: CardId,
    state: &CardState,
) -> Result<()> {
    exec_expect_row(
        conn,
        "card_state",
        "UPDATE card_state SET phase = ?1, stability = ?2, difficulty = ?3,
           due = ?4, last_review = ?5, reps = ?6, lapses = ?7
         WHERE card_id = ?8
           AND EXISTS (SELECT 1 FROM cards c WHERE c.id = ?8 AND c.user_id = ?9)",
        params![
            state.phase.as_i64(),
            state.stability.map(|v| v as f64),
            state.difficulty.map(|v| v as f64),
            state.due_ms,
            state.last_review_ms,
            state.reps as i64,
            state.lapses as i64,
            card.0,
            user.raw(),
        ],
    )
}

impl Store {
    pub fn get_card_state(&self, user: UserId, card: CardId) -> Result<CardState> {
        self.conn()
            .query_row(
                "SELECT s.phase, s.stability, s.difficulty, s.due, s.last_review, s.reps, s.lapses
                 FROM card_state s JOIN cards c ON c.id = s.card_id
                 WHERE s.card_id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| {
                    Ok(CardState {
                        phase: Phase::from_i64(r.get(0)?).expect("phase in 0..=3"),
                        stability: r.get::<_, Option<f64>>(1)?.map(|v| v as f32),
                        difficulty: r.get::<_, Option<f64>>(2)?.map(|v| v as f32),
                        due_ms: r.get(3)?,
                        last_review_ms: r.get(4)?,
                        reps: r.get::<_, i64>(5)? as u32,
                        lapses: r.get::<_, i64>(6)? as u32,
                    })
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound("card_state"))
    }

    /// Candidate cards for a study queue: everything in scope that is New
    /// or has come due, in an active deck (an archived deck's cards are
    /// out of every scope, the deck's own included). A deck's scope holds
    /// its subdecks: each entry names the deck the card lives in, which is
    /// the deck whose budget it spends. Ordering and new-card budgeting
    /// happen in flash_core::queue over this projection.
    pub fn queue_entries(
        &self,
        user: UserId,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<Vec<QueueEntry>> {
        let conn = self.conn();
        let (deck, tag_filter) = match scope {
            StudyScope::All => (None, None),
            StudyScope::Deck(id) => (Some(*id), None),
            StudyScope::Tag(tag) => (None, Some(tag.trim().to_lowercase())),
        };
        let deck_filter = super::cards::subtree_filter(&conn, user, deck)?;
        let mut stmt = conn.prepare(
            "SELECT s.card_id, c.deck_id, s.phase, s.due FROM card_state s
             JOIN cards c ON c.id = s.card_id
             JOIN decks d ON d.id = c.deck_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL AND c.suspended = 0
               AND d.archived_at IS NULL
               AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
               AND (?3 IS NULL OR EXISTS (
                     SELECT 1 FROM card_tags ct JOIN tags t ON t.id = ct.tag_id
                     WHERE ct.card_id = c.id AND t.name = ?3))
               AND (s.phase = 0 OR s.due <= ?4
                    OR (s.phase IN (1, 3) AND s.due <= ?5))",
        )?;
        let learn_ahead_ms = now_ms + flash_core::queue::LEARN_AHEAD_MS;
        let rows = stmt.query_map(
            params![user.raw(), deck_filter, tag_filter, now_ms, learn_ahead_ms],
            |r| {
                Ok(QueueEntry {
                    card_id: CardId(r.get(0)?),
                    deck_id: DeckId(r.get(1)?),
                    phase: Phase::from_i64(r.get(2)?).expect("phase in 0..=3"),
                    due_ms: r.get(3)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// How many new cards were introduced (first-ever review) since `since_ms`.
    /// Used to enforce the daily new-card budget across sessions.
    pub fn new_cards_introduced_since(&self, user: UserId, since_ms: i64) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM review_log
             WHERE user_id = ?1 AND phase_before = 0 AND reviewed_at >= ?2",
            params![user.raw(), since_ms],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Per-deck consumption of today's limits since `since_ms`: new cards
    /// introduced (`phase_before = 0`) and reviews answered (review or
    /// relearning phase: learning steps never count, as in Anki).
    /// Returns deck -> (new_introduced, reviews_done).
    pub fn daily_consumption_by_deck(
        &self,
        user: UserId,
        since_ms: i64,
    ) -> Result<HashMap<DeckId, (u32, u32)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT c.deck_id,
                    SUM(CASE WHEN l.phase_before = 0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN l.phase_before IN (2, 3) THEN 1 ELSE 0 END)
             FROM review_log l JOIN cards c ON c.id = l.card_id
             WHERE l.user_id = ?1 AND l.reviewed_at >= ?2
             GROUP BY c.deck_id",
        )?;
        let rows = stmt.query_map(params![user.raw(), since_ms], |r| {
            Ok((
                DeckId(r.get(0)?),
                (r.get::<_, i64>(1)? as u32, r.get::<_, i64>(2)? as u32),
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Persists one review atomically: append to review_log, update
    /// card_state. The study path; the only other writer of the log is
    /// `import_reviews`. `source` is the surface (`web`, `mobile`, `mcp`);
    /// `client` narrows it when the surface has several (an MCP client's
    /// name, the app's platform), None otherwise.
    #[allow(clippy::too_many_arguments)]
    pub fn record_review(
        &self,
        user: UserId,
        card: CardId,
        outcome: &ReviewOutcome,
        reviewed_at_ms: i64,
        source: &str,
        client: Option<&str>,
        session: Option<SessionId>,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if !insert_review(
            &tx,
            user,
            card,
            outcome,
            reviewed_at_ms,
            source,
            client,
            session,
        )? {
            return Err(StoreError::NotFound("card"));
        }
        write_card_state(&tx, user, card, &outcome.state)?;
        tx.commit()?;
        Ok(())
    }

    /// Appends replayed Anki reviews (source `import`, no session, no
    /// client) in one transaction and returns how many were new: a
    /// timestamp this user already imported is skipped by the log's
    /// import index, so importing the same collection twice adds
    /// nothing, and a replay cut short is repaired by running it again.
    /// Callers write in chunks (the connection is shared) and set the
    /// cards' states afterwards with `set_card_states`.
    pub fn import_reviews(&self, user: UserId, reviews: &[ImportedReview]) -> Result<usize> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let mut written = 0;
        for r in reviews {
            if insert_review(
                &tx,
                user,
                r.card,
                &r.outcome,
                r.reviewed_at_ms,
                "import",
                None,
                None,
            )? {
                written += 1;
            }
        }
        tx.commit()?;
        Ok(written)
    }

    /// Overwrites the scheduling state of a batch of cards in one
    /// transaction: the progress-import path, after its log rows.
    pub fn set_card_states(&self, user: UserId, states: &[(CardId, CardState)]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (card, state) in states {
            write_card_state(&tx, user, *card, state)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// (reviewed_at, rating, phase_before) rows since a timestamp, for stats.
    pub fn reviews_since(&self, user: UserId, since_ms: i64) -> Result<Vec<(i64, i64, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT reviewed_at, rating, phase_before FROM review_log
             WHERE user_id = ?1 AND reviewed_at >= ?2 ORDER BY reviewed_at",
        )?;
        let rows = stmt.query_map(params![user.raw(), since_ms], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// All-time review counts per 15-minute UTC bucket, oldest first, as
    /// (bucket start ms, count). A bucket never straddles a study-day
    /// boundary (zone offsets and the day cutoff are multiples of a
    /// quarter hour), so the caller folds buckets into local days; years
    /// of history come back as a few thousand rows, not one per review.
    pub fn review_buckets(&self, user: UserId) -> Result<Vec<(i64, u32)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT (reviewed_at / 900000) * 900000 AS bucket_ms, COUNT(*)
             FROM review_log WHERE user_id = ?1
             GROUP BY bucket_ms ORDER BY bucket_ms",
        )?;
        let rows = stmt.query_map([user.raw()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u32))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Due timestamps for non-new cards in a window, for upcoming-load
    /// stats. An archived deck's cards are not coming up, so they are not
    /// load.
    pub fn due_between(&self, user: UserId, from_ms: i64, to_ms: i64) -> Result<Vec<i64>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT s.due FROM card_state s JOIN cards c ON c.id = s.card_id
             JOIN decks d ON d.id = c.deck_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL AND c.suspended = 0
               AND d.archived_at IS NULL
               AND s.phase != 0 AND s.due >= ?2 AND s.due < ?3",
        )?;
        let rows = stmt.query_map(params![user.raw(), from_ms, to_ms], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn start_session(
        &self,
        user: UserId,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<SessionId> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO study_sessions (user_id, started_at, scope) VALUES (?1, ?2, ?3)",
            params![user.raw(), now_ms, scope.as_string()],
        )?;
        Ok(SessionId(conn.last_insert_rowid()))
    }

    /// Ends a session; returns None if the session isn't this user's.
    pub fn end_session(&self, user: UserId, session: SessionId, now_ms: i64) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "study_session",
            "UPDATE study_sessions SET ended_at = ?1 WHERE id = ?2 AND user_id = ?3",
            params![now_ms, session.0, user.raw()],
        )
    }

    pub fn get_session_scope(&self, user: UserId, session: SessionId) -> Result<StudyScope> {
        let scope: String = self
            .conn()
            .query_row(
                "SELECT scope FROM study_sessions WHERE id = ?1 AND user_id = ?2",
                params![session.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound("study_session"))?;
        Ok(parse_scope(&scope))
    }

    /// Whether a session of the user's has not been ended; another
    /// user's session is not found.
    pub fn session_open(&self, user: UserId, session: SessionId) -> Result<bool> {
        self.conn()
            .query_row(
                "SELECT ended_at IS NULL FROM study_sessions WHERE id = ?1 AND user_id = ?2",
                params![session.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound("study_session"))
    }

    pub fn session_stats(&self, user: UserId, session: SessionId) -> Result<SessionStats> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT rating, COUNT(*) FROM review_log
             WHERE user_id = ?1 AND session_id = ?2 GROUP BY rating",
        )?;
        let mut stats = SessionStats::default();
        let rows = stmt.query_map(params![user.raw(), session.0], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u32))
        })?;
        for row in rows {
            let (rating, n) = row?;
            stats.reviewed += n;
            match rating {
                1 => stats.again += n,
                2 => stats.hard += n,
                3 => stats.good += n,
                4 => stats.easy += n,
                _ => {}
            }
        }
        Ok(stats)
    }
}

fn parse_scope(s: &str) -> StudyScope {
    if let Some(id) = s.strip_prefix("deck:").and_then(|v| v.parse().ok()) {
        StudyScope::Deck(flash_core::DeckId(id))
    } else if let Some(tag) = s.strip_prefix("tag:") {
        StudyScope::Tag(tag.to_string())
    } else {
        StudyScope::All
    }
}
