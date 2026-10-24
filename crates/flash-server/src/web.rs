//! SSR web UI: page handlers and htmx partials. All domain work goes
//! through Services; this module only translates HTTP <-> service calls.

use askama::Template;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use flash_core::{DeckId, UserId};
use flash_store::CardRow;

use crate::auth::{self, Theme};
use crate::ext::{AccountEvent, NavSection, Surface};
use crate::flows::import_flow;
use crate::service::{DeckRow, ServiceError, Services};
use crate::state::AppState;
use crate::webauthn;

mod admin;
mod assets;
mod auth_pages;
mod cards;
mod dashboard;
mod deck;
mod decks;
mod export;
pub mod goal;
mod import;
mod preview;
mod public;
mod settings;
mod study;

pub(crate) use auth_pages::{render_enroll_error, render_login_error};
pub use cards::{note_type_label, EditorView, NoteForm};
pub use dashboard::dashboard_for;
pub use public::{connect_page, dead_link, gone, not_found, render_public};
pub(crate) use study::weekday;

/// Request-body ceiling, sized to what a typical edge proxy allows; larger
/// decks need the chunked upload that is still on the roadmap.
const IMPORT_MAX_BYTES: usize = import_flow::MAX_BYTES;
/// Deck page: cards per page.
pub const CARDS_PER_PAGE: u32 = 25;
/// One editor upload: the per-file media cap plus multipart framing.
pub const MEDIA_UPLOAD_MAX_BYTES: usize = flash_store::media::MAX_FILE_BYTES as usize + 64 * 1024;

/// The core's site root: the dashboard for members, the login page for
/// everyone else, and a robots.txt that keeps crawlers out of a private
/// server. A downstream extension may replace this bundle
/// (`ServerExtension::site_routes`).
pub fn site_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(dashboard::home))
        .route("/robots.txt", get(public::robots_private))
}

/// The admin users list. An extension can serve its own instead
/// (`ServerExtension::users_routes`).
pub fn users_routes() -> Router<AppState> {
    Router::new().route("/users", get(admin::users_page))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(auth_pages::login_page))
        .route("/connect", get(connect_page))
        .route("/enroll/{token}", get(auth_pages::enroll_page))
        .route("/reset", get(crate::password::reset_request_page))
        .route("/reset/{token}", get(crate::password::reset_confirm_page))
        .route(
            "/settings/password/set",
            post(crate::password::settings_set_password),
        )
        .route(
            "/settings/password/change",
            post(crate::password::settings_change_password),
        )
        .route(
            "/settings/password/remove",
            post(crate::password::settings_remove_password),
        )
        .route("/decks", get(decks::decks_page).post(decks::create_deck))
        .route("/decks/archived", post(decks::set_show_archived))
        .route("/decks/{id}", get(deck::deck_page))
        .route(
            "/decks/{id}/cards",
            get(deck::card_rows).post(cards::create_card),
        )
        .route("/decks/{id}/cards/bulk", post(deck::bulk))
        .route("/decks/{id}/export.apkg", get(export::deck_apkg))
        .route("/decks/{id}/export.csv", get(export::deck_csv))
        .route("/decks/{id}/notes", post(cards::create_note))
        .route(
            "/cards/{id}/edit",
            get(cards::card_editor).post(cards::save_card),
        )
        .route("/cards/{id}/restore/{revision}", post(cards::restore_card))
        .route(
            "/media",
            post(crate::media::upload_media).layer(DefaultBodyLimit::max(MEDIA_UPLOAD_MAX_BYTES)),
        )
        .route("/decks/{id}/settings", get(decks::deck_settings_page))
        .route("/decks/{id}/rename", post(decks::rename_deck))
        .route("/decks/{id}/fold", post(decks::fold_deck))
        .route("/decks/{id}/move", post(decks::move_deck))
        .route("/decks/{id}/subdecks", post(decks::create_subdeck))
        .route("/decks/{id}/limits", post(decks::save_deck_limits))
        .route("/decks/{id}/boost", post(decks::boost_deck))
        .route("/decks/{id}/goal", post(goal::save_goal))
        .route("/decks/{id}/goal/form", get(goal::goal_form))
        .route("/decks/{id}/goal/plan", get(goal::goal_plan))
        .route("/decks/{id}/archive", post(decks::archive_deck))
        .route("/decks/{id}/unarchive", post(decks::unarchive_deck))
        .route("/decks/{id}/delete", post(decks::delete_deck))
        .route("/decks/{id}/preview", get(preview::preview_page))
        .route("/study/boost", post(decks::boost_all))
        .route("/connect/dismiss", post(dashboard::dismiss_connect_cta))
        .route(
            "/cards/{id}",
            get(cards::card_page).delete(cards::delete_card),
        )
        .route("/cards/{id}/archive", post(cards::archive_card))
        .route("/cards/{id}/unarchive", post(cards::unarchive_card))
        .route("/study", get(study::study_page))
        .route("/study/reveal", post(study::study_reveal))
        .route("/study/card", get(study::study_card_again))
        .route("/study/review", post(study::study_review))
        .route("/study/archive", post(study::study_archive))
        .route("/study/unarchive", post(study::study_unarchive))
        .route("/study/remind", post(study::study_remind))
        .route("/stats", get(dashboard::stats_page))
        .route("/settings", get(settings::settings_page))
        .route("/settings/appearance", post(settings::save_appearance))
        .route("/settings/language", post(settings::save_language))
        .route("/settings/name", post(settings::save_name))
        .route("/settings/scheduling", post(settings::save_scheduling))
        .route("/settings/grading", post(settings::save_grading))
        .route("/settings/timezone", post(dashboard::adopt_timezone))
        .route("/settings/invite", post(settings::create_invite))
        .route("/settings/apps/disconnect", post(settings::disconnect_app))
        .route(
            "/settings/devices/sign_out",
            post(settings::sign_out_device),
        )
        .route(
            "/settings/devices/sign_out_others",
            post(settings::sign_out_other_devices),
        )
        .route(
            "/settings/delete-account",
            post(crate::password::settings_delete_account),
        )
        .route("/import", get(import::import_page))
        .route(
            "/import/preview",
            post(import::import_preview).layer(DefaultBodyLimit::max(IMPORT_MAX_BYTES)),
        )
        .route("/import/commit", post(import::import_commit))
        .route("/media/{id}", get(crate::media::serve_media))
        .route("/export.apkg", get(export::export_apkg))
        .route("/export.csv", get(export::export_csv))
        .merge(assets::routes())
}

