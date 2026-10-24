//! Shared scaffolding for the HTTP integration tests: one test `Config`,
//! one way to build the router with fakes swapped in, one way to mint
//! sessions and tokens against the store, and one way to send a request
//! and read the reply. Behind the `test-support` feature so this crate's
//! tests and a downstream crate's tests build the app the same way.

pub mod bounded;
pub mod routes;

use std::path::PathBuf;
use std::sync::Arc;

use crate::api::{MOBILE_CLIENT_ID, MOBILE_SCOPE};
use crate::auth::{hash_token, new_token, SESSION_TTL_MS};
use crate::config::Config;
use crate::email::Mailer;
use crate::ext::ServerExtension;
use crate::media_store::{Blob, ByteRange, DiskStore, MediaStore, MediaStoreError};
use crate::policy::{CardPolicy, Unlimited};
use crate::service::{now_ms, Services};
use crate::state::AppState;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use flash_core::UserId;
use flash_store::Store;
use tower::ServiceExt;

/// The canonical origin every test config uses; same-origin form posts
/// must send it as `Origin`.
pub const BASE: &str = "http://localhost:8437";

/// The table's language `tag`, for a test that ships it or reads as it.
pub fn reader(tag: &str) -> flash_core::locale::UiLocale {
    flash_core::locale::UiLocale::by_tag(tag)
        .unwrap_or_else(|| panic!("the table of languages has no {tag}"))
}

/// Every language of the table a person can read but English, for a test
/// that ships them all.
pub fn other_languages() -> Vec<flash_core::locale::UiLocale> {
    flash_core::locale::UiLocale::real()
        .filter(|l| *l != flash_core::locale::UiLocale::EN)
        .collect()
}

/// Spain's Spanish, for a test that ships it or reads as it.
pub fn spain() -> flash_core::locale::UiLocale {
    reader("es-ES")
}

/// Latin America's Spanish, for a test that ships it or reads as it.
pub fn latin_america() -> flash_core::locale::UiLocale {
    reader("es-419")
}

/// A fresh per-binary, per-tag scratch directory (media, import previews).
pub fn data_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flash-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The all-features-off config.
pub fn config(data_dir: PathBuf) -> Config {
    Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        client_ip_header: None,
        access_log: false,
        base_url: BASE.to_string(),
        previous_base_urls: Vec::new(),
        support_email: None,
        data_dir,
        mail: None,
        dev_mail_log: false,
        media_r2: None,
        languages: crate::i18n::SHIPPED.to_vec(),
    }
}

/// A built router plus the handles tests reach past it with.
pub struct TestApp {
    pub app: Router,
    pub store: Arc<Store>,
    pub services: Services,
    pub data_dir: PathBuf,
    /// The state the router was built over, for tests that drive a
    /// background loop against the same app.
    pub state: AppState,
}

impl TestApp {
    /// Lets the app finish what it started: the writes it made off the
    /// request path (`Services::off_request`), and the jobs now due, with
    /// whatever they start in turn, until nothing more is due. After it,
    /// what that work wrote is there, and what is not there was never
    /// written. A test waits for this, never for a while.
    pub async fn settle(&self) {
        loop {
            self.state.services.settled().await;
            let state = self.state.clone();
            let ran = tokio::task::spawn_blocking(move || crate::jobs::run_due(&state))
                .await
                .expect("a pass over the due jobs");
            if ran == 0 {
                return;
            }
        }
    }
}

/// Builds an `AppState` the way production does, then swaps in whichever
/// fakes the test provides, exactly the overrides the per-file harnesses
/// used to apply by hand.
pub struct AppBuilder {
    tag: String,
    store: Option<Arc<Store>>,
    mailer: Option<Arc<dyn Mailer>>,
    ext: Option<Arc<dyn ServerExtension>>,
    policy: Option<Arc<dyn CardPolicy>>,
    data_dir: Option<PathBuf>,
    media: Option<Arc<dyn MediaStore>>,
    limits: Limits,
    previous_base_urls: Vec<String>,
    base_url: Option<String>,
    access_log: bool,
    job_worker: bool,
    languages: Option<Vec<flash_core::locale::UiLocale>>,
}

