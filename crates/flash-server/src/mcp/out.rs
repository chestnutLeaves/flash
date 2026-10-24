use flash_core::UserId;
use rmcp::ErrorData as McpError;

use super::panel::MediaLinks;
use crate::service::{DayAhead, NextDue};

// ---- tool output types. Returning Json<T> makes the #[tool] macro derive
// each tool's outputSchema from these via schemars, and rmcp emits the
// payload as structuredContent plus the same JSON as a text block (so
// clients without structured-content support see exactly what they did
// before). Doc comments become schema descriptions.

/// Rich question only. Editable source is deliberately absent during study.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct CardFrontOut {
    pub card_id: i64,
    pub front_html: String,
    pub media: Vec<MediaOut>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SessionStartOut {
    pub session_id: i64,
    /// Cards in this session's queue right now.
    pub cards_due: u32,
    /// How to report grades for the whole session; follow it exactly.
    pub grading_mode: String,
    /// First card to ask, or null when nothing is due.
    pub first_card: Option<CardFrontOut>,
    /// Present only when the queue is empty: why, and what to do next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Present only when the queue is empty and more cards are coming:
    /// when the next ones are due.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due: Option<NextDueOut>,
}

/// When the next cards come, for a session with nothing left to study.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct NextDueOut {
    /// When the first of them is due, in ms since the epoch.
    pub at_ms: i64,
    /// Study days away: 1 is tomorrow, 0 later today.
    pub in_days: u32,
    /// How many come that day.
    pub cards: u32,
}

impl From<NextDue> for NextDueOut {
    fn from(n: NextDue) -> Self {
        NextDueOut {
            at_ms: n.at_ms,
            in_days: n.in_days,
            cards: n.cards,
        }
    }
}

/// One study day of the week ahead.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct DayAheadOut {
    /// When the study day starts, in ms since the epoch.
    pub at_ms: i64,
    /// How many cards come due that day.
    pub cards: u32,
}

impl From<DayAhead> for DayAheadOut {
    fn from(d: DayAhead) -> Self {
        DayAheadOut {
            at_ms: d.at_ms,
            cards: d.cards,
        }
    }
}

// The model is not told that a study panel is showing. A host saying it
// can render one is not a host drawing ours: a host holding a tool list
// from before the panel drew nothing, and its model said "your card is
// in the study panel" over an empty turn, even when the instruction was
// made conditional on a panel being shown. The
// model asks the card in chat as it always has; where the panel does
// show, the question appears twice, which is harmless.

/// The study panel's view of a session at one moment, in `_meta` of
/// start_study_session's result and as the whole result of its own tools.
/// Card HTML here carries signed media links the panel can load.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelOut {
    /// Which of the panel's views this is: always `study`.
    pub view: &'static str,
    pub session_id: i64,
    /// `card` (one to show), `held` (today's limits hold the rest back),
    /// `caught_up` (nothing due), or `empty` (no cards in the scope at all).
    pub state: String,
    pub card: Option<PanelCardOut>,
    /// The card expected after this one, its front only, so the panel can
    /// show it the moment a grade is tapped and record the grade behind
    /// it. The grade's own answer still says which card really comes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_card: Option<PanelCardOut>,
    /// Cards in the queue, this one included.
    pub remaining: u32,
    /// Reviews recorded in this session so far, in the panel or the chat:
    /// with `remaining`, the panel's progress ("2 / 10").
    pub reviewed: u32,
    /// Cards today's limits hold back.
    pub held_by_limit: u32,
    /// Signed links to warm: this card's back and the next card's front.
    pub preload: Vec<String>,
    /// False only from panel_grade, when the card had already been graded
    /// elsewhere (in the chat) and this grade was not recorded.
    pub recorded: bool,
    /// The deck as the site shows it (`Cardio › ECG`), for the panel's
    /// header; start only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck_label: Option<String>,
    /// The session's deck and tags as asked for: the tags are the header's
    /// chips, and both let the panel start the session again after
    /// allowing more new cards; start only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    // `default` on each field left out when empty keeps it out of the
    // output schema's `required`: a host that checks results against the
    // schema rejects every grade otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// When the next cards come, once the queue is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due: Option<NextDueOut>,
    /// The seven study days after today and the cards due on each, once
    /// the queue is empty, for the finished screen's week.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub week: Vec<DayAheadOut>,
    /// Whether the finished screen offers reminders (panel_reminders):
    /// only once the queue is empty, and only when the server can send
    /// this account reminders it has not turned on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reminders_offer: bool,
    /// The goal the session counted toward, once the queue is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<PanelGoalOut>,
    /// The site's front page, for the finished screen's "Study on the
    /// web": the cards live there too.
    pub site_url: String,
    /// The language the account reads Flash in (`panel_lang`); start
    /// only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_lang: Option<String>,
}

