use flash_core::queue::StudyScope;
use flash_core::{CardId, GradingMode, Rating, SessionId, UserId};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, Json};

use super::args::{CardIdArgs, SessionIdArgs, StartSessionArgs, SubmitReviewArgs};
use super::out::{
    panel_lang, AnswerOut, DayAheadOut, NextDueOut, PanelGoalOut, PanelOut, SessionStartOut,
    SessionSummaryOut, SubmitOut,
};
use super::{
    authed_client, authed_user, clients, media_out, panel, rich_side, run, site_link, with_panel,
    FlashMcp,
};
use crate::service::{now_ms, DayAhead, NextDue, ReviewOrigin, ServiceError, SessionSurface};
use crate::state::AppState;

/// What an assistant offers someone who has no cards yet. One sentence
/// for every place that can find it out (listing decks, starting a
/// session), so the offer reads the same wherever it comes from.
pub(super) const OFFER_FIRST_CARDS: &str = "The user has no cards yet. Ask what they \
are studying and offer to make their first cards: from notes, slides or a \
file they share, a topic they name, or this conversation. Write them with \
create_cards once they say what to cover (never cards they did not ask \
for), then offer a short session with start_study_session.";

/// Why an empty queue is no reason to quiz from the conversation: said as
/// a fact about the tools, so it reads the same to every assistant.
const ONLY_SESSIONS_COUNT: &str = "Cards asked from the conversation are \
not recorded.";

/// Plain-language guidance for the model when the queue is empty, so it
/// neither declares the user "done" when a daily limit is the real reason,
/// nor calls an empty account caught up, nor falls back to quizzing from
/// chat. `has_cards` is whether the scope names any card at all; `scoped`
/// whether the session was narrowed to a deck or tags.
fn queue_note(
    in_queue: u32,
    held_by_limit: u32,
    has_cards: bool,
    scoped: bool,
    next: Option<&NextDue>,
) -> Option<String> {
    if in_queue > 0 {
        return None;
    }
    let next_line = next
        .map(|n| {
            format!(
                " The next cards come {} ({}); say so.",
                when_words(n.in_days),
                n.cards
            )
        })
        .unwrap_or_default();
    Some(if !has_cards && held_by_limit == 0 {
        if scoped {
            format!(
                "Nothing to study: no card is in this deck or carries these tags. \
                 Tell the user, and offer to make cards for it from their notes, a \
                 file or a topic, written with create_cards once they say what to \
                 cover. {ONLY_SESSIONS_COUNT}"
            )
        } else {
            format!("{OFFER_FIRST_CARDS} {ONLY_SESSIONS_COUNT}")
        }
    } else if held_by_limit > 0 {
        format!(
            "Queue empty for now, but {held_by_limit} more card(s) are held back by \
             today's daily limit (new cards per day, default 20). Tell the user \
             they've finished today's allowance and that {held_by_limit} remain; if \
             they want to keep going, call boost_new_today (with this deck's name \
             if the session was deck-scoped) and then start_study_session again.{next_line} \
             {ONLY_SESSIONS_COUNT}"
        )
    } else {
        format!(
            "Nothing to study right now: every card in scope comes back later. \
             Tell the user they're caught up.{next_line} {ONLY_SESSIONS_COUNT}"
        )
    })
}

/// When the next cards come, in words for the model.
fn when_words(in_days: u32) -> String {
    match in_days {
        0 => "later today".to_string(),
        1 => "tomorrow".to_string(),
        n => format!("in {n} days"),
    }
}

/// What a session whose queue ran out says next. Empty while cards
/// remain or when the scope holds none.
#[derive(Default)]
pub(super) struct Finished {
    /// When the next cards come.
    pub(super) next_due: Option<NextDue>,
    /// The week after today, a day at a time; the panel's only.
    pub(super) week: Vec<DayAhead>,
    /// Whether the panel offers reminders.
    pub(super) reminders_offer: bool,
    pub(super) goal: Option<PanelGoalOut>,
}

