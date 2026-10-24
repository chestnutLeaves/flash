//! A deck's goal date on the web: the marker the deck tables draw in
//! their Goal column, the form that sets one (in a dialog over a deck
//! table, and as a card on the deck's settings page, where the dialog's
//! links lead without script), the plan that form shows for the date
//! picked, and the route that saves it. The rules are `goals`'.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::goal::{self, Date};
use flash_core::{DeckId, UserId};
use jiff::ToSpan;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::i18n::{filters, t, ta, Words};
use crate::service::goals::{GoalPreview, GoalView, TIGHT_PACE};
use crate::service::{now_ms, Result, ServiceError, Services};
use crate::state::AppState;
use crate::web::act;

const TABLE_RING: (f64, f64) = (16.0, 2.5);
pub const DONE_RING: (f64, f64) = (44.0, 4.0);
const DAYS_SHOWN: i64 = 60;
const NEAR_DAYS: i64 = 7;

/// A ring that fills as a deck is learned, drawn as two circles.
pub struct Ring {
    pub size: f64,
    pub mid: f64,
    pub r: f64,
    pub stroke: f64,
    /// The filled arc and the whole circle, as `stroke-dasharray`.
    pub dash: String,
}

pub fn ring(percent: u32, (size, stroke): (f64, f64)) -> Ring {
    let r = (size - stroke) / 2.0;
    let whole = 2.0 * std::f64::consts::PI * r;
    let filled = whole * f64::from(percent.min(100)) / 100.0;
    Ring {
        size,
        mid: size / 2.0,
        r,
        stroke,
        dash: format!("{filled:.2} {whole:.2}"),
    }
}

/// A deck's goal as a deck table's Goal column draws it.
pub struct GoalMark {
    /// `on`, `behind` or `done`: the ring's colour.
    pub state: &'static str,
    /// The last week: the words read brighter.
    pub near: bool,
    /// The time left: `12d`, `Dec 18`, `Oct 2027`, `Today`, `Done`.
    pub label: String,
    pub ring: Ring,
    /// The tooltip's first line, which is also the marker's spoken name,
    /// then its others; `act`, when behind, says how to catch up.
    pub head: Words,
    pub lines: Vec<Words>,
    pub act: Option<Words>,
    pub passed: bool,
}

/// `Oct 28`, or `Oct 28, 2026` with `year`.
pub fn day(date: Date, year: bool) -> String {
    match crate::i18n::tr().formats() {
        Some(f) if year => f.day_month_year(date),
        Some(f) => f.day_month(date),
        None if year => date.strftime("%b %-d, %Y").to_string(),
        None => date.strftime("%b %-d").to_string(),
    }
}

fn month_year(date: Date) -> String {
    let month = match crate::i18n::tr().formats() {
        Some(f) => f.month(date),
        None => date.strftime("%b").to_string(),
    };
    ta(
        "goal-left-month-year",
        &[
            ("month", month.into()),
            // The year as text: as a number it would be grouped ("2,027").
            ("year", date.year().to_string().into()),
        ],
    )
    .to_string()
}

fn left(view: &GoalView) -> String {
    if view.passed() {
        return t("goal-left-done").to_string();
    }
    match view.days_left {
        0 => t("goal-left-today").to_string(),
        n if n <= DAYS_SHOWN => ta("goal-left-days", &[("count", n.into())]).to_string(),
        _ if view.date.year() == view.today.year() => day(view.date, false),
        _ => month_year(view.date),
    }
}

fn learned(view: &GoalView) -> Words {
    ta(
        "goal-tip-learned",
        &[
            ("learned", i64::from(view.learned).into()),
            ("total", i64::from(view.total).into()),
        ],
    )
}

pub fn mark(view: &GoalView) -> GoalMark {
    let date = day(view.date, true);
    let behind = view.behind_by > 0 && !view.passed();
    let (head, lines, act) = if view.passed() {
        (
            ta("goal-tip-passed", &[("date", date.clone().into())]),
            Vec::new(),
            None,
        )
    } else if view.days_left == 0 {
        (t("goal-tip-today"), vec![learned(view)], None)
    } else if behind {
        (
            ta("goal-tip-behind", &[("date", date.clone().into())]),
            vec![learned(view)],
            Some(ta(
                "goal-tip-catch-up",
                &[("count", i64::from(view.behind_by).into())],
            )),
        )
    } else {
        let next = match (view.plan, view.cutoff()) {
            (Some(plan), Some(cutoff)) if view.unseen > 0 => ta(
                "goal-tip-pace",
                &[
                    ("count", i64::from(plan.pace).into()),
                    ("date", day(cutoff, false).into()),
                ],
            ),
            _ => ta("goal-tip-final", &[("date", day(view.date, false).into())]),
        };
        (
            ta("goal-tip-on-track", &[("date", date.clone().into())]),
            vec![learned(view), next],
            None,
        )
    };
    GoalMark {
        state: if view.passed() {
            "done"
        } else if behind {
            "behind"
        } else {
            "on"
        },
        near: !view.passed() && view.days_left <= NEAR_DAYS,
        label: left(view),
        ring: ring(view.percent(), TABLE_RING),
        head,
        lines,
        act,
        passed: view.passed(),
    }
}

