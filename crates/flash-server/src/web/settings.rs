use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::UserId;
use serde::Deserialize;

use crate::auth::{self, AdminUser, AuthUser, FullUser, Theme};
use crate::ext::Slot;
use crate::flows::account::KeepSession;
use crate::flows::devices;
use crate::i18n::filters;
use crate::service::{now_ms, ServiceError, Services};
use crate::state::AppState;

use super::{act, format_ago, format_day, respond, via_of, view_event, Shell, Via};

struct PasskeyView {
    label: String,
    created: String,
}

/// One row of the Devices card: a browser or the app, signed in.
struct DeviceView {
    kind: &'static str,
    id: i64,
    label: crate::i18n::Words,
    /// "This browser", or when it was last used (and, for a phone, that
    /// it is the app).
    detail: crate::i18n::Words,
    current: bool,
}

impl DeviceView {
    fn new(d: devices::Device, now_ms: i64) -> Self {
        // A browser's name already says it is one ("Chrome on Windows");
        // the app's is the phone's, so it says which it is. A session
        // with no recorded use (one from before uses were kept) says when
        // it signed in, not that it was used then.
        use crate::i18n::{t, ta};
        use devices::DeviceKind::{App, Browser};
        let ago = |id: &str, at| ta(id, &[("ago", format_ago(now_ms, at).into())]);
        let detail = if d.current {
            t("settings-device-this")
        } else {
            match (d.kind, d.last_used_at) {
                (Browser, Some(at)) => ago("settings-device-used", at),
                (Browser, None) => ago("settings-device-signed-in", d.created_at),
                (App, Some(at)) => ago("settings-device-app-used", at),
                (App, None) => ago("settings-device-app-signed-in", d.created_at),
            }
        };
        // Signed in by a link from an email, not by the person typing
        // anything: worth saying, so an unexpected one stands out.
        let detail = if d.via_link {
            ta("settings-device-via-link", &[("detail", detail.into())])
        } else {
            detail
        };
        let label = match (d.kind, d.label.split_once(" on ")) {
            (Browser, Some((browser, os))) => ta(
                "settings-device-browser",
                &[("browser", browser.into()), ("os", os.into())],
            ),
            _ => d.label,
        };
        DeviceView {
            kind: d.kind.as_str(),
            id: d.id,
            label,
            detail,
            current: d.current,
        }
    }
}

/// One row of the Connected apps card: a connector registered through
/// OAuth or the first-party app, with what disconnecting it revokes.
struct AppView {
    client_id: String,
    name: crate::i18n::Words,
    first_party: bool,
    tokens: u32,
    since: String,
    /// Empty when the grant has never been used.
    last_used: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    shell: Shell,
    display_name: String,
    name_saved: bool,
    name_error: bool,
    grading_mode: &'static str,
    new_per_day: u32,
    reviews_per_day: u32,
    /// Review cards due for their whole study day, or at the exact time.
    due_by_day: bool,
    saved: String,
    passkeys: Vec<PasskeyView>,
    /// Every browser and phone signed in, this browser first.
    devices: Vec<DeviceView>,
    apps: Vec<AppView>,
    invite_url: Option<String>,
    card_count: u32,
    has_password: bool,
    has_passkey: bool,
    /// An external login the extension knows about keeps the account
    /// reachable, so the password may be removed.
    has_external_login: bool,
    pw_saved: bool,
    pw_error: bool,
    /// Why the last delete-account attempt was refused (see
    /// password::settings_delete_account): confirm / password / admin.
    /// Empty = no attempt; the extension's own codes render in its badge.
    delete_error: String,
    account_html: String,
    providers_html: String,
    cards_html: String,
    delete_badge_html: String,
    delete_note_html: String,
    languages: Vec<LanguageChoice>,
    language_auto: bool,
}

struct LanguageChoice {
    tag: &'static str,
    name: &'static str,
    selected: bool,
}

struct SettingsFlags {
    saved: String,
    name_saved: bool,
    name_error: bool,
    pw_saved: bool,
    pw_error: bool,
    delete_error: String,
}

