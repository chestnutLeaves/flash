//! The filter lives in the page's address (`?status=archived&tags=a,b`),
//! so a reload, Back, or an editor save that reloads the page keeps it.

use std::collections::HashMap;

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::HeaderValue;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use flash_core::{deck_path, CardId, DeckId, UserId};
use flash_store::{CardStatus, DeckFacet};
use serde::{Deserialize, Serialize};

use super::cards::{editor_error_copy, split_tags, EditorView};
use super::{
    account_event, act, respond, view_event, CardView, Crumb, DeckFamily, Shell, Via,
    CARDS_PER_PAGE,
};
use crate::auth::{ApiUser, AuthUser, Theme};
use crate::ext::{AddCardsAt, Slot};
use crate::i18n::filters;
use crate::service::deck_cards::DeckFilter;
use crate::service::{now_ms, DeckRow, ServiceError, Services};
use crate::state::AppState;

/// The page's address as the filter writes it, and as a reload reads it
/// back. `archived=true` is the address the list used before the filter
/// had parts, kept so its links still land.
#[derive(Deserialize, Default)]
pub(super) struct CardsQuery {
    #[serde(default, deserialize_with = "crate::bounds::opt_search")]
    q: Option<String>,
    page: Option<u32>,
    #[serde(default)]
    archived: bool,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    status: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_tag_list_text")]
    tags: Option<String>,
    #[serde(
        default,
        rename = "match",
        deserialize_with = "crate::bounds::opt_keyword"
    )]
    match_: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_filter")]
    decks: Option<String>,
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    err: Option<String>,
}

impl CardsQuery {
    fn filter(&self) -> DeckFilter {
        let archived = self.archived || self.status.as_deref() == Some("archived");
        DeckFilter {
            decks: self
                .decks
                .as_deref()
                .unwrap_or_default()
                .split(',')
                .filter_map(|d| match d.trim() {
                    "own" => Some(DeckFacet::Own),
                    id => id.parse().ok().map(|id| DeckFacet::Sub(DeckId(id))),
                })
                .collect(),
            search: DeckFilter::search_from(self.q.as_deref()),
            status: if archived {
                CardStatus::Archived
            } else {
                CardStatus::Active
            },
            tags: flash_core::queue::normalize_tags(split_tags(
                self.tags.as_deref().unwrap_or_default(),
            )),
            match_all: self.match_.as_deref() == Some("all"),
        }
    }
}

fn filter_query(f: &DeckFilter) -> String {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(search) = &f.search {
        q.append_pair("q", search);
    }
    if f.status == CardStatus::Archived {
        q.append_pair("status", "archived");
    }
    if !f.tags.is_empty() {
        q.append_pair("tags", &f.tags.join(","));
    }
    if f.match_all && f.tags.len() > 1 {
        q.append_pair("match", "all");
    }
    if !f.decks.is_empty() {
        q.append_pair("decks", &decks_text(&f.decks));
    }
    q.finish()
}

fn decks_text(decks: &[DeckFacet]) -> String {
    decks
        .iter()
        .map(|d| match d {
            DeckFacet::Own => "own".to_string(),
            DeckFacet::Sub(id) => id.0.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) struct Chip {
    kind: &'static str,
    value: String,
    label: String,
}

pub(super) struct CardsView {
    cards: Vec<CardView>,
    deck_id: i64,
    page: u32,
    pages: u32,
    total: u32,
    q: String,
    archived_view: bool,
    archived_count: u32,
    query: String,
    chips: Vec<Chip>,
    tag_count: usize,
    match_all: bool,
    narrowed: bool,
}

impl CardsView {
    fn link(&self, path: &str, page: Option<u32>) -> String {
        let page = page.map(|n| format!("page={n}"));
        let query: Vec<&str> = [Some(self.query.as_str()), page.as_deref()]
            .into_iter()
            .flatten()
            .filter(|q| !q.is_empty())
            .collect();
        let mut url = format!("/decks/{}{path}", self.deck_id);
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query.join("&"));
        }
        url
    }

    fn page_link(&self, n: u32) -> String {
        self.link("/cards", Some(n))
    }

    fn address(&self) -> String {
        self.link("", (self.page > 1).then_some(self.page))
    }

    fn bulk_link(&self) -> String {
        self.link("/cards/bulk", Some(self.page))
    }
}

