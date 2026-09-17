//! Domain services: the single choke point through which every interface
//! (web UI, MCP tools, future API) reads and mutates study state. Handlers
//! translate protocol <-> these calls and nothing else.

use std::sync::Arc;

use flash_core::queue::{order_queue, StudyScope};
use flash_core::{
    validate_card_text, AiDraftId, AiJobId, AiSourceId, CardId, CardState, DeckId, GradingMode,
    MediaId, NoteId, Rating, Scheduler, SessionId, UserId, UserSettings, VoiceSettings,
    VoiceSettingsPatch,
};
use flash_store::import::{HistoryEntry, ImportRow};
use flash_store::notes::{generate_cards, GeneratedCard, NoteType, MAX_FIELD_HTML};
use flash_store::{
    richtext, AiCallStep, AiDraftCounts, AiDraftRow, AiDraftStatus, AiJobNew, AiJobOp, AiJobRow,
    AiJobState, AiSourceKind, AiSourceNew, AiSourceRow, AiTurnNew, AiTurnRole, AiTurnRow, CardRow,
    DeckSummary, GrantRow, ImportedReview, NoteSave, SessionStats, Store, StoreError,
};
use jiff::{tz::TimeZone, Timestamp, Zoned};
use sha2::{Digest, Sha256};

use crate::ai::provider::{
    CardContext, DialogState, LlmError, Reply, SpokenInput, SpokenIntent, SpokenTurn,
};
use crate::ai::sources;
use crate::media_store::MediaStore;
use crate::policy::{
    AddSource, Admission, AiAdmission, AiInput, AiPolicy, AiRequest, AiUsage, CardPolicy, Unlimited,
};
use crate::state::{AiRuntime, HeavyPermit};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Invalid(String),
    #[error("scheduler: {0}")]
    Scheduler(String),
    /// Our own failure (a template, a serializer, a build step): the
    /// detail is for the log, and every surface shows "internal error".
    #[error("internal error")]
    Internal(String),
    /// An add the card policy refused; `message` is the policy's own
    /// sentence and reaches AI assistants verbatim through MCP.
    #[error("{message}")]
    OverCap {
        current: u32,
        cap: u32,
        message: String,
    },
    /// A generation the plan or this server refused; `code` is the
    /// policy's stable token (`pro_required`, `monthly_limit`, …) a
    /// client branches on, `message` the sentence a person reads.
    #[error("{message}")]
    AiDenied { code: &'static str, message: String },
}

pub type Result<T> = std::result::Result<T, ServiceError>;

#[derive(Debug, Clone, PartialEq)]
pub struct CardFront {
    pub card_id: CardId,
    pub front: String,
    /// The front as a voice reads it (`richtext::spoken_text`): a cloze
    /// deletion is "blank", a picture is its alt text or its kind.
    pub spoken_front: String,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub grading_mode: GradingMode,
    pub cards_due: u32,
    /// Eligible cards today's new/review limits kept out of the queue.
    pub held_by_limit: u32,
    pub first_card: Option<CardFront>,
    /// The card after `first_card` in today's queue, so the UI can warm
    /// its media before it's shown. None when the queue has fewer than two.
    pub next_card_id: Option<CardId>,
    /// The cards after `first_card`, a few of them, so their voice clips
    /// can be made before they are shown.
    pub upcoming: Vec<CardId>,
}

#[derive(Debug, Clone)]
pub struct SubmitResult {
    pub next_card: Option<CardFront>,
    /// The card after `next_card` in the queue (media warm-up); None when
    /// there isn't one.
    pub next_card_id: Option<CardId>,
    /// The cards after `next_card`, a few of them (voice clips ahead).
    pub upcoming: Vec<CardId>,
    pub remaining: u32,
    /// Eligible cards today's new/review limits kept out of the queue.
    pub held_by_limit: u32,
}

/// A synthesized reading of a card side, ready to play: how long it
/// runs and its loudness curve (one byte per 50 ms). Where the bytes
/// are is the surface's to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceClip {
    pub duration_ms: u32,
    pub envelope: Vec<u8>,
}

/// What a study surface shows for a card: rich HTML when the card has
/// it, whether it asks for a typed answer, and which images to warm
/// (this card's still-hidden back, and the next card's both sides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyCard {
    pub card_id: CardId,
    pub front: String,
    pub front_html: Option<String>,
    /// The front read aloud; see `CardFront::spoken_front`.
    pub spoken_front: String,
    /// The front in the account's natural voice, when the clip exists
    /// (`tts::serve::attach_study_card`); None reads with the device.
    pub audio_front: Option<VoiceClip>,
    pub wants_typing: bool,
    pub preload_media: Vec<i64>,
    pub prefetch_media: Vec<i64>,
    /// What a speech recognizer is primed with while this front shows:
    /// the answer's words and their spelled-out forms
    /// (`voice_hints::answer_hints`). On the device before the reveal by
    /// design, like the hidden back's images.
    pub answer_hints: Vec<String>,
}

/// One run of a typed answer, right or wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSpan {
    pub text: String,
    pub ok: bool,
}

/// A revealed card: the front again (the surface re-renders it), the
/// back, and the typed-answer comparison when one was typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reveal {
    pub card_id: CardId,
    pub front: String,
    pub front_html: Option<String>,
    pub back: String,
    pub back_html: Option<String>,
    /// Both sides read aloud: the front still hides a cloze deletion
    /// ("blank"), the back speaks the word.
    pub spoken_front: String,
    pub spoken_back: String,
    /// Both sides in the account's natural voice, when the clips exist.
    pub audio_front: Option<VoiceClip>,
    pub audio_back: Option<VoiceClip>,
    pub typed_diff: Option<Vec<DiffSpan>>,
}

/// The Today page in one read.
#[derive(Debug, Clone)]
pub struct Dashboard {
    pub counts: QueueCounts,
    /// Account-wide "more new cards today" already granted.
    pub boost_today: u32,
    pub decks: Vec<DeckSummary>,
    /// The "Connect your AI" card, until the user dismisses it.
    pub show_connect_cta: bool,
}

/// One page of a deck's cards, as the lists show them.
#[derive(Debug, Clone)]
pub struct CardsPage {
    pub rows: Vec<CardRow>,
    pub page: u32,
    pub pages: u32,
    pub total: u32,
}

/// A deck's settings in one read: the deck, its limit overrides beside
/// the account defaults they would inherit, today's boost, and the live
/// card count (sharing is refused at zero).
#[derive(Debug, Clone)]
pub struct DeckDetail {
    pub deck: DeckSummary,
    pub limits: flash_store::DeckLimits,
    pub default_new: u32,
    pub default_reviews: u32,
    pub boost_today: u32,
    pub card_count: u32,
}

/// The cap the store must re-check (None = unlimited) plus the validated
/// texts, ready to insert.
type AdmittedCards = (Option<u32>, Vec<(flash_core::CardText, Vec<String>)>);

/// How a card relates to its note, for list badges: "cloze 2",
/// "reversed", "typed"; None for a plain basic card.
pub fn card_kind(card: &CardRow) -> Option<String> {
    match (card.cloze_index, card.ord, card.type_answer.is_some()) {
        (Some(n), _, _) => Some(format!("cloze {n}")),
        (None, 1, _) => Some("reversed".into()),
        (None, _, true) => Some("typed".into()),
        _ => None,
    }
}

/// The queue entry following `current`, for media warm-up.
fn lookahead(queue: &[CardId], current: Option<CardId>) -> Option<CardId> {
    let current = current?;
    let at = queue.iter().position(|&id| id == current)?;
    queue.get(at + 1).copied()
}

/// The few cards after `current` in the queue, for the clips made ahead.
fn upcoming(queue: &[CardId], current: Option<CardId>) -> Vec<CardId> {
    let Some(current) = current else {
        return Vec::new();
    };
    let Some(at) = queue.iter().position(|&id| id == current) else {
        return Vec::new();
    };
    queue
        .iter()
        .skip(at + 1)
        .take(crate::tts::LOOKAHEAD_CARDS)
        .copied()
        .collect()
}

/// Where a review came from: the surface (`web`, `mobile`, `mcp`) and,
/// when the surface has several clients, which one (an MCP client's
/// name, the app's platform).
#[derive(Debug, Clone, Copy)]
pub struct ReviewOrigin<'a> {
    pub source: &'a str,
    pub client: Option<&'a str>,
}

