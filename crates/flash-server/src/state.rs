//! Shared application state for all surfaces.

use std::any::Any;
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
/// (held in a cookie). In-memory: a restart just restarts the ceremony.
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
        state: PasskeyAuthentication,
        expires_ms: i64,
    },
    /// A signed-in user adding a passkey (settings, web or app).
    AddPasskey {
        state: PasskeyRegistration,
        user: flash_core::UserId,
        expires_ms: i64,
    },
    /// OAuth consent screen state: the validated authorize request.
    Consent {
        pending: crate::oauth::PendingConsent,
        expires_ms: i64,
    },
    /// A round trip owned by a sign-in provider (Google, Apple): the
    /// provider keeps its own state type and tags it with `kind`, so a
    /// cookie from one provider's ceremony is refused by another's.
    Ext {
        kind: &'static str,
        state: Box<dyn Any + Send + Sync>,
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
}

impl Site {
    /// The origin without its scheme, for prose ("cards.example.com · free").
    pub fn host(&self) -> &str {
        self.base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
    }

    /// The MCP endpoint users paste into Claude, ChatGPT, Grok, or Claude Code.
    pub fn mcp_url(&self) -> String {
        format!("{}/mcp", self.base_url)
    }

    /// The guided-setup prompt the "Set up with Claude / ChatGPT" buttons
    /// hand to the assistant; templates URL-encode it into the deep link.
    pub fn connect_prompt(&self) -> String {
        format!(
            "I want to connect Flash, my flashcard app, to you as a custom connector. \
             Its MCP server URL is: {}\n\n\
             Guide me through adding it on this platform, one step at a time, waiting \
             for me to say done after each step. Steps: open the connector settings for \
             this app; add a custom connector; paste the URL above exactly; confirm; then \
             I will be asked to sign in to Flash with my passkey to finish. After it \
             connects, list my decks to prove it works.",
            self.mcp_url()
        )
    }
}

#[derive(Clone)]
pub struct AppState {
    pub services: Services,
    pub config: Arc<Config>,
    pub site: Arc<Site>,
    /// What the hosted build plugged in; `CoreOnly` on the open server.
    pub ext: Arc<dyn ServerExtension>,
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Mutex<HashMap<String, Ceremony>>>,
    pub rate_limiter: Arc<crate::middleware::RateLimiter>,
    /// The looser per-IP budget for the mobile JSON API: a study session
    /// is a few requests per card, so the auth limiter would break it.
    pub api_rate_limiter: Arc<crate::middleware::RateLimiter>,
    /// Present => self-serve signup is enabled. Tests swap in a capture
    /// mailer; None keeps the classic invite-only behavior.
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
    /// Card generation and voice grading; None when no provider is
    /// configured, and every generate page and voice route says so.
    pub ai: Option<Arc<AiRuntime>>,
    /// Natural voice through the sidecar; None when none is configured,
    /// and every voice audio route says so.
    pub tts: Option<Arc<crate::tts::TtsRuntime>>,
    pub started_at_ms: i64,
}

/// Blob reads the server performs itself at once, for blobs large enough
/// to matter; smaller ones are not counted.
pub const MEDIA_READS: usize = 8;

/// Model calls in flight across the process: each holds a blocking
/// thread for up to minutes, and the runner's own bound is separate.
pub const AI_CALLS: usize = 4;

/// Everything a generation or a spoken-answer grade needs that a request
/// does not carry: the model behind one trait, what it reads (the
/// provider's own capabilities, narrowed by what the operator listed
/// and by whether a PDF renderer is present), the document converter
/// when there is one, and the handles the job runner is woken and
/// throttled through.
pub struct AiRuntime {
    pub provider: Arc<dyn crate::ai::provider::LlmProvider>,
    pub capabilities: crate::ai::provider::Capabilities,
    /// `capabilities.kinds`, the list every "which inputs" question
    /// reads.
    pub inputs: Vec<crate::policy::AiInput>,
    /// One renderer, used from blocking threads one at a time: rendering
    /// a page is CPU work that gains nothing from running twice at once.
    #[cfg(feature = "pdf")]
    pub pdf: Option<Mutex<crate::ai::pdf::Pdf>>,
    /// The operator's document converter; None refuses documents.
    pub converter: Option<crate::ai::convert::DocConverter>,
    /// Where the converter's scratch directories go.
    pub convert_dir: std::path::PathBuf,
    pub calls: tokio::sync::Semaphore,
    /// Poked when a job is queued, so the runner does not wait out its
    /// idle interval.
    pub wake: tokio::sync::Notify,
    /// What clients are told to send the grader for a spoken answer;
    /// the grader takes either shape regardless.
    pub voice_input: crate::config::VoiceInput,
}

