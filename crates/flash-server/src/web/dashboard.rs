use crate::i18n::filters;

use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::UserId;
use serde::Deserialize;

use super::{act, deck_list_rows, respond, view_event, DeckListRow, DeckRowsCtx, Shell, Via};
use crate::auth::{self, AuthUser, OptionalUser, Theme};
use crate::ext::Slot;
use crate::service::{now_ms, DayCount, HeatCell, MonthSpan};
use crate::state::AppState;

// ---- dashboard ----

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardPage {
    shell: Shell,
    due_now: u32,
    new_available: u32,
    reviewed_today: u32,
    /// Account-wide "more new cards today" already granted.
    boost_today: u32,
    decks: Vec<DeckListRow>,
    rows: DeckRowsCtx,
    /// The "Connect your AI" card, until the user dismisses it.
    show_connect_cta: bool,
    /// Loads `static/timezone.js`, which tells the server the browser's
    /// zone while the account still has the default one.
    ask_timezone: bool,
    card_html: String,
}

/// `/` on the open server: members get the dashboard, everyone else the
/// login page.
pub(super) async fn home(
    State(state): State<AppState>,
    OptionalUser(session): OptionalUser,
    theme: Theme,
    collapsed: auth::CollapsedDecks,
    via: Via,
) -> Response {
    match session {
        Some((user, is_admin)) => {
            dashboard_for(&state, user, is_admin, theme, collapsed, via).await
        }
        None => Redirect::to("/login").into_response(),
    }
}

/// The signed-in dashboard, shared by every owner of `/`.
pub async fn dashboard_for(
    state: &AppState,
    user: UserId,
    is_admin: bool,
    theme: Theme,
    collapsed: auth::CollapsedDecks,
    via: Via,
) -> Response {
    let shell = Shell::new(state, theme, "today", is_admin);
    view_event(state, user, "today", via);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let d = s.dashboard(user, now_ms())?;
        let card_html = app.ext.render(&app, Slot::DashboardCard { user })?;
        let decks = deck_list_rows(d.decks, &collapsed, true);
        Ok(DashboardPage {
            shell,
            due_now: d.counts.due,
            new_available: d.counts.new_available,
            reviewed_today: d.counts.reviewed_today,
            boost_today: d.boost_today,
            rows: DeckRowsCtx::today(&decks),
            decks,
            show_connect_cta: d.show_connect_cta,
            ask_timezone: d.timezone_is_default,
            card_html,
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct TimezoneForm {
    #[serde(deserialize_with = "crate::bounds::timezone")]
    timezone: String,
}

/// The browser's own zone, sent by `static/timezone.js`. It only ever
/// replaces the default, so a zone somebody chose stays theirs.
pub(super) async fn adopt_timezone(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<TimezoneForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.adopt_browser_timezone(user, &form.timezone)
    })
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(response) => response,
    }
}

pub(super) async fn dismiss_connect_cta(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
) -> Response {
    match act(state.services.clone(), move |s| s.dismiss_connect_cta(user)).await {
        Ok(()) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}

// ---- stats ----

#[derive(Template)]
#[template(path = "stats.html")]
struct StatsPage {
    shell: Shell,
    reviews_30d: u32,
    retention: crate::i18n::Words,
    streak: u32,
    longest_streak: u32,
    daily_avg: u32,
    days_learned_pct: u32,
    total_reviews: u32,
    active_cards: u32,
    mature_cards: u32,
    days: Vec<DayCount>,
    upcoming: Vec<DayCount>,
    heat_weeks: Vec<Vec<HeatCell>>,
    heat_months: Vec<MonthSpan>,
    /// The calendar year the heatmap shows; None is the past year.
    heat_year: Option<i32>,
    /// One link per year of history, oldest first, the trailing view
    /// last; empty when the history fits in one year.
    year_links: Vec<YearLink>,
}

struct YearLink {
    label: crate::i18n::Words,
    href: String,
    current: bool,
}

#[derive(Deserialize)]
pub(super) struct StatsQuery {
    year: Option<i32>,
}

pub(super) async fn stats_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Query(query): Query<StatsQuery>,
    via: Via,
) -> Response {
    let shell = Shell::new(&state, theme, "stats", is_admin);
    view_event(&state, user, "stats", via);
    let heavy = match state.heavy(user) {
        Ok(permit) => permit,
        Err(busy) => return busy.into_response(),
    };
    respond(state.services.clone(), move |s| {
        let stats = s.stats(&heavy, now_ms(), query.year)?;
        let last = stats.heat_years.last().copied();
        let year_links = stats
            .heat_years
            .iter()
            .map(|&y| {
                if Some(y) == last {
                    YearLink {
                        label: crate::i18n::t("stats-past-year"),
                        href: "/stats".to_string(),
                        current: stats.heat_year.is_none(),
                    }
                } else {
                    YearLink {
                        label: crate::i18n::Words::formatted(y.to_string()),
                        href: format!("/stats?year={y}"),
                        current: stats.heat_year == Some(y),
                    }
                }
            })
            .collect();
        Ok(StatsPage {
            shell,
            reviews_30d: stats.reviews_30d,
            retention: stats
                .retention_pct
                .map(|p| crate::i18n::ta("stats-percent", &[("n", p.into())]))
                .unwrap_or_else(|| crate::i18n::Words::sign("—")),
            streak: stats.streak,
            longest_streak: stats.longest_streak,
            daily_avg: stats.daily_avg,
            days_learned_pct: stats.days_learned_pct,
            total_reviews: stats.total_reviews,
            active_cards: stats.active_cards,
            mature_cards: stats.mature_cards,
            days: stats.days,
            upcoming: stats.upcoming,
            heat_weeks: stats.heat_weeks,
            heat_months: stats.heat_months,
            heat_year: stats.heat_year,
            year_links,
        })
    })
    .await
}