fn subdeck_labels(root: &DeckRow, subdecks: &[DeckRow]) -> HashMap<i64, (String, i64)> {
    let mut labels = HashMap::new();
    for d in subdecks {
        let rest = d
            .name
            .strip_prefix(&root.name)
            .and_then(|r| r.strip_prefix(deck_path::SEP))
            .unwrap_or(&d.name);
        let top = rest.split(deck_path::SEP).next().unwrap_or(rest);
        let top_id = subdecks
            .iter()
            .find(|s| s.name == format!("{}{}{top}", root.name, deck_path::SEP))
            .map_or(d.id.0, |s| s.id.0);
        labels.insert(d.id.0, (rest.replace(deck_path::SEP, " › "), top_id));
    }
    labels
}

fn cards_view(
    s: &Services,
    user: UserId,
    family: &DeckFamily,
    filter: &DeckFilter,
    page: Option<u32>,
) -> Result<CardsView, ServiceError> {
    let deck = family.deck.id;
    let view = s.deck_cards_filtered(user, deck, filter, page.unwrap_or(1), CARDS_PER_PAGE)?;
    let labels = subdeck_labels(&family.deck, &family.subdecks);
    let mut cards = Vec::with_capacity(view.rows.len());
    for row in view.rows {
        let from = labels.get(&row.deck_id.0).cloned();
        let mut card = CardView::from_row(s, user, row)?;
        if let Some((label, top)) = from {
            card.deck_label = Some(label);
            card.deck_ref = top;
        }
        cards.push(card);
    }
    let mut chips = Vec::new();
    if filter.status == CardStatus::Archived {
        chips.push(Chip {
            kind: "status",
            value: "archived".into(),
            label: crate::i18n::t("cards-chip-archived").to_string(),
        });
    }
    for facet in &filter.decks {
        let (value, label) = match facet {
            DeckFacet::Own => ("own".to_string(), crate::i18n::t("cards-own").to_string()),
            DeckFacet::Sub(id) => (
                id.0.to_string(),
                family
                    .subdecks
                    .iter()
                    .find(|d| d.id == *id)
                    .map_or_else(|| id.0.to_string(), |d| d.leaf.clone()),
            ),
        };
        chips.push(Chip {
            kind: "deck",
            value,
            label,
        });
    }
    for tag in &filter.tags {
        chips.push(Chip {
            kind: "tag",
            value: tag.clone(),
            label: tag.clone(),
        });
    }
    Ok(CardsView {
        cards,
        deck_id: deck.0,
        page: view.page,
        pages: view.pages,
        total: view.total,
        q: filter.search.clone().unwrap_or_default(),
        archived_view: filter.status == CardStatus::Archived,
        archived_count: view.archived,
        query: filter_query(filter),
        chips,
        tag_count: filter.tags.len(),
        match_all: filter.match_all,
        narrowed: filter.is_narrowed(),
    })
}

pub(super) struct SubRow {
    id: i64,
    leaf: String,
    due: u32,
    new: u32,
    cards: u32,
}

pub(super) struct FacetOpt {
    value: String,
    label: String,
    count: u32,
    checked: bool,
}

#[derive(Template)]
#[template(path = "deck.html")]
pub(super) struct DeckPage {
    shell: Shell,
    deck_id: i64,
    deck_name: String,
    deck_leaf: String,
    crumbs: Vec<Crumb>,
    archived: bool,
    due: u32,
    new: u32,
    card_total: u32,
    preview_cards: u32,
    goal: Option<super::goal::GoalMark>,
    has_goal: bool,
    header_html: String,
    subs: Vec<SubRow>,
    archived_subs: Vec<SubRow>,
    own_count: u32,
    own_checked: bool,
    facets: Vec<FacetOpt>,
    tags: Vec<FacetOpt>,
    move_targets: Vec<(i64, String)>,
    list: CardsView,
    editor_error: Option<crate::i18n::Words>,
    editor: EditorView,
    add_cards: String,
}

