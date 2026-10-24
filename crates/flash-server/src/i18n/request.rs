//! A request's reader.

use std::sync::Arc;

use flash_core::locale::UiLocale;

use super::{split_path, tr, Catalog, PublicPage, Tr};

// global-state: one request's reader, set by `context` on that request's
// own task and gone with it.
tokio::task_local! {
    /// The reader of the request being served (`context`).
    static TR: Tr;
}

// global-state: the reader for one call, set and put back around it by
// `scoped` and `blocking`; nothing outlives the call.
thread_local! {
    /// The reader the work on this thread is for, set by `scoped` (an
    /// email or a job for one account) and by `blocking` (a request's
    /// work handed off).
    static SCOPED: std::cell::RefCell<Option<Tr>> = const { std::cell::RefCell::new(None) };
}

/// The reader the work at hand is for: the one a `scoped` call names
/// (an email to one account, even while serving another's request),
/// else the reader of the request being served (its handler, anything it
/// awaits, and the blocking work it hands off through `blocking`).
pub fn current() -> Option<Tr> {
    SCOPED
        .with(|b| b.borrow().clone())
        .or_else(|| TR.try_with(Tr::clone).ok())
}

/// Work on this thread is for `tr` until the guard drops: `scoped` for a
/// function that would rather not wrap its whole body.
pub fn enter(tr: Tr) -> Entered {
    Entered(
        SCOPED.with(|b| b.replace(Some(tr))),
        std::marker::PhantomData,
    )
}

/// The reader `enter` replaced, put back when this drops. It is not
/// `Send`, so it cannot be held across an `.await` that may move the work
/// to another thread, whose reader it would then put back.
pub struct Entered(Option<Tr>, std::marker::PhantomData<*const ()>);

impl Drop for Entered {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCOPED.with(|b| *b.borrow_mut() = previous);
    }
}

/// The future `f` working for `tr`, across its awaits and the blocking
/// work it hands off through `blocking`: a job that runs for one account
/// words what it stores in that account's language.
pub async fn within<F: std::future::Future>(tr: Tr, f: F) -> F::Output {
    TR.scope(tr, f).await
}

/// `f` working for `tr`: everything it words (messages, numbers, dates)
/// is in `tr`'s language. For work that is not one request's (an email,
/// a job); `f` must not await.
pub fn scoped<T>(tr: Tr, f: impl FnOnce() -> T) -> T {
    let _entered = enter(tr);
    f()
}

/// `f` on the blocking pool, working for the reader of the request that
/// hands it off, so what it renders speaks their language.
pub async fn blocking<T, F>(f: F) -> Result<T, tokio::task::JoinError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let tr = current();
    tokio::task::spawn_blocking(move || match tr {
        Some(tr) => scoped(tr, f),
        None => f(),
    })
    .await
}

/// The reader of the work at hand as askama's runtime values:
/// `.render_with_values(&i18n::reader_values())`. Work with no reader
/// (a job that set none with `scoped`) reads English, so a page or an
/// email is never left unrendered.
pub fn reader_values() -> Tr {
    tr()
}

/// The reader of work for `user` with no request behind it (a job, a
/// scheduled email): the account's language, else English.
pub fn for_user(state: &crate::state::AppState, user: flash_core::UserId) -> Tr {
    for_account(
        state.services.store(),
        user,
        &Tr::english(state.i18n.clone()),
    )
}

/// The reader mail and jobs for `user` are written for: the language
/// the account holds (chosen, or learned from its browser, assistant or
/// phone), when a language this server gives readers answers it; else
/// `fallback` (the reader of the request that set the work off, or
/// English).
pub fn for_account(store: &flash_store::Store, user: flash_core::UserId, fallback: &Tr) -> Tr {
    let Ok(Some((tag, _))) = store.account_locale(user) else {
        return fallback.clone();
    };
    let zone = store.get_settings(user).ok().map(|s| s.timezone);
    let catalog = fallback.catalog().clone();
    match flash_core::locale::negotiate_tags(&[&tag], zone.as_deref(), catalog.shipped()) {
        Some(n) => Tr::new(catalog, n.locale, Some(&n.format_tag)),
        None => fallback.clone(),
    }
}

/// A reader as askama's runtime values: itself, under `"tr"`.
impl askama::Values for Tr {
    fn get_value<'a>(&'a self, key: &str) -> Option<&'a dyn std::any::Any> {
        (key == "tr").then_some(self as &dyn std::any::Any)
    }
}

/// A template rendered for the reader of the request being served.
pub fn render_current<T: askama::Template>(template: &T) -> askama::Result<String> {
    template.render_with_values(&reader_values())
}

/// The paths a page is never served from: they get no session lookup,
/// only what their own headers say.
fn serves_pages(path: &str) -> bool {
    ![
        "/static/",
        "/mcp",
        "/api/",
        "/media/",
        "/mcp-media/",
        "/.well-known/",
        "/oauth/token",
    ]
    .iter()
    .any(|p| path.starts_with(p))
}

/// The language a link's query names (`hl=es-419`), when it names one.
fn hl(query: Option<&str>) -> Option<&str> {
    query?.split('&').find_map(|kv| kv.strip_prefix("hl="))
}

