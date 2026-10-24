use flash_core::{CardId, Rating, SessionId};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, Json};

use super::args::CardIdArgs;
use super::out::{
    DayAheadOut, NextDueOut, PanelAnswerOut, PanelCardOut, PanelCardsPageOut, PanelOut,
};
use super::study_tools::{finished, panel_state, Finished};
use super::{authed_client, authed_user, clients, panel, panel_side, run, site_link, FlashMcp};
use crate::ext::{AccountEvent, Surface};
use crate::service::{now_ms, ReviewOrigin};

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PanelGradeArgs {
    /// The session the panel is showing.
    pub session_id: i64,
    /// The card the user graded.
    pub card_id: i64,
    /// 1=Again 2=Hard 3=Good 4=Easy
    pub rating: i64,
    /// How long the card's front was on screen before the grade, in ms,
    /// as the panel measured it.
    #[serde(default)]
    pub answer_ms: Option<i64>,
    /// The language the host told the panel its person reads (a BCP 47
    /// tag), sent with a page's first grade.
    #[serde(default, deserialize_with = "crate::bounds::opt_locale")]
    pub ui_locale: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PanelArchiveArgs {
    /// The session the panel is showing.
    pub session_id: i64,
    /// The card on screen.
    pub card_id: i64,
    /// True takes the card out of study; false puts it back (Undo).
    pub archived: bool,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PanelCardsArgs {
    /// Up to 20 of the card_ids the panel was given.
    #[serde(deserialize_with = "crate::bounds::ids")]
    pub card_ids: Vec<i64>,
}

/// The two fields behind a card, for the panel's quick edit: the note's,
/// which a reversed or cloze card shows only part of, with media links
/// the panel can load.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelEditSourceOut {
    pub card_id: i64,
    pub note_type: &'static str,
    /// What the two fields are called for this note type, as the card
    /// editor says it: Front and Back, or Text and Extra for a cloze.
    pub labels: [&'static str; 2],
    pub front_html: String,
    pub back_html: String,
    /// The card's own page on the site, for Advanced edit. A plain link:
    /// it signs no one in. The panel may have no other link to the site,
    /// in a host that hands it only what the model reads (Le Chat).
    pub card_url: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PanelSaveEditArgs {
    /// The card the panel is showing.
    pub card_id: i64,
    /// The note's two fields as the quick edit left them.
    #[serde(deserialize_with = "crate::bounds::field_html")]
    pub front_html: String,
    #[serde(deserialize_with = "crate::bounds::field_html")]
    pub back_html: String,
    /// The card the panel expects after it, which the edit may also have
    /// changed (a sibling of the same note).
    #[serde(default)]
    pub next_card_id: Option<i64>,
}

/// The card after a quick edit, as the panel shows it again: its front
/// and the next card's, and its back with the grade times.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelEditOut {
    pub card: PanelCardOut,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_card: Option<PanelCardOut>,
    pub answer: PanelAnswerOut,
    /// Images to fetch ahead, as a study view's.
    pub preload: Vec<String>,
}

/// The study panel's own tools. A host that renders the panel keeps them
/// out of the model's list (`panel::for_panel`); their descriptions
/// say the same for a host that does not.
#[tool_router(router = panel_router, vis = "pub(super)")]
impl FlashMcp {
    #[tool(
        title = "Study panel: show answer",
        description = "Used by Flash's study panel only; never call it \
        yourself (use show_answer). Returns a card's back for the panel.",
        annotations(
            title = "Study panel: show answer",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_show_answer(
        &self,
        Parameters(args): Parameters<CardIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelAnswerOut>, McpError> {
        let user = authed_user(&ctx)?;
        let base = self.base_url.clone();
        let links = self.links.clone();
        let answer = run(&self.services, move |s| {
            let now = now_ms();
            let row = s.card(user, CardId(args.card_id))?;
            let (back_html, back_lang) = panel_side(&s, user, &row, flash_store::CardSide::Back)?;
            let comes_back_in_ms = s
                .grade_preview(user, CardId(args.card_id), now)
                .inspect_err(|e| tracing::warn!("mcp grade preview: {e}"))
                .ok();
            Ok(PanelAnswerOut {
                card_id: args.card_id,
                back_html: links.sign_html(&back_html, &base, user, now),
                back_lang,
                comes_back_in_ms,
            })
        })
        .await?;
        Ok(Json(answer))
    }

    #[tool(
        title = "Study panel: card to edit",
        description = "Used by Flash's study panel only; never call it \
        yourself (use list_cards). Returns the two fields behind a card \
        for the panel's quick edit.",
        annotations(
            title = "Study panel: card to edit",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_edit_source(
        &self,
        Parameters(args): Parameters<CardIdArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelEditSourceOut>, McpError> {
        let user = authed_user(&ctx)?;
        let base = self.base_url.clone();
        let links = self.links.clone();
        let source = run(&self.services, move |s| {
            let now = now_ms();
            let seed = s.editor_seed(user, CardId(args.card_id))?;
            let labels = if seed.note_type == flash_store::notes::NoteType::Cloze {
                ["Text", "Extra"]
            } else {
                ["Front", "Back"]
            };
            Ok(PanelEditSourceOut {
                card_id: args.card_id,
                note_type: seed.note_type.as_str(),
                labels,
                front_html: links.sign_html(&seed.front_html, &base, user, now),
                back_html: links.sign_html(&seed.back_html, &base, user, now),
                card_url: site_link(&base, &format!("/cards/{}", args.card_id), "panel"),
            })
        })
        .await?;
        Ok(Json(source))
    }

    #[tool(
        title = "Study panel: save edit",
        description = "Used by Flash's study panel only; never call it \
        yourself (use update_cards). Saves the panel's quick edit of a \
        card's two fields and returns the card as it now reads.",
        annotations(
            title = "Study panel: save edit",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_save_edit(
        &self,
        Parameters(args): Parameters<PanelSaveEditArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelEditOut>, McpError> {
        let user = authed_user(&ctx)?;
        let base = self.base_url.clone();
        let links = self.links.clone();
        let card = CardId(args.card_id);
        let answer = run(&self.services, move |s| {
            let front = panel::MediaLinks::unsign_html(&args.front_html, &base);
            let back = panel::MediaLinks::unsign_html(&args.back_html, &base);
            let now = now_ms();
            s.save_panel_edit(user, card, front, back, now)?;
            let row = s.card(user, card)?;
            let (back_html, back_lang) = panel_side(&s, user, &row, flash_store::CardSide::Back)?;
            Ok(PanelAnswerOut {
                card_id: card.0,
                back_html: links.sign_html(&back_html, &base, user, now),
                back_lang,
                comes_back_in_ms: s
                    .grade_preview(user, card, now)
                    .inspect_err(|e| tracing::warn!("mcp grade preview: {e}"))
                    .ok(),
            })
        })
        .await?;
        self.ext.account_event(
            &self.services,
            user,
            AccountEvent::new("card_edited", Surface::Mcp)
                .detail(Some("panel"))
                .client(authed_client(&ctx).as_deref()),
        );
        let shown = self
            .panel_card(user, Some(card), args.next_card_id.map(CardId))
            .await?;
        let Some(card) = shown.card else {
            return Err(McpError::internal_error("the edited card is gone", None));
        };
        Ok(Json(PanelEditOut {
            card,
            next_card: shown.next_card,
            answer,
            preload: shown.preload,
        }))
    }

    #[tool(
        title = "Panel: more cards",
        description = "Used by Flash's panel only; never call it yourself \
        (use list_cards). Returns more of the cards the panel is paging \
        through, both sides.",
        annotations(
            title = "Panel: more cards",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_cards(
        &self,
        Parameters(args): Parameters<PanelCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelCardsPageOut>, McpError> {
        let user = authed_user(&ctx)?;
        // At most a page, by the argument's own bound.
        if args.card_ids.is_empty() {
            return Err(McpError::invalid_params("card_ids names no card", None));
        }
        let base = self.base_url.clone();
        let links = self.links.clone();
        let cards = run(&self.services, move |s| {
            let ids: Vec<CardId> = args.card_ids.into_iter().map(CardId).collect();
            Self::shown_cards(&s, &links, &base, user, &ids, &Default::default())
        })
        .await?;
        Ok(Json(PanelCardsPageOut { cards }))
    }

    #[tool(
        title = "Study panel: grade",
        description = "Used by Flash's study panel only; never call it \
        yourself (use submit_review). Records the grade the user tapped \
        and returns the panel's next card.",
        annotations(
            title = "Study panel: grade",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_grade(
        &self,
        Parameters(args): Parameters<PanelGradeArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelOut>, McpError> {
        let user = authed_user(&ctx)?;
        let client = authed_client(&ctx);
        let rating = Rating::from_i64(args.rating)
            .ok_or_else(|| McpError::invalid_params("rating must be 1-4", None))?;
        // What the host told the panel its person reads: the only way a
        // host that sends no language with its calls says it.
        if let Some(tag) = args
            .ui_locale
            .as_deref()
            .and_then(flash_core::locale::normalize)
        {
            let (_, host) = clients(&ctx);
            tracing::info!(host, locale = %tag, "mcp panel locale");
            crate::account_locale::keep_often_seen(
                &self.services,
                user,
                flash_store::LocaleSource::Host,
                vec![tag],
                now_ms(),
            )
            .await;
        }
        let state = self.state.clone();
        let (recorded, result, reviewed, after) = run(&self.services, move |s| {
            let session = SessionId(args.session_id);
            let now = now_ms();
            let (recorded, result) = s.submit_review_if_queued(
                user,
                session,
                CardId(args.card_id),
                rating,
                ReviewOrigin::new("mcp", client.as_deref()).answered_in(args.answer_ms),
                now,
            )?;
            let reviewed = s.store().session_stats(user, session)?.reviewed;
            let after = if result.next_card.is_none() {
                match s.session_scope(user, session) {
                    Ok(scope) => finished(&state, user, &scope, result.held_by_limit, now),
                    Err(e) => {
                        tracing::warn!("mcp session scope: {e}");
                        Finished::default()
                    }
                }
            } else {
                Finished::default()
            };
            Ok((recorded, result, reviewed, after))
        })
        .await?;
        let shown = result.next_card.as_ref().map(|c| c.card_id);
        let shown = self.panel_card(user, shown, result.next_card_id).await?;
        Ok(Json(PanelOut {
            view: "study",
            session_id: args.session_id,
            // A session the panel is grading in has cards.
            state: panel_state(shown.card.is_some(), result.held_by_limit, true),
            card: shown.card,
            next_card: shown.next_card,
            remaining: result.remaining,
            reviewed,
            held_by_limit: result.held_by_limit,
            preload: shown.preload,
            recorded,
            deck_label: None,
            deck: None,
            tags: Vec::new(),
            next_due: after.next_due.map(NextDueOut::from),
            week: after.week.into_iter().map(DayAheadOut::from).collect(),
            reminders_offer: after.reminders_offer,
            goal: after.goal,
            site_url: site_link(&self.base_url, "/", "panel"),
            ui_lang: None,
        }))
    }

    #[tool(
        title = "Study panel: archive",
        description = "Used by Flash's study panel only; never call it \
        yourself. Takes the card on screen out of study after the user \
        tapped Archive (or puts it back on Undo) and returns the panel's \
        next card.",
        annotations(
            title = "Study panel: archive",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_archive(
        &self,
        Parameters(args): Parameters<PanelArchiveArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelOut>, McpError> {
        let user = authed_user(&ctx)?;
        let state = self.state.clone();
        let card = CardId(args.card_id);
        let (head, reviewed, after) = run(&self.services, move |s| {
            let session = SessionId(args.session_id);
            let scope = s.session_scope(user, session)?;
            let now = now_ms();
            s.set_card_archived(user, card, args.archived, now)?;
            let mut head = s.session_head(user, session, now)?;
            if !args.archived {
                if let Some(front) = s.study_front(user, card)? {
                    let after = match &head.next_card {
                        Some(f) if f.card_id != card => Some(f.card_id),
                        _ => head.next_card_id,
                    };
                    head.next_card = Some(front);
                    head.next_card_id = after;
                }
            }
            let reviewed = s.store().session_stats(user, session)?.reviewed;
            let after = if head.next_card.is_none() {
                finished(&state, user, &scope, head.held_by_limit, now)
            } else {
                Finished::default()
            };
            Ok((head, reviewed, after))
        })
        .await?;
        let event = if args.archived {
            "cards_archived"
        } else {
            "cards_unarchived"
        };
        self.ext.account_event(
            &self.services,
            user,
            AccountEvent::new(event, Surface::Mcp)
                .detail(Some("panel"))
                .amount(Some(1))
                .client(authed_client(&ctx).as_deref()),
        );
        let shown = head.next_card.as_ref().map(|c| c.card_id);
        let shown = self.panel_card(user, shown, head.next_card_id).await?;
        Ok(Json(PanelOut {
            view: "study",
            session_id: args.session_id,
            state: panel_state(shown.card.is_some(), head.held_by_limit, true),
            card: shown.card,
            next_card: shown.next_card,
            remaining: head.remaining,
            reviewed,
            held_by_limit: head.held_by_limit,
            preload: shown.preload,
            recorded: true,
            deck_label: None,
            deck: None,
            tags: Vec::new(),
            next_due: after.next_due.map(NextDueOut::from),
            week: after.week.into_iter().map(DayAheadOut::from).collect(),
            reminders_offer: after.reminders_offer,
            goal: after.goal,
            site_url: site_link(&self.base_url, "/", "panel"),
            ui_lang: None,
        }))
    }

    #[tool(
        title = "Study panel: remind me",
        description = "Used by Flash's study panel only; never call it \
        yourself. Turns on reminders when cards are due, at this time of \
        day, after the user tapped the panel's offer.",
        annotations(
            title = "Study panel: remind me",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_panel()
    )]
    async fn panel_reminders(
        &self,
        Parameters(args): Parameters<PanelRemindersArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<PanelRemindersOut>, McpError> {
        let user = authed_user(&ctx)?;
        let state = self.state.clone();
        let message = run(&self.services, move |s| {
            // The browser's zone becomes the account's while it still has
            // the default, so the reminder and the study day both use it.
            if let Some(tz) = args.timezone.as_deref() {
                if let Err(e) = s.adopt_browser_timezone(user, tz) {
                    tracing::warn!("mcp panel zone: {e}");
                }
            }
            state.ext.turn_on_reminders(&state, user, "panel", now_ms())
        })
        .await?;
        Ok(Json(PanelRemindersOut {
            on: message.is_some(),
            message: message.map(crate::i18n::Words::into_string),
        }))
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct PanelRemindersArgs {
    /// The browser's IANA time zone, e.g. `America/New_York`.
    #[serde(default, deserialize_with = "crate::bounds::opt_timezone")]
    pub timezone: Option<String>,
}

/// What the panel shows after its reminders offer was tapped.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct PanelRemindersOut {
    /// Whether reminders are on now.
    pub on: bool,
    /// What was set up, for the person to read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
