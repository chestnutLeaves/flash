//! Wire types shared across API modules, with the conversions from the
//! `Services` structs they mirror. Kept apart from those structs so the
//! JSON shape can evolve without touching domain code.

use serde::Serialize;

use crate::service::{self, AccountOverview};

/// A tag with the number of the user's live cards carrying it.
#[derive(Debug, Serialize)]
pub struct Tag {
    pub name: String,
    pub cards: u32,
}

impl From<flash_store::TagCount> for Tag {
    fn from(t: flash_store::TagCount) -> Self {
        Self {
            name: t.name,
            cards: t.cards,
        }
    }
}

/// A deck with today's budgeted counts, as the lists show it. A list of
/// these is in tree order: each deck followed by the decks under it.
#[derive(Debug, Serialize)]
pub struct Deck {
    pub id: i64,
    /// The whole path: `Bio::Cells` is the deck `Cells` inside `Bio`.
    pub name: String,
    /// The name to show under its parent.
    pub leaf: String,
    /// How many decks it sits inside; 0 at the top level.
    pub depth: u32,
    pub parent_id: Option<i64>,
    pub description: String,
    /// Counts include the decks under this one, so rows are not to be
    /// added up.
    pub due: u32,
    pub new: u32,
    /// Epoch ms when the deck was archived; null while it is active. An
    /// archived deck's counts are zero.
    pub archived_at: Option<i64>,
    pub goal: Option<Goal>,
}

/// A deck's goal date and how the deck stands toward it, over the deck
/// and the decks under it.
#[derive(Debug, Serialize)]
pub struct Goal {
    /// `YYYY-MM-DD` on the account's calendar.
    pub date: String,
    /// Days until it: 0 on the day, negative once it has passed.
    pub days_left: i64,
    /// New cards behind the plan made when it was set.
    pub behind_by: u32,
    /// New cards a day until the last days before it; null from the day on.
    pub new_per_day: Option<u32>,
    /// The day new cards stop and the last reviews begin.
    pub cutoff: Option<String>,
    pub learned: u32,
    pub total: u32,
}

impl From<crate::service::goals::GoalView> for Goal {
    fn from(g: crate::service::goals::GoalView) -> Self {
        Goal {
            cutoff: g.cutoff().map(|d| d.to_string()),
            date: g.date.to_string(),
            days_left: g.days_left,
            behind_by: g.behind_by,
            new_per_day: g.plan.map(|p| p.pace),
            learned: g.learned,
            total: g.total,
        }
    }
}

impl From<service::DeckRow> for Deck {
    fn from(d: service::DeckRow) -> Self {
        Deck {
            id: d.id.0,
            name: d.name,
            leaf: d.leaf,
            depth: d.depth,
            parent_id: d.parent_id.map(|p| p.0),
            description: d.description,
            due: d.due_count,
            new: d.new_count,
            archived_at: d.archived_at,
            goal: d.goal.map(Into::into),
        }
    }
}

/// A card as the deck lists and the editor see it.
#[derive(Debug, Serialize)]
pub struct Card {
    pub id: i64,
    pub deck_id: i64,
    pub front: String,
    pub back: String,
    pub front_html: Option<String>,
    pub back_html: Option<String>,
    pub tags: Vec<String>,
    /// "cloze 2" / "reversed" / "typed" / null for a plain basic card.
    pub kind: Option<String>,
    pub suspended: bool,
    pub note_id: Option<i64>,
    pub ord: u32,
}

impl From<flash_store::CardRow> for Card {
    fn from(c: flash_store::CardRow) -> Self {
        Card {
            kind: service::card_kind(&c),
            id: c.id.0,
            deck_id: c.deck_id.0,
            front: c.front,
            back: c.back,
            front_html: c.front_html,
            back_html: c.back_html,
            tags: c.tags,
            suspended: c.suspended,
            note_id: c.note_id.map(|n| n.0),
            ord: c.ord,
        }
    }
}

/// A deck's daily-limit overrides beside the account defaults they
/// inherit when null, plus today's boost.
#[derive(Debug, Serialize)]
pub struct DeckLimits {
    pub new_per_day: Option<u32>,
    pub reviews_per_day: Option<u32>,
    pub default_new: u32,
    pub default_reviews: u32,
    pub boost_today: u32,
}

