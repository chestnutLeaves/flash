use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use flash_core::{CardId, DeckId, RevisionId, UserId};
use flash_store::notes::NoteType;
use flash_store::RevisionSource;
use serde::Deserialize;

use crate::auth::{ApiUser, AuthUser, Theme};
use crate::i18n::filters;
use crate::service::{now_ms, text_preview, EditorSeed, NoteInput, ServiceError, Services};
use crate::state::AppState;

use super::{account_event, act, format_ago, respond, view_event, CardView, Shell, Via};

/// The advanced editor's prefill (empty for the deck page's add form).
/// `card_id` only ever reaches the markup as an element-id suffix, so a
/// page showing several editors seeds each with its own id and the same
/// body renders.
pub struct EditorView {
    pub card_id: i64,
    pub deck_id: i64,
    pub note_type: &'static str,
    pub front_html: String,
    pub back_html: String,
    pub tags: String,
    pub sibling_count: usize,
    pub types: Vec<(&'static str, crate::i18n::Words)>,
    pub hues: Vec<(&'static str, crate::i18n::Words)>,
}

impl EditorView {
    pub(crate) fn empty() -> Self {
        EditorView {
            card_id: 0,
            deck_id: 0,
            note_type: NoteType::Basic.as_str(),
            front_html: String::new(),
            back_html: String::new(),
            tags: String::new(),
            sibling_count: 0,
            types: NoteType::ALL
                .iter()
                .map(|t| (t.as_str(), note_type_label(*t)))
                .collect(),
            hues: flash_store::richtext::HIGHLIGHT_HUES
                .iter()
                .map(|h| (*h, crate::i18n::t(&format!("hue-{h}"))))
                .collect(),
        }
    }

    /// The editor over fields that are not a note's yet (no siblings to
    /// warn about); `id` stands in for the card's in the markup.
    pub fn prefilled(
        id: i64,
        note_type: NoteType,
        front_html: &str,
        back_html: &str,
        tags: &[String],
    ) -> Self {
        EditorView {
            card_id: id,
            note_type: note_type.as_str(),
            front_html: front_html.to_string(),
            back_html: back_html.to_string(),
            tags: tags.join(", "),
            ..EditorView::empty()
        }
    }

    fn from_seed(seed: EditorSeed) -> Self {
        EditorView {
            card_id: seed.card_id.0,
            deck_id: seed.deck_id.0,
            note_type: seed.note_type.as_str(),
            front_html: seed.front_html,
            back_html: seed.back_html,
            tags: seed.tags.join(", "),
            sibling_count: seed.sibling_count,
            ..EditorView::empty()
        }
    }
}

#[derive(Deserialize)]
pub struct NoteForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    note_type: String,
    #[serde(default, deserialize_with = "crate::bounds::field_html")]
    front_html: String,
    #[serde(default, deserialize_with = "crate::bounds::field_html")]
    back_html: String,
    #[serde(default, deserialize_with = "crate::bounds::tag_list_text")]
    tags: String,
}

impl NoteForm {
    pub fn into_input(self) -> Result<NoteInput, ServiceError> {
        let note_type = NoteType::parse(&self.note_type)
            .ok_or_else(|| ServiceError::Malformed("unknown note type".into()))?;
        Ok(NoteInput {
            note_type,
            front_html: self.front_html,
            back_html: self.back_html,
            tags: split_tags(&self.tags),
            speech: Default::default(),
            langs: None,
        })
    }
}

pub(super) fn split_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Short query-string code for an editor rejection, so the deck page can
/// show a friendly banner after the redirect.
fn editor_error_code(err: &ServiceError) -> &'static str {
    match err {
        ServiceError::Refused { code, .. } => code,
        _ => "invalid",
    }
}

pub(super) fn editor_error_copy(code: &str) -> Option<crate::i18n::Words> {
    let id = match code {
        "nocloze" => "card-not-added-cloze",
        "empty" => "card-not-added-empty",
        "long" => "card-not-added-long",
        "media" => "card-not-added-media",
        "invalid" => "card-not-added-invalid",
        // Only a downstream card policy can refuse for size; the open
        // server never does.
        "limit" => "card-not-added-limit",
        _ => return None,
    };
    Some(crate::i18n::t(id))
}