/// How the rate limiters are set.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Limits {
    /// As the server sets them.
    Production,
    /// `tight_rate_limits`.
    Tight,
    /// `loose_rate_limits`.
    Loose,
}

/// The burst every limiter allows under `tight_rate_limits`: a probe
/// sends one more than this and expects 429.
pub const TIGHT_BURST: u32 = 3;
/// The generous band's burst under `tight_rate_limits`: two more than
/// the strict band's, so the request that draws the 429 says which
/// limiter a route sits behind.
pub const TIGHT_BURST_GENEROUS: u32 = TIGHT_BURST + 2;

impl AppBuilder {
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.to_string(),
            store: None,
            mailer: None,
            ext: None,
            policy: None,
            data_dir: None,
            media: None,
            limits: Limits::Production,
            previous_base_urls: Vec::new(),
            base_url: None,
            access_log: false,
            job_worker: false,
            languages: None,
        }
    }

    /// Gives readers `languages` too, shipped or not: a test of a
    /// language before it ships.
    pub fn languages(mut self, languages: &[flash_core::locale::UiLocale]) -> Self {
        self.languages = Some(languages.to_vec());
        self
    }

    /// Serves at `origin` instead of `BASE`: a test that puts the app on a
    /// real socket at a port of its own, for a browser, needs the origin
    /// checks and the links the pages write to name that port.
    pub fn base_url(mut self, origin: &str) -> Self {
        self.base_url = Some(origin.trim_end_matches('/').to_string());
        self
    }

    /// The job worker in the background, as production runs it: for a
    /// test of the worker itself. Otherwise owed work runs when the test
    /// says (`TestApp::settle`, or `jobs::run_due_at` at an instant it
    /// chooses), never while it is looking.
    pub fn job_worker(mut self) -> Self {
        self.job_worker = true;
        self
    }

    /// Every rate limiter allows `TIGHT_BURST` requests and refills
    /// slowly, so a test can see a 429 in a handful of requests. In the
    /// harness there is no socket, so every request shares one bucket.
    pub fn tight_rate_limits(mut self) -> Self {
        self.limits = Limits::Tight;
        self
    }

    /// Rate limiters that never refuse: for a server that browser tests
    /// sign in to dozens of times a minute from one address.
    pub fn loose_rate_limits(mut self) -> Self {
        self.limits = Limits::Loose;
        self
    }

    /// The request log on (`FLASH_ACCESS_LOG=1`).
    pub fn access_log(mut self) -> Self {
        self.access_log = true;
        self
    }

    /// An origin the server answered at before `BASE`
    /// (`FLASH_PREVIOUS_BASE_URLS`).
    pub fn previous_base_url(mut self, origin: &str) -> Self {
        self.previous_base_urls
            .push(origin.trim_end_matches('/').to_string());
        self
    }

    /// The media store to install instead of the disk store under the
    /// test's data dir: `PresigningStore` to exercise the redirect path.
    pub fn media(mut self, media: Arc<dyn MediaStore>) -> Self {
        self.media = Some(media);
        self
    }

    /// The server extension to install; the open server's `CoreOnly`
    /// unless a test plugs one in.
    pub fn ext(mut self, ext: Arc<dyn ServerExtension>) -> Self {
        self.ext = Some(ext);
        self
    }

    /// The card policy `Services` consults; `Unlimited` unless a test
    /// plugs a cap in.
    pub fn policy(mut self, policy: Arc<dyn CardPolicy>) -> Self {
        self.policy = Some(policy);
        self
    }

    /// Serves from `dir` (made by `data_dir`) instead of a fresh one: for
    /// a downstream harness that puts something in it before the build.
    pub fn data_dir(mut self, dir: PathBuf) -> Self {
        self.data_dir = Some(dir);
        self
    }

    /// Builds over an existing store (rebuilding the router mid-test with
    /// different fakes) instead of a fresh in-memory one.
    pub fn store(mut self, store: Arc<Store>) -> Self {
        self.store = Some(store);
        self
    }

    /// Present => the mail loops (password reset, and any an extension
    /// adds) are enabled with this capture mailer.
    pub fn mailer(mut self, mailer: Arc<dyn Mailer>) -> Self {
        self.mailer = Some(mailer);
        self
    }

    pub fn build(self) -> TestApp {
        let store = self
            .store
            .unwrap_or_else(|| Arc::new(Store::open_in_memory().unwrap()));
        let policy = self.policy.unwrap_or_else(|| Arc::new(Unlimited));
        let services = Services::with_policy(store.clone(), policy);
        let data_dir = self.data_dir.unwrap_or_else(|| data_dir(&self.tag));
        let mut config = config(data_dir.clone());
        config.previous_base_urls = self.previous_base_urls;
        if let Some(origin) = self.base_url {
            config.base_url = origin;
        }
        config.access_log = self.access_log;
        if let Some(languages) = &self.languages {
            config.languages = languages.clone();
        }
        let ext = self.ext.unwrap_or_else(|| Arc::new(crate::ext::CoreOnly));
        let mut state = AppState::new(services.clone(), config, ext, now_ms()).unwrap();
        if let Some(mailer) = self.mailer {
            state.mailer = Some(mailer);
        }
        if let Some(media) = self.media {
            state.install_media(media);
        }
        match self.limits {
            Limits::Production => {}
            Limits::Tight => {
                // Two different bursts, so a probe can tell which band
                // answered: the strict one (sign-in, reset, token minting)
                // refuses first.
                state.rate_limiter = Arc::new(crate::middleware::RateLimiter::new(TIGHT_BURST, 1));
                state.api_rate_limiter =
                    Arc::new(crate::middleware::RateLimiter::new(TIGHT_BURST_GENEROUS, 1));
            }
            Limits::Loose => {
                state.rate_limiter =
                    Arc::new(crate::middleware::RateLimiter::new(1_000_000, 1_000_000));
                state.api_rate_limiter =
                    Arc::new(crate::middleware::RateLimiter::new(1_000_000, 1_000_000));
            }
        }
        if self.job_worker && tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(crate::jobs::run(
                state.clone(),
                std::time::Duration::from_millis(25),
            ));
        }
        TestApp {
            app: crate::build_app(state.clone()),
            store,
            services,
            data_dir,
            state,
        }
    }
}