/// The goal a finished session counted toward, as the done screen shows
/// it: a ring, a line and a caption, and, when the deck is behind, a
/// button that adds the new cards it is behind by and goes on studying.
pub struct GoalDone {
    pub deck_id: i64,
    pub ring: Ring,
    pub behind: bool,
    pub head: Words,
    /// How much of the deck is learned, then the days left or how to
    /// catch up (none on the day itself): two sentences side by side.
    pub learned: Words,
    pub tail: Option<Words>,
    pub percent: Words,
    /// New cards behind, which the button adds (at most a boost's worth).
    pub catch_up: u32,
    pub study_more: Words,
}

pub fn done(goal: &crate::service::goals::SessionGoal) -> GoalDone {
    let view = &goal.view;
    let date = day(view.date, false);
    let behind = view.behind_by > 0;
    let learned = ta(
        "study-goal-learned",
        &[
            ("learned", i64::from(view.learned).into()),
            ("total", i64::from(view.total).into()),
            ("deck", goal.path.clone().into()),
        ],
    );
    let (head, tail) = if view.days_left == 0 {
        (t("goal-tip-today"), None)
    } else if behind {
        (
            ta(
                "study-goal-behind",
                &[
                    ("count", i64::from(view.behind_by).into()),
                    ("date", date.into()),
                ],
            ),
            Some(ta(
                "study-goal-catch-up",
                &[("count", i64::from(view.behind_by).into())],
            )),
        )
    } else {
        (
            ta("study-goal-on-track", &[("date", date.into())]),
            Some(ta(
                "study-goal-days-left",
                &[("count", view.days_left.into())],
            )),
        )
    };
    let catch_up = view.behind_by.min(flash_core::settings::MAX_BOOST);
    GoalDone {
        deck_id: goal.deck.0,
        ring: ring(view.percent(), DONE_RING),
        behind,
        head,
        learned,
        tail,
        percent: ta(
            "study-goal-percent",
            &[("count", i64::from(view.percent()).into())],
        ),
        catch_up,
        study_more: ta(
            "study-goal-study-more",
            &[("count", i64::from(catch_up).into())],
        ),
    }
}

/// The plan the goal form shows for a date: its lines (each with bold
/// parts, from a message of ours), a warning when it asks many new cards
/// a day, or what is wrong with the date.
pub struct GoalPlanView {
    pub lines: Vec<Words>,
    pub tight: Option<Words>,
    pub problem: Option<Words>,
}

impl GoalPlanView {
    fn pick() -> Self {
        Self {
            lines: Vec::new(),
            tight: None,
            problem: Some(t("goal-plan-pick")),
        }
    }

    fn of(preview: &GoalPreview) -> Self {
        let plan = preview.plan;
        let to = day(preview.date, false);
        let first = if preview.unseen == 0 {
            t("goal-plan-seen-html")
        } else {
            ta(
                "goal-plan-pace-html",
                &[
                    ("count", i64::from(plan.pace).into()),
                    ("date", day(preview.cutoff(), false).into()),
                    ("unseen", i64::from(preview.unseen).into()),
                ],
            )
        };
        let last = if plan.window_days > 0 {
            ta(
                "goal-plan-final-html",
                &[
                    ("from", day(preview.cutoff(), false).into()),
                    ("to", to.clone().into()),
                ],
            )
        } else {
            ta("goal-plan-final-close-html", &[("to", to.clone().into())])
        };
        Self {
            lines: vec![
                first,
                last,
                ta("goal-plan-after-html", &[("date", to.into())]),
            ],
            tight: (plan.pace > TIGHT_PACE)
                .then(|| ta("goal-tight", &[("count", i64::from(plan.pace).into())])),
            problem: None,
        }
    }

    fn for_date(s: &Services, user: UserId, deck: DeckId, date: Option<Date>) -> Result<Self> {
        let Some(date) = date else {
            return Ok(Self::pick());
        };
        match s.goal_preview(user, deck, date, now_ms()) {
            Ok(preview) => Ok(Self::of(&preview)),
            Err(ServiceError::Invalid(words)) => Ok(Self {
                lines: Vec::new(),
                tight: None,
                problem: Some(words),
            }),
            Err(e) => Err(e),
        }
    }
}

/// A quick pick under the date field: a label and the date it sets.
pub struct GoalChip {
    pub label: Words,
    pub date: String,
}