/// The advanced editor's create path: a full-page form post, like the
/// quick add, so the refusal banners ride the redirect.
pub(super) async fn create_note(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<NoteForm>,
) -> Response {
    let result = act(state.services.clone(), move |s| {
        let input = form.into_input()?;
        match s.create_note(user, DeckId(id), &input, now_ms()) {
            Ok(_) => Ok(None),
            Err(ServiceError::OverCap { .. }) => Ok(Some("err=limit")),
            Err(
                err @ (ServiceError::Invalid(_)
                | ServiceError::Malformed(_)
                | ServiceError::Refused { .. }),
            ) => Ok(Some(match editor_error_code(&err) {
                "nocloze" => "err=nocloze",
                "empty" => "err=empty",
                "long" => "err=long",
                "media" => "err=media",
                _ => "err=invalid",
            })),
            Err(other) => Err(other),
        }
    })
    .await;
    match result {
        Ok(None) => {
            account_event(&state, user, "note_created", None, None);
            Redirect::to(&format!("/decks/{id}")).into_response()
        }
        Ok(Some(query)) => Redirect::to(&format!("/decks/{id}?{query}")).into_response(),
        Err(response) => response,
    }
}

#[derive(Template)]
#[template(path = "card_editor.html")]
struct CardEditorDialog {
    editor: EditorView,
    error: Option<crate::i18n::Words>,
    /// A restore that went through, said once above the fields.
    notice: Option<crate::i18n::Words>,
    /// The note's earlier versions, newest first.
    history: Vec<RevisionView>,
    after: Option<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    Study,
    Page,
}

impl After {
    fn as_str(self) -> &'static str {
        match self {
            After::Study => "study",
            After::Page => "page",
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AfterQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    after: Option<String>,
}

impl AfterQuery {
    fn after(&self) -> Option<After> {
        match self.after.as_deref() {
            Some("study") => Some(After::Study),
            Some("page") => Some(After::Page),
            _ => None,
        }
    }
}

/// One earlier version in the edit dialog's History.
struct RevisionView {
    id: i64,
    /// When it was replaced, and by what.
    when: crate::i18n::Words,
    front: String,
    back: String,
}

impl RevisionView {
    fn list(s: &Services, user: UserId, card: CardId) -> Result<Vec<Self>, ServiceError> {
        let now = now_ms();
        Ok(s.card_revisions(user, card)?
            .into_iter()
            .map(|r| RevisionView {
                id: r.id.0,
                when: crate::i18n::ta(
                    "editor-replaced",
                    &[
                        ("ago", format_ago(now, r.at).into()),
                        ("by", crate::i18n::t(revision_by(r.source)).into()),
                    ],
                ),
                front: text_preview(&r.front_html, 80),
                back: text_preview(&r.back_html, 80),
            })
            .collect())
    }
}

/// Who made the edit that replaced a version, as History says it.
fn revision_by(source: RevisionSource) -> &'static str {
    match source {
        RevisionSource::Web => "editor-by-web",
        RevisionSource::Mcp => "editor-by-mcp",
        RevisionSource::Api => "editor-by-app",
        RevisionSource::Sync => "editor-by-sync",
        RevisionSource::Restore => "editor-by-restore",
    }
}

pub fn note_type_label(kind: NoteType) -> crate::i18n::Words {
    crate::i18n::t(match kind {
        NoteType::Basic => "note-type-basic",
        NoteType::BasicReversed => "note-type-reversed",
        NoteType::BasicTyped => "note-type-typed",
        NoteType::Cloze => "note-type-cloze",
    })
}

/// The edit dialog, prefilled from the card's note (htmx partial).
pub(super) async fn card_editor(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Query(query): Query<AfterQuery>,
) -> Response {
    let after = query.after().map(After::as_str);
    respond(state.services.clone(), move |s| {
        Ok(CardEditorDialog {
            editor: EditorView::from_seed(s.editor_seed(user, CardId(id))?),
            error: None,
            notice: None,
            history: RevisionView::list(&s, user, CardId(id))?,
            after,
        })
    })
    .await
}