impl<'a> ReviewOrigin<'a> {
    pub const WEB: ReviewOrigin<'static> = ReviewOrigin {
        source: "web",
        client: None,
    };

    pub fn new(source: &'a str, client: Option<&'a str>) -> Self {
        ReviewOrigin { source, client }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QueueCounts {
    pub due: u32,
    pub new_available: u32,
    pub reviewed_today: u32,
}

#[derive(Debug, Clone)]
pub struct DayCount {
    pub label: String,
    pub count: u32,
    /// Bar height as a percentage of the busiest day (chart-ready).
    pub pct: u32,
}

/// One day cell of the yearly heatmap.
#[derive(Debug, Clone)]
pub struct HeatCell {
    pub count: u32,
    /// 0 = empty, 1-4 = intensity relative to the busiest day.
    pub level: u8,
    /// MM/DD, for the tooltip.
    pub label: String,
    /// After today: `count` is cards due, not reviews done.
    pub future: bool,
    pub today: bool,
}

/// A run of heatmap columns sharing a month, for the label row.
#[derive(Debug, Clone)]
pub struct MonthSpan {
    pub label: String,
    pub weeks: u32,
}

#[derive(Debug, Clone)]
pub struct StatsData {
    pub reviews_30d: u32,
    /// None until there are mature (review-phase) reviews to measure.
    pub retention_pct: Option<u32>,
    pub streak: u32,
    pub longest_streak: u32,
    /// Reviews per day over the window the heatmap shows, counted from
    /// the first review when it falls inside it.
    pub daily_avg: u32,
    /// Share of days with at least one review, same denominator.
    pub days_learned_pct: u32,
    pub total_reviews: u32,
    pub active_cards: u32,
    pub mature_cards: u32,
    pub days: Vec<DayCount>,
    pub upcoming: Vec<DayCount>,
    /// Week columns of 7 cells each (Sunday first), oldest week first:
    /// 53 in the trailing view, 53 or 54 for a calendar year.
    pub heat_weeks: Vec<Vec<HeatCell>>,
    pub heat_months: Vec<MonthSpan>,
    /// Every year from the first review's through this one, oldest
    /// first; empty when the history fits in one.
    pub heat_years: Vec<i32>,
    /// The past calendar year the heatmap shows; None is the trailing view.
    pub heat_year: Option<i32>,
}

/// The account as a client's "me": profile, plan, theme, and which login
/// methods it holds (so settings can refuse to remove the last one).
#[derive(Debug, Clone)]
pub struct AccountOverview {
    pub id: UserId,
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub theme: String,
    pub grading_mode: GradingMode,
    pub has_password: bool,
    pub passkeys: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PasskeyInfo {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// Everything the settings page shows, in one read.
#[derive(Debug, Clone)]
pub struct SettingsOverview {
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub theme: String,
    pub grading_mode: GradingMode,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub timezone: String,
    pub day_cutoff_hour: u8,
    pub desired_retention: f32,
    pub has_password: bool,
    pub passkeys: Vec<PasskeyInfo>,
    /// Live cards across every deck.
    pub card_count: u32,
    /// How the account studies by voice.
    pub voice: VoiceSettings,
}

/// The knobs that decide when "today" starts and how FSRS schedules.
/// `None` leaves a field unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StudySettingsPatch {
    pub timezone: Option<String>,
    pub day_cutoff_hour: Option<u8>,
    pub desired_retention: Option<f32>,
}

/// Sync by design; the async layer wraps calls in spawn_blocking.
#[derive(Clone)]
pub struct Services {
    store: Arc<Store>,
    policy: Arc<dyn CardPolicy>,
    ai_policy: Arc<dyn AiPolicy>,
}

impl Services {
    /// Unlimited cards and generations: the open server.
    pub fn new(store: Arc<Store>) -> Self {
        Self::with_policies(store, Arc::new(Unlimited), Arc::new(Unlimited))
    }

    /// A card policy with unlimited generations.
    pub fn with_policy(store: Arc<Store>, policy: Arc<dyn CardPolicy>) -> Self {
        Self::with_policies(store, policy, Arc::new(Unlimited))
    }

    pub fn with_policies(
        store: Arc<Store>,
        policy: Arc<dyn CardPolicy>,
        ai_policy: Arc<dyn AiPolicy>,
    ) -> Self {
        Self {
            store,
            policy,
            ai_policy,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The gate a generation passes before the model is called.
    pub fn ai_policy(&self) -> &dyn AiPolicy {
        self.ai_policy.as_ref()
    }

    fn over_cap(&self, current: u32, cap: u32) -> ServiceError {
        ServiceError::OverCap {
            current,
            cap,
            message: self.policy.over_cap_message(current, cap),
        }
    }

    /// The store re-checks the cap inside its insert transaction; a race
    /// it loses surfaces as the same refusal the gate would have given.
    fn cap_error(&self, e: StoreError) -> ServiceError {
        match e {
            StoreError::CapExceeded { current, cap } => self.over_cap(current, cap),
            other => other.into(),
        }
    }

    fn scheduler_for(&self, settings: &UserSettings) -> Result<Scheduler> {
        Scheduler::new(settings.fsrs_params.as_deref(), settings.desired_retention)
            .map_err(|e| ServiceError::Scheduler(e.to_string()))
    }

    /// Start of the user's current study day (e.g. 4am local), for the
    /// daily new-card budget.
    fn study_day_start_ms(settings: &UserSettings, now_ms: i64) -> i64 {
        let tz = TimeZone::get(&settings.timezone).unwrap_or(TimeZone::UTC);
        let now = Timestamp::from_millisecond(now_ms).unwrap_or(Timestamp::UNIX_EPOCH);
        let local: Zoned = now.to_zoned(tz);
        let cutoff = local
            .with()
            .hour(settings.day_cutoff_hour as i8)
            .minute(0)
            .second(0)
            .subsec_nanosecond(0)
            .build()
            .unwrap_or_else(|_| local.clone());
        let day_start = if cutoff.timestamp() > now {
            cutoff.yesterday().unwrap_or(cutoff)
        } else {
            cutoff
        };
        day_start.timestamp().as_millisecond()
    }

    /// Today's remaining allowances, Anki-style: each deck's own new/review
    /// limit (or the account default) minus what that deck consumed today,
    /// plus any "more new cards today" boost; the account totals cap the
    /// sum. An account boost raises the default, so decks that inherit it
    /// get it too. The free-plan clamp applies to the account new total
    /// (unseen cards stop entering once rotation reaches the cap). Shared by
    /// the queue builder, dashboard counts, and deck summaries so they agree.
    fn budgets(
        &self,
        user: UserId,
        settings: &UserSettings,
        now_ms: i64,
    ) -> Result<flash_core::queue::Budgets> {
        let day_start = Self::study_day_start_ms(settings, now_ms);
        let consumed = self.store.daily_consumption_by_deck(user, day_start)?;
        let (introduced_total, reviewed_total) = consumed
            .values()
            .fold((0u32, 0u32), |acc, (n, r)| (acc.0 + n, acc.1 + r));
        let account_boost = active_boost(settings.boost_new, settings.boost_day, day_start);
        let review_total = settings.reviews_per_day.saturating_sub(reviewed_total);

        // A deck boost must actually yield more cards even when the account
        // allowance is already spent (the usual moment someone asks for
        // more), so today's deck boosts lift the account ceiling too.
        let mut deck_boosts_total = 0u32;
        let mut per_deck = std::collections::HashMap::new();
        for (deck, limits) in self.store.all_deck_limits(user)? {
            let (n, r) = consumed.get(&deck).copied().unwrap_or((0, 0));
            let deck_boost = active_boost(limits.boost_new, limits.boost_day, day_start);
            deck_boosts_total += deck_boost;
            let new_limit = match limits.new_per_day {
                Some(own) => own,
                None => settings.new_per_day + account_boost,
            } + deck_boost;
            let review_limit = limits.reviews_per_day.unwrap_or(settings.reviews_per_day);
            per_deck.insert(
                deck,
                flash_core::queue::DeckBudget {
                    new: new_limit.saturating_sub(n),
                    reviews: review_limit.saturating_sub(r),
                },
            );
        }
        let account_new = (settings.new_per_day + account_boost + deck_boosts_total)
            .saturating_sub(introduced_total);

        let new_total = self
            .policy
            .new_budget(&self.store, user, account_new, now_ms)?;
        Ok(flash_core::queue::Budgets {
            new_total,
            review_total,
            per_deck,
        })
    }

    /// Ordered due-card ids for a scope, respecting today's limits, plus
    /// how many otherwise-eligible cards (new, or due now) today's limits
    /// held back. The held count lets the study surfaces explain an empty
    /// or short queue instead of implying the user is done.
    fn build_queue(
        &self,
        user: UserId,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<(Vec<CardId>, u32)> {
        let settings = self.store.get_settings(user)?;
        let budgets = self.budgets(user, &settings, now_ms)?;
        let entries = self.store.queue_entries(user, scope, now_ms)?;
        let eligible = entries
            .iter()
            .filter(|e| e.phase == flash_core::Phase::New || e.due_ms <= now_ms)
            .count();
        let ordered = order_queue(&entries, now_ms, &budgets);
        let held = eligible.saturating_sub(ordered.len()) as u32;
        Ok((ordered, held))
    }

    // ---- Daily limits ----

    pub fn set_daily_limits(
        &self,
        user: UserId,
        new_per_day: u32,
        reviews_per_day: u32,
    ) -> Result<()> {
        if new_per_day > flash_core::settings::MAX_NEW_PER_DAY
            || reviews_per_day > flash_core::settings::MAX_REVIEWS_PER_DAY
        {
            return Err(ServiceError::Invalid("limit out of range".into()));
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
            return Err(ServiceError::Invalid("limit out of range".into()));
        }
        Ok(self
            .store
            .set_deck_limits(user, deck, new_per_day, reviews_per_day)?)
    }

    pub fn rename_deck(&self, user: UserId, deck: DeckId, name: &str) -> Result<()> {
        Ok(self.store.rename_deck(user, deck, name)?)
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
            return Err(ServiceError::Invalid("boost out of range".into()));
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

    fn card_front(&self, user: UserId, card: CardId) -> Result<Option<CardFront>> {
        Ok(self.store.get_card(user, card)?.map(|c| CardFront {
            card_id: c.id,
            spoken_front: richtext::spoken_text(c.front_html.as_deref(), &c.front),
            front: c.front,
        }))
    }

    // ---- Study flow ----

    pub fn start_session(
        &self,
        user: UserId,
        scope: StudyScope,
        now_ms: i64,
    ) -> Result<SessionInfo> {
        // A session scoped to a deck is a session on one of the user's
        // decks; a foreign id is not found, not an empty session.
        if let StudyScope::Deck(deck) = scope {
            self.store.get_deck_limits(user, deck)?;
        }
        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        let session_id = self.store.start_session(user, &scope, now_ms)?;
        let settings = self.store.get_settings(user)?;
        let first_card = match queue.first() {
            Some(&id) => self.card_front(user, id)?,
            None => None,
        };
        Ok(SessionInfo {
            session_id,
            grading_mode: settings.grading_mode,
            cards_due: queue.len() as u32,
            held_by_limit,
            upcoming: upcoming(&queue, queue.first().copied()),
            first_card,
            next_card_id: queue.get(1).copied(),
        })
    }

    /// Records a review and returns the next card in the same call: the
    /// voice loop is one tool call per card.
    pub fn submit_review(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        rating: Rating,
        origin: ReviewOrigin<'_>,
        now_ms: i64,
    ) -> Result<SubmitResult> {
        let scope = self.store.get_session_scope(user, session)?;
        let settings = self.store.get_settings(user)?;
        let state = self.store.get_card_state(user, card)?;

        // Transport-level retries (Streamable HTTP clients may re-POST)
        // must not double-record: a repeat submission for a card reviewed
        // seconds ago is answered idempotently with the current queue.
        const RETRY_WINDOW_MS: i64 = 5_000;
        if state
            .last_review_ms
            .is_some_and(|t| (0..RETRY_WINDOW_MS).contains(&now_ms.saturating_sub(t)))
        {
            let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
            let next = queue.iter().find(|&&id| id != card).or(queue.first());
            let next_card = match next {
                Some(&id) => self.card_front(user, id)?,
                None => None,
            };
            let next_card_id = lookahead(&queue, next.copied());
            return Ok(SubmitResult {
                remaining: queue.len() as u32,
                held_by_limit,
                next_card,
                next_card_id,
                upcoming: upcoming(&queue, next.copied()),
            });
        }

        let outcome = self
            .scheduler_for(&settings)?
            .review(&state, rating, now_ms)
            .map_err(|e| ServiceError::Scheduler(e.to_string()))?;
        self.store.record_review(
            user,
            card,
            &outcome,
            now_ms,
            origin.source,
            origin.client,
            Some(session),
        )?;

        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        // The just-reviewed card may already be due again (learning step);
        // don't hand it straight back.
        let next = queue.iter().find(|&&id| id != card).or(queue.first());
        let next_card = match next {
            Some(&id) => self.card_front(user, id)?,
            None => None,
        };
        let next_card_id = lookahead(&queue, next.copied());
        Ok(SubmitResult {
            remaining: queue.len() as u32,
            held_by_limit,
            next_card,
            next_card_id,
            upcoming: upcoming(&queue, next.copied()),
        })
    }

    /// Both sides of the user's card as a voice reads them, for the
    /// clips made ahead; None for a card that is not theirs.
    pub fn spoken_sides(&self, user: UserId, card: CardId) -> Result<Option<(String, String)>> {
        Ok(self.store.get_card(user, card)?.map(|c| {
            (
                richtext::spoken_text(c.front_html.as_deref(), &c.front),
                richtext::spoken_text(c.back_html.as_deref(), &c.back),
            )
        }))
    }

    /// The card's back text, for reveal ("show me the answer").
    pub fn reveal(&self, user: UserId, card: CardId) -> Result<String> {
        Ok(self
            .store
            .get_card(user, card)?
            .ok_or(StoreError::NotFound("card"))?
            .back)
    }

    /// The study view of `front`, with `next` (the queue's following card)
    /// contributing its images to the prefetch list.
    pub fn study_card(
        &self,
        user: UserId,
        front: CardFront,
        next: Option<CardId>,
    ) -> Result<StudyCard> {
        let row = self
            .store
            .get_card(user, front.card_id)?
            .ok_or(StoreError::NotFound("card"))?;
        let preload_media = row
            .back_html
            .as_deref()
            .map(richtext::media_image_ids)
            .unwrap_or_default();
        let prefetch_media = match next {
            Some(id) => self.card_image_ids(user, id)?,
            None => Vec::new(),
        };
        let spoken_back = richtext::spoken_text(row.back_html.as_deref(), &row.back);
        Ok(StudyCard {
            card_id: front.card_id,
            front: front.front,
            front_html: row.front_html,
            spoken_front: front.spoken_front,
            audio_front: None,
            wants_typing: row.type_answer.is_some(),
            preload_media,
            prefetch_media,
            answer_hints: crate::voice_hints::answer_hints(&spoken_back),
        })
    }

    /// Every image id on both sides of a card, for prefetching.
    fn card_image_ids(&self, user: UserId, card: CardId) -> Result<Vec<i64>> {
        let Some(row) = self.store.get_card(user, card)? else {
            return Ok(Vec::new());
        };
        let mut ids = Vec::new();
        for html in [row.front_html.as_deref(), row.back_html.as_deref()]
            .into_iter()
            .flatten()
        {
            for id in richtext::media_image_ids(html) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    /// Reveal for the interactive surfaces: the back, plus the diff of
    /// what the user typed when the card asked for it.
    pub fn reveal_card(&self, user: UserId, card: CardId, typed: Option<&str>) -> Result<Reveal> {
        let row = self
            .store
            .get_card(user, card)?
            .ok_or(StoreError::NotFound("card"))?;
        let typed_diff = match (&row.type_answer, typed.map(str::trim)) {
            (Some(expected), Some(typed)) if !typed.is_empty() => Some(type_diff(expected, typed)),
            _ => None,
        };
        Ok(Reveal {
            card_id: row.id,
            spoken_front: richtext::spoken_text(row.front_html.as_deref(), &row.front),
            spoken_back: richtext::spoken_text(row.back_html.as_deref(), &row.back),
            audio_front: None,
            audio_back: None,
            front: row.front,
            front_html: row.front_html,
            back: row.back,
            back_html: row.back_html,
            typed_diff,
        })
    }

    /// What a session was started on, so an empty queue can offer the
    /// right boost (that deck's, or the account's).
    pub fn session_scope(&self, user: UserId, session: SessionId) -> Result<StudyScope> {
        Ok(self.store.get_session_scope(user, session)?)
    }

    /// Start of the user's current study day, for anything that runs on
    /// the same clock as the daily budgets (the reminder scheduler).
    pub fn study_day_start(&self, user: UserId, now_ms: i64) -> Result<i64> {
        let settings = self.store.get_settings(user)?;
        Ok(Self::study_day_start_ms(&settings, now_ms))
    }

    // ---- Today ----

    /// Today's numbers and the decks behind them. Archived decks are not
    /// part of today: the web dashboard and the app's Today screen both
    /// read this, and neither lists them.
    pub fn dashboard(&self, user: UserId, now_ms: i64) -> Result<Dashboard> {
        Ok(Dashboard {
            counts: self.queue_counts(user, &StudyScope::All, now_ms)?,
            boost_today: self.boost_today(user, None, now_ms)?,
            decks: self
                .list_decks(user, now_ms)?
                .into_iter()
                .filter(|d| d.archived_at.is_none())
                .collect(),
            show_connect_cta: !self.store.connect_cta_hidden(user)?,
        })
    }

    pub fn dismiss_connect_cta(&self, user: UserId) -> Result<()> {
        Ok(self.store.set_connect_cta_hidden(user, true)?)
    }

    /// The first `n` cards a session over everything would show, in
    /// order, without starting one: what is worth making ready ahead.
    pub fn queue_head(&self, user: UserId, n: usize, now_ms: i64) -> Result<Vec<CardId>> {
        let (mut queue, _) = self.build_queue(user, &StudyScope::All, now_ms)?;
        queue.truncate(n);
        Ok(queue)
    }

    pub fn end_session(
        &self,
        user: UserId,
        session: SessionId,
        now_ms: i64,
    ) -> Result<SessionStats> {
        self.store.end_session(user, session, now_ms)?;
        Ok(self.store.session_stats(user, session)?)
    }

    /// Dashboard numbers without starting a session, derived from the
    /// same ordered queue a session would get, so they always agree.
    pub fn queue_counts(
        &self,
        user: UserId,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<QueueCounts> {
        let settings = self.store.get_settings(user)?;
        let day_start = Self::study_day_start_ms(&settings, now_ms);
        let budgets = self.budgets(user, &settings, now_ms)?;
        let entries = self.store.queue_entries(user, scope, now_ms)?;
        let ordered = order_queue(&entries, now_ms, &budgets);
        let by_id: std::collections::HashMap<CardId, &flash_core::queue::QueueEntry> =
            entries.iter().map(|e| (e.card_id, e)).collect();
        let mut due = 0u32;
        let mut new_available = 0u32;
        for id in &ordered {
            if let Some(e) = by_id.get(id) {
                if e.phase == flash_core::Phase::New {
                    new_available += 1;
                } else if e.due_ms <= now_ms {
                    due += 1;
                }
            }
        }
        let reviewed_today = self.store.reviews_since(user, day_start)?.len() as u32;
        Ok(QueueCounts {
            due,
            new_available,
            reviewed_today,
        })
    }

    /// The Stats page's numbers: a scan of the whole review history, so a
    /// heavy job that needs its permit. `year` asks for a past calendar
    /// year in the heatmap; None, the current year, or a year before the
    /// first review is the trailing view (the past 46 weeks, today, and
    /// six weeks of upcoming load).
    pub fn stats(&self, permit: &HeavyPermit, now_ms: i64, year: Option<i32>) -> Result<StatsData> {
        let user = permit.user();
        let settings = self.store.get_settings(user)?;
        let tz = TimeZone::get(&settings.timezone).unwrap_or(TimeZone::UTC);
        const DAY: i64 = flash_core::scheduler::MS_PER_DAY;
        // Shift by the study-day cutoff so a 2am review counts toward the
        // previous day, matching the queue's notion of "today".
        let cutoff_ms = i64::from(settings.day_cutoff_hour) * 3_600_000;
        let local_of = |ts_ms: i64| -> Zoned {
            Timestamp::from_millisecond(ts_ms - cutoff_ms)
                .unwrap_or(Timestamp::UNIX_EPOCH)
                .to_zoned(tz.clone())
        };
        // A study day is the civil epoch day of the cutoff-shifted local
        // time: an index that keeps ordering and, being civil already,
        // converts to and from a date through UTC with no zone work.
        let day_of = |ts_ms: i64| -> i64 {
            let shifted = ts_ms - cutoff_ms;
            let local = local_of(ts_ms);
            (shifted + i64::from(local.offset().seconds()) * 1000).div_euclid(DAY)
        };
        let civil_of = |idx: i64| -> jiff::civil::Date {
            Timestamp::from_millisecond(idx * DAY)
                .unwrap_or(Timestamp::UNIX_EPOCH)
                .to_zoned(TimeZone::UTC)
                .date()
        };
        let idx_of = |date: jiff::civil::Date| -> i64 {
            date.to_zoned(TimeZone::UTC)
                .map(|z| z.timestamp().as_millisecond())
                .unwrap_or(0)
                .div_euclid(DAY)
        };
        let label_of = |idx: i64| -> String {
            let date = civil_of(idx);
            format!("{:02}/{:02}", date.month(), date.day())
        };

        // The whole history, folded into study days.
        let mut per_day: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
        let mut total_reviews = 0u32;
        for (bucket_ms, count) in self.store.review_buckets(user)? {
            *per_day.entry(day_of(bucket_ms)).or_default() += count;
            total_reviews += count;
        }
        // Retention needs each review's rating and phase: the last 30 days.
        let recent = self.store.reviews_since(user, now_ms - 30 * DAY)?;
        let reviews_30d = recent.len() as u32;
        let mut review_reviews = 0u32;
        let mut review_recalled = 0u32;
        for &(_, rating, phase_before) in &recent {
            if phase_before == 2 {
                review_reviews += 1;
                if rating > 1 {
                    review_recalled += 1;
                }
            }
        }

        let today_idx = day_of(now_ms);
        let mut streak = 0u32;
        let mut idx = today_idx;
        // Today counts if studied; otherwise the streak may end yesterday.
        if !per_day.contains_key(&idx) {
            idx -= 1;
        }
        while per_day.contains_key(&idx) {
            streak += 1;
            idx -= 1;
        }

        let mut longest_streak = 0u32;
        {
            let mut keys: Vec<i64> = per_day.keys().copied().collect();
            keys.sort_unstable();
            let mut run = 0u32;
            let mut prev: Option<i64> = None;
            for k in keys {
                run = if prev == Some(k - 1) { run + 1 } else { 1 };
                prev = Some(k);
                longest_streak = longest_streak.max(run);
            }
        }

        let mut days = Vec::with_capacity(30);
        for offset in (0..30).rev() {
            let idx = day_of(now_ms - offset * DAY);
            days.push(DayCount {
                label: label_of(idx),
                count: per_day.get(&idx).copied().unwrap_or(0),
                pct: 0,
            });
        }
        let max = days.iter().map(|d| d.count).max().unwrap_or(0).max(1);
        for day in &mut days {
            day.pct = day.count * 100 / max;
        }

        let mut upcoming_map: std::collections::HashMap<i64, u32> =
            std::collections::HashMap::new();
        for due in self.store.due_between(user, now_ms, now_ms + 7 * DAY)? {
            *upcoming_map.entry(day_of(due)).or_default() += 1;
        }
        let mut upcoming = Vec::with_capacity(7);
        for offset in 0..7 {
            let idx = day_of(now_ms + offset * DAY);
            upcoming.push(DayCount {
                label: if offset == 0 {
                    "Today".to_string()
                } else {
                    label_of(idx)
                },
                count: upcoming_map.get(&idx).copied().unwrap_or(0),
                pct: 0,
            });
        }

        // ---- Heatmap: week columns, Sunday first. The trailing view is
        // 53 of them, today's week at column 46 and six of upcoming due
        // load after it; a calendar year runs from the Sunday on or
        // before Jan 1 to the Saturday on or after Dec 31, 53 or 54. ----
        const WEEKS: i64 = 53;
        const PAST_WEEKS: i64 = 46;
        let first_review_idx = per_day.keys().min().copied();
        let current_year = i32::from(civil_of(today_idx).year());
        let first_year =
            first_review_idx.map_or(current_year, |idx| i32::from(civil_of(idx).year()));
        let heat_years: Vec<i32> = if first_year < current_year {
            (first_year..=current_year).collect()
        } else {
            Vec::new()
        };
        let heat_year = year.filter(|y| (first_year..current_year).contains(y));
        // Epoch day 0 (1970-01-01) was a Thursday; +4 makes Sunday zero.
        let sunday_on_or_before = |idx: i64| idx - (idx + 4).rem_euclid(7);
        let (first_idx, last_idx, stat_lo, stat_hi) = match heat_year {
            None => {
                let first = sunday_on_or_before(today_idx) - PAST_WEEKS * 7;
                (first, first + WEEKS * 7 - 1, today_idx - 364, today_idx)
            }
            Some(y) => {
                let jan1 = idx_of(jiff::civil::date(y as i16, 1, 1));
                let dec31 = idx_of(jiff::civil::date(y as i16, 12, 31));
                (
                    sunday_on_or_before(jan1),
                    sunday_on_or_before(dec31) + 6,
                    jan1,
                    dec31,
                )
            }
        };
        let weeks = (last_idx - first_idx + 1) / 7;

        // Upcoming load fills the columns after today, in the trailing view.
        let mut future_map: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
        if heat_year.is_none() {
            let horizon_ms = now_ms + (last_idx - today_idx + 2) * DAY;
            for due in self.store.due_between(user, now_ms, horizon_ms)? {
                let idx = day_of(due);
                if idx > today_idx {
                    *future_map.entry(idx).or_default() += 1;
                }
            }
        }
        let past_max = per_day
            .iter()
            .filter(|(k, _)| (first_idx..=last_idx.min(today_idx)).contains(k))
            .map(|(_, v)| *v)
            .max()
            .unwrap_or(0)
            .max(1);
        let future_max = future_map.values().copied().max().unwrap_or(0).max(1);
        let level = |count: u32, max: u32| -> u8 {
            if count == 0 {
                0
            } else {
                (count * 4).div_ceil(max).clamp(1, 4) as u8
            }
        };

        let mut heat_weeks = Vec::with_capacity(weeks as usize);
        let mut heat_months: Vec<MonthSpan> = Vec::new();
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for week in 0..weeks {
            let mut cells = Vec::with_capacity(7);
            let col_start_idx = first_idx + week * 7;
            let col_month = MONTHS[(civil_of(col_start_idx).month() as usize - 1) % 12];
            match heat_months.last_mut() {
                Some(span) if span.label == col_month => span.weeks += 1,
                _ => heat_months.push(MonthSpan {
                    label: col_month.to_string(),
                    weeks: 1,
                }),
            }
            for dow in 0..7 {
                let cell_idx = col_start_idx + dow;
                let future = heat_year.is_none() && cell_idx > today_idx;
                let count = if future {
                    future_map.get(&cell_idx).copied().unwrap_or(0)
                } else {
                    per_day.get(&cell_idx).copied().unwrap_or(0)
                };
                cells.push(HeatCell {
                    count,
                    level: level(count, if future { future_max } else { past_max }),
                    label: label_of(cell_idx),
                    future,
                    today: heat_year.is_none() && cell_idx == today_idx,
                });
            }
            heat_weeks.push(cells);
        }
        // Blank cramped month labels (a month spanning <3 columns).
        for span in &mut heat_months {
            if span.weeks < 3 {
                span.label = String::new();
            }
        }

        // The legend's averages cover the window the heading names,
        // counted from the first review when it falls inside it.
        let lo = first_review_idx.map_or(stat_lo, |first| stat_lo.max(first));
        let denom = (stat_hi - lo + 1).max(1) as u32;
        let (window_reviews, window_days) = per_day
            .iter()
            .filter(|(k, _)| (lo..=stat_hi).contains(k))
            .fold((0u32, 0u32), |(r, d), (_, v)| (r + v, d + 1));
        let (active_cards, mature_cards) = self.store.card_totals(user)?;

        Ok(StatsData {
            reviews_30d,
            retention_pct: (review_reviews > 0).then(|| review_recalled * 100 / review_reviews),
            streak,
            longest_streak,
            daily_avg: window_reviews / denom,
            days_learned_pct: (window_days * 100 / denom).min(100),
            total_reviews,
            active_cards,
            mature_cards,
            days,
            upcoming,
            heat_weeks,
            heat_months,
            heat_years,
            heat_year,
        })
    }

    // ---- Card management ----

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

    /// Decks with today's *budgeted* due/new counts (what studying that
    /// deck would actually serve), not raw totals.
    pub fn list_decks(&self, user: UserId, now_ms: i64) -> Result<Vec<DeckSummary>> {
        let settings = self.store.get_settings(user)?;
        let budgets = self.budgets(user, &settings, now_ms)?;
        let mut decks = self.store.list_decks(user, now_ms)?;
        for d in &mut decks {
            let own =
                budgets
                    .per_deck
                    .get(&d.id)
                    .copied()
                    .unwrap_or(flash_core::queue::DeckBudget {
                        new: u32::MAX,
                        reviews: u32::MAX,
                    });
            d.new_count = d.new_count.min(own.new).min(budgets.new_total);
            d.due_count = d.due_count.min(own.reviews).min(budgets.review_total);
        }
        Ok(decks)
    }

    /// One of the user's decks by id, with today's budgeted counts.
    pub fn deck_by_id(&self, user: UserId, deck: DeckId, now_ms: i64) -> Result<DeckSummary> {
        self.list_decks(user, now_ms)?
            .into_iter()
            .find(|d| d.id == deck)
            .ok_or(ServiceError::Store(StoreError::NotFound("deck")))
    }

    /// The deck settings page in one read.
    pub fn deck_detail(&self, user: UserId, deck: DeckId, now_ms: i64) -> Result<DeckDetail> {
        let summary = self.deck_by_id(user, deck, now_ms)?;
        let settings = self.store.get_settings(user)?;
        Ok(DeckDetail {
            limits: self.store.get_deck_limits(user, deck)?,
            default_new: settings.new_per_day,
            default_reviews: settings.reviews_per_day,
            boost_today: self.boost_today(user, Some(deck), now_ms)?,
            card_count: self.store.count_cards(user, Some(deck), None)?,
            deck: summary,
        })
    }

    /// The admission gate plus text validation shared by every
    /// interactive add: all-or-nothing at the free-plan cap (the store
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
                    .map_err(ServiceError::Invalid)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((cap, validated))
    }

    /// Batch create into a deck named `deck_name` (created if missing).
    /// The MCP tools' path, where decks are spoken, not clicked.
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
        self.deck_by_id(user, deck, now_ms)?;
        let (cap, validated) = self.admitted_cards(user, cards, now_ms)?;
        self.store
            .create_cards(user, deck, &validated, cap, now_ms)
            .map_err(|e| self.cap_error(e))
    }

    /// One page of a deck's cards, optionally filtered by a search
    /// string; a deck that isn't the user's is not found.
    pub fn deck_cards_page(
        &self,
        user: UserId,
        deck: DeckId,
        search: Option<&str>,
        page: u32,
        per_page: u32,
        now_ms: i64,
    ) -> Result<CardsPage> {
        self.deck_by_id(user, deck, now_ms)?;
        let search = bounded_search(search)?;
        let per_page = per_page.clamp(1, 500);
        let (rows, total) = self.list_cards_page(user, deck, search, page.max(1), per_page)?;
        let pages = total.div_ceil(per_page).max(1);
        Ok(CardsPage {
            rows,
            page: page.max(1).min(pages),
            pages,
            total,
        })
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
    /// their own routes (copying a shared deck).
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
        let mut created = 0usize;
        for (deck_name, batch) in by_deck {
            let validated = batch
                .iter()
                .map(|row| {
                    validate_card_text(&row.front, &row.back)
                        .map(|text| (text, row.tags.clone()))
                        .map_err(ServiceError::Invalid)
                })
                .collect::<Result<Vec<_>>>()?;
            let deck = match self.store.find_deck_by_name(user, &deck_name)? {
                Some(id) => id,
                None => self.store.create_deck(user, &deck_name, "", now_ms)?,
            };
            // The batch was admitted as a whole above, so no per-deck cap.
            let ids = self
                .store
                .create_cards(user, deck, &validated, None, now_ms)?;
            created += ids.len();
            // Rich content, typing answers, cloze sources, suspension, and
            // media links (everything beyond the plain-text card).
            let mut extras_batch: Vec<(CardId, flash_store::CardExtras)> = Vec::new();
            let mut suspend_batch: Vec<CardId> = Vec::new();
            for (id, row) in ids.iter().zip(&batch) {
                if let Some(anki) = row.anki_card_id {
                    card_of.insert(anki, *id);
                }
                if row.suspended {
                    suspend_batch.push(*id);
                }
                let extras = flash_store::CardExtras {
                    front_html: row.front_html.clone(),
                    back_html: row.back_html.clone(),
                    cloze_text: row.cloze_text.clone(),
                    cloze_index: row.cloze_index,
                    type_answer: row.type_answer.clone(),
                };
                if !extras.is_empty() {
                    extras_batch.push((*id, extras));
                }
                if let Some(map) = media_ids {
                    for media_ref in &row.media {
                        if let Some(mid) = map.get(&media_ref.filename) {
                            let _ = self.store.link_card_media(user, *id, *mid);
                        }
                    }
                }
            }
            if !extras_batch.is_empty() {
                self.store.set_card_extras(user, &extras_batch)?;
            }
            if !suspend_batch.is_empty() {
                self.store.suspend_cards(user, &suspend_batch, now_ms)?;
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

    // ---- admin ----

    pub fn admin_users(&self, now_ms: i64) -> Result<Vec<flash_store::AdminUserRow>> {
        Ok(self.store.list_users_admin(now_ms)?)
    }

    /// A one-time member enrollment link, valid for seven days; returns
    /// the raw token (only its hash is stored) and the expiry.
    pub fn create_member_invite(
        &self,
        by: UserId,
        display_name: &str,
        now_ms: i64,
    ) -> Result<(String, i64)> {
        const INVITE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
        let name = display_name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ServiceError::Invalid("name required".into()));
        }
        let token = crate::auth::new_token();
        let expires = now_ms + INVITE_TTL_MS;
        self.store.create_invite(
            &crate::auth::hash_token(&token),
            name,
            "member",
            Some(by),
            expires,
            now_ms,
        )?;
        Ok((token, expires))
    }

    // ---- account ----

    pub fn account_overview(&self, user: UserId) -> Result<AccountOverview> {
        let row = self.store.account_row(user)?;
        let settings = self.store.get_settings(user)?;
        Ok(AccountOverview {
            id: user,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            theme: row.theme,
            grading_mode: settings.grading_mode,
            has_password: self.store.get_password_hash(user)?.is_some(),
            passkeys: self.store.passkeys_for_user(user)?.len() as u32,
        })
    }

    pub fn settings_overview(&self, user: UserId, _now_ms: i64) -> Result<SettingsOverview> {
        let row = self.store.account_row(user)?;
        let settings = self.store.get_settings(user)?;
        Ok(SettingsOverview {
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            theme: row.theme,
            grading_mode: settings.grading_mode,
            new_per_day: settings.new_per_day,
            reviews_per_day: settings.reviews_per_day,
            timezone: settings.timezone,
            day_cutoff_hour: settings.day_cutoff_hour,
            desired_retention: settings.desired_retention,
            has_password: self.store.get_password_hash(user)?.is_some(),
            passkeys: self
                .store
                .passkeys_for_user(user)?
                .into_iter()
                .map(|p| PasskeyInfo {
                    id: p.id,
                    label: p.label,
                    created_at: p.created_at,
                    last_used_at: p.last_used_at,
                })
                .collect(),
            card_count: self.store.count_cards(user, None, None)?,
            voice: self.store.get_voice_settings(user)?,
        })
    }

    /// The one read of an account's voice settings. A saved voice the
    /// table no longer lists (a language taken out of the picker) reads
    /// as the default: nothing would ever be made in it, and a study
    /// waiting for its clips would wait forever.
    pub fn voice_settings(&self, user: UserId) -> Result<VoiceSettings> {
        let mut settings = self.store.get_voice_settings(user)?;
        if crate::tts::voices::find(&settings.voice_id).is_none() {
            settings.voice_id = flash_core::settings::DEFAULT_VOICE.to_string();
        }
        Ok(settings)
    }

    /// Validated here so no surface stores a voice the sidecar lacks or
    /// a speed the players cannot honour. `None` fields stay as they are.
    pub fn set_voice_settings(&self, user: UserId, patch: &VoiceSettingsPatch) -> Result<()> {
        if let Some(voice) = &patch.voice_id {
            if crate::tts::voices::find(voice).is_none() {
                return Err(ServiceError::Invalid(format!("unknown voice {voice:?}")));
            }
        }
        if patch.speech_rate.is_some_and(|r| {
            !(flash_core::settings::SPEECH_RATE_MIN..=flash_core::settings::SPEECH_RATE_MAX)
                .contains(&r)
        }) {
            return Err(ServiceError::Invalid(format!(
                "speech rate must be between {} and {}",
                flash_core::settings::SPEECH_RATE_MIN,
                flash_core::settings::SPEECH_RATE_MAX
            )));
        }
        if patch.is_empty() {
            return Ok(());
        }
        Ok(self.store.set_voice_settings(user, patch)?)
    }

    /// The apps and connectors holding live grants for the user, most
    /// recently used first.
    pub fn connected_apps(&self, user: UserId, now_ms: i64) -> Result<Vec<GrantRow>> {
        Ok(self.store.list_grants_for_user(user, now_ms)?)
    }

    /// Revokes every live grant one client holds for the user. Idempotent:
    /// a client with nothing left to revoke is already disconnected.
    pub fn disconnect_app(&self, user: UserId, client_id: &str, now_ms: i64) -> Result<()> {
        self.store.revoke_grant(client_id, user, now_ms)?;
        Ok(())
    }

    /// Same bounds as signup's name field.
    pub fn set_display_name(&self, user: UserId, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ServiceError::Invalid(
                "a name is required (up to 80 characters)".into(),
            ));
        }
        Ok(self.store.set_display_name(user, name)?)
    }

    pub fn set_grading_mode(&self, user: UserId, mode: &str) -> Result<()> {
        let Some(mode) = GradingMode::from_str(mode) else {
            return Err(ServiceError::Invalid(format!("unknown mode {mode:?}")));
        };
        Ok(self.store.set_grading_mode(user, mode)?)
    }

    /// Validated here so no surface can store a zone jiff can't resolve
    /// (the day cutoff and the reminder scheduler both depend on it).
    pub fn set_study_settings(&self, user: UserId, patch: &StudySettingsPatch) -> Result<()> {
        if let Some(tz) = &patch.timezone {
            if TimeZone::get(tz).is_err() {
                return Err(ServiceError::Invalid(format!("unknown timezone {tz:?}")));
            }
        }
        if patch.day_cutoff_hour.is_some_and(|h| h > 23) {
            return Err(ServiceError::Invalid(
                "day cutoff must be an hour from 0 to 23".into(),
            ));
        }
        if patch
            .desired_retention
            .is_some_and(|r| !(0.70..=0.99).contains(&r))
        {
            return Err(ServiceError::Invalid(
                "desired retention must be between 0.70 and 0.99".into(),
            ));
        }
        Ok(self.store.set_study_settings(
            user,
            patch.timezone.as_deref(),
            patch.day_cutoff_hour,
            patch.desired_retention,
        )?)
    }

    /// One of the five UI themes. The web also keeps a cookie copy so a
    /// page can render before any request reaches the store.
    pub fn set_theme(&self, user: UserId, theme: &str) -> Result<()> {
        if crate::auth::Theme::from_str(theme).is_none() {
            return Err(ServiceError::Invalid(format!("unknown theme {theme:?}")));
        }
        Ok(self.store.set_theme(user, theme)?)
    }

    /// Every live card with deck and tags: the export surface. Never
    /// gated by any policy.
    pub fn export_cards(&self, user: UserId) -> Result<Vec<flash_store::export::ExportCard>> {
        Ok(self.store.export_cards(user)?)
    }

    pub fn update_card(
        &self,
        user: UserId,
        card: CardId,
        front: &str,
        back: &str,
        now_ms: i64,
    ) -> Result<()> {
        let text = validate_card_text(front, back).map_err(ServiceError::Invalid)?;
        Ok(self.store.update_card(user, card, &text, now_ms)?)
    }

    pub fn delete_card(&self, user: UserId, card: CardId, now_ms: i64) -> Result<()> {
        Ok(self.store.delete_card(user, card, now_ms)?)
    }

    // ---- notes (the advanced editor) ----

    /// Sanitizes the editor's fields, expands them into cards, and checks
    /// every media reference resolves to the user's own upload.
    fn prepare_note(&self, user: UserId, input: &NoteInput) -> Result<PreparedNote> {
        if input.front_html.len() > MAX_FIELD_HTML || input.back_html.len() > MAX_FIELD_HTML {
            return Err(ServiceError::Invalid(format!(
                "a field exceeds {MAX_FIELD_HTML} bytes of HTML"
            )));
        }
        let front_html = richtext::sanitize_with_media(&input.front_html).trimmed();
        let back_html = richtext::sanitize_with_media(&input.back_html).trimmed();
        let generated = generate_cards(input.note_type, &front_html, &back_html)
            .map_err(ServiceError::Invalid)?;
        let mut media = richtext::media_ids(&front_html);
        media.extend(richtext::media_ids(&back_html));
        for id in media {
            if self.store.get_media(user, MediaId(id))?.is_none() {
                return Err(ServiceError::Invalid(format!(
                    "media {id} is not one of your uploads"
                )));
            }
        }
        let tags = normalize_tags(&input.tags);
        Ok(PreparedNote {
            front_html,
            back_html,
            generated,
            tags,
        })
    }

    /// Note-editor gate: `inserts` new cards go through `admit_cards`. An
    /// edit that adds none is never blocked (an over-cap account can
    /// still fix its existing cards), which is the policy's contract for
    /// an admission of zero: it only reports the cap the store re-checks.
    fn note_cap(&self, user: UserId, inserts: u32, now_ms: i64) -> Result<Option<u32>> {
        self.admit_cards(user, inserts, AddSource::Interactive, now_ms)
    }

    /// Creates a note (and its cards) in `deck` from the advanced editor.
    pub fn create_note(
        &self,
        user: UserId,
        deck: DeckId,
        input: &NoteInput,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        self.create_note_from(user, deck, input, None, now_ms)
    }

    /// `create_note` with the note's lineage: the generation job it was
    /// kept from, so a study turn can find the reading it came from.
    /// Same gate, same sanitizing, same cards; only the column differs.
    pub fn create_note_from(
        &self,
        user: UserId,
        deck: DeckId,
        input: &NoteInput,
        ai_job_id: Option<AiJobId>,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        let p = self.prepare_note(user, input)?;
        let cap = self.note_cap(user, p.generated.len() as u32, now_ms)?;
        self.store
            .create_note(user, deck, &p.save(input.note_type, cap, ai_job_id, now_ms))
            .map_err(|e| self.cap_error(e))
    }

    /// Saves the editor over an existing card: its note is rewritten and
    /// regenerated in place; a note-less card is adopted into a fresh
    /// note first (an imported cloze card brings its siblings).
    pub fn save_card_editor(
        &self,
        user: UserId,
        card: CardId,
        input: &NoteInput,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        let p = self.prepare_note(user, input)?;
        let note = match self.store.note_for_card(user, card)? {
            Some(row) => row,
            None => {
                let id = self.store.adopt_card_into_note(
                    user,
                    card,
                    input.note_type,
                    &p.front_html,
                    &p.back_html,
                    now_ms,
                )?;
                self.store
                    .get_note(user, id)?
                    .ok_or(ServiceError::Store(StoreError::NotFound("note")))?
            }
        };
        let inserts = p
            .generated
            .iter()
            .filter(|g| !note.cards.iter().any(|(ord, _)| *ord == g.ord))
            .count() as u32;
        let cap = self.note_cap(user, inserts, now_ms)?;
        let ids = self
            .store
            .update_note(user, note.id, &p.save(input.note_type, cap, None, now_ms))
            .map_err(|e| self.cap_error(e))?;
        Ok((note.id, ids))
    }

    /// What the editor opens with for `card`: its note's source fields, or
    /// a synthesized note for a standalone card (imported cloze/typed cards
    /// reconstruct their type from the rich columns).
    pub fn editor_seed(&self, user: UserId, card: CardId) -> Result<EditorSeed> {
        let row = self
            .store
            .get_card(user, card)?
            .ok_or(ServiceError::Store(StoreError::NotFound("card")))?;
        if let Some(note) = self.store.note_for_card(user, card)? {
            return Ok(EditorSeed {
                card_id: card,
                deck_id: note.deck_id,
                note_id: Some(note.id),
                note_type: note.note_type,
                front_html: note.front_html,
                back_html: note.back_html,
                tags: note.tags,
                sibling_count: note.cards.len(),
            });
        }
        let cloze = self.store.cloze_source(user, card)?;
        let (note_type, front_html, back_html) = match cloze {
            Some(source) => {
                let (text, extra) = source.split_once('\u{1f}').unwrap_or((&source, ""));
                (NoteType::Cloze, text_to_html(text), text_to_html(extra))
            }
            None => {
                let kind = if row.type_answer.is_some() {
                    NoteType::BasicTyped
                } else {
                    NoteType::Basic
                };
                (
                    kind,
                    row.front_html
                        .clone()
                        .unwrap_or_else(|| text_to_html(&row.front)),
                    row.back_html
                        .clone()
                        .unwrap_or_else(|| text_to_html(&row.back)),
                )
            }
        };
        Ok(EditorSeed {
            card_id: card,
            deck_id: row.deck_id,
            note_id: None,
            note_type,
            front_html,
            back_html,
            tags: row.tags,
            sibling_count: 1,
        })
    }

    /// One page of a deck's cards (1-based `page`) plus the total match
    /// count, for the paginated deck view.
    pub fn list_cards_page(
        &self,
        user: UserId,
        deck: DeckId,
        search: Option<&str>,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<CardRow>, u32)> {
        let per_page = per_page.clamp(1, 500);
        let total = self.store.count_cards(user, Some(deck), search)?;
        let pages = total.div_ceil(per_page).max(1);
        let page = page.clamp(1, pages);
        let rows = self.store.list_cards_page(
            user,
            Some(deck),
            search,
            per_page,
            (page - 1) * per_page,
        )?;
        Ok((rows, total))
    }

    /// Hard-deletes a deck and all its cards; the caller removes the
    /// returned orphan blobs from the media store.
    pub fn delete_deck(&self, user: UserId, deck: DeckId) -> Result<flash_store::DeletedDeck> {
        Ok(self.store.delete_deck(user, deck)?)
    }

    pub fn list_cards(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CardRow>> {
        let search = bounded_search(search)?;
        Ok(self.store.list_cards(user, deck, search, limit.min(500))?)
    }

    // ---- voice ----

    /// What the grader is told about a card: its plain sides, its note
    /// type, and (when the card was generated) the reading of the
    /// source it came from, cut to `VOICE_CONTEXT_WORDS`. Reading the
    /// source for a study turn counts as using it, so housekeeping
    /// leaves it alone.
    pub fn voice_context(&self, user: UserId, card: CardId, now_ms: i64) -> Result<CardContext> {
        let row = self
            .store
            .get_card(user, card)?
            .ok_or(StoreError::NotFound("card"))?;
        let note = self.store.note_for_card(user, card)?;
        let note_type = match &note {
            Some(note) => note.note_type,
            None if row.cloze_index.is_some() => NoteType::Cloze,
            None if row.type_answer.is_some() => NoteType::BasicTyped,
            None => NoteType::Basic,
        };
        let extraction = match &note {
            Some(note) => self.store.ai_extraction_for_note(user, note.id)?,
            None => None,
        };
        let extraction = match extraction {
            Some(extraction) => {
                self.store
                    .touch_ai_extraction(user, extraction.id, now_ms)?;
                Some(first_words(&extraction.text, VOICE_CONTEXT_WORDS))
            }
            None => None,
        };
        let back = plain_side(&row.back, row.back_html.as_deref());
        Ok(CardContext {
            front: plain_side(&row.front, row.front_html.as_deref()),
            spelled: crate::voice_hints::spelled_forms(&back),
            back,
            note_type,
            extraction,
        })
    }

    /// One spoken turn about a card in an open session of the user's:
    /// what the student said, where the conversation is, and what the
    /// model made of it. An answer comes back graded with the reveal to
    /// show next; a question comes back answered from the card; a grade
    /// the student named comes back as the rating; the rest are bare
    /// intents the client acts on. No review is written: the client
    /// posts it after its undo window, through the one path every review
    /// takes. The call is blocking (the provider is) and runs on a
    /// blocking thread; the ledger gets a row whether the model answered
    /// or not, as `clarify` when the student asked a question and as
    /// `grade` otherwise, the two steps voice has always been recorded
    /// under.
    ///
    /// Eight parameters: the four things a turn is about (session, card,
    /// state, what was said) beside the caller, the runtime and the
    /// clock. A struct for them would exist for this one call.
    #[allow(clippy::too_many_arguments)]
    pub fn spoken_turn(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        state: DialogState,
        spoken: &SpokenPayload,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> std::result::Result<SpokenOutcome, VoiceError> {
        self.require_open_session(user, session)?;
        if let AiAdmission::Denied { code, message } =
            self.ai_policy.admit_voice(&self.store, user, now_ms)?
        {
            return Err(VoiceError::Service(ServiceError::AiDenied {
                code,
                message,
            }));
        }
        let transcript = match spoken {
            SpokenPayload::Transcript(text) => Some(spoken_input(text)?),
            SpokenPayload::Audio { .. } => None,
        };
        let input = match &transcript {
            Some(text) => SpokenInput::Transcript(text),
            None => spoken.as_input(),
        };
        let context = self.voice_context(user, card, now_ms)?;
        let started = std::time::Instant::now();
        let reply = ai.provider.turn(&context, state, input);
        let step = match &reply {
            Ok(turn) if turn.value.intent == SpokenIntent::Question => AiCallStep::Clarify,
            _ => AiCallStep::Grade,
        };
        self.record_voice_call(user, step, spoken.kind(), ai, &reply, started, now_ms)?;
        let mut turn = reply.map_err(VoiceError::Model)?.value;
        // Held to the state whatever the provider: nothing is undone
        // before a verdict, nothing answered after one.
        if !state.allows(turn.intent) {
            turn = SpokenTurn::bare(SpokenIntent::Unclear);
        }
        Ok(match turn.intent {
            SpokenIntent::Answer => SpokenOutcome::Graded(Box::new(self.graded(user, card, turn)?)),
            SpokenIntent::Question => SpokenOutcome::Answered(SpokenClarify {
                answer: turn.answer,
                spoken_answer: turn.spoken_answer,
                clip_token: None,
            }),
            SpokenIntent::Grade => SpokenOutcome::Rated(rating_of(turn.rating)?),
            SpokenIntent::Repeat => SpokenOutcome::Repeat,
            SpokenIntent::Skip => SpokenOutcome::Skip,
            SpokenIntent::Undo => SpokenOutcome::Undo,
            SpokenIntent::Unclear => SpokenOutcome::Unclear,
        })
    }

    /// The verdict on an answer, with the card's reveal.
    fn graded(&self, user: UserId, card: CardId, turn: SpokenTurn) -> Result<SpokenGrade> {
        Ok(SpokenGrade {
            rating: rating_of(turn.rating)?,
            feedback: turn.feedback,
            spoken_feedback: turn.spoken_feedback,
            reveal: self.reveal_card(user, card, None)?,
        })
    }

    /// The session must be the user's (not found otherwise) and still
    /// open: a grade for a session that was ended is a stale client.
    fn require_open_session(&self, user: UserId, session: SessionId) -> Result<()> {
        if !self.store.session_open(user, session)? {
            return Err(ServiceError::Invalid(
                "that study session has ended; start a new one".into(),
            ));
        }
        Ok(())
    }

    /// One ledger row for a spoken turn, answered or not, under the
    /// grader model and priced as it lands (`ai::ledger`).
    #[allow(clippy::too_many_arguments)]
    fn record_voice_call<T>(
        &self,
        user: UserId,
        step: AiCallStep,
        kind: AiSourceKind,
        ai: &AiRuntime,
        reply: &std::result::Result<Reply<T>, LlmError>,
        started: std::time::Instant,
        started_at: i64,
    ) -> Result<()> {
        let (_, grader_model) = ai.provider.model_names();
        crate::ai::ledger::record(
            &self.store,
            self.ai_policy.as_ref(),
            user,
            &crate::ai::ledger::Call {
                job: None,
                step,
                model: &grader_model,
                kind,
                started,
                started_at,
            },
            reply,
        )?;
        Ok(())
    }

    // ---- card generation ----
    //
    // A source is what the person handed over; a job is one pass of the
    // model over it; drafts are what the pass wrote, awaiting keep or
    // discard. A regenerate or an ask-for-more is a child job: its own
    // row for progress, its parent's drafts for the result, so one
    // review page follows a source through any number of passes. The
    // runner (`ai::runner`) does the model work; everything here is the
    // bookkeeping around it, and every id is the user's own or not found.

    /// A pasted passage, or the text of a text file, as a source. The
    /// text is the blob: it stays in the row, hashed like an upload so
    /// a second paste of the same passage finds the same cached
    /// reading. `filename` is empty for a paste.
    pub fn ai_source_from_text(
        &self,
        user: UserId,
        text: &str,
        filename: &str,
        mime: &str,
        now_ms: i64,
    ) -> Result<AiSourceId> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ServiceError::Invalid("Paste some text first.".into()));
        }
        if text.len() > crate::bounds::AI_TEXT_SOURCE {
            return Err(ServiceError::Invalid(format!(
                "That text is longer than {} bytes.",
                crate::bounds::AI_TEXT_SOURCE
            )));
        }
        self.require_unattached_room(user)?;
        let sha256 = sha256_hex(text.as_bytes());
        let words = count_u32(text.split_whitespace().count());
        Ok(self.store.create_ai_source(
            user,
            &AiSourceNew {
                kind: AiSourceKind::Text,
                sha256: &sha256,
                mime,
                size: text.len() as u64,
                filename,
                url: None,
                text: Some(text),
                pages: None,
                duration_s: None,
                est_words: Some(words),
                image_count: None,
                origin_mime: None,
                now_ms,
            },
        )?)
    }

    /// One uploaded file as a source: a PDF, a photo, a clip, a
    /// recording, a text file or an office document, decided from its
    /// bytes and its name. A text file becomes a text source; a
    /// document goes through the operator's converter and becomes a
    /// PDF that remembers what it was. The blob goes to the media store
    /// under its hash with no media row (a source is not a card's
    /// attachment), and the row records what the preflight will ask:
    /// the pages and words of a PDF, the length of a recording.
    pub fn ai_source_from_upload(
        &self,
        user: UserId,
        filename: &str,
        bytes: &[u8],
        media: &dyn MediaStore,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiSourceId> {
        use crate::ai::sources::UploadKind;
        let file = sources::classify_upload(filename, bytes).map_err(ServiceError::Invalid)?;
        let kind = file.upload.stored();
        require_ai_input(ai, kind)?;
        check_upload_size(bytes.len() as u64)?;
        match file.upload {
            UploadKind::TextFile => {
                let text = sources::decode_text(bytes, file.mime).map_err(ServiceError::Invalid)?;
                return self.ai_source_from_text(user, &text, &file.filename, file.mime, now_ms);
            }
            UploadKind::Document => {
                let Some(converter) = ai.converter.as_ref().filter(|_| ai.converts_documents())
                else {
                    return Err(ServiceError::AiDenied {
                        code: "input_disabled",
                        message: sources::NO_CONVERTER.to_string(),
                    });
                };
                self.require_unattached_room(user)?;
                let pdf = converter
                    .to_pdf(
                        &ai.convert_dir,
                        &file.extension,
                        bytes,
                        flash_store::media::MAX_FILE_BYTES,
                    )
                    .map_err(|e| {
                        tracing::warn!(file = ?file.filename, "document conversion: {e}");
                        ServiceError::Invalid(e.user_message().to_string())
                    })?;
                let facts = self.pdf_facts(ai, &pdf)?;
                return self.store_source(
                    user,
                    media,
                    kind,
                    "application/pdf",
                    &pdf,
                    &file.filename,
                    Some(file.mime),
                    facts,
                    now_ms,
                );
            }
            UploadKind::Pdf | UploadKind::Image | UploadKind::Video | UploadKind::Audio => {}
        }
        require_ai_mime(ai, kind, file.mime, &file.extension)?;
        let mut facts = SourceFacts::default();
        match kind {
            AiSourceKind::Pdf => facts = self.pdf_facts(ai, bytes)?,
            AiSourceKind::Image => facts.images = Some(1),
            AiSourceKind::Video | AiSourceKind::Audio => {
                facts.duration_s = sources::mp4_duration_seconds(bytes);
            }
            AiSourceKind::Text | AiSourceKind::Youtube => {}
        }
        self.require_unattached_room(user)?;
        self.store_source(
            user,
            media,
            kind,
            file.mime,
            bytes,
            &file.filename,
            None,
            facts,
            now_ms,
        )
    }

    /// The blob under its hash and the row that names it.
    #[allow(clippy::too_many_arguments)]
    fn store_source(
        &self,
        user: UserId,
        media: &dyn MediaStore,
        kind: AiSourceKind,
        mime: &str,
        bytes: &[u8],
        filename: &str,
        origin_mime: Option<&str>,
        facts: SourceFacts,
        now_ms: i64,
    ) -> Result<AiSourceId> {
        let sha256 = sha256_hex(bytes);
        self.store_source_blob(media, &sha256, mime, bytes)?;
        Ok(self.store.create_ai_source(
            user,
            &AiSourceNew {
                kind,
                sha256: &sha256,
                mime,
                size: bytes.len() as u64,
                filename,
                url: None,
                text: None,
                pages: facts.pages,
                duration_s: facts.duration_s,
                est_words: facts.words,
                image_count: facts.images,
                origin_mime,
                now_ms,
            },
        )?)
    }

    /// A video link as a source. Only the one canonical form is kept,
    /// and only for the hosts `ai::sources` allows; its length is not
    /// known until the model has watched it, so the call is metered
    /// from what was consumed.
    pub fn ai_source_from_youtube(
        &self,
        user: UserId,
        url: &str,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiSourceId> {
        require_ai_input(ai, AiSourceKind::Youtube)?;
        let url = sources::canonical_youtube_url(url).map_err(ServiceError::Invalid)?;
        self.require_unattached_room(user)?;
        let sha256 = sha256_hex(url.as_bytes());
        Ok(self.store.create_ai_source(
            user,
            &AiSourceNew {
                kind: AiSourceKind::Youtube,
                sha256: &sha256,
                mime: "",
                size: 0,
                filename: "",
                url: Some(&url),
                text: None,
                pages: None,
                duration_s: None,
                est_words: None,
                image_count: None,
                origin_mime: None,
                now_ms,
            },
        )?)
    }

    /// Removes a source the person took back before generating from
    /// it, and its blob when nothing else holds it. A source a job has
    /// read stays with the job.
    pub fn ai_delete_source(
        &self,
        user: UserId,
        source: AiSourceId,
        media: &dyn MediaStore,
    ) -> Result<()> {
        if let Some(sha) = self.store.delete_ai_source(user, source)? {
            if let Err(e) = media.delete(&sha) {
                tracing::warn!(sha = ?sha, "source blob: {e}");
            }
        }
        Ok(())
    }

    /// The user's sources no job has read yet, newest first, each with
    /// its preflight: what a composer would show if it were reopened.
    pub fn ai_unattached_sources(
        &self,
        user: UserId,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<Vec<(AiSourceRow, AiPreflight)>> {
        let rows = self
            .store
            .list_unattached_ai_sources(user, crate::bounds::AI_UNATTACHED_SOURCES_PER_USER)?;
        rows.into_iter()
            .map(|row| {
                let preflight = self.preflight_of(user, &row, ai, now_ms)?;
                Ok((row, preflight))
            })
            .collect()
    }

    /// What a generation from `source` would be, before it is asked
    /// for: the facts ingest recorded, whether this server and the
    /// account's plan would allow it, and why not. Reading the source
    /// counts as using it.
    pub fn ai_preflight(
        &self,
        user: UserId,
        source: AiSourceId,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiPreflight> {
        let row = self
            .store
            .get_ai_source(user, source)?
            .ok_or(StoreError::NotFound("ai source"))?;
        self.store.touch_ai_source(user, source, now_ms)?;
        self.preflight_of(user, &row, ai, now_ms)
    }

    /// What one generation over the whole set would be: every member's
    /// own preflight, the totals, and the verdict on the set. The first
    /// refusal wins, nearest the person's control first: a member this
    /// server or the plan will not read (and which one), then the caps
    /// over the totals, then the plan's answer to the set as a whole.
    /// Reading the set counts as using every member.
    pub fn ai_preflight_set(
        &self,
        user: UserId,
        ids: &[AiSourceId],
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiSetPreflight> {
        if ids.is_empty() {
            return Err(ServiceError::Invalid(
                "Add something for the model to read first.".into(),
            ));
        }
        if ids.len() > crate::bounds::AI_SOURCES_PER_JOB {
            return Err(ServiceError::Invalid(too_many_sources()));
        }
        for (i, id) in ids.iter().enumerate() {
            if ids[..i].contains(id) {
                return Err(ServiceError::Invalid(
                    "The same source is listed twice.".into(),
                ));
            }
        }
        let mut rows = Vec::with_capacity(ids.len());
        for id in ids {
            let row = self
                .store
                .get_ai_source(user, *id)?
                .ok_or(StoreError::NotFound("ai source"))?;
            self.store.touch_ai_source(user, *id, now_ms)?;
            rows.push(row);
        }
        let mut members = Vec::with_capacity(rows.len());
        let mut refused: Option<(&'static str, String, Option<AiSourceId>)> = None;
        let mut totals = AiSetTotals {
            sources: count_u32(rows.len()),
            ..AiSetTotals::default()
        };
        let mut inputs: Vec<AiInput> = Vec::new();
        let mut media_known = false;
        for row in &rows {
            let preflight = self.preflight_of(user, row, ai, now_ms)?;
            if refused.is_none() {
                if let (Some(code), Some(reason)) = (preflight.code, preflight.reason.clone()) {
                    refused = Some((code, reason, Some(row.id)));
                }
            }
            totals.pages += row.pages.unwrap_or(0);
            totals.est_words += row.est_words.unwrap_or(0);
            totals.image_count += row.image_count.unwrap_or(0);
            if let Some(seconds) = row.duration_s {
                totals.duration_s += seconds;
                media_known = true;
            }
            totals.bytes += row.size;
            if !inputs.contains(&row.kind) {
                inputs.push(row.kind);
            }
            members.push((row.id, preflight));
        }
        if refused.is_none() && rows.len() > 1 {
            if let Some((code, reason)) = set_cap_refusal(&totals) {
                refused = Some((code, reason, None));
            }
        }
        let usage = self.ai_policy.usage(&self.store, user, now_ms)?;
        if refused.is_none() {
            let request = AiRequest {
                inputs: inputs.clone(),
                media_seconds: media_known.then_some(totals.duration_s),
            };
            if let AiAdmission::Denied { code, message } =
                self.ai_policy
                    .admit_generation(&self.store, user, request, now_ms)?
            {
                refused = Some((code, message, None));
            }
        }
        let cost_class = AiSourceKind::costliest(inputs.iter().copied())
            .unwrap_or(AiSourceKind::Text)
            .as_str();
        let line = match members.as_slice() {
            [(_, one)] => facts_line(one),
            _ => set_facts_line(&totals),
        };
        let (code, reason, refused_source_id) = match refused {
            Some((code, reason, id)) => (Some(code), Some(reason), id),
            None => (None, None, None),
        };
        Ok(AiSetPreflight {
            sources: members,
            totals,
            inputs,
            cost_class,
            allowed: code.is_none(),
            code,
            reason,
            refused_source_id,
            line,
            usage,
        })
    }

    /// Queues a first pass over the set `ids`, in order, into `deck`.
    /// Everything the preflight would refuse is refused here too: the
    /// preflight is advice, this is the gate.
    pub fn ai_create_job(
        &self,
        user: UserId,
        ids: &[AiSourceId],
        deck: DeckId,
        instruction: &str,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiJobId> {
        let instruction = clean_instruction(instruction)?;
        self.store.get_deck_limits(user, deck)?;
        self.require_no_active_ai_job(user)?;
        let preflight = self.ai_preflight_set(user, ids, ai, now_ms)?;
        if let (Some(code), Some(reason)) = (preflight.code, preflight.reason) {
            return Err(refusal(code, reason));
        }
        Ok(self.store.create_ai_job(
            user,
            &AiJobNew {
                source_ids: ids,
                deck_id: Some(deck),
                extraction_id: None,
                op: AiJobOp::Generate,
                parent_job_id: None,
                instruction: &instruction,
                now_ms,
            },
        )?)
    }

    /// Whether the plan admits each kind this server reads, for a page
    /// that marks a kind as withheld before anything is uploaded: the
    /// policy's own code and sentence per refused kind.
    pub fn ai_kind_admissions(
        &self,
        user: UserId,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<Vec<KindAdmission>> {
        ai.inputs
            .iter()
            .map(|kind| {
                let request = AiRequest::single(*kind, None);
                let verdict =
                    match self
                        .ai_policy
                        .admit_generation(&self.store, user, request, now_ms)?
                    {
                        AiAdmission::Allowed => None,
                        AiAdmission::Denied { code, message } => Some((code, message)),
                    };
                Ok((*kind, verdict))
            })
            .collect()
    }

    /// Queues a pass that rewrites the open drafts of `job`'s review
    /// under `instruction`; the kept and discarded ones stay as they
    /// are. Returns the child job to poll. The older shape of
    /// `ai_edit_job`, kept for clients that still send it.
    pub fn ai_revise_job(
        &self,
        user: UserId,
        job: AiJobId,
        instruction: &str,
        now_ms: i64,
    ) -> Result<AiJobId> {
        Ok(self
            .ai_child_job(user, job, AiJobOp::Revise, instruction, now_ms)?
            .job_id)
    }

    /// Queues a pass that adds drafts beside the ones `job`'s review
    /// already has, the model seeing what is there so it does not
    /// repeat it. Returns the child job to poll.
    pub fn ai_more_job(
        &self,
        user: UserId,
        job: AiJobId,
        instruction: &str,
        now_ms: i64,
    ) -> Result<AiJobId> {
        Ok(self
            .ai_child_job(user, job, AiJobOp::More, instruction, now_ms)?
            .job_id)
    }

    /// One message of the review's thread: queues the pass that answers
    /// it, which may rewrite, add, drop, retype or retag the open
    /// drafts. Kept and discarded drafts are never touched. Returns the
    /// pass to poll and the turn the message became.
    pub fn ai_edit_job(
        &self,
        user: UserId,
        job: AiJobId,
        message: &str,
        now_ms: i64,
    ) -> Result<AiEditQueued> {
        if message.trim().is_empty() {
            return Err(ServiceError::Invalid("Say what you'd like changed.".into()));
        }
        let queued = self.ai_child_job(user, job, AiJobOp::Revise, message, now_ms)?;
        Ok(AiEditQueued {
            job_id: queued.job_id,
            turn_id: queued
                .turn_id
                .ok_or_else(|| ServiceError::Internal("edit queued without its turn".into()))?,
        })
    }

    /// The thread of `job`'s review (by the root or any pass of it):
    /// every turn in order, and the pass still running, when one is.
    pub fn ai_thread(&self, user: UserId, job: AiJobId) -> Result<AiThread> {
        let root = self.ai_root_job(user, job)?;
        let turns = self.store.list_ai_turns(user, root.id)?;
        let active_job_id = self
            .store
            .active_ai_job(user)?
            .filter(|active| active.parent_job_id == Some(root.id))
            .map(|active| active.id);
        Ok(AiThread {
            root,
            turns,
            active_job_id,
        })
    }

    /// A child pass needs no check against this server's inputs: the
    /// reading is cached, and writing cards from text is what every
    /// server does. Its instruction, when there is one, becomes the
    /// person's turn in the review's thread.
    fn ai_child_job(
        &self,
        user: UserId,
        job: AiJobId,
        op: AiJobOp,
        instruction: &str,
        now_ms: i64,
    ) -> Result<ChildQueued> {
        let instruction = clean_instruction(instruction)?;
        let root = self.ai_root_job(user, job)?;
        if root.state != AiJobState::Done {
            return Err(ServiceError::Invalid(
                "Wait for the current generation to finish.".into(),
            ));
        }
        let Some(extraction_id) = root.extraction_id else {
            return Err(ServiceError::Invalid(
                "This generation did not finish; start a new one.".into(),
            ));
        };
        self.require_no_active_ai_job(user)?;
        let mut ids = Vec::new();
        let mut inputs: Vec<AiInput> = Vec::new();
        for (id, row) in self.store.list_ai_job_sources(user, root.id)? {
            let row = row.ok_or_else(|| ServiceError::Invalid(SOURCE_EXPIRED.into()))?;
            if !inputs.contains(&row.kind) {
                inputs.push(row.kind);
            }
            ids.push(id);
        }
        if ids.is_empty() {
            return Err(ServiceError::Invalid(SOURCE_EXPIRED.into()));
        }
        // The reading is cached, so the model is only asked to write:
        // no media minutes, but still a generation the plan counts,
        // charged as the kinds the set was.
        let request = AiRequest {
            inputs,
            media_seconds: None,
        };
        if let AiAdmission::Denied { code, message } =
            self.ai_policy
                .admit_generation(&self.store, user, request, now_ms)?
        {
            return Err(ServiceError::AiDenied { code, message });
        }
        for id in &ids {
            self.store.touch_ai_source(user, *id, now_ms)?;
        }
        self.store
            .touch_ai_extraction(user, extraction_id, now_ms)?;
        let job_id = self.store.create_ai_job(
            user,
            &AiJobNew {
                source_ids: &ids,
                deck_id: root.deck_id,
                extraction_id: Some(extraction_id),
                op,
                parent_job_id: Some(root.id),
                instruction: &instruction,
                now_ms,
            },
        )?;
        let turn_id = if instruction.is_empty() {
            None
        } else {
            Some(self.store.add_ai_turn(
                user,
                &AiTurnNew {
                    job_id: root.id,
                    role: AiTurnRole::User,
                    text: &instruction,
                    child_job_id: Some(job_id),
                    now_ms,
                },
            )?)
        };
        Ok(ChildQueued { job_id, turn_id })
    }

    /// One job of the user's.
    pub fn ai_job(&self, user: UserId, job: AiJobId) -> Result<AiJobRow> {
        self.store
            .get_ai_job(user, job)?
            .ok_or(ServiceError::Store(StoreError::NotFound("ai job")))
    }

    /// A job with what a client shows beside it: the sources it reads
    /// (those housekeeping has not taken, and how many it has), and
    /// the standing of the drafts on its review.
    pub fn ai_job_detail(&self, user: UserId, job: AiJobId) -> Result<AiJobDetail> {
        let job = self.ai_job(user, job)?;
        let root = job.parent_job_id.unwrap_or(job.id);
        let mut sources = Vec::new();
        let mut sources_expired = 0u32;
        for (_, row) in self.store.list_ai_job_sources(user, job.id)? {
            match row {
                Some(row) => sources.push(row),
                None => sources_expired += 1,
            }
        }
        let counts = self.store.count_ai_drafts(user, root)?;
        Ok(AiJobDetail {
            job,
            root,
            sources,
            sources_expired,
            counts,
        })
    }

    /// The drafts of `job`'s review, every status, in order.
    pub fn ai_drafts(&self, user: UserId, job: AiJobId) -> Result<Vec<AiDraftRow>> {
        let root = self.ai_root_job(user, job)?;
        Ok(self.store.list_ai_drafts(user, root.id)?)
    }

    /// One draft of the user's.
    pub fn ai_draft(&self, user: UserId, draft: AiDraftId) -> Result<AiDraftRow> {
        self.store
            .get_ai_draft(user, draft)?
            .ok_or(ServiceError::Store(StoreError::NotFound("ai draft")))
    }

    /// The user's edit of a draft, through the editor's own sanitizing
    /// and validation, so what is kept later is exactly what a hand-made
    /// note would have been.
    #[allow(clippy::too_many_arguments)]
    pub fn ai_update_draft(
        &self,
        user: UserId,
        draft: AiDraftId,
        note_type: NoteType,
        front_html: &str,
        back_html: &str,
        tags: &[String],
        now_ms: i64,
    ) -> Result<()> {
        self.open_ai_draft(user, draft)?;
        let input = NoteInput {
            note_type,
            front_html: front_html.to_string(),
            back_html: back_html.to_string(),
            tags: tags.to_vec(),
        };
        let p = self.prepare_note(user, &input)?;
        self.store.update_ai_draft_fields(
            user,
            draft,
            note_type,
            &p.front_html,
            &p.back_html,
            &p.tags,
            now_ms,
        )?;
        Ok(())
    }

    /// Discards one open draft.
    pub fn ai_discard_draft(&self, user: UserId, draft: AiDraftId, now_ms: i64) -> Result<()> {
        self.open_ai_draft(user, draft)?;
        self.store
            .set_ai_draft_status(user, draft, AiDraftStatus::Discarded, None, now_ms)?;
        Ok(())
    }

    /// Discards every open draft of `job`'s review; returns how many.
    pub fn ai_discard_job(&self, user: UserId, job: AiJobId, now_ms: i64) -> Result<u32> {
        let root = self.ai_root_job(user, job)?;
        let mut discarded = 0u32;
        for draft in self.store.list_ai_drafts(user, root.id)? {
            if draft.status.is_open() {
                self.store.set_ai_draft_status(
                    user,
                    draft.id,
                    AiDraftStatus::Discarded,
                    None,
                    now_ms,
                )?;
                discarded += 1;
            }
        }
        Ok(discarded)
    }

    /// Keeps the open drafts of `job`'s review (those in `drafts` when
    /// given) as notes in `deck`, or in the job's own deck when none is
    /// chosen. Each draft takes the editor's path, so the plan's card
    /// policy, the sanitizer and the media links all apply; a refusal
    /// by the policy stops the loop and is reported, and what was kept
    /// before it stays kept.
    pub fn ai_keep_drafts(
        &self,
        user: UserId,
        job: AiJobId,
        drafts: Option<&[AiDraftId]>,
        deck: Option<DeckId>,
        now_ms: i64,
    ) -> Result<AiKeepReport> {
        let root = self.ai_root_job(user, job)?;
        let Some(deck) = deck.or(root.deck_id) else {
            return Err(ServiceError::Invalid(
                "Choose a deck to keep the cards in.".into(),
            ));
        };
        self.store.get_deck_limits(user, deck)?;
        let mut report = AiKeepReport::default();
        for draft in self.store.list_ai_drafts(user, root.id)? {
            if !draft.status.is_open() || drafts.is_some_and(|ids| !ids.contains(&draft.id)) {
                continue;
            }
            let input = NoteInput {
                note_type: draft.note_type,
                front_html: draft.front_html,
                back_html: draft.back_html,
                tags: draft.tags,
            };
            match self.create_note_from(user, deck, &input, Some(root.id), now_ms) {
                Ok((note, cards)) => {
                    self.store.set_ai_draft_status(
                        user,
                        draft.id,
                        AiDraftStatus::Kept,
                        Some(note),
                        now_ms,
                    )?;
                    report.kept += 1;
                    report.cards += count_u32(cards.len());
                }
                Err(ServiceError::OverCap { message, .. }) => {
                    report.stopped_by = Some(message);
                    break;
                }
                // A draft the editor would not take (its media went
                // away, its cloze was edited out) stays open for the
                // person to fix; the others still go through.
                Err(ServiceError::Invalid(reason)) => {
                    tracing::warn!(draft = draft.id.0, reason = ?reason, "draft not kept");
                }
                Err(other) => return Err(other),
            }
        }
        report.remaining_pending = self.store.count_ai_drafts(user, root.id)?.pending;
        Ok(report)
    }

    /// The account's generation standing, as the policy reports it.
    pub fn ai_quota(&self, user: UserId, now_ms: i64) -> Result<AiUsage> {
        Ok(self.ai_policy.usage(&self.store, user, now_ms)?)
    }

    /// The job the runner is on, or about to take, for this user.
    pub fn ai_active_job(&self, user: UserId) -> Result<Option<AiJobRow>> {
        Ok(self.store.active_ai_job(user)?)
    }

    /// The job whose drafts `job` belongs to: itself, or the parent a
    /// revise or more pass was queued against.
    fn ai_root_job(&self, user: UserId, job: AiJobId) -> Result<AiJobRow> {
        let row = self.ai_job(user, job)?;
        match row.parent_job_id {
            Some(parent) => self.ai_job(user, parent),
            None => Ok(row),
        }
    }

    fn open_ai_draft(&self, user: UserId, draft: AiDraftId) -> Result<AiDraftRow> {
        let row = self
            .store
            .get_ai_draft(user, draft)?
            .ok_or(StoreError::NotFound("ai draft"))?;
        if !row.status.is_open() {
            return Err(ServiceError::Invalid(
                "That draft was already decided; edit the card instead.".into(),
            ));
        }
        Ok(row)
    }

    fn require_no_active_ai_job(&self, user: UserId) -> Result<()> {
        if self.store.active_ai_job(user)?.is_some() {
            return Err(ServiceError::Invalid(
                "A generation is already running.".into(),
            ));
        }
        Ok(())
    }

    /// A composer may hold only so many sources nobody has generated
    /// from: past that, the person is asked to generate or remove.
    fn require_unattached_room(&self, user: UserId) -> Result<()> {
        let waiting = self.store.count_unattached_ai_sources(user)?;
        if waiting >= crate::bounds::AI_UNATTACHED_SOURCES_PER_USER {
            return Err(ServiceError::Invalid(format!(
                "You have {waiting} files waiting already. Generate from them or remove some."
            )));
        }
        Ok(())
    }

    /// The preflight of a source row: this server's inputs, the caps,
    /// then the plan, in that order, so the first reason is the one
    /// nearest the person's control.
    fn preflight_of(
        &self,
        user: UserId,
        row: &AiSourceRow,
        ai: &AiRuntime,
        now_ms: i64,
    ) -> Result<AiPreflight> {
        let input = row.kind;
        let usage = self.ai_policy.usage(&self.store, user, now_ms)?;
        let mut refused = if ai.accepts(input) {
            cap_refusal(row)
        } else {
            Some(("input_disabled", not_accepted(row.kind)))
        };
        if refused.is_none() {
            let request = AiRequest::single(input, row.duration_s);
            if let AiAdmission::Denied { code, message } =
                self.ai_policy
                    .admit_generation(&self.store, user, request, now_ms)?
            {
                refused = Some((code, message));
            }
        }
        let (code, reason) = match refused {
            Some((code, reason)) => (Some(code), Some(reason)),
            None => (None, None),
        };
        Ok(AiPreflight {
            kind: input,
            pages: row.pages,
            est_words: row.est_words,
            image_count: row.image_count,
            duration_s: row.duration_s,
            allowed: code.is_none(),
            code,
            reason,
            cost_class: input.as_str(),
            usage,
        })
    }

    /// Pages, words and pictures of a PDF through the renderer; no
    /// renderer, no facts, and the preflight cannot cap what it does
    /// not know.
    #[cfg(feature = "pdf")]
    fn pdf_facts(&self, ai: &AiRuntime, bytes: &[u8]) -> Result<SourceFacts> {
        let Some(pdf) = ai.pdf() else {
            return Ok(SourceFacts::default());
        };
        match pdf.stats(bytes) {
            Ok(stats) => Ok(SourceFacts {
                pages: Some(stats.pages),
                words: Some(stats.words),
                images: Some(stats.images),
                duration_s: None,
            }),
            Err(e) => {
                tracing::warn!("pdf stats: {e}");
                Err(ServiceError::Invalid(e.user_message()))
            }
        }
    }

    #[cfg(not(feature = "pdf"))]
    fn pdf_facts(&self, _ai: &AiRuntime, _bytes: &[u8]) -> Result<SourceFacts> {
        Ok(SourceFacts::default())
    }

    /// The blob under its hash, unless some upload already put it there.
    fn store_source_blob(
        &self,
        media: &dyn MediaStore,
        sha256: &str,
        mime: &str,
        bytes: &[u8],
    ) -> Result<()> {
        if !self.store.blob_known(sha256)? {
            media
                .put(sha256, mime, bytes)
                .map_err(|e| ServiceError::Internal(format!("source blob: {e}")))?;
        }
        Ok(())
    }
}

/// The sentence for a job whose source housekeeping has removed.
pub const SOURCE_EXPIRED: &str =
    "The source of this generation has expired; upload it again to continue.";

/// What ingest could read off a file for the preflight.
#[derive(Debug, Default, Clone, Copy)]
struct SourceFacts {
    pages: Option<u32>,
    words: Option<u32>,
    images: Option<u32>,
    duration_s: Option<u32>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::media_store::hex(&Sha256::digest(bytes))
}

fn count_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The sentence for one photo too many in a set.
pub fn too_many_photos() -> String {
    format!(
        "Only {} photos per generation.",
        crate::bounds::AI_MAX_IMAGES
    )
}

/// The sentence for one source too many in a set.
pub fn too_many_sources() -> String {
    format!(
        "Only {} files per generation.",
        crate::bounds::AI_SOURCES_PER_JOB
    )
}

/// The sentence for an upload past the per-file cap.
pub fn upload_too_large() -> String {
    format!(
        "That upload is over the {} MB limit.",
        flash_store::media::MAX_FILE_BYTES / (1024 * 1024)
    )
}

/// The upload cap, stated in the unit a person reads.
fn check_upload_size(bytes: u64) -> Result<()> {
    if bytes > flash_store::media::MAX_FILE_BYTES {
        return Err(ServiceError::Invalid(upload_too_large()));
    }
    Ok(())
}

fn clean_instruction(instruction: &str) -> Result<String> {
    let instruction = instruction.trim();
    if instruction.len() > crate::bounds::AI_INSTRUCTION {
        return Err(ServiceError::Invalid(format!(
            "Instructions are at most {} bytes.",
            crate::bounds::AI_INSTRUCTION
        )));
    }
    Ok(instruction.to_string())
}

/// This server does not read the kind at all: a refusal with a code,
/// so a client hides the input rather than retrying.
fn require_ai_input(ai: &AiRuntime, kind: AiSourceKind) -> Result<()> {
    if ai.accepts(kind) {
        Ok(())
    } else {
        Err(ServiceError::AiDenied {
            code: "input_disabled",
            message: not_accepted(kind),
        })
    }
}

/// The kind is read, but not this type of it: the model takes JPEG
/// and PNG, say, and this is a GIF. A sentence naming the type, since
/// the fix is a different file rather than a different server.
fn require_ai_mime(ai: &AiRuntime, kind: AiSourceKind, mime: &str, extension: &str) -> Result<()> {
    if ai.accepts_mime(kind, mime) {
        return Ok(());
    }
    let alternatives: Vec<String> = ai
        .capabilities
        .mimes_of(kind)
        .iter()
        .filter_map(|m| m.rsplit('/').next())
        .take(3)
        .map(|s| s.to_ascii_uppercase())
        .collect();
    let what = match kind {
        AiSourceKind::Image => "images",
        AiSourceKind::Video => "video",
        AiSourceKind::Audio => "audio",
        AiSourceKind::Pdf => "documents",
        AiSourceKind::Text | AiSourceKind::Youtube => "files",
    };
    let hint = if alternatives.is_empty() {
        String::new()
    } else {
        format!(" Use {}.", alternatives.join(", "))
    };
    Err(ServiceError::Invalid(format!(
        "This server's model does not read .{extension} {what}.{hint}"
    )))
}

fn not_accepted(kind: AiSourceKind) -> String {
    let what = match kind {
        AiSourceKind::Text => "pasted text",
        AiSourceKind::Pdf => "PDFs",
        AiSourceKind::Image => "photos",
        AiSourceKind::Youtube => "YouTube links",
        AiSourceKind::Video => "video",
        AiSourceKind::Audio => "audio",
    };
    format!("This server does not accept {what}.")
}

/// The cap a source is over, if any, with the sentence that says so.
fn cap_refusal(row: &AiSourceRow) -> Option<(&'static str, String)> {
    use crate::bounds::{AI_MAX_IMAGES, AI_MAX_MEDIA_SECONDS, AI_MAX_PAGES, AI_MAX_WORDS};
    match row.kind {
        AiSourceKind::Pdf | AiSourceKind::Text => {
            if let Some(pages) = row.pages.filter(|p| *p > AI_MAX_PAGES) {
                return Some((
                    "too_many_pages",
                    format!("This PDF has {pages} pages; the limit is {AI_MAX_PAGES}."),
                ));
            }
            if let Some(words) = row.est_words.filter(|w| *w > AI_MAX_WORDS) {
                return Some((
                    "too_many_words",
                    format!(
                        "That's about {} words; the limit is {}.",
                        thousands(words),
                        thousands(AI_MAX_WORDS)
                    ),
                ));
            }
        }
        AiSourceKind::Image => {
            if row.image_count.is_some_and(|n| n > AI_MAX_IMAGES) {
                return Some(("too_many_images", too_many_photos()));
            }
        }
        AiSourceKind::Youtube | AiSourceKind::Video | AiSourceKind::Audio => {
            if let Some(seconds) = row.duration_s.filter(|s| *s > AI_MAX_MEDIA_SECONDS) {
                return Some((
                    "too_long",
                    format!(
                        "This recording is {}; the limit is {} minutes.",
                        duration_phrase(seconds),
                        AI_MAX_MEDIA_SECONDS / 60
                    ),
                ));
            }
        }
    }
    None
}

/// The cap a set's totals are over, if any, with the sentence that
/// says so. A single member's caps are its own preflight's; these are
/// for what several members add up to.
fn set_cap_refusal(totals: &AiSetTotals) -> Option<(&'static str, String)> {
    use crate::bounds::{
        AI_MAX_IMAGES, AI_MAX_MEDIA_SECONDS, AI_MAX_PAGES, AI_MAX_SET_BYTES, AI_MAX_WORDS,
    };
    if totals.pages > AI_MAX_PAGES {
        return Some((
            "too_many_pages",
            format!(
                "These files have {} pages in all; one generation reads up to {AI_MAX_PAGES}.",
                thousands(totals.pages)
            ),
        ));
    }
    if totals.est_words > AI_MAX_WORDS {
        return Some((
            "too_many_words",
            format!(
                "That's about {} words in all; the limit is {}.",
                thousands(totals.est_words),
                thousands(AI_MAX_WORDS)
            ),
        ));
    }
    if totals.image_count > AI_MAX_IMAGES {
        return Some((
            "too_many_images",
            format!(
                "That's {} photos; only {AI_MAX_IMAGES} go into one generation.",
                totals.image_count
            ),
        ));
    }
    if totals.duration_s > AI_MAX_MEDIA_SECONDS {
        return Some((
            "too_long",
            format!(
                "These recordings run {} in all; the limit is {} minutes.",
                duration_phrase(totals.duration_s),
                AI_MAX_MEDIA_SECONDS / 60
            ),
        ));
    }
    if totals.bytes > AI_MAX_SET_BYTES {
        return Some((
            "too_large",
            format!(
                "These files are {} MB together; one generation takes up to {} MB.",
                totals.bytes / (1024 * 1024),
                AI_MAX_SET_BYTES / (1024 * 1024)
            ),
        ));
    }
    None
}

/// A cap's refusal is the person's to fix (a shorter file); the plan's
/// and the server's carry a code a client acts on.
fn refusal(code: &'static str, reason: String) -> ServiceError {
    match code {
        "too_many_pages" | "too_many_words" | "too_many_images" | "too_long" | "too_large"
        | "too_many_sources" => ServiceError::Invalid(reason),
        _ => ServiceError::AiDenied {
            code,
            message: reason,
        },
    }
}

/// The facts line of one source: what the reading found, in the units
/// that suit the kind ("12 pages · ~4,800 words · 3 figures", "18:32
/// of video"). Empty facts (a video link, whose length only the model
/// knows) say so rather than showing nothing.
pub fn facts_line(p: &AiPreflight) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(pages) = p.pages {
        parts.push(count_of(pages, "page", "pages"));
    }
    if let Some(words) = p.est_words {
        parts.push(format!("~{} words", thousands(words)));
    }
    // A PDF without pictures says nothing about figures; a photo is
    // always one photo.
    if let Some(n) = p.image_count.filter(|n| *n > 0) {
        match p.kind {
            AiSourceKind::Pdf => parts.push(count_of(n, "figure", "figures")),
            _ => parts.push(count_of(n, "photo", "photos")),
        }
    }
    if let Some(seconds) = p.duration_s {
        let what = if p.kind.is_audio() { "audio" } else { "video" };
        parts.push(format!("{} of {what}", clock(seconds)));
    }
    if parts.is_empty() {
        return "Length is known once the model has watched it".to_string();
    }
    parts.join(" · ")
}

/// The facts line of a set: "3 sources · 48 pages · ~4,800 words · 6
/// photos · 4:31 of media".
pub fn set_facts_line(t: &AiSetTotals) -> String {
    let mut parts = vec![count_of(t.sources, "source", "sources")];
    if t.pages > 0 {
        parts.push(count_of(t.pages, "page", "pages"));
    }
    if t.est_words > 0 {
        parts.push(format!("~{} words", thousands(t.est_words)));
    }
    if t.image_count > 0 {
        parts.push(count_of(t.image_count, "photo", "photos"));
    }
    if t.duration_s > 0 {
        parts.push(format!("{} of media", clock(t.duration_s)));
    }
    parts.join(" · ")
}

/// "150 pages", "1 page".
pub fn count_of(n: u32, one: &str, many: &str) -> String {
    let word = if n == 1 { one } else { many };
    format!("{} {word}", thousands(n))
}

/// "18:32", or "1:18:32" past the hour.
pub fn clock(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `21000` as `21,000`.
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// `6720` seconds as `1 h 52 min`; under an hour, `52 min`.
fn duration_phrase(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours} h {minutes} min")
    } else {
        format!("{minutes} min")
    }
}

/// What a generation from a source would be: the facts the preflight
/// shows, and the verdict with its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPreflight {
    pub kind: AiInput,
    pub pages: Option<u32>,
    pub est_words: Option<u32>,
    pub image_count: Option<u32>,
    pub duration_s: Option<u32>,
    pub allowed: bool,
    /// The refusal's token: a cap's (`too_many_pages`, `too_many_words`,
    /// `too_many_images`, `too_long`), this server's (`input_disabled`),
    /// or the plan's own.
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    /// What the generation is charged as: the input kind's token.
    pub cost_class: &'static str,
    pub usage: AiUsage,
}

/// One kind this server reads and, when the plan withholds it from the
/// person, the policy's code and sentence.
pub type KindAdmission = (AiInput, Option<(&'static str, String)>);

/// What a set adds up to, over the facts ingest recorded. A member
/// whose fact is unknown (a link's length) adds nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiSetTotals {
    pub sources: u32,
    pub pages: u32,
    pub est_words: u32,
    pub image_count: u32,
    pub duration_s: u32,
    pub bytes: u64,
}

/// What one generation over a set would be: each member's preflight,
/// the totals, and the verdict on the whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSetPreflight {
    pub sources: Vec<(AiSourceId, AiPreflight)>,
    pub totals: AiSetTotals,
    /// Every kind in the set, in order of first use.
    pub inputs: Vec<AiInput>,
    /// What the generation is charged as: the dearest kind's token.
    pub cost_class: &'static str,
    pub allowed: bool,
    pub code: Option<&'static str>,
    pub reason: Option<String>,
    /// The member a refusal is about, when it is about one member.
    pub refused_source_id: Option<AiSourceId>,
    /// The facts line for the whole set.
    pub line: String,
    pub usage: AiUsage,
}

/// A job with what a client shows beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiJobDetail {
    pub job: AiJobRow,
    /// The job whose drafts this one's review shows.
    pub root: AiJobId,
    /// The set the job reads, in order, less what housekeeping took.
    pub sources: Vec<AiSourceRow>,
    /// How many members housekeeping has taken.
    pub sources_expired: u32,
    pub counts: AiDraftCounts,
}

/// A pass queued against a review, and the turn its instruction became
/// (none when it had no instruction).
struct ChildQueued {
    job_id: AiJobId,
    turn_id: Option<i64>,
}

/// A message accepted into a review's thread: the pass to poll and
/// the turn to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiEditQueued {
    pub job_id: AiJobId,
    pub turn_id: i64,
}

/// A review's thread as a client shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiThread {
    pub root: AiJobRow,
    pub turns: Vec<AiTurnRow>,
    /// The pass of this review still queued or running, when one is.
    pub active_job_id: Option<AiJobId>,
}

/// What a keep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiKeepReport {
    pub kept: u32,
    /// Cards the kept notes expanded into.
    pub cards: u32,
    /// Open drafts left on the review afterwards.
    pub remaining_pending: u32,
    /// The card policy's sentence, when it stopped the loop.
    pub stopped_by: Option<String>,
}

/// How much of a generated card's source the grader is shown: enough
/// for any passage a card could come from, small enough that a grade
/// stays a short, cheap call.
pub const VOICE_CONTEXT_WORDS: usize = 6_000;

/// The first `limit` whitespace-separated words of `text`, with a note
/// that it was cut when it was; the whole text otherwise.
fn first_words(text: &str, limit: usize) -> String {
    let mut words = text.split_whitespace();
    let kept: Vec<&str> = words.by_ref().take(limit).collect();
    if words.next().is_none() {
        return text.trim().to_string();
    }
    let mut out = kept.join(" ");
    out.push_str("\n[the source continues beyond what is shown here]");
    out
}

/// A card side as text: the plain column, or the rich side reduced to
/// text when the plain one is empty.
fn plain_side(plain: &str, html: Option<&str>) -> String {
    let plain = plain.trim();
    match html {
        Some(html) if plain.is_empty() => richtext::html_to_text(html),
        _ => plain.to_string(),
    }
}

/// What the student said, as the model gets it: trimmed, not empty,
/// and no longer than `bounds::TRANSCRIPT` (the edge refuses longer
/// requests first; a direct caller meets the same bound here).
fn spoken_input(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ServiceError::Invalid("nothing was said".into()));
    }
    if text.len() > crate::bounds::TRANSCRIPT {
        return Err(ServiceError::Invalid(format!(
            "transcript is longer than {} bytes",
            crate::bounds::TRANSCRIPT
        )));
    }
    Ok(text.to_string())
}

/// A spoken answer as a request carries it: the device's transcript, or
/// the recording for the grader to listen to. Built by `spoken_payload`
/// from the optional fields both routes take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpokenPayload {
    Transcript(String),
    Audio { mime: String, bytes: Vec<u8> },
}

impl SpokenPayload {
    pub fn as_input(&self) -> SpokenInput<'_> {
        match self {
            SpokenPayload::Transcript(text) => SpokenInput::Transcript(text),
            SpokenPayload::Audio { mime, bytes } => SpokenInput::Audio { mime, bytes },
        }
    }

    /// What the ledger records the call as having read.
    pub fn kind(&self) -> AiSourceKind {
        match self {
            SpokenPayload::Transcript(_) => AiSourceKind::Text,
            SpokenPayload::Audio { .. } => AiSourceKind::Audio,
        }
    }
}

/// One spoken answer from a request's optional fields: exactly one of
/// the transcript or the recording (its type and base64), the recording
/// decoded and held to `bounds::VOICE_AUDIO_BYTES`, its type reduced to
/// the bare media type and checked against `bounds::VOICE_AUDIO_MIMES`.
pub fn spoken_payload(
    transcript: Option<String>,
    audio_mime: Option<String>,
    audio_b64: Option<String>,
) -> Result<SpokenPayload> {
    use base64::Engine;
    // A blank transcript stays a transcript: `spoken_input` says
    // "nothing was said", which is what the client should hear.
    let audio = match (audio_mime, audio_b64) {
        (None, None) => None,
        (Some(mime), Some(data)) => Some((mime, data)),
        _ => {
            return Err(ServiceError::Invalid(
                "a recording needs both its type and its data".into(),
            ))
        }
    };
    match (transcript, audio) {
        (Some(text), None) => Ok(SpokenPayload::Transcript(text)),
        (None, Some((mime, data))) => {
            let mime = mime
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if !crate::bounds::VOICE_AUDIO_MIMES.contains(&mime.as_str()) {
                return Err(ServiceError::Invalid(format!(
                    "recording type {mime:?} is not one the grader takes"
                )));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data.trim())
                .map_err(|_| ServiceError::Invalid("recording is not valid base64".into()))?;
            if bytes.is_empty() {
                return Err(ServiceError::Invalid("recording is empty".into()));
            }
            if bytes.len() > crate::bounds::VOICE_AUDIO_BYTES {
                return Err(ServiceError::Invalid(format!(
                    "recording is larger than {} bytes",
                    crate::bounds::VOICE_AUDIO_BYTES
                )));
            }
            Ok(SpokenPayload::Audio { mime, bytes })
        }
        (Some(_), Some(_)) => Err(ServiceError::Invalid(
            "send the transcript or the recording, not both".into(),
        )),
        (None, None) => Err(ServiceError::Invalid(
            "say something or send a recording".into(),
        )),
    }
}

/// A rating on the scheduler's scale from what the model returned;
/// anything outside 1 to 4 is pulled to the nearest end, and a missing
/// one is Again (the provider has already turned a grade without a
/// rating into `unclear`, so this is the last resort, not the rule).
fn rating_of(rating: Option<u8>) -> Result<Rating> {
    Rating::from_i64(i64::from(rating.unwrap_or(1).clamp(1, 4)))
        .ok_or_else(|| ServiceError::Internal("rating out of range after clamp".into()))
}

/// What one spoken turn came to, for the surfaces to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpokenOutcome {
    /// The student answered: the verdict, with the reveal to show
    /// (boxed: it is most of the enum, and the bare intents should not
    /// pay for it).
    Graded(Box<SpokenGrade>),
    /// The student asked about the card: the grounded answer.
    Answered(SpokenClarify),
    /// The student named a grade instead of answering, or changed it.
    Rated(Rating),
    /// Hear the card again.
    Repeat,
    /// Does not know it; move on.
    Skip,
    /// Take the pending grade back.
    Undo,
    /// Nothing the session could use; ask again.
    Unclear,
}

/// A spoken answer as the grader judged it, with the reveal the client
/// shows while the student hears the verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpokenGrade {
    pub rating: Rating,
    pub feedback: String,
    pub spoken_feedback: String,
    pub reveal: Reveal,
}

