//! Study-queue ordering: which card comes next, given what's due.
//!
//! Pure functions over slices so the policy is trivially testable. The
//! store fetches candidates; this module decides order and new-card limits.

use std::collections::{HashMap, HashSet};

use crate::ids::{CardId, DeckId};
use crate::rating::Phase;

/// What a study session covers: every card, one deck (with its
/// subdecks), the cards carrying tags, or a deck narrowed by tags.
///
/// `tags` is normalized (trimmed, lowercased, deduplicated, blanks
/// dropped) so its names compare equal to stored ones, and a card is in
/// scope only when it carries every one of them. An empty `tags` with no
/// `deck` is the whole account.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StudyScope {
    pub deck: Option<DeckId>,
    pub tags: Vec<String>,
}

/// Most tags one scope may name. The surfaces refuse a longer list at
/// the edge; the queue refuses it last.
pub const MAX_SCOPE_TAGS: usize = 10;

impl StudyScope {
    /// Everything due, in every deck.
    pub fn all() -> Self {
        Self::default()
    }

    /// One deck and its subdecks.
    pub fn deck(id: DeckId) -> Self {
        Self {
            deck: Some(id),
            tags: Vec::new(),
        }
    }

    /// The cards carrying every one of `tags`, in any deck.
    pub fn tagged<I, S>(tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::new(None, tags)
    }

    /// A deck, tags, or both; the tags are normalized on the way in.
    pub fn new<I, S>(deck: Option<DeckId>, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            deck,
            tags: normalize_tags(tags),
        }
    }

    /// Whether this covers the whole account, unfiltered.
    pub fn is_all(&self) -> bool {
        self.deck.is_none() && self.tags.is_empty()
    }

    /// Serialized form stored on study_sessions.scope. The two shapes
    /// that predate tags keep their spelling ("all", "deck:3") so old
    /// rows and new ones look alike; anything with tags is JSON, because
    /// a tag may hold any character and no separator would be safe.
    pub fn as_string(&self) -> String {
        match (self.deck, self.tags.is_empty()) {
            (None, true) => "all".to_string(),
            (Some(id), true) => format!("deck:{id}"),
            _ => serde_json::json!({
                "deck": self.deck.map(|d| d.0),
                "tags": self.tags,
            })
            .to_string(),
        }
    }

    /// Reads back any form `as_string` has ever written, including the
    /// pre-tags "tag:x". A row this cannot read is the whole account:
    /// a session that studies too much beats one that studies nothing.
    pub fn parse(s: &str) -> Self {
        if s.starts_with('{') {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else {
                return Self::all();
            };
            let deck = v
                .get("deck")
                .and_then(serde_json::Value::as_i64)
                .map(DeckId);
            let tags = v
                .get("tags")
                .and_then(serde_json::Value::as_array)
                .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_else(Vec::new);
            return Self::new(deck, tags);
        }
        if let Some(id) = s.strip_prefix("deck:").and_then(|v| v.parse().ok()) {
            return Self::deck(DeckId(id));
        }
        if let Some(tag) = s.strip_prefix("tag:") {
            return Self::tagged([tag]);
        }
        Self::all()
    }
}

/// One tag as every path stores and compares it: trimmed, lowercased,
/// in one Unicode form (`text::nfc`). Blank stays blank.
pub fn normalize_tag(tag: &str) -> String {
    crate::text::nfc(&tag.trim().to_lowercase())
}

/// Each tag `normalize_tag`ged, blanks dropped, the first of each kept.
/// Order is preserved so a scope reads back as it was written.
pub fn normalize_tags<I, S>(tags: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = normalize_tag(tag.as_ref());
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// Minimal projection of card_state needed for queue decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueEntry {
    pub card_id: CardId,
    pub deck_id: DeckId,
    pub phase: Phase,
    pub due_ms: i64,
}

/// What one deck may still introduce/review today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckBudget {
    pub new: u32,
    pub reviews: u32,
}

/// Today's remaining allowances: account-wide totals plus per-deck
/// remainders (Anki-style: each deck's own limit applies, and the account
/// limit caps the sum). A deck missing from `per_deck` is bounded only by
/// the totals.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Budgets {
    pub new_total: u32,
    pub review_total: u32,
    pub per_deck: HashMap<DeckId, DeckBudget>,
    /// Decks paced toward a goal date: their new cards come from their
    /// own slot in `per_deck` alone, on top of `new_total`, so no other
    /// deck can spend what the goal needs.
    pub paced: HashSet<DeckId>,
}

impl Budgets {
    /// Totals only, no per-deck limits.
    pub fn flat(new_total: u32, review_total: u32) -> Self {
        Self {
            new_total,
            review_total,
            per_deck: HashMap::new(),
            paced: HashSet::new(),
        }
    }

