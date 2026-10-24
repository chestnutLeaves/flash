use askama::Template;
use axum::extract::{Path, Query, RawQuery, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::DeckId;
use serde::Deserialize;

use crate::auth::{self, AuthUser, Theme};
use crate::ext::{AddCardsAt, Slot};
use crate::i18n::filters;
use crate::service::{now_ms, ServiceError, Services};
use crate::state::AppState;

use super::cards::split_tags;
use super::{
    account_event, act, deck_list_rows, err_response, respond, rows_destination, view_event,
    DeckFamily, DeckListRow, DeckRowsCtx, Shell, Via,
};

#[derive(Template)]
#[template(path = "decks.html")]
struct DecksPage {
    shell: Shell,
    /// The decks to list, in one order. An archived deck keeps its place
    /// among the others and is left out unless this browser asked to see
    /// them (`auth::ShowArchived`, set by the button in the page head,
    /// which is offered while `any_archived`).
    decks: Vec<DeckListRow>,
    rows: DeckRowsCtx,
    any_archived: bool,
    /// Whether any deck is not archived: when none is and archived decks
    /// are hidden, the page says so in place of an empty table.
    any_active: bool,
    show_archived: bool,
    add_cards: String,
}

#[derive(Deserialize)]
pub(super) struct ShowArchivedForm {
    /// "1" to list archived decks, anything else to leave them out.
    #[serde(default, deserialize_with = "crate::bounds::keyword")]
    show: String,
}

/// "Show archived" and "Hide archived" on the deck list. The choice is
/// how this browser likes the page, so it goes in a cookie beside the
/// theme: a refresh keeps it, and it stays until it is turned off.
pub(super) async fn set_show_archived(
    _user: AuthUser,
    Form(form): Form<ShowArchivedForm>,
) -> Response {
    let mut response = Redirect::to("/decks").into_response();
    let cookie = auth::pref_cookie_header(
        auth::SHOW_ARCHIVED_COOKIE,
        (form.show == "1").then_some("1"),
    );
    if let Ok(value) = cookie.parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[derive(Deserialize)]
pub(super) struct FoldForm {
    /// "1" to fold the deck shut, anything else to open it.
    #[serde(default, deserialize_with = "crate::bounds::keyword")]
    collapsed: String,
    /// Where the row was (`DeckRowsCtx`), to come back to.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    back: Option<String>,
    #[serde(default)]
    from: Option<i64>,
}

/// The fold beside a deck that has subdecks. Every deck starts open; one
/// that is folded shut stays shut in this browser until it is opened
/// again, kept the way "Show archived" is. The ids kept are held to the
/// decks the caller has, so a deleted deck's fold is forgotten and a
/// stranger's id is never stored.
pub(super) async fn fold_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    collapsed: auth::CollapsedDecks,
    Form(form): Form<FoldForm>,
) -> Response {
    let existing = match act(state.services.clone(), move |s| {
        Ok(s.list_decks(user, now_ms())?
            .into_iter()
            .map(|d| d.id.0)
            .collect::<Vec<i64>>())
    })
    .await
    {
        Ok(ids) => ids,
        Err(response) => return response,
    };
    let folded = collapsed.with(id, form.collapsed == "1", &existing);
    let mut response =
        Redirect::to(&rows_destination(form.back.as_deref(), form.from)).into_response();
    if let Ok(value) = folded.cookie().parse() {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

pub(super) async fn decks_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    auth::ShowArchived(show_archived): auth::ShowArchived,
    collapsed: auth::CollapsedDecks,
    via: Via,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    view_event(&state, user, "decks", via);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let add_cards = app
            .ext
            .render(&app, Slot::AddCards(AddCardsAt::DecksLinks))?;
        let decks = s.list_decks(user, now_ms())?;
        let any_archived = decks.iter().any(|d| d.archived_at.is_some());
        let any_active = decks.iter().any(|d| d.archived_at.is_none());
        let decks = deck_list_rows(decks, &collapsed, show_archived);
        Ok(DecksPage {
            shell,
            rows: DeckRowsCtx::decks(&decks),
            decks,
            any_archived,
            any_active,
            show_archived,
            add_cards,
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct NewDeckForm {
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    name: String,
}

pub(super) async fn create_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<NewDeckForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.create_deck(user, &form.name, "", now_ms())
    })
    .await
    {
        Ok(deck) => {
            account_event(&state, user, "deck_created", None, None);
            Redirect::to(&format!("/decks/{deck}")).into_response()
        }
        Err(response) => response,
    }
}

/// Deck-page view of daily limits: the deck's overrides (None = inherit)
/// beside the account defaults they'd inherit, plus today's boost.
struct DeckLimitsView {
    new_override: Option<u32>,
    reviews_override: Option<u32>,
    default_new: u32,
    default_reviews: u32,
    boost_today: u32,
}

#[derive(Template)]
#[template(path = "deck_settings.html")]
struct DeckSettingsPage {
    shell: Shell,
    deck_id: i64,
    /// The whole path, for the page title and under the heading.
    deck_name: String,
    /// The deck's own name: what the rename field edits.
    deck_leaf: String,
    /// Where the deck could sit: every deck that is not under it. The one
    /// it sits in now is `selected`; none selected is the top level.
    parents: Vec<ParentOption>,
    /// Decks under it and live cards in the lot: what archiving takes
    /// along and what deleting removes.
    subdeck_count: u32,
    card_count: u32,
    limits: DeckLimitsView,
    goal: Option<super::goal::GoalFormView>,
    saved: String,
    /// Why the last rename or move was refused, if it was.
    name_error: Option<crate::i18n::Words>,
    parent_error: Option<crate::i18n::Words>,
    card_html: String,
    end_html: String,
    /// Whether the deck is archived, which flips the archive card.
    archived: bool,
}

#[derive(Deserialize)]
pub(super) struct DeckSettingsQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    saved: Option<String>,
    /// Why a rename was refused, and why a move was: each shows on its
    /// own card.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    err: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    moverr: Option<String>,
}

/// One entry of the "Parent deck" select, named by its whole path.
struct ParentOption {
    id: i64,
    name: String,
    selected: bool,
}

/// Short query-string code for a refused rename or move, so the settings
/// page can say why after the redirect.
fn deck_path_error_code(err: &ServiceError) -> Option<&'static str> {
    if let ServiceError::Refused { code, .. } = err {
        return Some(code);
    }
    let m: &str = match err {
        ServiceError::Invalid(m) => m,
        ServiceError::Store(flash_store::StoreError::Invalid(m)) => m,
        _ => return None,
    };
    Some(crate::service::deck_refusal_code(m).unwrap_or("invalid"))
}

