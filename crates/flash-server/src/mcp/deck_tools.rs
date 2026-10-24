use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, Json};

use super::args::{
    BoostArgs, CreateDeckArgs, DeckNameArgs, DeleteDeckArgs, ListTagsArgs, RenameDeckArgs,
    RenamedOut, SavedOut, SetLimitsArgs,
};
use super::out::{
    goal_date_arg, AccountLimitsOut, BoostOut, CreatedOut, DeckLimitsOut, DeckOut, DecksOut,
    DeleteOutcome, GoalOut, LimitsOut, TagOut, TagsOut,
};
use super::study_tools::OFFER_FIRST_CARDS;
use super::{authed_user, deck_named, panel, run, FlashMcp};
use crate::service::{now_ms, ServiceError};

#[tool_router(router = deck_router, vis = "pub(super)")]
impl FlashMcp {
    #[tool(
        title = "List decks",
        description = "Use this when the user asks what flashcard decks they \
        have or what is due, or before choosing a deck to study or add \
        cards to. Lists every deck with its due and new card counts. \
        A name is a path: Parent::Child is a subdeck of Parent, and each \
        deck is followed by its subdecks. A deck's counts already include \
        its subdecks, so never add rows up. An archived deck keeps its \
        place, with archived: true and zero counts.",
        annotations(
            title = "List decks",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn list_decks(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<DecksOut>, McpError> {
        let user = authed_user(&ctx)?;
        let (decks, cards) = run(&self.services, move |s| {
            Ok((
                s.list_decks(user, now_ms())?,
                s.store().count_cards(user, None, None)?,
            ))
        })
        .await?;
        let rows = decks
            .iter()
            .map(|d| DeckOut {
                name: d.name.clone(),
                due: d.due_count,
                r#new: d.new_count,
                archived: d.archived_at.is_some(),
                goal: d.goal.as_ref().map(GoalOut::of),
            })
            .collect();
        Ok(Json(DecksOut {
            decks: rows,
            note: (cards == 0).then(|| OFFER_FIRST_CARDS.to_string()),
        }))
    }

    #[tool(
        title = "List tags",
        description = "Use this when the user wants to study one part of \
        their cards, such as a chapter, a lesson or an exam, and the tag \
        names are needed to narrow start_study_session. Lists the user's \
        tags with how many cards carry each, in one deck (and its subdecks) \
        or across every deck.",
        annotations(
            title = "List tags",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn list_tags(
        &self,
        Parameters(args): Parameters<ListTagsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<TagsOut>, McpError> {
        let user = authed_user(&ctx)?;
        let tags = run(&self.services, move |s| {
            let deck = args
                .deck
                .map(|name| Self::deck_by_name(&s, user, &name))
                .transpose()?;
            s.list_tags(user, deck)
        })
        .await?;
        let rows = tags
            .into_iter()
            .map(|t| TagOut {
                name: t.name,
                cards: t.cards,
            })
            .collect();
        Ok(Json(TagsOut { tags: rows }))
    }

    #[tool(
        title = "Create deck",
        description = "Use this when the user asks for an empty deck to fill \
        later, or to set up a structure of decks. Creates a new deck; name \
        it Parent::Child to make a subdeck, and missing parents are created \
        too. Not needed before adding cards, since create_cards creates its \
        deck. When the user is studying for a date (an exam, a test, a \
        deadline), pass goal_date. Refused if a deck with that name already \
        exists.",
        annotations(
            title = "Create deck",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn create_deck(
        &self,
        Parameters(args): Parameters<CreateDeckArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<CreatedOut>, McpError> {
        let user = authed_user(&ctx)?;
        let goal = args
            .goal_date
            .as_deref()
            .map(goal_date_arg)
            .transpose()?
            .flatten();
        run(&self.services, move |s| {
            let now = now_ms();
            if let Some(date) = goal {
                s.check_deck_goal(user, date, now)?;
            }
            let deck = s.create_deck(
                user,
                &args.name,
                args.description.as_deref().unwrap_or(""),
                now,
            )?;
            if let Some(languages) = &args.languages {
                s.declare_deck_languages(user, deck, languages)?;
            }
            if goal.is_some() {
                s.set_deck_goal(user, deck, goal, now)?;
            }
            Ok(deck)
        })
        .await?;
        self.account_event(&ctx, user, "deck_created", None);
        Ok(Json(CreatedOut { created: true }))
    }

    #[tool(
        title = "Delete deck",
        description = "Use this when the user asks to permanently delete a whole deck, every subdeck under it and all their cards. The first call deletes nothing and returns decks_affected, the affected card IDs and count, and a confirm_token: name every affected deck, ask once, then repeat the same deck with the token. Tokens last ten minutes across reconnects; a changed card or subdeck needs a fresh confirmation. To keep the cards but stop studying them, use archive_deck.",
        annotations(
            title = "Delete deck",
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn delete_deck(
        &self,
        Parameters(args): Parameters<DeleteDeckArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<DeleteOutcome>, McpError> {
        let user = authed_user(&ctx)?;
        let client = Self::client_id(&ctx)?;
        let target = format!("deck:{}", args.deck.trim());
        if let Some(token) = args.confirm_token {
            let done = self.confirmed(user, client, target, token).await?;
            self.count_deleted(&ctx, user, &done);
            return Ok(Json(done));
        }
        let deck = run(&self.services, move |s| deck_named(&s, user, &args.deck)).await?;
        Ok(Json(
            self.ask_to_confirm(user, client, target, Some(deck), Vec::new())
                .await?,
        ))
    }

    #[tool(
        title = "Get limits",
        description = "Use this when the user asks how many new cards or \
        reviews they get per day, or why a session stopped while cards \
        remain. Shows today's study limits: the account defaults (new cards \
        per day, max reviews per day, any extra new cards granted today) and \
        every deck's override (null = inherits) with how many new and due \
        cards it can still serve today.",
        annotations(
            title = "Get limits",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn get_limits(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<LimitsOut>, McpError> {
        let user = authed_user(&ctx)?;
        let (settings, boost, rows) = run(&self.services, move |s| {
            let now = now_ms();
            let settings = s.store().get_settings(user)?;
            let boost = s.boost_today(user, None, now)?;
            let mut rows = Vec::new();
            for d in s.list_decks(user, now)? {
                let l = s.deck_limits(user, d.id)?;
                let b = s.boost_today(user, Some(d.id), now)?;
                rows.push(DeckLimitsOut {
                    goal: d.goal.as_ref().map(GoalOut::of),
                    name: d.name,
                    new_per_day: l.new_per_day,
                    reviews_per_day: l.reviews_per_day,
                    boost_today: b,
                    new_available: d.new_count,
                    due: d.due_count,
                });
            }
            Ok((settings, boost, rows))
        })
        .await?;
        Ok(Json(LimitsOut {
            account: AccountLimitsOut {
                new_per_day: settings.new_per_day,
                reviews_per_day: settings.reviews_per_day,
                boost_today: boost,
            },
            decks: rows,
        }))
    }

    #[tool(
        title = "Set limits",
        description = "Use this when the user wants to change how many new \
        cards or reviews they get per day, for every deck or for one, or \
        gives the date they need to know a deck by (an exam, a test, a \
        deadline): goal_date paces that deck toward it. Without a deck it \
        sets the account defaults; with a deck it sets that deck's override \
        (-1 clears it so the deck inherits the account default). Omitted \
        fields are left unchanged. For extra new cards today only, use \
        boost_new_today.",
        annotations(
            title = "Set limits",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn set_limits(
        &self,
        Parameters(args): Parameters<SetLimitsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SavedOut>, McpError> {
        let user = authed_user(&ctx)?;
        for v in [args.new_per_day, args.reviews_per_day]
            .into_iter()
            .flatten()
        {
            if v < -1 {
                return Err(McpError::invalid_params(
                    "limits must be >= 0 (or -1 to clear)",
                    None,
                ));
            }
        }
        let goal = args.goal_date.as_deref().map(goal_date_arg).transpose()?;
        if goal.is_some() && args.deck.is_none() {
            return Err(McpError::invalid_params(
                "goal_date is a deck's: name the deck",
                None,
            ));
        }
        run(&self.services, move |s| match args.deck {
            None => {
                let current = s.store().get_settings(user)?;
                let pick = |v: Option<i64>, cur: u32| match v {
                    Some(n) if n >= 0 => n as u32,
                    _ => cur,
                };
                s.set_daily_limits(
                    user,
                    pick(args.new_per_day, current.new_per_day),
                    pick(args.reviews_per_day, current.reviews_per_day),
                )
            }
            Some(name) => {
                let id = s.store().find_deck_by_name(user, &name)?.ok_or_else(|| {
                    ServiceError::Malformed(format!("no_deck: no deck named '{name}'"))
                })?;
                let current = s.deck_limits(user, id)?;
                let pick = |v: Option<i64>, cur: Option<u32>| match v {
                    None => cur,
                    Some(-1) => None,
                    Some(n) => Some(n as u32),
                };
                let now = now_ms();
                // Both halves checked before either is written.
                if let Some(Some(date)) = goal {
                    s.check_deck_goal(user, date, now)?;
                }
                s.set_deck_limits(
                    user,
                    id,
                    pick(args.new_per_day, current.new_per_day),
                    pick(args.reviews_per_day, current.reviews_per_day),
                )?;
                if let Some(goal) = goal {
                    s.set_deck_goal(user, id, goal, now)?;
                }
                Ok(())
            }
        })
        .await?;
        Ok(Json(SavedOut { saved: true }))
    }

    #[tool(
        title = "Rename or move deck",
        description = "Use this when the user asks to rename a deck, nest it \
        under another, or pull it out to the top level. new_name is the full \
        path: Pharm::Cardio moves Cardio into Pharm (created if missing). \
        Its subdecks, cards and review history follow. Refused if a deck \
        already has that name (decks are never merged) or the target is \
        inside the deck itself.",
        annotations(
            title = "Rename or move deck",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn rename_deck(
        &self,
        Parameters(args): Parameters<RenameDeckArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<RenamedOut>, McpError> {
        let user = authed_user(&ctx)?;
        let out = run(&self.services, move |s| {
            let id = deck_named(&s, user, &args.deck)?;
            s.rename_deck(user, id, &args.new_name)?;
            let detail = s.deck_detail(user, id, now_ms())?;
            Ok(RenamedOut {
                renamed: true,
                name: detail.deck.name,
                decks_renamed: detail.subdeck_count + 1,
            })
        })
        .await?;
        Ok(Json(out))
    }

    #[tool(
        title = "Archive deck",
        description = "Use this when the user is done with a deck (an exam \
        passed) but wants to keep it. Every card, its media and its review \
        history stay, but the deck and its subdecks leave the study queue \
        and the due counts, and start_study_session on it finds nothing. \
        Nothing is deleted, so no confirmation is needed; unarchive_deck \
        reverses it. Cards can still be added to an archived deck by name.",
        annotations(
            title = "Archive deck",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn archive_deck(
        &self,
        Parameters(args): Parameters<DeckNameArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SavedOut>, McpError> {
        let user = authed_user(&ctx)?;
        run(&self.services, move |s| {
            let id = deck_named(&s, user, &args.deck)?;
            s.archive_deck(user, id, now_ms())
        })
        .await?;
        Ok(Json(SavedOut { saved: true }))
    }

    #[tool(
        title = "Unarchive deck",
        description = "Use this when the user wants to study an archived deck \
        again. Puts it back into study: its cards come back with the review \
        history they had, so whatever came due while it was archived is due \
        now, within the daily limits. Its subdecks come back with it, and so \
        do the decks it sits in.",
        annotations(
            title = "Unarchive deck",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn unarchive_deck(
        &self,
        Parameters(args): Parameters<DeckNameArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SavedOut>, McpError> {
        let user = authed_user(&ctx)?;
        run(&self.services, move |s| {
            let id = deck_named(&s, user, &args.deck)?;
            s.unarchive_deck(user, id)
        })
        .await?;
        Ok(Json(SavedOut { saved: true }))
    }

    #[tool(
        title = "More new cards today",
        description = "Use this when the user wants more new cards today than \
        the daily limit allows ('give me 10 more new cards today'). Adds \
        extra new cards for today only: to one deck, or without a deck to \
        the account-wide allowance. Each call adds more, up to 500, and \
        returns the total extra granted today. To change the limit for every \
        day, use set_limits.",
        annotations(
            title = "More new cards today",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_both()
    )]
    async fn boost_new_today(
        &self,
        Parameters(args): Parameters<BoostArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<BoostOut>, McpError> {
        let user = authed_user(&ctx)?;
        let total = run(&self.services, move |s| {
            let deck = match args.deck {
                Some(name) => Some(s.store().find_deck_by_name(user, &name)?.ok_or_else(|| {
                    ServiceError::Malformed(format!("no_deck: no deck named '{name}'"))
                })?),
                None => None,
            };
            s.boost_new_today(user, deck, args.extra, now_ms())
        })
        .await?;
        Ok(Json(BoostOut { boost_today: total }))
    }
}