    /// The most new cards `order_queue` can take from any one deck: a
    /// paced deck's own remainder, any other deck's no more than the
    /// total. A deck's new cards are offered in id order and the first
    /// one refused ends that deck's run (every count only falls), so its
    /// new cards past this many never reach a queue and the store may
    /// leave them out.
    pub fn most_new_per_deck(&self) -> u32 {
        self.paced
            .iter()
            .filter_map(|deck| self.per_deck.get(deck))
            .map(|d| d.new)
            .fold(self.new_total, u32::max)
    }

    /// Takes one unit of `kind` for `deck` if both the deck's remainder and
    /// the total allow it (a paced deck's new cards: its remainder alone).
    fn take(&mut self, deck: DeckId, new: bool) -> bool {
        if new && self.paced.contains(&deck) {
            return match self.per_deck.get_mut(&deck) {
                Some(d) if d.new > 0 => {
                    d.new -= 1;
                    true
                }
                _ => false,
            };
        }
        let total = if new {
            &mut self.new_total
        } else {
            &mut self.review_total
        };
        if *total == 0 {
            return false;
        }
        if let Some(d) = self.per_deck.get_mut(&deck) {
            let slot = if new { &mut d.new } else { &mut d.reviews };
            if *slot == 0 {
                return false;
            }
            *slot -= 1;
        }
        *total -= 1;
        true
    }
}

/// What counts as due at a moment. Learning and relearning steps are
/// minutes long and always exact. A review card is due either from its
/// exact time or, Anki's way and the default, for its whole study day:
/// every review card due before the day ends can be studied from the
/// day's start, so one sitting clears the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Due {
    pub now_ms: i64,
    /// Review cards due before this are due now: `now_ms + 1` when exact,
    /// the end of the current study day when by day.
    pub reviews_before_ms: i64,
}

impl Due {
    pub fn exact(now_ms: i64) -> Self {
        Self {
            now_ms,
            reviews_before_ms: now_ms + 1,
        }
    }

    pub fn by_day(now_ms: i64, day_end_ms: i64) -> Self {
        Self {
            now_ms,
            reviews_before_ms: day_end_ms.max(now_ms + 1),
        }
    }

    /// Whether a card that is not new is due now.
    pub fn is_due(&self, phase: Phase, due_ms: i64) -> bool {
        match phase {
            Phase::New => true,
            Phase::Review => due_ms < self.reviews_before_ms,
            Phase::Learning | Phase::Relearning => due_ms <= self.now_ms,
        }
    }
}

/// When nothing else is due, learning cards within this window are shown
/// early rather than ending the session mid-acquisition (Anki's
/// "learn ahead limit"; covers the 10/15-minute steps).
pub const LEARN_AHEAD_MS: i64 = 20 * 60_000;

/// Orders due cards for a session:
/// 1. Learning/relearning cards whose step timer expired (most urgent:
///    they're mid-acquisition and short-interval; never capped).
/// 2. Due review cards, oldest due first (most overdue = most at risk),
///    under the per-deck and account review budgets.
/// 3. New cards in creation (id) order, under the per-deck and account
///    new-card budgets.
///
/// If that yields nothing, learning cards due within LEARN_AHEAD_MS are
/// returned instead (soonest first) so a session never strands a card in
/// a sub-day step.
pub fn order_queue(entries: &[QueueEntry], due: Due, budgets: &Budgets) -> Vec<CardId> {
    let now_ms = due.now_ms;
    let mut budgets = budgets.clone();
    let mut learning: Vec<&QueueEntry> = Vec::new();
    let mut review: Vec<&QueueEntry> = Vec::new();
    let mut new: Vec<&QueueEntry> = Vec::new();
    for e in entries {
        if !due.is_due(e.phase, e.due_ms) {
            continue;
        }
        match e.phase {
            Phase::Learning | Phase::Relearning => learning.push(e),
            Phase::Review => review.push(e),
            Phase::New => new.push(e),
        }
    }
    learning.sort_by_key(|e| (e.due_ms, e.card_id));
    review.sort_by_key(|e| (e.due_ms, e.card_id));
    new.sort_by_key(|e| e.card_id);
    let review: Vec<&QueueEntry> = review
        .into_iter()
        .filter(|e| budgets.take(e.deck_id, false))
        .collect();
    let new: Vec<&QueueEntry> = new
        .into_iter()
        .filter(|e| budgets.take(e.deck_id, true))
        .collect();

    let ordered: Vec<CardId> = learning
        .into_iter()
        .chain(review)
        .chain(new)
        .map(|e| e.card_id)
        .collect();
    if !ordered.is_empty() {
        return ordered;
    }

    // Learn-ahead fallback.
    let mut ahead: Vec<&QueueEntry> = entries
        .iter()
        .filter(|e| {
            matches!(e.phase, Phase::Learning | Phase::Relearning)
                && e.due_ms <= now_ms + LEARN_AHEAD_MS
        })
        .collect();
    ahead.sort_by_key(|e| (e.due_ms, e.card_id));
    ahead.into_iter().map(|e| e.card_id).collect()
}