/// Credential-ceremony endpoints, rate-limited separately in lib.rs.
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/reset", post(crate::password::reset_request_submit))
        .route(
            "/auth/login/password",
            post(crate::password::login_password),
        )
        .route(
            "/auth/reset/{token}",
            post(crate::password::reset_confirm_submit),
        )
        .route(
            "/auth/enroll/{token}/password",
            post(crate::password::enroll_password),
        )
        .route("/auth/enroll/{token}/start", post(webauthn::enroll_start))
        .route("/auth/enroll/finish", post(webauthn::enroll_finish))
        .route("/auth/login/start", post(webauthn::login_start))
        .route("/auth/login/finish", post(webauthn::login_finish))
        .route("/auth/logout", post(webauthn::logout))
}

// ---- plumbing ----

/// Per-request context consumed by base.html: theme + active nav item,
/// the viewer's role so the sidebar can show admin-only entries, and the
/// extension's own sidebar items.
pub struct Shell {
    pub theme: &'static str,
    pub active: &'static str,
    pub is_admin: bool,
    /// The extension's items after "Stats".
    pub nav_html: String,
    /// The extension's items after "Users", in the Account section.
    pub account_nav_html: String,
    /// The extension's items in the sidebar footer, above "Sign out".
    pub foot_html: String,
    /// The extension's page-level overlays, emitted after the layout.
    pub overlay_html: String,
    /// The time zone a page that shows times rendered them in, as
    /// `data-zone` on `<html>` (`crate::when`); None on every other page.
    pub zone: Option<String>,
}

impl Shell {
    pub fn new(state: &AppState, theme: Theme, active: &'static str, is_admin: bool) -> Shell {
        Shell {
            theme: theme.as_str(),
            active,
            is_admin,
            nav_html: state.ext.nav_html(NavSection::Primary, active, is_admin),
            account_nav_html: state.ext.nav_html(NavSection::Account, active, is_admin),
            foot_html: state.ext.nav_html(NavSection::Foot, active, is_admin),
            overlay_html: state.ext.overlay_html(is_admin),
            zone: None,
        }
    }

