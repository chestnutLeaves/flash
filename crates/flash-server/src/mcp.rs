//! MCP server: rich card management and study. Tool descriptions and server
//! instructions carry the conversation protocol; responses stay terse so
//! there is nothing scheduling-ish for a model to read out.

use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, UserId};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::session::{
    SessionState, SessionStore, SessionStoreError,
};
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::{tool_handler, ErrorData as McpError};

use std::sync::Arc;

use crate::ext::{AccountEvent, ServerExtension, Surface};
use crate::oauth::ConnectorClient;
use crate::service::{now_ms, ServiceError, Services};
use crate::state::AppState;
use errors::{to_mcp_err, tool_failure_reason, tool_success_detail};
use panel::MediaLinks;

mod args;
mod card_tools;
mod deck_tools;
mod errors;
mod out;
pub mod panel;
mod panel_tools;
mod study_tools;

pub use args::*;
pub use out::*;
pub use panel_tools::*;

fn rich_side(html: Option<&str>, plain: &str) -> String {
    html.map(str::to_string)
        .unwrap_or_else(|| crate::service::text_to_html(plain))
}

fn panel_side(
    s: &Services,
    user: UserId,
    row: &flash_store::CardRow,
    side: flash_store::CardSide,
) -> Result<(String, String), ServiceError> {
    let plain = match side {
        flash_store::CardSide::Front => rich_side(row.front_html.as_deref(), &row.front),
        flash_store::CardSide::Back => rich_side(row.back_html.as_deref(), &row.back),
    };
    let labeled = s.labeled_side(user, row, side)?;
    Ok((labeled.html.unwrap_or(plain), labeled.lang))
}

fn media_out(
    s: &Services,
    user: UserId,
    base: &str,
    fields: &[&str],
) -> Result<Vec<MediaOut>, ServiceError> {
    let mut ids = std::collections::BTreeSet::new();
    for field in fields {
        ids.extend(flash_store::richtext::media_ids(field));
    }
    ids.into_iter()
        .map(|id| {
            let row = s
                .store()
                .get_media(user, flash_core::MediaId(id))?
                .ok_or_else(|| ServiceError::Malformed(format!("missing media reference {id}")))?;
            Ok(MediaOut {
                media_id: id,
                url: format!("{}/mcp-media/{id}", base.trim_end_matches('/')),
                mime: row.mime,
                filename: row.filename,
                size: row.size,
            })
        })
        .collect()
}

// Kept short on purpose: claude.ai discards server-level instructions
// entirely (only per-tool descriptions reach the model), so every rule
// that matters rides in the tool descriptions and responses. Said as
// what Flash is and how its tools fit together, not as orders, and
// whole within the first 512 characters, where hosts that read it look.
const INSTRUCTIONS: &str = "\
Flash keeps the user's flashcards in decks and schedules each card's \
reviews with spaced repetition, so a card comes back right before it \
would be forgotten. create_cards makes cards from notes, a file or a \
topic; start_study_session asks the cards that are due, one at a time, \
and submit_review records each answer. Progress counts only for cards \
asked in a session and graded with submit_review, so a quiz from the \
conversation leaves the schedule unchanged. An account with no cards \
starts by making some.";

/// The largest JSON-RPC message MCP accepts; a card's two fields are a
/// few tens of kilobytes at most, and `create_cards` takes at most
/// `bounds::CARDS_PER_CALL` of them.
pub const MCP_BODY_LIMIT: usize = 1024 * 1024;

/// `public_host` is the authority of FLASH_BASE_URL: rmcp validates the
/// Host header against loopback only by default (DNS-rebinding guard),
/// which would reject requests arriving through a proxy or tunnel.
///
/// Sessions are kept in the database (`DbSessions`), so a restart does
/// not end an assistant's connection, and `shutdown` ends the open
/// streams when the process stops, so they don't hold up its drain.
///
/// Replies carry no `retry:` priming frame, from the transport or the
/// session layer: Gemini's client (since 2026-10) cannot parse one and
/// drops the connection after initialize.
pub fn mcp_service(
    state: AppState,
    public_host: String,
) -> StreamableHttpService<FlashMcp, LocalSessionManager> {
    let services = state.services.clone();
    let mut config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
    config.allowed_hosts = vec![
        public_host,
        "localhost:8437".to_string(),
        "127.0.0.1:8437".to_string(),
    ];
    // The transport reads the body itself, so this is the only limit
    // that applies to it (an axum body-limit layer around the service is
    // a marker its extractors consult and this service never does).
    config.max_request_body_bytes = MCP_BODY_LIMIT;
    config.session_store = Some(Arc::new(DbSessions {
        services: services.clone(),
    }));
    config.cancellation_token = state.shutdown.clone();
    config.sse_retry = None;
    let mut sessions = LocalSessionManager::default();
    sessions.session_config.sse_retry = None;
    StreamableHttpService::new(
        move || Ok(FlashMcp::new(state.clone())),
        Arc::new(sessions),
        config,
    )
}

