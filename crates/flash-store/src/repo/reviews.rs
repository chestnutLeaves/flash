//! Card scheduling state, the immutable review log, and study sessions.

use std::collections::HashMap;

use flash_core::queue::{Due, QueueEntry, StudyScope};
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

/// 自选刷题会话列表所需的持久化摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomStudySummary {
    pub session: SessionId,
    pub mode: String,
    pub card_order: String,
    pub scope: String,
    pub position: u32,
    pub total: u32,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// One replayed Anki review, ready for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedReview {
    pub card: CardId,
    pub reviewed_at_ms: i64,
    pub outcome: ReviewOutcome,
    /// How long it took to answer, when the other app kept it.
    pub answer_ms: Option<i64>,
}

/// How long a card may count as on screen before its grade: past ten
/// minutes the page was left open, not the card thought about.
pub const ANSWER_MS_MAX: i64 = 10 * 60 * 1000;

/// The one INSERT into review_log. The row lands only when the card is
/// the user's; the conflict target is the import index (user, time,
/// source `import`), so a replayed review the user already holds is
/// skipped while Flash's own reviews never collide. True when written.
/// A written row counts on its session (`reviews`, `last_review_at`):
/// a session's length is read from its reviews, since only some
/// clients end one.
#[allow(clippy::too_many_arguments)]
pub fn insert_review(
    conn: &rusqlite::Connection,
    user: UserId,
    card: CardId,
    outcome: &ReviewOutcome,
    reviewed_at_ms: i64,
    source: &str,
    client: Option<&str>,
    session: Option<SessionId>,
    answer_ms: Option<i64>,
) -> Result<bool> {
    let n = conn.execute(
        "INSERT INTO review_log (card_id, user_id, reviewed_at, rating, phase_before,
           elapsed_ms, stability_after, difficulty_after, due_after, source, session_id,
           client, answer_ms)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
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
            answer_ms.map(|ms| ms.clamp(0, ANSWER_MS_MAX)),
        ],
    )?;
    if n == 1 {
        if let Some(session) = session {
            conn.execute(
                "UPDATE study_sessions SET reviews = reviews + 1,
                   last_review_at = MAX(COALESCE(last_review_at, 0), ?1)
                 WHERE id = ?2 AND user_id = ?3",
                params![reviewed_at_ms, session.0, user.raw()],
            )?;
        }
    }
    Ok(n == 1)
}