    /// The same page, saying it shows times in `zone`.
    pub fn in_zone(mut self, zone: &crate::when::Zone) -> Shell {
        self.zone = Some(zone.name.clone());
        self
    }
}

/// One value from a raw query string of plain tokens (no percent-decoding),
/// for the flags an extension's own redirects carry.
pub fn query_param<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    raw.split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

/// A post-login destination is honored only when it is a local path.
pub fn clean_next(raw: Option<String>) -> String {
    match raw {
        Some(n) if is_local_path(&n) => n,
        _ => "/".to_string(),
    }
}

/// Where a sign-in page sends someone who is already signed in: on to
/// `next`, as a signed-out visit would have after signing in, but never
/// back to a sign-in page, which would send them round again.
pub fn onward(raw: Option<String>) -> String {
    let next = clean_next(raw);
    let page = next.split(['?', '#']).next().unwrap_or_default();
    if page == "/login" || page == "/signup" {
        "/".to_string()
    } else {
        next
    }
}

/// A path browsers will resolve against this origin and nothing else.
/// Besides the obvious `//host`, the WHATWG URL parser treats `\` as `/`
/// for http(s) and strips tabs and newlines before parsing, so `/\host`
/// and `/<tab>/host` are protocol-relative too. Control characters are
/// also refused because they cannot be placed in a Location header.
pub fn is_local_path(n: &str) -> bool {
    n.len() <= crate::bounds::NEXT_PATH
        && n.starts_with('/')
        && !n.starts_with("//")
        && !n.starts_with("/\\")
        && !n.bytes().any(|b| b < 0x20 || b == 0x7f || b == b'\\')
}

/// A `?ref=` code: lowercased, short, [a-z0-9_-] only; anything else is None.
pub fn clean_ref(raw: Option<String>) -> Option<String> {
    let r = raw?.trim().to_lowercase();
    if r.is_empty()
        || r.len() > 32
        || !r
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return None;
    }
    Some(r)
}

/// Why a page request is not a person looking at a page, or None when it
/// probably is: a crawler or script user agent (`ua`, or `no_ua` when
/// absent), a `HEAD` probe, a request that doesn't accept HTML
/// (`accept`), a browser prefetch or prerender (`prefetch`), or a
/// subresource fetch (`fetch`). Headers are only inspected, never stored.
pub fn bot_reason(
    method: &axum::http::Method,
    headers: &axum::http::HeaderMap,
) -> Option<&'static str> {
    let text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if method == axum::http::Method::HEAD {
        return Some("head");
    }
    if let Some(reason) = agent_reason(headers) {
        return Some(reason);
    }
    if let Some(accept) = text("accept") {
        if !accept.contains("text/html") && !accept.contains("*/*") {
            return Some("accept");
        }
    }
    let purpose = text("sec-purpose")
        .or_else(|| text("purpose"))
        .unwrap_or("");
    if purpose.contains("prefetch") || purpose.contains("prerender") {
        return Some("prefetch");
    }
    if let Some(dest) = text("sec-fetch-dest") {
        if dest != "document" {
            return Some("fetch");
        }
    }
    None
}

/// The user-agent half of the gate alone: `no_ua` when absent, `ua` when
/// it names a crawler or script. The right check for a request a page's
/// own script sends, which carries no document headers.
pub fn agent_reason(headers: &axum::http::HeaderMap) -> Option<&'static str> {
    let Some(ua) = headers.get("user-agent").and_then(|v| v.to_str().ok()) else {
        return Some("no_ua");
    };
    let ua = ua.to_ascii_lowercase();
    [
        "bot",
        "crawl",
        "spider",
        "slurp",
        "preview",
        "curl",
        "wget",
        "python",
        "httpx",
        "go-http",
        "headless",
        "lighthouse",
        "ureq/",
        // Safari as it shipped on iOS 13.2.3 in 2019, a user agent
        // crawlers still send word for word; a phone on that release
        // today is too rare to count.
        "iphone os 13_2_3 like mac os x) applewebkit/605.1.15 (khtml, like gecko) version/13.0.3 mobile/15e148",
    ]
    .iter()
    .any(|m| ua.contains(m))
    .then_some("ua")
}

