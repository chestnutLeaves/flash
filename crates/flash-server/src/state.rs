//! Shared application state for all surfaces.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use flash_core::UserId;

use webauthn_rs::prelude::*;

use crate::config::Config;
use crate::ext::ServerExtension;
use crate::service::Services;

/// Short-lived WebAuthn ceremony state, keyed by a random ceremony id
/// (held in a cookie). In-memory: a restart just restarts the ceremony,
/// a few seconds of someone's time. What a restart must not cut off (an
/// OAuth consent screen, a provider sign-in) is a durable ceremony in
/// the database instead (`put_durable_ceremony`).
pub enum Ceremony {
    Registration {
        state: PasskeyRegistration,
        invite_token_hash: String,
        display_name: String,
        /// Normalized (trimmed, lowercased) email collected at start.
        email: String,
        /// The extension's signup details when the invite came from its
        /// verification loop; None for admin invites.
        signup: Option<crate::ext::SignupInvite>,
        role: String,
        /// Local path to land on once enrolled.
        next: String,
        expires_ms: i64,
    },
    Authentication {
        state: DiscoverableAuthentication,
        expires_ms: i64,
    },
    /// A signed-in user adding a passkey (settings, web or app).
    AddPasskey {
        state: PasskeyRegistration,
        user: flash_core::UserId,
        expires_ms: i64,
    },
}

/// The deployment's public identity, handed to every page that prints its
/// own address: canonical links, OG tags, the MCP URL people paste into
/// their AI, and the support address. Nothing in the binary names a host.
pub struct Site {
    /// Canonical public origin without a trailing slash (FLASH_BASE_URL).
    pub base_url: String,
    /// FLASH_SUPPORT_EMAIL; None hides every "email us" line.
    pub support_email: Option<String>,
    /// The deployment's words (the open core's and an extension's), in
    /// the languages it gives readers: what its mail and jobs word with.
    pub i18n: Arc<crate::i18n::Catalog>,
}

impl Site {
    /// The origin without its scheme, for prose ("cards.example.com · free").
    pub fn host(&self) -> &str {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
    }

    /// The MCP endpoint users paste into Claude, ChatGPT, Grok, Mistral,
    /// Gemini, or Claude Code.
    pub fn mcp_url(&self) -> String {
        format!("{}/mcp", self.base_url)
    }

    /// English, for mail to the people who run the server: it stays
    /// English whoever's request sets it off.
    pub fn english(&self) -> crate::i18n::Tr {
        crate::i18n::Tr::english(self.i18n.clone())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub services: Services,
    pub config: Arc<Config>,
    pub site: Arc<Site>,
    /// What a downstream extension plugged in; `CoreOnly` on the open
    /// server.
    pub ext: Arc<dyn ServerExtension>,
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Mutex<HashMap<String, Ceremony>>>,
    pub rate_limiter: Arc<crate::middleware::RateLimiter>,
    /// The looser per-IP budget for the mobile JSON API: a study session
    /// is a few requests per card, so the auth limiter would break it.
    pub api_rate_limiter: Arc<crate::middleware::RateLimiter>,
    /// Present => mail goes out. Tests swap in a capture mailer; None
    /// sends nothing.
    pub mailer: Option<Arc<dyn crate::email::Mailer>>,
    /// Where media bytes live: a bucket when configured, disk otherwise.
    /// Never optional: serving must always work.
    pub media: Arc<dyn crate::media_store::MediaStore>,
    /// The pages' Content-Security-Policy, built once: it names the media
    /// store's origin when blobs are fetched from there directly.
    pub csp: String,
    /// Blob reads the server itself performs (stores without presigned
    /// URLs, and the API's byte route) that are large enough to matter,
    /// bounded so a burst of big fetches cannot exhaust memory.
    pub media_reads: Arc<tokio::sync::Semaphore>,
    /// At most four Argon2 hashes in flight: each holds 64 MiB and a core,
    /// and flash.service caps the process at 1 GiB.
    pub pw_semaphore: Arc<tokio::sync::Semaphore>,
    /// One import (preview or commit) at a time: a 100 MB package is
    /// parsed in memory, and a retrying phone must not stack them.
    pub import_semaphore: Arc<tokio::sync::Semaphore>,
    /// Upload bodies being received at once (imports and media). Taken
    /// before a body is read, so the per-request size limit multiplied by
    /// concurrency stays within the process's memory budget.
    pub upload_semaphore: Arc<tokio::sync::Semaphore>,
    /// The gate every heavy job (import, export, Stats) must pass; see
    /// `HeavyJobs`.
    pub heavy: Arc<HeavyJobs>,
    /// Signs the media links the MCP study panel loads, with the
    /// deployment's key (kept in the database, so links outlive a
    /// restart).
    pub panel_links: crate::mcp::panel::MediaLinks,
    pub started_at_ms: i64,
    /// Cancelled when the process is asked to stop (`boot::serve`): the
    /// job runner stops taking jobs, and background senders hand over
    /// what they hold before the process exits.
    pub shutdown: tokio_util::sync::CancellationToken,
    /// Poked when a request writes a job (`jobs::wake`), so the worker
    /// does not wait out its poll.
    pub jobs_wake: Arc<tokio::sync::Notify>,
    /// Every word Flash says, in each language: the core's messages and
    /// the extension's (`i18n`).
    pub i18n: Arc<crate::i18n::Catalog>,
}

/// Blob reads the server performs itself at once, for blobs large enough
/// to matter; smaller ones are not counted.
pub const MEDIA_READS: usize = 8;

/// The pages' Content-Security-Policy. Scripts stay `'self'` only; inline
/// *style* attributes are allowed (cosmetic, no script surface). Images
/// and media come from us, or from the media store's origin when blobs
/// are fetched there directly: CSP applies to a redirect's target.
/// `blob:` images are thumbnails of photos not yet uploaded, minted by
/// a page's own script from the person's files. `blob:` media is a file
/// the person picked, read in the page before it is sent, as a photo's
/// thumbnail is shown.
pub fn page_csp(media_origin: Option<&str>) -> String {
    let media = media_origin.map(|o| format!(" {o}")).unwrap_or_default();
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:{media}; media-src 'self' blob:{media}; \
         frame-ancestors 'none'; base-uri 'none'; form-action 'self'; {CSP_REPORTING}"
    )
}

