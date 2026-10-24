//! The deck page's card list with its filters, and what is done to many
//! of its cards at once: archive, delete, move, tag.
//!
//! One `CardFilter` answers the count, the page and "every card that
//! matches", so a selection of "all 62" is the same 62 the list showed.
//! Each action runs in one transaction, refuses the whole set when any
//! card is not the user's (or not in the deck the page is about), and
//! returns what it changed, which is what an Undo puts back.

use std::collections::{BTreeMap, HashMap, HashSet};

use flash_core::{CardId, DeckId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::repo::cards::{
    card_from_row, check_tags, fill_tags, like_pattern, CardRow, CARD_COLS, MAX_TAGS_PER_CARD,
};
use crate::repo::decks::{id_list, subtree};
use crate::{Result, Store, StoreError};

/// Most cards one action takes: a selection of "every card that
/// matches" past this is refused, never cut short.
pub const MAX_BULK_CARDS: usize = 5_000;

/// One choice in the Decks part of the filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckFacet {
    /// The deck's own cards, none of its subdecks'.
    Own,
    /// One subdeck and the decks under it.
    Sub(DeckId),
}

/// Which cards by whether they are in study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStatus {
    #[default]
    Active,
    Archived,
    All,
}

impl CardStatus {
    fn bind(self) -> i64 {
        match self {
            CardStatus::Active => 0,
            CardStatus::Archived => 1,
            CardStatus::All => 2,
        }
    }
}

/// The cards of a deck's page, as its search and filter narrow them.
/// Choices within one part widen (any of these decks, any of these tags,
/// unless `match_all`); the parts narrow each other.
#[derive(Debug, Clone, Copy)]
pub struct CardFilter<'a> {
    pub root: DeckId,
    pub decks: &'a [DeckFacet],
    pub search: Option<&'a str>,
    pub status: CardStatus,
    pub tags: &'a [String],
    pub match_all: bool,
}

impl<'a> CardFilter<'a> {
    /// Every active card of `root` and its subdecks.
    pub fn deck(root: DeckId) -> Self {
        CardFilter {
            root,
            decks: &[],
            search: None,
            status: CardStatus::Active,
            tags: &[],
            match_all: false,
        }
    }
}

const FILTER_WHERE: &str = "deleted_at IS NULL
   AND deck_id IN (SELECT value FROM json_each(?2))
   AND (?3 IS NULL OR flash_fold(front) LIKE ?3 ESCAPE '\\' OR flash_fold(back) LIKE ?3 ESCAPE '\\')
   AND (?4 = 2 OR suspended = ?4)
   AND (?5 IS NULL OR (SELECT COUNT(*) FROM card_tags ct JOIN tags t ON t.id = ct.tag_id
        WHERE ct.card_id = cards.id AND t.user_id = ?1
          AND t.name IN (SELECT value FROM json_each(?5))) >= ?6)";

/// The decks a filter covers, as a JSON id list. The deck's archived
/// subdecks are left out, as study leaves them out, unless the deck
/// itself is archived (its page then shows what it holds). A facet
/// naming a deck outside the deck's tree is not found.
fn filter_decks(conn: &Connection, user: UserId, f: &CardFilter<'_>) -> Result<String> {
    let tree = subtree(conn, user, f.root)?;
    if tree.is_empty() {
        return Err(StoreError::NotFound("deck"));
    }
    let archived: HashSet<i64> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM decks WHERE user_id = ?1 AND archived_at IS NOT NULL
               AND id IN (SELECT value FROM json_each(?2))",
        )?;
        let ids = stmt.query_map(params![user.raw(), id_list(&tree)], |r| r.get::<_, i64>(0))?;
        ids.collect::<rusqlite::Result<_>>()?
    };
    let root_archived = archived.contains(&f.root.0);
    let live = |id: DeckId| root_archived || !archived.contains(&id.0);
    let mut chosen: Vec<i64> = Vec::new();
    if f.decks.is_empty() {
        chosen.extend(
            tree.iter()
                .map(|d| d.0)
                .filter(|id| live(*id))
                .map(|id| id.0),
        );
    }
    for facet in f.decks {
        match facet {
            DeckFacet::Own => chosen.push(f.root.0),
            DeckFacet::Sub(sub) => {
                let Some((_, name)) = tree.iter().find(|(id, _)| id == sub) else {
                    return Err(StoreError::NotFound("deck"));
                };
                let prefix = format!("{name}::");
                chosen.extend(
                    tree.iter()
                        .filter(|(id, n)| (id == sub || n.starts_with(&prefix)) && live(*id))
                        .map(|(id, _)| id.0),
                );
            }
        }
    }
    chosen.sort_unstable();
    chosen.dedup();
    Ok(ids_json(chosen))
}