/// Puts an earlier version from History back and re-renders the dialog
/// over it: the fields hold the restored text, and History now lists the
/// text it replaced. A refusal (it would drop cards, or add past the
/// cap) re-renders the dialog unchanged with the reason.
pub(super) async fn restore_card(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path((id, revision)): Path<(i64, i64)>,
    Query(query): Query<AfterQuery>,
) -> Response {
    let after = query.after().map(After::as_str);
    let result = act(state.services.clone(), move |s| {
        let card = CardId(id);
        let restored = s.restore_card_revision(user, card, RevisionId(revision), now_ms());
        let (notice, error) = match restored {
            Ok(_) => (Some(crate::i18n::t("editor-restored")), None),
            Err(
                err @ (ServiceError::Invalid(_)
                | ServiceError::Malformed(_)
                | ServiceError::Refused { .. }
                | ServiceError::OverCap { .. }),
            ) => (None, Some(err.words())),
            Err(other) => return Err(other),
        };
        Ok(CardEditorDialog {
            editor: EditorView::from_seed(s.editor_seed(user, card)?),
            error,
            notice,
            history: RevisionView::list(&s, user, card)?,
            after,
        })
    })
    .await;
    match result {
        Ok(dialog) => {
            if dialog.notice.is_some() {
                account_event(&state, user, "card_edited", Some("restore"), None);
            }
            match crate::i18n::render_current(&dialog) {
                Ok(html) => Html(html).into_response(),
                Err(e) => {
                    tracing::error!("template render: {e}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        crate::i18n::t("error-render"),
                    )
                        .into_response()
                }
            }
        }
        Err(response) => response,
    }
}

/// Saves the edit dialog. Success asks htmx to reload the page (rows,
/// counts and pager all change); from the study page it says the card
/// was saved so the page shows it again in place, and on the card's own
/// page the editor comes back saying "Saved". A refusal re-renders the
/// dialog with the reason so nothing typed is lost.
pub(super) async fn save_card(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Query(query): Query<AfterQuery>,
    Form(form): Form<NoteForm>,
) -> Response {
    let after = query.after();
    let result = act(state.services.clone(), move |s| {
        let input = form.into_input()?;
        let saved = s.save_card_editor(user, CardId(id), &input, RevisionSource::Web, now_ms());
        match saved {
            Ok(_) if after == Some(After::Page) => Ok((
                true,
                Some(CardEditorDialog {
                    editor: EditorView::from_seed(s.editor_seed(user, CardId(id))?),
                    error: None,
                    notice: Some(crate::i18n::t("editor-saved")),
                    history: RevisionView::list(&s, user, CardId(id))?,
                    after: Some(After::Page.as_str()),
                }),
            )),
            Ok(_) => Ok((true, None)),
            Err(
                err @ (ServiceError::Invalid(_)
                | ServiceError::Malformed(_)
                | ServiceError::Refused { .. }
                | ServiceError::OverCap { .. }),
            ) => {
                let seed = s.editor_seed(user, CardId(id))?;
                let history = RevisionView::list(&s, user, CardId(id))?;
                let mut editor = EditorView::from_seed(seed);
                // Echo what was typed, not what is stored.
                editor.note_type = input.note_type.as_str();
                editor.front_html =
                    flash_store::richtext::sanitize_with_media(&input.front_html).into_string();
                editor.back_html =
                    flash_store::richtext::sanitize_with_media(&input.back_html).into_string();
                editor.tags = input.tags.join(", ");
                Ok((
                    false,
                    Some(CardEditorDialog {
                        editor,
                        error: Some(err.words()),
                        notice: None,
                        history,
                        after: after.map(After::as_str),
                    }),
                ))
            }
            Err(other) => Err(other),
        }
    })
    .await;
    if let Ok((true, _)) = result {
        account_event(&state, user, "card_edited", Some("editor"), None);
    }
    match result {
        Ok((_, None)) => match after {
            // htmx fires the event on a 204 too, and swaps nothing.
            Some(After::Study) => {
                (StatusCode::NO_CONTENT, [("HX-Trigger", "flash:card-saved")]).into_response()
            }
            _ => (StatusCode::NO_CONTENT, [("HX-Refresh", "true")]).into_response(),
        },
        Ok((_, Some(dialog))) => match crate::i18n::render_current(&dialog) {
            Ok(html) => Html(html).into_response(),
            Err(e) => {
                tracing::error!("template render: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    crate::i18n::t("error-render"),
                )
                    .into_response()
            }
        },
        Err(response) => response,
    }
}