/// The end of every page's policy: where a browser reports what the
/// policy blocked (`client_errors`). `report-to` names the endpoint the
/// `Reporting-Endpoints` header declares (`middleware::security_headers`);
/// `report-uri` is for the browsers that only know the older form, and
/// the ones that know both use `report-to`.
pub const CSP_REPORTING: &str = "report-uri /client-errors/csp; report-to csp";

/// The `Reporting-Endpoints` header that goes with `CSP_REPORTING`.
pub const REPORTING_ENDPOINTS: &str = "csp=\"/client-errors/csp\"";

/// How many heavy jobs the whole process runs at once. Each holds a core
/// and up to a few hundred megabytes; the unit file caps the process at
/// one gigabyte.
pub const HEAVY_GLOBAL: usize = 4;

/// Work that holds a core or a large buffer for seconds: an import
/// preview or commit, an export, the review-history scan behind Stats.
/// One such job per user at a time and `HEAVY_GLOBAL` across the
/// process. The functions that do this work take a `&HeavyPermit`, so a
/// new heavy path cannot be wired up without passing the gate.
pub struct HeavyJobs {
    inflight: Mutex<HashSet<UserId>>,
    global: Arc<tokio::sync::Semaphore>,
}

impl HeavyJobs {
    pub fn new(global: usize) -> Self {
        Self {
            inflight: Mutex::new(HashSet::new()),
            global: Arc::new(tokio::sync::Semaphore::new(global)),
        }
    }

    /// A permit for `user`, or why not: they already have a job running,
    /// or the process is at its limit. Never waits.
    pub fn acquire(self: &Arc<Self>, user: UserId) -> Result<HeavyPermit, HeavyBusy> {
        let mut inflight = self.inflight.lock();
        if !inflight.insert(user) {
            return Err(HeavyBusy::Yours);
        }
        match self.global.clone().try_acquire_owned() {
            Ok(global) => Ok(HeavyPermit {
                user,
                jobs: Arc::clone(self),
                _global: global,
            }),
            Err(_) => {
                inflight.remove(&user);
                Err(HeavyBusy::Everyone)
            }
        }
    }

    /// Users with a job running right now.
    pub fn inflight(&self) -> usize {
        self.inflight.lock().len()
    }
}

/// Proof that the holder may run one heavy job for `user()`; released
/// on drop.
pub struct HeavyPermit {
    user: UserId,
    jobs: Arc<HeavyJobs>,
    _global: tokio::sync::OwnedSemaphorePermit,
}

impl HeavyPermit {
    pub fn user(&self) -> UserId {
        self.user
    }
}

impl std::fmt::Debug for HeavyPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeavyPermit({})", self.user)
    }
}

impl Drop for HeavyPermit {
    fn drop(&mut self) {
        self.jobs.inflight.lock().remove(&self.user);
    }
}

/// Why a heavy job could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeavyBusy {
    /// This user already has an import, export or Stats page in flight.
    Yours,
    /// The process is running as many heavy jobs as it will.
    Everyone,
}