/// Assistants' MCP sessions in the database: the transport stores the
/// client's `initialize` once the handshake is done, and restores the
/// session from it when a request names one this process never saw (a
/// deploy restarted the server). Rows are keyed by the hash of the
/// session id, which the client holds like a bearer secret for the
/// session; every call on it still carries the account's own token.
struct DbSessions {
    services: Services,
}

#[async_trait::async_trait]
impl SessionStore for DbSessions {
    async fn load(&self, session_id: &str) -> Result<Option<SessionState>, SessionStoreError> {
        let services = self.services.clone();
        let id_hash = crate::auth::hash_token(session_id);
        let row = tokio::task::spawn_blocking(move || {
            services.store().load_mcp_session(&id_hash, now_ms())
        })
        .await??;
        Ok(match row {
            Some(state) => Some(serde_json::from_str(&state)?),
            None => None,
        })
    }

    async fn store(&self, session_id: &str, state: &SessionState) -> Result<(), SessionStoreError> {
        let services = self.services.clone();
        let id_hash = crate::auth::hash_token(session_id);
        let state = serde_json::to_string(state)?;
        tokio::task::spawn_blocking(move || {
            services
                .store()
                .store_mcp_session(&id_hash, &state, now_ms())
        })
        .await??;
        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<(), SessionStoreError> {
        let services = self.services.clone();
        let id_hash = crate::auth::hash_token(session_id);
        tokio::task::spawn_blocking(move || services.store().delete_mcp_session(&id_hash))
            .await??;
        Ok(())
    }
}

#[derive(Clone)]
pub struct FlashMcp {
    services: Services,
    /// The deployment's per-account counters.
    ext: Arc<dyn ServerExtension>,
    /// Signs the study panel's media links.
    links: MediaLinks,
    base_url: String,
    /// The whole server, for the extension's calls that need more than
    /// the store (whether it can send mail at all).
    state: AppState,
    /// The model's tools and the study panel's, built once per server;
    /// every call, list and lookup goes through it.
    tool_router: ToolRouter<FlashMcp>,
}

fn authed_user(ctx: &RequestContext<RoleServer>) -> Result<UserId, McpError> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<UserId>().copied())
        .ok_or_else(|| McpError::invalid_request("unauthenticated", None))
}

/// Who is asking, for the logs: the connector's registered name and the
/// name its MCP client gave in `initialize`, each "-" when absent.
fn clients(ctx: &RequestContext<RoleServer>) -> (String, String) {
    let client = authed_client(ctx).unwrap_or_else(|| "-".to_string());
    let host = ctx
        .peer
        .peer_info()
        .map(|info| info.client_info.name.clone())
        .unwrap_or_else(|| "-".to_string());
    (client, host)
}

/// The language the chat host says its person reads, from a call's
/// `_meta`: ChatGPT sends `openai/locale` (older clients `webplus/i18n`).
/// Normalized; None when the host says nothing a language can be read
/// from.
fn host_locale(ctx: &RequestContext<RoleServer>) -> Option<String> {
    ["openai/locale", "webplus/i18n"]
        .iter()
        .find_map(|key| ctx.meta.get(*key).and_then(|v| v.as_str()))
        .and_then(flash_core::locale::normalize)
}

/// The registered name of the client making the call, when the bearer
/// layer resolved one.
fn authed_client(ctx: &RequestContext<RoleServer>) -> Option<String> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ConnectorClient>())
        .map(|c| c.0.clone())
        .filter(|name| !name.is_empty())
}

/// The user's deck by name, or the error every deck-naming tool shares.
fn deck_named(s: &Services, user: UserId, name: &str) -> Result<DeckId, ServiceError> {
    s.store()
        .find_deck_by_name(user, name)?
        .ok_or_else(|| ServiceError::Malformed(format!("no_deck: no deck named '{name}'")))
}

