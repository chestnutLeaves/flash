//! The static files, embedded: the binary is self-contained. KaTeX is
//! self-hosted too, since the CSP forbids CDNs.

use std::future::{ready, Ready};

use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

const JS: &str = "application/javascript";
const CSS: &str = "text/css";
const PNG: &str = "image/png";
const HOUR: &str = "max-age=3600";
const DAY: &str = "max-age=86400";

macro_rules! embedded {
    ($file:expr) => {
        include_bytes!(concat!("../../static/", $file)).as_slice()
    };
}

fn file(
    body: &'static [u8],
    mime: &'static str,
    max_age: &'static str,
) -> impl Fn() -> Ready<Response> + Clone + Send + Sync + 'static {
    move || {
        ready(
            (
                [
                    (header::CONTENT_TYPE, mime),
                    (header::CACHE_CONTROL, max_age),
                ],
                body,
            )
                .into_response(),
        )
    }
}

/// A font, open to any origin: the MCP panel draws its cards in them from
/// a host's sandbox, and a browser loads a font from another origin only
/// with this header.
fn font(body: &'static [u8]) -> impl Fn() -> Ready<Response> + Clone + Send + Sync + 'static {
    move || {
        ready(
            (
                [
                    (header::CONTENT_TYPE, "font/woff2"),
                    (header::CACHE_CONTROL, DAY),
                    (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
                ],
                body,
            )
                .into_response(),
        )
    }
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/static/og.png", get(file(embedded!("og.png"), PNG, DAY)))
        .route(
            "/static/app.css",
            get(file(embedded!("app.css"), CSS, HOUR)),
        )
        .route(
            "/static/htmx.min.js",
            get(file(embedded!("htmx.min.js"), JS, HOUR)),
        )
        .route(
            "/static/webauthn.js",
            get(file(embedded!("webauthn.js"), JS, HOUR)),
        )
        .route(
            "/static/study.js",
            get(file(embedded!("study.js"), JS, HOUR)),
        )
        .route(
            "/static/preview.js",
            get(file(embedded!("preview.js"), JS, HOUR)),
        )
        .route(
            "/static/favicon.svg",
            get(file(embedded!("favicon.svg"), "image/svg+xml", DAY)),
        )
        // The same tile as bitmaps: search engines and icon scrapers read
        // ICO/PNG, not SVG, and many try /favicon.ico before parsing
        // <link rel="icon">.
        .route(
            "/static/favicon-192.png",
            get(file(embedded!("favicon-192.png"), PNG, DAY)),
        )
        .route(
            "/static/apple-touch-icon.png",
            get(file(embedded!("apple-touch-icon.png"), PNG, DAY)),
        )
        .route(
            "/favicon.ico",
            get(file(embedded!("favicon.ico"), "image/x-icon", DAY)),
        )
        .route("/static/ui.js", get(file(embedded!("ui.js"), JS, HOUR)))
        .route(
            "/static/report.js",
            get(file(embedded!("report.js"), JS, HOUR)),
        )
        .route("/static/i18n.js", get(file(embedded!("i18n.js"), JS, HOUR)))
        .route("/static/dom.js", get(file(embedded!("dom.js"), JS, HOUR)))
        .route(
            "/static/timezone.js",
            get(file(embedded!("timezone.js"), JS, HOUR)),
        )
        .route(
            "/static/consent-once.js",
            get(file(embedded!("consent-once.js"), JS, HOUR)),
        )
        .route(
            "/static/editor.js",
            get(file(embedded!("editor.js"), JS, HOUR)),
        )
        .route("/static/deck.js", get(file(embedded!("deck.js"), JS, HOUR)))
        .route(
            "/static/fonts/Geist-Regular.woff2",
            get(font(embedded!("fonts/Geist-Regular.woff2"))),
        )
        .route(
            "/static/fonts/Geist-Medium.woff2",
            get(font(embedded!("fonts/Geist-Medium.woff2"))),
        )
        .route(
            "/static/fonts/Geist-SemiBold.woff2",
            get(font(embedded!("fonts/Geist-SemiBold.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-Regular.woff2",
            get(font(embedded!("fonts/GeistMono-Regular.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-Medium.woff2",
            get(font(embedded!("fonts/GeistMono-Medium.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-SemiBold.woff2",
            get(font(embedded!("fonts/GeistMono-SemiBold.woff2"))),
        )
        // Geist's extended part (Latin Extended, Cyrillic), loaded only
        // for a page whose text holds its characters (`i18n::scripts`).
        .route(
            "/static/fonts/Geist-Regular-ext.woff2",
            get(font(embedded!("fonts/Geist-Regular-ext.woff2"))),
        )
        .route(
            "/static/fonts/Geist-Medium-ext.woff2",
            get(font(embedded!("fonts/Geist-Medium-ext.woff2"))),
        )
        .route(
            "/static/fonts/Geist-SemiBold-ext.woff2",
            get(font(embedded!("fonts/Geist-SemiBold-ext.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-Regular-ext.woff2",
            get(font(embedded!("fonts/GeistMono-Regular-ext.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-Medium-ext.woff2",
            get(font(embedded!("fonts/GeistMono-Medium-ext.woff2"))),
        )
        .route(
            "/static/fonts/GeistMono-SemiBold-ext.woff2",
            get(font(embedded!("fonts/GeistMono-SemiBold-ext.woff2"))),
        )
        .route(
            "/static/katex/katex.min.css",
            get(file(embedded!("katex/katex.min.css"), CSS, HOUR)),
        )
        .route(
            "/static/katex/katex.min.js",
            get(file(embedded!("katex/katex.min.js"), JS, HOUR)),
        )
        .route(
            "/static/katex/auto-render.min.js",
            get(file(embedded!("katex/auto-render.min.js"), JS, HOUR)),
        )
        .route("/static/katex/fonts/{file}", get(katex_font))
}

async fn katex_font(Path(file): Path<String>) -> Response {
    macro_rules! fonts {
        ($($name:literal),+ $(,)?) => {
            match file.as_str() {
                $($name => Some(embedded!(concat!("katex/fonts/", $name))),)+
                _ => None,
            }
        };
    }
    let bytes: Option<&'static [u8]> = fonts!(
        "KaTeX_AMS-Regular.woff2",
        "KaTeX_Caligraphic-Bold.woff2",
        "KaTeX_Caligraphic-Regular.woff2",
        "KaTeX_Fraktur-Bold.woff2",
        "KaTeX_Fraktur-Regular.woff2",
        "KaTeX_Main-Bold.woff2",
        "KaTeX_Main-BoldItalic.woff2",
        "KaTeX_Main-Italic.woff2",
        "KaTeX_Main-Regular.woff2",
        "KaTeX_Math-BoldItalic.woff2",
        "KaTeX_Math-Italic.woff2",
        "KaTeX_SansSerif-Bold.woff2",
        "KaTeX_SansSerif-Italic.woff2",
        "KaTeX_SansSerif-Regular.woff2",
        "KaTeX_Script-Regular.woff2",
        "KaTeX_Size1-Regular.woff2",
        "KaTeX_Size2-Regular.woff2",
        "KaTeX_Size3-Regular.woff2",
        "KaTeX_Size4-Regular.woff2",
        "KaTeX_Typewriter-Regular.woff2",
    );
    match bytes {
        Some(b) => (
            [
                (header::CONTENT_TYPE, "font/woff2"),
                (header::CACHE_CONTROL, DAY),
            ],
            b,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