/// The one UPDATE of a card's scheduling state, for the user's own card.
pub fn write_card_state(
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
    /// 区分自选会话和普通到期会话，避免自由练习误走正式评分入口。
    pub fn is_custom_study(&self, user: UserId, session: SessionId) -> Result<bool> {
        Ok(self.conn().query_row("SELECT EXISTS(SELECT 1 FROM custom_study_sessions c JOIN study_sessions s ON s.id = c.session_id WHERE s.id = ?1 AND s.user_id = ?2)", params![session.0, user.raw()], |r| r.get(0))?)
    }

    /// 返回最近一个未完成的自选会话，供用户继续。
    pub fn latest_custom_study(&self, user: UserId) -> Result<Option<SessionId>> {
        Ok(self.conn().query_row("SELECT s.id FROM study_sessions s JOIN custom_study_sessions c ON c.session_id = s.id WHERE s.user_id = ?1 AND s.ended_at IS NULL ORDER BY s.id DESC LIMIT 1", [user.raw()], |r| Ok(SessionId(r.get(0)?))).optional()?)
    }

    /// 返回用户的全部刷题会话，列表页按结束状态分组。
    pub fn custom_study_summaries(&self, user: UserId) -> Result<Vec<CustomStudySummary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT s.id, c.mode, c.card_order, s.scope, c.position,
                    json_array_length(c.card_ids), s.started_at, s.ended_at
             FROM study_sessions s
             JOIN custom_study_sessions c ON c.session_id = s.id
             WHERE s.user_id = ?1
             ORDER BY COALESCE(s.ended_at, 9223372036854775807) DESC, s.id DESC",
        )?;
        let rows = stmt.query_map([user.raw()], |r| {
            Ok(CustomStudySummary {
                session: SessionId(r.get(0)?),
                mode: r.get(1)?,
                card_order: r.get(2)?,
                scope: r.get(3)?,
                position: r.get::<_, i64>(4)?.max(0) as u32,
                total: r.get::<_, i64>(5)?.max(0) as u32,
                started_at: r.get(6)?,
                ended_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    /// 按学习范围选出可用卡片，自选学习不受到期时间和每日额度限制。
    pub fn custom_study_cards(
        &self,
        user: UserId,
        scope: &StudyScope,
        random: bool,
        limit: u32,
    ) -> Result<Vec<CardId>> {
        let conn = self.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope)?;
        let order = if random { "RANDOM()" } else { "c.id" };
        let sql = format!("SELECT c.id FROM cards c JOIN decks d ON d.id = c.deck_id WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE} {STUDYABLE} ORDER BY {order} LIMIT ?5");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![
                user.raw(),
                deck_filter,
                tag_filter,
                tag_count,
                i64::from(limit)
            ],
            |r| Ok(CardId(r.get(0)?)),
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 保存固定的出题顺序，刷新后仍能继续当前自选学习。
    pub fn start_custom_study(
        &self,
        user: UserId,
        scope: &StudyScope,
        mode: &str,
        card_order: &str,
        cards: &[CardId],
        now_ms: i64,
    ) -> Result<SessionId> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO study_sessions (user_id, started_at, scope, surface) VALUES (?1, ?2, ?3, 'web')", params![user.raw(), now_ms, scope.as_string()])?;
        let id = tx.last_insert_rowid();
        let ids: Vec<i64> = cards.iter().map(|card| card.0).collect();
        let json = serde_json::json!(ids).to_string();
        tx.execute(
            "INSERT INTO custom_study_sessions (session_id, mode, card_ids, card_order)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, mode, json, card_order],
        )?;
        if cards.is_empty() {
            tx.execute(
                "UPDATE study_sessions SET ended_at = ?1 WHERE id = ?2",
                params![now_ms, id],
            )?;
        }
        tx.commit()?;
        Ok(SessionId(id))
    }

    /// 返回自选会话的模式、固定顺序和当前位置。
    pub fn custom_study_session(
        &self,
        user: UserId,
        session: SessionId,
        now_ms: i64,
    ) -> Result<(String, Vec<CardId>, usize)> {
        let conn = self.conn();
        let row: Option<(String, String, i64)> = conn.query_row("SELECT c.mode, c.card_ids, c.position FROM custom_study_sessions c JOIN study_sessions s ON s.id = c.session_id WHERE s.id = ?1 AND s.user_id = ?2", params![session.0, user.raw()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
        let (mode, json, pos) = row.ok_or(StoreError::NotFound("study_session"))?;
        let ids: Vec<i64> = serde_json::from_str(&json)
            .map_err(|_| StoreError::Invalid("invalid custom study session".into()))?;
        let mut position = pos.max(0) as usize;
        // 会话期间卡片可能被删除或归档，跳过不再可学习的卡片。
        while let Some(id) = ids.get(position) {
            let active: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM cards c JOIN decks d ON d.id = c.deck_id WHERE c.id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL AND c.suspended = 0 AND d.archived_at IS NULL)", params![id, user.raw()], |r| r.get(0))?;
            if active {
                break;
            }
            position += 1;
        }
        if position != pos.max(0) as usize {
            let changed = conn.execute("UPDATE custom_study_sessions SET position = ?1 WHERE session_id = ?2 AND position = ?3", params![position as i64, session.0, pos])?;
            if changed == 0 {
                drop(conn);
                return self.custom_study_session(user, session, now_ms);
            }
            if position >= ids.len() {
                conn.execute("UPDATE study_sessions SET ended_at = COALESCE(ended_at, ?1) WHERE id = ?2 AND user_id = ?3", params![now_ms, session.0, user.raw()])?;
            }
        }
        Ok((mode, ids.into_iter().map(CardId).collect(), position))
    }

    /// 原子写入正式复习结果并推进进度；自由练习仅推进进度。
    #[allow(clippy::too_many_arguments)]
    pub fn advance_custom_study(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        outcome: Option<&ReviewOutcome>,
        expected_last: Option<i64>,
        now_ms: i64,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let row: Option<(String, String, i64)> = tx.query_row("SELECT c.mode, c.card_ids, c.position FROM custom_study_sessions c JOIN study_sessions s ON s.id = c.session_id WHERE s.id = ?1 AND s.user_id = ?2 AND s.ended_at IS NULL", params![session.0, user.raw()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
        let (mode, json, pos) = row.ok_or(StoreError::NotFound("study_session"))?;
        let ids: Vec<i64> = serde_json::from_str(&json)
            .map_err(|_| StoreError::Invalid("invalid custom study session".into()))?;
        if ids.get(pos.max(0) as usize) != Some(&card.0) {
            return Err(StoreError::Invalid(
                "card is not the current custom study card".into(),
            ));
        }
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM cards c JOIN decks d ON d.id = c.deck_id WHERE c.id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL AND c.suspended = 0 AND d.archived_at IS NULL)", params![card.0, user.raw()], |r| r.get(0))?;
        if !active {
            return Err(StoreError::NotFound("card"));
        }
        match (mode.as_str(), outcome) {
            ("review", Some(outcome)) => {
                let current: Option<i64> = tx.query_row("SELECT s.last_review FROM card_state s JOIN cards c ON c.id = s.card_id WHERE c.id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL AND c.suspended = 0", params![card.0, user.raw()], |r| r.get(0))?;
                if current != expected_last {
                    return Err(StoreError::Invalid(
                        "card was reviewed elsewhere; refresh the page".into(),
                    ));
                }
                if !insert_review(
                    &tx,
                    user,
                    card,
                    outcome,
                    now_ms,
                    "web",
                    None,
                    Some(session),
                    None,
                )? {
                    return Err(StoreError::NotFound("card"));
                }
                write_card_state(&tx, user, card, &outcome.state)?;
            }
            ("practice", None) => {}
            _ => return Err(StoreError::Invalid("invalid custom study mode".into())),
        }
        tx.execute(
            "UPDATE custom_study_sessions SET position = position + 1 WHERE session_id = ?1",
            [session.0],
        )?;
        if pos as usize + 1 >= ids.len() {
            tx.execute(
                "UPDATE study_sessions SET ended_at = ?1 WHERE id = ?2 AND user_id = ?3",
                params![now_ms, session.0, user.raw()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
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
    /// the deck whose budget it spends. A scope's tags are an
    /// intersection, and they narrow the deck when it names one: a card
    /// is a candidate only when it carries every tag. Ordering and
    /// new-card budgeting happen in flash_core::queue over this
    /// projection.
    ///
    /// Of each deck's new cards only the first `new_per_deck` by id are
    /// kept, the ones flash_core::queue would take first
    /// (`Budgets::most_new_per_deck`), so an account holding thousands of
    /// unstudied cards hands the queue the few a day can use. The count
    /// beside the entries is how many new cards in scope that left out:
    /// today's limits hold them back as surely as the ones the queue
    /// refuses.
    pub fn queue_entries(
        &self,
        user: UserId,
        scope: &StudyScope,
        due: Due,
        new_per_deck: u32,
    ) -> Result<(Vec<QueueEntry>, u32)> {
        let conn = self.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope)?;
        let mut stmt = conn.prepare(&queue_sql())?;
        let learn_ahead_ms = due.now_ms + flash_core::queue::LEARN_AHEAD_MS;
        let rows = stmt.query_map(
            params![
                user.raw(),
                deck_filter,
                tag_filter,
                tag_count,
                due.reviews_before_ms,
                learn_ahead_ms
            ],
            |r| {
                Ok(QueueEntry {
                    card_id: CardId(r.get(0)?),
                    deck_id: DeckId(r.get(1)?),
                    phase: Phase::from_i64(r.get(2)?).expect("phase in 0..=3"),
                    due_ms: r.get(3)?,
                })
            },
        )?;
        // Every new card in scope is still read: the ones left out are
        // counted. `kept` restarts at each deck, so this needs the rows
        // deck by deck, as `queue_sql` orders them.
        let mut entries = Vec::new();
        let mut deck = None;
        let mut kept = 0;
        let mut left_out = 0;
        for entry in rows {
            let entry = entry?;
            if deck != Some(entry.deck_id) {
                deck = Some(entry.deck_id);
                kept = 0;
            }
            if entry.phase == Phase::New {
                if kept >= new_per_deck {
                    left_out += 1;
                    continue;
                }
                kept += 1;
            }
            entries.push(entry);
        }
        Ok((entries, left_out))
    }

    /// Whether the scope names any card at all: due or not, suspended or
    /// in an archived deck. An empty queue over a scope with cards means
    /// caught up (or set aside); over one without, there is nothing there
    /// to study yet.
    pub fn scope_has_cards(&self, user: UserId, scope: &StudyScope) -> Result<bool> {
        let conn = self.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope)?;
        Ok(conn.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM cards c JOIN decks d ON d.id = c.deck_id
                 WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE})"
            ),
            params![user.raw(), deck_filter, tag_filter, tag_count],
            |r| r.get(0),
        )?)
    }

    /// When the first card in scope that is not due by `due` comes due,
    /// among cards already seen (a new card has no due time; the daily
    /// limit decides when it comes). None when nothing is scheduled.
    pub fn next_due_after(
        &self,
        user: UserId,
        scope: &StudyScope,
        due: Due,
    ) -> Result<Option<i64>> {
        let conn = self.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope)?;
        Ok(conn.query_row(
            &format!(
                "SELECT MIN(s.due) FROM card_state s
                 JOIN cards c ON c.id = s.card_id
                 JOIN decks d ON d.id = c.deck_id
                 WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE} {STUDYABLE}
                   AND ((s.phase = 2 AND s.due >= ?5)
                        OR (s.phase IN (1, 3) AND s.due > ?6))"
            ),
            params![
                user.raw(),
                deck_filter,
                tag_filter,
                tag_count,
                due.reviews_before_ms,
                due.now_ms
            ],
            |r| r.get(0),
        )?)
    }

    /// How many cards in scope, already seen, come due in `[from_ms, before_ms)`.
    pub fn count_due_between(
        &self,
        user: UserId,
        scope: &StudyScope,
        from_ms: i64,
        before_ms: i64,
    ) -> Result<u32> {
        let conn = self.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope)?;
        let n: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM card_state s
                 JOIN cards c ON c.id = s.card_id
                 JOIN decks d ON d.id = c.deck_id
                 WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE} {STUDYABLE}
                   AND s.phase != 0 AND s.due >= ?5 AND s.due < ?6"
            ),
            params![
                user.raw(),
                deck_filter,
                tag_filter,
                tag_count,
                from_ms,
                before_ms
            ],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as u32)
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
        self.record_review_timed(
            user,
            card,
            outcome,
            reviewed_at_ms,
            source,
            client,
            session,
            None,
        )
    }

    /// `record_review` with how long the card was on screen before the
    /// grade, as the page that showed it measured (`answer_ms`, clamped
    /// to `ANSWER_MS_MAX`).
    #[allow(clippy::too_many_arguments)]
    pub fn record_review_timed(
        &self,
        user: UserId,
        card: CardId,
        outcome: &ReviewOutcome,
        reviewed_at_ms: i64,
        source: &str,
        client: Option<&str>,
        session: Option<SessionId>,
        answer_ms: Option<i64>,
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
            answer_ms,
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
                r.answer_ms,
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

    /// A session opened from `surface` (web, mcp, panel, mobile). With
    /// `reuse_ms`, an open session of this account on the same scope and
    /// surface, started within that long and not graded in yet, is
    /// handed back instead of a new one: a study page loaded again is
    /// the same sitting, not another session nobody finished.
    pub fn open_session(
        &self,
        user: UserId,
        scope: &StudyScope,
        surface: &str,
        reuse_ms: Option<i64>,
        now_ms: i64,
    ) -> Result<SessionId> {
        let conn = self.conn();
        let scope = scope.as_string();
        if let Some(window) = reuse_ms {
            let open: Option<i64> = conn
                .query_row(
                    "SELECT id FROM study_sessions
                     WHERE user_id = ?1 AND scope = ?2 AND surface = ?3
                       AND ended_at IS NULL AND reviews = 0 AND started_at > ?4
                        AND NOT EXISTS (SELECT 1 FROM custom_study_sessions c WHERE c.session_id = study_sessions.id)
                     ORDER BY started_at DESC LIMIT 1",
                    params![user.raw(), scope, surface, now_ms - window],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(id) = open {
                return Ok(SessionId(id));
            }
        }
        conn.execute(
            "INSERT INTO study_sessions (user_id, started_at, scope, surface)
             VALUES (?1, ?2, ?3, ?4)",
            params![user.raw(), now_ms, scope, surface],
        )?;
        Ok(SessionId(conn.last_insert_rowid()))
    }

    /// Ends a session; returns None if the session isn't this user's.
    /// The first end stands: ending it again changes nothing.
    pub fn end_session(&self, user: UserId, session: SessionId, now_ms: i64) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "study_session",
            "UPDATE study_sessions SET ended_at = COALESCE(ended_at, ?1)
             WHERE id = ?2 AND user_id = ?3",
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
        Ok(StudyScope::parse(&scope))
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

    /// When the session started and when its last review was, or None
    /// when it has no review (or is not this user's).
    pub fn session_span(&self, user: UserId, session: SessionId) -> Result<Option<(i64, i64)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT s.started_at, MAX(r.reviewed_at) FROM study_sessions s
                 JOIN review_log r ON r.session_id = s.id AND r.user_id = s.user_id
                 WHERE s.user_id = ?1 AND s.id = ?2",
                params![user.raw(), session.0],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .optional()?
            .and_then(|(start, last)| start.zip(last)))
    }
}

