//! The study page's tag picker: the rows it offers, the filter it
//! carries back into the session, and the boost that returns to it;
//! and what the grade buttons say.

use crate::common;

use axum::http::StatusCode;
use common::*;
use flash_server::service::now_ms;

/// Two decks of tagged cards: one card carries both tags, one each
/// carries one.
fn tagged(t: &TestApp) -> (String, i64) {
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Pharm",
            &[
                (
                    "both".into(),
                    "b".into(),
                    vec!["nclex".into(), "cardio".into()],
                ),
                ("one".into(), "b".into(), vec!["nclex".into()]),
            ],
            now_ms(),
        )
        .unwrap();
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[("a1".into(), "b".into(), vec!["bones".into()])],
            now_ms(),
        )
        .unwrap();
    let pharm = t.store.find_deck_by_name(user, "Pharm").unwrap().unwrap();
    (cookie, pharm.0)
}

#[tokio::test]
async fn the_picker_lists_every_tag_and_marks_the_picked_ones() {
    let t = AppBuilder::new("study-page-tags").build();
    let (cookie, _) = tagged(&t);

    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    assert!(html.contains("id=\"study-tags-dialog\""));
    assert!(
        html.contains(">Tags</a>"),
        "the link reads plainly with no filter on: {html}"
    );
    for row in [
        r#"<li class="study-tags-row" data-tag="bones">"#,
        r#"<li class="study-tags-row" data-tag="cardio">"#,
        r#"<li class="study-tags-row" data-tag="nclex">"#,
    ] {
        assert!(html.contains(row), "{row} missing");
    }
    assert!(!html.contains("checked"), "nothing is picked yet");

    let html = send(&t.app, get("/study?tags=cardio,NCLEX", Some(&cookie)))
        .await
        .text();
    assert!(
        html.contains(">Tags: cardio, nclex</a>"),
        "the link names the filter: {html}"
    );
    assert!(html.contains("1 left"), "only the card carrying both");
    assert_eq!(
        html.matches(" checked>").count(),
        2,
        "the picked tags come back checked"
    );
    assert!(
        html.contains(r#"value="cardio,nclex" data-study-tags-value"#),
        "and the form starts from them"
    );
}

#[tokio::test]
async fn a_deck_scope_narrows_the_picker_to_that_deck() {
    let t = AppBuilder::new("study-page-tags-deck").build();
    let (cookie, pharm) = tagged(&t);

    let html = send(&t.app, get(&format!("/study?deck={pharm}"), Some(&cookie)))
        .await
        .text();
    assert!(html.contains(r#"data-tag="nclex""#));
    assert!(html.contains(r#"data-tag="cardio""#));
    assert!(
        !html.contains(r#"data-tag="bones""#),
        "another deck's tags are not this deck's filter: {html}"
    );
    assert!(
        html.contains(&format!(
            r#"<input type="hidden" name="deck" value="{pharm}">"#
        )),
        "and a pick stays inside the deck"
    );

    let html = send(
        &t.app,
        get(&format!("/study?deck={pharm}&tags=cardio"), Some(&cookie)),
    )
    .await
    .text();
    assert!(
        html.contains("1 left"),
        "the deck and the tag narrow each other"
    );
}

/// The number after `key` in a page's markup, e.g. `"session": 7`.
fn number_after(html: &str, key: &str) -> i64 {
    let rest = &html[html.find(key).unwrap_or_else(|| panic!("{key} in {html}")) + key.len()..];
    rest.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn a_finished_session_shows_its_grades_the_week_and_the_next_cards() {
    let t = AppBuilder::new("study-page-done").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[
                ("Chambers of the heart?".into(), "Four".into(), vec![]),
                (
                    "Valve between the left chambers?".into(),
                    "Mitral".into(),
                    vec![],
                ),
            ],
            now_ms(),
        )
        .unwrap();

    let mut html = send(&t.app, get("/study", Some(&cookie))).await.text();
    let session = number_after(&html, "\"session\":");
    for rating in [3, 4] {
        let card = number_after(&html, "data-card-id=");
        html = send(
            &t.app,
            form_post(
                "/study/review",
                Some(&cookie),
                &format!("session={session}&card={card}&rating={rating}&total=2"),
            ),
        )
        .await
        .text();
    }
    assert!(html.contains("Session done"), "{html}");
    assert!(
        html.contains("2 cards reviewed in under a minute"),
        "{html}"
    );
    assert!(html.contains(r#"<span class="done-pill done-good">1 good</span>"#));
    assert!(html.contains(r#"<span class="done-pill done-easy">1 easy</span>"#));
    assert!(!html.contains("done-again"), "no grade nobody gave");
    // Good brings its card back within the week: the bars, the day the
    // next cards come in the accent, and that day in words.
    assert_eq!(html.matches(r#"<div class="done-day"#).count(), 7, "{html}");
    assert_eq!(html.matches("done-day-next").count(), 1, "{html}");
    assert!(html.contains(" due "), "{html}");
    // The open server sends no reminders, so it offers no switch, and
    // its route says where reminders would be.
    assert!(!html.contains("done-switch"), "{html}");
    let reply = send(&t.app, form_post("/study/remind", Some(&cookie), "")).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply
        .text()
        .contains("You can turn on reminders in Settings."));

    // Opening the study page with nothing due is the same screen, with
    // nothing reviewed in its own session.
    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    assert!(html.contains("All caught up"), "{html}");
    assert!(html.contains("Nothing is due right now."), "{html}");
    assert!(html.contains("done-day-next"), "{html}");
}

#[tokio::test]
async fn a_finished_session_says_how_its_goal_stands() {
    let t = AppBuilder::new("study-page-goal").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[("Chambers of the heart?".into(), "Four".into(), vec![])],
            now_ms(),
        )
        .unwrap();
    let deck = t.store.find_deck_by_name(user, "Anatomy").unwrap().unwrap();
    let goal = t
        .store
        .get_settings(user)
        .unwrap()
        .study_date(now_ms())
        .checked_add(jiff::Span::new().days(20))
        .unwrap();
    t.services
        .set_deck_goal(user, deck, Some(goal), now_ms())
        .unwrap();

    let html = send(&t.app, get(&format!("/study?deck={deck}"), Some(&cookie)))
        .await
        .text();
    let session = number_after(&html, "\"session\":");
    let card = number_after(&html, "data-card-id=");
    let html = send(
        &t.app,
        form_post(
            "/study/review",
            Some(&cookie),
            &format!("session={session}&card={card}&rating=3&total=1"),
        ),
    )
    .await
    .text();
    assert!(html.contains("data-done-goal"), "{html}");
    assert!(html.contains("On track for"), "{html}");
    assert!(html.contains("1 of 1 card learned in Anatomy."), "{html}");
    assert!(html.contains("20 days left."), "{html}");
}

#[tokio::test]
async fn a_boost_returns_to_the_filtered_queue() {
    let t = AppBuilder::new("study-page-tags-boost").build();
    let (cookie, pharm) = tagged(&t);

    let reply = send(
        &t.app,
        form_post(
            "/study/boost",
            Some(&cookie),
            "extra=5&back=study&tags=cardio",
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.location(), "/study?tags=cardio");

    let reply = send(
        &t.app,
        form_post(
            &format!("/decks/{pharm}/boost"),
            Some(&cookie),
            "extra=5&back=study&tags=cardio,nclex",
        ),
    )
    .await;
    assert_eq!(
        reply.location(),
        format!("/study?deck={pharm}&tags=cardio%2Cnclex")
    );

    // Without a filter the link is what it always was.
    let reply = send(
        &t.app,
        form_post("/study/boost", Some(&cookie), "extra=5&back=study"),
    )
    .await;
    assert_eq!(reply.location(), "/study");
}

/// What a web study sitting records: loading the page again before any
/// grade is the same session, not another one nobody finished; a grade
/// says how long the card was on screen and counts on its session,
/// which knows where it was started; ending it twice keeps the first
/// end.
#[tokio::test]
async fn a_web_session_records_where_it_started_its_length_and_answer_times() {
    let t = AppBuilder::new("study-page-measures").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Anatomy",
            &[("Chambers of the heart?".into(), "Four".into(), vec![])],
            now_ms(),
        )
        .unwrap();

    let first = send(&t.app, get("/study", Some(&cookie))).await.text();
    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    let session = number_after(&html, "\"session\":");
    assert_eq!(session, number_after(&first, "\"session\":"), "one sitting");
    let shown = number_after(&html, "\"shown\":");
    assert!(shown > 0, "{html}");

    let card = number_after(&html, "data-card-id=");
    let shown_earlier = shown - 4_000;
    send(
        &t.app,
        form_post(
            "/study/review",
            Some(&cookie),
            &format!("session={session}&card={card}&rating=3&total=1&shown={shown_earlier}"),
        ),
    )
    .await;
    let (answer_ms, surface, reviews, last): (i64, String, i64, Option<i64>) = t
        .store
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT r.answer_ms, s.surface, s.reviews, s.last_review_at
                 FROM review_log r JOIN study_sessions s ON s.id = r.session_id
                 WHERE r.user_id = ?1",
                [user.raw()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?)
        })
        .unwrap();
    assert!((4_000..60_000).contains(&answer_ms), "{answer_ms}");
    assert_eq!(surface, "web");
    assert_eq!(reviews, 1);
    assert!(last.is_some());

    let id = flash_core::SessionId(session);
    t.services.end_session(user, id, 1_000).unwrap();
    t.services.end_session(user, id, 2_000).unwrap();
    let ended: i64 = t
        .store
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT ended_at FROM study_sessions WHERE id = ?1",
                [session],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(ended, 1_000);
}

/// After Show answer each grade says when the card comes back, short, and
/// the buttons no longer print their keys.
#[tokio::test]
async fn the_grades_say_when_the_card_comes_back() {
    let t = AppBuilder::new("study-page-grade-times").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    t.services
        .create_cards(
            user,
            "Pharm",
            &[("Metoprolol class?".into(), "Beta-1 blocker".into(), vec![])],
            now_ms(),
        )
        .unwrap();
    let html = send(&t.app, get("/study", Some(&cookie))).await.text();
    let session = number_after(&html, "\"session\":");
    let card = number_after(&html, "data-card-id=");

    let revealed = send(
        &t.app,
        form_post(
            "/study/reveal",
            Some(&cookie),
            &format!("session={session}&card={card}&remaining=1&total=1"),
        ),
    )
    .await
    .text();
    assert!(revealed.contains("grade-row has-when"), "{revealed}");
    assert_eq!(
        revealed.matches(r#"<span class="grade-when">"#).count(),
        4,
        "{revealed}"
    );
    // A new card: minutes for Again, days for Easy.
    assert!(
        revealed.contains(r#"<span>Again</span><span class="grade-when">"#),
        "{revealed}"
    );
    let easy = revealed
        .split(r#"<span>Easy</span><span class="grade-when">"#)
        .nth(1)
        .and_then(|rest| rest.split('<').next())
        .unwrap_or_default();
    assert!(easy.ends_with('d'), "Easy reads in days: {easy:?}");
    assert!(!revealed.contains("<kbd>1</kbd>"), "{revealed}");
}

/// A side's language on the study page comes from a real label: the
/// side's speech blocks (one language names the side, several mark the
/// runs they spell) or nothing, which says unknown (`lang=""`) rather
/// than leave the page's own language to claim it.
#[tokio::test]
async fn a_cards_language_comes_from_its_labels_only() {
    use flash_core::speech::SpeechBlock;
    use flash_store::notes::{generate_cards, NoteType};
    use flash_store::richtext::sanitize_with_media;
    use flash_store::{NoteSave, NoteSpeech};
    let t = AppBuilder::new("study-page-langs").build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let say = |lang: &str, text: &str| SpeechBlock::Say {
        lang: lang.into(),
        text: text.into(),
    };
    let note = |deck: &str, front: &str, back: &str, speech: NoteSpeech| {
        let deck = t.store.create_deck(user, deck, "", now_ms()).unwrap();
        let (front, back) = (sanitize_with_media(front), sanitize_with_media(back));
        let generated = generate_cards(NoteType::Basic, &front, &back).unwrap();
        t.store
            .create_note(
                user,
                deck,
                &NoteSave {
                    note_type: NoteType::Basic,
                    front_html: &front,
                    back_html: &back,
                    generated: &generated,
                    tags: &[],
                    cap: None,
                    origin: None,
                    speech,
                    langs: None,
                    now_ms: now_ms(),
                },
            )
            .unwrap();
        deck
    };
    let japanese = note(
        "Kanji",
        "<p>猫</p>",
        "<p>neko (cat)</p>",
        NoteSpeech {
            front: Some(vec![say("ja", "ねこ")]),
            back: Some(vec![say("ja", "neko"), say("en", "cat")]),
        },
    );
    let unlabeled = note("Plain", "<p>chien</p>", "<p>dog</p>", NoteSpeech::default());

    let page = |deck: flash_core::DeckId| {
        let app = t.app.clone();
        let cookie = cookie.clone();
        async move {
            send(&app, get(&format!("/study?deck={}", deck.0), Some(&cookie)))
                .await
                .text()
        }
    };
    let html = page(japanese).await;
    assert!(
        html.contains(r#"<div class="front" dir="auto" lang="ja">"#),
        "{html}"
    );
    let html = page(unlabeled).await;
    assert!(
        html.contains(r#"<div class="front" dir="auto" lang="">"#),
        "{html}"
    );

    let card = t
        .store
        .list_cards(user, Some(japanese), None, 1)
        .unwrap()
        .remove(0);
    let reveal = t.services.reveal_card(user, card.id, None).unwrap();
    assert_eq!(reveal.back_lang, "");
    assert_eq!(
        reveal.back_html.as_deref(),
        Some(r#"<p><span lang="ja">neko</span> (<span lang="en">cat</span>)</p>"#)
    );
}
