use flash_core::{CardId, RevisionId};
use flash_store::{CardFilter, CardStatus};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_router, ErrorData as McpError, Json};

use super::args::{
    speech_blocks, CreateCardsArgs, DeleteCardsArgs, ListCardsArgs, RestoreCardsArgs,
    UpdateCardsArgs,
};
use super::out::{
    CardOut, CardsOut, DeleteOutcome, MutationOut, PanelCardsOut, RestoreOut, RevisionOut,
    SourceOut, UpdateOut,
};
use super::{
    authed_user, deck_named, media_out, panel, rich_side, run, site_link, with_panel, FlashMcp,
};
use crate::service::{now_ms, ServiceError};

/// The most card JSON one `list_cards` page carries. The result travels
/// both as text and as structured content, and clients cap what a tool
/// may return (commonly some 25,000 tokens of text), so a page stops well
/// short of that and the cursor carries on.
const LIST_PAGE_BYTES: usize = 64 * 1024;

fn update_out(saved: flash_store::McpMutationReceipt) -> UpdateOut {
    let revision_ids = saved.revisions.iter().map(|r| r.map(|id| id.0)).collect();
    UpdateOut {
        mutation: mutation_out(saved),
        revision_ids,
    }
}

fn revision_out(r: flash_store::NoteRevision) -> RevisionOut {
    RevisionOut {
        revision_id: r.id.0,
        replaced_at_ms: r.at,
        replaced_by: r.source.as_str().into(),
        note_type: r.note_type.as_str().into(),
        front_preview: crate::service::text_preview(&r.front_html, 120),
        back_preview: crate::service::text_preview(&r.back_html, 120),
        tags: r.tags,
    }
}

fn mutation_out(saved: flash_store::McpMutationReceipt) -> MutationOut {
    MutationOut {
        source_ids: saved.sources.iter().map(|(id, _)| id.0).collect(),
        card_ids: saved
            .sources
            .iter()
            .flat_map(|(_, ids)| ids.iter().map(|id| id.0))
            .collect(),
        created: saved.created,
        updated: saved.updated,
        deck_totals: saved.deck_totals,
        note: None,
    }
}

/// What the model reads when the cards it just wrote are on screen in the
/// panel, so its reply confirms them instead of repeating them. Said only
/// to a host that says it renders the panel (ChatGPT, Claude and Grok all do; Grok
/// puts it below the reply, so the wording names no position); a host
/// that does not gets the cards as text alone.
const CARDS_SHOWN: &str = "The user sees these cards in Flash's panel, so a \
one-line confirmation (how many, which deck) is enough; there is no need \
to list or restate them.";

/// `CARDS_SHOWN` for a host that renders the panel, else nothing; the
/// note on cards a tool just wrote. Only with a panel to show them in.
fn cards_shown<P>(ctx: &RequestContext<RoleServer>, panel: &Option<P>) -> Option<String> {
    (panel.is_some() && panel::host_shows_panel(ctx)).then(|| CARDS_SHOWN.to_string())
}

/// Where else the user's cards are: their account on this server's site,
/// where they can study any time. Told to the model as a fact it may pass
/// on after the task, never as something to push.
fn on_the_site(base: &str) -> String {
    format!(
        "The cards are saved in the user's Flash account, where they can also study \
         them any time: {}",
        site_link(base, "/study", "chat")
    )
}

/// The panel's view of cards a tool just wrote, or none. The cards are
/// saved by then, so a view that cannot be built is logged and left out
/// rather than failing the call: a failed call is one the model retries,
/// making the cards twice.
fn written_panel(view: Result<PanelCardsOut, ServiceError>) -> Option<PanelCardsOut> {
    view.inspect_err(|e| tracing::warn!("mcp cards panel: {e}"))
        .ok()
}