pub(super) async fn deck_settings_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    Query(query): Query<DeckSettingsQuery>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    let saved = query
        .saved
        .filter(|v| {
            matches!(
                v.as_str(),
                "name" | "parent" | "limits" | "goal" | "archive"
            )
        })
        .unwrap_or_default();
    let name_error = query
        .err
        .as_deref()
        .and_then(crate::service::deck_path_error_copy);
    let parent_error = query
        .moverr
        .as_deref()
        .and_then(crate::service::deck_path_error_copy);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let deck = DeckId(id);
        let d = s.deck_detail(user, deck, now_ms())?;
        let family = DeckFamily::of(s.list_decks(user, now_ms())?, deck)?;
        let archived = d.deck.archived_at.is_some();
        // An active deck is never put inside an archived one, so an
        // archived deck is offered only to a deck that is archived too.
        let parents = family
            .others
            .into_iter()
            .filter(|p| archived || p.archived_at.is_none())
            .map(|p| ParentOption {
                id: p.id.0,
                selected: Some(p.id) == d.deck.parent_id,
                name: p.path,
            })
            .collect();
        let goal = if archived {
            None
        } else {
            Some(super::goal::GoalFormView::build(
                &s, user, deck, "", 0, false,
            )?)
        };
        Ok(DeckSettingsPage {
            shell,
            deck_id: id,
            archived,
            goal,
            deck_leaf: d.deck.leaf,
            parents,
            subdeck_count: d.subdeck_count,
            card_count: d.card_count,
            parent_error,
            deck_name: d.deck.path,
            limits: DeckLimitsView {
                new_override: d.limits.new_per_day,
                reviews_override: d.limits.reviews_per_day,
                default_new: d.default_new,
                default_reviews: d.default_reviews,
                boost_today: d.boost_today,
            },
            card_html: app
                .ext
                .render(&app, Slot::DeckSettingsCard { user, deck })?,
            saved,
            name_error,
            end_html: app.ext.render(
                &app,
                Slot::DeckSettingsEnd {
                    user,
                    deck,
                    query: raw_query.as_deref().unwrap_or_default(),
                },
            )?,
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RenameDeckForm {
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    name: String,
}

pub(super) async fn rename_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<RenameDeckForm>,
) -> Response {
    if form.name.trim().is_empty() {
        return Redirect::to(&format!("/decks/{id}/settings?err=empty")).into_response();
    }
    // The field holds the deck's own name; where the deck sits is the
    // Parent deck card's business.
    deck_path_change(&state, id, "name", "err", move |s| {
        s.rename_deck_leaf(user, DeckId(id), &form.name)
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct MoveDeckForm {
    /// The deck to move into; blank for the top level.
    #[serde(default, deserialize_with = "crate::bounds::keyword")]
    parent: String,
}

pub(super) async fn move_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<MoveDeckForm>,
) -> Response {
    let parent = match form.parent.trim() {
        "" => None,
        raw => match raw.parse::<i64>() {
            Ok(parent) => Some(DeckId(parent)),
            Err(_) => return err_response(ServiceError::Malformed("no such deck".into())),
        },
    };
    deck_path_change(&state, id, "parent", "moverr", move |s| {
        s.move_deck(user, DeckId(id), parent)
    })
    .await
}

/// Runs a rename or a move and returns to the settings page: `saved` names
/// the card that shows "Saved", and a refusal the page has words for goes
/// back as a code under `err_key`, to show on that card.
async fn deck_path_change(
    state: &AppState,
    id: i64,
    saved: &'static str,
    err_key: &'static str,
    change: impl FnOnce(Services) -> Result<(), ServiceError> + Send + 'static,
) -> Response {
    let services = state.services.clone();
    match crate::i18n::blocking(move || change(services)).await {
        Ok(Ok(())) => Redirect::to(&format!("/decks/{id}/settings?saved={saved}")).into_response(),
        Ok(Err(err)) => match deck_path_error_code(&err) {
            Some(code) => {
                Redirect::to(&format!("/decks/{id}/settings?{err_key}={code}")).into_response()
            }
            None => err_response(err),
        },
        Err(e) => {
            tracing::error!("join: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::i18n::t("error-internal"),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub(super) struct SubdeckForm {
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    name: String,
}

/// "Add subdeck" on a deck's page: makes the deck inside it and opens it.
pub(super) async fn create_subdeck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<SubdeckForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.create_subdeck(user, DeckId(id), &form.name, "", now_ms())
    })
    .await
    {
        Ok(deck) => {
            account_event(&state, user, "deck_created", None, None);
            Redirect::to(&format!("/decks/{deck}")).into_response()
        }
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct DeckLimitsForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    new_per_day: String,
    #[serde(deserialize_with = "crate::bounds::keyword")]
    reviews_per_day: String,
}

/// Blank = inherit (None); otherwise a non-negative integer.
fn parse_limit(raw: &str) -> Option<Option<u32>> {
    let t = raw.trim();
    if t.is_empty() {
        Some(None)
    } else {
        t.parse::<u32>().ok().map(Some)
    }
}

pub(super) async fn save_deck_limits(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<DeckLimitsForm>,
) -> Response {
    let (Some(new), Some(reviews)) = (
        parse_limit(&form.new_per_day),
        parse_limit(&form.reviews_per_day),
    ) else {
        return (StatusCode::BAD_REQUEST, "limits must be whole numbers").into_response();
    };
    match act(state.services.clone(), move |s| {
        s.set_deck_limits(user, DeckId(id), new, reviews)
    })
    .await
    {
        Ok(()) => Redirect::to(&format!("/decks/{id}/settings?saved=limits")).into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct BoostForm {
    extra: u32,
    /// `study` sends the user back into the study queue they came from.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    back: Option<String>,
    /// The tags that queue was filtered by, so the boost returns to it.
    #[serde(default, deserialize_with = "crate::bounds::opt_scope_tag_text")]
    tags: Option<String>,
}

pub(super) async fn boost_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<BoostForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.boost_new_today(user, Some(DeckId(id)), form.extra, now_ms())
    })
    .await
    {
        Ok(_) if form.back.as_deref() == Some("study") => {
            let tags = form.tags.as_deref().map(split_tags).unwrap_or_default();
            Redirect::to(&super::study::study_url(Some(id), &tags)).into_response()
        }
        Ok(_) => Redirect::to(&format!("/decks/{id}/settings")).into_response(),
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct ArchiveForm {
    /// Where the button lived: `decks` returns to the list (which shows
    /// its archived rows or not as the browser last asked), `deck` to the
    /// deck page; the settings page otherwise, with its saved note.
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    back: Option<String>,
    /// With `deck`: the deck whose page the button was on, when that is
    /// not the deck acted on (a subdeck's row on its parent's page). It
    /// only names a page to go back to; a page that is not the caller's
    /// is not found when it gets there.
    #[serde(default)]
    from: Option<i64>,
}

impl ArchiveForm {
    fn destination(&self, id: i64) -> String {
        match self.back.as_deref() {
            Some("today") => "/".to_string(),
            Some("decks") => "/decks".to_string(),
            Some("deck") => format!("/decks/{}", self.from.unwrap_or(id)),
            _ => format!("/decks/{id}/settings?saved=archive"),
        }
    }
}

pub(super) async fn archive_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<ArchiveForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.archive_deck(user, DeckId(id), now_ms())
    })
    .await
    {
        Ok(()) => Redirect::to(&form.destination(id)).into_response(),
        Err(response) => response,
    }
}

pub(super) async fn unarchive_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<ArchiveForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.unarchive_deck(user, DeckId(id))
    })
    .await
    {
        Ok(()) => Redirect::to(&form.destination(id)).into_response(),
        Err(response) => response,
    }
}