// ---- fixtures ----

/// A heavy-job permit for `user` from a gate of its own, for tests that
/// call an import, export or Stats function directly.
pub fn heavy_permit(user: UserId) -> crate::state::HeavyPermit {
    Arc::new(crate::state::HeavyJobs::new(1))
        .acquire(user)
        .expect("fresh gate")
}

/// A member account.
pub fn member(store: &Store, name: &str, email: &str) -> UserId {
    store
        .create_user(name, Some(email), "member", now_ms())
        .unwrap()
}

/// Mints a web session for `user` directly against the store and returns
/// the cookie header value a browser would send back.
pub fn web_session(store: &Store, user: UserId) -> String {
    let token = new_token();
    store
        .create_web_session(
            &hash_token(&token),
            user,
            "Chrome on Windows",
            now_ms(),
            now_ms() + SESSION_TTL_MS,
        )
        .unwrap();
    format!("__Host-flash={token}")
}

/// `web_session`, but opened the way an emailed sign-in link opens one:
/// it studies, and signs in again for what `auth::FullUser` guards.
pub fn link_session(store: &Store, user: UserId) -> String {
    let token = new_token();
    store
        .create_link_session(
            &hash_token(&token),
            user,
            "Safari on iPhone",
            now_ms(),
            now_ms() + SESSION_TTL_MS,
        )
        .unwrap();
    format!("__Host-flash={token}")
}

/// A verified member plus a live session cookie.
pub fn signed_in(store: &Store, name: &str, email: &str) -> (UserId, String) {
    let user = member(store, name, email);
    let cookie = web_session(store, user);
    (user, cookie)
}

/// An OAuth access token for `user` (the MCP bearer), minted directly.
pub fn oauth_bearer(store: &Store, user: UserId) -> String {
    let now = now_ms();
    let access = new_token();
    store
        .insert_oauth_token(
            &hash_token(&access),
            &hash_token(&new_token()),
            "test-client",
            user,
            "flashcards",
            "",
            None,
            now + 3_600_000,
            now + 86_400_000,
            now,
        )
        .unwrap();
    access
}