#[derive(Deserialize)]
pub(super) struct NewCardForm {
    #[serde(deserialize_with = "crate::bounds::card_side")]
    front: String,
    #[serde(deserialize_with = "crate::bounds::card_side")]
    back: String,
    #[serde(default, deserialize_with = "crate::bounds::tag_list_text")]
    tags: String,
}

pub(super) async fn create_card(
    State(state): State<AppState>,
    AuthUser { id: user, .. }: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<NewCardForm>,
) -> Response {
    let result = act(state.services.clone(), move |s| {
        let tags = split_tags(&form.tags);
        match s.create_cards_in_deck(user, DeckId(id), &[(form.front, form.back, tags)], now_ms()) {
            Ok(_) => Ok(true),
            // Land back on the deck page with the inline refusal instead
            // of a bare error page.
            Err(ServiceError::OverCap { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    })
    .await;
    match result {
        Ok(true) => {
            account_event(&state, user, "cards_created", Some("manual"), Some(1));
            Redirect::to(&format!("/decks/{id}")).into_response()
        }
        Ok(false) => Redirect::to(&format!("/decks/{id}?err=limit")).into_response(),
        Err(response) => response,
    }
}

#[derive(Template)]
#[template(path = "card_page.html")]
struct CardPage {
    shell: Shell,
    deck_id: i64,
    deck_name: String,
    editor: EditorView,
    error: Option<String>,
    notice: Option<String>,
    history: Vec<RevisionView>,
    after: Option<&'static str>,
}

pub(super) async fn card_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    via: Via,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    view_event(&state, user, "card", via);
    respond(state.services.clone(), move |s| {
        let seed = s.editor_seed(user, CardId(id))?;
        let deck = s.deck_by_id(user, seed.deck_id, now_ms())?;
        Ok(CardPage {
            shell,
            deck_id: seed.deck_id.0,
            deck_name: deck.path,
            history: RevisionView::list(&s, user, CardId(id))?,
            editor: EditorView::from_seed(seed),
            error: None,
            notice: None,
            after: Some(After::Page.as_str()),
        })
    })
    .await
}

pub(super) async fn delete_card(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    match act(state.services.clone(), move |s| {
        s.delete_card(user, CardId(id), now_ms())
    })
    .await
    {
        Ok(()) => {
            account_event(&state, user, "cards_deleted", None, Some(1));
            StatusCode::OK.into_response()
        }
        Err(response) => response,
    }
}

#[derive(Template)]
#[template(source = r#"{% include "card_row.html" %}"#, ext = "html")]
struct CardRowPartial {
    card: CardView,
}

async fn set_card_archived(state: AppState, user: UserId, id: i64, archived: bool) -> Response {
    let response = respond(state.services.clone(), move |s| {
        let card = CardId(id);
        s.set_card_archived(user, card, archived, now_ms())?;
        Ok(CardRowPartial {
            card: CardView::from_row(&s, user, s.card(user, card)?)?,
        })
    })
    .await;
    if response.status().is_success() {
        let event = if archived {
            "cards_archived"
        } else {
            "cards_unarchived"
        };
        account_event(&state, user, event, None, Some(1));
    }
    response
}

pub(super) async fn archive_card(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    set_card_archived(state, user, id, true).await
}

pub(super) async fn unarchive_card(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> Response {
    set_card_archived(state, user, id, false).await
}