impl AiRuntime {
    /// A runtime over `provider` reading what it can, narrowed to
    /// `inputs` when the operator listed some. Err names a listed kind
    /// the provider cannot read.
    pub fn new(
        provider: Arc<dyn crate::ai::provider::LlmProvider>,
        inputs: Option<Vec<crate::policy::AiInput>>,
        #[cfg(feature = "pdf")] pdf: Option<crate::ai::pdf::Pdf>,
        converter: Option<crate::ai::convert::DocConverter>,
        convert_dir: std::path::PathBuf,
    ) -> Result<Self, String> {
        let all = provider.capabilities();
        let mut capabilities = match &inputs {
            Some(listed) => all.narrowed_to(listed).map_err(|kind| {
                format!(
                    "FLASH_AI_INPUTS lists {kind} but the {} model does not read it",
                    provider.describe()
                )
            })?,
            None => all,
        };
        // Without a renderer there are no page counts, no figure crops
        // and no cap on pages: a listed `pdf` is a boot error upstream,
        // an implied one is simply dropped.
        #[cfg(feature = "pdf")]
        let pdf_ready = pdf.is_some();
        #[cfg(not(feature = "pdf"))]
        let pdf_ready = false;
        if inputs.is_none() && !pdf_ready && capabilities.accepts(crate::policy::AiInput::Pdf) {
            tracing::warn!(
                "pdf generation is off: FLASH_PDFIUM_PATH is not set (see tools/fetch-pdfium)"
            );
            capabilities = capabilities.without(crate::policy::AiInput::Pdf);
        }
        Ok(Self {
            provider,
            inputs: capabilities.kinds.clone(),
            capabilities,
            #[cfg(feature = "pdf")]
            pdf: pdf.map(Mutex::new),
            converter,
            convert_dir,
            calls: tokio::sync::Semaphore::new(AI_CALLS),
            wake: tokio::sync::Notify::new(),
            voice_input: crate::config::VoiceInput::default(),
        })
    }

    /// Builds the runtime the configuration describes: the configured
    /// provider, the renderer loaded from `FLASH_PDFIUM_PATH` when there
    /// is one, and the document converter when one is named. Errors
    /// name the missing piece.
    pub fn from_config(
        config: &crate::config::AiConfig,
        data_dir: &std::path::Path,
    ) -> Result<Self, String> {
        let provider = crate::ai::build_provider(config)?;
        let lists_pdf = config
            .inputs
            .as_ref()
            .is_some_and(|list| list.contains(&crate::policy::AiInput::Pdf));
        #[cfg(feature = "pdf")]
        let pdf = match &config.pdfium_path {
            Some(path) => Some(crate::ai::pdf::load(path)?),
            None => None,
        };
        #[cfg(not(feature = "pdf"))]
        if lists_pdf {
            return Err(
                "FLASH_AI_INPUTS lists pdf but this binary was built without the pdf feature"
                    .to_string(),
            );
        }
        let _ = lists_pdf;
        let converter = match &config.doc_converter {
            Some(template) => Some(crate::ai::convert::DocConverter::parse(
                template,
                config.doc_converter_timeout,
            )?),
            None => None,
        };
        let convert_dir = data_dir.join("convert");
        if converter.is_some() {
            std::fs::create_dir_all(&convert_dir)
                .map_err(|e| format!("{}: {e}", convert_dir.display()))?;
        }
        let mut runtime = Self::new(
            provider,
            config.inputs.clone(),
            #[cfg(feature = "pdf")]
            pdf,
            converter,
            convert_dir,
        )?;
        runtime.voice_input = config.voice_input;
        Ok(runtime)
    }

    pub fn accepts(&self, input: crate::policy::AiInput) -> bool {
        self.capabilities.accepts(input)
    }

    pub fn accepts_mime(&self, input: crate::policy::AiInput, mime: &str) -> bool {
        self.capabilities.accepts_mime(input, mime)
    }

    /// Whether office documents can be taken: a converter is configured
    /// and the PDF it makes can be read.
    pub fn converts_documents(&self) -> bool {
        self.converter.is_some() && self.accepts(crate::policy::AiInput::Pdf)
    }

    /// The renderer, when PDFs are on. Callers hold the guard only
    /// inside a blocking task.
    #[cfg(feature = "pdf")]
    pub fn pdf(&self) -> Option<parking_lot::MutexGuard<'_, crate::ai::pdf::Pdf>> {
        self.pdf.as_ref().map(|m| m.lock())
    }

    pub fn describe(&self) -> String {
        let inputs: Vec<&str> = self.inputs.iter().map(|i| i.as_str()).collect();
        let documents = match &self.converter {
            Some(c) if self.converts_documents() => format!("; documents via {}", c.program()),
            _ => String::new(),
        };
        format!(
            "{} ({}{documents})",
            self.provider.describe(),
            inputs.join(", ")
        )
    }
}

/// The pages' Content-Security-Policy. Scripts stay `'self'` only; inline
/// *style* attributes are allowed (cosmetic, no script surface). Images
/// and media come from us, or from the media store's origin when blobs
/// are fetched there directly: CSP applies to a redirect's target.
/// `blob:` images are the composer's thumbnails of photos not yet
/// uploaded, minted by the page's own script from the person's files.
pub fn page_csp(media_origin: Option<&str>) -> String {
    let media = media_origin.map(|o| format!(" {o}")).unwrap_or_default();
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:{media}; media-src 'self'{media}; \
         frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    )
}

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
    pub fn message(self) -> &'static str {
        match self {
            HeavyBusy::Yours => "another import, export or stats page of yours is still running; try again in a moment",
            HeavyBusy::Everyone => "the server is busy with other imports and exports; try again in a moment",
        }
    }
}