impl HeavyBusy {
    /// What the person reads, in the reader's language.
    pub fn message(self) -> crate::i18n::Words {
        crate::i18n::t(match self {
            HeavyBusy::Yours => "heavy-busy-yours",
            HeavyBusy::Everyone => "heavy-busy-everyone",
        })
    }
}

impl IntoResponse for HeavyBusy {
    fn into_response(self) -> Response {
        (StatusCode::TOO_MANY_REQUESTS, self.message()).into_response()
    }
}

impl AppState {
    pub fn new(
        services: Services,
        config: Config,
        ext: Arc<dyn ServerExtension>,
        started_at_ms: i64,
    ) -> Result<Self, String> {
        let origin = Url::parse(&config.base_url).map_err(|e| format!("FLASH_BASE_URL: {e}"))?;
        let rp_id = origin
            .host_str()
            .ok_or("FLASH_BASE_URL has no host")?
            .to_string();
        // Passkeys are bound to a domain, so the relying-party id has to be
        // a hostname: a bare IP address such as http://192.168.1.5:8437 is
        // rejected here. Say so, instead of surfacing the library's wording.
        let webauthn_err = |e: WebauthnError| {
            format!(
                "webauthn: {e} (FLASH_BASE_URL is {}; passkeys need a hostname, \
                 not an IP address: http://localhost:8437 or a DNS name works)",
                config.base_url
            )
        };
        let webauthn = WebauthnBuilder::new(&rp_id, &origin)
            .map_err(webauthn_err)?
            .rp_name("flash")
            .build()
            .map_err(webauthn_err)?;
        let mailer: Option<Arc<dyn crate::email::Mailer>> =
            match (&config.mail, config.dev_mail_log) {
                (Some(mail), _) => Some(Arc::new(crate::email::ResendMailer::new(
                    mail,
                    config.support_email.as_deref(),
                ))),
                (None, true) => Some(Arc::new(crate::email::LogMailer)),
                (None, false) => None,
            };
        let media: Arc<dyn crate::media_store::MediaStore> = crate::media_store::from_config(
            config.media_r2.as_ref(),
            &config.data_dir,
            flash_store::media::MAX_FILE_BYTES + 1,
        )?
        .into();
        tracing::info!("media store: {}", media.describe());
        let csp = page_csp(media.origin().as_deref());
        let panel_links = crate::mcp::panel::MediaLinks::stored(services.store())
            .map_err(|e| format!("panel link key: {e}"))?;
        let i18n = crate::i18n::server_catalog(ext.words(), &config.languages)?;
        let site = Arc::new(Site {
            base_url: config.base_url.clone(),
            support_email: config.support_email.clone(),
            i18n: i18n.clone(),
        });
        let jobs_wake = Arc::new(tokio::sync::Notify::new());
        Ok(Self {
            services,
            config: Arc::new(config),
            site,
            ext,
            webauthn: Arc::new(webauthn),
            ceremonies: Arc::new(Mutex::new(HashMap::new())),
            // 20 requests/min bursting to 30, per IP+path: generous for
            // humans and OAuth flows, hostile to guessing.
            rate_limiter: Arc::new(crate::middleware::RateLimiter::new(30, 20)),
            // 300/min bursting to 600 per IP: ~100 cards a minute with
            // reveal + grade + media, still far below anything abusive.
            api_rate_limiter: Arc::new(crate::middleware::RateLimiter::new(600, 300)),
            mailer,
            media,
            pw_semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            import_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            upload_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
            heavy: Arc::new(HeavyJobs::new(HEAVY_GLOBAL)),
            csp,
            media_reads: Arc::new(tokio::sync::Semaphore::new(MEDIA_READS)),
            panel_links,
            started_at_ms,
            shutdown: tokio_util::sync::CancellationToken::new(),
            jobs_wake,
            i18n,
        })
    }

    /// Swaps the media store (tests install fakes) and everything derived
    /// from it.
    pub fn install_media(&mut self, media: Arc<dyn crate::media_store::MediaStore>) {
        self.csp = page_csp(media.origin().as_deref());
        self.media = media;
    }

    /// The permit a heavy job needs: one per user at a time, a few per
    /// process, and no more than the auth limiter's per-minute budget for
    /// one user, so a scripted loop of exports is throttled too.
    pub fn heavy(&self, user: UserId) -> Result<HeavyPermit, HeavyBusy> {
        if !self.rate_limiter.allow(&format!("heavy:{}", user.raw())) {
            return Err(HeavyBusy::Yours);
        }
        self.heavy.acquire(user)
    }

