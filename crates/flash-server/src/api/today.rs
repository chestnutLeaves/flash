//! Today: the dashboard, the app-icon badge, the account-wide boost, and
//! the "Connect your AI" card.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use flash_core::queue::StudyScope;
use serde::{Deserialize, Serialize};

use super::dto::Deck;
use super::{app_platform, call, ApiError, ApiJson, ApiResult, BearerUser};
use crate::ext::{AccountEvent, Surface};
use crate::service::{now_ms, ServiceError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/today", get(today))
        .route("/badge", get(badge))
        .route("/study/boost", post(boost))
        .route("/connect", get(connect))
        .route("/connect/dismiss", post(dismiss_connect_cta))
}

/// What the Connect screen needs: the MCP address people paste into
/// their assistant, and the Claude Code command.
#[derive(Serialize)]
pub struct Connect {
    pub mcp_url: String,
    pub claude_code_cmd: String,
}

async fn connect(State(state): State<AppState>, user: BearerUser) -> ApiResult<Connect> {
    let mcp_url = state.site.mcp_url();
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("view", Surface::Mobile).detail(Some("connect")),
    );
    Ok(Json(Connect {
        claude_code_cmd: format!("claude mcp add --transport http flash {mcp_url}"),
        mcp_url,
    }))
}

#[derive(Serialize)]
pub struct Today {
    pub due: u32,
    pub new_available: u32,
    pub reviewed_today: u32,
    /// Account-wide "more new cards today" already granted.
    pub boost_today: u32,
    /// What the app icon should show: cards due now.
    pub badge: u32,
    pub decks: Vec<Deck>,
    pub show_connect_cta: bool,
}

/// The app's first screen, which reports an `app_session` account event.
async fn today(
    State(state): State<AppState>,
    user: BearerUser,
    headers: HeaderMap,
) -> ApiResult<Today> {
    state.ext.account_event(
        &state.services,
        user.id,
        AccountEvent::new("app_session", Surface::Mobile).client(app_platform(&headers)),
    );
    let d = call(state.services.clone(), move |s| {
        let d = s.dashboard(user.id, now_ms())?;
        Ok::<_, ServiceError>(d)
    })
    .await?;
    Ok(Json(Today {
        due: d.counts.due,
        new_available: d.counts.new_available,
        reviewed_today: d.counts.reviewed_today,
        boost_today: d.boost_today,
        badge: d.counts.due,
        decks: d.decks.into_iter().map(Into::into).collect(),
        show_connect_cta: d.show_connect_cta,
    }))
}

/// The number the app puts on its icon: cards due right now, the same
/// figure the Today page carries.
#[derive(Serialize)]
pub struct Badge {
    pub badge: u32,
}

async fn badge(State(state): State<AppState>, user: BearerUser) -> ApiResult<Badge> {
    let counts = call(state.services.clone(), move |s| {
        s.queue_counts(user.id, &StudyScope::all(), now_ms())
    })
    .await?;
    Ok(Json(Badge { badge: counts.due }))
}

#[derive(Deserialize)]
pub struct BoostBody {
    pub extra: u32,
}

#[derive(Serialize)]
pub struct Boosted {
    /// The total boost now active for today.
    pub boost_today: u32,
}

async fn boost(
    State(state): State<AppState>,
    user: BearerUser,
    ApiJson(body): ApiJson<BoostBody>,
) -> ApiResult<Boosted> {
    let boost_today = call(state.services.clone(), move |s| {
        s.boost_new_today(user.id, None, body.extra, now_ms())
    })
    .await?;
    Ok(Json(Boosted { boost_today }))
}

async fn dismiss_connect_cta(
    State(state): State<AppState>,
    user: BearerUser,
) -> Result<StatusCode, ApiError> {
    call(state.services.clone(), move |s| {
        s.dismiss_connect_cta(user.id)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