/// `queue_entries`' candidates: review cards by `due.reviews_before_ms`
/// (?5); learning steps up to the learn-ahead window (?6), which
/// flash_core::queue narrows to what is due. Deck by deck, each in id
/// order, the order new cards are taken in: idx_cards_study walks it, so
/// nothing is sorted and no card's own row is read.
fn queue_sql() -> String {
    format!(
        "SELECT s.card_id, c.deck_id, s.phase, s.due FROM card_state s
         JOIN cards c ON c.id = s.card_id
         JOIN decks d ON d.id = c.deck_id
         WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE} {STUDYABLE}
           AND (s.phase = 0 OR (s.phase = 2 AND s.due < ?5)
                OR (s.phase IN (1, 3) AND s.due <= ?6))
         ORDER BY c.deck_id, c.id"
    )
}

/// The scope's tags as a JSON list, to bind for `json_each`. A tag may
/// hold any character, so this goes through serde rather than joining.
fn tag_list(tags: &[String]) -> String {
    serde_json::json!(tags).to_string()
}

/// The cards a study scope names, over `cards c JOIN decks d`, after the
/// query's own `c.user_id = ?1` (spelled out in each statement, where
/// the scoped-SQL check reads it): live cards inside the deck's subtree
/// when the scope names one (?2), carrying every tag it names (?3,
/// counted against ?4). Bound by `scope_filters`. Suspended cards and
/// archived decks are still in it; with `STUDYABLE` after it, it is what
/// a session studies.
const NAMED_BY_SCOPE: &str = "c.deleted_at IS NULL
    AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
    AND (?3 IS NULL OR (
          SELECT COUNT(*) FROM card_tags ct JOIN tags t ON t.id = ct.tag_id
          WHERE ct.card_id = c.id AND t.user_id = ?1
            AND t.name IN (SELECT value FROM json_each(?3))) = ?4)";

