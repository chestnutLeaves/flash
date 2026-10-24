//! What a downstream extension plugs into the open server. The open
//! server is a complete product on its own (accounts, decks, study,
//! import, MCP) and `build_app` composes it from core routers plus
//! whatever the extension returns for each band. Every method has a no-op default;
//! `CoreOnly` implements nothing and is what the open binary installs.

use std::any::Any;
use std::sync::Arc;

use axum::http::HeaderValue;
use axum::Router;
use flash_core::{CardId, DeckId, UserId};
use flash_store::Store;

use crate::captcha::CaptchaVerifier;
use crate::flows::account::DeleteRefusal;
use crate::service::Services;
use crate::state::AppState;

/// Which JSON view of the account the extension is adding fields to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountView {
    /// `Me`: what every sign-in and `/me` return.
    Me,
    /// `Settings`: everything the settings screen renders.
    Settings,
}

/// Extra top-level JSON fields, flattened into a core object.
pub type JsonFields = serde_json::Map<String, serde_json::Value>;

/// Which of the product's surfaces an account action came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Web,
    Mobile,
    Mcp,
    Api,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Web => "web",
            Surface::Mobile => "mobile",
            Surface::Mcp => "mcp",
            Surface::Api => "api",
        }
    }
}

/// One thing a signed-in account did, for `ServerExtension::account_event`.
/// Each field holds one kind of fact, the same on every surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountEvent<'a> {
    pub event: &'static str,
    pub surface: Surface,
    /// A short fixed label, such as a login method or a tool's name.
    pub detail: Option<&'a str>,
    /// A number whose meaning the event's caller documents.
    pub amount: Option<i64>,
    /// The OAuth client's registered name, or the app's platform.
    pub client: Option<&'a str>,
    /// The fixed `?via=` label of the link followed (`web::Via`).
    pub via: Option<&'a str>,
}

impl<'a> AccountEvent<'a> {
    pub fn new(event: &'static str, surface: Surface) -> Self {
        AccountEvent {
            event,
            surface,
            detail: None,
            amount: None,
            client: None,
            via: None,
        }
    }

    pub fn detail(mut self, detail: Option<&'a str>) -> Self {
        self.detail = detail;
        self
    }

    pub fn amount(mut self, amount: Option<i64>) -> Self {
        self.amount = amount;
        self
    }

    pub fn client(mut self, client: Option<&'a str>) -> Self {
        self.client = client;
        self
    }

    pub fn via(mut self, via: Option<&'a str>) -> Self {
        self.via = via;
        self
    }
}

/// A consent page about to render, for `ServerExtension::consent_shown`.
pub struct ConsentShown<'a> {
    /// The name the client registered under, when it gave one.
    pub client: Option<&'a str>,
    pub headers: &'a axum::http::HeaderMap,
    /// The validated callback.
    pub redirect_uri: &'a str,
    /// The `state` the client sent, a one-time token never to be kept.
    pub oauth_state: Option<&'a str>,
}

/// Which part of the sidebar `ServerExtension::nav_html` is filling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    /// The study group at the top, after "Stats".
    Primary,
    /// The Account group, after the admin-only "Users" entry.
    Account,
    /// The sidebar footer, above "Sign out": entries that belong to the
    /// window rather than to a page, such as one that opens a dialog.
    Foot,
}

/// Where `Slot::AddCards` renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddCardsAt {
    /// The decks page's links, after Import: each entry starts with its
    /// own separator.
    DecksLinks,
    /// `deck`'s add menu, between Write and Import: `study-more-item`
    /// entries.
    DeckMenu { deck: DeckId },
    /// A finished study session, before "Back to Today": buttons.
    StudyDone,
}