/// The goal form, in the dialog or on the settings page.
pub struct GoalFormView {
    pub deck_id: i64,
    pub deck_name: String,
    pub has_goal: bool,
    /// The date field: its value and the range it allows.
    pub value: String,
    pub min: String,
    pub max: String,
    pub chips: Vec<GoalChip>,
    pub plan: GoalPlanView,
    /// Where saving returns to (`today`, `decks`, `deck`, or the
    /// settings page) and, for `deck`, which deck's page.
    pub back: String,
    pub from: i64,
    /// In the dialog, with Cancel; on the settings page without it.
    pub dialog: bool,
}

impl GoalFormView {
    pub fn build(
        s: &Services,
        user: UserId,
        deck: DeckId,
        back: &str,
        from: i64,
        dialog: bool,
    ) -> Result<Self> {
        let now = now_ms();
        let row = s.deck_by_id(user, deck, now)?;
        let today = s.store().get_settings(user)?.study_date(now);
        let goal = row.goal.as_ref().filter(|g| !g.passed()).map(|g| g.date);
        let shown = goal.or_else(|| today.checked_add(1.month()).ok());
        let after = |span: jiff::Span| today.checked_add(span).ok().map(|d| d.to_string());
        let chips = [
            (ta("goal-pick-weeks", &[("count", 2.into())]), 2.weeks()),
            (ta("goal-pick-months", &[("count", 1.into())]), 1.month()),
            (ta("goal-pick-months", &[("count", 3.into())]), 3.months()),
            (ta("goal-pick-months", &[("count", 6.into())]), 6.months()),
            (t("goal-pick-year"), 1.year()),
        ]
        .into_iter()
        .filter_map(|(label, span)| after(span).map(|date| GoalChip { label, date }))
        .collect();
        Ok(Self {
            deck_id: deck.0,
            deck_name: row.path,
            has_goal: goal.is_some(),
            value: shown.map(|d| d.to_string()).unwrap_or_default(),
            min: after(1.day()).unwrap_or_default(),
            max: after(goal::MAX_DAYS_AHEAD.days()).unwrap_or_default(),
            chips,
            plan: GoalPlanView::for_date(s, user, deck, shown)?,
            back: back.to_string(),
            from,
            dialog,
        })
    }
}

#[derive(Template)]
#[template(path = "goal_form_fragment.html")]
struct GoalFormFragment {
    goal: GoalFormView,
}

#[derive(Template)]
#[template(path = "goal_plan.html")]
struct GoalPlanFragment {
    plan: GoalPlanView,
}

#[derive(Deserialize)]
pub struct GoalFormQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    back: Option<String>,
    #[serde(default)]
    from: Option<i64>,
}

/// The goal form for the dialog over a deck table.
pub async fn goal_form(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<GoalFormQuery>,
) -> Response {
    let back = query.back.unwrap_or_default();
    let from = query.from.unwrap_or(0);
    match act(state.services.clone(), move |s| {
        GoalFormView::build(&s, user, DeckId(id), &back, from, true)
    })
    .await
    {
        Ok(goal) => {
            Html(crate::i18n::render_current(&GoalFormFragment { goal }).unwrap_or_default())
                .into_response()
        }
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub struct GoalPlanQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_goal_date")]
    goal_date: Option<String>,
}

/// The plan for the date the form's field holds, as it changes.
pub async fn goal_plan(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Query(query): Query<GoalPlanQuery>,
) -> Response {
    let date = query.goal_date.as_deref().and_then(goal::parse_date);
    match act(state.services.clone(), move |s| {
        GoalPlanView::for_date(&s, user, DeckId(id), date)
    })
    .await
    {
        Ok(plan) => {
            Html(crate::i18n::render_current(&GoalPlanFragment { plan }).unwrap_or_default())
                .into_response()
        }
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub struct GoalSaveForm {
    #[serde(default, deserialize_with = "crate::bounds::opt_goal_date")]
    goal_date: Option<String>,
    /// Present (`1`) to remove the goal.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    clear: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    back: Option<String>,
    #[serde(default)]
    from: Option<i64>,
}

fn destination(id: i64, back: Option<&str>, from: Option<i64>) -> String {
    match back {
        Some("today") => "/".to_string(),
        Some("decks") => "/decks".to_string(),
        Some("deck") => format!("/decks/{}", from.unwrap_or(id)),
        _ => format!("/decks/{id}/settings?saved=goal#goal-card"),
    }
}

/// Sets, moves or removes a deck's goal date.
pub async fn save_goal(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<GoalSaveForm>,
) -> Response {
    let date = if form.clear.is_some() {
        None
    } else {
        match form.goal_date.as_deref().and_then(goal::parse_date) {
            Some(date) => Some(date),
            None => {
                return (axum::http::StatusCode::BAD_REQUEST, t("goal-plan-pick")).into_response()
            }
        }
    };
    let to = destination(id, form.back.as_deref(), form.from);
    match act(state.services.clone(), move |s| {
        s.set_deck_goal(user, DeckId(id), date, now_ms())
    })
    .await
    {
        Ok(()) => Redirect::to(&to).into_response(),
        Err(response) => response,
    }
}
