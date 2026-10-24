//! Domain services: the single choke point through which every interface
//! (web UI, MCP tools, future API) reads and mutates study state. Handlers
//! translate protocol <-> these calls and nothing else.

use std::sync::Arc;

use flash_core::queue::{order_queue, StudyScope};
use flash_core::{CardId, DeckId, GradingMode, Scheduler, SessionId, UserId, UserSettings};
use flash_store::{richtext, CardRow, SessionStats, Store, StoreError};
use jiff::Timestamp;

use crate::i18n::Words;
use crate::policy::{CardPolicy, Unlimited};

mod account;
mod admin;
mod cards;
pub mod deck_cards;
mod deck_settings;
pub mod goals;
mod notes;
mod study;
mod today;

pub use notes::{EditorSeed, McpSourcePatch, NoteFields, NoteInput};
pub use study::type_diff;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A request a person can put right, worded for them (`i18n`).
    #[error("{0}")]
    Invalid(Words),
    /// A request no person makes: a value outside the bounds its client
    /// keeps, a kind it never sends, an AI assistant's call that names
    /// nothing. English, for whoever sent it (a developer, the model
    /// reading an MCP result), in every reader's language.
    #[error("{0}")]
    Malformed(String),
    /// A request refused for a reason the person can act on: `message`
    /// is worded in the reader's language (`i18n`), `code` is the stable
    /// reason a page decides by (`"nocloze"`, `"empty"`, `"long"`...).
    #[error("{message}")]
    Refused { code: &'static str, message: Words },
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
        message: Words,
    },
    /// A downstream extension's policy refused the work: `code` is its
    /// stable token a client branches on, `message` the sentence a person
    /// reads.
    #[error("{message}")]
    PolicyDenied { code: &'static str, message: Words },
}

pub type Result<T> = std::result::Result<T, ServiceError>;

impl ServiceError {
    /// What a person reads about it: a refusal's own words, the English
    /// of a request no person makes, and our own failure as "internal
    /// error" in their language.
    pub fn words(&self) -> Words {
        match self {
            ServiceError::Invalid(words)
            | ServiceError::Refused { message: words, .. }
            | ServiceError::OverCap { message: words, .. }
            | ServiceError::PolicyDenied { message: words, .. } => words.clone(),
            // for machines: a request no person makes.
            ServiceError::Malformed(text) => Words::for_machines(text.clone()),
            ServiceError::Store(StoreError::Invalid(text)) => store_refusal(text.clone()),
            // for machines: a page or a client asked for what is not there.
            ServiceError::Store(StoreError::NotFound(what)) => {
                Words::for_machines(format!("not found: {what}"))
            }
            ServiceError::Store(e) if e.is_constraint() => crate::i18n::t("error-exists"),
            ServiceError::Store(_) | ServiceError::Scheduler(_) | ServiceError::Internal(_) => {
                crate::i18n::t("error-internal")
            }
        }
    }

    /// A refusal with `code`, worded by the message `id` and its
    /// variables.
    pub fn refused(code: &'static str, id: &str, args: &[(&str, crate::i18n::Value)]) -> Self {
        ServiceError::Refused {
            code,
            message: crate::i18n::ta(id, args),
        }
    }
}

pub(crate) fn deck_refusal_code(message: &str) -> Option<&'static str> {
    Some(match message {
        m if m.contains("cannot contain") => "nest",
        "deck name is empty" => "empty",
        m if m.contains("a deck with that name already exists") => "taken",
        m if m.contains("moved into itself") => "self",
        m if m.contains("name") && m.contains("longer than") => "long",
        m if m.starts_with("Your deck \"") && m.contains("\" is archived") => "archived",
        m if m.starts_with("an account may have at most") => "cap",
        _ => return None,
    })
}