/// A grounded answer to a question about the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpokenClarify {
    pub answer: String,
    pub spoken_answer: String,
    /// The token of the spoken answer's clip in the account's voice,
    /// queued by the surface that has a voice engine; none otherwise.
    pub clip_token: Option<String>,
}

/// Why a spoken turn produced nothing: our own refusal (a foreign or
/// ended session, an empty transcript, a missing card), or the model's
/// failure, which the surfaces report with its user sentence.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error("model: {0}")]
    Model(LlmError),
}

impl From<StoreError> for VoiceError {
    fn from(err: StoreError) -> Self {
        VoiceError::Service(err.into())
    }
}

/// A search term as the store may run it: trimmed, absent when blank,
/// refused past `bounds::SEARCH` (a term longer than that matches
/// nothing a card side can hold, and LIKE's cost grows with it).
fn bounded_search(search: Option<&str>) -> Result<Option<&str>> {
    let search = search.map(str::trim).filter(|q| !q.is_empty());
    if search.is_some_and(|q| q.len() > crate::bounds::SEARCH) {
        return Err(ServiceError::Invalid(format!(
            "search terms are at most {} bytes",
            crate::bounds::SEARCH
        )));
    }
    Ok(search)
}

/// The advanced editor's submission: a note type, two raw HTML fields
/// (sanitized here, never trusted), and the tag list every generated card
/// receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteInput {
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
}