pub(super) async fn deck_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
    Query(query): Query<CardsQuery>,
    via: Via,
) -> Response {
    let shell = Shell::new(&state, theme, "decks", is_admin);
    view_event(&state, user, "deck", via);
    let editor_error = query.err.as_deref().and_then(editor_error_copy);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let deck = DeckId(id);
        let add_cards = app
            .ext
            .render(&app, Slot::AddCards(AddCardsAt::DeckMenu { deck }))?;
        let now = now_ms();
        let all_decks = s.list_decks(user, now)?;
        let move_targets = all_decks
            .iter()
            .filter(|d| d.archived_at.is_none())
            .map(|d| (d.id.0, d.path.clone()))
            .collect();
        let family = DeckFamily::of(all_decks, deck)?;
        let filter = query.filter();
        let list = cards_view(&s, user, &family, &filter, query.page)?;
        let counts = s.deck_card_counts(user, deck)?;
        let in_tree = |root: &DeckRow| -> u32 {
            counts
                .iter()
                .filter(|(id, _)| {
                    **id == root.id
                        || family
                            .subdecks
                            .iter()
                            .any(|d| d.id == **id && deck_path::is_descendant(&d.name, &root.name))
                })
                .map(|(_, n)| n)
                .sum()
        };
        let row = |d: &DeckRow| SubRow {
            id: d.id.0,
            leaf: d.leaf.clone(),
            due: d.due_count,
            new: d.new_count,
            cards: in_tree(d),
        };
        let children: Vec<&DeckRow> = family.subdecks.iter().filter(|d| d.depth == 0).collect();
        let subs: Vec<SubRow> = children
            .iter()
            .filter(|d| d.archived_at.is_none())
            .map(|d| row(d))
            .collect();
        let archived_subs = children
            .iter()
            .filter(|d| d.archived_at.is_some())
            .map(|d| row(d))
            .collect();
        let facets = subs
            .iter()
            .map(|sub| FacetOpt {
                value: sub.id.to_string(),
                label: sub.leaf.clone(),
                count: sub.cards,
                checked: filter.decks.contains(&DeckFacet::Sub(DeckId(sub.id))),
            })
            .collect();
        let tags = s
            .list_tags(user, Some(deck))?
            .into_iter()
            .map(|t| FacetOpt {
                checked: filter.tags.contains(&t.name),
                value: t.name.clone(),
                label: t.name,
                count: t.cards,
            })
            .collect();
        let summary = &family.deck;
        Ok(DeckPage {
            shell,
            deck_id: id,
            deck_name: summary.path.clone(),
            deck_leaf: summary.leaf.clone(),
            archived: summary.archived_at.is_some(),
            due: summary.due_count,
            new: summary.new_count,
            card_total: in_tree(summary),
            preview_cards: summary.card_count,
            goal: summary.goal.as_ref().map(super::goal::mark),
            has_goal: summary.goal.is_some(),
            header_html: app.ext.render(&app, Slot::DeckHeader { user, deck })?,
            own_count: counts.get(&deck).copied().unwrap_or(0),
            own_checked: filter.decks.contains(&DeckFacet::Own),
            subs,
            archived_subs,
            facets,
            tags,
            move_targets,
            list,
            editor_error,
            editor: EditorView::empty(),
            add_cards,
            crumbs: family.crumbs,
        })
    })
    .await
}

#[derive(Template)]
#[template(path = "card_rows.html")]
pub(super) struct CardRowsPartial {
    list: CardsView,
}

/// The list as its search, filter or pager asks for it. The answer names
/// the page's address for the new filter, so the address bar follows.
pub(super) async fn card_rows(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Query(query): Query<CardsQuery>,
) -> Response {
    let rendered = act(state.services.clone(), move |s| {
        let family = DeckFamily::of(s.list_decks(user, now_ms())?, DeckId(id))?;
        let list = cards_view(&s, user, &family, &query.filter(), query.page)?;
        let address = list.address();
        Ok((render(&CardRowsPartial { list })?, address))
    })
    .await;
    match rendered {
        Ok((html, address)) => {
            let mut response = Html(html).into_response();
            if let Ok(v) = HeaderValue::from_str(&address) {
                response.headers_mut().insert("HX-Push-Url", v);
            }
            response
        }
        Err(response) => response,
    }
}