pub(crate) fn store_refusal(message: String) -> Words {
    if crate::i18n::reads_english() {
        // from the store: its English, to an English reader.
        return Words::from_store(message);
    }
    if let Some(name) = message
        .strip_prefix("Your deck \"")
        .and_then(|rest| rest.split_once("\" is archived"))
        .map(|(name, _)| name)
    {
        return crate::i18n::ta("deck-error-archived-sentence", &[("name", name.into())]);
    }
    if let Some(copy) = deck_refusal_code(&message).and_then(deck_path_error_copy) {
        return copy;
    }
    if message.starts_with("a card may carry at most") {
        return crate::i18n::ta(
            "store-error-tags-many",
            &[("max", flash_store::MAX_TAGS_PER_CARD.into())],
        );
    }
    if message.starts_with("a tag may be at most") {
        return crate::i18n::ta(
            "store-error-tag-long",
            &[("max", flash_store::MAX_TAG_LEN.into())],
        );
    }
    if message.ends_with("cards can be changed at once") {
        return crate::i18n::ta(
            "cards-error-bulk-many",
            &[("max", flash_store::MAX_BULK_CARDS.into())],
        );
    }
    if message == "That deck is archived. Unarchive it first." {
        return crate::i18n::t("cards-error-move-archived");
    }
    if message == "name at least one tag" {
        return crate::i18n::t("cards-error-tags-none");
    }
    // from the store: a refusal the pages have no words for yet.
    Words::from_store(message)
}

pub(crate) fn deck_path_error_copy(code: &str) -> Option<Words> {
    let id = match code {
        "nest" => "deck-error-nest",
        "empty" => "deck-error-empty",
        "taken" => "deck-error-taken",
        "self" => "deck-error-self",
        "long" => "deck-error-long",
        "archived" => "deck-error-archived",
        "cap" => "deck-error-cap",
        "invalid" => "deck-error-invalid",
        _ => return None,
    };
    Some(crate::i18n::t(id))
}

pub fn card_refused(e: flash_core::CardTextError) -> ServiceError {
    let (code, message) = card_refusal(e);
    ServiceError::Refused { code, message }
}

fn card_refusal(e: flash_core::CardTextError) -> (&'static str, Words) {
    use crate::i18n::{t, ta};
    use flash_core::CardTextError::*;
    match e {
        FrontEmpty => ("empty", t("card-error-front-empty")),
        BackEmpty => ("empty", t("card-error-back-empty")),
        TooLong => (
            "long",
            ta(
                "card-error-side-long",
                &[("max", flash_core::MAX_SIDE_LEN.into())],
            ),
        ),
    }
}