#[derive(Debug, Serialize)]
pub struct DeckDetail {
    pub deck: Deck,
    pub limits: DeckLimits,
    /// Live cards in the deck and the decks under it.
    pub card_count: u32,
    /// Decks under this one, at any depth: what archiving or deleting it
    /// takes along.
    pub subdeck_count: u32,
    /// Languages the cards are in, as the deck's settings name them.
    pub speech_language_hints: Vec<String>,
}

impl From<service::DeckDetail> for DeckDetail {
    fn from(d: service::DeckDetail) -> Self {
        DeckDetail {
            deck: d.deck.into(),
            limits: DeckLimits {
                new_per_day: d.limits.new_per_day,
                reviews_per_day: d.limits.reviews_per_day,
                default_new: d.default_new,
                default_reviews: d.default_reviews,
                boost_today: d.boost_today,
            },
            card_count: d.card_count,
            subdeck_count: d.subdeck_count,
            speech_language_hints: d.speech_language_hints,
        }
    }
}

/// What the editor opens with.
#[derive(Debug, Serialize)]
pub struct EditorSeed {
    pub card_id: i64,
    pub deck_id: i64,
    pub note_id: Option<i64>,
    pub note_type: &'static str,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    pub sibling_count: usize,
    /// The language picked for each field, as a tag; null where none is.
    pub langs: LangsOut,
}

/// The language picked for each field of a card.
#[derive(Debug, Serialize)]
pub struct LangsOut {
    pub front: Option<String>,
    pub back: Option<String>,
}

impl From<service::EditorSeed> for EditorSeed {
    fn from(s: service::EditorSeed) -> Self {
        EditorSeed {
            card_id: s.card_id.0,
            deck_id: s.deck_id.0,
            note_id: s.note_id.map(|n| n.0),
            note_type: s.note_type.as_str(),
            front_html: s.front_html,
            back_html: s.back_html,
            tags: s.tags,
            sibling_count: s.sibling_count,
            langs: LangsOut {
                front: s.langs.front,
                back: s.langs.back,
            },
        }
    }
}

/// The card a study screen shows, plus the media to warm.
#[derive(Debug, Serialize)]
pub struct StudyCard {
    pub card_id: i64,
    pub front: String,
    pub front_html: Option<String>,
    pub wants_typing: bool,
    /// Image ids on this card's hidden back: fetch before the reveal.
    pub preload_media: Vec<i64>,
    /// Image ids on the next card, both sides: fetch at low priority.
    pub prefetch_media: Vec<i64>,
    /// The card after this one, for a client that prepares it while this
    /// one is being answered. Null on the last card.
    pub next_card_id: Option<i64>,
    /// The languages this card's own deck is in, as the deck's settings
    /// name them (`["fr", "en"]`); empty when they name none.
    pub speech_language_hints: Vec<String>,
    /// A downstream extension's fields (`ServerExtension::study_card_json`).
    #[serde(flatten)]
    pub extra: crate::ext::JsonFields,
}