/// A first-party (mobile app) bearer for `user`, minted directly the way
/// the API's grant flow does.
pub fn api_bearer(store: &Store, user: UserId) -> String {
    let now = now_ms();
    let access = new_token();
    store
        .insert_oauth_token(
            &hash_token(&access),
            &hash_token(&new_token()),
            MOBILE_CLIENT_ID,
            user,
            MOBILE_SCOPE,
            "test device",
            None,
            now + 3_600_000,
            now + 86_400_000,
            now,
        )
        .unwrap();
    access
}

// ---- requests ----

/// A JSON API request: optional bearer, optional body.
pub fn json_req(
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<&serde_json::Value>,
) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::ACCEPT, "application/json");
    if let Some(b) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {b}"));
    }
    match body {
        Some(json) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    }
}

pub fn json_get(path: &str, bearer: Option<&str>) -> Request<Body> {
    json_req("GET", path, bearer, None)
}

pub fn json_post(path: &str, bearer: Option<&str>, body: &serde_json::Value) -> Request<Body> {
    json_req("POST", path, bearer, Some(body))
}

pub fn json_put(path: &str, bearer: Option<&str>, body: &serde_json::Value) -> Request<Body> {
    json_req("PUT", path, bearer, Some(body))
}

pub fn json_patch(path: &str, bearer: Option<&str>, body: &serde_json::Value) -> Request<Body> {
    json_req("PATCH", path, bearer, Some(body))
}

pub fn json_delete(path: &str, bearer: Option<&str>) -> Request<Body> {
    json_req("DELETE", path, bearer, None)
}

/// A fully read response.
pub struct Reply {
    pub status: StatusCode,
    pub headers: Vec<(String, String)>,
    pub bytes: Vec<u8>,
}

impl Reply {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.bytes).unwrap_or(serde_json::Value::Null)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        header_of(&self.headers, name)
    }

    /// The Location header, or "" when absent.
    pub fn location(&self) -> &str {
        self.header("location").unwrap_or("")
    }

    /// Every Set-Cookie header value.
    pub fn set_cookies(&self) -> Vec<String> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, v)| v.clone())
            .collect()
    }
}

/// What an in-memory connection holds unread each way.
pub const PIPE_BYTES: usize = 1 << 20;

/// The peer address a connection made by `connect_in_memory` comes from,
/// as the rate limiters and the request log read it.
pub const IN_MEMORY_PEER: std::net::SocketAddr = std::net::SocketAddr::V4(
    std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 40000),
);

/// One connection to the app over an in-memory pipe: the app served on
/// the far end as a listener serves it (HTTP/1 with upgrades, its peer at
/// `IN_MEMORY_PEER`), the client's end returned. No socket, so what a
/// busy machine does to its own connections never reaches the test, and
/// a test may run on a clock it advances (`start_paused`).
pub fn connect_in_memory(app: &Router) -> tokio::io::DuplexStream {
    let (client, server) = tokio::io::duplex(PIPE_BYTES);
    let app = app
        .clone()
        .layer(axum::extract::connect_info::MockConnectInfo(IN_MEMORY_PEER));
    let service = hyper_util::service::TowerToHyperService::new(app);
    tokio::spawn(async move {
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(hyper_util::rt::TokioIo::new(server), service)
            .with_upgrades()
            .await;
    });
    client
}

/// A listener with no socket under it, for what must be tested through
/// a listener (`boot::serve`'s stop): each `MemoryConnector::connect`
/// is one connection it accepts, from `IN_MEMORY_PEER`.
pub struct MemoryListener {
    incoming: tokio::sync::mpsc::Receiver<tokio::io::DuplexStream>,
}

/// Makes connections to its `MemoryListener`, and tells when the
/// listener is gone (nothing new will be taken).
#[derive(Clone)]
pub struct MemoryConnector {
    outgoing: tokio::sync::mpsc::Sender<tokio::io::DuplexStream>,
}