pub fn note_refused(e: flash_store::notes::NoteError) -> ServiceError {
    use flash_store::notes::NoteError::*;
    match e {
        FieldTooLong => ServiceError::refused(
            "long",
            "card-error-field-long",
            &[("max", flash_store::notes::MAX_FIELD_HTML.into())],
        ),
        NoCloze => ServiceError::refused("nocloze", "card-error-no-cloze", &[]),
        EmptyCloze(n) => {
            ServiceError::refused("empty", "card-error-cloze-empty", &[("n", n.into())])
        }
        Card(e) => card_refused(e),
        ClozeCard(n, e) => {
            let (code, why) = card_refusal(e);
            ServiceError::refused(
                code,
                "card-error-cloze-card",
                &[("n", n.into()), ("error", why.into())],
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardFront {
    pub card_id: CardId,
    pub front: String,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub grading_mode: GradingMode,
    pub cards_due: u32,
    /// Eligible cards today's new/review limits kept out of the queue.
    pub held_by_limit: u32,
    /// Whether the scope holds any card at all. False means an empty
    /// queue is not "caught up" but nothing there to study yet.
    pub scope_has_cards: bool,
    pub first_card: Option<CardFront>,
    /// The card after `first_card` in today's queue, so the UI can warm
    /// its media before it's shown. None when the queue has fewer than two.
    pub next_card_id: Option<CardId>,
}

#[derive(Debug, Clone)]
pub struct SubmitResult {
    pub next_card: Option<CardFront>,
    /// The card after `next_card` in the queue (media warm-up); None when
    /// there isn't one.
    pub next_card_id: Option<CardId>,
    pub remaining: u32,
    /// Eligible cards today's new/review limits kept out of the queue.
    pub held_by_limit: u32,
}

/// What a study surface shows for a card: rich HTML when the card has
/// it, whether it asks for a typed answer, and which images to warm
/// (this card's still-hidden back, and the next card's both sides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyCard {
    pub card_id: CardId,
    pub front: String,
    pub front_html: Option<String>,
    /// The front's language as its labels say it, for the page's `lang`;
    /// empty when nothing says (`labels::label_side`).
    pub front_lang: String,
    pub wants_typing: bool,
    pub preload_media: Vec<i64>,
    pub prefetch_media: Vec<i64>,
    /// The card after this one in the queue, so a client can prepare it
    /// ahead of getting there. None at the end.
    pub next_card: Option<CardId>,
    /// The languages the card's own deck names (its language hints);
    /// empty when it names none.
    pub speech_language_hints: Vec<String>,
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
    /// Each side's language as its labels say it, for the page's `lang`;
    /// empty when nothing says (`labels::label_side`).
    pub front_lang: String,
    pub back_lang: String,
    pub typed_diff: Option<Vec<DiffSpan>>,
}

/// A deck as every list shows it. `Services::list_decks` hands these out
/// in tree order: each deck followed by the decks under it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckRow {
    pub id: DeckId,
    /// The whole path, `Bio::Cells`: what a deck is called wherever it is
    /// named by a program (a connector, an export, the API).
    pub name: String,
    /// The same path as a person reads it, `Bio › Cells`: what a page
    /// shows wherever a deck is named without its surroundings (a picker,
    /// a title). `::` is for storing and for programs, never for reading.
    pub path: String,
    /// The name to show under its parent: the last segment of the path,
    /// or the rest of it when the deck it would sit in does not exist.
    pub leaf: String,
    /// How many decks it sits inside, among the decks that exist.
    pub depth: u32,
    pub parent_id: Option<DeckId>,
    pub description: String,
    /// Today's budgeted counts, the decks under it included: what studying
    /// this deck would serve. Zero for an archived deck.
    pub due_count: u32,
    pub new_count: u32,
    /// Every card a session over it could ask, the decks under it
    /// included, whatever their schedule: what its preview goes through.
    /// Zero for an archived deck.
    pub card_count: u32,
    /// When the deck was archived; None while it is active.
    pub archived_at: Option<i64>,
    pub goal: Option<goals::GoalView>,
}

/// The Today page in one read.
#[derive(Debug, Clone)]
pub struct Dashboard {
    pub counts: QueueCounts,
    /// Account-wide "more new cards today" already granted.
    pub boost_today: u32,
    pub decks: Vec<DeckRow>,
    /// The "Connect your AI" card, until the user dismisses it.
    pub show_connect_cta: bool,
    /// The account still has the default time zone, so the page asks
    /// the browser for its own.
    pub timezone_is_default: bool,
}

/// One page of a deck's cards, as the lists show them.
#[derive(Debug, Clone)]
pub struct CardsPage {
    pub rows: Vec<CardRow>,
    pub page: u32,
    pub pages: u32,
    pub total: u32,
    /// How many of the deck's cards are archived, whatever the filter.
    pub archived: u32,
}

/// A deck's settings in one read: the deck, its limit overrides beside
/// the account defaults they would inherit, today's boost, and the live
/// card count.
#[derive(Debug, Clone)]
pub struct DeckDetail {
    pub deck: DeckRow,
    pub limits: flash_store::DeckLimits,
    pub default_new: u32,
    pub default_reviews: u32,
    pub boost_today: u32,
    /// Live cards in the deck and every deck under it, archived ones
    /// too: what deleting it removes. Its card list leaves archived
    /// subdecks out.
    pub card_count: u32,
    /// How many decks sit under it, at any depth.
    pub subdeck_count: u32,
    /// The languages its cards are in, as the deck names them; empty when
    /// it names none.
    pub speech_language_hints: Vec<String>,
}

/// The cap the store must re-check (None = unlimited) plus the validated
/// texts, ready to insert.
type AdmittedCards = (Option<u32>, Vec<(flash_core::CardText, Vec<String>)>);

/// Refuses a scope naming more tags than a person would pick by hand.
/// Both the counting and the starting path go through it, so a page's
/// number and the session it promises are refused together.
fn check_scope(scope: &StudyScope) -> Result<()> {
    if scope.tags.len() > flash_core::queue::MAX_SCOPE_TAGS {
        return Err(ServiceError::Malformed(format!(
            "a study scope may name at most {} tags",
            flash_core::queue::MAX_SCOPE_TAGS
        )));
    }
    Ok(())
}

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

/// Where a review came from: the surface (`web`, `mobile`, `mcp`) and,
/// when the surface has several clients, which one (an MCP client's
/// name, the app's platform).
#[derive(Debug, Clone, Copy)]
pub struct ReviewOrigin<'a> {
    pub source: &'a str,
    pub client: Option<&'a str>,
    /// How long the card was on screen before this grade, as the page
    /// that showed it measured; None where nobody measured (an
    /// assistant grading by itself, an older app).
    pub answer_ms: Option<i64>,
}

impl<'a> ReviewOrigin<'a> {
    pub const WEB: ReviewOrigin<'static> = ReviewOrigin {
        source: "web",
        client: None,
        answer_ms: None,
    };

    pub fn new(source: &'a str, client: Option<&'a str>) -> Self {
        ReviewOrigin {
            source,
            client,
            answer_ms: None,
        }
    }

    /// The same origin, with the time the page measured.
    pub fn answered_in(mut self, answer_ms: Option<i64>) -> Self {
        self.answer_ms = answer_ms;
        self
    }
}

