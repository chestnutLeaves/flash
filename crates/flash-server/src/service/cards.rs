use flash_core::queue::StudyScope;
use flash_core::{validate_card_text, CardId, CardState, DeckId, Scheduler, UserId};
use flash_store::import::{HistoryEntry, ImportRow};
use flash_store::{CardRow, ImportedReview, StoreError};

use crate::policy::{AddSource, Admission};
use crate::service::{
    card_refused, AdmittedCards, DeckDetail, DeckRow, NextDue, Result, ServiceError, Services,
};

impl Services {
    pub fn create_deck(
        &self,
        user: UserId,
        name: &str,
        description: &str,
        now_ms: i64,
    ) -> Result<DeckId> {
        // The per-account deck cap is the store's: it holds for every
        // caller of `create_deck`, imports included.
        Ok(self.store.create_deck(user, name, description, now_ms)?)
    }

    /// The deck a typed name asks for: the one already carrying it, or a
    /// new one. The bool says which, so a caller can report a deck it
    /// just made. An archived deck is refused instead of reused: the
    /// pickers that take a name never list one, so filing into it would
    /// contradict the page the name was typed on.
    pub fn deck_named(&self, user: UserId, name: &str, now_ms: i64) -> Result<(DeckId, bool)> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ServiceError::refused("empty", "deck-error-name-new", &[]));
        }
        match self.store.find_deck_by_name(user, name)? {
            Some(deck) => {
                if self.deck_by_id(user, deck, now_ms)?.archived_at.is_some() {
                    return Err(ServiceError::refused(
                        "archived",
                        "deck-error-archived-sentence",
                        &[("name", name.into())],
                    ));
                }
                Ok((deck, false))
            }
            None => Ok((self.store.create_deck(user, name, "", now_ms)?, true)),
        }
    }

    /// Decks in tree order with today's *budgeted* due/new counts (what
    /// studying that deck would actually serve), not raw totals.
    ///
    /// Each deck's own cards are held to its own budget first; a deck's
    /// row then adds the decks under it and holds the sum to the account's
    /// budget. That is what a session over the deck does: every card
    /// spends the budget of the deck it lives in, and the account's budget
    /// caps the lot.
    pub fn list_decks(&self, user: UserId, now_ms: i64) -> Result<Vec<DeckRow>> {
        let settings = self.store.get_settings(user)?;
        let (budgets, mut goals) = self.budgets_and_goals(user, &settings, now_ms)?;
        let decks = self
            .store
            .list_decks(user, Self::due_at(&settings, now_ms))?;
        let nodes: Vec<flash_core::deck_tree::Node<'_>> =
            decks
                .iter()
                .map(|d| {
                    let own = budgets.per_deck.get(&d.id).copied().unwrap_or(
                        flash_core::queue::DeckBudget {
                            new: u32::MAX,
                            reviews: u32::MAX,
                        },
                    );
                    flash_core::deck_tree::Node {
                        name: &d.name,
                        due: d.due_count.min(own.reviews),
                        new: d.new_count.min(own.new),
                        paced: budgets.paced.contains(&d.id),
                    }
                })
                .collect();
        let rows = flash_core::deck_tree::build(&nodes, budgets.new_total, budgets.review_total);
        let mut cards: Vec<u32> = decks.iter().map(|d| d.card_count).collect();
        for row in rows.iter().rev() {
            if let Some(parent) = row.parent {
                cards[parent] = cards[parent].saturating_add(cards[row.index]);
            }
        }
        Ok(rows
            .into_iter()
            .map(|row| {
                let deck = &decks[row.index];
                DeckRow {
                    id: deck.id,
                    path: flash_core::deck_path::display(&deck.name),
                    name: deck.name.clone(),
                    leaf: row.leaf,
                    depth: row.depth,
                    parent_id: row.parent.map(|p| decks[p].id),
                    description: deck.description.clone(),
                    due_count: row.due,
                    new_count: row.new,
                    card_count: cards[row.index],
                    archived_at: deck.archived_at,
                    goal: goals.views.remove(&deck.id),
                }
            })
            .collect())
    }

    /// One of the user's decks by id, with today's budgeted counts.
    pub fn deck_by_id(&self, user: UserId, deck: DeckId, now_ms: i64) -> Result<DeckRow> {
        self.list_decks(user, now_ms)?
            .into_iter()
            .find(|d| d.id == deck)
            .ok_or(ServiceError::Store(StoreError::NotFound("deck")))
    }

    pub(crate) fn own_deck(&self, user: UserId, deck: DeckId) -> Result<()> {
        self.store.get_deck_limits(user, deck)?;
        Ok(())
    }

    /// The deck settings page in one read.
    pub fn deck_detail(&self, user: UserId, deck: DeckId, now_ms: i64) -> Result<DeckDetail> {
        let decks = self.list_decks(user, now_ms)?;
        let summary = decks
            .iter()
            .find(|d| d.id == deck)
            .cloned()
            .ok_or(ServiceError::Store(StoreError::NotFound("deck")))?;
        let subdeck_count = decks
            .iter()
            .filter(|d| flash_core::deck_path::is_descendant(&d.name, &summary.name))
            .count() as u32;
        let settings = self.store.get_settings(user)?;
        Ok(DeckDetail {
            subdeck_count,
            limits: self.store.get_deck_limits(user, deck)?,
            default_new: settings.new_per_day,
            default_reviews: settings.reviews_per_day,
            boost_today: self.boost_today(user, Some(deck), now_ms)?,
            card_count: self.store.count_cards(user, Some(deck), None)?,
            speech_language_hints: self.store.deck_speech_hints(user, deck)?,
            deck: summary,
        })
    }

    /// Names the languages a deck's cards are in; unknown codes are
    /// refused, duplicates dropped.
    pub fn set_deck_speech_hints(
        &self,
        user: UserId,
        deck: DeckId,
        hints: &[String],
    ) -> Result<Vec<String>> {
        let hints = crate::deck_languages::normalize_hints(hints)
            .map_err(|e| ServiceError::Malformed(e.to_string()))?;
        self.store.set_deck_speech_hints(user, deck, &hints)?;
        Ok(hints)
    }

    /// The languages an assistant says a deck's cards are in, kept as the
    /// deck's declared hints while it has none. Regions fall away (hints
    /// are languages) and an unknown code is skipped, never refused: the
    /// cards were the point of the call.
    pub fn declare_deck_languages(
        &self,
        user: UserId,
        deck: DeckId,
        languages: &[String],
    ) -> Result<()> {
        if !self.store.deck_speech_hints(user, deck)?.is_empty() {
            return Ok(());
        }
        let mut codes: Vec<String> = Vec::new();
        for tag in languages {
            let code = crate::languages::base(tag);
            if crate::deck_languages::LANGUAGE_CODES.contains(&code.as_str())
                && !codes.contains(&code)
            {
                codes.push(code);
            }
        }
        codes.truncate(crate::deck_languages::MAX_LANGUAGE_HINTS);
        if !codes.is_empty() {
            self.store.set_deck_speech_hints(user, deck, &codes)?;
        }
        Ok(())
    }

    /// The admission gate plus text validation shared by every
    /// interactive add: all-or-nothing at the card policy's cap (the store
    /// re-checks inside its insert transaction).
    fn admitted_cards(
        &self,
        user: UserId,
        cards: &[(String, String, Vec<String>)],
        now_ms: i64,
    ) -> Result<AdmittedCards> {
        let cap = self.admit_cards(user, cards.len() as u32, AddSource::Interactive, now_ms)?;
        let validated = cards
            .iter()
            .map(|(front, back, tags)| {
                validate_card_text(front, back)
                    .map(|text| (text, tags.clone()))
                    .map_err(card_refused)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((cap, validated))
    }

    /// Batch create into a deck named `deck_name` (created if missing).
    /// The MCP tools' path, where decks are named, not clicked.
    pub fn create_cards(
        &self,
        user: UserId,
        deck_name: &str,
        cards: &[(String, String, Vec<String>)],
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        let (cap, validated) = self.admitted_cards(user, cards, now_ms)?;
        let deck = match self.store.find_deck_by_name(user, deck_name)? {
            Some(id) => id,
            None => self.store.create_deck(user, deck_name, "", now_ms)?,
        };
        self.store
            .create_cards(user, deck, &validated, cap, now_ms)
            .map_err(|e| self.cap_error(e))
    }

    /// Batch create into an existing deck of the user's. The quick-add
    /// form on the web and in the app.
    pub fn create_cards_in_deck(
        &self,
        user: UserId,
        deck: DeckId,
        cards: &[(String, String, Vec<String>)],
        now_ms: i64,
    ) -> Result<Vec<CardId>> {
        self.own_deck(user, deck)?;
        let (cap, validated) = self.admitted_cards(user, cards, now_ms)?;
        self.store
            .create_cards(user, deck, &validated, cap, now_ms)
            .map_err(|e| self.cap_error(e))
    }

    /// The deck and the cards a preview of it goes through
    /// (`Store::preview_cards`). A preview only reads: nothing it shows is
    /// scheduled or recorded.
    pub fn preview_cards(
        &self,
        user: UserId,
        deck: DeckId,
        now_ms: i64,
    ) -> Result<(DeckRow, Vec<CardRow>)> {
        let row = self.deck_by_id(user, deck, now_ms)?;
        Ok((row, self.store.preview_cards(user, deck)?))
    }

    /// What the end of a preview says is still to study in `deck`: the
    /// cards a session there would ask now, and, when none, when the next
    /// ones come, counted as a finished session's screen counts them.
    pub fn preview_outlook(
        &self,
        user: UserId,
        deck: DeckId,
        now_ms: i64,
    ) -> Result<(u32, Option<NextDue>)> {
        let scope = StudyScope::deck(deck);
        let (queue, held) = self.build_queue(user, &scope, now_ms)?;
        if !queue.is_empty() {
            return Ok((queue.len() as u32, None));
        }
        Ok((0, self.next_due(user, &scope, held, now_ms)?))
    }

    /// One card of the user's.
    pub fn card(&self, user: UserId, card: CardId) -> Result<CardRow> {
        self.store
            .get_card(user, card)?
            .ok_or(ServiceError::Store(StoreError::NotFound("card")))
    }

    /// Admission gate for adding `adding` cards: asks the card policy and
    /// returns the cap the store must re-check inside its own transaction
    /// (None = unlimited). Public for the layers that insert cards by
    /// their own routes.
    pub fn admit_cards(
        &self,
        user: UserId,
        adding: u32,
        source: AddSource,
        now_ms: i64,
    ) -> Result<Option<u32>> {
        match self
            .policy
            .admit(&self.store, user, adding, source, now_ms)?
        {
            Admission::Allowed { recheck_cap } => Ok(recheck_cap),
            Admission::Denied { current, cap } => Err(self.over_cap(current, cap)),
        }
    }

    /// Commits a parsed import in one decision: the whole batch is either
    /// admitted by the card policy or rejected before any insert. Rows
    /// without a deck land in `default_deck`. `history` is the package's
    /// review log when the uploader asked to bring their progress (empty
    /// otherwise): it is replayed through FSRS so every review lands in
    /// the log and each card keeps its real memory state instead of
    /// arriving as new. Returns the number of cards created.
    pub fn import_cards(
        &self,
        user: UserId,
        default_deck: &str,
        rows: Vec<ImportRow>,
        history: Vec<HistoryEntry>,
        media_ids: Option<&std::collections::HashMap<String, flash_core::MediaId>>,
        now_ms: i64,
    ) -> Result<usize> {
        self.admit_cards(user, rows.len() as u32, AddSource::Import, now_ms)?;
        let mut card_of: std::collections::HashMap<i64, CardId> = std::collections::HashMap::new();
        let mut by_deck: std::collections::BTreeMap<String, Vec<ImportRow>> =
            std::collections::BTreeMap::new();
        for row in rows {
            let deck = row
                .deck
                .clone()
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| default_deck.to_string());
            by_deck.entry(deck).or_default().push(row);
        }
        // The decks are filled one after another, each in its own
        // transaction, so a name that cannot be made (too long, inside an
        // archived deck, past the deck cap) has to stop the import here,
        // before the first card, and not after half of them.
        let targets: Vec<String> = by_deck.keys().cloned().collect();
        self.store.check_deck_paths(user, &targets)?;
        let mut created = 0usize;
        for (deck_name, batch) in by_deck {
            // Each card whole: its text, and everything beyond the plain
            // card (rich content, typing answers, cloze sources,
            // suspension, media links).
            let cards = batch
                .iter()
                .map(|row| {
                    let text = validate_card_text(&row.front, &row.back).map_err(card_refused)?;
                    let media = match media_ids {
                        Some(map) => row
                            .media
                            .iter()
                            .filter_map(|m| map.get(&m.filename).copied())
                            .collect(),
                        None => Vec::new(),
                    };
                    Ok(flash_store::ImportedCard {
                        text,
                        tags: row.tags.clone(),
                        extras: flash_store::CardExtras {
                            front_html: row.front_html.clone(),
                            back_html: row.back_html.clone(),
                            cloze_text: row.cloze_text.clone(),
                            cloze_index: row.cloze_index,
                            type_answer: row.type_answer.clone(),
                        },
                        suspended: row.suspended,
                        media,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let deck = match self.store.find_deck_by_name(user, &deck_name)? {
                Some(id) => id,
                None => self.store.create_deck(user, &deck_name, "", now_ms)?,
            };
            // The batch was admitted as a whole above, so no per-deck cap.
            let ids = self.store.fill_imported_deck(user, deck, &cards, now_ms)?;
            created += ids.len();
            for (id, row) in ids.iter().zip(&batch) {
                if let Some(anki) = row.anki_card_id {
                    card_of.insert(anki, *id);
                }
            }
        }
        if !history.is_empty() {
            self.replay_history(user, history, &card_of, now_ms)?;
        }
        Ok(created)
    }

    /// Replays imported Anki reviews, oldest first, through FSRS: each
    /// becomes a review_log row (written in chunks, the connection being
    /// shared) and the last one per card its state. An entry before the
    /// epoch or after `now_ms`, for an Anki card no row came from, or not
    /// after the card's previous review is skipped, so a crafted log
    /// cannot leave a card with a "last review" in the far future; a
    /// scheduler error skips the entry as well. Never fails the import
    /// over one review.
    fn replay_history(
        &self,
        user: UserId,
        mut history: Vec<HistoryEntry>,
        card_of: &std::collections::HashMap<i64, CardId>,
        now_ms: i64,
    ) -> Result<()> {
        const HISTORY_WRITE_CHUNK: usize = 10_000;
        let started = std::time::Instant::now();
        history.sort_unstable_by_key(|e| e.at_ms);
        let scheduler = self.scheduler_for(&self.store.get_settings(user)?)?;
        let mut states: std::collections::HashMap<CardId, CardState> =
            std::collections::HashMap::new();
        let mut pending: Vec<ImportedReview> = Vec::with_capacity(HISTORY_WRITE_CHUNK);
        let rows = history.len();
        let mut written = 0usize;
        let mut skipped = 0usize;
        for entry in history {
            let Some(&card) = card_of.get(&entry.anki_card_id) else {
                skipped += 1;
                continue;
            };
            if !(0..=now_ms).contains(&entry.at_ms) {
                skipped += 1;
                continue;
            }
            let state = states
                .entry(card)
                .or_insert_with(|| CardState::new_card(entry.at_ms));
            if state.last_review_ms.is_some_and(|last| entry.at_ms <= last) {
                skipped += 1;
                continue;
            }
            match scheduler.review(state, entry.rating, entry.at_ms) {
                Ok(outcome) => {
                    *state = outcome.state.clone();
                    pending.push(ImportedReview {
                        card,
                        reviewed_at_ms: entry.at_ms,
                        outcome,
                        answer_ms: entry.answer_ms,
                    });
                }
                Err(_) => skipped += 1,
            }
            if pending.len() == HISTORY_WRITE_CHUNK {
                written += self.store.import_reviews(user, &pending)?;
                pending.clear();
            }
        }
        if !pending.is_empty() {
            written += self.store.import_reviews(user, &pending)?;
        }
        let final_states: Vec<(CardId, CardState)> =
            states.into_iter().filter(|(_, s)| s.reps > 0).collect();
        self.store.set_card_states(user, &final_states)?;
        tracing::info!(
            user = user.raw(),
            rows,
            written,
            skipped,
            cards = final_states.len(),
            replay_ms = started.elapsed().as_millis() as u64,
            "import history replayed"
        );
        Ok(())
    }

    /// Adopts the FSRS-relevant settings an Anki package carried (opt-in
    /// at import). Params are validated by building a scheduler before
    /// anything is saved; invalid params are dropped, not fatal.
    pub fn adopt_imported_settings(
        &self,
        user: UserId,
        settings: &flash_store::import::ImportedSettings,
    ) -> Result<()> {
        let mut params = settings.fsrs_params.as_deref();
        if let Some(p) = params {
            if Scheduler::new(Some(p), settings.desired_retention.unwrap_or(0.9)).is_err() {
                tracing::warn!("imported FSRS params rejected by scheduler; skipping them");
                params = None;
            }
        }
        self.store.adopt_study_settings(
            user,
            settings.desired_retention,
            params,
            settings.new_per_day,
        )?;
        Ok(())
    }
}