/// The places in the core's pages that are the extension's to fill. Each
/// is rendered to a String the page emits with `|safe`; an extension
/// that has nothing for a slot returns "". The store-backed ones are
/// rendered inside the page's blocking closure.
pub enum Slot<'a> {
    /// login.html, above the server's own sign-in methods: external
    /// sign-in buttons, plus the error line when `err` is one of the
    /// extension's codes.
    LoginProviders {
        next: &'a str,
        err: Option<&'a str>,
        /// The page's `?ref=` code, already cleaned (`web::clean_ref`).
        ref_code: Option<&'a str>,
    },
    /// The challenge widget on the email-sending public forms.
    Captcha,
    /// settings: the extension's account card, replacing the core's plain
    /// "Your cards" card when non-empty. `query` is the raw query string
    /// the extension's own redirects carry flags in.
    SettingsAccount { user: UserId, query: &'a str },
    /// settings: connected external accounts.
    SettingsProviders { user: UserId, query: &'a str },
    /// settings, after the daily limits: the extension's own cards.
    SettingsCards { user: UserId, query: &'a str },
    /// dashboard, right under the "Connect your AI" card: one card of the
    /// extension's for this account, or nothing.
    DashboardCard { user: UserId },
    /// study, in the page's head after the tag picker: the extension's
    /// buttons.
    StudyActions,
    /// study, after the page's own dialogs and before its scripts: the
    /// extension's dialogs and script tags, already HTML.
    StudyFoot { user: UserId },
    /// More ways to add cards, where `AddCardsAt` says.
    AddCards(AddCardsAt),
    /// settings, delete card header: the extension's refusal badge.
    SettingsDeleteBadge { query: &'a str },
    /// settings, delete card: the extension's note under the warning.
    SettingsDeleteNote { user: UserId },
    /// deck settings, after the limits: one card of the extension's for
    /// the deck, or nothing.
    DeckSettingsCard { user: UserId, deck: DeckId },
    /// deck settings, at the end: the extension's card for the deck.
    DeckSettingsEnd {
        user: UserId,
        deck: DeckId,
        query: &'a str,
    },
    /// a deck's page, beside Study in its head: the extension's actions
    /// on the deck (a button and the dialog it opens).
    DeckHeader { user: UserId, deck: DeckId },
    /// The `<head>` of a public page the core renders whole (connect):
    /// the extension's own tags, such as a script that reports a visitor
    /// once they interact with the page.
    PublicHead,
    /// connect: steps replacing the core's custom-app steps when non-empty.
    ConnectAppSteps,
    /// consent: the page's whole content, replacing the core's own when
    /// non-empty, for an approval going back to a callback host the
    /// extension knows. It must still show that host and what the app
    /// may do, and carry the form that posts `token` with `decision`
    /// approve or deny to /oauth/consent. `redirect_host` is the
    /// validated callback's host, not the name the client registered
    /// under.
    ConsentBody {
        redirect_host: &'a str,
        token: &'a str,
    },
}

/// What the extension attached to an invite: the address the account is
/// fixed to and an opaque code. Admin invites carry none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignupInvite {
    pub email: String,
    pub ref_code: Option<String>,
}

/// Where account security consults the extension: the core never leaves
/// an account without a way in and never deletes the last admin; the
/// extension adds its own login methods and its own reasons to refuse a
/// deletion.
pub trait AccountHooks: Send + Sync {
    /// The signup details behind a valid invite; None for admin invites
    /// (the open server's only kind).
    fn signup_invite(
        &self,
        _store: &Store,
        _invite_token_hash: &str,
    ) -> flash_store::Result<Option<SignupInvite>> {
        Ok(None)
    }

    /// An account was just created from an invite; `signup` is what
    /// `signup_invite` returned for it, `invite_token_hash` names the
    /// invite it came from.
    ///
    /// `conn` is the enrollment's own transaction: what the hook writes
    /// commits with the account or not at all, so it writes the
    /// database only (a mail it owes is a job, `flash_store::enqueue_job`)
    /// and calls no `Store` method, whose lock the transaction holds.
    fn user_enrolled(
        &self,
        _conn: &flash_store::Connection,
        _user: UserId,
        _signup: Option<&SignupInvite>,
        _invite_token_hash: &str,
        _now_ms: i64,
    ) -> flash_store::Result<()> {
        Ok(())
    }

    /// An external login that keeps the account reachable without a
    /// password or passkey, so either may be removed.
    fn has_external_login(&self, _store: &Store, _user: UserId) -> flash_store::Result<bool> {
        Ok(false)
    }

    /// Runs after the last-admin check and before the purge. `Err` on
    /// the inner result aborts with nothing deleted; `Ok` may have had
    /// side effects of its own (a grant revoked, say).
    fn before_delete(
        &self,
        _state: &AppState,
        _user: UserId,
        _now_ms: i64,
    ) -> crate::service::Result<std::result::Result<(), DeleteRefusal>> {
        Ok(Ok(()))
    }
}

pub trait ServerExtension: AccountHooks + Send + Sync + 'static {
    /// For the extension's own handlers to reach their own state through
    /// `AppState.ext` (`as_any().downcast_ref()`).
    fn as_any(&self) -> &dyn Any;

    // ---- routers, one per band in build_app ----

    /// The crawler-facing site root: `/`, `/robots.txt` and any other
    /// pages the deployment wants to own there. None keeps the
    /// core's: dashboard-or-login at `/`, robots.txt disallowing all.
    fn site_routes(&self, _state: &AppState) -> Option<Router<AppState>> {
        None
    }

    /// Whether a path the extension serves answers the same to everyone,
    /// so it may be cached (the core's own assets already are). False
    /// leaves it `no-store` unless its handler says otherwise.
    fn cacheable_path(&self, _path: &str) -> bool {
        false
    }

    /// The admin users page at `/users`, for a deployment that knows more
    /// about its accounts than the core does. None keeps the core's list.
    /// Served under the same guards as the browser routes.
    fn users_routes(&self, _state: &AppState) -> Option<Router<AppState>> {
        None
    }

    /// Browser routes under the same-origin mutation guard.
    fn browser_routes(&self, _state: &AppState) -> Router<AppState> {
        Router::new()
    }

    /// Browser routes under the same-origin guard *and* the per-IP auth
    /// rate limit: credential ceremonies, and anything else a stranger
    /// can hit repeatedly.
    fn auth_routes(&self, _state: &AppState) -> Router<AppState> {
        Router::new()
    }

    /// Outside the CSRF guard: signature-authenticated webhooks and
    /// cross-site form posts from identity providers. Apply any rate
    /// limit inside the returned router.
    fn protocol_routes(&self, _state: &AppState) -> Router<AppState> {
        Router::new()
    }

    /// Nested under /api/v1 by the core, so the API limiter and the
    /// no-store header apply.
    fn api_routes(&self, _state: &AppState) -> Router<AppState> {
        Router::new()
    }

    /// The handler for each kind of job the extension writes (`jobs`):
    /// work it owes someone that must survive a restart.
    fn job_handlers(&self) -> Vec<(&'static str, crate::jobs::Handler)> {
        Vec::new()
    }

    /// A job failed for good (`why` is its last error): whatever it owed
    /// will not happen without someone looking. The extension may tell
    /// the people who run the deployment.
    fn job_failed(&self, _store: &Store, _job: &flash_store::JobRow, _why: &str, _now_ms: i64) {}

    /// A page reported a script error, or a browser a policy violation,
    /// not seen in the week before (`client_errors`): something broke in
    /// someone's browser. The core logs it; the extension may tell the
    /// people who run the deployment.
    fn client_error_new(
        &self,
        _store: &Store,
        _error: &flash_store::ClientErrorNew<'_>,
        _now_ms: i64,
    ) {
    }

    /// Site-root JSON the platforms fetch (app association files).
    fn well_known_routes(&self, _state: &AppState) -> Router<AppState> {
        Router::new()
    }

    // ---- fragments the core's pages render ----

    /// Sidebar items for one section of base.html: `Primary` renders after
    /// "Stats", `Account` after "Users", `Foot` above "Sign out". `active`
    /// is the page's nav key; admin-only items check `is_admin`. Pure: no
    /// store.
    fn nav_html(&self, _section: NavSection, _active: &str, _is_admin: bool) -> String {
        String::new()
    }

    /// Markup appended to the end of every signed-in page: dialogs and
    /// overlays that belong to no single page, together with the styles
    /// and scripts they need. Rendered once per page, outside the layout,
    /// so nothing here shifts what the page itself drew. Pure: no store.
    fn overlay_html(&self, _is_admin: bool) -> String {
        String::new()
    }

    /// One slot's HTML. Store-backed slots are rendered on a blocking
    /// thread, inside the page's own closure.
    fn render(&self, _state: &AppState, _slot: Slot<'_>) -> crate::service::Result<String> {
        Ok(String::new())
    }

    /// Where "Create an account" points; None hides it.
    fn signup_path(&self, _state: &AppState) -> Option<String> {
        None
    }

    /// Whether the login page folds the server's own sign-in methods
    /// (passkey, password, reset, email signup) under one "Other ways to
    /// continue", leaving the extension's buttons as the page's first
    /// choice. For an extension whose own buttons are how most people
    /// sign in; false keeps every method in view.
    fn fold_own_sign_in(&self, _state: &AppState) -> bool {
        false
    }

    // ---- JSON the API carries for the extension ----

    /// Fields flattened into the `Me` / `Settings` objects. Runs on a
    /// blocking thread.
    fn account_json(
        &self,
        _state: &AppState,
        _s: &Services,
        _user: UserId,
        _view: AccountView,
    ) -> crate::service::Result<JsonFields> {
        Ok(JsonFields::new())
    }

    /// Fields flattened into each row of the admin users list. Runs on a
    /// blocking thread.
    fn admin_user_json(&self, _s: &Services, _user: UserId) -> crate::service::Result<JsonFields> {
        Ok(JsonFields::new())
    }

    /// `/meta` `features` beside the core's own (`signup`, `passkeys`),
    /// which a key of the same name replaces.
    fn features(&self, _state: &AppState) -> JsonFields {
        JsonFields::new()
    }

    /// Extra top-level `/meta` fields.
    fn meta_extra(&self, _state: &AppState) -> JsonFields {
        JsonFields::new()
    }

    /// The oldest app this server speaks to, when the extension needs a
    /// newer one than the core's `api::MIN_APP_VERSION`.
    fn min_app_version(&self, _state: &AppState) -> Option<&'static str> {
        None
    }

    /// Fields flattened into a study card the app is sent. Runs on a
    /// blocking thread.
    fn study_card_json(
        &self,
        _state: &AppState,
        _s: &Services,
        _user: UserId,
        _card: &crate::service::StudyCard,
    ) -> JsonFields {
        JsonFields::new()
    }

    /// `data-*` attributes on the study page's card element, for the
    /// extension's scripts: (name without `data-`, value), which the page
    /// escapes. `revealed` once the back is on screen. Runs on a blocking
    /// thread.
    fn study_card_data(
        &self,
        _state: &AppState,
        _user: UserId,
        _card: CardId,
        _revealed: bool,
    ) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    /// Fields flattened into a revealed card the app is sent. Runs on a
    /// blocking thread.
    fn reveal_json(
        &self,
        _state: &AppState,
        _s: &Services,
        _user: UserId,
        _reveal: &crate::service::Reveal,
    ) -> JsonFields {
        JsonFields::new()
    }

    /// The deployment's own words beside the core's (`i18n::CORE`): its
    /// messages in each language it speaks ((folder tag, text) pairs, a
    /// tag being `en`, `es`, `es-ES` or `es-419`); its public pages with
    /// an address per language Flash ships (`/guide`, `/es/guia`),
    /// served under each language's prefix by its own routes and English
    /// at the bare address (`i18n::context`); and the starts of the ids
    /// of its messages that only English has, which a language is
    /// shipped whole without (`i18n::Catalog::shipping`). A constant, so
    /// what renders what this server renders builds the same catalog
    /// (`i18n::server_catalog`). None by default.
    fn words(&self) -> &'static crate::i18n::DeploymentWords {
        &crate::i18n::DeploymentWords::NONE
    }

    /// An anonymous counter: an event name and an optional short code
    /// only, never a user id. Fire-and-forget.
    fn count_event(&self, _s: &Services, _event: &'static str, _ref_code: Option<&str>) {}

    /// The `?via=` labels the links this deployment hands out carry,
    /// beside the core's own (`web::VIA_LABELS`).
    fn via_labels(&self) -> &'static [&'static str] {
        &[]
    }

    /// The consent page is about to render for `user`. Fire-and-forget.
    fn consent_shown(&self, _state: &AppState, _user: UserId, _consent: &ConsentShown<'_>) {}

    /// `page` ("login" or "connect") was loaded by a browser asking for
    /// `languages`, most wanted first. Never keep who. Fire-and-forget.
    fn page_language(
        &self,
        _state: &AppState,
        _page: &'static str,
        _languages: &[String],
        _parts: &axum::http::request::Parts,
    ) {
    }

    /// A signed-in account did something (`AccountEvent`). Fire-and-forget;
    /// never a request address or a user agent's text.
    fn account_event(&self, _s: &Services, _user: UserId, _event: AccountEvent<'_>) {}

    // ---- study reminders ----

    /// Whether this deployment can remind `user` when cards are due and
    /// they have not turned that on, so a finished study session may
    /// offer it. The open server sends no reminders.
    fn reminders_offer(&self, _state: &AppState, _user: UserId) -> crate::service::Result<bool> {
        Ok(false)
    }

    /// Turns study reminders on for `user` from a finished session's
    /// offer (`from` names which: `study`, `panel`; when they go is the
    /// deployment's to decide), and records it when this call is what
    /// turned them on. The text tells the person what was set up;
    /// None when this deployment keeps no reminders or cannot send this
    /// account any.
    fn turn_on_reminders(
        &self,
        _state: &AppState,
        _user: UserId,
        _from: &'static str,
        _now_ms: i64,
    ) -> crate::service::Result<Option<crate::i18n::Words>> {
        Ok(None)
    }

    // ---- the challenge on email-sending public forms ----

    /// Verifies the form's challenge token; None means no challenge.
    fn captcha_verifier(&self, _state: &AppState) -> Option<Arc<dyn CaptchaVerifier>> {
        None
    }

    /// The Content-Security-Policy a page carrying the challenge widget
    /// needs (its script and frame origins).
    fn captcha_csp(&self, _state: &AppState) -> Option<HeaderValue> {
        None
    }
}

/// The open server: nothing plugged in.
pub struct CoreOnly;

impl AccountHooks for CoreOnly {}

impl ServerExtension for CoreOnly {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