    pub fn take_ceremony(&self, id_hash: &str, now_ms: i64) -> Option<Ceremony> {
        let mut map = self.ceremonies.lock();
        // Opportunistic cleanup keeps the map from accumulating abandoned
        // ceremonies; `put_ceremony` does the same when the map is large.
        map.retain(|_, c| c.expires_ms() > now_ms);
        map.remove(id_hash)
    }

    /// Parks a ceremony that must outlive a restart (an OAuth consent
    /// screen, a provider sign-in round trip) in the database under
    /// `kind`, for `user` when it has one. False when it could not be
    /// parked (the store failed, or is full of live ceremonies), and the
    /// caller answers that the sign-in can't start just now.
    pub async fn put_durable_ceremony<T: serde::Serialize>(
        &self,
        id_hash: String,
        kind: &'static str,
        user: Option<UserId>,
        state: &T,
        expires_ms: i64,
    ) -> bool {
        let Ok(payload) = serde_json::to_string(state) else {
            return false;
        };
        let services = self.services.clone();
        let parked = tokio::task::spawn_blocking(move || {
            services.store().put_durable_ceremony(
                &id_hash,
                kind,
                user,
                &payload,
                expires_ms,
                crate::service::now_ms(),
            )
        })
        .await;
        match parked {
            Ok(Ok(true)) => true,
            Ok(Ok(false)) => {
                tracing::warn!(kind, "ceremony store full; refusing a new ceremony");
                false
            }
            Ok(Err(e)) => {
                tracing::error!(kind, "park ceremony: {e}");
                false
            }
            Err(e) => {
                tracing::error!(kind, "park ceremony join: {e}");
                false
            }
        }
    }

    /// Takes a durable ceremony of `kind`, with the account it was
    /// parked for; None when absent, expired, of another kind, or not
    /// readable. It is gone after either way.
    pub async fn take_durable_ceremony<T: serde::de::DeserializeOwned>(
        &self,
        id_hash: String,
        kind: &'static str,
    ) -> Option<(T, Option<UserId>)> {
        let services = self.services.clone();
        let taken = tokio::task::spawn_blocking(move || {
            services
                .store()
                .take_durable_ceremony(&id_hash, kind, crate::service::now_ms())
        })
        .await;
        match taken {
            Ok(Ok(Some((payload, user)))) => serde_json::from_str(&payload)
                .ok()
                .map(|state| (state, user)),
            Ok(Ok(None)) => None,
            Ok(Err(e)) => {
                tracing::error!(kind, "take ceremony: {e}");
                None
            }
            Err(e) => {
                tracing::error!(kind, "take ceremony join: {e}");
                None
            }
        }
    }

    /// Changes a live durable ceremony of `kind` in place; false when
    /// there is none.
    pub async fn update_durable_ceremony<T>(
        &self,
        id_hash: String,
        kind: &'static str,
        f: impl FnOnce(&mut T) + Send + 'static,
    ) -> bool
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let services = self.services.clone();
        let updated = tokio::task::spawn_blocking(move || {
            services.store().update_durable_ceremony(
                &id_hash,
                kind,
                crate::service::now_ms(),
                |payload| {
                    let mut state: T = serde_json::from_str(payload).ok()?;
                    f(&mut state);
                    serde_json::to_string(&state).ok()
                },
            )
        })
        .await;
        matches!(updated, Ok(Ok(true)))
    }

    pub fn put_ceremony(&self, id_hash: String, ceremony: Ceremony) {
        let mut map = self.ceremonies.lock();
        // Several ceremony kinds start unauthenticated (passkey login,
        // provider sign-in), so the map must not depend on a finish
        // arriving to shed what expired.
        if map.len() >= CEREMONY_SWEEP_AT {
            let now = crate::service::now_ms();
            map.retain(|_, c| c.expires_ms() > now);
        }
        if map.len() >= CEREMONY_HARD_CAP {
            // Still full of live ceremonies: a flood. Refusing a new one
            // fails that one login attempt; keeping it would eventually
            // fail the process.
            tracing::warn!(
                len = map.len(),
                "ceremony store full; refusing a new ceremony"
            );
            return;
        }
        map.insert(id_hash, ceremony);
    }
}

/// Map size at which `put_ceremony` sweeps expired entries first.
const CEREMONY_SWEEP_AT: usize = 1_000;
/// Live ceremonies past which new ones are refused.
const CEREMONY_HARD_CAP: usize = 20_000;

impl Ceremony {
    fn expires_ms(&self) -> i64 {
        match self {
            Ceremony::Registration { expires_ms, .. }
            | Ceremony::Authentication { expires_ms, .. }
            | Ceremony::AddPasskey { expires_ms, .. } => *expires_ms,
        }
    }
}