#[tool_router(router = card_router, vis = "pub(super)")]
impl FlashMcp {
    #[tool(
        title = "Create cards",
        description = "Use this when the user wants flashcards: made from notes, slides or a file they share, a topic they name, or this conversation, or to memorize something and review it later. Saves 1-500 cards into one deck of their Flash account (created if missing, with parents for Parent::Child), where spaced repetition schedules each card's reviews. Follow the field notes; cloze uses {{c1::answer}}; keep local media as /media/{id}. HTTP(S) images are retained temporarily and localized asynchronously, then removed after repeated failed downloads. To change existing cards use update_cards, not new copies.",
        annotations(
            title = "Create cards",
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        ),
        output_schema = rmcp::handler::server::tool::schema_for_output::<MutationOut>(),
        meta = panel::shows_panel("Making your cards", "Cards made")
    )]
    async fn create_cards(
        &self,
        Parameters(args): Parameters<CreateCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let user = authed_user(&ctx)?;
        let site = self.panel_site();
        let (result, panel) = run(&self.services, move |s| {
            let inputs = args
                .cards
                .into_iter()
                .map(|c| {
                    let note_type = flash_store::notes::NoteType::parse(
                        c.note_type.as_deref().unwrap_or("basic"),
                    )
                    .ok_or_else(|| ServiceError::Malformed("unknown note_type".into()))?;
                    Ok(crate::service::NoteInput {
                        note_type,
                        front_html: c.front_html,
                        back_html: c.back_html,
                        tags: c.tags,
                        speech: flash_store::NoteSpeech {
                            front: speech_blocks(c.front_speech)?,
                            back: speech_blocks(c.back_speech)?,
                        },
                        // An assistant's blocks say its languages.
                        langs: None,
                        allow_external_images: true,
                    })
                })
                .collect::<Result<Vec<_>, ServiceError>>()?;
            let saved = s.create_mcp_notes(user, &args.deck, &inputs, now_ms())?;
            if let (Some(languages), Some((deck, _))) = (&args.languages, saved.deck_totals.first())
            {
                s.declare_deck_languages(user, flash_core::DeckId(*deck), languages)?;
            }
            let out = mutation_out(saved);
            let cards: Vec<CardId> = out.card_ids.iter().copied().map(CardId).collect();
            let panel = written_panel(Self::cards_panel(
                &s,
                &site,
                user,
                "created",
                &cards,
                &Default::default(),
            ));
            Ok((out, panel))
        })
        .await?;
        // MCP 创建卡片会同时入队外部媒体任务，提交成功后立即唤醒后台 worker。
        crate::jobs::wake(&self.state);
        self.account_event(&ctx, user, "cards_created", Some(result.created as i64));
        let mut result = result;
        let site = on_the_site(&self.base_url);
        result.note = Some(match cards_shown(&ctx, &panel) {
            Some(shown) => format!("{shown} {site}"),
            None => site,
        });
        with_panel(result, panel)
    }

    #[tool(
        title = "Update cards",
        description = "Use this when the user asks to fix, reword, correct or retag cards they already have. Changes 1-500 cards atomically: give card_id and only the fields to change (front_html, back_html, tags); the rest stay, cards keep their schedule and review history, and restore_cards can undo the edit. Read list_cards first; for a shared source copy its sibling_card_ids, one edit per source. The note type stays; an edit that would remove cards is refused (use delete_cards). Follow create_cards' field notes.",
        annotations(
            title = "Update cards",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = rmcp::handler::server::tool::schema_for_output::<UpdateOut>(),
        meta = panel::shows_panel("Saving your edits", "Cards updated")
    )]
    async fn update_cards(
        &self,
        Parameters(args): Parameters<UpdateCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let user = authed_user(&ctx)?;
        let site = self.panel_site();
        let (result, panel) = run(&self.services, move |s| {
            let patches = args
                .cards
                .into_iter()
                .map(|c| {
                    Ok(crate::service::McpSourcePatch {
                        card_id: CardId(c.card_id),
                        front_html: c.front_html,
                        back_html: c.back_html,
                        tags: c.tags,
                        sibling_card_ids: c.sibling_card_ids.into_iter().map(CardId).collect(),
                        speech: flash_store::NoteSpeech {
                            front: speech_blocks(c.front_speech)?,
                            back: speech_blocks(c.back_speech)?,
                        },
                    })
                })
                .collect::<Result<Vec<_>, ServiceError>>()?;
            // The panel shows the cards the edit named, each beside what
            // it said before; their siblings change too but go unshown.
            let edited: Vec<CardId> = patches.iter().map(|p| p.card_id).collect();
            let before = edited
                .iter()
                .filter_map(|&id| Self::card_sides(&s, user, id).ok().map(|sides| (id, sides)))
                .collect();
            let saved = s.update_mcp_sources(user, &patches, now_ms())?;
            let panel = written_panel(Self::cards_panel(
                &s, &site, user, "edited", &edited, &before,
            ));
            Ok((update_out(saved), panel))
        })
        .await?;
        self.account_event(
            &ctx,
            user,
            "card_edited",
            Some(result.mutation.updated as i64),
        );
        let mut result = result;
        result.mutation.note = cards_shown(&ctx, &panel);
        with_panel(result, panel)
    }

    #[tool(
        title = "Restore cards",
        description = "Use this when the user wants an earlier version of a card back, for example after an edit they did not want. Every edit keeps the text it replaced. Give card_id alone to list the card's earlier versions, newest first, and confirm with the user which one; then call again with its revision_id. A restore rewrites the card's source and its sibling cards in place, keeping their schedules and review history, and keeps the text it replaces too. A restore that would remove cards is refused.",
        annotations(
            title = "Restore cards",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        output_schema = rmcp::handler::server::tool::schema_for_output::<RestoreOut>(),
        meta = panel::shows_panel("Restoring your card", "Card restored")
    )]
    async fn restore_cards(
        &self,
        Parameters(args): Parameters<RestoreCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, McpError> {
        let user = authed_user(&ctx)?;
        let site = self.panel_site();
        let card = CardId(args.card_id);
        let (result, panel) = run(&self.services, move |s| {
            let Some(revision) = args.revision_id else {
                let revisions = s.card_revisions(user, card)?;
                // The panel shows the card as it is, beside what it said
                // in the newest earlier version: what a restore of that
                // version would bring back.
                let mut before = std::collections::HashMap::new();
                if let Some(newest) = revisions.first() {
                    if let Some(sides) = Self::revision_sides(&s, user, card, newest)? {
                        before.insert(card, sides);
                    }
                }
                let panel = written_panel(Self::cards_panel(
                    &s,
                    &site,
                    user,
                    "edited",
                    &[card],
                    &before,
                ));
                let out = RestoreOut {
                    revisions: revisions.into_iter().map(revision_out).collect(),
                    restored: None,
                };
                return Ok((out, panel));
            };
            let before = Self::card_sides(&s, user, card)
                .map(|sides| std::collections::HashMap::from([(card, sides)]))
                .unwrap_or_default();
            let restored = s.restore_card_revision(user, card, RevisionId(revision), now_ms())?;
            let panel = written_panel(Self::cards_panel(
                &s,
                &site,
                user,
                "edited",
                &[card],
                &before,
            ));
            let cards = restored.update.cards;
            let out = RestoreOut {
                revisions: Vec::new(),
                restored: Some(UpdateOut {
                    mutation: MutationOut {
                        source_ids: vec![restored.note.0],
                        updated: cards.len() as u32 - restored.created,
                        created: restored.created,
                        card_ids: cards.iter().map(|id| id.0).collect(),
                        deck_totals: restored.deck_totals,
                        note: None,
                    },
                    revision_ids: vec![restored.update.revision.map(|id| id.0)],
                }),
            };
            Ok((out, panel))
        })
        .await?;
        if let Some(restored) = &result.restored {
            self.account_event(
                &ctx,
                user,
                "card_edited",
                Some(restored.mutation.updated as i64),
            );
        }
        // A listing of earlier versions is for the user to choose from,
        // so only a restore that happened is left to the panel.
        let mut result = result;
        let shown = cards_shown(&ctx, &panel);
        if let Some(restored) = result.restored.as_mut() {
            restored.mutation.note = shown;
        }
        with_panel(result, panel)
    }

    #[tool(
        title = "Delete cards",
        description = "Use this when the user asks to delete specific cards. Takes 1-500 card IDs (from list_cards) with one confirmation for the batch: the first call deletes nothing and returns the exact set and a confirm_token; ask the user once, then repeat the same card_ids with confirm_token. Tokens last ten minutes and survive reconnects; a completed retry returns its original result. For a whole deck use delete_deck; to stop studying a deck but keep it, archive_deck.",
        annotations(
            title = "Delete cards",
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn delete_cards(
        &self,
        Parameters(args): Parameters<DeleteCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<DeleteOutcome>, McpError> {
        let user = authed_user(&ctx)?;
        let client = Self::client_id(&ctx)?;
        let mut selected = args.card_ids;
        if selected.is_empty() || selected.len() > 500 || selected.iter().any(|id| *id <= 0) {
            return Err(McpError::invalid_params(
                "supply 1-500 positive card IDs",
                None,
            ));
        }
        selected.sort_unstable();
        if selected.windows(2).any(|w| w[0] == w[1]) {
            return Err(McpError::invalid_params("duplicate card ID", None));
        }
        let target = format!("cards:{}", serde_json::json!(selected));
        match args.confirm_token {
            Some(token) => {
                let done = self.confirmed(user, client, target, token).await?;
                self.count_deleted(&ctx, user, &done);
                Ok(Json(done))
            }
            None => Ok(Json(
                self.ask_to_confirm(
                    user,
                    client,
                    target,
                    None,
                    selected.into_iter().map(CardId).collect(),
                )
                .await?,
            )),
        }
    }

    #[tool(
        title = "List cards",
        description = "Use this when the user wants to see, find or check their cards, and before update_cards or delete_cards. Returns whole cards and editable sources, newest first, by deck or search text, with authenticated media URLs (GET with the connector Bearer token; never speak URLs or markup). Sources show cloze answers, so quiz with start_study_session. Edits to a shared source reach its sibling_card_ids. Page with next_cursor as before_card_id while has_more; trust has_more, not the count.",
        annotations(
            title = "List cards",
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        ),
        meta = panel::for_model()
    )]
    async fn list_cards(
        &self,
        Parameters(args): Parameters<ListCardsArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<CardsOut>, McpError> {
        let user = authed_user(&ctx)?;
        let base = self.base_url.clone();
        let out = run(&self.services, move |s| {
            let deck = args
                .deck
                .as_deref()
                .map(|name| deck_named(&s, user, name))
                .transpose()?;
            let limit = args.limit.unwrap_or(50);
            if !(1..=500).contains(&limit) {
                return Err(ServiceError::Malformed("limit must be 1-500".into()));
            }
            let search = args.search.as_deref();
            let before = args.before_card_id.map(CardId);
            let store = s.store();
            // A deck takes its subdecks as the API's deck-card list does:
            // archived ones are left out unless the deck itself is archived.
            let (mut rows, total_count) = match deck {
                Some(deck) => {
                    let filter = CardFilter {
                        search,
                        status: CardStatus::All,
                        ..CardFilter::deck(deck)
                    };
                    (
                        store.list_filtered_cards(user, &filter, limit + 1, 0, before)?,
                        store.count_filtered_cards(user, &filter)?,
                    )
                }
                None => (
                    store.list_cards_cursor(user, None, search, limit + 1, 0, before)?,
                    store.count_cards(user, None, search)?,
                ),
            };
            let mut has_more = rows.len() > limit as usize;
            rows.truncate(limit as usize);
            let mut cards: Vec<CardOut> = Vec::new();
            let mut page_bytes = 0;
            for row in rows {
                let seed = s.editor_seed(user, row.id)?;
                let siblings = s.store().mcp_source_state(user, row.id)?.1;
                let read_only_reason = s.mcp_source_warning(user, &seed, &siblings)?;
                let front_html = rich_side(row.front_html.as_deref(), &row.front);
                let back_html = rich_side(row.back_html.as_deref(), &row.back);
                let media = media_out(
                    &s,
                    user,
                    &base,
                    &[&front_html, &back_html, &seed.front_html, &seed.back_html],
                )?;
                let card = CardOut {
                    card_id: row.id.0,
                    deck_id: row.deck_id.0,
                    ord: row.ord,
                    cloze_index: row.cloze_index,
                    type_answer: row.type_answer,
                    front_html,
                    back_html,
                    media,
                    source: SourceOut {
                        read_only_reason,
                        note_id: seed.note_id.map(|id| id.0),
                        note_type: seed.note_type.as_str().into(),
                        front_html: seed.front_html,
                        back_html: seed.back_html,
                        tags: seed.tags,
                        sibling_card_ids: siblings.iter().map(|id| id.0).collect(),
                    },
                };
                // A page is bounded by size as well as by count: rich
                // imported cards run to kilobytes each, and a client drops
                // a tool result that is too large. The cursor makes a
                // short page harmless. One card always fits.
                page_bytes += serde_json::to_vec(&card).map_or(0, |v| v.len());
                if page_bytes > LIST_PAGE_BYTES && !cards.is_empty() {
                    has_more = true;
                    break;
                }
                cards.push(card);
            }
            let next_cursor = if has_more {
                cards.last().map(|c| c.card_id)
            } else {
                None
            };
            Ok(CardsOut {
                cards,
                total_count,
                next_cursor,
                has_more,
            })
        })
        .await?;
        Ok(Json(out))
    }
}
