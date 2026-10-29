//! 自选学习：主动复习更新 FSRS，自由练习只推进会话。

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, Rating, SessionId, UserId};
use serde::Deserialize;

use super::{act, respond, Shell};
use crate::auth::{AuthUser, Theme};
use crate::i18n::filters;
use crate::service::{now_ms, DiffSpan, ServiceError, Services};
use crate::state::AppState;
use flash_store::CustomStudySummary;

#[derive(Deserialize)]
pub(super) struct SetupQuery {
    deck: Option<i64>,
    tab: Option<String>,
}

struct DeckOption {
    id: i64,
    path: String,
    selected: bool,
}

struct StudySummaryView {
    id: i64,
    mode: String,
    card_order: String,
    deck: Option<String>,
    tags: String,
    position: u32,
    total: u32,
    ended: bool,
}

#[derive(Template)]
#[template(path = "custom_study_setup.html")]
struct SetupPage {
    shell: Shell,
    decks: Vec<DeckOption>,
    tags: Vec<String>,
    active: Vec<StudySummaryView>,
    completed: Vec<StudySummaryView>,
    tab: String,
    available: u32,
}

fn summary_view(
    summary: CustomStudySummary,
    decks: &std::collections::HashMap<DeckId, String>,
) -> StudySummaryView {
    let scope = StudyScope::parse(&summary.scope);
    StudySummaryView {
        id: summary.session.0,
        mode: summary.mode,
        card_order: summary.card_order,
        deck: scope.deck.and_then(|id| decks.get(&id).cloned()),
        tags: scope.tags.join(", "),
        position: summary.position.min(summary.total),
        total: summary.total,
        ended: summary.ended_at.is_some(),
    }
}