/// Where a study session was started, as `study_sessions.surface` keeps
/// it; an extension may keep its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSurface {
    Web,
    /// An assistant asked for it, in a host that does not show the panel.
    Mcp,
    /// An assistant asked for it, in a host that shows the panel.
    Panel,
    Mobile,
}

impl SessionSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionSurface::Web => "web",
            SessionSurface::Mcp => "mcp",
            SessionSurface::Panel => "panel",
            SessionSurface::Mobile => "mobile",
        }
    }

    /// How long an ungraded session may be handed back instead of a new
    /// one. Only the web page opens a session per load; the others open
    /// one when asked to.
    pub fn reuse_ms(self) -> Option<i64> {
        match self {
            SessionSurface::Web => Some(30 * 60 * 1000),
            _ => None,
        }
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
    pub label: Words,
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
    pub label: Words,
    /// After today: `count` is cards due, not reviews done.
    pub future: bool,
    pub today: bool,
}

/// A run of heatmap columns sharing a month, for the label row.
#[derive(Debug, Clone)]
pub struct MonthSpan {
    pub label: Words,
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

/// The account as a client's "me": profile, theme, and which login
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
    pub due_by_day: bool,
    pub has_password: bool,
    pub passkeys: Vec<PasskeyInfo>,
    /// Live cards across every deck.
    pub card_count: u32,
}

/// The knobs that decide when "today" starts and how FSRS schedules.
/// `None` leaves a field unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StudySettingsPatch {
    pub timezone: Option<String>,
    pub day_cutoff_hour: Option<u8>,
    pub desired_retention: Option<f32>,
    /// Review cards due for their whole study day (true) or at their
    /// exact time (false).
    pub due_by_day: Option<bool>,
}

/// When the next cards come, for a session that ran out: the moment the
/// first one is due (or, for new cards held by the daily limit, the next
/// study day's start), how many study days away that is, and how many
/// cards come that day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct NextDue {
    pub at_ms: i64,
    /// 0 is later today (only with exact times), 1 tomorrow, and so on.
    pub in_days: u32,
    pub cards: u32,
}