async fn run<T, F>(services: &Services, f: F) -> Result<T, McpError>
where
    T: Send + 'static,
    F: FnOnce(Services) -> Result<T, ServiceError> + Send + 'static,
{
    let services = services.clone();
    tokio::task::spawn_blocking(move || f(services))
        .await
        .map_err(|e| {
            tracing::error!("join: {e}");
            McpError::internal_error("internal error", None)
        })?
        .map_err(to_mcp_err)
}

/// A result the panel shows: `out` exactly as `Json` would carry it for
/// the model, and the panel's view in `_meta`, which hosts hand to the
/// panel and keep out of the model's context. Without a view the result
/// is just `out`.
fn with_panel<T: serde::Serialize, P: serde::Serialize>(
    out: T,
    panel: Option<P>,
) -> Result<rmcp::model::CallToolResult, McpError> {
    let encode = |e: serde_json::Error| {
        tracing::error!("mcp result: {e}");
        McpError::internal_error("internal error", None)
    };
    let result =
        rmcp::model::CallToolResult::structured(serde_json::to_value(out).map_err(encode)?);
    let Some(panel) = panel else {
        return Ok(result);
    };
    let mut meta = serde_json::Map::new();
    meta.insert(
        panel::PAYLOAD.to_string(),
        serde_json::to_value(panel).map_err(encode)?,
    );
    Ok(result.with_meta(Some(rmcp::model::MetaObject(meta))))
}

/// A page of this server's site, with `?via=` naming where the link was
/// handed out (`web::VIA_LABELS`).
fn site_link(base: &str, path: &str, via: &str) -> String {
    format!("{}{path}?via={via}", base.trim_end_matches('/'))
}

impl FlashMcp {
    pub fn new(state: AppState) -> Self {
        let tool_router =
            Self::study_router() + Self::deck_router() + Self::card_router() + Self::panel_router();
        Self {
            services: state.services.clone(),
            ext: state.ext.clone(),
            links: state.panel_links.clone(),
            base_url: state.config.base_url.clone(),
            state,
            tool_router,
        }
    }

    fn client_id(ctx: &RequestContext<RoleServer>) -> Result<String, McpError> {
        ctx.extensions
            .get::<http::request::Parts>()
            .and_then(|p| p.extensions.get::<crate::oauth::ConnectorClientId>())
            .map(|c| c.0.clone())
            .ok_or_else(|| McpError::invalid_request("unauthenticated", None))
    }

    async fn ask_to_confirm(
        &self,
        user: UserId,
        client: String,
        target: String,
        deck: Option<DeckId>,
        cards: Vec<CardId>,
    ) -> Result<DeleteOutcome, McpError> {
        let token = crate::auth::new_token();
        let hash = crate::auth::hash_token(&token);
        let now = now_ms();
        let preview = run(&self.services, move |s| {
            Ok(s.store()
                .stage_mcp_delete(user, &client, &target, deck, &cards, &hash, now)?)
        })
        .await?;
        Ok(DeleteOutcome {
            deleted: false,
            pending: true,
            confirm_token: Some(token),
            cards_affected: preview.card_ids.len() as u32,
            card_ids: preview.card_ids,
            decks_affected: preview.decks,
            deck_totals: Vec::new(),
            expires_ms: Some(now + 600_000),
            note: Some("Nothing deleted. Describe this exact deletion set, naming every deck in decks_affected, and ask the user to confirm once. After approval, repeat the same arguments with confirm_token. Valid for ten minutes across reconnects.".into()),
        })
    }

    async fn confirmed(
        &self,
        user: UserId,
        client: String,
        target: String,
        token: String,
    ) -> Result<DeleteOutcome, McpError> {
        let hash = crate::auth::hash_token(&token);
        let receipt = run(&self.services, move |s| {
            Ok(s.store()
                .confirm_mcp_delete(user, &client, &target, &hash, now_ms())?)
        })
        .await?;
        // A deck deletion owes its orphaned blobs to the job worker,
        // which takes them at its next poll.
        Ok(DeleteOutcome {
            deleted: true,
            pending: false,
            confirm_token: None,
            cards_affected: receipt.card_ids.len() as u32,
            card_ids: receipt.card_ids,
            decks_affected: receipt.decks,
            deck_totals: receipt.deck_totals,
            expires_ms: None,
            note: None,
        })
    }