pub(super) async fn setup(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Query(query): Query<SetupQuery>,
) -> Response {
    let shell = Shell::new(&state, theme, "study", is_admin);
    respond(state.services.clone(), move |s| {
        let deck_rows = s.list_decks(user, now_ms())?;
        let decks = deck_rows
            .iter()
            .filter(|d| d.archived_at.is_none())
            .map(|d| DeckOption {
                id: d.id.0,
                path: d.path.clone(),
                selected: query.deck == Some(d.id.0),
            })
            .collect();
        let deck_names = deck_rows.iter().map(|d| (d.id, d.path.clone())).collect::<std::collections::HashMap<_, _>>();
        let views = s.store().custom_study_summaries(user)?.into_iter().map(|item| summary_view(item, &deck_names)).collect::<Vec<_>>();
        let tags = s
            .list_tags(user, None)?
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        let available = s
            .store()
            .custom_study_cards(
                user,
                &StudyScope::new(query.deck.map(DeckId), std::iter::empty::<&str>()),
                false,
                u32::MAX,
            )?
            .len() as u32;
        let (active, completed): (Vec<_>, Vec<_>) = views.into_iter().partition(|item| !item.ended);
        Ok(SetupPage {
            shell,
            decks,
            tags,
            active,
            completed,
            tab: query.tab.unwrap_or_else(|| "active".into()),
            available,
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct CountQuery {
    deck: Option<i64>,
    #[serde(default, deserialize_with = "crate::bounds::opt_scope_tag_text")]
    tags: Option<String>,
}

/// 创建表单改变范围时返回实时可选数量，用户手改过数量后只更新提示。
pub(super) async fn count(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Query(query): Query<CountQuery>,
) -> Response {
    let scope = StudyScope::new(
        query.deck.map(DeckId),
        super::cards::split_tags(query.tags.as_deref().unwrap_or("")),
    );
    match act(state.services.clone(), move |s| {
        Ok(s.store()
            .custom_study_cards(user, &scope, false, u32::MAX)?
            .len())
    })
    .await
    {
        Ok(total) => total.to_string().into_response(),
        Err(response) => response,
    }
}

/// 牌组列表的刷题入口：有进行中记录时继续，否则打开已预选牌组的新建表单。
pub(super) async fn deck_entry(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    let deck = DeckId(id);
    match act(state.services.clone(), move |s| {
        // 同一牌组允许保留多轮历史，快捷入口只继续最近一轮进行中的记录。
        let session = s
            .store()
            .custom_study_summaries(user)?
            .into_iter()
            .find(|item| {
                item.ended_at.is_none()
                    && StudyScope::parse(&item.scope).deck == Some(deck)
            })
            .map(|item| item.session);
        Ok(session)
    })
    .await
    {
        Ok(Some(session)) => {
            Redirect::to(&format!("/study/custom/{}", session.0)).into_response()
        }
        Ok(None) => Redirect::to(&format!("/study/custom?deck={id}#custom-study-form"))
            .into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct StartForm {
    deck: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_scope_tag_text")]
    tags: Option<String>,
    mode: String,
    order: String,
    limit: u32,
}

pub(super) async fn start(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<StartForm>,
) -> Response {
    if !matches!(form.order.as_str(), "random" | "ordered") {
        return super::err_response(ServiceError::Malformed("invalid order".into()));
    }
    let deck = if form.deck.is_empty() {
        None
    } else {
        match form.deck.parse::<i64>() {
            Ok(id) => Some(DeckId(id)),
            Err(_) => return super::err_response(ServiceError::Malformed("invalid deck".into())),
        }
    };
    let scope = StudyScope::new(
        deck,
        super::cards::split_tags(form.tags.as_deref().unwrap_or("")),
    );
    let limit = if form.limit == 0 {
        u32::MAX
    } else {
        form.limit
    };
    match act(state.services.clone(), move |s| {
        s.start_custom_study(
            user,
            scope,
            &form.mode,
            form.order == "random",
            limit,
            now_ms(),
        )
    })
    .await
    {
        Ok(id) => Redirect::to(&format!("/study/custom/{}", id.0)).into_response(),
        Err(response) => response,
    }
}

struct CardView {
    id: i64,
    front: String,
    front_html: Option<String>,
    back: String,
    back_html: Option<String>,
    wants_typing: bool,
    typed_diff: Option<Vec<DiffSpan>>,
}

#[derive(Template)]
#[template(path = "custom_study_session.html")]
struct SessionPage {
    shell: Shell,
    session: i64,
    mode: String,
    position: usize,
    total: usize,
    card: Option<CardView>,
    revealed: bool,
}

fn page(
    s: &Services,
    shell: Shell,
    user: UserId,
    id: i64,
    revealed: bool,
    typed: Option<String>,
) -> Result<SessionPage, ServiceError> {
    let head = s.custom_study_head(user, SessionId(id))?;
    let card = match head.current {
        Some(card) if revealed => {
            let r = s.reveal_card(user, card, typed.as_deref())?;
            Some(CardView {
                id: card.0,
                front: r.front,
                front_html: r.front_html,
                back: r.back,
                back_html: r.back_html,
                wants_typing: false,
                typed_diff: r.typed_diff,
            })
        }
        Some(card) => {
            let front = s
                .study_front(user, card)?
                .ok_or_else(|| ServiceError::Malformed("card unavailable".into()))?;
            let view = s.study_card(user, front, None)?;
            Some(CardView {
                id: card.0,
                front: view.front,
                front_html: view.front_html,
                back: String::new(),
                back_html: None,
                wants_typing: view.wants_typing,
                typed_diff: None,
            })
        }
        None => None,
    };
    Ok(SessionPage {
        shell,
        session: id,
        mode: head.mode,
        position: head.position,
        total: head.total,
        card,
        revealed,
    })
}

pub(super) async fn session(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
) -> Response {
    let shell = Shell::new(&state, theme, "study", is_admin);
    respond(state.services.clone(), move |s| {
        page(&s, shell, user, id, false, None)
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RevealForm {
    #[serde(default, deserialize_with = "crate::bounds::opt_typed_answer")]
    typed: Option<String>,
}

pub(super) async fn reveal(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    Form(form): Form<RevealForm>,
) -> Response {
    let shell = Shell::new(&state, theme, "study", is_admin);
    respond(state.services.clone(), move |s| {
        page(&s, shell, user, id, true, form.typed)
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct AdvanceForm {
    card: i64,
    rating: Option<i64>,
}

pub(super) async fn advance(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<AdvanceForm>,
) -> Response {
    let rating = match form.rating {
        Some(value) => match Rating::from_i64(value) {
            Some(rating) => Some(rating),
            None => return super::err_response(ServiceError::Malformed("invalid rating".into())),
        },
        None => None,
    };
    match act(state.services.clone(), move |s| {
        s.advance_custom_study(user, SessionId(id), CardId(form.card), rating, now_ms())
    })
    .await
    {
        Ok(()) => Redirect::to(&format!("/study/custom/{id}")).into_response(),
        Err(response) => response,
    }
}