/// `Finished` for a scope whose queue ran out. The grade before it is
/// already recorded, so a failure here is logged and left out, never the
/// call's failure.
pub(super) fn finished(
    state: &AppState,
    user: UserId,
    scope: &StudyScope,
    held_by_limit: u32,
    now: i64,
) -> Finished {
    let next_due = state
        .services
        .next_due(user, scope, held_by_limit, now)
        .inspect_err(|e| tracing::warn!("mcp next due: {e}"))
        .ok()
        .flatten();
    let week = state
        .services
        .week_ahead(user, scope, held_by_limit, now)
        .inspect_err(|e| tracing::warn!("mcp week ahead: {e}"))
        .unwrap_or_default();
    let reminders_offer = state
        .ext
        .reminders_offer(state, user)
        .inspect_err(|e| tracing::warn!("mcp reminders offer: {e}"))
        .unwrap_or(false);
    let goal = panel_goal(state, user, scope, now)
        .inspect_err(|e| tracing::warn!("mcp session goal: {e}"))
        .ok()
        .flatten();
    Finished {
        next_due,
        week,
        reminders_offer,
        goal,
    }
}

fn panel_goal(
    state: &AppState,
    user: UserId,
    scope: &StudyScope,
    now: i64,
) -> Result<Option<PanelGoalOut>, ServiceError> {
    let Some(goal) = state.services.session_goal(user, scope, now)? else {
        return Ok(None);
    };
    let store = state.services.store();
    let Some((_, deck)) = store
        .deck_names(user)?
        .into_iter()
        .find(|(id, _)| *id == goal.deck)
    else {
        return Ok(None);
    };
    let tag = panel_lang(store, user, state.i18n.shipped());
    let locale = tag
        .as_deref()
        .and_then(flash_core::locale::UiLocale::by_tag)
        .unwrap_or(flash_core::locale::UiLocale::EN);
    let _reader = crate::i18n::enter(crate::i18n::Tr::new(
        state.i18n.clone(),
        locale,
        tag.as_deref(),
    ));
    let done = crate::web::goal::done(&goal);
    Ok(Some(PanelGoalOut {
        deck,
        percent: goal.view.percent(),
        behind: done.behind,
        head: done.head.to_string(),
        caption: match done.tail {
            Some(tail) => format!("{} {tail}", done.learned),
            None => done.learned.to_string(),
        },
        percent_label: done.percent.to_string(),
        catch_up: if done.behind { done.catch_up } else { 0 },
        study_more: done.study_more.to_string(),
    }))
}

/// Which of its screens the study panel shows.
pub(super) fn panel_state(has_card: bool, held_by_limit: u32, scope_has_cards: bool) -> String {
    match (has_card, held_by_limit, scope_has_cards) {
        (true, _, _) => "card",
        (false, 1.., _) => "held",
        (false, 0, false) => "empty",
        (false, 0, true) => "caught_up",
    }
    .to_string()
}

fn grading_instruction(mode: GradingMode) -> &'static str {
    match mode {
        GradingMode::Silent => {
            "silent: never mention ratings, intervals, or scheduling; confirm \
             correctness naturally and continue"
        }
        GradingMode::Announce => {
            "announce: state your rating in one word after each answer; if the \
             user corrects it, resubmit with their rating"
        }
        GradingMode::SelfGrade => {
            "self: after revealing whether they were right, ask the user to \
             rate the card 1-4 and submit their rating"
        }
    }
}