    async fn question(
        &self,
        user: UserId,
        card: Option<&crate::service::CardFront>,
    ) -> Result<Option<CardFrontOut>, McpError> {
        let Some(card) = card else {
            return Ok(None);
        };
        let id = card.card_id;
        let base = self.base_url.clone();
        run(&self.services, move |s| {
            let row = s.card(user, id)?;
            let front_html = rich_side(row.front_html.as_deref(), &row.front);
            let media = media_out(&s, user, &base, &[&front_html])?;
            Ok(Some(CardFrontOut {
                card_id: id.0,
                front_html,
                media,
            }))
        })
        .await
    }

    /// The study panel's copy of a card and of the one expected after it:
    /// their fronts with media links the panel can load, and the images
    /// worth fetching before they are needed (this card's back, the next
    /// card's front).
    async fn panel_card(
        &self,
        user: UserId,
        shown: Option<CardId>,
        next: Option<CardId>,
    ) -> Result<PanelCards, McpError> {
        let Some(shown) = shown else {
            return Ok(PanelCards::default());
        };
        let base = self.base_url.clone();
        let links = self.links.clone();
        run(&self.services, move |s| {
            let now = now_ms();
            let front = |row: &flash_store::CardRow| -> Result<(String, String), ServiceError> {
                let (html, lang) = panel_side(&s, user, row, flash_store::CardSide::Front)?;
                Ok((links.sign_html(&html, &base, user, now), lang))
            };
            let row = s.card(user, shown)?;
            let back_html = rich_side(row.back_html.as_deref(), &row.back);
            let mut warm = flash_store::richtext::media_image_ids(&back_html);
            let next_card = match next {
                Some(next) => {
                    let next_row = s.card(user, next)?;
                    let next_front = rich_side(next_row.front_html.as_deref(), &next_row.front);
                    warm.extend(flash_store::richtext::media_image_ids(&next_front));
                    let (front_html, front_lang) = front(&next_row)?;
                    Some(PanelCardOut {
                        card_id: next.0,
                        front_html,
                        front_lang,
                    })
                }
                None => None,
            };
            let preload = warm
                .into_iter()
                .map(|id| links.url(&base, user, flash_core::MediaId(id), now))
                .collect();
            let (front_html, front_lang) = front(&row)?;
            Ok(PanelCards {
                card: Some(PanelCardOut {
                    card_id: shown.0,
                    front_html,
                    front_lang,
                }),
                next_card,
                preload,
            })
        })
        .await
    }

    /// Both sides of a card as a study shows them.
    fn card_sides(
        s: &Services,
        user: UserId,
        card: CardId,
    ) -> Result<(String, String), ServiceError> {
        let row = s.card(user, card)?;
        Ok((
            rich_side(row.front_html.as_deref(), &row.front),
            rich_side(row.back_html.as_deref(), &row.back),
        ))
    }

    /// Both sides of `card` as an earlier version of its note would make
    /// them, or None when that version makes no card in the card's slot.
    fn revision_sides(
        s: &Services,
        user: UserId,
        card: CardId,
        revision: &flash_store::NoteRevision,
    ) -> Result<Option<(String, String)>, ServiceError> {
        let ord = s.card(user, card)?.ord;
        let front = flash_store::richtext::sanitize_with_media(&revision.front_html).trimmed();
        let back = flash_store::richtext::sanitize_with_media(&revision.back_html).trimmed();
        let Ok(generated) = flash_store::notes::generate_cards(revision.note_type, &front, &back)
        else {
            return Ok(None);
        };
        Ok(generated.into_iter().find(|g| g.ord == ord).map(|g| {
            (
                rich_side(g.extras.front_html.as_deref(), &g.text.front),
                rich_side(g.extras.back_html.as_deref(), &g.text.back),
            )
        }))
    }