/// Hard-deletes a deck and all its cards. The button is htmx-confirmed;
/// success answers 200 with HX-Redirect so htmx does a full-page hop back
/// to the deck list.
pub(super) async fn delete_deck(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.delete_deck(user, DeckId(id))
    })
    .await
    {
        Ok(deleted) => {
            if deleted.cards > 0 {
                account_event(
                    &state,
                    user,
                    "cards_deleted",
                    Some("deck"),
                    Some(deleted.cards as i64),
                );
            }
            // The deletion owes its orphaned blobs to the job worker.
            crate::jobs::wake(&state);
            let mut response = StatusCode::OK.into_response();
            response.headers_mut().insert(
                axum::http::HeaderName::from_static("hx-redirect"),
                axum::http::HeaderValue::from_static("/decks"),
            );
            response
        }
        Err(response) => response,
    }
}

pub(super) async fn boost_all(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Form(form): Form<BoostForm>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.boost_new_today(user, None, form.extra, now_ms())
    })
    .await
    {
        Ok(_) if form.back.as_deref() == Some("study") => {
            let tags = form.tags.as_deref().map(split_tags).unwrap_or_default();
            Redirect::to(&super::study::study_url(None, &tags)).into_response()
        }
        Ok(_) => Redirect::to("/").into_response(),
        Err(response) => response,
    }
}
