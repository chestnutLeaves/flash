use flash_core::{CardId, DeckId, UserId};

use crate::service::{active_boost, now_ms, CardFront, Result, ServiceError, Services};

impl Services {
    pub fn set_daily_limits(
        &self,
        user: UserId,
        new_per_day: u32,
        reviews_per_day: u32,
    ) -> Result<()> {
        if new_per_day > flash_core::settings::MAX_NEW_PER_DAY
            || reviews_per_day > flash_core::settings::MAX_REVIEWS_PER_DAY
        {
            return Err(ServiceError::Malformed("limit out of range".into()));
        }
        Ok(self
            .store
            .set_daily_limits(user, new_per_day, reviews_per_day)?)
    }

    pub fn deck_limits(&self, user: UserId, deck: DeckId) -> Result<flash_store::DeckLimits> {
        Ok(self.store.get_deck_limits(user, deck)?)
    }

    /// Per-deck overrides; None = inherit the account default.
    pub fn set_deck_limits(
        &self,
        user: UserId,
        deck: DeckId,
        new_per_day: Option<u32>,
        reviews_per_day: Option<u32>,
    ) -> Result<()> {
        if new_per_day.is_some_and(|n| n > flash_core::settings::MAX_NEW_PER_DAY)
            || reviews_per_day.is_some_and(|n| n > flash_core::settings::MAX_REVIEWS_PER_DAY)
        {
            return Err(ServiceError::Malformed("limit out of range".into()));
        }
        Ok(self
            .store
            .set_deck_limits(user, deck, new_per_day, reviews_per_day)?)
    }

    /// `name` is the deck's whole path, so this renames it, moves it, or
    /// both; the decks under it follow.
    pub fn rename_deck(&self, user: UserId, deck: DeckId, name: &str) -> Result<()> {
        Ok(self.store.rename_deck(user, deck, name, now_ms())?)
    }

    /// Gives the deck a new name where it is: the last segment of its path
    /// changes and the decks it sits in do not. A `::` in the name would be
    /// a move in disguise, and the page this serves has "Parent deck" for
    /// that.
    pub fn rename_deck_leaf(&self, user: UserId, deck: DeckId, leaf: &str) -> Result<()> {
        if leaf.contains(flash_core::deck_path::SEP) {
            return Err(ServiceError::refused(
                "nest",
                "deck-error-nest-sentence",
                &[],
            ));
        }
        let current = self.deck_by_id(user, deck, now_ms())?;
        let parent = flash_core::deck_path::parent(&current.name).unwrap_or_default();
        let path = format!("{parent}{}{leaf}", flash_core::deck_path::SEP);
        self.rename_deck(user, deck, &path)
    }

    /// Moves the deck, with the decks under it, into `parent`, or to the
    /// top level.
    pub fn move_deck(&self, user: UserId, deck: DeckId, parent: Option<DeckId>) -> Result<()> {
        Ok(self.store.move_deck(user, deck, parent, now_ms())?)
    }

    /// A new deck inside `parent`.
    pub fn create_subdeck(
        &self,
        user: UserId,
        parent: DeckId,
        name: &str,
        description: &str,
        now_ms: i64,
    ) -> Result<DeckId> {
        if name.trim().is_empty() {
            return Err(ServiceError::refused("empty", "deck-error-name-new", &[]));
        }
        let parent = self.deck_by_id(user, parent, now_ms)?;
        let path = format!("{}{}{name}", parent.name, flash_core::deck_path::SEP);
        self.create_deck(user, &path, description, now_ms)
    }

    /// Archives a deck: every card, its media and its review history stay,
    /// but the deck leaves the study queue and the due counts until it is
    /// unarchived. Nothing on the cards is written, so unarchiving just
    /// recomputes the queue from the review data that was there all along.
    pub fn archive_deck(&self, user: UserId, deck: DeckId, now_ms: i64) -> Result<()> {
        Ok(self.store.set_deck_archived(user, deck, Some(now_ms))?)
    }

    pub fn unarchive_deck(&self, user: UserId, deck: DeckId) -> Result<()> {
        Ok(self.store.set_deck_archived(user, deck, None)?)
    }

    /// "More new cards today": adds `extra` to today's new-card allowance
    /// for one deck, or for the account default when `deck` is None.
    /// Returns the total boost now active for today.
    pub fn boost_new_today(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        extra: u32,
        now_ms: i64,
    ) -> Result<u32> {
        if extra == 0 || extra > flash_core::settings::MAX_BOOST {
            return Err(ServiceError::Malformed("boost out of range".into()));
        }
        let settings = self.store.get_settings(user)?;
        let day = Self::study_day_start_ms(&settings, now_ms);
        let current = match deck {
            Some(d) => {
                let limits = self.store.get_deck_limits(user, d)?;
                active_boost(limits.boost_new, limits.boost_day, day)
            }
            None => active_boost(settings.boost_new, settings.boost_day, day),
        };
        let total = (current + extra).min(flash_core::settings::MAX_BOOST);
        match deck {
            Some(d) => self.store.set_deck_boost(user, d, total, day)?,
            None => self.store.set_account_boost(user, total, day)?,
        }
        Ok(total)
    }

    /// Today's active boost for a deck (or the account when None).
    pub fn boost_today(&self, user: UserId, deck: Option<DeckId>, now_ms: i64) -> Result<u32> {
        let settings = self.store.get_settings(user)?;
        let day = Self::study_day_start_ms(&settings, now_ms);
        Ok(match deck {
            Some(d) => {
                let l = self.store.get_deck_limits(user, d)?;
                active_boost(l.boost_new, l.boost_day, day)
            }
            None => active_boost(settings.boost_new, settings.boost_day, day),
        })
    }

    pub(super) fn card_front(&self, user: UserId, card: CardId) -> Result<Option<CardFront>> {
        Ok(self.store.get_card(user, card)?.map(|c| CardFront {
            card_id: c.id,
            front: c.front,
        }))
    }
}