    /// The panel's copies of `cards`, whole, with media links it can load.
    /// `before` holds an edited card's sides from before the edit; a side
    /// that did not change is left out of the card's `before`.
    fn shown_cards(
        s: &Services,
        links: &MediaLinks,
        base: &str,
        user: UserId,
        cards: &[CardId],
        before: &std::collections::HashMap<CardId, (String, String)>,
    ) -> Result<Vec<PanelShownCard>, ServiceError> {
        let now = now_ms();
        let sign = |html: &str| links.sign_html(html, base, user, now);
        cards
            .iter()
            .map(|&id| {
                let row = s.card(user, id)?;
                let front = rich_side(row.front_html.as_deref(), &row.front);
                let back = rich_side(row.back_html.as_deref(), &row.back);
                let before = before.get(&id).and_then(|(old_front, old_back)| {
                    let front_html = (*old_front != front).then(|| sign(old_front));
                    let back_html = (*old_back != back).then(|| sign(old_back));
                    (front_html.is_some() || back_html.is_some()).then_some(PanelBefore {
                        front_html,
                        back_html,
                    })
                });
                let (front_shown, front_lang) =
                    panel_side(s, user, &row, flash_store::CardSide::Front)?;
                let (back_shown, back_lang) =
                    panel_side(s, user, &row, flash_store::CardSide::Back)?;
                Ok(PanelShownCard {
                    card_id: id.0,
                    front_html: sign(&front_shown),
                    back_html: sign(&back_shown),
                    front_lang,
                    back_lang,
                    tags: row.tags,
                    before,
                })
            })
            .collect()
    }

    /// The panel's view of cards just written: the first page whole, the
    /// rest by id, and their deck when they share one.
    fn cards_panel(
        s: &Services,
        site: &PanelSite,
        user: UserId,
        mode: &'static str,
        cards: &[CardId],
        before: &std::collections::HashMap<CardId, (String, String)>,
    ) -> Result<PanelCardsOut, ServiceError> {
        let PanelSite {
            links,
            base,
            shipped,
        } = site;
        let first = &cards[..cards.len().min(PANEL_PAGE)];
        let shown = Self::shown_cards(s, links, base, user, first, before)?;
        let mut decks = std::collections::BTreeSet::new();
        for &id in cards {
            decks.insert(s.card(user, id)?.deck_id);
        }
        // One deck names the header and what Study starts; a deck the
        // list leaves out (an archived one) just goes unnamed.
        let deck = match decks.into_iter().collect::<Vec<_>>()[..] {
            [only] => s.deck_by_id(user, only, now_ms()).ok(),
            _ => None,
        };
        Ok(PanelCardsOut {
            view: "cards",
            mode,
            deck_label: deck.as_ref().map(|d| d.path.clone()),
            deck_url: deck
                .as_ref()
                .map(|d| format!("{}/decks/{}?via=panel", base.trim_end_matches('/'), d.id.0)),
            deck: deck.map(|d| d.name),
            card_ids: cards.iter().map(|id| id.0).collect(),
            cards: shown,
            ui_lang: panel_lang(s.store(), user, shipped),
        })
    }

    fn panel_site(&self) -> PanelSite {
        PanelSite {
            links: self.links.clone(),
            base: self.base_url.clone(),
            shipped: self.state.i18n.shipped().to_vec(),
        }
    }

    /// Counts something the caller's account did over MCP, with the
    /// client's registered name as its `client`.
    fn account_event(
        &self,
        ctx: &RequestContext<RoleServer>,
        user: UserId,
        event: &'static str,
        amount: Option<i64>,
    ) {
        let client = authed_client(ctx);
        self.ext.account_event(
            &self.services,
            user,
            AccountEvent::new(event, Surface::Mcp)
                .amount(amount)
                .client(client.as_deref()),
        );
    }

    /// A confirmed deletion that deleted cards counts them
    /// (`cards_deleted`); a preview or a refused token deleted nothing.
    fn count_deleted(&self, ctx: &RequestContext<RoleServer>, user: UserId, done: &DeleteOutcome) {
        if done.deleted && done.cards_affected > 0 {
            self.account_event(ctx, user, "cards_deleted", Some(done.cards_affected as i64));
        }
    }

    fn deck_by_name(services: &Services, user: UserId, name: &str) -> Result<DeckId, ServiceError> {
        services
            .store()
            .find_deck_by_name(user, name)?
            .ok_or_else(|| ServiceError::Malformed(format!("no_deck: no deck named '{name}'")))
    }

