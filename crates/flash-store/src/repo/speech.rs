//! A deck's declared speech languages (its hints).

use flash_core::{CardId, DeckId, UserId};
use rusqlite::{params, OptionalExtension};

use crate::{exec_expect_row, Result, Store, StoreError};

impl Store {
    /// The languages a deck is spoken in, as stored (already checked by
    /// the server); empty when none were named.
    pub fn deck_speech_hints(&self, user: UserId, deck: DeckId) -> Result<Vec<String>> {
        let raw: String = self
            .conn()
            .query_row(
                "SELECT speech_language_hints FROM decks WHERE id = ?1 AND user_id = ?2",
                params![deck.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound("deck"))?;
        Ok(parse_speech_hints(&raw))
    }

    /// The hints of the deck a card belongs to: the card's own deck,
    /// never a parent's.
    pub fn card_speech_hints(&self, user: UserId, card: CardId) -> Result<Vec<String>> {
        let raw: String = self
            .conn()
            .query_row(
                "SELECT d.speech_language_hints FROM cards c JOIN decks d ON d.id = c.deck_id
                 WHERE c.id = ?1 AND c.user_id = ?2",
                params![card.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound("card"))?;
        Ok(parse_speech_hints(&raw))
    }

    /// Replaces a deck's hints. The caller has checked the codes.
    pub fn set_deck_speech_hints(
        &self,
        user: UserId,
        deck: DeckId,
        hints: &[String],
    ) -> Result<()> {
        // A list of strings always serializes.
        let json = serde_json::to_string(hints)
            .map_err(|_| StoreError::Invalid("language hints could not be stored".into()))?;
        exec_expect_row(
            &self.conn(),
            "deck",
            "UPDATE decks SET speech_language_hints = ?1 WHERE id = ?2 AND user_id = ?3",
            params![json, deck.0, user.raw()],
        )
    }
}

/// A deck's stored hints (`decks.speech_language_hints`); anything
/// unreadable is no hints at all.
pub fn parse_speech_hints(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}