/// `query` is the raw query string, handed to the extension's slots for
/// the flags its own redirects carry. `this` is the browser asking.
#[allow(clippy::too_many_arguments)]
fn settings_page_data(
    s: &Services,
    app: &AppState,
    shell: Shell,
    user: UserId,
    this: &KeepSession,
    flags: SettingsFlags,
    invite_url: Option<String>,
    query: &str,
) -> Result<SettingsPage, ServiceError> {
    let now = now_ms();
    let o = s.settings_overview(user, now)?;
    let devices: Vec<DeviceView> = devices::list(s.store(), user, this, now)?
        .into_iter()
        .map(|d| DeviceView::new(d, now))
        .collect();
    let passkeys: Vec<PasskeyView> = o
        .passkeys
        .iter()
        .map(|p| PasskeyView {
            label: if p.label.is_empty() {
                "passkey".into()
            } else {
                p.label.clone()
            },
            created: format_day(p.created_at),
        })
        .collect();
    let has_passkey = !passkeys.is_empty();
    let apps: Vec<AppView> = s
        .connected_apps(user, now_ms())?
        .into_iter()
        .map(|g| {
            let first_party = g.client_id == crate::api::MOBILE_CLIENT_ID;
            AppView {
                name: if first_party {
                    crate::i18n::t("settings-app-flash")
                } else if g.client_name.trim().is_empty() {
                    crate::i18n::t("settings-app-unnamed")
                } else {
                    // theirs: the name the connected app registered.
                    crate::i18n::Words::theirs(g.client_name.clone())
                },
                client_id: g.client_id,
                first_party,
                tokens: g.tokens,
                since: format_day(g.first_granted_at),
                last_used: g.last_used_at.map(format_day).unwrap_or_default(),
            }
        })
        .collect();
    let has_external_login = app.ext.has_external_login(s.store(), user)?;
    let chosen = s
        .store()
        .account_locale(user)?
        .filter(|(_, source)| *source == flash_store::LocaleSource::Chosen)
        .map(|(tag, _)| tag);
    let shipped = app.i18n.shipped();
    let languages = if shipped.len() < 2 {
        Vec::new()
    } else {
        crate::i18n::tr()
            .picker_order(shipped)
            .into_iter()
            .map(|l| LanguageChoice {
                tag: l.tag(),
                name: l.native_name(),
                selected: chosen.as_deref() == Some(l.tag()),
            })
            .collect()
    };
    Ok(SettingsPage {
        shell,
        display_name: o.display_name,
        name_saved: flags.name_saved,
        name_error: flags.name_error,
        grading_mode: o.grading_mode.as_str(),
        new_per_day: o.new_per_day,
        reviews_per_day: o.reviews_per_day,
        due_by_day: o.due_by_day,
        saved: flags.saved,
        passkeys,
        devices,
        apps,
        invite_url,
        card_count: o.card_count,
        has_password: o.has_password,
        has_passkey,
        has_external_login,
        pw_saved: flags.pw_saved,
        pw_error: flags.pw_error,
        delete_error: flags.delete_error,
        account_html: app.ext.render(app, Slot::SettingsAccount { user, query })?,
        providers_html: app
            .ext
            .render(app, Slot::SettingsProviders { user, query })?,
        cards_html: app.ext.render(app, Slot::SettingsCards { user, query })?,
        delete_badge_html: app.ext.render(app, Slot::SettingsDeleteBadge { query })?,
        delete_note_html: app.ext.render(app, Slot::SettingsDeleteNote { user })?,
        language_auto: chosen.is_none(),
        languages,
    })
}

#[derive(Deserialize)]
pub(super) struct SavedQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    saved: Option<String>,
    name: Option<u8>,
    nameerr: Option<u8>,
    pw: Option<u8>,
    pwerr: Option<u8>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    delerr: Option<String>,
}

pub(super) async fn settings_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Query(query): Query<SavedQuery>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
) -> Response {
    let flags = SettingsFlags {
        saved: query.saved.unwrap_or_default(),
        name_saved: query.name == Some(1),
        name_error: query.nameerr == Some(1),
        pw_saved: query.pw == Some(1),
        pw_error: query.pwerr == Some(1),
        delete_error: query
            .delerr
            .filter(|e| matches!(e.as_str(), "confirm" | "password" | "admin"))
            .unwrap_or_default(),
    };
    let shell = Shell::new(&state, theme, "settings", is_admin);
    view_event(
        &state,
        user,
        "settings",
        Via(raw_query.as_deref().and_then(|q| via_of(&state, q))),
    );
    let this = this_browser(&headers);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        settings_page_data(
            &s,
            &app,
            shell,
            user,
            &this,
            flags,
            None,
            raw_query.as_deref().unwrap_or_default(),
        )
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct DisconnectForm {
    #[serde(deserialize_with = "crate::bounds::token")]
    client_id: String,
}

/// The browser making a request, for the Devices card.
fn this_browser(headers: &HeaderMap) -> KeepSession {
    KeepSession {
        web_session_hash: auth::session_hash(headers),
        api_token_id: None,
    }
}

#[derive(Deserialize)]
pub(super) struct DeviceForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    kind: String,
    id: i64,
}

