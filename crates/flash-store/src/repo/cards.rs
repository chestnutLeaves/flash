use flash_core::speech::{self, SpeechBlock};
use flash_core::{deck_path, CardId, CardText, DeckId, MediaId, NoteId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::repo::decks::{id_list, subtree, subtree_filter};
use crate::repo::ids::{next_id, IdTable};
use crate::repo::notes::drop_slots;
use crate::richtext::SanitizedHtml;
use crate::{exec_expect_row, Result, Store, StoreError};

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
    /// How each side reads aloud and in which language; None for a
    /// side with no blocks (`flash_core::speech`).
    pub front_speech: Option<Vec<SpeechBlock>>,
    pub back_speech: Option<Vec<SpeechBlock>>,
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
    /// export surface). Never filtered by a card cap.
    pub fn export_cards(&self, user: UserId) -> Result<Vec<crate::export::ExportCard>> {
        self.export_cards_in(user, None)
    }

    /// As `export_cards`, narrowed to `deck` and its subdecks when given.
    /// Their names then start at the deck (`Bio::Cells::Membrane` from
    /// `Cells` exports as `Cells::Membrane`), so the package brings no
    /// empty parent deck along.
    pub fn export_cards_in(
        &self,
        user: UserId,
        deck: Option<DeckId>,
    ) -> Result<Vec<crate::export::ExportCard>> {
        let conn = self.conn();
        let decks = subtree_filter(&conn, user, deck)?;
        let root = match deck {
            Some(deck) => Some(
                conn.query_row(
                    "SELECT name FROM decks WHERE id = ?1 AND user_id = ?2",
                    params![deck.0, user.raw()],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .ok_or(StoreError::NotFound("deck"))?,
            ),
            None => None,
        };
        let mut stmt = conn.prepare(
            "SELECT c.id, d.name, c.front, c.back,
                    c.front_html, c.back_html, c.cloze_text, c.cloze_index, c.type_answer
             FROM cards c
             JOIN decks d ON d.id = c.deck_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL
               AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
             ORDER BY c.id",
        )?;
        let rows = stmt.query_map(params![user.raw(), decks], |r| {
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
        if let Some(root) = &root {
            let leaf = root
                .rsplit_once(deck_path::SEP)
                .map_or(root.as_str(), |(_, l)| l);
            for (_, card) in &mut cards {
                card.deck = deck_path::reroot(&card.deck, root, leaf);
            }
        }

        let mut tag_stmt = conn.prepare(
            "SELECT ct.card_id, t.name FROM card_tags ct
             JOIN tags t ON t.id = ct.tag_id
             JOIN cards c ON c.id = ct.card_id
             WHERE c.user_id = ?1 AND c.deleted_at IS NULL
               AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
             ORDER BY t.name",
        )?;
        let tag_rows = tag_stmt.query_map(params![user.raw(), decks], |r| {
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
        self.with_tx(|tx| {
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
            cards
                .iter()
                .map(|(text, tags)| insert_card(tx, user, deck, text, tags, now_ms))
                .collect()
        })
    }

    /// Fills `deck` with imported cards and everything the file said
    /// about them (rich content, suspension, media links) in one
    /// transaction, so a deck is never left with cards missing what makes
    /// them whole. Returns the new ids in `cards` order.
    pub fn fill_imported_deck(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[ImportedCard],
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        self.with_tx(|tx| {
            let mut ids = Vec::with_capacity(cards.len());
            for card in cards {
                let id = insert_card(tx, user, deck, &card.text, &card.tags, now_ms)?;
                if !card.extras.is_empty() {
                    write_card_extras(tx, user, id, &card.extras)?;
                }
                if card.suspended {
                    suspend_card(tx, user, id, now_ms)?;
                }
                for media in &card.media {
                    link_card_media_tx(tx, user, id, *media)?;
                }
                ids.push(id);
            }
            Ok(ids)
        })
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
        self.detach_card_tx(&tx, user, card, now_ms)?;
        set_card_words(&tx, user, card, text, now_ms)?;
        tx.execute(
            "UPDATE cards SET front_html = NULL, back_html = NULL,
               cloze_text = NULL, cloze_index = NULL, type_answer = NULL
             WHERE id = ?1 AND user_id = ?2",
            params![card.0, user.raw()],
        )?;
        // A plain card references no media; the links go with the HTML.
        unlink_card_media(&tx, user, card)?;
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
        self.with_tx(|tx| {
            for card in cards {
                suspend_card(tx, user, *card, now_ms)?;
            }
            Ok(())
        })
    }

    /// Takes one live card out of study, or puts it back. Its schedule and
    /// history stay as they were; a card that is not the user's, or is
    /// gone, is not found.
    pub fn set_card_suspended(
        &self,
        user: UserId,
        card: CardId,
        suspended: bool,
        now_ms: i64,
    ) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "card",
            "UPDATE cards SET suspended = ?1, updated_at = ?2
             WHERE id = ?3 AND user_id = ?4 AND deleted_at IS NULL",
            params![i64::from(suspended), now_ms, card.0, user.raw()],
        )
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

/// One card as an import brings it: its text and tags, and what else the
/// file said about it.
#[derive(Debug, Clone)]
pub struct ImportedCard {
    pub text: CardText,
    pub tags: Vec<String>,
    pub extras: CardExtras,
    pub suspended: bool,
    /// Media rows of the importer's the card's HTML references.
    pub media: Vec<MediaId>,
}

/// A new card with its state row and tags.
pub fn insert_card(
    conn: &Connection,
    user: UserId,
    deck: DeckId,
    text: &CardText,
    tags: &[String],
    now_ms: i64,
) -> Result<CardId> {
    check_tags(tags)?;
    let card_id = next_id(conn, IdTable::Cards)?;
    conn.execute(
        "INSERT INTO cards (id, user_id, deck_id, front, back, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![card_id, user.raw(), deck.0, text.front, text.back, now_ms],
    )?;
    conn.execute(
        "INSERT INTO card_state (card_id, due)
         SELECT ?1, ?2 FROM cards WHERE id = ?1 AND user_id = ?3",
        params![card_id, now_ms, user.raw()],
    )?;
    for tag in tags {
        attach_tag(conn, user, CardId(card_id), tag)?;
    }
    Ok(CardId(card_id))
}

pub fn suspend_card(conn: &Connection, user: UserId, card: CardId, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE cards SET suspended = 1, updated_at = ?1
         WHERE id = ?2 AND user_id = ?3 AND deleted_at IS NULL",
        params![now_ms, card.0, user.raw()],
    )?;
    Ok(())
}

/// Links a card to a media row, both the user's (a pair that isn't
/// links nothing).
pub fn link_card_media_tx(
    conn: &Connection,
    user: UserId,
    card: CardId,
    media: MediaId,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO card_media (card_id, media_id)
         SELECT c.id, m.id FROM cards c, media m
         WHERE c.id = ?1 AND c.user_id = ?3 AND m.id = ?2 AND m.user_id = ?3",
        params![card.0, media.0, user.raw()],
    )?;
    Ok(())
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

    /// Takes a live card out of its note, which then leaves its slot out
    /// (`leave_note`); a note left with no live card goes.
    pub fn detach_card_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        card: CardId,
        now_ms: i64,
    ) -> Result<()> {
        let note = leave_note(tx, user, card)?;
        tx.execute(
            "UPDATE cards SET note_id = NULL, ord = 0 WHERE id = ?1 AND user_id = ?2",
            params![card.0, user.raw()],
        )?;
        reap_empty_note(tx, user, note, now_ms)
    }

    pub fn delete_card_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        card: CardId,
        now_ms: i64,
    ) -> Result<()> {
        let note = leave_note(tx, user, card)?;
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
               AND (?3 IS NULL OR flash_fold(front) LIKE ?3 ESCAPE '\\'
                    OR flash_fold(back) LIKE ?3 ESCAPE '\\')",
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
        self.list_cards_cursor(user, deck, search, limit, 0, None)
    }

    /// Newest first, `offset` rows in. A deck holds its subdecks' cards
    /// too; `CardRow::deck_id` says whose.
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
               AND (?3 IS NULL OR flash_fold(front) LIKE ?3 ESCAPE '\\'
                    OR flash_fold(back) LIKE ?3 ESCAPE '\\')
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
        fill_tags(&conn, user, &mut cards)?;
        Ok(cards)
    }

    /// The cards a preview of `deck` goes through: what a study session
    /// there could ask (not deleted, not archived, in `deck` or a subdeck
    /// that is not archived), oldest first, the order they were made in.
    /// Tags are left empty.
    pub fn preview_cards(&self, user: UserId, deck: DeckId) -> Result<Vec<CardRow>> {
        let conn = self.conn();
        let decks = id_list(&subtree(&conn, user, deck)?);
        let mut stmt = conn.prepare(&format!(
            "SELECT {CARD_COLS} FROM cards
             WHERE user_id = ?1 AND deleted_at IS NULL AND suspended = 0
               AND deck_id IN (SELECT value FROM json_each(?2))
               AND deck_id IN (SELECT id FROM decks WHERE user_id = ?1 AND archived_at IS NULL)
             ORDER BY id"
        ))?;
        let rows = stmt.query_map(params![user.raw(), decks], card_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

pub(crate) fn fill_tags(conn: &Connection, user: UserId, cards: &mut [CardRow]) -> Result<()> {
    if cards.is_empty() {
        return Ok(());
    }
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
    let mut by_card: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for row in tag_rows {
        let (card_id, tag) = row?;
        by_card.entry(card_id).or_default().push(tag);
    }
    for card in cards {
        if let Some(tags) = by_card.remove(&card.id.0) {
            card.tags = tags;
        }
    }
    Ok(())
}

pub(crate) const CARD_COLS: &str =
    "id, deck_id, front, back, suspended, front_html, back_html, type_answer,
    note_id, ord, cloze_index, front_speech, back_speech";

pub(crate) fn card_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CardRow> {
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
        front_speech: speech::from_json(r.get::<_, Option<String>>(11)?.as_deref()),
        back_speech: speech::from_json(r.get::<_, Option<String>>(12)?.as_deref()),
    })
}

/// A search term as a `LIKE` pattern against folded text (`fold`): the
/// term folded the same way, its `\`, `%` and `_` taken literally.
pub(crate) fn like_pattern(search: Option<&str>) -> Option<String> {
    search.map(|s| {
        let folded = crate::fold::fold(s);
        format!(
            "%{}%",
            folded
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    })
}

/// The note a live card is leaving (deleted, or edited apart from it),
/// whose later saves then leave its slot out rather than make it again.
fn leave_note(conn: &Connection, user: UserId, card: CardId) -> Result<Option<i64>> {
    let row: Option<(Option<i64>, u32)> = conn
        .query_row(
            "SELECT note_id, ord FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
            params![card.0, user.raw()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((Some(note), ord)) = row else {
        return Ok(None);
    };
    drop_slots(conn, user, NoteId(note), &[ord])?;
    Ok(Some(note))
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

/// Longest tag the store accepts, in bytes. The store is the last wall:
/// every surface bounds its input first, and this refuses whatever a
/// surface forgot.
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
pub fn unlink_card_media(conn: &Connection, user: UserId, card: CardId) -> Result<()> {
    conn.execute(
        "DELETE FROM card_media WHERE card_id = ?1
           AND EXISTS (SELECT 1 FROM cards WHERE id = ?1 AND user_id = ?2)",
        params![card.0, user.raw()],
    )?;
    Ok(())
}

/// Writes a live card's words. A side whose words changed loses its
/// speech blocks; one that kept its words keeps them. Every expression
/// in a SET reads the row as it was.
pub fn set_card_words(
    conn: &Connection,
    user: UserId,
    card: CardId,
    text: &CardText,
    now_ms: i64,
) -> Result<()> {
    exec_expect_row(
        conn,
        "card",
        "UPDATE cards SET front = ?1, back = ?2, updated_at = ?3,
           front_speech = CASE WHEN front = ?1 THEN front_speech END,
           back_speech = CASE WHEN back = ?2 THEN back_speech END,
           speech_by = CASE WHEN (front = ?1 AND front_speech IS NOT NULL)
                              OR (back = ?2 AND back_speech IS NOT NULL)
                            THEN speech_by END
         WHERE id = ?4 AND user_id = ?5 AND deleted_at IS NULL",
        params![text.front, text.back, now_ms, card.0, user.raw()],
    )
}

/// Tags `card` with `tag` (`normalize_tag`; blank is a no-op),
/// creating the user's tag row on first use.
pub fn attach_tag(conn: &Connection, user: UserId, card: CardId, tag: &str) -> Result<()> {
    let tag = flash_core::queue::normalize_tag(tag);
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