pub fn memory_listener() -> (MemoryListener, MemoryConnector) {
    let (outgoing, incoming) = tokio::sync::mpsc::channel(1);
    (MemoryListener { incoming }, MemoryConnector { outgoing })
}

impl axum::serve::Listener for MemoryListener {
    type Io = tokio::io::DuplexStream;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.incoming.recv().await {
            Some(io) => (io, IN_MEMORY_PEER),
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(IN_MEMORY_PEER)
    }
}

impl MemoryConnector {
    /// One request over a connection of its own, read whole; None when
    /// the listener is gone.
    pub async fn send(&self, req: Request<Body>) -> Option<Reply> {
        let (client, server) = tokio::io::duplex(PIPE_BYTES);
        self.outgoing.send(server).await.ok()?;
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(client))
                .await
                .ok()?;
        tokio::spawn(connection);
        let response = sender.send_request(req).await.ok()?;
        Some(reply(response.map(Body::new)).await)
    }

    /// Resolves once the listener is dropped: it takes nothing new.
    pub async fn listener_gone(&self) {
        self.outgoing.closed().await;
    }
}

/// Runs one request through the router and reads the whole reply.
pub async fn send(app: &Router, req: Request<Body>) -> Reply {
    reply(app.clone().oneshot(req).await.unwrap()).await
}

async fn reply(response: axum::response::Response) -> Reply {
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Reply {
        status,
        headers,
        bytes,
    }
}

/// Case-insensitive header lookup in a captured header list.
pub fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// A GET, optionally signed in. No User-Agent: the server takes a
/// UA-less request for a bot (`web::is_probably_bot`), and tests that
/// care set one.
pub fn get(path: &str, cookie: Option<&str>) -> Request<Body> {
    let mut req = Request::get(path);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    req.body(Body::empty()).unwrap()
}

/// A same-origin urlencoded form post, optionally signed in.
pub fn form_post(path: &str, cookie: Option<&str>, body: &str) -> Request<Body> {
    let mut req = Request::post(path)
        .header(header::ORIGIN, BASE)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    req.body(Body::from(body.to_string())).unwrap()
}

/// The number after `"key": ` in a study button's hx-vals, which is how
/// a test posts the session and card the page is showing.
pub fn hx_value(html: &str, key: &str) -> String {
    let marker = format!("\"{key}\": ");
    let at = html
        .find(&marker)
        .unwrap_or_else(|| panic!("{key} in {html}"))
        + marker.len();
    html[at..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect()
}

// ---- media store fakes ----

/// The origin `PresigningStore` hands out URLs on.
pub const PRESIGN_ORIGIN: &str = "https://media.example";

/// A disk store that also offers presigned URLs, the way the bucket
/// store does in production: `https://media.example/<key>?sig=test`.
/// Bytes still come from disk for uploads, exports and the API's byte
/// route.
pub struct PresigningStore {
    disk: DiskStore,
}

impl PresigningStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            disk: DiskStore::new(root),
        }
    }
}

impl MediaStore for PresigningStore {
    fn put(&self, sha256: &str, mime: &str, bytes: &[u8]) -> Result<(), MediaStoreError> {
        self.disk.put(sha256, mime, bytes)
    }

    fn get(&self, sha256: &str, range: Option<ByteRange>) -> Result<Blob, MediaStoreError> {
        self.disk.get(sha256, range)
    }

    fn delete(&self, sha256: &str) -> Result<(), MediaStoreError> {
        self.disk.delete(sha256)
    }

    fn describe(&self) -> String {
        "presigning test store".into()
    }

    fn presigned_get(
        &self,
        sha256: &str,
        mime: &str,
        _filename: &str,
        ttl: std::time::Duration,
    ) -> Option<Result<String, MediaStoreError>> {
        Some(Ok(format!(
            "{PRESIGN_ORIGIN}/{sha256}?sig=test&mime={mime}&ttl={}",
            ttl.as_secs()
        )))
    }

    fn origin(&self) -> Option<String> {
        Some(PRESIGN_ORIGIN.to_string())
    }
}