impl From<service::StudyCard> for StudyCard {
    fn from(c: service::StudyCard) -> Self {
        StudyCard {
            card_id: c.card_id.0,
            front: c.front,
            front_html: c.front_html,
            wants_typing: c.wants_typing,
            preload_media: c.preload_media,
            prefetch_media: c.prefetch_media,
            next_card_id: c.next_card.map(|id| id.0),
            speech_language_hints: c.speech_language_hints,
            extra: crate::ext::JsonFields::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DiffSpan {
    pub text: String,
    pub ok: bool,
}

impl From<service::DiffSpan> for DiffSpan {
    fn from(s: service::DiffSpan) -> Self {
        DiffSpan {
            text: s.text,
            ok: s.ok,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Reveal {
    pub back: String,
    pub back_html: Option<String>,
    /// Present when the card asked for typing and something was typed.
    pub typed_diff: Option<Vec<DiffSpan>>,
    /// A downstream extension's fields (`ServerExtension::reveal_json`).
    #[serde(flatten)]
    pub extra: crate::ext::JsonFields,
}

impl From<service::Reveal> for Reveal {
    fn from(r: service::Reveal) -> Self {
        Reveal {
            back: r.back,
            back_html: r.back_html,
            typed_diff: r
                .typed_diff
                .map(|spans| spans.into_iter().map(Into::into).collect()),
            extra: crate::ext::JsonFields::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

impl From<flash_store::SessionStats> for SessionSummary {
    fn from(s: flash_store::SessionStats) -> Self {
        SessionSummary {
            reviewed: s.reviewed,
            again: s.again,
            hard: s.hard,
            good: s.good,
            easy: s.easy,
        }
    }
}

/// The signed-in account as the app sees it. The extension's fields are
/// flattened in through `extra`.
#[derive(Debug, Serialize)]
pub struct Me {
    pub id: i64,
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub theme: String,
    pub grading_mode: &'static str,
    pub login_methods: LoginMethods,
    /// The language the account chose ("es-ES"), which the app shows
    /// itself in; null while it reads what the phone says.
    pub language: Option<&'static str>,
    #[serde(flatten)]
    pub extra: crate::ext::JsonFields,
}

#[derive(Debug, Serialize)]
pub struct LoginMethods {
    pub password: bool,
    pub passkeys: u32,
}

impl From<AccountOverview> for Me {
    fn from(a: AccountOverview) -> Self {
        Me {
            id: a.id.raw(),
            display_name: a.display_name,
            email: a.email,
            role: a.role,
            theme: a.theme,
            grading_mode: a.grading_mode.as_str(),
            login_methods: LoginMethods {
                password: a.has_password,
                passkeys: a.passkeys,
            },
            language: None,
            extra: crate::ext::JsonFields::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Passkey {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// Which ways into the account exist; external providers are the
/// extension's and arrive in its `providers` field.
#[derive(Debug, Serialize)]
pub struct SettingsLoginMethods {
    pub password: bool,
    pub passkeys: Vec<Passkey>,
}

/// Everything the settings screen renders. The extension's fields are
/// flattened in through `extra`.
#[derive(Debug, Serialize)]
pub struct Settings {
    pub display_name: String,
    pub email: Option<String>,
    pub role: String,
    pub theme: String,
    pub grading_mode: &'static str,
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub timezone: String,
    pub day_cutoff_hour: u8,
    pub desired_retention: f32,
    /// Review cards due for their whole study day (true) or at their
    /// exact time.
    pub due_by_day: bool,
    pub login_methods: SettingsLoginMethods,
    pub card_count: u32,
    pub language: LanguageDto,
    #[serde(flatten)]
    pub extra: crate::ext::JsonFields,
}

/// The language the account reads Flash in, everywhere (app, website,
/// emails, the study panel): the one it chose, or null for automatic
/// (what the phone, the browser or the assistant says).
#[derive(Debug, Serialize, Default)]
pub struct LanguageDto {
    /// The chosen language, as Flash files its words ("en", "es-ES",
    /// "es-419"); null when none is chosen.
    pub chosen: Option<&'static str>,
    /// The languages Flash ships, English first, each named in itself.
    pub shipped: Vec<LanguageOption>,
}

#[derive(Debug, Serialize)]
pub struct LanguageOption {
    pub tag: &'static str,
    pub name: &'static str,
}

impl Settings {
    pub fn from_overview(o: service::SettingsOverview) -> Self {
        Settings {
            login_methods: SettingsLoginMethods {
                password: o.has_password,
                passkeys: o
                    .passkeys
                    .iter()
                    .map(|p| Passkey {
                        id: p.id,
                        label: p.label.clone(),
                        created_at: p.created_at,
                        last_used_at: p.last_used_at,
                    })
                    .collect(),
            },
            display_name: o.display_name,
            email: o.email,
            role: o.role,
            theme: o.theme,
            grading_mode: o.grading_mode.as_str(),
            new_per_day: o.new_per_day,
            reviews_per_day: o.reviews_per_day,
            timezone: o.timezone,
            day_cutoff_hour: o.day_cutoff_hour,
            desired_retention: o.desired_retention,
            due_by_day: o.due_by_day,
            card_count: o.card_count,
            language: LanguageDto::default(),
            extra: crate::ext::JsonFields::new(),
        }
    }
}

/// What every successful sign-in and refresh returns.
#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    /// Seconds until `access_token` expires.
    pub expires_in: i64,
    pub user: Me,
}

impl TokenPair {
    pub fn new(access_token: String, refresh_token: String, expires_in: i64, user: Me) -> Self {
        TokenPair {
            access_token,
            refresh_token,
            token_type: "Bearer",
            expires_in,
            user,
        }
    }
}
