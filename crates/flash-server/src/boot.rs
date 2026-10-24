//! What every binary does around `build_app`: the health probe, the
//! first-boot admin invite, and serving until asked to stop.

use std::time::Duration;

use axum::{extract::State, Json};
use tokio::task::JoinHandle;

use crate::auth::{hash_token, new_token};
use crate::service::{now_ms, Services};
use crate::state::AppState;

/// The rate limiters key on the client's address. Behind a reverse proxy
/// every request arrives from the proxy, so without a configured header
/// the whole instance shares one bucket and twenty bad sign-ins a
/// minute from anyone lock everyone out. A loopback bind is the
/// documented proxy deployment, so the two together are worth a warning
/// on every boot until the header is set.
pub fn warn_about_shared_buckets(config: &crate::config::Config) {
    // The session cookie is always Secure, so over plain http the web UI
    // signs nobody in except on localhost, which browsers treat as a
    // secure context. Better said at boot than discovered at the login
    // form.
    let base = config.base_url.as_str();
    let plain_http = base.starts_with("http://");
    let loopback_base = base.starts_with("http://localhost")
        || base.starts_with("http://127.0.0.1")
        || base.starts_with("http://[::1]");
    if plain_http && !loopback_base {
        tracing::warn!(
            base_url = %config.base_url,
            "FLASH_BASE_URL is plain http on a non-loopback host: the web UI's session \
             cookie is Secure and browsers will drop it, so nobody can sign in through a \
             browser. Put the server behind HTTPS, or use http://localhost for a trial."
        );
    }
    if config.client_ip_header.is_none() && config.bind.ip().is_loopback() {
        tracing::warn!(
            bind = %config.bind,
            "FLASH_CLIENT_IP_HEADER is unset while listening on loopback: behind a proxy \
             every client shares one rate-limit bucket. Set it to the header your proxy \
             fills (x-real-ip, cf-connecting-ip) once nothing but the proxy can reach the port."
        );
    }
}

/// The commit this binary was built from, when the build named it
/// (`FLASH_GIT_SHA` at compile time); `unknown` otherwise. A version
/// names a release, and a deployment can run commits between releases.
pub const GIT_SHA: &str = match option_env!("FLASH_GIT_SHA") {
    Some(sha) => sha,
    None => "unknown",
};

/// `/healthz`: a real DB round-trip so "healthy" means "serving".
pub async fn healthz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let db_ok = tokio::task::spawn_blocking({
        let services = state.services.clone();
        move || services.store().health_check().is_ok()
    })
    .await
    .unwrap_or(false);
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": GIT_SHA,
        "db": if db_ok { "ok" } else { "error" },
        "uptime_ms": now_ms() - state.started_at_ms,
    }))
}

/// How long requests still open when the stop comes get to finish. An
/// assistant's stream or a WebSocket stays open for as long as
/// it is used, so waiting for all of them would hold a deploy forever.
const REQUEST_DRAIN: Duration = Duration::from_secs(15);

/// How long background work (the job runner, the mail thread) gets
/// after requests are done. With `REQUEST_DRAIN`, inside the 30 seconds
/// flash.service allows a stop.
const BACKGROUND_DRAIN: Duration = Duration::from_secs(10);

/// Resolves when the process is asked to stop: SIGTERM (systemd, a
/// deploy) or Ctrl-C.
pub async fn stop_requested() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = term => {}
    }
}

/// Serves `app` until `stop` resolves, then stops gracefully: no new
/// connections, open requests get `REQUEST_DRAIN` to finish,
/// `state.shutdown` is cancelled so background workers hand over what
/// they hold, and each of `background` gets the rest of the budget.
/// Blocking work already running (a mail being sent, a store write)
/// finishes as the runtime winds down. A restart then loses nothing a
/// worker had promised. `listener` is a socket in production and an
/// in-memory one in tests (`testing::memory_listener`).
pub async fn serve<L>(
    listener: L,
    app: axum::Router,
    state: AppState,
    background: Vec<JoinHandle<()>>,
    stop: impl std::future::Future<Output = ()> + Send + 'static,
) where
    L: axum::serve::Listener<Addr = std::net::SocketAddr>,
{
    use axum::serve::ListenerExt;
    let shutdown = state.shutdown.clone();
    let signalled = shutdown.clone();
    // `tap_io` doing nothing: axum hands a tapped listener's own address
    // to `ConnectInfo`, whatever the listener.
    let server = axum::serve(
        listener.tap_io(|_| {}),
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        stop.await;
        tracing::info!("stop requested: draining");
        signalled.cancel();
    });
    // The drain clock starts at the stop, not at boot.
    let drain_deadline = async {
        shutdown.cancelled().await;
        tokio::time::sleep(REQUEST_DRAIN).await;
    };
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                tracing::error!("serve: {e}");
            }
        }
        _ = drain_deadline => tracing::warn!("requests still open after the drain; closing them"),
    }
    // However serving ended, background work winds down now.
    shutdown.cancel();
    let background = async {
        for handle in background {
            let _ = handle.await;
        }
    };
    if tokio::time::timeout(BACKGROUND_DRAIN, background)
        .await
        .is_err()
    {
        tracing::warn!("background work still running after the drain");
    }
    // The next boot knows this stop was ours: work a downstream extension
    // queues and this stop cut off goes back to its queue without it
    // counting against the work.
    if let Err(e) = state
        .services
        .store()
        .set_server_state(CLEAN_STOP, &crate::service::now_ms().to_string())
    {
        tracing::warn!("record clean stop: {e}");
    }
    tracing::info!("stopped");
}

/// The `server_state` a graceful stop leaves for the next boot.
pub const CLEAN_STOP: &str = "clean_stop";

/// First boot on an empty database: mint a one-time admin enrollment link
/// and print it to the journal. Nothing else can create the first user.
pub fn bootstrap_invite(services: &Services, base_url: &str) {
    let store = services.store();
    match store.user_count() {
        Ok(0) => {
            let token = new_token();
            let now = now_ms();
            let day_ms = 24 * 60 * 60 * 1000;
            if let Err(e) = store.create_invite(
                &hash_token(&token),
                "Admin",
                "admin",
                None,
                now + day_ms,
                now,
            ) {
                tracing::error!("bootstrap invite: {e}");
                return;
            }
            tracing::info!("no users yet. Enroll the first admin within 24h at:");
            tracing::info!("{base_url}/enroll/{token}");
        }
        Ok(_) => {}
        Err(e) => tracing::error!("user_count: {e}"),
    }
}