    /// What a study call covers. A deck and tags narrow each other, and
    /// `tag` is one more tag.
    fn resolve_scope(
        services: &Services,
        user: UserId,
        deck: Option<String>,
        tag: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Result<StudyScope, ServiceError> {
        let deck = deck
            .map(|name| Self::deck_by_name(services, user, &name))
            .transpose()?;
        Ok(StudyScope::new(
            deck,
            tags.unwrap_or_default().into_iter().chain(tag),
        ))
    }
}

#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for FlashMcp {
    /// What `#[tool_handler]` would generate, plus an account event per
    /// call (`amount`, ms; listing the tools reports none). The detail is
    /// the tool's own name, or "unknown" for a name the router does not
    /// know, and for a failure `tool:reason` (`tool_failure_reason`), so a
    /// client never writes its own text into the event.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        let user = authed_user(&context).ok();
        let client = authed_client(&context);
        if let (Some(user), Some(tag)) = (user, host_locale(&context)) {
            let (_, host) = clients(&context);
            let kept = crate::account_locale::keep_often_seen(
                &self.services,
                user,
                flash_store::LocaleSource::Host,
                vec![tag.clone()],
                crate::service::now_ms(),
            )
            .await;
            if kept {
                tracing::info!(host, locale = %tag, "mcp host locale");
            }
        }
        let tool = self
            .tool_router
            .get(&request.name)
            .map(|t| t.name.clone())
            .unwrap_or(std::borrow::Cow::Borrowed("unknown"));
        let started = std::time::Instant::now();
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(tcc).await;
        if let Some(user) = user {
            let (event, detail) = match tool_failure_reason(&result) {
                None => ("mcp_tool", tool_success_detail(&tool, &result)),
                Some(reason) => ("mcp_tool_error", format!("{tool}:{reason}")),
            };
            self.ext.account_event(
                &self.services,
                user,
                AccountEvent::new(event, Surface::Mcp)
                    .detail(Some(&detail))
                    .amount(Some(started.elapsed().as_millis() as i64))
                    .client(client.as_deref()),
            );
        }
        result
    }

    fn get_info(&self) -> ServerConfig {
        use base64::Engine;
        let mut info = ServerConfig::default();
        info.server_info.name = "flash".to_string();
        info.server_info.title = Some("Flash".to_string());
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info.server_info.description =
            Some("Spaced-repetition study and rich flashcard management.".to_string());
        // SEP-973 server icons (MCP spec 2025-11-25): a data URI keeps the
        // metadata self-contained. The favicon is a full-bleed square on
        // purpose: hosts clip icons with their own corner radius, and a tile
        // with baked-in rounding shows its transparent corners as white wedges.
        let icon_uri = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(include_str!("../static/favicon.svg"))
        );
        info.server_info.icons = Some(vec![rmcp::model::Icon::new(icon_uri)
            .with_mime_type("image/svg+xml")
            .with_sizes(vec!["any".to_string()])]);
        // The study panel is a resource, and the extension says hosts may
        // render it (MCP Apps).
        let mut extensions = rmcp::model::ExtensionCapabilities::new();
        extensions.insert(panel::EXTENSION.to_string(), Default::default());
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_extensions_with(extensions)
            .build();
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, McpError> {
        let (client, host) = clients(&context);
        tracing::info!(client, host, "mcp resources listed");
        let mut list =
            rmcp::model::ListResourcesResult::with_all_items(vec![panel::resource(&self.base_url)]);
        if let Some((ttl, scope)) = panel::cache_hints(&context) {
            list = list.with_ttl_ms(ttl).with_cache_scope(scope);
        }
        Ok(list)
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let (client, host) = clients(&context);
        let found = panel::is_panel(&request.uri);
        // Debug-quoted: the URI is the client's own text.
        tracing::info!(client, host, uri = ?request.uri, found, "mcp resource read");
        if !found {
            return Err(McpError::resource_not_found("no such resource", None));
        }
        let mut page = panel::read(&self.base_url, &request.uri, &self.state.i18n);
        if let Some((ttl, scope)) = panel::cache_hints(&context) {
            page = page.with_ttl_ms(ttl).with_cache_scope(scope);
        }
        Ok(page.into())
    }
}

#[cfg(test)]
mod icon_tests {
    /// Connector dialogs and app launchers round icons themselves; a tile
    /// with its own radius shows white wedges in the corners.
    #[test]
    fn favicon_is_a_full_bleed_square() {
        let svg = include_str!("../static/favicon.svg");
        assert!(svg.starts_with("<svg"));
        assert!(!svg.contains("rx="), "corner radius crept back: {svg}");
        assert!(svg.contains("<rect width=\"64\" height=\"64\" fill="));
    }
}
