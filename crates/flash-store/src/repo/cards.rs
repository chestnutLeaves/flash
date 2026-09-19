//! Decks, cards, and tags.

use flash_core::{deck_path, CardId, CardText, DeckId, NoteId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::richtext::SanitizedHtml;
use crate::{exec_expect_row, Result, Store, StoreError};

/// Outcome of a deck deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedDeck {
    /// Decks removed: the deck and the decks under it.
    pub decks: u32,
    pub cards: u32,
    /// Media hashes no user references any more; safe to remove from storage.
    pub orphan_blobs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckSummary {
    pub id: DeckId,
    pub name: String,
    pub description: String,
    /// Zero for an archived deck, whatever its cards say.
    pub due_count: u32,
    pub new_count: u32,
    /// When the deck was archived; None while it is active.
    pub archived_at: Option<i64>,
}

/// Per-deck daily-limit overrides (None = inherit the account default)
/// plus the deck's "more new cards today" boost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckLimits {
    pub new_per_day: Option<u32>,
    pub reviews_per_day: Option<u32>,
    pub boost_new: u32,
    pub boost_day: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardRow {
    pub id: CardId,
    pub deck_id: DeckId,
    pub front: String,
    pub back: String,
    /// Sanitized rich HTML for the web UI; None = plain card.
    pub front_html: Option<String>,
    pub back_html: Option<String>,
    /// Expected answer for the interactive typing UI, when the card asks
    /// for typed input.
    pub type_answer: Option<String>,
    pub suspended: bool,
    pub tags: Vec<String>,
    /// The note this card was generated from (None: standalone card).
    pub note_id: Option<NoteId>,
    /// Slot within the note (0 for standalone cards).
    pub ord: u32,
    /// Cloze-derived cards: which `{{cN}}` this card blanks.
    pub cloze_index: Option<u32>,
}

/// Rich columns: set in one batch after import, or per card by the note
/// expansion in `crate::notes`. The HTML sides are `SanitizedHtml`: the
/// store's rich columns take nothing else, so what reaches a page with
/// `|safe` is by construction what the sanitizer last produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardExtras {
    pub front_html: Option<SanitizedHtml>,
    pub back_html: Option<SanitizedHtml>,
    pub cloze_text: Option<String>,
    pub cloze_index: Option<u32>,
    pub type_answer: Option<String>,
}

impl CardExtras {
    pub fn is_empty(&self) -> bool {
        self.front_html.is_none()
            && self.back_html.is_none()
            && self.cloze_text.is_none()
            && self.type_answer.is_none()
    }
}

impl Store {
    /// Creates the deck at `name`, which is a path (`flash_core::deck_path`):
    /// `Bio::Cells` is the deck `Cells` inside `Bio`, and the decks it sits
    /// in are created with it where they do not exist yet.
    pub fn create_deck(
        &self,
        user: UserId,
        name: &str,
        description: &str,
        now_ms: i64,
    ) -> Result<DeckId> {
        self.with_tx(|tx| insert_deck_path_tx(tx, user, name, description, now_ms))
    }