/// True when the request looks like a crawler or script rather than a
/// person. Those must not inflate view counters.
pub fn is_probably_bot(headers: &axum::http::HeaderMap) -> bool {
    bot_reason(&axum::http::Method::GET, headers).is_some()
}

fn page_language(state: &AppState, page: &'static str, parts: &axum::http::request::Parts) {
    if is_probably_bot(&parts.headers) {
        return;
    }
    let languages = auth::accept_languages(&parts.headers);
    state.ext.page_language(state, page, &languages, parts);
}

/// The extension's anonymous product counter (event + ref only, never a
/// user id, session, or IP).
pub(crate) fn count_event(state: &AppState, event: &'static str, ref_code: Option<&str>) {
    state.ext.count_event(&state.services, event, ref_code);
}

/// The extension's per-account counter for something a signed-in user
/// did on the web (never a request address or user agent).
pub fn account_event(
    state: &AppState,
    user: UserId,
    event: &'static str,
    detail: Option<&str>,
    amount: Option<i64>,
) {
    record_event(
        state,
        user,
        AccountEvent::new(event, Surface::Web)
            .detail(detail)
            .amount(amount),
    );
}

/// `account_event` with every field the event can carry.
pub(crate) fn record_event(state: &AppState, user: UserId, event: AccountEvent<'_>) {
    state.ext.account_event(&state.services, user, event);
}

/// The labels the core's own links may carry as `?via=`: where a visit
/// came from, for the account's `view` event. A deployment adds its own
/// (`ServerExtension::via_labels`); anything else is dropped, so a URL
/// never writes its own text into the counts. `chat` is the link a tool
/// result tells the model about.
pub const VIA_LABELS: &[&str] = &["panel", "consent", "app", "chat"];

/// The `via` of the request's query, when it is an allowed label.
#[derive(Debug, Clone, Copy, Default)]
pub struct Via(pub Option<&'static str>);

impl axum::extract::FromRequestParts<AppState> for Via {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Via(parts.uri.query().and_then(|q| via_of(state, q))))
    }
}

/// The allowed `via` label in a query string: the core's or the
/// deployment's.
pub fn via_of(state: &AppState, query: &str) -> Option<&'static str> {
    query.split('&').find_map(|pair| {
        let value = pair.strip_prefix("via=")?;
        VIA_LABELS
            .iter()
            .chain(state.ext.via_labels())
            .copied()
            .find(|label| *label == value)
    })
}

/// A signed-in page was opened: its fixed name, and how the person got
/// there when the link said (`Via`).
pub(crate) fn view_event(state: &AppState, user: UserId, page: &'static str, via: Via) {
    record_event(
        state,
        user,
        AccountEvent::new("view", Surface::Web)
            .detail(Some(page))
            .via(via.0),
    );
}