fn ids_json(ids: impl IntoIterator<Item = i64>) -> String {
    let ids: Vec<String> = ids.into_iter().map(|id| id.to_string()).collect();
    format!("[{}]", ids.join(","))
}

fn tags_json(tags: &[String]) -> Option<String> {
    (!tags.is_empty()).then(|| serde_json::Value::from(tags.to_vec()).to_string())
}

/// What a filtered statement binds as ?1 to ?6: the user, the decks, the
/// search pattern, the status, the tags and how many must match.
type FilterParams = (i64, String, Option<String>, i64, Option<String>, i64);

fn filter_params(conn: &Connection, user: UserId, f: &CardFilter<'_>) -> Result<FilterParams> {
    let tags: Vec<String> = flash_core::queue::normalize_tags(f.tags);
    let need = if f.match_all { tags.len().max(1) } else { 1 } as i64;
    Ok((
        user.raw(),
        filter_decks(conn, user, f)?,
        like_pattern(f.search),
        f.status.bind(),
        tags_json(&tags),
        need,
    ))
}

impl Store {
    pub fn count_filtered_cards(&self, user: UserId, filter: &CardFilter<'_>) -> Result<u32> {
        let conn = self.conn();
        let p = filter_params(&conn, user, filter)?;
        let n: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM cards WHERE user_id = ?1 AND {FILTER_WHERE}"),
            params![p.0, p.1, p.2, p.3, p.4, p.5],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// One page of what `filter` matches, newest first, tags filled:
    /// `offset` rows in, and only cards older than `before` when given.
    pub fn list_filtered_cards(
        &self,
        user: UserId,
        filter: &CardFilter<'_>,
        limit: u32,
        offset: u32,
        before: Option<CardId>,
    ) -> Result<Vec<CardRow>> {
        let conn = self.conn();
        let p = filter_params(&conn, user, filter)?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {CARD_COLS} FROM cards WHERE user_id = ?1 AND {FILTER_WHERE}
               AND (?9 IS NULL OR id < ?9)
             ORDER BY id DESC LIMIT ?7 OFFSET ?8"
        ))?;
        let rows = stmt.query_map(
            params![
                p.0,
                p.1,
                p.2,
                p.3,
                p.4,
                p.5,
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

    /// Every card `filter` matches, for an action on all of them. More
    /// than `MAX_BULK_CARDS` is refused rather than cut short.
    pub fn filtered_card_ids(&self, user: UserId, filter: &CardFilter<'_>) -> Result<Vec<CardId>> {
        let conn = self.conn();
        let p = filter_params(&conn, user, filter)?;
        let mut stmt = conn.prepare(&format!(
            "SELECT id FROM cards WHERE user_id = ?1 AND {FILTER_WHERE} ORDER BY id DESC LIMIT ?7"
        ))?;
        let rows = stmt.query_map(
            params![p.0, p.1, p.2, p.3, p.4, p.5, MAX_BULK_CARDS as i64 + 1],
            |r| r.get::<_, i64>(0),
        )?;
        let ids: Vec<CardId> = rows
            .map(|r| r.map(CardId))
            .collect::<rusqlite::Result<_>>()?;
        if ids.len() > MAX_BULK_CARDS {
            return Err(too_many());
        }
        Ok(ids)
    }

    /// The active cards of each deck in `root`'s tree (each deck's own,
    /// not its subdecks'): what the subdeck rows and the Decks filter
    /// count.
    pub fn card_counts_by_deck(&self, user: UserId, root: DeckId) -> Result<HashMap<DeckId, u32>> {
        let conn = self.conn();
        let tree = subtree(&conn, user, root)?;
        let mut stmt = conn.prepare(
            "SELECT deck_id, COUNT(*) FROM cards
             WHERE user_id = ?1 AND deleted_at IS NULL AND suspended = 0
               AND deck_id IN (SELECT value FROM json_each(?2))
             GROUP BY deck_id",
        )?;
        let rows = stmt.query_map(params![user.raw(), id_list(&tree)], |r| {
            Ok((DeckId(r.get(0)?), r.get::<_, i64>(1)? as u32))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Takes cards out of study, or puts them back. Returns the cards
    /// whose state changed, which is what an Undo flips back.
    pub fn set_cards_suspended(
        &self,
        user: UserId,
        within: Option<DeckId>,
        cards: &[CardId],
        suspended: bool,
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        self.with_tx(|tx| {
            let ids = owned_cards(tx, user, within, cards)?;
            let mut stmt = tx.prepare(
                "SELECT id FROM cards WHERE user_id = ?1 AND suspended != ?2
                   AND id IN (SELECT value FROM json_each(?3)) ORDER BY id",
            )?;
            let changed: Vec<CardId> = stmt
                .query_map(
                    params![user.raw(), suspended, ids_json(ids.iter().map(|c| c.0))],
                    |r| r.get::<_, i64>(0).map(CardId),
                )?
                .collect::<rusqlite::Result<_>>()?;
            tx.execute(
                "UPDATE cards SET suspended = ?2, updated_at = ?3
                 WHERE user_id = ?1 AND id IN (SELECT value FROM json_each(?4))",
                params![
                    user.raw(),
                    suspended,
                    now_ms,
                    ids_json(changed.iter().map(|c| c.0))
                ],
            )?;
            Ok(changed)
        })
    }

    /// Deletes cards (soft, as one card's delete is). Returns how many.
    pub fn delete_cards(
        &self,
        user: UserId,
        within: Option<DeckId>,
        cards: &[CardId],
        now_ms: i64,
    ) -> Result<u32> {
        self.with_tx(|tx| {
            let ids = owned_cards(tx, user, within, cards)?;
            for card in &ids {
                self.delete_card_tx(tx, user, *card, now_ms)?;
            }
            Ok(ids.len() as u32)
        })
    }

    /// Moves cards to `to`, each with its note and the note's other
    /// cards, so a note and its cards never sit in two decks. The target
    /// is any of the user's decks that is not archived. Returns every
    /// card that moved with the deck it left, which is what an Undo
    /// moves back.
    pub fn move_cards(
        &self,
        user: UserId,
        within: Option<DeckId>,
        cards: &[CardId],
        to: DeckId,
        now_ms: i64,
    ) -> Result<Vec<(CardId, DeckId)>> {
        self.with_tx(|tx| {
            require_open_deck(tx, user, to)?;
            let ids = with_siblings(tx, user, &owned_cards(tx, user, within, cards)?)?;
            let mut stmt = tx.prepare(
                "SELECT id, deck_id FROM cards WHERE user_id = ?1 AND deck_id != ?2
                   AND id IN (SELECT value FROM json_each(?3)) ORDER BY id",
            )?;
            let moved: Vec<(CardId, DeckId)> = stmt
                .query_map(
                    params![user.raw(), to.0, ids_json(ids.iter().map(|c| c.0))],
                    |r| Ok((CardId(r.get(0)?), DeckId(r.get(1)?))),
                )?
                .collect::<rusqlite::Result<_>>()?;
            put_in_deck(tx, user, moved.iter().map(|(c, _)| *c), to, now_ms)?;
            Ok(moved)
        })
    }

    /// Puts cards back in the decks they came from: an Undo of
    /// `move_cards`. Each card must still be the user's; each deck must
    /// be theirs and not archived.
    pub fn return_cards(
        &self,
        user: UserId,
        moves: &[(CardId, DeckId)],
        now_ms: i64,
    ) -> Result<()> {
        self.with_tx(|tx| {
            let all: Vec<CardId> = moves.iter().map(|(c, _)| *c).collect();
            owned_cards(tx, user, None, &all)?;
            let mut by_deck: BTreeMap<i64, Vec<CardId>> = BTreeMap::new();
            for (card, deck) in moves {
                by_deck.entry(deck.0).or_default().push(*card);
            }
            for (deck, cards) in by_deck {
                require_open_deck(tx, user, DeckId(deck))?;
                put_in_deck(tx, user, cards.into_iter(), DeckId(deck), now_ms)?;
            }
            Ok(())
        })
    }

    /// Adds tags to cards and to the other cards of their notes (a note's
    /// cards carry one list, as the editor writes it). Returns each link
    /// that is new, which is what an Undo removes.
    pub fn add_card_tags(
        &self,
        user: UserId,
        within: Option<DeckId>,
        cards: &[CardId],
        tags: &[String],
        now_ms: i64,
    ) -> Result<Vec<(CardId, String)>> {
        let tags = checked_tags(tags)?;
        self.with_tx(|tx| {
            let ids = with_siblings(tx, user, &owned_cards(tx, user, within, cards)?)?;
            let pairs: Vec<(CardId, String)> = ids
                .iter()
                .flat_map(|c| tags.iter().map(move |t| (*c, t.clone())))
                .collect();
            let added = link_tags(tx, user, &pairs, now_ms)?;
            check_tag_counts(tx, user, &ids)?;
            Ok(added)
        })
    }

    /// Removes tags from cards and the other cards of their notes.
    /// Returns each link removed, which is what an Undo puts back.
    pub fn remove_card_tags(
        &self,
        user: UserId,
        within: Option<DeckId>,
        cards: &[CardId],
        tags: &[String],
        now_ms: i64,
    ) -> Result<Vec<(CardId, String)>> {
        let tags = checked_tags(tags)?;
        self.with_tx(|tx| {
            let ids = with_siblings(tx, user, &owned_cards(tx, user, within, cards)?)?;
            let pairs: Vec<(CardId, String)> = ids
                .iter()
                .flat_map(|c| tags.iter().map(move |t| (*c, t.clone())))
                .collect();
            unlink_tags(tx, user, &pairs, now_ms)
        })
    }

    /// Links exactly these (card, tag) pairs: an Undo of a removal. The
    /// pairs come back from a browser, so each tag meets the rules
    /// `add_card_tags` holds it to.
    pub fn link_card_tags(
        &self,
        user: UserId,
        pairs: &[(CardId, String)],
        now_ms: i64,
    ) -> Result<()> {
        let pairs = pairs
            .iter()
            .map(|(c, t)| Ok((*c, checked_tags(std::slice::from_ref(t))?.remove(0))))
            .collect::<Result<Vec<_>>>()?;
        self.with_tx(|tx| {
            let cards: Vec<CardId> = pairs.iter().map(|(c, _)| *c).collect();
            let ids = owned_cards(tx, user, None, &cards)?;
            link_tags(tx, user, &pairs, now_ms)?;
            check_tag_counts(tx, user, &ids)
        })
    }

    /// Unlinks exactly these (card, tag) pairs: an Undo of an addition.
    pub fn unlink_card_tags(
        &self,
        user: UserId,
        pairs: &[(CardId, String)],
        now_ms: i64,
    ) -> Result<()> {
        self.with_tx(|tx| {
            let cards: Vec<CardId> = pairs.iter().map(|(c, _)| *c).collect();
            owned_cards(tx, user, None, &cards)?;
            unlink_tags(tx, user, pairs, now_ms).map(|_| ())
        })
    }
}

fn too_many() -> StoreError {
    StoreError::Invalid(format!(
        "at most {MAX_BULK_CARDS} cards can be changed at once"
    ))
}

/// `cards` without repeats, each checked to be one of the user's live
/// cards (and, with `within`, in that deck or under it). One that is not
/// fails the whole set, so nothing is written.
fn owned_cards(
    conn: &Connection,
    user: UserId,
    within: Option<DeckId>,
    cards: &[CardId],
) -> Result<Vec<CardId>> {
    let mut ids: Vec<i64> = cards.iter().map(|c| c.0).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() > MAX_BULK_CARDS {
        return Err(too_many());
    }
    let decks = match within {
        Some(deck) => {
            let tree = subtree(conn, user, deck)?;
            if tree.is_empty() {
                return Err(StoreError::NotFound("deck"));
            }
            Some(id_list(&tree))
        }
        None => None,
    };
    let found: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE user_id = ?1 AND deleted_at IS NULL
           AND (?2 IS NULL OR deck_id IN (SELECT value FROM json_each(?2)))
           AND id IN (SELECT value FROM json_each(?3))",
        params![user.raw(), decks, ids_json(ids.iter().copied())],
        |r| r.get(0),
    )?;
    if found as usize != ids.len() {
        return Err(StoreError::NotFound("card"));
    }
    Ok(ids.into_iter().map(CardId).collect())
}

fn with_siblings(conn: &Connection, user: UserId, cards: &[CardId]) -> Result<Vec<CardId>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM cards WHERE user_id = ?1 AND deleted_at IS NULL
           AND (id IN (SELECT value FROM json_each(?2))
                OR note_id IN (SELECT note_id FROM cards WHERE user_id = ?1
                     AND note_id IS NOT NULL AND id IN (SELECT value FROM json_each(?2))))
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map(
            params![user.raw(), ids_json(cards.iter().map(|c| c.0))],
            |r| r.get::<_, i64>(0).map(CardId),
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

fn require_open_deck(conn: &Connection, user: UserId, deck: DeckId) -> Result<()> {
    let archived: Option<Option<i64>> = conn
        .query_row(
            "SELECT archived_at FROM decks WHERE id = ?1 AND user_id = ?2",
            params![deck.0, user.raw()],
            |r| r.get(0),
        )
        .optional()?;
    match archived {
        None => Err(StoreError::NotFound("deck")),
        Some(Some(_)) => Err(StoreError::Invalid(
            "That deck is archived. Unarchive it first.".into(),
        )),
        Some(None) => Ok(()),
    }
}

fn put_in_deck(
    conn: &Connection,
    user: UserId,
    cards: impl Iterator<Item = CardId>,
    to: DeckId,
    now_ms: i64,
) -> Result<()> {
    let ids = ids_json(cards.map(|c| c.0));
    conn.execute(
        "UPDATE notes SET deck_id = ?2, updated_at = ?3
         WHERE user_id = ?1 AND deck_id != ?2 AND id IN (SELECT note_id FROM cards
           WHERE user_id = ?1 AND note_id IS NOT NULL AND id IN (SELECT value FROM json_each(?4)))",
        params![user.raw(), to.0, now_ms, ids],
    )?;
    conn.execute(
        "UPDATE cards SET deck_id = ?2, updated_at = ?3
         WHERE user_id = ?1 AND id IN (SELECT value FROM json_each(?4))",
        params![user.raw(), to.0, now_ms, ids],
    )?;
    Ok(())
}

fn checked_tags(tags: &[String]) -> Result<Vec<String>> {
    let tags = flash_core::queue::normalize_tags(tags);
    if tags.is_empty() {
        return Err(StoreError::Invalid("name at least one tag".into()));
    }
    check_tags(&tags)?;
    Ok(tags)
}

/// Refuses (and so rolls back) a write that left one of `cards` with more
/// than `MAX_TAGS_PER_CARD` tags.
fn check_tag_counts(conn: &Connection, user: UserId, cards: &[CardId]) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM card_tags ct JOIN tags t ON t.id = ct.tag_id
         WHERE t.user_id = ?1 AND ct.card_id = ?2",
    )?;
    for card in cards {
        let n: i64 = stmt.query_row(params![user.raw(), card.0], |r| r.get(0))?;
        if n as usize > MAX_TAGS_PER_CARD {
            return Err(StoreError::Invalid(format!(
                "a card may carry at most {MAX_TAGS_PER_CARD} tags"
            )));
        }
    }
    Ok(())
}

/// Links each pair that is not linked yet; returns the new ones. The
/// cards it touched get a fresh `updated_at`, as an editor save gives.
fn link_tags(
    conn: &Connection,
    user: UserId,
    pairs: &[(CardId, String)],
    now_ms: i64,
) -> Result<Vec<(CardId, String)>> {
    let mut added = Vec::new();
    let mut touched: HashSet<i64> = HashSet::new();
    for (card, tag) in pairs {
        conn.execute(
            "INSERT OR IGNORE INTO tags (user_id, name) VALUES (?1, ?2)",
            params![user.raw(), tag],
        )?;
        let n = conn.execute(
            "INSERT OR IGNORE INTO card_tags (card_id, tag_id)
             SELECT c.id, t.id FROM cards c JOIN tags t ON t.user_id = c.user_id AND t.name = ?3
             WHERE c.id = ?2 AND c.user_id = ?1",
            params![user.raw(), card.0, tag],
        )?;
        if n > 0 {
            added.push((*card, tag.clone()));
            touched.insert(card.0);
        }
    }
    touch_cards(conn, user, touched, now_ms)?;
    Ok(added)
}

fn unlink_tags(
    conn: &Connection,
    user: UserId,
    pairs: &[(CardId, String)],
    now_ms: i64,
) -> Result<Vec<(CardId, String)>> {
    let mut removed = Vec::new();
    let mut touched: HashSet<i64> = HashSet::new();
    for (card, tag) in pairs {
        let n = conn.execute(
            "DELETE FROM card_tags WHERE card_id = ?2
               AND card_id IN (SELECT id FROM cards WHERE user_id = ?1)
               AND tag_id IN (SELECT id FROM tags WHERE user_id = ?1 AND name = ?3)",
            params![user.raw(), card.0, tag],
        )?;
        if n > 0 {
            removed.push((*card, tag.clone()));
            touched.insert(card.0);
        }
    }
    touch_cards(conn, user, touched, now_ms)?;
    Ok(removed)
}

fn touch_cards(conn: &Connection, user: UserId, cards: HashSet<i64>, now_ms: i64) -> Result<()> {
    if cards.is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE cards SET updated_at = ?2 WHERE user_id = ?1 AND id IN (SELECT value FROM json_each(?3))",
        params![user.raw(), now_ms, ids_json(cards)],
    )?;
    Ok(())
}