fn render<T: Template>(template: &T) -> Result<String, ServiceError> {
    crate::i18n::render_current(template)
        .map_err(|e| ServiceError::Internal(format!("render: {e}")))
}

/// What an Undo puts back, carried by the toast's button and checked
/// again when it comes back: every card and deck in it must still be the
/// caller's.
#[derive(Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
enum UndoRecord {
    Archive {
        cards: Vec<i64>,
        archived: bool,
    },
    Move {
        moves: Vec<(i64, i64)>,
    },
    Tags {
        pairs: Vec<(i64, String)>,
        added: bool,
    },
}

#[derive(Deserialize)]
pub(super) struct BulkForm {
    #[serde(deserialize_with = "crate::bounds::keyword")]
    action: String,
    #[serde(default, deserialize_with = "crate::bounds::opt_bulk_ids")]
    ids: Option<String>,
    /// Every card the filter matches, instead of `ids`.
    #[serde(default)]
    all: bool,
    /// With `all`: how many the page said that was. A different count
    /// means the list changed underneath, and nothing is done.
    #[serde(default)]
    expect: Option<u32>,
    /// Move: the deck to move to.
    #[serde(default)]
    to: Option<i64>,
    /// Tag and untag: the tags.
    #[serde(default, deserialize_with = "crate::bounds::opt_tag_list_text")]
    tags: Option<String>,
    /// Undo: the record the toast carried.
    #[serde(default, deserialize_with = "crate::bounds::opt_undo_record")]
    undo: Option<String>,
}

pub(super) struct Toast {
    message: String,
    undo: Option<String>,
}

#[derive(Template)]
#[template(
    source = r#"{% include "card_rows.html" %}{% include "deck_toast.html" %}"#,
    ext = "html"
)]
pub(super) struct BulkPartial {
    list: CardsView,
    toast: Toast,
}

/// An action on many of a deck's cards: archive, unarchive, delete,
/// move, tag, untag, or the undo of one of them. The list comes back for
/// the same filter and page, with a toast swapped in beside it.
pub(super) async fn bulk(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Query(query): Query<CardsQuery>,
    Form(form): Form<BulkForm>,
) -> Response {
    let rendered = act(state.services.clone(), move |s| {
        let deck = DeckId(id);
        let now = now_ms();
        let filter = query.filter();
        let (toast, event) = apply(&s, user, deck, &filter, &form, now)?;
        let family = DeckFamily::of(s.list_decks(user, now)?, deck)?;
        let list = cards_view(&s, user, &family, &filter, query.page)?;
        Ok((render(&BulkPartial { list, toast })?, event))
    })
    .await;
    match rendered {
        Ok((html, event)) => {
            if let Some((event, n)) = event {
                account_event(&state, user, event, None, Some(n));
            }
            Html(html).into_response()
        }
        Err(response) => response,
    }
}