/// A service error as a reply: its words (`ServiceError::words`) under
/// the status that fits it.
pub fn err_response(err: ServiceError) -> Response {
    let status = match &err {
        ServiceError::Invalid(_)
        | ServiceError::Refused { .. }
        | ServiceError::Malformed(_)
        | ServiceError::Store(flash_store::StoreError::Invalid(_)) => StatusCode::BAD_REQUEST,
        ServiceError::Store(flash_store::StoreError::NotFound(_)) => StatusCode::NOT_FOUND,
        ServiceError::OverCap { .. } | ServiceError::PolicyDenied { .. } => StatusCode::FORBIDDEN,
        ServiceError::Store(e) if e.is_constraint() => StatusCode::CONFLICT,
        other => {
            // Debug, so an `Internal` carries its detail to the log while
            // its Display stays "internal error".
            tracing::error!("service error: {other:?}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, err.words()).into_response()
}

/// Runs a blocking service call and renders the resulting template.
pub async fn respond<T, F>(services: Services, f: F) -> Response
where
    T: Template + Send + 'static,
    F: FnOnce(Services) -> Result<T, ServiceError> + Send + 'static,
{
    match crate::i18n::blocking(move || f(services)).await {
        Ok(Ok(template)) => match crate::i18n::render_current(&template) {
            Ok(html) => Html(html).into_response(),
            Err(e) => {
                tracing::error!("template render: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    crate::i18n::t("error-render"),
                )
                    .into_response()
            }
        },
        Ok(Err(err)) => err_response(err),
        Err(e) => {
            tracing::error!("join: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
    }
}

pub async fn act<T, F>(services: Services, f: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: FnOnce(Services) -> Result<T, ServiceError> + Send + 'static,
{
    match crate::i18n::blocking(move || f(services)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(err_response(err)),
        Err(e) => {
            tracing::error!("join: {e}");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response())
        }
    }
}

/// One row of a deck table (`deck_rows.html`): a deck as the lists show
/// it, with whether it has rows under it here and whether this browser
/// has folded it shut.
struct DeckListRow {
    id: i64,
    /// The whole path as a person reads it (`Bio › Cells`), for the hover.
    path: String,
    leaf: String,
    depth: u32,
    due_count: u32,
    new_count: u32,
    card_count: u32,
    archived_at: Option<i64>,
    /// Whether the page shows this row as it loads. Every row is in the
    /// page, shown or not: showing archived decks and folding a deck are
    /// then a matter for the page's script, at once and with no request
    /// (`static/ui.js`, which works out the same three things from the
    /// rows' depth and state), and the page still loads as it was left.
    hidden: bool,
    /// Whether a deck sits under it at all, so it carries a fold; and
    /// whether one that is showing does, so the fold is offered. A deck
    /// whose only subdecks are archived and out of sight has one and does
    /// not offer it.
    foldable: bool,
    has_subdecks: bool,
    /// Whether this browser folded it shut, whatever is showing under it.
    collapsed: bool,
    goal: Option<goal::GoalMark>,
    has_goal: bool,
}

/// Where a deck table sits, which is where its rows' buttons (fold,
/// unarchive) come back to: `decks` is the deck list, which swaps itself
/// in place; `today` and `deck` (a deck's own page, `from` being that
/// deck) reload. Every page that includes `deck_rows.html` carries one as
/// `rows`.
struct DeckRowsCtx {
    back: &'static str,
    from: i64,
    /// Whether an archived row offers to unarchive. Today lists none.
    unarchive: bool,
    /// Whether any row has a fold, so the rows without one leave its
    /// width free and every name starts in the same column. A list with
    /// no subdecks in it looks as it always did.
    folds: bool,
}

impl DeckRowsCtx {
    fn decks(rows: &[DeckListRow]) -> Self {
        Self::at("decks", 0, true, rows)
    }
    fn today(rows: &[DeckListRow]) -> Self {
        Self::at("today", 0, false, rows)
    }
    fn at(back: &'static str, from: i64, unarchive: bool, rows: &[DeckListRow]) -> Self {
        Self {
            back,
            from,
            unarchive,
            folds: rows.iter().any(|r| r.foldable),
        }
    }
}

/// Where a row's button returns to, from the `back` and `from` its form
/// sent. `from` only names a page; one that is not the caller's is not
/// found when it gets there.
fn rows_destination(back: Option<&str>, from: Option<i64>) -> String {
    match (back, from) {
        (Some("today"), _) => "/".to_string(),
        (Some("deck"), Some(deck)) => format!("/decks/{deck}"),
        _ => "/decks".to_string(),
    }
}

/// A row for every one of `decks` (in tree order), each marked with how
/// the page shows it as it loads. Every deck starts open. An archived
/// deck is hidden unless `show_archived`, and so is what sits under a
/// deck this browser folded shut, whose counts already hold what it
/// hides.
fn deck_list_rows(
    decks: Vec<DeckRow>,
    collapsed: &auth::CollapsedDecks,
    show_archived: bool,
) -> Vec<DeckListRow> {
    // What the page may show: everything, or everything not archived.
    let showable: Vec<bool> = decks
        .iter()
        .map(|d| show_archived || d.archived_at.is_none())
        .collect();
    let depths: Vec<u32> = decks.iter().map(|d| d.depth).collect();
    let mut rows = Vec::with_capacity(decks.len());
    // While set, rows deeper than this are inside a folded deck.
    let mut folded_at: Option<u32> = None;
    for (i, deck) in decks.into_iter().enumerate() {
        let foldable = depths.get(i + 1).is_some_and(|next| *next > deck.depth);
        let next_showable = (i + 1..depths.len()).find(|j| showable[*j]);
        let has_subdecks = next_showable.is_some_and(|j| depths[j] > deck.depth);
        let is_collapsed = collapsed.contains(deck.id.0);
        // A fold ends at the next deck that is not under the folded one.
        if folded_at.is_some_and(|depth| deck.depth <= depth) {
            folded_at = None;
        }
        let inside_fold = folded_at.is_some();
        if showable[i] && !inside_fold && has_subdecks && is_collapsed {
            folded_at = Some(deck.depth);
        }
        rows.push(DeckListRow {
            id: deck.id.0,
            path: deck.path,
            leaf: deck.leaf,
            depth: deck.depth,
            due_count: deck.due_count,
            new_count: deck.new_count,
            card_count: deck.card_count,
            archived_at: deck.archived_at,
            hidden: !showable[i] || inside_fold,
            foldable,
            has_subdecks,
            collapsed: is_collapsed,
            has_goal: deck.goal.is_some(),
            goal: deck.goal.as_ref().map(goal::mark),
        });
    }
    rows
}

/// One deck a deck sits in, as a link above its heading.
struct Crumb {
    id: i64,
    leaf: String,
}

/// A deck among the user's decks: the decks it sits in, itself, and the
/// decks under it. Both deck pages read the one list this way.
struct DeckFamily {
    crumbs: Vec<Crumb>,
    deck: DeckRow,
    /// In tree order, `depth` counted from the deck (its children are 0).
    subdecks: Vec<DeckRow>,
    /// Every other deck that is not under it: where it could move to.
    others: Vec<DeckRow>,
}

impl DeckFamily {
    fn of(decks: Vec<DeckRow>, id: DeckId) -> Result<Self, ServiceError> {
        let deck = decks
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or(ServiceError::Store(flash_store::StoreError::NotFound(
                "deck",
            )))?;
        let mut crumbs = Vec::new();
        let mut up = deck.parent_id;
        while let Some(parent) = up.and_then(|p| decks.iter().find(|d| d.id == p)) {
            crumbs.push(Crumb {
                id: parent.id.0,
                leaf: parent.leaf.clone(),
            });
            up = parent.parent_id;
        }
        crumbs.reverse();
        let (subdecks, others): (Vec<DeckRow>, Vec<DeckRow>) = decks
            .into_iter()
            .filter(|d| d.id != id)
            .partition(|d| flash_core::deck_path::is_descendant(&d.name, &deck.name));
        let subdecks = subdecks
            .into_iter()
            .map(|mut d| {
                d.depth = d.depth.saturating_sub(deck.depth + 1);
                d
            })
            .collect();
        Ok(Self {
            crumbs,
            deck,
            subdecks,
            others,
        })
    }
}

struct CardView {
    id: i64,
    front: String,
    back: String,
    front_html: Option<String>,
    back_html: Option<String>,
    front_lang: String,
    back_lang: String,
    tags: String,
    /// "cloze 2", "reversed", "typed": how this card relates to its note.
    kind: Option<crate::i18n::Words>,
    archived: bool,
    deck_label: Option<String>,
    deck_ref: i64,
}

impl CardView {
    fn from_row(s: &Services, user: UserId, c: CardRow) -> Result<CardView, ServiceError> {
        let front = s.labeled_side(user, &c, flash_store::CardSide::Front)?;
        let back = s.labeled_side(user, &c, flash_store::CardSide::Back)?;
        Ok(CardView {
            id: c.id.0,
            kind: card_kind_label(&c),
            archived: c.suspended,
            front_html: front.html.or(c.front_html),
            back_html: back.html.or(c.back_html),
            front_lang: front.lang,
            back_lang: back.lang,
            front: c.front,
            back: c.back,
            tags: c.tags.join(", "),
            deck_label: None,
            deck_ref: 0,
        })
    }
}

fn card_kind_label(card: &CardRow) -> Option<crate::i18n::Words> {
    use crate::i18n::{t, ta};
    match (card.cloze_index, card.ord, card.type_answer.is_some()) {
        (Some(n), _, _) => Some(ta("cards-kind-cloze", &[("n", i64::from(n).into())])),
        (None, 1, _) => Some(t("cards-kind-reversed")),
        (None, _, true) => Some(t("cards-kind-typed")),
        _ => None,
    }
}

/// Compact relative time for activity columns; falls back to the date
/// once it's over a month old.
pub fn format_ago(now_ms: i64, ts_ms: i64) -> crate::i18n::Words {
    let mins = (now_ms - ts_ms).max(0) / 60_000;
    let ago = |id: &str, n: i64| crate::i18n::ta(id, &[("count", n.into())]);
    match mins {
        0 => crate::i18n::t("when-just-now"),
        1..=59 => ago("when-minutes-ago", mins),
        60..=1439 => ago("when-hours-ago", mins / 60),
        1440..=43_199 => ago("when-days-ago", mins / 1440),
        _ => crate::i18n::Words::formatted(format_day(ts_ms)),
    }
}

pub fn format_day(ts_ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ts_ms)
        .map(|t| {
            let z = t.to_zoned(jiff::tz::TimeZone::UTC);
            format!("{:04}-{:02}-{:02}", z.year(), z.month(), z.day())
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, Method};

    use super::{bot_reason, clean_next};

    #[test]
    fn next_accepts_only_paths_this_origin_will_serve() {
        for good in ["/", "/decks", "/decks/3?limit=1", "/s/abc#top"] {
            assert_eq!(clean_next(Some(good.to_string())), good, "{good}");
        }
        // A percent-encoded control character is just a path byte on this
        // origin, so `/%0aevil` is fine; the raw characters are not.
        assert_eq!(clean_next(Some("/%0aevil".to_string())), "/%0aevil");
        for bad in [
            "//evil.example",
            "/\\evil.example",
            "/\t/evil.example",
            "/x\ny",
            "https://evil.example/",
            "javascript:alert(1)",
            "decks",
            "",
        ] {
            assert_eq!(clean_next(Some(bad.to_string())), "/", "{bad:?}");
        }
        assert_eq!(clean_next(Some("/".repeat(600))), "/");
        assert_eq!(clean_next(None), "/");
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    const BROWSER: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Safari/605.1";

    #[test]
    fn a_browser_page_load_is_a_person() {
        let h = headers(&[
            ("user-agent", BROWSER),
            ("accept", "text/html,application/xhtml+xml,*/*;q=0.8"),
            ("sec-fetch-dest", "document"),
        ]);
        assert_eq!(bot_reason(&Method::GET, &h), None);
        // A bare client that says nothing about itself beyond a UA passes too.
        assert_eq!(
            bot_reason(&Method::GET, &headers(&[("user-agent", BROWSER)])),
            None
        );
    }

    #[test]
    fn crawlers_probes_and_prefetches_are_named() {
        type Case = (&'static str, Method, Vec<(&'static str, &'static str)>);
        let cases: [Case; 6] = [
            ("no_ua", Method::GET, vec![]),
            ("ua", Method::GET, vec![("user-agent", "Googlebot/2.1")]),
            ("head", Method::HEAD, vec![("user-agent", BROWSER)]),
            (
                "accept",
                Method::GET,
                vec![("user-agent", BROWSER), ("accept", "application/json")],
            ),
            (
                "prefetch",
                Method::GET,
                vec![
                    ("user-agent", BROWSER),
                    ("sec-purpose", "prefetch;prerender"),
                ],
            ),
            (
                "fetch",
                Method::GET,
                vec![("user-agent", BROWSER), ("sec-fetch-dest", "empty")],
            ),
        ];
        for (reason, method, pairs) in cases {
            assert_eq!(bot_reason(&method, &headers(&pairs)), Some(reason));
        }
    }

    #[test]
    fn a_script_in_a_browsers_clothes_is_named_by_its_agent() {
        let page = |ua| {
            bot_reason(
                &Method::GET,
                &headers(&[("user-agent", ua), ("accept", "*/*")]),
            )
        };
        assert_eq!(page("ureq/3.4.2"), Some("ua"));
        assert_eq!(
            page(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/13.0.3 Mobile/15E148 Safari/604.1"
            ),
            Some("ua")
        );
        assert_eq!(
            page(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/26.6.1 Mobile/15E148 Safari/604.1"
            ),
            None
        );
    }
}