/// What the editor opens with for an existing card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSeed {
    pub card_id: CardId,
    pub deck_id: DeckId,
    /// None: a standalone card the save will adopt into a new note.
    pub note_id: Option<NoteId>,
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// Live cards this save will touch (siblings included).
    pub sibling_count: usize,
}

struct PreparedNote {
    front_html: richtext::SanitizedHtml,
    back_html: richtext::SanitizedHtml,
    generated: Vec<GeneratedCard>,
    tags: Vec<String>,
}

impl PreparedNote {
    fn save(
        &self,
        note_type: NoteType,
        cap: Option<u32>,
        ai_job_id: Option<AiJobId>,
        now_ms: i64,
    ) -> NoteSave<'_> {
        NoteSave {
            note_type,
            front_html: &self.front_html,
            back_html: &self.back_html,
            generated: &self.generated,
            tags: &self.tags,
            cap,
            ai_job_id,
            now_ms,
        }
    }
}

/// A stored "more new cards today" boost counts only on the study day it
/// was granted for; any other day it is spent. The one place that rule
/// lives: budgets, boost_today and boost_new_today all read through it.
fn active_boost(boost_new: u32, boost_day: i64, day_start: i64) -> u32 {
    if boost_day == day_start {
        boost_new
    } else {
        0
    }
}

