//! flash-server: the open, self-hosted Flash binary. Web UI, JSON API,
//! MCP and OAuth surfaces composed over the shared Services layer, with
//! nothing plugged in. A downstream extension builds its own binary the
//! same way with its routers merged into each band.

use std::process::ExitCode;
use std::sync::Arc;

use axum::routing::get;
use flash_server::boot;
use flash_server::config::Config;
use flash_server::ext::CoreOnly;
use flash_server::service::{now_ms, Services};
use flash_server::state::AppState;
use flash_store::Store;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {
            serve();
            ExitCode::SUCCESS
        }
        [command, email] if command == "reset-link" => reset_link(email),
        _ => {
            eprintln!("usage: flash-server [reset-link <email>]");
            ExitCode::from(2)
        }
    }
}

/// `flash-server reset-link <email>`: prints a one-time password-reset
/// link for that account and exits, for an instance that sends no mail.
/// Safe beside the running server. The link is a credential, so it goes
/// to stdout alone, never to the log.
fn reset_link(email: &str) -> ExitCode {
    let link = Config::from_env().and_then(|config| {
        let store = Store::open_current(&config.db_path()).map_err(|e| e.to_string())?;
        flash_server::flows::reset::link_for(&store, &config.base_url, email, now_ms())
            .map_err(|e| e.to_string())
    });
    match link {
        Ok(Some(link)) => {
            println!("{link}");
            eprintln!("It works once, within the next hour.");
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("No account has the email {email}.");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("reset-link: {e}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn serve() {
    tracing_subscriber::fmt().with_target(false).init();

    let config = Config::from_env().expect("config");
    boot::warn_about_shared_buckets(&config);
    std::fs::create_dir_all(&config.data_dir).expect("create data dir");
    let store = Store::open(&config.db_path()).expect("open database");
    let services = Services::new(Arc::new(store));
    boot::bootstrap_invite(&services, &config.base_url);

    let bind = config.bind;
    let state = AppState::new(services, config, Arc::new(CoreOnly), now_ms()).expect("app state");

    tokio::spawn(flash_server::scheduler::run_housekeeping(
        state.services.clone(),
        state.config.data_dir.clone(),
    ));
    // A stopping process waits for this: owed work kept in the database
    // until done.
    let background = vec![tokio::spawn(flash_server::jobs::run(
        state.clone(),
        flash_server::jobs::POLL,
    ))];

    let app = flash_server::build_app(state.clone())
        .route("/healthz", get(boot::healthz).with_state(state.clone()));

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .unwrap_or_else(|e| panic!("bind {bind}: {e}"));
    tracing::info!(%bind, "flash-server listening");
    // Until SIGTERM or Ctrl-C, then a bounded drain; peer addresses reach
    // the rate limiter through ConnectInfo.
    boot::serve(listener, app, state, background, boot::stop_requested()).await;
}