/// Signs one browser or phone out; the card re-renders without it.
pub(super) async fn sign_out_device(
    State(state): State<AppState>,
    FullUser { id: user, .. }: FullUser,
    Form(form): Form<DeviceForm>,
) -> Response {
    let Some(kind) = devices::DeviceKind::parse(&form.kind) else {
        return (StatusCode::NOT_FOUND, "not found: session").into_response();
    };
    match act(state.services.clone(), move |s| {
        Ok(devices::sign_out(s.store(), user, kind, form.id, now_ms())?)
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?saved=devices").into_response(),
        Err(response) => response,
    }
}

/// Signs out every browser and phone but this browser.
pub(super) async fn sign_out_other_devices(
    State(state): State<AppState>,
    FullUser { id: user, .. }: FullUser,
    headers: HeaderMap,
) -> Response {
    let this = this_browser(&headers);
    match act(state.services.clone(), move |s| {
        Ok(devices::sign_out_others(s.store(), user, &this, now_ms())?)
    })
    .await
    {
        Ok(_) => Redirect::to("/settings?saved=devices").into_response(),
        Err(response) => response,
    }
}

/// Revokes every live grant of one connected app or connector; the card
/// re-renders without it.
pub(super) async fn disconnect_app(
    State(state): State<AppState>,
    FullUser { id: user, .. }: FullUser,
    Form(form): Form<DisconnectForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.disconnect_app(user, &form.client_id, now_ms())
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?saved=apps").into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct NameForm {
    #[serde(deserialize_with = "crate::bounds::display_name")]
    display_name: String,
}

/// Same bounds as signup's name field.
pub(super) async fn save_name(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<NameForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.set_display_name(user, &form.display_name)
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?name=1").into_response(),
        Err(response) if response.status() == StatusCode::BAD_REQUEST => {
            Redirect::to("/settings?nameerr=1").into_response()
        }
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct AppearanceForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    theme: String,
}

/// The theme lives in a cookie, not the account, but only a signed-in
/// user has the form: a stranger gets no route that sets cookies.
pub(super) async fn save_appearance(_user: AuthUser, Form(form): Form<AppearanceForm>) -> Response {
    let Some(theme) = Theme::from_str(&form.theme) else {
        return (StatusCode::BAD_REQUEST, "unknown theme").into_response();
    };
    let mut response = Redirect::to("/settings").into_response();
    if let Ok(value) =
        auth::cookie_header(auth::THEME_COOKIE, theme.as_str(), 365 * 24 * 60 * 60).parse()
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[derive(Deserialize)]
pub(super) struct LanguageForm {
    /// A shipped language's tag, or empty for "Automatic".
    #[serde(default, deserialize_with = "crate::bounds::opt_locale")]
    language: Option<String>,
}

fn picked_language(
    state: &AppState,
    value: Option<&str>,
) -> Result<Option<&'static str>, Response> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    state
        .i18n
        .shipped()
        .iter()
        .map(|l| l.tag())
        .find(|tag| tag.eq_ignore_ascii_case(value))
        .map(Some)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "unknown language").into_response())
}

/// Settings' language picker: the account's choice, kept with it, or
/// back to the language its browser, assistant or phone says.
pub(super) async fn save_language(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<LanguageForm>,
) -> Response {
    let picked = match picked_language(&state, form.language.as_deref()) {
        Ok(p) => p,
        Err(response) => return response,
    };
    match act(state.services.clone(), move |s| {
        match picked {
            Some(tag) => s.store().set_chosen_locale(user, tag)?,
            None => s.store().clear_chosen_locale(user)?,
        }
        Ok(())
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?saved=language").into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct GradingForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    mode: String,
}

pub(super) async fn save_grading(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<GradingForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.set_grading_mode(user, &form.mode)
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?saved=grading").into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct SchedulingForm {
    new_per_day: u32,
    reviews_per_day: u32,
    /// "day" or "exact"; a form without it leaves the choice alone.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    due: Option<String>,
}

pub(super) async fn save_scheduling(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<SchedulingForm>,
) -> Response {
    let due_by_day = match form.due.as_deref() {
        None => None,
        Some("day") => Some(true),
        Some("exact") => Some(false),
        Some(_) => return (StatusCode::BAD_REQUEST, "unknown due choice").into_response(),
    };
    match act(state.services.clone(), move |s| {
        s.set_daily_limits(user, form.new_per_day, form.reviews_per_day)?;
        s.set_study_settings(
            user,
            &crate::service::StudySettingsPatch {
                due_by_day,
                ..Default::default()
            },
        )
    })
    .await
    {
        Ok(()) => Redirect::to("/settings?saved=limits").into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct InviteForm {
    #[serde(deserialize_with = "crate::bounds::display_name")]
    display_name: String,
}

pub(super) async fn create_invite(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    theme: Theme,
    headers: HeaderMap,
    Form(form): Form<InviteForm>,
) -> Response {
    let this = this_browser(&headers);
    let base_url = state.config.base_url.clone();
    let shell = Shell::new(&state, theme, "settings", true);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let (token, _) = s.create_member_invite(user, &form.display_name, now_ms())?;
        let url = format!("{base_url}/enroll/{token}");
        let flags = SettingsFlags {
            saved: String::new(),
            name_saved: false,
            name_error: false,
            pw_saved: false,
            pw_error: false,
            delete_error: String::new(),
        };
        settings_page_data(&s, &app, shell, user, &this, flags, Some(url), "")
    })
    .await
}
