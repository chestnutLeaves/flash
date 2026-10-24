//! A deck's preview: every card a study session there could ask, one
//! after another, each marked missed or got, with spaced repetition left
//! alone. The page carries the cards and `static/preview.js` walks them;
//! nothing goes back to the server, so a preview schedules and records
//! nothing.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::Response;
use flash_core::DeckId;

use crate::auth::{AuthUser, Theme};
use crate::i18n::{filters, ta, Words};
use crate::service::{now_ms, NextDue};
use crate::state::AppState;
use crate::web::{respond, Shell};

#[derive(Template)]
#[template(path = "preview.html")]
struct PreviewPage {
    shell: Shell,
    deck_id: i64,
    deck_leaf: String,
    cards: Vec<PreviewCard>,
    outlook: Option<Words>,
}

struct PreviewCard {
    front: String,
    front_html: Option<String>,
    back: String,
    back_html: Option<String>,
}

pub async fn preview_page(
    State(state): State<AppState>,
    AuthUser { id: user, is_admin }: AuthUser,
    theme: Theme,
    Path(id): Path<i64>,
) -> Response {
    let shell = Shell::new(&state, theme, "study", is_admin);
    respond(state.services.clone(), move |s| {
        let now = now_ms();
        let (deck, rows) = s.preview_cards(user, DeckId(id), now)?;
        let (ready, next) = s.preview_outlook(user, deck.id, now)?;
        let outlook = if ready > 0 {
            Some(ta("preview-ready", &[("count", ready.into())]))
        } else {
            let zone = flash_core::day::time_zone(&s.store().get_settings(user)?.timezone)
                .unwrap_or(jiff::tz::TimeZone::UTC);
            next.map(|n| {
                let at = jiff::Timestamp::from_millisecond(n.at_ms)
                    .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
                    .to_zoned(zone);
                next_words(&n, &at)
            })
        };
        Ok(PreviewPage {
            shell,
            deck_id: id,
            deck_leaf: deck.leaf,
            cards: rows
                .into_iter()
                .map(|c| PreviewCard {
                    front: c.front,
                    front_html: c.front_html,
                    back: c.back,
                    back_html: c.back_html,
                })
                .collect(),
            outlook,
        })
    })
    .await
}

fn next_words(next: &NextDue, at: &jiff::Zoned) -> Words {
    let count = ("count", i64::from(next.cards).into());
    match next.in_days {
        0 => ta("preview-next-later-today", &[count]),
        1 => ta("preview-next-tomorrow", &[count]),
        2..=6 => ta(
            "preview-next-weekday",
            &[count, ("day", crate::web::weekday(at, true).into())],
        ),
        n => ta(
            "preview-next-in-days",
            &[count, ("days", i64::from(n).into())],
        ),
    }
}
