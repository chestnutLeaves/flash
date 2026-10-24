//! A deck page's cards as the service serves them: the filtered list,
//! the counts its subdeck rows and filter show, the actions on many
//! cards at once with what an Undo needs back, and a deck's own export.
//! The rules are the store's (`flash_store::repo::card_bulk`); this
//! checks the deck is the caller's and bounds what a request may name.

use std::collections::HashMap;

use flash_core::{CardId, DeckId, UserId};
use flash_store::{CardFilter, CardStatus, DeckFacet};

use crate::service::{CardsPage, Result, ServiceError, Services};

/// What the deck page's search and filter ask for, owned so a request
/// can carry it to the service and back into the page's links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckFilter {
    pub decks: Vec<DeckFacet>,
    pub search: Option<String>,
    pub status: CardStatus,
    pub tags: Vec<String>,
    pub match_all: bool,
}

impl DeckFilter {
    fn on(&self, deck: DeckId) -> CardFilter<'_> {
        CardFilter {
            root: deck,
            decks: &self.decks,
            search: self.search.as_deref(),
            status: self.status,
            tags: &self.tags,
            match_all: self.match_all,
        }
    }

    /// A request's search term as the filter holds it: trimmed, absent
    /// when blank.
    pub fn search_from(q: Option<&str>) -> Option<String> {
        q.map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string)
    }

    /// Whether the filter narrows anything beyond the deck's active
    /// cards.
    pub fn is_narrowed(&self) -> bool {
        *self != DeckFilter::default()
    }
}

impl Services {
    /// One page of `deck`'s cards as `filter` narrows them, with the
    /// deck's archived count (whatever the filter) for the filter's
    /// Archived choice.
    pub fn deck_cards_filtered(
        &self,
        user: UserId,
        deck: DeckId,
        filter: &DeckFilter,
        page: u32,
        per_page: u32,
    ) -> Result<CardsPage> {
        self.own_deck(user, deck)?;
        bounded(filter)?;
        let per_page = per_page.clamp(1, 500);
        let total = self.store().count_filtered_cards(user, &filter.on(deck))?;
        let pages = total.div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let rows = self.store().list_filtered_cards(
            user,
            &filter.on(deck),
            per_page,
            (page - 1) * per_page,
            None,
        )?;
        let archived = self.store().count_filtered_cards(
            user,
            &CardFilter {
                status: CardStatus::Archived,
                ..CardFilter::deck(deck)
            },
        )?;
        Ok(CardsPage {
            rows,
            page,
            pages,
            total,
            archived,
        })
    }

    /// Every card of `deck` that `filter` matches: what an action on "all
    /// of them" takes. Past `MAX_BULK_CARDS` it is refused.
    pub fn deck_card_ids(
        &self,
        user: UserId,
        deck: DeckId,
        filter: &DeckFilter,
    ) -> Result<Vec<CardId>> {
        self.own_deck(user, deck)?;
        bounded(filter)?;
        Ok(self.store().filtered_card_ids(user, &filter.on(deck))?)
    }

    /// Each deck's own active cards across `deck`'s tree.
    pub fn deck_card_counts(&self, user: UserId, deck: DeckId) -> Result<HashMap<DeckId, u32>> {
        self.own_deck(user, deck)?;
        Ok(self.store().card_counts_by_deck(user, deck)?)
    }

    /// Archives (or unarchives) cards of `deck`. Returns the ones that
    /// changed.
    pub fn archive_cards(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[CardId],
        archived: bool,
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        self.own_deck(user, deck)?;
        Ok(self
            .store()
            .set_cards_suspended(user, Some(deck), cards, archived, now_ms)?)
    }

    /// Flips back exactly the cards an archive or unarchive changed.
    pub fn restore_archived(
        &self,
        user: UserId,
        cards: &[CardId],
        archived: bool,
        now_ms: i64,
    ) -> Result<()> {
        self.store()
            .set_cards_suspended(user, None, cards, archived, now_ms)?;
        Ok(())
    }

    pub fn delete_cards(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[CardId],
        now_ms: i64,
    ) -> Result<u32> {
        self.own_deck(user, deck)?;
        Ok(self.store().delete_cards(user, Some(deck), cards, now_ms)?)
    }

    /// Moves cards of `deck` (with their notes' other cards) to `to`.
    /// Returns each card moved with the deck it left.
    pub fn move_cards(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[CardId],
        to: DeckId,
        now_ms: i64,
    ) -> Result<Vec<(CardId, DeckId)>> {
        self.own_deck(user, deck)?;
        Ok(self
            .store()
            .move_cards(user, Some(deck), cards, to, now_ms)?)
    }

    /// Puts moved cards back where they were.
    pub fn return_cards(
        &self,
        user: UserId,
        moves: &[(CardId, DeckId)],
        now_ms: i64,
    ) -> Result<()> {
        Ok(self.store().return_cards(user, moves, now_ms)?)
    }

    /// Adds (or removes) tags on cards of `deck` and their notes' other
    /// cards. Returns each link that changed.
    pub fn tag_cards(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[CardId],
        tags: &[String],
        add: bool,
        now_ms: i64,
    ) -> Result<Vec<(CardId, String)>> {
        self.own_deck(user, deck)?;
        if add {
            Ok(self
                .store()
                .add_card_tags(user, Some(deck), cards, tags, now_ms)?)
        } else {
            Ok(self
                .store()
                .remove_card_tags(user, Some(deck), cards, tags, now_ms)?)
        }
    }

    /// Links (or unlinks) exactly these card and tag pairs: an Undo.
    pub fn restore_tag_links(
        &self,
        user: UserId,
        pairs: &[(CardId, String)],
        link: bool,
        now_ms: i64,
    ) -> Result<()> {
        if link {
            self.store().link_card_tags(user, pairs, now_ms)?;
        } else {
            self.store().unlink_card_tags(user, pairs, now_ms)?;
        }
        Ok(())
    }

    /// `deck` and its subdecks' cards, as an export writes them.
    pub fn export_deck_cards(
        &self,
        user: UserId,
        deck: DeckId,
    ) -> Result<Vec<flash_store::export::ExportCard>> {
        self.own_deck(user, deck)?;
        Ok(self.store().export_cards_in(user, Some(deck))?)
    }
}

fn bounded(filter: &DeckFilter) -> Result<()> {
    if filter.tags.len() > crate::bounds::TAGS_PER_CARD {
        return Err(ServiceError::Malformed(
            "too many tags in the filter".into(),
        ));
    }
    if filter.decks.len() > crate::bounds::DECKS_PER_FILTER {
        return Err(ServiceError::Malformed(
            "too many decks in the filter".into(),
        ));
    }
    super::bounded_search(filter.search.as_deref())?;
    Ok(())
}