impl IntoResponse for HeavyBusy {
    fn into_response(self) -> Response {
        (StatusCode::TOO_MANY_REQUESTS, self.message()).into_response()
    }
}

/// No model is configured on this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiOff;

impl AiOff {
    pub const MESSAGE: &'static str = "card generation is not set up on this server";
}

impl IntoResponse for AiOff {
    fn into_response(self) -> Response {
        (StatusCode::NOT_FOUND, Self::MESSAGE).into_response()
    }
}

/// No natural-voice sidecar is configured on this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceOff;

impl VoiceOff {
    pub const MESSAGE: &'static str = "natural voice is not set up on this server";
}

impl IntoResponse for VoiceOff {
    fn into_response(self) -> Response {
        (StatusCode::NOT_FOUND, Self::MESSAGE).into_response()
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
        let ai = match &config.ai {
            Some(ai_config) => {
                let runtime = AiRuntime::from_config(ai_config, &config.data_dir)?;
                tracing::info!("card generation: {}", runtime.describe());
                Some(Arc::new(runtime))
            }
            None => None,
        };
        let tts = config.tts.as_ref().map(|tts_config| {
            let runtime = crate::tts::TtsRuntime::from_config(tts_config);
            tracing::info!("natural voice: {}", runtime.describe());
            Arc::new(runtime)
        });
        let csp = page_csp(media.origin().as_deref());
        let site = Arc::new(Site {
            base_url: config.base_url.clone(),
            support_email: config.support_email.clone(),
        });
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
            ai,
            tts,
            started_at_ms,
        })
    }

    /// Installs a natural-voice runtime (tests install the fake sidecar).
    pub fn install_tts(&mut self, tts: Arc<crate::tts::TtsRuntime>) {
        self.tts = Some(tts);
    }

    /// The voice runtime, or the refusal every voice audio route answers
    /// with when none is configured.
    pub fn tts(&self) -> Result<&Arc<crate::tts::TtsRuntime>, VoiceOff> {
        self.tts.as_ref().ok_or(VoiceOff)
    }

    /// Swaps the media store (tests install fakes) and everything derived
    /// from it.
    pub fn install_media(&mut self, media: Arc<dyn crate::media_store::MediaStore>) {
        self.csp = page_csp(media.origin().as_deref());
        self.media = media;
    }

    /// Installs a generation runtime (tests install the fake provider).
    pub fn install_ai(&mut self, ai: Arc<AiRuntime>) {
        self.ai = Some(ai);
    }

    /// The generation runtime, or the refusal every AI route answers
    /// with when none is configured.
    pub fn ai(&self) -> Result<&Arc<AiRuntime>, AiOff> {
        self.ai.as_ref().ok_or(AiOff)
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

    /// Parks a provider-owned ceremony under `kind`.
    pub fn put_ext_ceremony<T: Any + Send + Sync>(
        &self,
        id_hash: String,
        kind: &'static str,
        state: T,
        expires_ms: i64,
    ) {
        self.put_ceremony(
            id_hash,
            Ceremony::Ext {
                kind,
                state: Box::new(state),
                expires_ms,
            },
        );
    }

    /// Takes a provider-owned ceremony; None when absent, expired, or
    /// parked by a different provider.
    pub fn take_ext_ceremony<T: Any + Send + Sync>(
        &self,
        id_hash: &str,
        kind: &'static str,
        now_ms: i64,
    ) -> Option<T> {
        match self.take_ceremony(id_hash, now_ms)? {
            Ceremony::Ext {
                kind: found, state, ..
            } if found == kind => state.downcast().ok().map(|b| *b),
            _ => None,
        }
    }

    /// Mutates a live provider-owned ceremony in place; false when there
    /// is none of that kind.
    pub fn update_ext_ceremony<T: Any + Send + Sync>(
        &self,
        id_hash: &str,
        kind: &'static str,
        f: impl FnOnce(&mut T),
    ) -> bool {
        let mut found = false;
        self.update_ceremony(id_hash, |c| {
            if let Ceremony::Ext { kind: k, state, .. } = c {
                if *k == kind {
                    if let Some(s) = state.downcast_mut::<T>() {
                        f(s);
                        found = true;
                    }
                }
            }
        });
        found
    }

    /// Mutates a live ceremony in place; false when there is none.
    pub fn update_ceremony(&self, id_hash: &str, f: impl FnOnce(&mut Ceremony)) -> bool {
        let mut map = self.ceremonies.lock();
        match map.get_mut(id_hash) {
            Some(c) => {
                f(c);
                true
            }
            None => false,
        }
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
            | Ceremony::AddPasskey { expires_ms, .. }
            | Ceremony::Consent { expires_ms, .. }
            | Ceremony::Ext { expires_ms, .. } => *expires_ms,
        }
    }
}