/// Which language a request is answered in, source by source in this
/// order: the account's chosen language, then the language it holds
/// (learned from its browser, assistant or phone), then a `?hl=` on the
/// link, then what the browser asks for, as one list. A signed-out
/// visitor is not asked: the device's language is theirs. The first
/// source a language this server gives readers answers decides
/// (`negotiate`); a source naming
/// only languages it does not give is passed over. English when none
/// does. Never where the request comes from. A variant told apart by
/// time zone goes by the zone the person studies in, the account's or the
/// browser's.
pub fn reader(
    catalog: &Arc<Catalog>,
    headers: &axum::http::HeaderMap,
    query: Option<&str>,
    account: Option<&crate::auth::AccountLang>,
) -> Tr {
    let jar = axum_extra::extract::cookie::CookieJar::from_headers(headers);
    let zone = account.and_then(|a| a.zone.clone()).or_else(|| {
        jar.get(crate::when::ZONE_COOKIE)
            .map(|c| c.value().to_string())
    });
    let sources: [Vec<String>; 3] = [
        account.and_then(|a| a.locale.clone()).into_iter().collect(),
        hl(query).map(str::to_string).into_iter().collect(),
        crate::auth::accept_languages(headers),
    ];
    let asked = sources[2].clone();
    for tags in &sources {
        if let Some(n) =
            flash_core::locale::negotiate_tags(tags, zone.as_deref(), catalog.shipped())
        {
            return Tr::new(catalog.clone(), n.locale, Some(&n.format_tag)).asking(asked, zone);
        }
    }
    Tr::english(catalog.clone()).asking(asked, zone)
}

/// A public page asked for at its address in a language (`/es/guia`),
/// which `localize` handed to the page's own route: the language the
/// address names and the page, for `context` and the access log.
#[derive(Clone, Copy, Debug)]
pub struct Localized {
    pub locale: UiLocale,
    pub page: PublicPage,
}

/// Where `localize` hands a public page with no English address (one
/// written for a single language's readers): one route serves them all,
/// and reads `Localized` to know which page was asked for. Asked for at
/// this address itself, nothing was localized, and the route answers as
/// at no page.
pub const LANGUAGE_ONLY_PAGE: &str = "/language-only-page";

/// Before routing: a public page's address in a shipped language it is
/// written in goes to the page's own route (`/es/guia` to `/guide`,
/// the query kept), carrying the language beside it (`Localized`), so a
/// page has one route whatever the number of languages. A page with no
/// English address goes to `LANGUAGE_ONLY_PAGE`. Only reads are handed
/// on: a form posts to the English address, its language on the link
/// (`?hl=`).
pub async fn localize(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::Method;
    let reads = matches!(*request.method(), Method::GET | Method::HEAD);
    let found = split_path(request.uri().path())
        .filter(|(locale, _)| reads && state.i18n.shipped().contains(locale))
        .and_then(|(locale, rest)| Some((locale, state.i18n.page_in(locale, &rest)?)));
    if let Some((locale, page)) = found {
        let route = page.en.unwrap_or(LANGUAGE_ONLY_PAGE);
        let target = match request.uri().query() {
            Some(query) => format!("{route}?{query}"),
            None => route.to_string(),
        };
        let mut uri = request.uri().clone().into_parts();
        if let Ok(path_and_query) = target.parse() {
            uri.path_and_query = Some(path_and_query);
            if let Ok(uri) = axum::http::Uri::from_parts(uri) {
                *request.uri_mut() = uri;
            }
        }
        request.extensions_mut().insert(Localized { locale, page });
    }
    next.run(request).await
}

/// The request's reader and session, worked out once before any handler
/// runs: the session is cached for every extractor after it
/// (`auth::CachedSession`), the reader set for everything the handler
/// renders (`current`), and a page says the language it is in
/// (`Content-Language`) and what chose it (`Vary`).
pub async fn context(
    axum::extract::State(state): axum::extract::State<crate::state::AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header;
    let (mut parts, body) = request.into_parts();
    let pages = serves_pages(parts.uri.path());
    let account = if pages {
        let found = crate::auth::lookup_session(&parts, &state).await;
        parts
            .extensions
            .insert(crate::auth::CachedSession(found.as_ref().map(|(s, _)| *s)));
        found.map(|(_, lang)| lang)
    } else {
        None
    };
    let wanted = reader(
        &state.i18n,
        &parts.headers,
        parts.uri.query(),
        account.as_ref(),
    );
    let path = parts.uri.path().to_string();
    let localized = parts.extensions.get::<Localized>().copied();
    let tr = match localized {
        Some(Localized { locale, page }) => wanted.reading(locale).on_page(page, None),
        None => match split_path(&path) {
            Some((locale, _)) => {
                use axum::extract::FromRequestParts;
                let Ok(theme) = crate::auth::Theme::from_request_parts(&mut parts, &state).await;
                let reader = if state.i18n.shipped().contains(&locale) {
                    wanted.reading(locale)
                } else {
                    Tr::english(state.i18n.clone())
                };
                return TR.scope(reader, crate::web::not_found(theme)).await;
            }
            None => match state.i18n.page_at(&path) {
                Some(page) if page.slugs.is_empty() => {
                    Tr::english(state.i18n.clone()).on_page(page, None)
                }
                Some(page) if account.is_none() && hl(parts.uri.query()).is_none() => {
                    Tr::english(state.i18n.clone()).on_page(page, Some(wanted.locale()))
                }
                Some(page) => wanted.on_page(page, None),
                None => wanted,
            },
        },
    };
    parts.extensions.insert(tr.clone());
    let request = axum::extract::Request::from_parts(parts, body);
    let lang = tr.html_lang();
    let mut response = TR.scope(tr, next.run(request)).await;
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if pages && is_html {
        let headers = response.headers_mut();
        headers.insert(
            header::CONTENT_LANGUAGE,
            header::HeaderValue::from_static(lang),
        );
        headers.append(
            header::VARY,
            header::HeaderValue::from_static("Accept-Language, Cookie"),
        );
    }
    response
}