type Event = Option<(&'static str, i64)>;

fn apply(
    s: &Services,
    user: UserId,
    deck: DeckId,
    filter: &DeckFilter,
    form: &BulkForm,
    now: i64,
) -> Result<(Toast, Event), ServiceError> {
    use crate::i18n::{ta, Value};
    let count = |n: usize| -> Value { (n as i64).into() };
    if form.action == "undo" {
        let record: UndoRecord = form
            .undo
            .as_deref()
            .and_then(|r| serde_json::from_str(r).ok())
            .ok_or_else(|| ServiceError::Malformed("no undo record".into()))?;
        match record {
            UndoRecord::Archive { cards, archived } => {
                let cards: Vec<CardId> = cards.into_iter().map(CardId).collect();
                s.restore_archived(user, &cards, archived, now)?;
            }
            UndoRecord::Move { moves } => {
                let moves: Vec<(CardId, DeckId)> = moves
                    .into_iter()
                    .map(|(c, d)| (CardId(c), DeckId(d)))
                    .collect();
                s.return_cards(user, &moves, now)?;
            }
            UndoRecord::Tags { pairs, added } => {
                let pairs: Vec<(CardId, String)> =
                    pairs.into_iter().map(|(c, t)| (CardId(c), t)).collect();
                s.restore_tag_links(user, &pairs, !added, now)?;
            }
        }
        let toast = Toast {
            message: crate::i18n::t("cards-bulk-undone").to_string(),
            undo: None,
        };
        return Ok((toast, None));
    }

    let cards: Vec<CardId> = if form.all {
        let ids = s.deck_card_ids(user, deck, filter)?;
        if form.expect.is_some_and(|n| n as usize != ids.len()) {
            return Err(ServiceError::refused(
                "changed",
                "cards-error-bulk-changed",
                &[],
            ));
        }
        ids
    } else {
        form.ids
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter_map(|id| id.trim().parse().ok().map(CardId))
            .collect()
    };
    if cards.is_empty() {
        return Err(ServiceError::Malformed("no cards named".into()));
    }
    let undo_of = |changed: usize, record: UndoRecord| {
        serde_json::to_string(&record)
            .ok()
            .filter(|r| changed > 0 && r.len() <= crate::bounds::UNDO_RECORD)
    };
    let (toast, event) = match BulkAction::parse(&form.action)? {
        BulkAction::Archive(archived) => {
            let changed = s.archive_cards(user, deck, &cards, archived, now)?;
            let id = if archived {
                "cards-bulk-archived"
            } else {
                "cards-bulk-unarchived"
            };
            let n = changed.len();
            let undo = undo_of(
                n,
                UndoRecord::Archive {
                    cards: changed.iter().map(|c| c.0).collect(),
                    archived: !archived,
                },
            );
            let event = if archived {
                "cards_archived"
            } else {
                "cards_unarchived"
            };
            (
                Toast {
                    message: ta(id, &[("count", count(n))]).to_string(),
                    undo,
                },
                Some((event, n as i64)),
            )
        }
        BulkAction::Delete => {
            let n = s.delete_cards(user, deck, &cards, now)? as usize;
            (
                Toast {
                    message: ta("cards-bulk-deleted", &[("count", count(n))]).to_string(),
                    undo: None,
                },
                Some(("cards_deleted", n as i64)),
            )
        }
        BulkAction::Move => {
            let to = DeckId(
                form.to
                    .ok_or_else(|| ServiceError::Malformed("no deck to move to".into()))?,
            );
            let moved = s.move_cards(user, deck, &cards, to, now)?;
            let name = s.deck_by_id(user, to, now)?.path;
            let n = moved.len();
            let undo = undo_of(
                n,
                UndoRecord::Move {
                    moves: moved.iter().map(|(c, d)| (c.0, d.0)).collect(),
                },
            );
            (
                Toast {
                    message: ta(
                        "cards-bulk-moved",
                        &[("count", count(n)), ("deck", name.into())],
                    )
                    .to_string(),
                    undo,
                },
                Some(("cards_moved", n as i64)),
            )
        }
        BulkAction::Tag(add) => {
            let tags = split_tags(form.tags.as_deref().unwrap_or_default());
            let changed = s.tag_cards(user, deck, &cards, &tags, add, now)?;
            let touched = {
                let mut ids: Vec<i64> = changed.iter().map(|(c, _)| c.0).collect();
                ids.sort_unstable();
                ids.dedup();
                ids.len()
            };
            let undo = undo_of(
                changed.len(),
                UndoRecord::Tags {
                    pairs: changed.iter().map(|(c, t)| (c.0, t.clone())).collect(),
                    added: add,
                },
            );
            let id = if add {
                "cards-bulk-tagged"
            } else {
                "cards-bulk-untagged"
            };
            (
                Toast {
                    message: ta(id, &[("count", count(touched))]).to_string(),
                    undo,
                },
                Some(("cards_tagged", touched as i64)),
            )
        }
    };
    Ok((toast, event))
}

#[derive(Clone, Copy)]
enum BulkAction {
    Archive(bool),
    Delete,
    Move,
    Tag(bool),
}

impl BulkAction {
    fn parse(action: &str) -> Result<Self, ServiceError> {
        Ok(match action {
            "archive" => Self::Archive(true),
            "unarchive" => Self::Archive(false),
            "delete" => Self::Delete,
            "move" => Self::Move,
            "tag" => Self::Tag(true),
            "untag" => Self::Tag(false),
            _ => return Err(ServiceError::Malformed("unknown action".into())),
        })
    }
}