/// Of the cards a scope names, the ones a session studies: unsuspended,
/// in active decks.
const STUDYABLE: &str = "AND c.suspended = 0 AND d.archived_at IS NULL";

/// The scope filters' ?2, ?3 and ?4 for a scope.
fn scope_filters(
    conn: &rusqlite::Connection,
    user: UserId,
    scope: &StudyScope,
) -> Result<(Option<String>, Option<String>, i64)> {
    let deck_filter = super::decks::subtree_filter(conn, user, scope.deck)?;
    // The fields are public, so normalize defensively: the tag count is
    // only exact over a deduplicated list.
    let tags = flash_core::queue::normalize_tags(&scope.tags);
    let tag_filter = (!tags.is_empty()).then(|| tag_list(&tags));
    Ok((deck_filter, tag_filter, tags.len() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_core::queue::{order_queue, Budgets, DeckBudget, LEARN_AHEAD_MS};
    use flash_core::{validate_card_text, CardState};

    const NOW: i64 = 1_700_000_000_000;
    const MINUTE: i64 = 60_000;

    /// The candidates as `queue_entries` read them before new cards were
    /// bounded: every one in scope.
    fn every_candidate(
        store: &Store,
        user: UserId,
        scope: &StudyScope,
        due: Due,
    ) -> Vec<QueueEntry> {
        let conn = store.conn();
        let (deck_filter, tag_filter, tag_count) = scope_filters(&conn, user, scope).unwrap();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT s.card_id, c.deck_id, s.phase, s.due FROM card_state s
                 JOIN cards c ON c.id = s.card_id
                 JOIN decks d ON d.id = c.deck_id
                 WHERE c.user_id = ?1 AND {NAMED_BY_SCOPE} {STUDYABLE}
                   AND (s.phase = 0 OR (s.phase = 2 AND s.due < ?5)
                        OR (s.phase IN (1, 3) AND s.due <= ?6))"
            ))
            .unwrap();
        let rows = stmt
            .query_map(
                params![
                    user.raw(),
                    deck_filter,
                    tag_filter,
                    tag_count,
                    due.reviews_before_ms,
                    due.now_ms + LEARN_AHEAD_MS
                ],
                |r| {
                    Ok(QueueEntry {
                        card_id: CardId(r.get(0)?),
                        deck_id: DeckId(r.get(1)?),
                        phase: Phase::from_i64(r.get(2)?).unwrap(),
                        due_ms: r.get(3)?,
                    })
                },
            )
            .unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    fn state(phase: Phase, due_ms: i64) -> CardState {
        CardState {
            phase,
            stability: Some(3.0),
            difficulty: Some(5.0),
            due_ms,
            last_review_ms: Some(NOW - 3 * 24 * 60 * MINUTE),
            reps: 2,
            lapses: 0,
        }
    }

    /// Leaving out each deck's new cards past the most any deck can take
    /// changes no queue: over subdecks, an archived subdeck, tags, every
    /// kind of due card (with ties), and budgets with per-deck limits, a
    /// boost, goal-paced decks and a deck with fewer new cards than its
    /// budget, the order is the same as over every candidate, and what is
    /// left out makes up exactly the cards the shorter list no longer
    /// holds back.
    #[test]
    fn bounding_new_cards_changes_no_queue() {
        let store = Store::open_in_memory().unwrap();
        let user = store
            .create_user("FlashTester", None, "member", NOW)
            .unwrap();
        let deck = |name: &str| store.create_deck(user, name, "", NOW).unwrap();
        let (bio, cells, old, chem, exam, lang) = (
            deck("Bio"),
            deck("Bio::Cells"),
            deck("Bio::Old"),
            deck("Chem"),
            deck("Exam"),
            deck("Lang"),
        );
        // Card by card across the decks, so their ids interleave; Chem
        // gets two.
        let mut cards: HashMap<DeckId, Vec<CardId>> = HashMap::new();
        for round in 0..12 {
            for d in [bio, cells, old, chem, exam, lang] {
                if d == chem && round >= 2 {
                    continue;
                }
                let n = cards.values().map(Vec::len).sum::<usize>();
                let tags: Vec<String> = [(n % 3 == 0, "exam"), (n % 2 == 0, "core")]
                    .into_iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, tag)| tag.to_string())
                    .collect();
                let text = validate_card_text(&format!("q{n}"), "a").unwrap();
                let id = store
                    .create_cards(user, d, &[(text, tags)], None, NOW)
                    .unwrap()[0];
                cards.entry(d).or_default().push(id);
            }
        }
        let day = 24 * 60 * MINUTE;
        store
            .set_card_states(
                user,
                &[
                    // Reviews due: two at the same moment in two decks, one
                    // later today, one tomorrow.
                    (cards[&bio][0], state(Phase::Review, NOW - day)),
                    (cards[&cells][0], state(Phase::Review, NOW - day)),
                    (cards[&bio][1], state(Phase::Review, NOW + 3 * 60 * MINUTE)),
                    (cards[&bio][2], state(Phase::Review, NOW + day)),
                    (cards[&exam][0], state(Phase::Review, NOW - 2 * day)),
                    // Steps: one due, one within the learn-ahead window,
                    // one relearning.
                    (cards[&lang][0], state(Phase::Learning, NOW - MINUTE)),
                    (cards[&lang][1], state(Phase::Learning, NOW + 5 * MINUTE)),
                    (cards[&cells][1], state(Phase::Relearning, NOW)),
                ],
            )
            .unwrap();
        store.set_deck_archived(user, old, Some(NOW)).unwrap();
        store
            .set_card_suspended(user, cards[&exam][3], true, NOW)
            .unwrap();
        store.delete_card(user, cards[&lang][4], NOW).unwrap();

        let scopes = [
            StudyScope::all(),
            StudyScope::deck(bio),
            StudyScope::deck(cells),
            StudyScope::deck(old),
            StudyScope::deck(chem),
            StudyScope::deck(exam),
            StudyScope::tagged(["exam"]),
            StudyScope::tagged(["exam", "core"]),
            StudyScope::new(Some(bio), ["core"]),
        ];
        let limit = |new: u32, reviews: u32| DeckBudget { new, reviews };
        let per_deck: [(&str, HashMap<DeckId, DeckBudget>); 4] = [
            ("account limits only", HashMap::new()),
            (
                "tight limits",
                [bio, cells, chem, exam, lang]
                    .into_iter()
                    .map(|d| (d, limit(2, 1)))
                    .collect(),
            ),
            (
                "mixed limits, Chem's above its 2 new cards",
                HashMap::from([
                    (bio, limit(0, 5)),
                    (cells, limit(4, 0)),
                    (chem, limit(9, 5)),
                    (exam, limit(30, 5)),
                    (lang, limit(1, 5)),
                ]),
            ),
            (
                "Lang boosted past the total",
                HashMap::from([(lang, limit(40, 5))]),
            ),
        ];
        let paced = [vec![], vec![exam], vec![exam, chem]];
        let dues = [Due::exact(NOW), Due::by_day(NOW, NOW + 6 * 60 * MINUTE)];

        let mut compared = 0;
        let mut bounded = 0;
        for scope in &scopes {
            for due in dues {
                let all = every_candidate(&store, user, scope, due);
                let eligible = |entries: &[QueueEntry]| {
                    entries
                        .iter()
                        .filter(|e| due.is_due(e.phase, e.due_ms))
                        .count()
                };
                let mut new_by_deck: HashMap<DeckId, u32> = HashMap::new();
                for e in all.iter().filter(|e| e.phase == Phase::New) {
                    *new_by_deck.entry(e.deck_id).or_default() += 1;
                }
                for new_total in [0, 1, 3, 8, 50] {
                    for review_total in [0, 50] {
                        for (limits, deck_budgets) in &per_deck {
                            for paced in &paced {
                                let budgets = Budgets {
                                    new_total,
                                    review_total,
                                    per_deck: deck_budgets.clone(),
                                    paced: paced.iter().copied().collect(),
                                };
                                let k = budgets.most_new_per_deck();
                                let (kept, left_out) =
                                    store.queue_entries(user, scope, due, k).unwrap();
                                let case = format!(
                                    "{scope:?}, {due:?}, totals {new_total}/{review_total}, \
                                     {limits}, paced {paced:?}"
                                );
                                assert_eq!(
                                    order_queue(&kept, due, &budgets),
                                    order_queue(&all, due, &budgets),
                                    "{case}"
                                );
                                assert_eq!(
                                    eligible(&kept) + left_out as usize,
                                    eligible(&all),
                                    "{case}"
                                );
                                // Every deck's new cards past the first k
                                // are left out, however the decks' ids
                                // interleave.
                                assert_eq!(
                                    left_out,
                                    new_by_deck
                                        .values()
                                        .map(|n| n.saturating_sub(k))
                                        .sum::<u32>(),
                                    "{case}"
                                );
                                compared += 1;
                                bounded += usize::from(left_out > 0);
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(compared, 9 * 2 * 5 * 2 * 4 * 3);
        assert_ne!(bounded, 0, "the bound never left a card out");
    }

    /// What every grade and dashboard view asks walks idx_cards_study and
    /// sorts nothing, so an account's cards cost an index step each.
    #[test]
    fn the_study_queries_walk_the_study_index() {
        let store = Store::open_in_memory().unwrap();
        let conn = store.conn();
        for sql in [
            queue_sql(),
            crate::repo::deck_goals::GOAL_COUNTS.to_string(),
            crate::repo::decks::LIST_DECKS.to_string(),
        ] {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let unbound = vec![rusqlite::types::Null; stmt.parameter_count()];
            let plan = stmt
                .query_map(rusqlite::params_from_iter(unbound), |r| {
                    r.get::<_, String>(3)
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .join("\n");
            assert!(plan.contains("idx_cards_study"), "{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{plan}");
        }
    }
}