/// One study day ahead, from its start, and how many cards come due on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DayAhead {
    pub at_ms: i64,
    pub cards: u32,
}

/// A session whose queue ran out (`Services::finished_session`).
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedSession {
    pub stats: SessionStats,
    /// From the session's start to its last review; None without one.
    pub took_ms: Option<i64>,
    pub next_due: Option<NextDue>,
    /// The seven study days after today.
    pub week: Vec<DayAhead>,
    pub goal: Option<goals::SessionGoal>,
}

/// Sync by design; the async layer wraps calls in spawn_blocking.
#[derive(Clone)]
pub struct Services {
    store: Arc<Store>,
    policy: Arc<dyn CardPolicy>,
    pub(crate) seen_locales: Arc<crate::account_locale::Seen>,
    off_request: tokio_util::task::TaskTracker,
}

impl Services {
    /// Unlimited cards: the open server.
    pub fn new(store: Arc<Store>) -> Self {
        Self::with_policy(store, Arc::new(Unlimited))
    }

    pub fn with_policy(store: Arc<Store>, policy: Arc<dyn CardPolicy>) -> Self {
        Self {
            store,
            policy,
            seen_locales: Arc::default(),
            off_request: tokio_util::task::TaskTracker::new(),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Starts `work` on a blocking thread and does not wait for it: a
    /// counter or an event written off the request path, which nobody was
    /// promised (a restart losing it costs a row of a count). Every such
    /// write goes through here, so `settled` can tell when they are done.
    pub fn off_request(&self, work: impl FnOnce(&Services) + Send + 'static) {
        let services = self.clone();
        self.off_request.spawn_blocking(move || work(&services));
    }

    /// Resolves once every write `off_request` started has finished,
    /// those they start included: what a test awaits before reading what
    /// they wrote, or that they wrote nothing.
    pub async fn settled(&self) {
        self.off_request.close();
        self.off_request.wait().await;
        self.off_request.reopen();
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
            .map(|s| s.on_calendar(&settings.timezone, settings.day_cutoff_hour))
            .map_err(|e| ServiceError::Scheduler(e.to_string()))
    }

    fn card_goal_window(
        &self,
        user: UserId,
        settings: &UserSettings,
        card: CardId,
        now_ms: i64,
    ) -> Result<Option<flash_core::goal::GoalWindow>> {
        let Some(row) = self.store.get_card(user, card)? else {
            return Ok(None);
        };
        self.goal_window(user, settings, row.deck_id, now_ms)
    }

    /// Start of the user's current study day (e.g. 4am local), for the
    /// daily new-card budget (`flash_core::day`).
    fn study_day_start_ms(settings: &UserSettings, now_ms: i64) -> i64 {
        settings.study_day_start_ms(now_ms)
    }

    /// The start of the study day after the one holding `now_ms`
    /// (`flash_core::day`).
    fn next_study_day_ms(settings: &UserSettings, now_ms: i64) -> i64 {
        settings.next_study_day_ms(now_ms)
    }

    /// What is due at `now_ms` for this account: review cards for their
    /// whole study day unless it asked for exact times.
    fn due_at(settings: &UserSettings, now_ms: i64) -> flash_core::queue::Due {
        if settings.due_by_day {
            flash_core::queue::Due::by_day(now_ms, Self::next_study_day_ms(settings, now_ms))
        } else {
            flash_core::queue::Due::exact(now_ms)
        }
    }

    /// Today's remaining allowances, Anki-style: each deck's own new/review
    /// limit (or the account default) minus what that deck consumed today,
    /// plus any "more new cards today" boost; the account totals cap the
    /// sum. An account boost raises the default, so decks that inherit it
    /// get it too. The card policy may clamp the account new total. Shared
    /// by the queue builder, dashboard counts, and deck summaries so they
    /// agree.
    fn budgets(
        &self,
        user: UserId,
        settings: &UserSettings,
        now_ms: i64,
    ) -> Result<flash_core::queue::Budgets> {
        Ok(self.budgets_and_goals(user, settings, now_ms)?.0)
    }

    fn budgets_and_goals(
        &self,
        user: UserId,
        settings: &UserSettings,
        now_ms: i64,
    ) -> Result<(flash_core::queue::Budgets, goals::Goals)> {
        let day_start = Self::study_day_start_ms(settings, now_ms);
        let consumed = self.store.daily_consumption_by_deck(user, day_start)?;
        let reviewed_total = consumed.values().map(|(_, r)| r).sum::<u32>();
        let introduced: std::collections::HashMap<DeckId, u32> =
            consumed.iter().map(|(d, (n, _))| (*d, *n)).collect();
        let goals = self.goals(user, settings, now_ms, &introduced)?;
        let account_boost = active_boost(settings.boost_new, settings.boost_day, day_start);
        let review_total = settings.reviews_per_day.saturating_sub(reviewed_total);

        // A deck boost must actually yield more cards even when the account
        // allowance is already spent (the usual moment someone asks for
        // more), so today's deck boosts lift the account ceiling too.
        let mut deck_boosts_total = 0u32;
        let mut introduced_total = 0u32;
        let mut per_deck = std::collections::HashMap::new();
        let mut paced = std::collections::HashSet::new();
        for (deck, limits) in self.store.all_deck_limits(user)? {
            let (n, r) = consumed.get(&deck).copied().unwrap_or((0, 0));
            let deck_boost = active_boost(limits.boost_new, limits.boost_day, day_start);
            let new_limit = match goals.pace(deck) {
                Some(pace) => {
                    paced.insert(deck);
                    pace + deck_boost
                }
                None => {
                    deck_boosts_total += deck_boost;
                    introduced_total += n;
                    let own = limits
                        .new_per_day
                        .unwrap_or(settings.new_per_day + account_boost);
                    own + deck_boost
                }
            };
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
        Ok((
            flash_core::queue::Budgets {
                new_total,
                review_total,
                per_deck,
                paced,
            },
            goals,
        ))
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
        check_scope(scope)?;
        let settings = self.store.get_settings(user)?;
        let budgets = self.budgets(user, &settings, now_ms)?;
        let due = Self::due_at(&settings, now_ms);
        let (entries, left_out) =
            self.store
                .queue_entries(user, scope, due, budgets.most_new_per_deck())?;
        let eligible = entries
            .iter()
            .filter(|e| due.is_due(e.phase, e.due_ms))
            .count()
            + left_out as usize;
        let ordered = order_queue(&entries, due, &budgets);
        let held = eligible.saturating_sub(ordered.len()) as u32;
        Ok((ordered, held))
    }
}

/// A search term as the store may run it: trimmed, absent when blank,
/// refused past `bounds::SEARCH` (a term longer than that matches
/// nothing a card side can hold, and LIKE's cost grows with it).
fn bounded_search(search: Option<&str>) -> Result<Option<&str>> {
    let search = search.map(str::trim).filter(|q| !q.is_empty());
    if search.is_some_and(|q| q.len() > crate::bounds::SEARCH) {
        return Err(ServiceError::Malformed(format!(
            "search terms are at most {} bytes",
            crate::bounds::SEARCH
        )));
    }
    Ok(search)
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

/// A side's words in one short line, for a list of versions: its text
/// without markup, cut at `max_chars` with an ellipsis.
pub fn text_preview(html: &str, max_chars: usize) -> String {
    let text = richtext::html_to_text(html);
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max_chars {
        return line;
    }
    let cut: String = line.chars().take(max_chars).collect();
    format!("{}...", cut.trim_end())
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
pub fn now_ms() -> i64 {
    Timestamp::now().as_millisecond()
}