/// Trimmed, lowercased, deduped, empties dropped. The store lowercases
/// too, but doing it here keeps the editor's echo consistent.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().to_lowercase();
        if !t.is_empty() && seen.insert(t.clone()) {
            out.push(t);
        }
    }
    out
}

/// Plain card text -> editor HTML: escaped, newlines as `<br>`.
pub fn text_to_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Current wall-clock in epoch ms.
/// Anki-style typed-answer feedback: the typed string with each char
/// marked right/wrong by LCS alignment against the expected answer (which
/// the revealed back shows anyway). Expected chars the user missed are
/// simply skipped; extra typed chars are wrong; an empty result renders
/// as a single dash.
pub fn type_diff(expected: &str, typed: &str) -> Vec<DiffSpan> {
    let e: Vec<char> = expected.trim().chars().take(200).collect();
    let t: Vec<char> = typed.trim().chars().take(200).collect();
    let (n, m) = (e.len(), t.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if e[i] == t[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut spans: Vec<DiffSpan> = Vec::new();
    let push = |ch: char, ok: bool, spans: &mut Vec<DiffSpan>| match spans.last_mut() {
        Some(s) if s.ok == ok => s.text.push(ch),
        _ => spans.push(DiffSpan {
            text: ch.to_string(),
            ok,
        }),
    };
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if e[i] == t[j] {
            push(t[j], true, &mut spans);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1; // expected char the user missed; the back shows it
        } else {
            push(t[j], false, &mut spans);
            j += 1;
        }
    }
    while j < m {
        push(t[j], false, &mut spans);
        j += 1;
    }
    if spans.is_empty() {
        spans.push(DiffSpan {
            text: "–".into(),
            ok: false,
        });
    }
    spans
}

pub fn now_ms() -> i64 {
    Timestamp::now().as_millisecond()
}