/// The goal a finished session counted toward, for the panel's finished
/// screen: the web's done-screen block, its words in the account's
/// language.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelGoalOut {
    /// The deck the goal is on, as boost_new_today names it.
    pub deck: String,
    /// How much of the deck is learned, out of 100: the ring.
    pub percent: u32,
    /// Behind its plan: the ring in amber.
    pub behind: bool,
    /// "On track for Dec 18", "15 cards behind for Dec 18".
    pub head: String,
    /// How much is learned, then the days left or how to catch up.
    pub caption: String,
    /// The share learned as text ("40%"), shown when there is no button.
    pub percent_label: String,
    /// New cards "Study N more" adds to the deck today; 0 for no button.
    pub catch_up: u32,
    /// The button's words.
    pub study_more: String,
}

/// What `FlashMcp::panel_card` gives a study view: the card on screen,
/// the one expected after it, and the images to warm.
#[derive(Default)]
pub(super) struct PanelCards {
    pub(super) card: Option<PanelCardOut>,
    pub(super) next_card: Option<PanelCardOut>,
    pub(super) preload: Vec<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelCardOut {
    pub card_id: i64,
    pub front_html: String,
    /// The front's language, for its `lang`; absent when nothing says it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub front_lang: String,
}

/// The back of a card for the panel, its media links signed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelAnswerOut {
    pub card_id: i64,
    pub back_html: String,
    /// The back's language, for its `lang`; absent when nothing says it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub back_lang: String,
    /// How long until the card comes back after each grade, Again, Hard,
    /// Good, Easy, in ms: what the grade buttons say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comes_back_in_ms: Option<[i64; 4]>,
}

/// How many cards the panel gets at once: the first page rides in the
/// tool's result, and `panel_cards` brings the rest as the user pages on,
/// as many ids as any request may echo back.
pub(super) const PANEL_PAGE: usize = crate::bounds::IDS_PER_CALL;

/// The panel's view of the cards create_cards or update_cards just wrote,
/// in `_meta` of their results.
#[derive(serde::Serialize)]
pub struct PanelCardsOut {
    /// Which of the panel's views this is: always `cards`.
    pub view: &'static str,
    /// `created` or `edited`.
    pub mode: &'static str,
    /// The deck as the site shows it (`Cardio › ECG`), when every card is
    /// in one deck.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck_label: Option<String>,
    /// That deck's full name, which the panel studies on request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    /// That deck's page on the site, where every card in it is listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck_url: Option<String>,
    /// Every card to page through, in order; `cards` holds the first page.
    pub card_ids: Vec<i64>,
    pub cards: Vec<PanelShownCard>,
    /// The language the account reads Flash in (`panel_lang`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_lang: Option<String>,
}

pub(super) struct PanelSite {
    pub(super) links: MediaLinks,
    pub(super) base: String,
    pub(super) shipped: Vec<flash_core::locale::UiLocale>,
}

pub(super) fn panel_lang(
    store: &flash_store::Store,
    user: UserId,
    shipped: &[flash_core::locale::UiLocale],
) -> Option<String> {
    let (tag, _) = store.account_locale(user).ok().flatten()?;
    let zone = store.get_settings(user).ok().map(|s| s.timezone);
    flash_core::locale::negotiate_tags(&[&tag], zone.as_deref(), shipped)
        .map(|n| n.locale.tag().to_string())
}

/// One card whole, both sides, for the panel to show; its media links
/// signed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelShownCard {
    pub card_id: i64,
    pub front_html: String,
    pub back_html: String,
    /// Each side's language, for its `lang`; absent when nothing says it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub front_lang: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub back_lang: String,
    pub tags: Vec<String>,
    /// An edited card's sides from before the edit, each only when it
    /// changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<PanelBefore>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelBefore {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub front_html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_html: Option<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelCardsPageOut {
    pub cards: Vec<PanelShownCard>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SubmitOut {
    pub recorded: bool,
    /// Next card to ask immediately, or null when the session is finished.
    pub next_card: Option<CardFrontOut>,
    /// Cards left in the queue.
    pub remaining: u32,
    /// Present only when the queue is empty: why, and what to do next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct AnswerOut {
    pub back_html: String,
    pub media: Vec<MediaOut>,
}

/// Session summary: total reviewed and counts by rating.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SessionSummaryOut {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
    /// Where the user can study next time, on their own.
    pub note: String,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct DeckOut {
    pub name: String,
    /// Reviews due now.
    pub due: u32,
    /// New cards available today.
    pub r#new: u32,
    /// Archived: kept whole but out of study; its counts are zero.
    pub archived: bool,
    /// Present when the deck has a goal date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalOut>,
}

/// A deck's goal date and how the deck stands toward it, over the deck
/// and its subdecks.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct GoalOut {
    /// The date, YYYY-MM-DD, on the user's calendar.
    pub date: String,
    /// Days until it: 0 on the day, negative once it has passed.
    pub days_left: i64,
    /// The date has passed; the deck is studied as any other again.
    pub passed: bool,
    /// Keeping to the plan made when the date was set.
    pub on_track: bool,
    /// New cards behind that plan; studying that many more today catches up.
    pub behind_by: u32,
    /// New cards a day until the last days before the date, which are kept
    /// for review; null on the day and after.
    pub new_per_day: Option<u32>,
    /// Cards past their first learning steps.
    pub learned: u32,
    /// Cards in the deck and its subdecks.
    pub total: u32,
}

