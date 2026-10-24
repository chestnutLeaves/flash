use flash_core::queue::Due;
use flash_core::{deck_path, DeckId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::repo::ids::{next_id, IdTable};
use crate::{exec_expect_row, Result, Store, StoreError};

/// Outcome of a deck deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedDeck {
    /// Decks removed: the deck and the decks under it.
    pub decks: u32,
    pub cards: u32,
    /// Media hashes no user references any more. The deletion already
    /// owes their removal from storage (`JOB_DELETE_BLOBS`).
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
    /// Its own cards a study session could ask (not deleted, not
    /// archived), whatever their schedule; zero for an archived deck.
    pub card_count: u32,
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

    pub fn delete_deck_tx(
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
        let orphan_blobs = self.collect_unlinked_media_tx(tx, user)?;
        crate::repo::jobs::owe_blob_deletes(tx, &orphan_blobs)?;
        Ok(DeletedDeck {
            decks: decks.len() as u32,
            cards,
            orphan_blobs,
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
        // A downstream extension's rows that name the deck or its notes
        // lose the reference and outlive the deck.
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

    /// Every deck's id and whole path, archived ones included.
    pub fn deck_names(&self, user: UserId) -> Result<Vec<(DeckId, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, name FROM decks WHERE user_id = ?1")?;
        let rows = stmt.query_map([user.raw()], |r| Ok((DeckId(r.get(0)?), r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
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
    /// Due as `due` says (flash_core::queue::Due), so a deck's count agrees
    /// with the session it would start.
    pub fn list_decks(&self, user: UserId, due: Due) -> Result<Vec<DeckSummary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(LIST_DECKS)?;
        let rows = stmt.query_map(
            params![user.raw(), due.now_ms, due.reviews_before_ms],
            |r| {
                Ok(DeckSummary {
                    id: DeckId(r.get(0)?),
                    name: r.get(1)?,
                    description: r.get(2)?,
                    due_count: r.get::<_, i64>(3)? as u32,
                    new_count: r.get::<_, i64>(4)? as u32,
                    archived_at: r.get(5)?,
                    card_count: r.get::<_, i64>(6)? as u32,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// `list_decks` for the user ?1, due at ?2 (steps) and before ?3
/// (reviews). Each count seeks its deck in idx_cards_study, which holds
/// all it asks of a card.
pub(crate) const LIST_DECKS: &str = "SELECT d.id, d.name, d.description,
       CASE WHEN d.archived_at IS NULL THEN
         (SELECT COUNT(*) FROM cards c JOIN card_state s ON s.card_id = c.id
           WHERE c.user_id = ?1 AND c.deck_id = d.id AND c.deleted_at IS NULL
             AND c.suspended = 0
             AND ((s.phase = 2 AND s.due < ?3)
                  OR (s.phase IN (1, 3) AND s.due <= ?2)))
       ELSE 0 END AS due_count,
       CASE WHEN d.archived_at IS NULL THEN
         (SELECT COUNT(*) FROM cards c JOIN card_state s ON s.card_id = c.id
           WHERE c.user_id = ?1 AND c.deck_id = d.id AND c.deleted_at IS NULL
             AND c.suspended = 0 AND s.phase = 0)
       ELSE 0 END AS new_count,
       d.archived_at,
       CASE WHEN d.archived_at IS NULL THEN
         (SELECT COUNT(*) FROM cards c
           WHERE c.user_id = ?1 AND c.deck_id = d.id AND c.deleted_at IS NULL
             AND c.suspended = 0)
       ELSE 0 END AS card_count
     FROM decks d WHERE d.user_id = ?1
     ORDER BY d.name";

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

/// The first name the user has no deck by, compared as stored: `base`,
/// or `base (2)`, `base (3)`, up to `base (limit - 1)`; past that the
/// name is refused.
pub fn free_deck_path(conn: &Connection, user: UserId, base: &str, limit: u32) -> Result<String> {
    let taken = |name: &str| -> Result<bool> {
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM decks WHERE user_id = ?1 AND name = ?2)",
            params![user.raw(), name],
            |r| r.get(0),
        )?)
    };
    if !taken(base)? {
        return Ok(base.to_string());
    }
    for n in 2..limit {
        let candidate = format!("{base} ({n})");
        if !taken(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(StoreError::Invalid("too many decks with that name".into()))
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
        let id = next_id(conn, IdTable::Decks)?;
        conn.execute(
            "INSERT INTO decks (id, user_id, name, description, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, user.raw(), name, description, now_ms],
        )?;
        Ok(DeckId(id))
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
pub(crate) fn id_list(decks: &[(DeckId, String)]) -> String {
    let ids: Vec<String> = decks.iter().map(|(id, _)| id.0.to_string()).collect();
    format!("[{}]", ids.join(","))
}

/// `Store::rename_deck`, inside the caller's transaction.
pub fn rename_deck_tx(
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

/// Most decks one account may hold; `create_deck` refuses the next.
pub const MAX_DECKS_PER_USER: u32 = 500;