    /// Removes a deck and every card in it: hard delete, unlike the
    /// per-card soft delete, because soft-deleted cards would pin the
    /// deck's FK and its UNIQUE(user_id, name) slot forever. Review history
    /// stays (review_log has no card FK); the only wrinkle is that today's
    /// per-deck consumption undercounts same-day-deleted cards, marginally
    /// re-opening the day's new-card budget. Media rows the user no longer
    /// references anywhere are dropped too, and hashes no user references
    /// come back as orphan blobs for the caller to delete from storage.
    /// `cards` counts rows removed (including previously soft-deleted ones).
    ///
    /// The decks under it go with it, deepest first, in the one
    /// transaction: a subdeck left behind would be a path into a deck that
    /// no longer exists. `decks` counts them, the deck itself included.
    pub fn delete_deck(&self, user: UserId, deck: DeckId) -> Result<DeletedDeck> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let result = self.delete_deck_tx(&tx, user, deck)?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn delete_deck_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        deck: DeckId,
    ) -> Result<DeletedDeck> {
        let decks = subtree(tx, user, deck)?;
        if decks.is_empty() {
            return Err(StoreError::NotFound("deck"));
        }
        let mut cards = 0;
        for (deck, _) in &decks {
            cards += self.delete_deck_rows_tx(tx, user, *deck)?;
        }
        Ok(DeletedDeck {
            decks: decks.len() as u32,
            cards,
            orphan_blobs: self.collect_unlinked_media_tx(tx, user)?,
        })
    }

    /// The deck's rows and everything that hangs off its cards; returns
    /// how many cards went.
    fn delete_deck_rows_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        deck: DeckId,
    ) -> Result<u32> {
        require_deck(tx, user, deck)?;
        for sql in [
            "DELETE FROM card_media WHERE card_id IN
               (SELECT id FROM cards WHERE deck_id = ?1 AND user_id = ?2)",
            "DELETE FROM card_tags WHERE card_id IN
               (SELECT id FROM cards WHERE deck_id = ?1 AND user_id = ?2)",
            "DELETE FROM card_state WHERE card_id IN
               (SELECT id FROM cards WHERE deck_id = ?1 AND user_id = ?2)",
        ] {
            tx.execute(sql, params![deck.0, user.raw()])?;
        }
        let cards = tx.execute(
            "DELETE FROM cards WHERE deck_id = ?1 AND user_id = ?2",
            params![deck.0, user.raw()],
        )?;
        // A kept draft remembers the note it made and a generation job
        // remembers its deck; both references are dropped rather than
        // the rows, so the job's review page and the ledger survive
        // the deck and the user picks another deck at keep.
        tx.execute(
            "UPDATE ai_drafts SET note_id = NULL WHERE user_id = ?2 AND note_id IN
               (SELECT id FROM notes WHERE deck_id = ?1 AND user_id = ?2)",
            params![deck.0, user.raw()],
        )?;
        tx.execute(
            "DELETE FROM notes WHERE deck_id = ?1 AND user_id = ?2",
            params![deck.0, user.raw()],
        )?;
        tx.execute(
            "UPDATE ai_jobs SET deck_id = NULL WHERE deck_id = ?1 AND user_id = ?2",
            params![deck.0, user.raw()],
        )?;
        for ext in &self.extensions {
            ext.before_delete_deck(tx, user, deck)?;
        }
        tx.execute(
            "DELETE FROM decks WHERE id = ?1 AND user_id = ?2",
            params![deck.0, user.raw()],
        )?;
        Ok(cards as u32)
    }

    /// Run once after a deletion's rows are gone; returns the hashes
    /// nothing references any more.
    fn collect_unlinked_media_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
    ) -> Result<Vec<String>> {
        // Media GC: rows of this user with no remaining card link go away;
        // their hashes are orphans only if nothing else holds them. Rows
        // younger than an hour are left alone: an upload the editor made
        // in another tab is linked only when its note is saved, and
        // deleting a different deck must not eat it (the daily sweep
        // takes it if it is never linked).
        let unlinked: Vec<(i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT m.id, m.sha256 FROM media m
                 WHERE m.user_id = ?1
                   AND m.created_at < (unixepoch() * 1000) - 3600000
                   AND NOT EXISTS (SELECT 1 FROM card_media cm WHERE cm.media_id = m.id)
                   AND NOT EXISTS (SELECT 1 FROM ai_figures f WHERE f.media_id = m.id)",
            )?;
            let rows = stmt.query_map([user.raw()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, _) in &unlinked {
            tx.execute(
                "DELETE FROM media WHERE id = ?1 AND user_id = ?2",
                params![id, user.raw()],
            )?;
        }
        let mut orphan_blobs = Vec::new();
        for (_, sha) in unlinked {
            if self.blob_refs(tx, &sha)? == 0 && !orphan_blobs.contains(&sha) {
                orphan_blobs.push(sha);
            }
        }
        Ok(orphan_blobs)
    }

    pub fn find_deck_by_name(&self, user: UserId, name: &str) -> Result<Option<DeckId>> {
        find_deck(&self.conn(), user, name)
    }

    /// Limits for every deck the user owns.
    pub fn all_deck_limits(&self, user: UserId) -> Result<Vec<(DeckId, DeckLimits)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, new_per_day, reviews_per_day, boost_new, boost_day
             FROM decks WHERE user_id = ?1",
        )?;
        let rows = stmt.query_map([user.raw()], |r| {
            Ok((
                DeckId(r.get(0)?),
                DeckLimits {
                    new_per_day: r.get::<_, Option<i64>>(1)?.map(|n| n as u32),
                    reviews_per_day: r.get::<_, Option<i64>>(2)?.map(|n| n as u32),
                    boost_new: r.get::<_, i64>(3)? as u32,
                    boost_day: r.get(4)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_deck_limits(&self, user: UserId, deck: DeckId) -> Result<DeckLimits> {
        self.conn()
            .query_row(
                "SELECT new_per_day, reviews_per_day, boost_new, boost_day
                 FROM decks WHERE id = ?1 AND user_id = ?2",
                params![deck.0, user.raw()],
                |r| {
                    Ok(DeckLimits {
                        new_per_day: r.get::<_, Option<i64>>(0)?.map(|n| n as u32),
                        reviews_per_day: r.get::<_, Option<i64>>(1)?.map(|n| n as u32),
                        boost_new: r.get::<_, i64>(2)? as u32,
                        boost_day: r.get(3)?,
                    })
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound("deck"))
    }

    /// Per-deck overrides; None clears an override (inherit the account).
    pub fn set_deck_limits(
        &self,
        user: UserId,
        deck: DeckId,
        new_per_day: Option<u32>,
        reviews_per_day: Option<u32>,
    ) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "deck",
            "UPDATE decks SET new_per_day = ?1, reviews_per_day = ?2
             WHERE id = ?3 AND user_id = ?4",
            params![new_per_day, reviews_per_day, deck.0, user.raw()],
        )
    }

    /// Whether every one of `paths` could be created, or exists: what
    /// `create_deck` would refuse, asked of the whole set before any of it
    /// is made. For a caller that fills many decks outside one transaction
    /// (an import), so a path that cannot be made stops it before the
    /// first card instead of after half of them. The cap counts the decks
    /// the paths would make on the way, once each.
    pub fn check_deck_paths(&self, user: UserId, paths: &[String]) -> Result<()> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name, archived_at FROM decks WHERE user_id = ?1")?;
        let existing: std::collections::HashMap<String, Option<i64>> = stmt
            .query_map([user.raw()], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut wanted = Vec::new();
        for path in paths {
            let Some(path) = deck_path::normalise(path) else {
                return Err(StoreError::Invalid("deck name is empty".into()));
            };
            if path.len() > deck_path::MAX_LEN {
                return Err(StoreError::Invalid(format!(
                    "the deck name \"{path}\" is longer than {} bytes",
                    deck_path::MAX_LEN
                )));
            }
            if !existing.contains_key(&path) {
                wanted.push(path);
            }
        }
        for path in &wanted {
            for ancestor in deck_path::ancestors(path) {
                if let Some(Some(_)) = existing.get(ancestor) {
                    return Err(StoreError::Invalid(format!(
                        "Your deck \"{ancestor}\" is archived. Unarchive it, or pick another name."
                    )));
                }
            }
        }
        let made = deck_path::closure(wanted.iter().map(String::as_str))
            .into_iter()
            .filter(|path| !existing.contains_key(path))
            .count();
        if existing.len() + made > MAX_DECKS_PER_USER as usize {
            return Err(StoreError::Invalid(format!(
                "an account may have at most {MAX_DECKS_PER_USER} decks"
            )));
        }
        Ok(())
    }

    /// Gives the deck a new path, and with it every deck under it: a name
    /// is a path, so a rename and a move are the one operation. Rejects an
    /// empty name, a path inside the deck itself, a path another deck
    /// already has (a friendlier error than the UNIQUE trip; two decks are
    /// never merged), and a move that would push a subdeck's path past the
    /// bound. The deck the new path sits in is made when it does not exist.
    pub fn rename_deck(&self, user: UserId, deck: DeckId, name: &str, now_ms: i64) -> Result<()> {
        self.with_tx(|tx| rename_deck_tx(tx, user, deck, name, now_ms))
    }

    /// Moves the deck, with the decks under it, into `parent`, or to the
    /// top level. It keeps its own name.
    pub fn move_deck(
        &self,
        user: UserId,
        deck: DeckId,
        parent: Option<DeckId>,
        now_ms: i64,
    ) -> Result<()> {
        self.with_tx(|tx| {
            let name_of = |deck: DeckId| -> Result<String> {
                tx.query_row(
                    "SELECT name FROM decks WHERE id = ?1 AND user_id = ?2",
                    params![deck.0, user.raw()],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or(StoreError::NotFound("deck"))
            };
            let name = name_of(deck)?;
            let parent = parent.map(name_of).transpose()?.unwrap_or_default();
            let moved = format!("{parent}{}{}", deck_path::SEP, deck_path::leaf(&name));
            rename_deck_tx(tx, user, deck, &moved, now_ms)
        })
    }

    /// Archives the deck (`Some(now)`) or puts it back (`None`), and the
    /// decks under it with it. Nothing on the cards changes: an archived
    /// deck is skipped by the queue and the counts, and unarchiving hands
    /// the same review data back to them. Idempotent; a stranger's deck is
    /// not found.
    ///
    /// Archiving leaves alone a subdeck archived earlier, which keeps its
    /// own date. Unarchiving also puts back the decks this one sits in
    /// (those decks only, not what else is under them), so an active deck
    /// is never inside an archived one, in study and out of sight.
    pub fn set_deck_archived(
        &self,
        user: UserId,
        deck: DeckId,
        archived_at: Option<i64>,
    ) -> Result<()> {
        self.with_tx(|tx| {
            let decks = subtree(tx, user, deck)?;
            let Some((_, name)) = decks.iter().find(|(id, _)| *id == deck) else {
                return Err(StoreError::NotFound("deck"));
            };
            let ids = id_list(&decks);
            match archived_at {
                Some(at) => tx.execute(
                    "UPDATE decks SET archived_at = ?1
                     WHERE user_id = ?2 AND archived_at IS NULL
                       AND id IN (SELECT value FROM json_each(?3))",
                    params![at, user.raw(), ids],
                )?,
                None => tx.execute(
                    "UPDATE decks SET archived_at = NULL
                     WHERE user_id = ?1
                       AND (id IN (SELECT value FROM json_each(?2))
                            OR name IN (SELECT value FROM json_each(?3)))",
                    params![
                        user.raw(),
                        ids,
                        serde_json::json!(deck_path::ancestors(name)).to_string()
                    ],
                )?,
            };
            Ok(())
        })
    }

    /// Sets the deck's "more new cards today" boost for the study day
    /// starting at `day_ms` (replaces any earlier value).
    pub fn set_deck_boost(
        &self,
        user: UserId,
        deck: DeckId,
        extra: u32,
        day_ms: i64,
    ) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "deck",
            "UPDATE decks SET boost_new = ?1, boost_day = ?2 WHERE id = ?3 AND user_id = ?4",
            params![extra, day_ms, deck.0, user.raw()],
        )
    }

    /// How many decks the user owns, for the per-account cap.
    pub fn deck_count(&self, user: UserId) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM decks WHERE user_id = ?1",
            [user.raw()],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as u32)
    }

    /// Decks with counts of currently-due and unseen cards, by name. An
    /// archived deck keeps its place in that order; its counts are zero.
    pub fn list_decks(&self, user: UserId, now_ms: i64) -> Result<Vec<DeckSummary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT d.id, d.name, d.description,
               CASE WHEN d.archived_at IS NULL THEN
                 (SELECT COUNT(*) FROM cards c JOIN card_state s ON s.card_id = c.id
                   WHERE c.deck_id = d.id AND c.deleted_at IS NULL AND c.suspended = 0
                     AND s.phase != 0 AND s.due <= ?2)
               ELSE 0 END AS due_count,
               CASE WHEN d.archived_at IS NULL THEN
                 (SELECT COUNT(*) FROM cards c JOIN card_state s ON s.card_id = c.id
                   WHERE c.deck_id = d.id AND c.deleted_at IS NULL AND c.suspended = 0
                     AND s.phase = 0)
               ELSE 0 END AS new_count,
               d.archived_at
             FROM decks d WHERE d.user_id = ?1
             ORDER BY d.name",
        )?;
        let rows = stmt.query_map(params![user.raw(), now_ms], |r| {
            Ok(DeckSummary {
                id: DeckId(r.get(0)?),
                name: r.get(1)?,
                description: r.get(2)?,
                due_count: r.get::<_, i64>(3)? as u32,
                new_count: r.get::<_, i64>(4)? as u32,
                archived_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// (active, mature) card counts. Mature = review-phase cards whose FSRS
    /// stability is at least 21 days, Anki's traditional threshold.
    pub fn card_totals(&self, user: UserId) -> Result<(u32, u32)> {
        let (active, mature): (i64, i64) = self.conn().query_row(
            "SELECT COUNT(*),
               COUNT(CASE WHEN s.phase = 2 AND s.stability >= 21.0 THEN 1 END)
             FROM cards c JOIN card_state s ON s.card_id = c.id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL AND c.suspended = 0",
            [user.raw()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((active as u32, mature as u32))
    }

    /// Every live card with its deck name and tags, oldest first (the
    /// export surface). Never filtered by plan or cap.
    pub fn export_cards(&self, user: UserId) -> Result<Vec<crate::export::ExportCard>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT c.id, d.name, c.front, c.back,
                    c.front_html, c.back_html, c.cloze_text, c.cloze_index, c.type_answer
             FROM cards c
             JOIN decks d ON d.id = c.deck_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL
             ORDER BY c.id",
        )?;
        let rows = stmt.query_map([user.raw()], |r| {
            let cloze_text: Option<String> = r.get(6)?;
            let cloze_index: Option<i64> = r.get(7)?;
            Ok((
                r.get::<_, i64>(0)?,
                crate::export::ExportCard {
                    deck: r.get(1)?,
                    front: r.get(2)?,
                    back: r.get(3)?,
                    front_html: r.get(4)?,
                    back_html: r.get(5)?,
                    cloze: cloze_text.zip(cloze_index.map(|i| i.max(1) as u32)),
                    type_answer: r.get(8)?,
                    tags: Vec::new(),
                },
            ))
        })?;
        let mut cards: Vec<(i64, crate::export::ExportCard)> =
            rows.collect::<rusqlite::Result<_>>()?;

        let mut tag_stmt = conn.prepare(
            "SELECT ct.card_id, t.name FROM card_tags ct
             JOIN tags t ON t.id = ct.tag_id
             JOIN cards c ON c.id = ct.card_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL
             ORDER BY t.name",
        )?;
        let tag_rows = tag_stmt.query_map([user.raw()], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut by_card: std::collections::HashMap<i64, Vec<String>> =
            std::collections::HashMap::new();
        for row in tag_rows {
            let (card_id, tag) = row?;
            by_card.entry(card_id).or_default().push(tag);
        }
        Ok(cards
            .iter_mut()
            .map(|(id, card)| {
                if let Some(tags) = by_card.remove(id) {
                    card.tags = tags;
                }
                card.clone()
            })
            .collect())
    }

    /// Live cards the user has already seen (phase past New), suspended or
    /// not: suspending must not free up rotation slots for the cap gate.
    pub fn cards_in_rotation(&self, user: UserId) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM cards c JOIN card_state s ON s.card_id = c.id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL AND s.phase != 0",
            [user.raw()],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Creates cards (with their state rows and tags) in one transaction.
    /// With `cap`, the batch is rejected atomically if it would push the
    /// user's live-card count past it; the count shares the insert's
    /// transaction (and the connection Mutex), so callers' pre-checks can
    /// never be undercut by a concurrent insert.
    pub fn create_cards(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[(CardText, Vec<String>)],
        cap: Option<u32>,
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        if let Some(cap) = cap {
            let current: i64 = tx.query_row(
                "SELECT COUNT(*) FROM cards WHERE user_id = ?1 AND deleted_at IS NULL",
                [user.raw()],
                |r| r.get(0),
            )?;
            let current = current as u32;
            if current.saturating_add(cards.len() as u32) > cap {
                return Err(StoreError::CapExceeded { current, cap });
            }
        }
        let mut ids = Vec::with_capacity(cards.len());
        for (text, tags) in cards {
            check_tags(tags)?;
            tx.execute(
                "INSERT INTO cards (user_id, deck_id, front, back, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![user.raw(), deck.0, text.front, text.back, now_ms],
            )?;
            let card_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO card_state (card_id, due)
                 SELECT ?1, ?2 FROM cards WHERE id = ?1 AND user_id = ?3",
                params![card_id, now_ms, user.raw()],
            )?;
            for tag in tags {
                attach_tag(&tx, user, CardId(card_id), tag)?;
            }
            ids.push(CardId(card_id));
        }
        tx.commit()?;
        Ok(ids)
    }

    pub fn get_card(&self, user: UserId, card: CardId) -> Result<Option<CardRow>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                &format!(
                    "SELECT {CARD_COLS} FROM cards
                     WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL"
                ),
                params![card.0, user.raw()],
                card_from_row,
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some(mut row) => {
                row.tags = card_tags(&conn, user, card)?;
                Ok(Some(row))
            }
        }
    }

    pub fn update_card(
        &self,
        user: UserId,
        card: CardId,
        text: &CardText,
        now_ms: i64,
    ) -> Result<()> {
        // A plain-text edit (MCP, CSV) makes the card plain by definition:
        // the rich/cloze/typing columns describe content this edit just
        // replaced. It also detaches the card from its note, so a later
        // note edit cannot overwrite it; its siblings stay on the note.
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let note = note_of(&tx, user, card)?;
        exec_expect_row(
            &tx,
            "card",
            "UPDATE cards SET front = ?1, back = ?2, updated_at = ?3,
               front_html = NULL, back_html = NULL,
               cloze_text = NULL, cloze_index = NULL, type_answer = NULL,
               note_id = NULL, ord = 0
             WHERE id = ?4 AND user_id = ?5 AND deleted_at IS NULL",
            params![text.front, text.back, now_ms, card.0, user.raw()],
        )?;
        // A plain card references no media; the links go with the HTML.
        unlink_card_media(&tx, user, card)?;
        reap_empty_note(&tx, user, note, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    /// The original `Text\x1fExtra` cloze source of a live cloze-derived
    /// card (None for every other card).
    pub fn cloze_source(&self, user: UserId, card: CardId) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT cloze_text FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Marks imported cards suspended, mirroring their Anki state.
    pub fn suspend_cards(&self, user: UserId, cards: &[CardId], now_ms: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for card in cards {
            tx.execute(
                "UPDATE cards SET suspended = 1, updated_at = ?1
                 WHERE id = ?2 AND user_id = ?3 AND deleted_at IS NULL",
                params![now_ms, card.0, user.raw()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Writes the rich/cloze/typing columns for freshly imported cards in
    /// one transaction (ownership-guarded per row).
    pub fn set_card_extras(&self, user: UserId, items: &[(CardId, CardExtras)]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (card, extras) in items {
            write_card_extras(&tx, user, *card, extras)?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// The one statement that writes a card's rich columns, for the batch
/// above and for an extension copying cards inside its own transaction.
/// Ownership-guarded; the HTML arrives typed, so nothing but sanitizer
/// output can reach these columns from any crate.
pub fn write_card_extras(
    conn: &Connection,
    user: UserId,
    card: CardId,
    extras: &CardExtras,
) -> Result<()> {
    conn.execute(
        "UPDATE cards SET front_html = ?1, back_html = ?2,
           cloze_text = ?3, cloze_index = ?4, type_answer = ?5
         WHERE id = ?6 AND user_id = ?7 AND deleted_at IS NULL",
        params![
            extras.front_html,
            extras.back_html,
            extras.cloze_text,
            extras.cloze_index,
            extras.type_answer,
            card.0,
            user.raw()
        ],
    )?;
    Ok(())
}

impl Store {
    /// Soft delete: review_log stays valid, the card just disappears. Its
    /// media links go with it, so the blobs it alone referenced are
    /// released by the next sweep rather than kept until the deck goes.
    pub fn delete_card(&self, user: UserId, card: CardId, now_ms: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        self.delete_card_tx(&tx, user, card, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn delete_card_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        card: CardId,
        now_ms: i64,
    ) -> Result<()> {
        let note = note_of(tx, user, card)?;
        exec_expect_row(
            tx,
            "card",
            "UPDATE cards SET deleted_at = ?1 WHERE id = ?2 AND user_id = ?3 AND deleted_at IS NULL",
            params![now_ms, card.0, user.raw()],
        )?;
        unlink_card_media(tx, user, card)?;
        reap_empty_note(tx, user, note, now_ms)?;
        Ok(())
    }

    /// Live cards in `deck` and its subdecks (optionally matching
    /// `search`), for pagination.
    pub fn count_cards(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
    ) -> Result<u32> {
        let conn = self.conn();
        let decks = subtree_filter(&conn, user, deck)?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cards
             WHERE user_id = ?1 AND deleted_at IS NULL
               AND (?2 IS NULL OR deck_id IN (SELECT value FROM json_each(?2)))
               AND (?3 IS NULL OR front LIKE ?3 ESCAPE '\\' OR back LIKE ?3 ESCAPE '\\')",
            params![user.raw(), decks, like_pattern(search)],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    pub fn list_cards(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CardRow>> {
        self.list_cards_page(user, deck, search, limit, 0)
    }

    /// Newest first, `offset` rows in (one page of the deck view). A deck's
    /// view holds its subdecks' cards too; `CardRow::deck_id` says whose.
    pub fn list_cards_page(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CardRow>> {
        self.list_cards_cursor(user, deck, search, limit, offset, None)
    }

    pub fn list_cards_cursor(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
        limit: u32,
        offset: u32,
        before: Option<CardId>,
    ) -> Result<Vec<CardRow>> {
        let conn = self.conn();
        let decks = subtree_filter(&conn, user, deck)?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CARD_COLS} FROM cards
             WHERE user_id = ?1 AND deleted_at IS NULL
               AND (?2 IS NULL OR deck_id IN (SELECT value FROM json_each(?2)))
               AND (?3 IS NULL OR front LIKE ?3 ESCAPE '\\' OR back LIKE ?3 ESCAPE '\\')
               AND (?6 IS NULL OR id < ?6)
             ORDER BY id DESC LIMIT ?4 OFFSET ?5"
        ))?;
        let rows = stmt.query_map(
            params![
                user.raw(),
                decks,
                like_pattern(search),
                limit,
                offset,
                before.map(|c| c.0)
            ],
            card_from_row,
        )?;
        let mut cards: Vec<CardRow> = rows.collect::<rusqlite::Result<_>>()?;

        // Fill tags in one pass over card_tags for the returned set.
        if !cards.is_empty() {
            let placeholders = vec!["?"; cards.len()].join(",");
            let mut stmt = conn.prepare(&format!(
                "SELECT ct.card_id, t.name FROM card_tags ct
                 JOIN tags t ON t.id = ct.tag_id
                 WHERE t.user_id = ? AND ct.card_id IN ({placeholders}) ORDER BY t.name"
            ))?;
            let bound: Vec<i64> = std::iter::once(user.raw())
                .chain(cards.iter().map(|c| c.id.0))
                .collect();
            let tag_rows = stmt.query_map(rusqlite::params_from_iter(bound.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            let mut by_card: std::collections::HashMap<i64, Vec<String>> =
                std::collections::HashMap::new();
            for row in tag_rows {
                let (card_id, tag) = row?;
                by_card.entry(card_id).or_default().push(tag);
            }
            for card in &mut cards {
                if let Some(tags) = by_card.remove(&card.id.0) {
                    card.tags = tags;
                }
            }
        }
        Ok(cards)
    }
}

const CARD_COLS: &str = "id, deck_id, front, back, suspended, front_html, back_html, type_answer,
    note_id, ord, cloze_index";

fn card_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CardRow> {
    Ok(CardRow {
        id: CardId(r.get(0)?),
        deck_id: DeckId(r.get(1)?),
        front: r.get(2)?,
        back: r.get(3)?,
        suspended: r.get::<_, i64>(4)? != 0,
        front_html: r.get(5)?,
        back_html: r.get(6)?,
        type_answer: r.get(7)?,
        tags: Vec::new(),
        note_id: r.get::<_, Option<i64>>(8)?.map(NoteId),
        ord: r.get::<_, i64>(9)? as u32,
        cloze_index: r.get::<_, Option<i64>>(10)?.map(|i| i.max(1) as u32),
    })
}

fn like_pattern(search: Option<&str>) -> Option<String> {
    search.map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")))
}

fn note_of(conn: &Connection, user: UserId, card: CardId) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT note_id FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
            params![card.0, user.raw()],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten())
}

/// A note whose last live card just left is soft-deleted with it.
fn reap_empty_note(conn: &Connection, user: UserId, note: Option<i64>, now_ms: i64) -> Result<()> {
    if let Some(note) = note {
        conn.execute(
            "UPDATE notes SET deleted_at = ?1 WHERE id = ?2 AND user_id = ?3 AND deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM cards WHERE note_id = ?2 AND deleted_at IS NULL)",
            params![now_ms, note, user.raw()],
        )?;
    }
    Ok(())
}

/// The user's deck of that name. The name as given comes first, so a deck
/// stored before names were paths is still found by what it is called;
/// then the name as a path would be stored (` Bio :: Cells ` finds
/// `Bio::Cells`).
pub fn find_deck(conn: &Connection, user: UserId, name: &str) -> Result<Option<DeckId>> {
    let by_name = |name: &str| -> Result<Option<DeckId>> {
        Ok(conn
            .query_row(
                "SELECT id FROM decks WHERE user_id = ?1 AND name = ?2",
                params![user.raw(), name],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(DeckId))
    };
    let name = name.trim();
    if let Some(deck) = by_name(name)? {
        return Ok(Some(deck));
    }
    match deck_path::normalise(name) {
        Some(path) if path != name => by_name(&path),
        _ => Ok(None),
    }
}

/// Creates the deck at `path` and whichever of the decks it sits in do not
/// exist yet, inside the caller's transaction. Every way a deck comes to
/// exist goes through here, so each of them meets the same rules: the
/// path is cleaned and bounded, the per-account cap counts the decks made
/// on the way, and nothing is put inside an archived deck (an active deck
/// under an archived one would be in study and out of sight at once).
/// A path that is already a deck is refused with the sentence a rename
/// gives, where a plain name used to trip UNIQUE and reach a connector as
/// an internal error.
pub fn insert_deck_path_tx(
    conn: &Connection,
    user: UserId,
    path: &str,
    description: &str,
    now_ms: i64,
) -> Result<DeckId> {
    let Some(path) = deck_path::normalise(path) else {
        return Err(StoreError::Invalid("deck name is empty".into()));
    };
    if path.len() > deck_path::MAX_LEN {
        return Err(StoreError::Invalid(format!(
            "deck name is longer than {} bytes",
            deck_path::MAX_LEN
        )));
    }
    if find_deck(conn, user, &path)?.is_some() {
        return Err(StoreError::Invalid(
            "a deck with that name already exists".into(),
        ));
    }
    let mut missing = Vec::new();
    for ancestor in deck_path::ancestors(&path) {
        let archived_at: Option<Option<i64>> = conn
            .query_row(
                "SELECT archived_at FROM decks WHERE user_id = ?1 AND name = ?2",
                params![user.raw(), ancestor],
                |r| r.get(0),
            )
            .optional()?;
        match archived_at {
            None => missing.push(ancestor),
            Some(None) => {}
            Some(Some(_)) => {
                return Err(StoreError::Invalid(format!(
                    "Your deck \"{ancestor}\" is archived. Unarchive it, or pick another name."
                )));
            }
        }
    }
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM decks WHERE user_id = ?1",
        [user.raw()],
        |r| r.get(0),
    )?;
    if count as usize + missing.len() + 1 > MAX_DECKS_PER_USER as usize {
        return Err(StoreError::Invalid(format!(
            "an account may have at most {MAX_DECKS_PER_USER} decks"
        )));
    }
    let insert = |name: &str, description: &str| -> Result<DeckId> {
        conn.execute(
            "INSERT INTO decks (user_id, name, description, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![user.raw(), name, description, now_ms],
        )?;
        Ok(DeckId(conn.last_insert_rowid()))
    };
    for ancestor in missing {
        insert(ancestor, "")?;
    }
    insert(&path, description)
}

/// `deck` and every deck under it, as (id, name), deepest first so a
/// caller that deletes can go in order. Empty when the deck is not the
/// user's. The match is on whole segments and is exact: no `LIKE`, whose
/// wildcards a deck name may contain and whose ASCII case folding would
/// take `bio::x` for a subdeck of `Bio`.
pub fn subtree(conn: &Connection, user: UserId, deck: DeckId) -> Result<Vec<(DeckId, String)>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name FROM decks t JOIN decks r ON r.user_id = t.user_id
         WHERE r.id = ?1 AND r.user_id = ?2
           AND (t.id = r.id OR substr(t.name, 1, length(r.name) + 2) = r.name || '::')
         ORDER BY length(t.name) DESC, t.id",
    )?;
    let rows = stmt.query_map(params![deck.0, user.raw()], |r| {
        Ok((DeckId(r.get(0)?), r.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What a read over `deck` binds in place of one deck id: the ids of the
/// deck and its subdecks as a JSON list, for `IN (SELECT value FROM
/// json_each(?))`. `None` stays `None` (no deck filter); a deck that is
/// not the user's gives an empty list, which matches nothing.
pub(crate) fn subtree_filter(
    conn: &Connection,
    user: UserId,
    deck: Option<DeckId>,
) -> Result<Option<String>> {
    let Some(deck) = deck else { return Ok(None) };
    Ok(Some(id_list(&subtree(conn, user, deck)?)))
}

/// The ids of `decks` as a JSON list, to bind for `json_each`.
fn id_list(decks: &[(DeckId, String)]) -> String {
    let ids: Vec<String> = decks.iter().map(|(id, _)| id.0.to_string()).collect();
    format!("[{}]", ids.join(","))
}

/// `Store::rename_deck`, inside the caller's transaction.
fn rename_deck_tx(
    conn: &Connection,
    user: UserId,
    deck: DeckId,
    name: &str,
    now_ms: i64,
) -> Result<()> {
    let decks = subtree(conn, user, deck)?;
    let Some((_, old)) = decks.iter().find(|(id, _)| *id == deck) else {
        return Err(StoreError::NotFound("deck"));
    };
    let Some(new) = deck_path::normalise(name) else {
        return Err(StoreError::Invalid("deck name is empty".into()));
    };
    if new == *old {
        return Ok(());
    }
    if deck_path::is_descendant(&new, old) {
        return Err(StoreError::Invalid(
            "a deck cannot be moved into itself".into(),
        ));
    }
    let renamed: Vec<(DeckId, String)> = decks
        .iter()
        .map(|(id, name)| (*id, deck_path::reroot(name, old, &new)))
        .collect();
    if let Some((_, long)) = renamed
        .iter()
        .find(|(_, name)| name.len() > deck_path::MAX_LEN)
    {
        return Err(StoreError::Invalid(format!(
            "that would make the name of \"{}\" longer than {} bytes",
            deck_path::leaf(long),
            deck_path::MAX_LEN
        )));
    }
    let names: Vec<&str> = renamed.iter().map(|(_, name)| name.as_str()).collect();
    let taken: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM decks WHERE user_id = ?1
           AND name IN (SELECT value FROM json_each(?2))
           AND id NOT IN (SELECT value FROM json_each(?3)))",
        params![
            user.raw(),
            serde_json::json!(names).to_string(),
            id_list(&decks)
        ],
        |r| r.get(0),
    )?;
    if taken {
        return Err(StoreError::Invalid(
            "a deck with that name already exists".into(),
        ));
    }
    // The deck the new path sits in: made when there is none (which holds
    // it to the cap and keeps it out of an archived deck), and when there
    // is one, it may be archived only if what moves into it is too.
    if let Some(parent) = deck_path::parent(&new) {
        let archived = |name: &str| -> Result<Option<bool>> {
            Ok(conn
                .query_row(
                    "SELECT archived_at IS NOT NULL FROM decks WHERE user_id = ?1 AND name = ?2",
                    params![user.raw(), name],
                    |r| r.get(0),
                )
                .optional()?)
        };
        match archived(parent)? {
            None => {
                insert_deck_path_tx(conn, user, parent, "", now_ms)?;
            }
            Some(true) if archived(old)? == Some(false) => {
                return Err(StoreError::Invalid(format!(
                    "Your deck \"{parent}\" is archived. Unarchive it, or pick another name."
                )));
            }
            Some(_) => {}
        }
    }
    // Two passes, because UNIQUE is checked row by row: moving `A::A` to
    // `A` renames `A::A::A` to `A::A` while the deck of that name may not
    // have moved yet. No stored name starts with U+0001.
    conn.execute(
        "UPDATE decks SET name = char(1) || id
         WHERE user_id = ?1 AND id IN (SELECT value FROM json_each(?2))",
        params![user.raw(), id_list(&decks)],
    )?;
    for (id, name) in &renamed {
        conn.execute(
            "UPDATE decks SET name = ?1 WHERE id = ?2 AND user_id = ?3",
            params![name, id.0, user.raw()],
        )?;
    }
    Ok(())
}

/// The deck must exist and belong to `user`; anything else is
/// `NotFound("deck")`: a stranger's deck is indistinguishable from none.
/// Every multi-statement mutation on a deck opens with this.
pub fn require_deck(conn: &Connection, user: UserId, deck: DeckId) -> Result<()> {
    let owned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM decks WHERE id = ?1 AND user_id = ?2)",
        params![deck.0, user.raw()],
        |r| r.get(0),
    )?;
    if !owned {
        return Err(StoreError::NotFound("deck"));
    }
    Ok(())
}

/// Tags `card` with `tag` (trimmed, lowercased; blank is a no-op),
/// creating the user's tag row on first use.
/// Longest tag the store accepts, in bytes. The store is the last wall:
/// every surface bounds its input first, and this refuses whatever a
/// surface forgot.
/// Most decks one account may hold; `create_deck` refuses the next.
pub const MAX_DECKS_PER_USER: u32 = 500;
pub const MAX_TAG_LEN: usize = 64;
/// Most tags one card may carry.
pub const MAX_TAGS_PER_CARD: usize = 50;

/// Refuses a tag list the store will not write: too many, or one too
/// long. Called by every path that attaches tags to a card.
pub fn check_tags<S: AsRef<str>>(tags: &[S]) -> Result<()> {
    if tags.len() > MAX_TAGS_PER_CARD {
        return Err(StoreError::Invalid(format!(
            "a card may carry at most {MAX_TAGS_PER_CARD} tags"
        )));
    }
    if tags.iter().any(|t| t.as_ref().len() > MAX_TAG_LEN) {
        return Err(StoreError::Invalid(format!(
            "a tag may be at most {MAX_TAG_LEN} bytes"
        )));
    }
    Ok(())
}

/// Drops every media link of one of the user's cards. The media rows
/// stay; the daily sweep removes the ones nothing links any more.
pub(crate) fn unlink_card_media(conn: &Connection, user: UserId, card: CardId) -> Result<()> {
    conn.execute(
        "DELETE FROM card_media WHERE card_id = ?1
           AND EXISTS (SELECT 1 FROM cards WHERE id = ?1 AND user_id = ?2)",
        params![card.0, user.raw()],
    )?;
    Ok(())
}

pub fn attach_tag(conn: &Connection, user: UserId, card: CardId, tag: &str) -> Result<()> {
    let tag = tag.trim().to_lowercase();
    if tag.is_empty() {
        return Ok(());
    }
    if tag.len() > MAX_TAG_LEN {
        return Err(StoreError::Invalid(format!(
            "a tag may be at most {MAX_TAG_LEN} bytes"
        )));
    }
    conn.execute(
        "INSERT OR IGNORE INTO tags (user_id, name) VALUES (?1, ?2)",
        params![user.raw(), tag],
    )?;
    let tag_id: i64 = conn.query_row(
        "SELECT id FROM tags WHERE user_id = ?1 AND name = ?2",
        params![user.raw(), tag],
        |r| r.get(0),
    )?;
    // The link lands only if the card is the user's: a tag row is theirs
    // by construction, the card id came from the caller.
    conn.execute(
        "INSERT OR IGNORE INTO card_tags (card_id, tag_id)
         SELECT ?1, ?2 FROM cards WHERE id = ?1 AND user_id = ?3",
        params![card.0, tag_id, user.raw()],
    )?;
    Ok(())
}

/// A card's tags, sorted; a card that is not the user's has none.
pub fn card_tags(conn: &Connection, user: UserId, card: CardId) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t JOIN card_tags ct ON ct.tag_id = t.id
         WHERE ct.card_id = ?1 AND t.user_id = ?2 ORDER BY t.name",
    )?;
    let rows = stmt.query_map(params![card.0, user.raw()], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}
