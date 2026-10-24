//! The tags a user's cards carry, for the study scope's picker.

use flash_core::{DeckId, UserId};
use rusqlite::params;

use crate::{Result, Store};

/// A tag and how many of the user's live cards carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub name: String,
    pub cards: u32,
}

impl Store {
    /// Every tag on a live card of the user's, by name, with the number
    /// of cards carrying it. A card is live when it is not deleted and
    /// its deck is not archived; a suspended card still counts, since it
    /// is still in the deck. With `deck`, only that deck's subtree
    /// counts, and a deck that is not the user's is `NotFound`.
    pub fn list_tags(&self, user: UserId, deck: Option<DeckId>) -> Result<Vec<TagCount>> {
        let conn = self.conn();
        if let Some(deck) = deck {
            super::decks::require_deck(&conn, user, deck)?;
        }
        let deck_filter = super::decks::subtree_filter(&conn, user, deck)?;
        let mut stmt = conn.prepare(
            "SELECT t.name, COUNT(*) FROM tags t
             JOIN card_tags ct ON ct.tag_id = t.id
             JOIN cards c ON c.id = ct.card_id AND c.user_id = t.user_id
             JOIN decks d ON d.id = c.deck_id
             WHERE t.user_id = ?1 AND c.deleted_at IS NULL AND d.archived_at IS NULL
               AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
             GROUP BY t.id ORDER BY t.name",
        )?;
        let rows = stmt.query_map(params![user.raw(), deck_filter], |r| {
            Ok(TagCount {
                name: r.get(0)?,
                cards: r.get::<_, i64>(1)? as u32,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
