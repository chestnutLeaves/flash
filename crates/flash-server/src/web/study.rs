use askama::Template;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::Form;
use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, Rating, SessionId, UserId};
use serde::Deserialize;

use crate::auth::{ApiUser, AuthUser, Theme};
use crate::ext::{AddCardsAt, Slot};
use crate::i18n::filters;
use crate::service::{
    now_ms, CardFront, DiffSpan, ReviewOrigin, ServiceError, Services, SessionSurface, StudyCard,
};
use crate::state::AppState;

use super::cards::split_tags;
use super::{account_event, respond, view_event, Shell, Via};

/// What the study card shows: rich HTML when the import carried it, and
/// whether this card asks for a typed answer.
struct StudyCardView {
    card_id: i64,
    front: String,
    front_html: Option<String>,
    front_lang: String,
    ext_data: Vec<(&'static str, String)>,
    wants_typing: bool,
    /// Images on this card's (still hidden) back: emitted as
    /// `<link rel="preload">` so they're cached before the reveal.
    preload: Vec<i64>,
    /// Images on the next card (both sides): `<link rel="prefetch">`.
    prefetch: Vec<i64>,
    /// The card after this one, in `data-next-card`, carried through an
    /// edit so the card shows again as it was (static/study.js).
    next_card: Option<i64>,
}

struct RevealView {
    back: String,
    back_html: Option<String>,
    back_lang: String,
    /// Typed-answer comparison, char-aligned against the expected answer.
    typed_diff: Option<Vec<DiffSpan>>,
    /// When the card comes back after each grade, Again to Easy, as each
    /// button says it ("10m", "2d"); empty when the preview failed, and
    /// the buttons then grade without them.
    comes_back: Vec<crate::i18n::Words>,
}

/// How long until a card comes back, as a grade button says it, short:
/// `10m`, `3h`, `2d`, `4mo`, `1.5y`. The study panel rounds the same
/// way (panel.html `comesBack`).
fn comes_back_label(ms: i64) -> crate::i18n::Words {
    let ms = ms as f64;
    let say = |id: &str, n: f64| crate::i18n::ta(id, &[("count", n.into())]);
    let minutes = (ms / 60_000.0).round();
    if minutes < 60.0 {
        return say("interval-minutes", minutes.max(1.0));
    }
    let hours = (ms / 3_600_000.0).round();
    if hours < 24.0 {
        return say("interval-hours", hours);
    }
    let days = (ms / 86_400_000.0).round();
    if days < 31.0 {
        return say("interval-days", days);
    }
    let months = (days / 30.4).round();
    if months < 12.0 {
        return say("interval-months", months);
    }
    say("interval-years", (days / 36.5).round() / 10.0)
}

fn card_view(app: &AppState, user: UserId, c: StudyCard) -> StudyCardView {
    StudyCardView {
        ext_data: app.ext.study_card_data(app, user, c.card_id, false),
        card_id: c.card_id.0,
        front: c.front,
        front_html: c.front_html,
        front_lang: c.front_lang,
        wants_typing: c.wants_typing,
        preload: c.preload_media,
        prefetch: c.prefetch_media,
        next_card: c.next_card.map(|id| id.0),
    }
}

#[derive(Template)]
#[template(path = "study.html")]
struct StudyPage {
    shell: Shell,
    card: Option<StudyCardView>,
    revealed: Option<RevealView>,
    typed: Option<String>,
    /// When this card's front was first put on screen (epoch ms), carried
    /// through Show answer to the grade, which turns it into answer time.
    shown_at: i64,
    session_id: i64,
    remaining: u32,
    total: u32,
    /// Eligible cards today's limits kept out of the queue. The empty
    /// state explains itself instead of a bare "nothing due" (same fix
    /// the MCP queue_note applies for assistants).
    held_by_limit: u32,
    /// The deck being studied, if the session is deck-scoped. The done
    /// state offers a boost for that deck rather than the account.
    scope_deck: Option<i64>,
    done: Option<DoneView>,
    archived: Option<i64>,
    actions_html: String,
    foot_html: String,
    /// The tags the session is filtered by, and the picker's rows: the
    /// tags of the deck being studied, or of every deck.
    scope_tags: Vec<String>,
    tags_label: crate::i18n::Words,
    tag_options: Vec<TagOption>,
}

/// One row of the study page's tag picker.
struct TagOption {
    name: String,
    cards: u32,
    checked: bool,
}

/// What the study header's link reads: the filter itself, short enough
/// to sit beside the heading.
fn tags_label(tags: &[String]) -> crate::i18n::Words {
    use crate::i18n::{t, ta};
    match tags.len() {
        0 => t("study-tags"),
        1..=2 => ta(
            "study-tags-some",
            &[(
                "tags",
                crate::i18n::list(tags, crate::i18n::ListKind::Unit).into(),
            )],
        ),
        n => ta(
            "study-tags-more",
            &[
                ("first", tags[0].as_str().into()),
                ("second", tags[1].as_str().into()),
                ("more", (n - 2).into()),
            ],
        ),
    }
}

/// The study page for a scope, as a link. The picker submits a form of
/// the same shape; this is for the redirects back into a queue.
pub(super) fn study_url(deck: Option<i64>, tags: &[String]) -> String {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if let Some(deck) = deck {
        q.append_pair("deck", &deck.to_string());
    }
    if !tags.is_empty() {
        q.append_pair("tags", &tags.join(","));
    }
    let q = q.finish();
    if q.is_empty() {
        "/study".to_string()
    } else {
        format!("/study?{q}")
    }
}

#[derive(Template)]
#[template(path = "study_card.html")]
struct StudyCardPartial {
    card: Option<StudyCardView>,
    revealed: Option<RevealView>,
    typed: Option<String>,
    /// As on `StudyPage`.
    shown_at: i64,
    session_id: i64,
    remaining: u32,
    total: u32,
    held_by_limit: u32,
    /// The deck being studied, if the session is deck-scoped. The done
    /// state offers a boost for that deck rather than the account.
    scope_deck: Option<i64>,
    /// The tags the session is filtered by, so a boost returns to the
    /// same queue rather than the whole account's.
    scope_tags: Vec<String>,
    /// The finished screen, once the queue is empty.
    done: Option<DoneView>,
    archived: Option<i64>,
}

/// A finished session as the study page shows it, as the MCP panel does:
/// its grades and time, the week ahead, when the next cards come.
struct DoneView {
    reviewed: u32,
    took: Option<crate::i18n::Words>,
    pills: Vec<PillView>,
    /// Empty when no card comes due all week.
    week: Vec<DayView>,
    next: Option<crate::i18n::Words>,
    /// Whether the page offers due-card reminders
    /// (`ServerExtension::reminders_offer`).
    remind: bool,
    add_cards: String,
    goal: Option<super::goal::GoalDone>,
}

/// One grade's count, as a tinted pill: `kind` is good, easy, hard or again.
struct PillView {
    kind: &'static str,
    label: crate::i18n::Words,
}

/// One day of the week ahead.
struct DayView {
    cards: u32,
    /// The bar's height, by the busiest day.
    bar_px: u32,
    name: String,
    /// The day the next cards come.
    next: bool,
}

/// The tallest bar of the week ahead, in px.
const WEEK_BAR_PX: u32 = 64;

fn done_view(
    app: &AppState,
    s: &Services,
    user: UserId,
    session: SessionId,
    held_by_limit: u32,
    now: i64,
) -> Result<DoneView, ServiceError> {
    let done = s.finished_session(user, session, held_by_limit, now)?;
    let zone = flash_core::day::time_zone(&s.store().get_settings(user)?.timezone)
        .unwrap_or(jiff::tz::TimeZone::UTC);
    let at = |ms: i64| {
        jiff::Timestamp::from_millisecond(ms)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
            .to_zoned(zone.clone())
    };
    let stats = done.stats;
    let pills = [
        ("good", stats.good),
        ("easy", stats.easy),
        ("hard", stats.hard),
        ("again", stats.again),
    ]
    .into_iter()
    .filter(|&(_, n)| n > 0)
    .map(|(kind, n)| PillView {
        kind,
        label: crate::i18n::ta(&format!("study-pill-{kind}"), &[("count", n.into())]),
    })
    .collect();
    let took = done
        .took_ms
        .map(|ms| (ms + 30_000) / 60_000)
        .map(|minutes| match minutes {
            0 => crate::i18n::t("study-took-under-minute"),
            n => crate::i18n::ta("study-took-minutes", &[("count", n.into())]),
        });
    let busiest = done.week.iter().map(|d| d.cards).max().unwrap_or(0);
    let next_day = done
        .next_due
        .filter(|n| n.in_days >= 1)
        .map(|n| n.in_days as usize - 1);
    let week = if busiest == 0 {
        Vec::new()
    } else {
        done.week
            .iter()
            .enumerate()
            .map(|(i, d)| DayView {
                cards: d.cards,
                bar_px: (WEEK_BAR_PX * d.cards / busiest).max(3),
                name: weekday(&at(d.at_ms), false),
                next: next_day == Some(i),
            })
            .collect()
    };
    let remind = app
        .ext
        .reminders_offer(app, user)
        .inspect_err(|e| tracing::warn!("study reminders offer: {e}"))
        .unwrap_or(false);
    Ok(DoneView {
        reviewed: stats.reviewed,
        took,
        pills,
        week,
        next: done.next_due.map(|n| next_words(&n, &at(n.at_ms))),
        remind,
        add_cards: app.ext.render(app, Slot::AddCards(AddCardsAt::StudyDone))?,
        goal: done.goal.as_ref().map(super::goal::done),
    })
}

/// "6 cards due tomorrow" for a session that ran out: tomorrow, a
/// weekday within the week, or a number of days; "later today" only on
/// an account that keeps exact times.
fn next_words(next: &crate::service::NextDue, at: &jiff::Zoned) -> crate::i18n::Words {
    let cards = i64::from(next.cards);
    let say = |id: &str, extra: (&str, crate::i18n::Value)| {
        crate::i18n::ta(id, &[("count", cards.into()), extra])
    };
    match next.in_days {
        0 => {
            let time = match crate::i18n::tr().formats() {
                Some(f) => f.time(at.time()),
                None => at.strftime("%-I:%M %p").to_string(),
            };
            let hour = crate::i18n::clock_hour(&time);
            crate::i18n::ta(
                "study-next-later-today",
                &[
                    ("count", cards.into()),
                    ("time", time.into()),
                    ("hour", hour.into()),
                ],
            )
        }
        1 => crate::i18n::ta("study-next-tomorrow", &[("count", cards.into())]),
        2..=6 => say("study-next-weekday", ("day", weekday(at, true).into())),
        n => say("study-next-in-days", ("days", i64::from(n).into())),
    }
}

pub(crate) fn weekday(at: &jiff::Zoned, whole: bool) -> String {
    match (crate::i18n::tr().formats(), whole) {
        (Some(f), true) => f.weekday_long(at.date()),
        (Some(f), false) => f.weekday(at.date()),
        (None, true) => at.strftime("%A").to_string(),
        (None, false) => at.strftime("%a").to_string(),
    }
}

#[derive(Deserialize)]
pub(super) struct StudyQuery {
    deck: Option<i64>,
    /// One tag, as older links spell it; merged into `tags`.
    #[serde(default, deserialize_with = "crate::bounds::opt_tag")]
    tag: Option<String>,
    /// The picked tags, comma-joined: a card must carry every one.
    #[serde(default, deserialize_with = "crate::bounds::opt_scope_tag_text")]
    tags: Option<String>,
}

pub(super) async fn study_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Query(query): Query<StudyQuery>,
    via: Via,
) -> Response {
    let shell = Shell::new(&state, theme, "study", is_admin);
    view_event(&state, user, "study", via);
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let scope_deck = query.deck;
        let picked = query
            .tags
            .as_deref()
            .map(split_tags)
            .unwrap_or_default()
            .into_iter()
            .chain(query.tag);
        let scope = StudyScope::new(query.deck.map(DeckId), picked);
        let scope_tags = scope.tags.clone();
        let now = now_ms();
        let session = s.start_session_from(user, scope.clone(), SessionSurface::Web, now)?;
        let done = if session.cards_due == 0 && session.scope_has_cards {
            let held = session.held_by_limit;
            Some(done_view(&app, &s, user, session.session_id, held, now)?)
        } else {
            None
        };
        let tag_options: Vec<TagOption> = s
            .list_tags(user, scope_deck.map(DeckId))?
            .into_iter()
            .map(|t| TagOption {
                checked: scope_tags.contains(&t.name),
                name: t.name,
                cards: t.cards,
            })
            .collect();
        let card = session
            .first_card
            .map(|c| s.study_card(user, c, session.next_card_id))
            .transpose()?
            .map(|c| card_view(&app, user, c));
        Ok(StudyPage {
            shell,
            card,
            revealed: None,
            typed: None,
            shown_at: now,
            session_id: session.session_id.0,
            remaining: session.cards_due,
            total: session.cards_due,
            held_by_limit: session.held_by_limit,
            scope_deck,
            actions_html: app.ext.render(&app, Slot::StudyActions)?,
            foot_html: app.ext.render(&app, Slot::StudyFoot { user })?,
            tags_label: tags_label(&scope_tags),
            scope_tags,
            tag_options,
            done,
            archived: None,
        })
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct RevealForm {
    session: i64,
    card: i64,
    remaining: u32,
    #[serde(default)]
    total: u32,
    /// When the front went on screen (`StudyPage::shown_at`).
    #[serde(default)]
    shown: Option<i64>,
    /// What the user typed, when the card asks for it.
    #[serde(default, deserialize_with = "crate::bounds::opt_typed_answer")]
    typed: Option<String>,
}

pub(super) async fn study_reveal(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Form(form): Form<RevealForm>,
) -> Response {
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let at = StudyAt {
            session: form.session,
            remaining: form.remaining,
            total: form.total,
            shown: form.shown.unwrap_or(0),
        };
        revealed_partial(&app, &s, user, CardId(form.card), form.typed, at)
    })
    .await
}