/// A fake of a service the server calls out to (a mail provider, a
/// bucket), reached with no socket: `agent` is a client set up as
/// production's (`http::agent`) whose every connection is an in-memory
/// pipe to this host. Serve it a router (`MemoryHost::router`), or take
/// its connections one at a time as plain streams
/// (`MemoryHost::accepting`) for a fake that reads the HTTP itself. As
/// with a listener, a connection is refused once nothing takes them any
/// more (the acceptor dropped, the host gone). The host runs on a
/// runtime of its own, so a plain `#[test]` can use it, and a test's own
/// clock never governs it.
pub struct MemoryHost {
    runtime: Option<tokio::runtime::Runtime>,
    connections: tokio::sync::mpsc::UnboundedSender<tokio::io::DuplexStream>,
}

/// Takes a `MemoryHost`'s connections, for a fake that reads the HTTP
/// itself on a thread of its own. Dropping it refuses the rest.
pub struct MemoryAcceptor {
    incoming: tokio::sync::mpsc::UnboundedReceiver<tokio::io::DuplexStream>,
    runtime: tokio::runtime::Handle,
}

impl MemoryAcceptor {
    /// The next connection made to the host, for a fake that serves it on
    /// an async task; None once no client can connect any more.
    pub async fn next(&mut self) -> Option<tokio::io::DuplexStream> {
        self.incoming.recv().await
    }

    /// The next connection made to the host, as a blocking stream; None
    /// once no client can connect any more. Call it off any async task.
    pub fn accept(&mut self) -> Option<MemoryStream> {
        let io = self.runtime.block_on(self.incoming.recv())?;
        Some(tokio_util::io::SyncIoBridge::new_with_handle(
            io,
            self.runtime.clone(),
        ))
    }
}

/// What a fake reading its own HTTP reads and writes: one connection.
pub type MemoryStream = tokio_util::io::SyncIoBridge<tokio::io::DuplexStream>;

impl MemoryHost {
    fn with_incoming() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<tokio::io::DuplexStream>,
    ) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("a runtime for the memory host");
        let (connections, incoming) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                runtime: Some(runtime),
                connections,
            },
            incoming,
        )
    }

    /// A host whose connections the test takes with the acceptor.
    pub fn accepting() -> (Self, MemoryAcceptor) {
        let (host, incoming) = Self::with_incoming();
        let runtime = host.handle();
        (host, MemoryAcceptor { incoming, runtime })
    }

    /// A host that refuses every connection: a service down, or one a
    /// test never expects to be called.
    pub fn unreachable() -> Self {
        Self::with_incoming().0
    }

    /// A host that answers every connection with `app`, its peer at
    /// `IN_MEMORY_PEER` as `connect_in_memory`'s is. Every request's body
    /// is read whole before `app` answers, so a handler may ignore it:
    /// hyper closes a connection whose body is unread, and a client still
    /// writing it fails with a broken pipe.
    pub fn router(app: Router) -> Self {
        let app = app
            .layer(axum::middleware::from_fn(read_whole))
            .layer(axum::extract::connect_info::MockConnectInfo(IN_MEMORY_PEER));
        let (host, mut incoming) = Self::with_incoming();
        host.handle().spawn(async move {
            while let Some(io) = incoming.recv().await {
                let service = hyper_util::service::TowerToHyperService::new(app.clone());
                tokio::spawn(async move {
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(hyper_util::rt::TokioIo::new(io), service)
                        .await;
                });
            }
        });
        host
    }

    /// Where a client dials this host: each pipe sent is one connection.
    pub fn dial(&self) -> tokio::sync::mpsc::UnboundedSender<tokio::io::DuplexStream> {
        self.connections.clone()
    }

    fn handle(&self) -> tokio::runtime::Handle {
        self.runtime
            .as_ref()
            .expect("the host's runtime runs until it is dropped")
            .handle()
            .clone()
    }

    /// A client of this host, set up as `http::agent` sets up one of
    /// production's. Any address reaches the host; give it `http://`.
    pub fn agent(&self, global_timeout: std::time::Duration, status_as_error: bool) -> ureq::Agent {
        ureq::Agent::with_parts(
            crate::http::config(global_timeout, status_as_error),
            HostConnector {
                connections: self.connections.clone(),
                runtime: self.handle(),
            },
            HostResolver,
        )
    }
}

