//! Server library: composition of config, services, and (in later phases)
//! web, OAuth, and MCP surfaces. The binary in main.rs stays thin.

pub mod account_locale;
pub mod api;
pub mod auth;
pub mod boot;
pub mod bounds;
pub mod captcha;
pub mod client_errors;
pub mod config;
pub mod deck_languages;
pub mod email;
pub mod external_media;
pub mod ext;
pub mod flows;
pub mod http;
pub mod i18n;
pub mod jobs;
pub use flash_core::languages;
pub mod mcp;
pub mod media;
pub mod media_store;
pub mod middleware;
pub mod oauth;
pub mod password;
pub mod policy;
pub mod scheduler;
pub mod service;
pub mod state;
#[cfg(feature = "test-support")]
pub mod testing;
pub mod web;
pub mod webauthn;
pub mod when;

pub use mcp::panel as mcp_panel;
pub use service::{deck_cards, goals};
pub use web::goal as web_goal;

use axum::Router;
use state::AppState;

/// The complete application router: web UI + auth ceremonies (same-origin
/// guarded), OAuth + MCP (token-authenticated), all under security
/// headers, with the extension's routers merged into each band.
pub fn build_app(state: AppState) -> Router {
    let ext = state.ext.clone();
    let rate_limit =
        axum::middleware::from_fn_with_state(state.clone(), middleware::auth_rate_limit);
    let public_host = state
        .config
        .base_url
        .split("//")
        .nth(1)
        .unwrap_or("localhost:8437")
        .to_string();
    let mcp_routes = Router::new()
        .route(
            "/mcp-media/{id}",
            axum::routing::get(media::serve_media_connector),
        )
        .nest_service("/mcp", mcp::mcp_service(state.clone(), public_host))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            oauth::require_bearer,
        ))
        // The study panel's media: the signature is the credential (the
        // host's sandbox frame carries no Bearer token).
        .route(
            "/mcp-media/{id}/s",
            axum::routing::get(media::serve_media_signed),
        )
        // The body limit is the transport's own (`mcp::MCP_BODY_LIMIT`,
        // set on its config); the API's per-IP budget applies here.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::mcp_rate_limit,
        ));
    let app = Router::new()
        // Browser surfaces: same-origin mutation guard (CSRF), and the
        // per-address budget on every mutation (rate_limit_inventory_tests
        // probes each one).
        .merge(
            ext.site_routes(&state)
                .unwrap_or_else(web::site_routes)
                .merge(web::router())
                .merge(ext.users_routes(&state).unwrap_or_else(web::users_routes))
                .merge(client_errors::router(&state))
                .merge(ext.browser_routes(&state))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    middleware::web_mutation_rate_limit,
                )),
        )
        .merge(
            web::auth_router()
                .merge(oauth::browser_router())
                .merge(ext.auth_routes(&state))
                .layer(rate_limit.clone()),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::same_origin_mutations,
        ))
        // Protocol surfaces: token-authenticated, no Origin requirement.
        .merge(oauth::router().layer(rate_limit))
        // What a policy blocked, as the browser reports it: no Origin of
        // the page's, so outside the guard, with its own budget.
        .merge(client_errors::csp_router(&state))
        .merge(ext.protocol_routes(&state))
        .merge(mcp_routes)
        // Bearer-authenticated JSON with its own limiter, and whatever
        // the extension serves at the site root for the platforms.
        .merge(api::router(&state))
        .merge(ext.well_known_routes(&state))
        .fallback(web::not_found)
        // The session and the reader's language, worked out once for
        // every handler and template after it (`i18n::context`).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            i18n::context,
        ))
        // A panic anywhere in a request is a logged 500 for that request,
        // never a dropped connection, and (with no poisonable lock in
        // the process, see lock_hygiene_tests) never anything for the
        // next request. Inside the header layer so the reply is still a
        // normal reply.
        .layer(tower_http::catch_panic::CatchPanicLayer::custom(
            middleware::panic_response,
        ))
        // Before routing: every path segment and query string a handler
        // will see is bounded here (bounded_input_tests).
        .layer(axum::middleware::from_fn(middleware::uri_length))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::security_headers,
        ))
        // No request lives forever. hyper's own header-read timeout is
        // inert without a timer, so this is the only bound on a client
        // that connects and trickles; it is long enough for a 100 MB
        // upload on a slow link and answers 408 past that.
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        // Outermost, so the status it logs is the one the client got,
        // a timeout included. Still inside routing: it logs the route
        // template, never the path.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::access_log,
        ))
        .with_state(state.clone());
    // Before routing: a public page's address in a language goes to the
    // page's own route (`i18n::localize`). Every layer above runs after
    // the router's routing, so this one wraps the whole router as its
    // fallback, the one way in.
    Router::new()
        .fallback_service(app)
        .layer(axum::middleware::from_fn_with_state(state, i18n::localize))
}

/// Upper bound on one request, body included.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