struct StudyAt {
    session: i64,
    remaining: u32,
    total: u32,
    shown: i64,
}

fn revealed_partial(
    app: &AppState,
    s: &Services,
    user: UserId,
    card: CardId,
    typed: Option<String>,
    at: StudyAt,
) -> Result<StudyCardPartial, ServiceError> {
    let reveal = s.reveal_card(user, card, typed.as_deref())?;
    let comes_back = s
        .grade_preview(user, card, now_ms())
        .inspect_err(|e| tracing::warn!("web grade preview: {e}"))
        .map(|ms| ms.iter().map(|&m| comes_back_label(m)).collect())
        .unwrap_or_default();
    Ok(StudyCardPartial {
        shown_at: at.shown,
        card: Some(StudyCardView {
            ext_data: app.ext.study_card_data(app, user, reveal.card_id, true),
            card_id: reveal.card_id.0,
            front: reveal.front,
            front_html: reveal.front_html,
            front_lang: reveal.front_lang,
            wants_typing: false,
            preload: Vec::new(),
            prefetch: Vec::new(),
            next_card: None,
        }),
        revealed: Some(RevealView {
            back: reveal.back,
            back_html: reveal.back_html,
            back_lang: reveal.back_lang,
            typed_diff: reveal.typed_diff,
            comes_back,
        }),
        typed,
        session_id: at.session,
        remaining: at.remaining,
        total: at.total,
        held_by_limit: 0,
        scope_deck: None,
        scope_tags: Vec::new(),
        done: None,
        archived: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn front_partial(
    app: &AppState,
    s: &Services,
    user: UserId,
    front: Option<CardFront>,
    next: Option<CardId>,
    held_by_limit: u32,
    at: StudyAt,
) -> Result<StudyCardPartial, ServiceError> {
    let now = now_ms();
    let (scope_deck, scope_tags, done) = if front.is_none() {
        let session = SessionId(at.session);
        let scope = s.session_scope(user, session)?;
        let done = done_view(app, s, user, session, held_by_limit, now)?;
        (scope.deck.map(|d| d.0), scope.tags, Some(done))
    } else {
        (None, Vec::new(), None)
    };
    let card = front
        .map(|c| s.study_card(user, c, next))
        .transpose()?
        .map(|c| card_view(app, user, c));
    Ok(StudyCardPartial {
        card,
        revealed: None,
        typed: None,
        shown_at: at.shown,
        session_id: at.session,
        remaining: at.remaining,
        total: at.total,
        held_by_limit,
        scope_deck,
        scope_tags,
        done,
        archived: None,
    })
}

#[derive(Deserialize)]
pub(super) struct StudyCardQuery {
    session: i64,
    card: i64,
    remaining: u32,
    #[serde(default)]
    total: u32,
    /// When the front went on screen, less the time spent editing.
    #[serde(default)]
    shown: Option<i64>,
    #[serde(default)]
    revealed: bool,
    #[serde(default, deserialize_with = "crate::bounds::opt_typed_answer")]
    typed: Option<String>,
    #[serde(default)]
    next: Option<i64>,
}

/// The card on the study page shown again after an edit (study.js asks
/// once the edit dialog saved): front or answer as it was, the same
/// session and counts. A card the edit took away (a new note type drops
/// it) gives way to the session's next card, or the finished screen.
pub(super) async fn study_card_again(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Query(query): Query<StudyCardQuery>,
) -> Response {
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let session = SessionId(query.session);
        s.session_scope(user, session)?;
        let card = CardId(query.card);
        let at = StudyAt {
            session: query.session,
            remaining: query.remaining,
            total: query.total,
            shown: query.shown.unwrap_or(0),
        };
        match s.study_front(user, card)? {
            Some(_) if query.revealed => revealed_partial(&app, &s, user, card, query.typed, at),
            Some(front) => {
                let next = query.next.map(CardId);
                front_partial(&app, &s, user, Some(front), next, 0, at)
            }
            None => {
                let now = now_ms();
                let head = s.session_head(user, session, now)?;
                let at = StudyAt {
                    remaining: head.remaining,
                    shown: now,
                    ..at
                };
                front_partial(
                    &app,
                    &s,
                    user,
                    head.next_card,
                    head.next_card_id,
                    head.held_by_limit,
                    at,
                )
            }
        }
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct ReviewForm {
    session: i64,
    card: i64,
    rating: i64,
    #[serde(default)]
    total: u32,
    /// When the front went on screen (`StudyPage::shown_at`); the grade's
    /// time minus it is the answer time. 0 or absent: not measured.
    #[serde(default)]
    shown: Option<i64>,
}

pub(super) async fn study_review(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Form(form): Form<ReviewForm>,
) -> Response {
    let app = state.clone();
    respond(state.services.clone(), move |s| {
        let rating = Rating::from_i64(form.rating)
            .ok_or_else(|| ServiceError::Malformed("rating must be 1-4".into()))?;
        let now = now_ms();
        let answer_ms = form.shown.filter(|t| *t > 0).map(|t| now - t);
        let result = s.submit_review(
            user,
            SessionId(form.session),
            CardId(form.card),
            rating,
            ReviewOrigin::WEB.answered_in(answer_ms.filter(|ms| *ms >= 0)),
            now,
        )?;
        let at = StudyAt {
            session: form.session,
            remaining: result.remaining,
            total: form.total,
            shown: now,
        };
        front_partial(
            &app,
            &s,
            user,
            result.next_card,
            result.next_card_id,
            result.held_by_limit,
            at,
        )
    })
    .await
}

#[derive(Deserialize)]
pub(super) struct StudyArchiveForm {
    session: i64,
    card: i64,
    #[serde(default)]
    total: u32,
}

/// Archive on the study page: the card on screen leaves study, and the
/// session goes on with its next card, or the finished screen, with Undo
/// offered for the one archived.
pub(super) async fn study_archive(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Form(form): Form<StudyArchiveForm>,
) -> Response {
    let app = state.clone();
    let response = respond(state.services.clone(), move |s| {
        let session = SessionId(form.session);
        s.session_scope(user, session)?;
        let card = CardId(form.card);
        let now = now_ms();
        s.set_card_archived(user, card, true, now)?;
        let head = s.session_head(user, session, now)?;
        let at = StudyAt {
            session: form.session,
            remaining: head.remaining,
            total: form.total,
            shown: now,
        };
        let mut partial = front_partial(
            &app,
            &s,
            user,
            head.next_card,
            head.next_card_id,
            head.held_by_limit,
            at,
        )?;
        partial.archived = Some(card.0);
        Ok(partial)
    })
    .await;
    if response.status().is_success() {
        account_event(&state, user, "cards_archived", None, Some(1));
    }
    response
}

/// Undo of an Archive on the study page: the card comes back to study and
/// to the screen, front up, timed afresh.
pub(super) async fn study_unarchive(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    Form(form): Form<StudyArchiveForm>,
) -> Response {
    let app = state.clone();
    let response = respond(state.services.clone(), move |s| {
        let session = SessionId(form.session);
        s.session_scope(user, session)?;
        let card = CardId(form.card);
        let now = now_ms();
        s.set_card_archived(user, card, false, now)?;
        let head = s.session_head(user, session, now)?;
        let at = StudyAt {
            session: form.session,
            remaining: head.remaining,
            total: form.total,
            shown: now,
        };
        let next = match &head.next_card {
            Some(f) if f.card_id != card => Some(f.card_id),
            _ => head.next_card_id,
        };
        let front = s.study_front(user, card)?;
        front_partial(&app, &s, user, front, next, 0, at)
    })
    .await;
    if response.status().is_success() {
        account_event(&state, user, "cards_unarchived", None, Some(1));
    }
    response
}

/// What the finished screen's reminder switch turned into.
#[derive(Template)]
#[template(
    source = r#"<div class="done-remind" id="done-remind"><span class="done-caption" role="status">{{ message }}</span>{% if on %}<span class="done-switch" role="switch" aria-checked="true" aria-label="{{ "study-remind"|t }}"></span>{% endif %}</div>"#,
    ext = "html"
)]
struct StudyRemindPartial {
    on: bool,
    message: crate::i18n::Words,
}

/// The finished screen's switch: turns on due-card reminders at this time
/// of day in the account's zone, as the MCP panel's does. A server that
/// sends none says where reminders would be.
pub(super) async fn study_remind(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
) -> Response {
    let app = state.clone();
    respond(state.services.clone(), move |_| {
        let message = app.ext.turn_on_reminders(&app, user, "study", now_ms())?;
        Ok(StudyRemindPartial {
            on: message.is_some(),
            message: message.unwrap_or_else(|| crate::i18n::t("study-remind-elsewhere")),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::comes_back_label;

    #[test]
    fn grade_times_are_short() {
        const MIN: i64 = 60_000;
        const DAY: i64 = 24 * 60 * MIN;
        for (ms, want) in [
            (5_000, "1m"),
            (10 * MIN, "10m"),
            (59 * MIN, "59m"),
            (60 * MIN, "1h"),
            (23 * 60 * MIN, "23h"),
            (DAY, "1d"),
            (10 * DAY, "10d"),
            (45 * DAY, "1mo"),
            (200 * DAY, "7mo"),
            (548 * DAY, "1.5y"),
        ] {
            assert_eq!(comes_back_label(ms), want, "{ms}");
        }
    }
}