impl Drop for MemoryHost {
    fn drop(&mut self) {
        // Dropping a runtime blocks, which an async test may not do.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

async fn read_whole(
    request: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let (parts, body) = request.into_parts();
    match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[derive(Debug)]
struct HostResolver;

impl ureq::unversioned::resolver::Resolver for HostResolver {
    fn resolve(
        &self,
        _uri: &axum::http::Uri,
        _config: &ureq::config::Config,
        _timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut addrs = self.empty();
        addrs.push(IN_MEMORY_PEER);
        Ok(addrs)
    }
}

#[derive(Debug)]
struct HostConnector {
    connections: tokio::sync::mpsc::UnboundedSender<tokio::io::DuplexStream>,
    runtime: tokio::runtime::Handle,
}

impl ureq::unversioned::transport::Connector for HostConnector {
    type Out = HostTransport;

    fn connect(
        &self,
        details: &ureq::unversioned::transport::ConnectionDetails,
        _chained: Option<()>,
    ) -> Result<Option<HostTransport>, ureq::Error> {
        let (client, server) = tokio::io::duplex(PIPE_BYTES);
        self.connections.send(server).map_err(|_| {
            ureq::Error::Io(std::io::Error::from(std::io::ErrorKind::ConnectionRefused))
        })?;
        Ok(Some(HostTransport {
            io: client,
            buffers: ureq::unversioned::transport::LazyBuffers::new(
                details.config.input_buffer_size(),
                details.config.output_buffer_size(),
            ),
            runtime: self.runtime.clone(),
            open: true,
        }))
    }
}

/// One connection's client end, read and written as ureq's socket
/// transport does, with ureq's timeouts kept.
#[derive(Debug)]
struct HostTransport {
    io: tokio::io::DuplexStream,
    buffers: ureq::unversioned::transport::LazyBuffers,
    runtime: tokio::runtime::Handle,
    open: bool,
}

impl HostTransport {
    fn within<T>(
        runtime: &tokio::runtime::Handle,
        timeout: ureq::unversioned::transport::NextTimeout,
        work: impl std::future::Future<Output = std::io::Result<T>>,
    ) -> Result<T, ureq::Error> {
        let limit = timeout.not_zero().map(|d| *d);
        runtime.block_on(async move {
            match limit {
                Some(limit) => tokio::time::timeout(limit, work)
                    .await
                    .map_err(|_| ureq::Error::Timeout(timeout.reason))?
                    .map_err(ureq::Error::Io),
                None => work.await.map_err(ureq::Error::Io),
            }
        })
    }
}

impl ureq::unversioned::transport::Transport for HostTransport {
    fn buffers(&mut self) -> &mut dyn ureq::unversioned::transport::Buffers {
        &mut self.buffers
    }

    fn transmit_output(
        &mut self,
        amount: usize,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<(), ureq::Error> {
        use tokio::io::AsyncWriteExt;
        use ureq::unversioned::transport::Buffers;
        let Self {
            io,
            buffers,
            runtime,
            ..
        } = self;
        let output = &buffers.output()[..amount];
        Self::within(runtime, timeout, io.write_all(output))
    }

    fn await_input(
        &mut self,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<bool, ureq::Error> {
        use tokio::io::AsyncReadExt;
        use ureq::unversioned::transport::Buffers;
        let Self {
            io,
            buffers,
            runtime,
            open,
        } = self;
        let input = buffers.input_append_buf();
        let read = Self::within(runtime, timeout, io.read(input))?;
        buffers.input_appended(read);
        if read == 0 {
            *open = false;
        }
        Ok(read > 0)
    }

    fn is_open(&mut self) -> bool {
        use tokio::io::AsyncReadExt;
        // As ureq probes a socket before reusing it: open while a read
        // would wait, closed at its end, and unusable if bytes nobody
        // asked for are waiting.
        let Self {
            io, runtime, open, ..
        } = self;
        if !*open {
            return false;
        }
        let mut byte = [0u8; 1];
        let waiting = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::ZERO, io.read(&mut byte)).await
        });
        *open = waiting.is_err();
        *open
    }
}