impl GoalOut {
    pub(super) fn of(view: &crate::service::goals::GoalView) -> Self {
        Self {
            date: view.date.to_string(),
            days_left: view.days_left,
            passed: view.passed(),
            on_track: !view.passed() && view.behind_by == 0,
            behind_by: view.behind_by,
            new_per_day: view.plan.map(|p| p.pace),
            learned: view.learned,
            total: view.total,
        }
    }
}

pub(super) fn goal_date_arg(text: &str) -> Result<Option<flash_core::goal::Date>, McpError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    flash_core::goal::parse_date(text)
        .map(Some)
        .ok_or_else(|| McpError::invalid_params("goal_date must be YYYY-MM-DD", None))
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct DecksOut {
    pub decks: Vec<DeckOut>,
    /// Present only when the account has no cards at all: what to offer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct TagOut {
    pub name: String,
    /// Cards carrying it.
    pub cards: u32,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct TagsOut {
    pub tags: Vec<TagOut>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct MediaOut {
    pub media_id: i64,
    pub url: String,
    pub mime: String,
    pub filename: String,
    pub size: u64,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SourceOut {
    /// Non-null when a legacy import lacks a lossless editable source.
    pub read_only_reason: Option<String>,
    pub note_id: Option<i64>,
    pub note_type: String,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    pub sibling_card_ids: Vec<i64>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct CardOut {
    pub card_id: i64,
    pub deck_id: i64,
    pub ord: u32,
    pub cloze_index: Option<u32>,
    pub type_answer: Option<String>,
    pub front_html: String,
    pub back_html: String,
    pub source: SourceOut,
    pub media: Vec<MediaOut>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct CardsOut {
    pub cards: Vec<CardOut>,
    pub total_count: u32,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct AccountLimitsOut {
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    /// Extra new cards granted for today.
    pub boost_today: u32,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct DeckLimitsOut {
    pub name: String,
    /// Deck override; null = inherits the account default.
    pub new_per_day: Option<u32>,
    /// Deck override; null = inherits the account default.
    pub reviews_per_day: Option<u32>,
    /// Extra new cards granted to this deck for today.
    pub boost_today: u32,
    /// New cards this deck can still serve today.
    pub new_available: u32,
    /// Reviews due now in this deck.
    pub due: u32,
    /// Present when the deck has a goal date: while it is ahead, the
    /// goal's pace stands in for new_per_day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<GoalOut>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct LimitsOut {
    pub account: AccountLimitsOut,
    pub decks: Vec<DeckLimitsOut>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct BoostOut {
    /// Total extra new cards granted today (after this call).
    pub boost_today: u32,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct CreatedOut {
    pub created: bool,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct MutationOut {
    pub source_ids: Vec<i64>,
    pub card_ids: Vec<i64>,
    pub created: u32,
    pub updated: u32,
    pub deck_totals: Vec<(i64, u32)>,
    /// How to reply when the cards are already on screen; on new cards,
    /// also where else the user can study them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What update_cards (and a restore) wrote: create_cards' fields, and
/// the earlier versions they kept.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct UpdateOut {
    #[serde(flatten)]
    pub mutation: MutationOut,
    /// One per source in source_ids: the earlier version of that source
    /// this edit kept, which restore_cards can put back; null when the
    /// edit changed nothing.
    pub revision_ids: Vec<Option<i64>>,
}

/// restore_cards' answer: the card's earlier versions when asked for
/// them, or what a restore wrote.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct RestoreOut {
    /// The card's earlier versions, newest first; empty after a restore
    /// and for a card that was never edited.
    pub revisions: Vec<RevisionOut>,
    /// After a restore, the same fields update_cards returns; absent
    /// when listing.
    #[serde(flatten)]
    pub restored: Option<UpdateOut>,
}

/// One earlier version of a card's source.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct RevisionOut {
    /// Pass as revision_id to restore this version.
    pub revision_id: i64,
    /// When an edit replaced this version, ms since the epoch.
    pub replaced_at_ms: i64,
    /// What made that edit: web, mcp, api, sync or restore.
    pub replaced_by: String,
    pub note_type: String,
    /// The source's front as plain text, shortened.
    pub front_preview: String,
    /// The source's back as plain text, shortened.
    pub back_preview: String,
    pub tags: Vec<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct DeleteOutcome {
    pub deleted: bool,
    pub pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm_token: Option<String>,
    pub cards_affected: u32,
    pub card_ids: Vec<i64>,
    /// A deck deletion: the decks that go, by name, the deck first and
    /// then the decks under it. Empty for a deletion of cards.
    pub decks_affected: Vec<String>,
    pub deck_totals: Vec<(i64, u32)>,
    pub expires_ms: Option<i64>,
    pub note: Option<String>,
}