#[tool_router(router = study_router, vis = "pub(super)")]
impl FlashMcp {
    #[tool(
        title = "Start study session",
        description = "Use this when the user wants to study, review, \
        practice, be quizzed on or drill their flashcards, including cards \
        made moments ago. Starts a spaced-repetition session over the cards \
        due now: all of them, one deck (with its subdecks), or the cards \
        carrying every given tag (list_tags has the names). Progress counts \
        only for cards asked here and graded with submit_review, so ask \
        each card from this session, never from the conversation. To make \
        new cards use create_cards. Returns the first card's front (ask it \
        and wait for the user's answer), how many cards are due, and \
        grading_mode. Render front_html for display or interpret it for \
        speech; present cloze gaps without revealing answers, and never \
        read raw markup or URLs aloud. Fetch media URLs with the connector \
        Bearer token if needed. Follow grading_mode for the whole session. \
        Internal fields (ids, counts) are never read aloud.",
        annotations(
            title = "Start study session",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        output_schema = rmcp::handler::server::tool::schema_for_output::<SessionStartOut>(),
        meta = panel::opens_panel()
    )]
    async fn start_study_session(
        &self,
        Parameters(args): Parameters<StartSessionArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let user = authed_user(&ctx)?;
        let tags: Vec<String> = args
            .tags
            .iter()
            .flatten()
            .chain(args.tag.iter())
            .cloned()
            .collect();
        let scoped = args.deck.is_some() || !tags.is_empty();
        let deck = args.deck.clone();
        let state = self.state.clone();
        // Whether the host says it can render the panel: logged, and kept
        // as the session's surface, so a session studied in the panel can
        // be told from one an assistant ran in words.
        let shows_panel = panel::host_shows_panel(&ctx);
        let surface = if shows_panel {
            SessionSurface::Panel
        } else {
            SessionSurface::Mcp
        };
        let (session, after, ui_lang) = run(&self.services, move |s| {
            let scope = Self::resolve_scope(&s, user, args.deck, args.tag, args.tags)?;
            let now = now_ms();
            let session = s.start_session_from(user, scope.clone(), surface, now)?;
            let after = if session.cards_due == 0 && session.scope_has_cards {
                finished(&state, user, &scope, session.held_by_limit, now)
            } else {
                Finished::default()
            };
            Ok((
                session,
                after,
                panel_lang(s.store(), user, state.i18n.shipped()),
            ))
        })
        .await?;
        let next_due = after.next_due;
        let (client, host) = clients(&ctx);
        tracing::info!(shows_panel, client, host, "mcp study session started");
        let out = SessionStartOut {
            session_id: session.session_id.0,
            cards_due: session.cards_due,
            grading_mode: grading_instruction(session.grading_mode).to_string(),
            first_card: self.question(user, session.first_card.as_ref()).await?,
            note: queue_note(
                session.cards_due,
                session.held_by_limit,
                session.scope_has_cards,
                scoped,
                next_due.as_ref(),
            ),
            next_due: next_due.map(NextDueOut::from),
        };
        let first = session.first_card.as_ref().map(|c| c.card_id);
        let shown = self.panel_card(user, first, session.next_card_id).await?;
        let panel = PanelOut {
            view: "study",
            session_id: session.session_id.0,
            state: panel_state(
                shown.card.is_some(),
                session.held_by_limit,
                session.scope_has_cards,
            ),
            card: shown.card,
            next_card: shown.next_card,
            remaining: session.cards_due,
            reviewed: 0,
            held_by_limit: session.held_by_limit,
            preload: shown.preload,
            recorded: true,
            deck_label: deck.as_deref().map(flash_core::deck_path::display),
            deck,
            tags,
            next_due: next_due.map(NextDueOut::from),
            week: after.week.into_iter().map(DayAheadOut::from).collect(),
            reminders_offer: after.reminders_offer,
            goal: after.goal,
            site_url: site_link(&self.base_url, "/", "panel"),
            ui_lang,
        };
        with_panel(out, Some(panel))
    }

    #[tool(
        title = "Submit review",
        description = "Use this after the user answers the card just asked \
        in a study session: records how well they knew it, which sets when \
        the card comes back, and returns the next card. Grade STRICTLY against the card's back: facts, numbers, doses, \
        and units must match precisely. An imprecise answer is 1 (Again), \
        never 'close enough'; only phrasing may differ. Part of an answer is \
        a miss, so '60' for '60 to 100 bpm' is 1. 1=Again (wrong, blank, or \
        anything the card asks for left out), 2=Hard (the whole answer, but \
        slowly or unsurely), 3=Good (the whole answer), 4=Easy (the whole \
        answer at once). 2, 3 and 4 all record that the user knew it. State \
        the exact answer on a miss, then immediately ask next_card's front. \
        A null next_card ends the session. Wrap up briefly.",
        annotations(
            title = "Submit review",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn submit_review(
        &self,
        Parameters(args): Parameters<SubmitReviewArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SubmitOut>, McpError> {
        let user = authed_user(&ctx)?;
        let client = authed_client(&ctx);
        let rating = Rating::from_i64(args.rating)
            .ok_or_else(|| McpError::invalid_params("rating must be 1-4", None))?;
        let (result, next_due) = run(&self.services, move |s| {
            let session = SessionId(args.session_id);
            let now = now_ms();
            let result = s.submit_review(
                user,
                session,
                CardId(args.card_id),
                rating,
                ReviewOrigin::new("mcp", client.as_deref()),
                now,
            )?;
            let next_due = match (result.next_card.is_none(), s.session_scope(user, session)) {
                (true, Ok(scope)) => s
                    .next_due(user, &scope, result.held_by_limit, now)
                    .inspect_err(|e| tracing::warn!("mcp next due: {e}"))
                    .ok()
                    .flatten(),
                _ => None,
            };
            Ok((result, next_due))
        })
        .await?;
        Ok(Json(SubmitOut {
            recorded: true,
            next_card: self.question(user, result.next_card.as_ref()).await?,
            remaining: result.remaining,
            // A card was just graded here, so the scope has cards.
            note: queue_note(
                result.remaining,
                result.held_by_limit,
                true,
                false,
                next_due.as_ref(),
            ),
        }))
    }

    #[tool(
        title = "Show answer",
        description = "Use this when the user gives up on the card just \
        asked in a study session, or asks for its answer. Returns the card's \
        back and records nothing: tell the user the answer, then call \
        submit_review with rating 1 (Again). To read whole cards outside a \
        session, use list_cards.",
        annotations(
            title = "Show answer",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn show_answer(
        &self,
        Parameters(args): Parameters<CardIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<AnswerOut>, McpError> {
        let user = authed_user(&ctx)?;
        let base = self.base_url.clone();
        let answer = run(&self.services, move |s| {
            let row = s.card(user, CardId(args.card_id))?;
            let back_html = rich_side(row.back_html.as_deref(), &row.back);
            let media = media_out(&s, user, &base, &[&back_html])?;
            Ok(AnswerOut { back_html, media })
        })
        .await?;
        Ok(Json(answer))
    }

    #[tool(
        title = "End session",
        description = "Use this when the user wants to stop studying before \
        the cards run out, or after the last card. Closes the session and \
        returns its summary (cards reviewed, counts by rating); give the \
        user a one-sentence recap. Every review already recorded stays, and \
        ending a session again changes nothing.",
        annotations(
            title = "End session",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_both()
    )]
    async fn end_session(
        &self,
        Parameters(args): Parameters<SessionIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SessionSummaryOut>, McpError> {
        let user = authed_user(&ctx)?;
        let stats = run(&self.services, move |s| {
            s.end_session(user, SessionId(args.session_id), now_ms())
        })
        .await?;
        Ok(Json(SessionSummaryOut {
            reviewed: stats.reviewed,
            again: stats.again,
            hard: stats.hard,
            good: stats.good,
            easy: stats.easy,
            note: format!(
                "The cards come back when they are due; the user can study them any time \
                 in their Flash account: {}",
                site_link(&self.base_url, "/study", "chat")
            ),
        }))
    }
}
