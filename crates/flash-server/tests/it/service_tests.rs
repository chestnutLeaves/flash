//! End-to-end service-layer tests over an in-memory store: the exact code
//! path the web UI and MCP tools share.

use std::sync::Arc;

use flash_core::queue::StudyScope;
use flash_core::Rating;
use flash_store::Store;

use flash_server::service::{ReviewOrigin, Services};
use flash_server::testing::heavy_permit;

const NOW: i64 = 1_700_000_000_000;

/// Services the way the open server builds them: no card cap.
fn services(store: Arc<Store>) -> Services {
    Services::new(store)
}

fn setup() -> (Services, flash_core::UserId) {
    let store = Store::open_in_memory().unwrap();
    let user = store.create_user("Test", None, "member", NOW).unwrap();
    (services(Arc::new(store)), user)
}

/// An archived deck leaves today's numbers, the dashboard's deck list and
/// every study scope, and unarchiving brings the same cards back.
#[test]
fn an_archived_deck_leaves_today_and_comes_back_whole() {
    let (services, user) = setup();
    let ten = |prefix: &str| -> Vec<(String, String, Vec<String>)> {
        (0..10)
            .map(|i| (format!("{prefix} {i}"), "b".to_string(), vec![]))
            .collect()
    };
    services.create_cards(user, "Old", &ten("o"), NOW).unwrap();
    services.create_cards(user, "Live", &ten("l"), NOW).unwrap();
    let old = services
        .store()
        .find_deck_by_name(user, "Old")
        .unwrap()
        .unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .new_available,
        20
    );

    services.archive_deck(user, old, NOW).unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .new_available,
        10
    );
    let dashboard = services.dashboard(user, NOW).unwrap();
    assert_eq!(dashboard.counts.new_available, 10);
    assert_eq!(dashboard.decks.len(), 1, "the archived deck is not today's");
    assert_eq!(dashboard.decks[0].name, "Live");
    let listed = services.list_decks(user, NOW).unwrap();
    assert_eq!(listed.len(), 2, "but it is still listed, last");
    assert_eq!(listed[1].name, "Old");
    assert_eq!(listed[1].archived_at, Some(NOW));
    assert_eq!((listed[1].due_count, listed[1].new_count), (0, 0));
    assert_eq!(
        services
            .start_session(user, StudyScope::deck(old), NOW)
            .unwrap()
            .cards_due,
        0,
        "studying it by name finds nothing"
    );
    assert_eq!(services.deck_detail(user, old, NOW).unwrap().card_count, 10);

    services.unarchive_deck(user, old).unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .new_available,
        20
    );
    assert_eq!(services.dashboard(user, NOW).unwrap().decks.len(), 2);
}

#[test]
fn full_study_session_flow() {
    let (services, user) = setup();
    services
        .create_cards(
            user,
            "Pharm",
            &[
                ("Warfarin antidote?".into(), "Vitamin K".into(), vec![]),
                (
                    "Heparin antidote?".into(),
                    "Protamine sulfate".into(),
                    vec![],
                ),
            ],
            NOW,
        )
        .unwrap();

    let session = services
        .start_session(user, StudyScope::all(), NOW)
        .unwrap();
    assert_eq!(session.cards_due, 2);
    let first = session.first_card.clone().unwrap();
    assert_eq!(first.front, "Warfarin antidote?");
    assert!(
        session.next_card_id.is_some(),
        "second card is announced for warm-up"
    );
    assert_ne!(session.next_card_id, Some(first.card_id));
    assert_eq!(services.reveal(user, first.card_id).unwrap(), "Vitamin K");

    // Good on card 1 -> card 2 comes next.
    let result = services
        .submit_review(
            user,
            session.session_id,
            first.card_id,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            NOW,
        )
        .unwrap();
    let second = result.next_card.unwrap();
    assert_eq!(second.front, "Heparin antidote?");
    assert_eq!(
        result.next_card_id, None,
        "nothing queued after the last card"
    );

    // Again on card 2 -> it's in a 10-minute learning step; nothing else is
    // due right now, and the queue hands it back (it's all that's left).
    let result = services
        .submit_review(
            user,
            session.session_id,
            second.card_id,
            Rating::Again,
            ReviewOrigin::new("mcp", Some("Claude")),
            NOW,
        )
        .unwrap();
    assert_eq!(result.next_card.map(|c| c.card_id), Some(second.card_id));

    // Ten minutes later the learning step has expired; review it Good.
    let later = NOW + 10 * 60_000;
    let result = services
        .submit_review(
            user,
            session.session_id,
            second.card_id,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            later,
        )
        .unwrap();
    assert!(result.next_card.is_none(), "queue is empty");
    assert_eq!(result.remaining, 0);

    let stats = services
        .end_session(user, session.session_id, later)
        .unwrap();
    assert_eq!((stats.reviewed, stats.good, stats.again), (3, 2, 1));
}

#[test]
fn new_card_budget_limits_queue() {
    let (services, user) = setup();
    let cards: Vec<(String, String, Vec<String>)> = (0..30)
        .map(|i| (format!("front {i}"), format!("back {i}"), vec![]))
        .collect();
    services.create_cards(user, "Big", &cards, NOW).unwrap();

    // Default budget is 20 new/day.
    let session = services
        .start_session(user, StudyScope::all(), NOW)
        .unwrap();
    assert_eq!(session.cards_due, 20);
}

/// A tag scope is the cards carrying every tag, and it narrows a deck
/// rather than replacing it. Both counting and starting go the same way,
/// so the number on the page is the session the user gets.
#[test]
fn a_deck_and_its_tags_narrow_each_other() {
    let (services, user) = setup();
    services
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
            NOW,
        )
        .unwrap();
    services
        .create_cards(
            user,
            "Anatomy",
            &[("a1".into(), "b".into(), vec!["cardio".into()])],
            NOW,
        )
        .unwrap();
    let pharm = services
        .store()
        .find_deck_by_name(user, "Pharm")
        .unwrap()
        .unwrap();
    let due = |scope: StudyScope| {
        let counted = services
            .queue_counts(user, &scope, NOW)
            .unwrap()
            .new_available;
        let started = services.start_session(user, scope, NOW).unwrap().cards_due;
        assert_eq!(counted, started, "the count and the session agree");
        started
    };

    assert_eq!(due(StudyScope::tagged(["cardio"])), 2);
    assert_eq!(due(StudyScope::tagged(["nclex", "cardio"])), 1);
    assert_eq!(due(StudyScope::new(Some(pharm), ["cardio"])), 1);
    assert_eq!(due(StudyScope::new(Some(pharm), ["bones"])), 0);
}

/// The cap is enforced where every surface passes, so no route can slip a
/// hundred tags into one query.
#[test]
fn a_scope_naming_more_tags_than_the_cap_is_refused() {
    let (services, user) = setup();
    services
        .create_cards(user, "Pharm", &[("f".into(), "b".into(), vec![])], NOW)
        .unwrap();
    let many: Vec<String> = (0..=flash_core::queue::MAX_SCOPE_TAGS)
        .map(|i| format!("t{i}"))
        .collect();
    let scope = StudyScope::tagged(&many);
    assert!(services.queue_counts(user, &scope, NOW).is_err());
    let err = services.start_session(user, scope, NOW).unwrap_err();
    assert!(format!("{err:?}").contains("tags"), "{err:?}");
}

#[test]
fn per_deck_limits_and_boost() {
    let (services, user) = setup();
    let cards: Vec<(String, String, Vec<String>)> = (0..10)
        .map(|i| (format!("q{i}"), "a".into(), vec![]))
        .collect();
    services.create_cards(user, "A", &cards, NOW).unwrap();
    services.create_cards(user, "B", &cards, NOW).unwrap();
    let decks = services.list_decks(user, NOW).unwrap();
    let a = decks.iter().find(|d| d.name == "A").unwrap().id;
    let b = decks.iter().find(|d| d.name == "B").unwrap().id;
    let new_for = |scope: StudyScope, at: i64| {
        services
            .queue_counts(user, &scope, at)
            .unwrap()
            .new_available
    };

    // Deck A overrides to 3 new/day; B inherits the account default (20).
    services.set_deck_limits(user, a, Some(3), None).unwrap();
    assert_eq!(new_for(StudyScope::deck(a), NOW), 3);
    assert_eq!(new_for(StudyScope::deck(b), NOW), 10);
    // All: A contributes 3, B 10 = 13, under the account total of 20.
    assert_eq!(new_for(StudyScope::all(), NOW), 13);
    assert_eq!(
        services
            .start_session(user, StudyScope::all(), NOW)
            .unwrap()
            .cards_due,
        13
    );
    // Deck summaries show the budgeted count, not the raw 10.
    let decks = services.list_decks(user, NOW).unwrap();
    assert_eq!(decks.iter().find(|d| d.id == a).unwrap().new_count, 3);

    // Studying B consumes B's and the account's allowance, never A's.
    let session = services
        .start_session(user, StudyScope::deck(b), NOW)
        .unwrap();
    let first = session.first_card.unwrap();
    services
        .submit_review(
            user,
            session.session_id,
            first.card_id,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            NOW,
        )
        .unwrap();
    assert_eq!(new_for(StudyScope::deck(a), NOW + 1), 3);
    assert_eq!(new_for(StudyScope::deck(b), NOW + 1), 9);
    assert_eq!(new_for(StudyScope::all(), NOW + 1), 12);

    // Boost A by 2 for today: 5 available now, back to 3 tomorrow.
    assert_eq!(services.boost_new_today(user, Some(a), 2, NOW).unwrap(), 2);
    assert_eq!(services.boost_today(user, Some(a), NOW).unwrap(), 2);
    assert_eq!(new_for(StudyScope::deck(a), NOW + 2), 5);
    let tomorrow = NOW + 36 * 3_600_000;
    assert_eq!(new_for(StudyScope::deck(a), tomorrow), 3);
    assert_eq!(services.boost_today(user, Some(a), tomorrow).unwrap(), 0);

    // Account default lowered to 4: B (inheriting) follows, A keeps its 3.
    services.set_daily_limits(user, 4, 200).unwrap();
    assert_eq!(new_for(StudyScope::deck(b), tomorrow), 4);
    assert_eq!(new_for(StudyScope::deck(a), tomorrow), 3);
    // An account-wide boost raises the inherited default for B only.
    services.boost_new_today(user, None, 3, tomorrow).unwrap();
    assert_eq!(new_for(StudyScope::deck(b), tomorrow), 7);
    assert_eq!(new_for(StudyScope::deck(a), tomorrow), 3);
    // Clearing A's override returns it to the default.
    services.set_deck_limits(user, a, None, None).unwrap();
    assert_eq!(new_for(StudyScope::deck(a), tomorrow), 7);

    assert!(services.set_daily_limits(user, 10_000, 200).is_err());
    assert!(services.boost_new_today(user, None, 0, NOW).is_err());
}

/// What one grade and one dashboard view cost on an account holding
/// 30,000 new cards, the size of a large imported deck, beside a small
/// deck of 200: medians of 21. Grades again once the small deck has a
/// goal, and once the big one has one too.
/// Run with `cargo nextest run -p flash-server --test it --run-ignored
/// only --no-capture service_tests::time_a_grade`.
#[test]
#[ignore = "a timing report, not a check"]
fn time_a_grade_over_30000_new_cards() {
    let (services, user) = setup();
    let store = services.store();
    let deck = store.create_deck(user, "Big", "", NOW).unwrap();
    let exam = store.create_deck(user, "Exam", "", NOW).unwrap();
    let back = "An answer about as long as an imported card's. ".repeat(5);
    let card = |front: String| {
        (
            flash_core::validate_card_text(&front, &back).unwrap(),
            vec![],
        )
    };
    for chunk in 0..30 {
        let cards: Vec<_> = (0..1_000)
            .map(|i| card(format!("Question {chunk}-{i}: what does this card ask?")))
            .collect();
        store.create_cards(user, deck, &cards, None, NOW).unwrap();
    }
    let cards: Vec<_> = (0..200).map(|i| card(format!("Exam {i}?"))).collect();
    store.create_cards(user, exam, &cards, None, NOW).unwrap();
    services.set_daily_limits(user, 50, 200).unwrap();
    let median = |mut ms: Vec<f64>| {
        ms.sort_by(f64::total_cmp);
        ms[ms.len() / 2]
    };
    let time = |f: &mut dyn FnMut()| {
        let start = std::time::Instant::now();
        f();
        start.elapsed().as_secs_f64() * 1000.0
    };
    let grades = |at: i64| {
        let session = services.start_session(user, StudyScope::all(), at).unwrap();
        let mut card = session.first_card.unwrap().card_id;
        let ms: Vec<f64> = (0..21)
            .map(|i| {
                time(&mut || {
                    let result = services
                        .submit_review(
                            user,
                            session.session_id,
                            card,
                            Rating::Good,
                            ReviewOrigin::WEB,
                            at + i * 1_000,
                        )
                        .unwrap();
                    card = result.next_card.unwrap().card_id;
                })
            })
            .collect();
        median(ms)
    };
    let date = |text: &str| flash_core::goal::parse_date(text).unwrap();

    let grade = grades(NOW);
    let later = NOW + 30_000;
    let count = median(
        (0..21)
            .map(|_| {
                time(&mut || {
                    services
                        .queue_counts(user, &StudyScope::all(), later)
                        .unwrap();
                })
            })
            .collect(),
    );
    let dashboard = median(
        (0..21)
            .map(|_| {
                time(&mut || {
                    services.dashboard(user, later).unwrap();
                })
            })
            .collect(),
    );
    services
        .set_deck_goal(user, exam, Some(date("2023-12-15")), later)
        .unwrap();
    let small_goal = grades(NOW + 60_000);
    services
        .set_deck_goal(user, deck, Some(date("2024-06-01")), later)
        .unwrap();
    let big_goal = grades(NOW + 90_000);
    println!(
        "30,000 new cards: a grade {grade:.2} ms, a dashboard count {count:.2} ms, \
         the dashboard {dashboard:.2} ms; a grade with a goal on the small deck \
         {small_goal:.2} ms, with one on the big deck too {big_goal:.2} ms (medians of 21)",
    );
}

#[test]
fn rename_deck_trims_and_rejects_duplicates() {
    let (services, user) = setup();
    let a = services.create_deck(user, "A", "", NOW).unwrap();
    let b = services.create_deck(user, "B", "", NOW).unwrap();
    services.rename_deck(user, a, "  Alpha ").unwrap();
    let names: Vec<String> = services
        .list_decks(user, NOW)
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert!(names.contains(&"Alpha".to_string()));
    assert!(!names.contains(&"A".to_string()));
    // Renaming to your own current name is a no-op, not a conflict.
    services.rename_deck(user, a, "Alpha").unwrap();
    assert!(matches!(
        services.rename_deck(user, b, "Alpha"),
        Err(ServiceError::Store(flash_store::StoreError::Invalid(_)))
    ));
    assert!(matches!(
        services.rename_deck(user, b, "   "),
        Err(ServiceError::Store(flash_store::StoreError::Invalid(_)))
    ));
    assert!(matches!(
        services.rename_deck(user, flash_core::DeckId(999), "Z"),
        Err(ServiceError::Store(flash_store::StoreError::NotFound(_)))
    ));
}

#[test]
fn review_cap_limits_due_reviews_not_learning() {
    let (services, store, user) = setup_with_store();
    seed_cards(&store, user, 5);
    store
        .raw_execute_for_tests("UPDATE card_state SET phase = 2, due = 1")
        .unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .due,
        5
    );
    services.set_daily_limits(user, 20, 2).unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .due,
        2
    );
    assert_eq!(
        services
            .start_session(user, StudyScope::all(), NOW)
            .unwrap()
            .cards_due,
        2
    );
    // Learning cards are never capped.
    store
        .raw_execute_for_tests(
            "UPDATE card_state SET phase = 1 WHERE card_id IN (SELECT id FROM cards ORDER BY id LIMIT 3)",
        )
        .unwrap();
    assert_eq!(
        services
            .queue_counts(user, &StudyScope::all(), NOW)
            .unwrap()
            .due,
        5
    );
}

const HOUR: i64 = 60 * 60 * 1000;

/// NOW is 22:13 UTC, and a new account's study day ends at 04:00 UTC:
/// the first three of five review cards come due before then (01:13),
/// the other two in tomorrow's study day (10:13).
fn three_later_today_two_tomorrow(store: &Store, user: flash_core::UserId) {
    seed_cards(store, user, 5);
    store
        .raw_execute_for_tests(&format!(
            "UPDATE card_state SET phase = 2, due = CASE
               WHEN card_id IN (SELECT id FROM cards ORDER BY id LIMIT 3) THEN {}
               ELSE {} END",
            NOW + 3 * HOUR,
            NOW + 12 * HOUR
        ))
        .unwrap();
}

#[test]
fn by_default_a_review_is_due_for_its_whole_study_day() {
    let (services, store, user) = setup_with_store();
    three_later_today_two_tomorrow(&store, user);
    let all = StudyScope::all();
    assert_eq!(services.queue_counts(user, &all, NOW).unwrap().due, 3);
    let decks = services.list_decks(user, NOW).unwrap();
    assert_eq!(
        decks[0].due_count, 3,
        "the deck list counts what a session serves"
    );
    let session = services.start_session(user, all.clone(), NOW).unwrap();
    assert_eq!(session.cards_due, 3);

    // Once today's three are done, the next cards are tomorrow's two.
    store
        .raw_execute_for_tests(&format!(
            "UPDATE card_state SET due = {} WHERE due = {}",
            NOW + 30 * DAY,
            NOW + 3 * HOUR
        ))
        .unwrap();
    let next = services.next_due(user, &all, 0, NOW).unwrap().unwrap();
    assert_eq!(
        (next.at_ms, next.in_days, next.cards),
        (NOW + 12 * HOUR, 1, 2)
    );
}

#[test]
fn with_exact_times_a_review_waits_for_its_hour() {
    let (services, store, user) = setup_with_store();
    three_later_today_two_tomorrow(&store, user);
    services
        .set_study_settings(
            user,
            &flash_server::service::StudySettingsPatch {
                due_by_day: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!services.settings_overview(user, NOW).unwrap().due_by_day);
    let all = StudyScope::all();
    assert_eq!(services.queue_counts(user, &all, NOW).unwrap().due, 0);
    assert_eq!(services.list_decks(user, NOW).unwrap()[0].due_count, 0);
    let next = services.next_due(user, &all, 0, NOW).unwrap().unwrap();
    assert_eq!(
        (next.at_ms, next.in_days, next.cards),
        (NOW + 3 * HOUR, 0, 3),
        "later today, and only today's"
    );
    // At its hour the card is due.
    assert_eq!(
        services
            .queue_counts(user, &all, NOW + 3 * HOUR)
            .unwrap()
            .due,
        3
    );
}

#[test]
fn new_cards_held_by_the_limit_come_tomorrow() {
    let (services, store, user) = setup_with_store();
    seed_cards(&store, user, 30);
    services.set_daily_limits(user, 0, 200).unwrap();
    let all = StudyScope::all();
    let session = services.start_session(user, all.clone(), NOW).unwrap();
    assert_eq!((session.cards_due, session.held_by_limit), (0, 30));
    let next = services
        .next_due(user, &all, session.held_by_limit, NOW)
        .unwrap()
        .unwrap();
    // Tomorrow's study day starts at 04:00 UTC, and brings as many new
    // cards as a day allows.
    assert_eq!(next.in_days, 1);
    assert!(next.at_ms > NOW && next.at_ms - NOW < DAY);
    assert_eq!(next.at_ms % DAY, 4 * HOUR);
    assert_eq!(next.cards, 0, "a daily limit of 0 brings none");
    services.set_daily_limits(user, 20, 200).unwrap();
    let next = services.next_due(user, &all, 30, NOW).unwrap().unwrap();
    assert_eq!(next.cards, 20);
    // Nothing seen and nothing held: nothing is coming.
    assert_eq!(services.next_due(user, &all, 0, NOW).unwrap(), None);
}

#[test]
fn held_cards_come_at_the_decks_own_limit() {
    let (services, store, user) = setup_with_store();
    seed_cards(&store, user, 30);
    let deck = store.find_deck_by_name(user, "Seed").unwrap().unwrap();
    services.set_deck_limits(user, deck, Some(5), None).unwrap();
    let scope = StudyScope::deck(deck);
    let today = services.start_session(user, scope.clone(), NOW).unwrap();
    assert_eq!((today.cards_due, today.held_by_limit), (5, 25));

    let promised = services
        .next_due(user, &scope, today.held_by_limit, NOW)
        .unwrap()
        .unwrap();
    let week = services
        .week_ahead(user, &scope, today.held_by_limit, NOW)
        .unwrap();
    let tomorrow = services.start_session(user, scope, NOW + DAY).unwrap();
    assert_eq!(tomorrow.cards_due, 5, "tomorrow serves the deck's 5");
    assert_eq!(promised.cards, tomorrow.cards_due, "due tomorrow");
    assert_eq!(week[0].cards, tomorrow.cards_due, "week ahead, tomorrow");
}

/// A session over a parent draws from each deck under it on that deck's
/// own budget, within the account's; so does what it says comes next.
#[test]
fn held_cards_under_a_parent_come_at_each_subdecks_limit() {
    let (services, user) = setup();
    let fill = |deck: &str, n: usize| {
        let rows: Vec<(String, String, Vec<String>)> = (0..n)
            .map(|i| (format!("{deck} {i}"), "b".into(), vec![]))
            .collect();
        services.create_cards(user, deck, &rows, NOW).unwrap();
        services
            .store()
            .find_deck_by_name(user, deck)
            .unwrap()
            .unwrap()
    };
    // "Languages" set to 5 a day, "Languages::Spanish" left at the
    // account's 20; "Bio" holds no cards, its two subdecks 5 a day each.
    fill("Languages::Spanish", 50);
    let languages = fill("Languages", 0);
    services
        .set_deck_limits(user, languages, Some(5), None)
        .unwrap();
    let bio = fill("Bio", 0);
    for part in ["Bio::Cells", "Bio::Genes"] {
        let deck = fill(part, 30);
        services.set_deck_limits(user, deck, Some(5), None).unwrap();
    }
    for (deck, serves) in [(languages, 20), (bio, 10)] {
        let scope = StudyScope::deck(deck);
        let today = services.start_session(user, scope.clone(), NOW).unwrap();
        assert_eq!(today.cards_due, serves);
        let promised = services
            .next_due(user, &scope, today.held_by_limit, NOW)
            .unwrap()
            .unwrap();
        let week = services
            .week_ahead(user, &scope, today.held_by_limit, NOW)
            .unwrap();
        let tomorrow = services.start_session(user, scope, NOW + DAY).unwrap();
        assert_eq!(tomorrow.cards_due, serves);
        assert_eq!(promised.cards, serves, "due tomorrow");
        assert_eq!(week[0].cards, serves, "week ahead, tomorrow");
    }
}

#[test]
fn deck_scoping_and_deck_autocreation() {
    let (services, user) = setup();
    services
        .create_cards(user, "Pharm", &[("a".into(), "b".into(), vec![])], NOW)
        .unwrap();
    services
        .create_cards(user, "Anatomy", &[("c".into(), "d".into(), vec![])], NOW)
        .unwrap();

    let decks = services.list_decks(user, NOW).unwrap();
    assert_eq!(decks.len(), 2, "decks auto-created by name");

    let pharm = decks.iter().find(|d| d.name == "Pharm").unwrap();
    let session = services
        .start_session(user, StudyScope::deck(pharm.id), NOW)
        .unwrap();
    assert_eq!(session.cards_due, 1);
    assert_eq!(session.first_card.unwrap().front, "a");
}

#[test]
fn duplicate_submission_within_retry_window_is_idempotent() {
    let (services, user) = setup();
    services
        .create_cards(user, "D", &[("f".into(), "b".into(), vec![])], NOW)
        .unwrap();
    let session = services
        .start_session(user, StudyScope::all(), NOW)
        .unwrap();
    let card = session.first_card.unwrap().card_id;

    services
        .submit_review(
            user,
            session.session_id,
            card,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            NOW,
        )
        .unwrap();
    // A transport retry lands 2 seconds later: must not double-record.
    services
        .submit_review(
            user,
            session.session_id,
            card,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            NOW + 2_000,
        )
        .unwrap();
    let stats = services
        .store()
        .session_stats(user, session.session_id)
        .unwrap();
    assert_eq!(stats.reviewed, 1, "retry must not create a second review");

    // A genuine re-review outside the window still records.
    let later = NOW + 11 * 60_000;
    services
        .submit_review(
            user,
            session.session_id,
            card,
            Rating::Good,
            ReviewOrigin::new("mcp", Some("Claude")),
            later,
        )
        .unwrap();
    assert_eq!(
        services
            .store()
            .session_stats(user, session.session_id)
            .unwrap()
            .reviewed,
        2
    );
}

#[test]
fn empty_and_invalid_input_handling() {
    let (services, user) = setup();
    // Empty queue -> clean empty session, not an error.
    let session = services
        .start_session(user, StudyScope::all(), NOW)
        .unwrap();
    assert_eq!(session.cards_due, 0);
    assert!(session.first_card.is_none());

    // Invalid card text is rejected before anything is written, including
    // the would-be auto-created deck.
    let err = services
        .create_cards(user, "D", &[("  ".into(), "b".into(), vec![])], NOW)
        .unwrap_err();
    assert!(err.to_string().contains("front is empty"));
    assert!(services.list_decks(user, NOW).unwrap().is_empty());
}

const DAY: i64 = 24 * 60 * 60 * 1000;

/// Setup that also hands back the store for direct review seeding.
fn setup_with_store() -> (Services, Arc<Store>, flash_core::UserId) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let user = store.create_user("Test", None, "member", NOW).unwrap();
    (services(store.clone()), store, user)
}

fn seed_review(store: &Store, user: flash_core::UserId, card: flash_core::CardId, ts: i64) {
    let scheduler = flash_core::Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(user, card).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, ts).unwrap();
    store
        .record_review(user, card, &outcome, ts, "web", None, None)
        .unwrap();
}

/// The days between grades are the account's study days: in Tokyo, 21:00
/// to 07:00 the next morning is a day on, though in UTC it is one day.
#[test]
fn grades_count_days_on_the_accounts_calendar() {
    let (services, store, user) = setup_with_store();
    let card = services
        .create_cards(user, "D", &[("q".into(), "a".into(), vec![])], NOW)
        .unwrap()[0];
    store
        .set_study_settings(user, Some("Asia/Tokyo"), None, None, None)
        .unwrap();
    let tokyo = |day: i8, hour: i8| {
        jiff::civil::date(2026, 10, day)
            .at(hour, 0, 0, 0)
            .in_tz("Asia/Tokyo")
            .unwrap()
            .timestamp()
            .as_millisecond()
    };
    let monday_9pm = tokyo(12, 21);
    let graduated = flash_core::Scheduler::new(None, 0.9)
        .unwrap()
        .review(
            &store.get_card_state(user, card).unwrap(),
            Rating::Good,
            monday_9pm,
        )
        .unwrap()
        .state;
    store.set_card_states(user, &[(card, graduated)]).unwrap();
    let good_in = |at| services.grade_preview(user, card, at).unwrap()[2];
    assert_eq!(good_in(tokyo(13, 7)), good_in(tokyo(13, 21)));
}

#[test]
fn stats_day_cutoff_merges_late_night_reviews() {
    let (services, store, user) = setup_with_store();
    let ids = store
        .create_cards(
            user,
            store.create_deck(user, "D", "", NOW).unwrap(),
            &[(flash_core::validate_card_text("f", "b").unwrap(), vec![])],
            None,
            NOW,
        )
        .unwrap();
    // The cutoff is evaluated in the user's own zone, so pin one: in
    // America/New_York NOW is ~17:13, and nine hours later is ~2:13am the
    // next civil day, still before the 4am cutoff. Both land on the same
    // study day.
    store
        .set_study_settings(user, Some("America/New_York"), None, None, None)
        .unwrap();
    seed_review(&store, user, ids[0], NOW);
    seed_review(&store, user, ids[0], NOW + 9 * 60 * 60 * 1000);

    let stats = services
        .stats(&heavy_permit(user), NOW + 9 * 60 * 60 * 1000, None)
        .unwrap();
    assert_eq!(stats.streak, 1, "2am review must not start a second day");
    assert_eq!(stats.days_learned_pct, 100);
}

#[test]
fn stats_streaks_and_heatmap_shape() {
    let (services, store, user) = setup_with_store();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[(flash_core::validate_card_text("f", "b").unwrap(), vec![])],
            None,
            NOW,
        )
        .unwrap();
    // Current run: today, -1d, -2d. Older run: -5d..-8d (longest = 4).
    for offset in [0i64, 1, 2, 5, 6, 7, 8] {
        seed_review(&store, user, ids[0], NOW - offset * DAY);
    }

    let stats = services.stats(&heavy_permit(user), NOW, None).unwrap();
    assert_eq!(stats.streak, 3);
    assert_eq!(stats.longest_streak, 4);
    assert_eq!(stats.total_reviews, 7);
    assert!(
        stats.heat_years.is_empty(),
        "one year of history, no picker"
    );
    assert_eq!(stats.heat_year, None);
    // Denominator: 9 days since the first review.
    assert_eq!(stats.days_learned_pct, 7 * 100 / 9);

    assert_eq!(stats.heat_weeks.len(), 53);
    assert!(stats.heat_weeks.iter().all(|w| w.len() == 7));
    let cells: Vec<_> = stats.heat_weeks.iter().flatten().collect();
    let today_pos = cells.iter().position(|c| c.today).expect("today cell");
    assert_eq!(cells.iter().filter(|c| c.today).count(), 1);
    assert!(cells[..today_pos].iter().all(|c| !c.future));
    assert!(cells[today_pos + 1..].iter().all(|c| c.future));
    // Seven studied days show up as leveled past cells.
    assert_eq!(cells.iter().filter(|c| !c.future && c.level > 0).count(), 7);
    let month_cols: u32 = stats.heat_months.iter().map(|m| m.weeks).sum();
    assert_eq!(month_cols, 53);
}

#[test]
fn stats_future_due_lands_in_heatmap() {
    let (services, store, user) = setup_with_store();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[(flash_core::validate_card_text("f", "b").unwrap(), vec![])],
            None,
            NOW,
        )
        .unwrap();
    // One Good review schedules the card a few days out.
    seed_review(&store, user, ids[0], NOW);

    let stats = services.stats(&heavy_permit(user), NOW, None).unwrap();
    let future_due: u32 = stats
        .heat_weeks
        .iter()
        .flatten()
        .filter(|c| c.future)
        .map(|c| c.count)
        .sum();
    assert_eq!(future_due, 1, "the scheduled card appears as future load");
    assert_eq!(stats.active_cards, 1);
    assert_eq!(stats.mature_cards, 0, "stability is far below 21 days");
}

/// One card to review, for the stats tests that only need history.
fn one_card(store: &Store, user: flash_core::UserId) -> flash_core::CardId {
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    store
        .create_cards(
            user,
            deck,
            &[(flash_core::validate_card_text("f", "b").unwrap(), vec![])],
            None,
            NOW,
        )
        .unwrap()[0]
}

/// A local wall-clock moment in the zone the stats tests study in.
fn at(zone: &str, date: (i16, i8, i8), hour: i8, minute: i8) -> i64 {
    jiff::civil::date(date.0, date.1, date.2)
        .at(hour, minute, 0, 0)
        .in_tz(zone)
        .unwrap()
        .timestamp()
        .as_millisecond()
}

#[test]
fn stats_longest_streak_spans_more_than_a_year() {
    let (services, store, user) = setup_with_store();
    let card = one_card(&store, user);
    // A daily run of 400 days that ended 500 days ago, then today.
    for offset in (500..900).rev() {
        seed_review(&store, user, card, NOW - offset * DAY);
    }
    seed_review(&store, user, card, NOW);

    let stats = services.stats(&heavy_permit(user), NOW, None).unwrap();
    assert_eq!(stats.longest_streak, 400, "the record is all-time");
    assert_eq!(stats.streak, 1);
    assert_eq!(stats.total_reviews, 401);
    assert_eq!(stats.heat_years, vec![2021, 2022, 2023]);
    assert_eq!(stats.heat_year, None);
    // The legend still describes the past year: one studied day of 365.
    assert_eq!(stats.days_learned_pct, 0);
    assert_eq!(stats.daily_avg, 0);
}

#[test]
fn stats_calendar_year_view() {
    let (services, store, user) = setup_with_store();
    let card = one_card(&store, user);
    let march = at("America/New_York", (2022, 3, 15), 12, 0);
    seed_review(&store, user, card, march);
    seed_review(&store, user, card, NOW);

    let stats = services
        .stats(&heavy_permit(user), NOW, Some(2022))
        .unwrap();
    assert_eq!(stats.heat_year, Some(2022));
    assert_eq!(stats.heat_years, vec![2022, 2023]);
    // Jan 1 2022 was a Saturday and Dec 31 a Saturday: the Sunday before
    // through that Saturday, 53 columns.
    assert_eq!(stats.heat_weeks.len(), 53);
    let cells: Vec<_> = stats.heat_weeks.iter().flatten().collect();
    assert_eq!(cells[0].label, "12/26");
    assert_eq!(cells.last().unwrap().label, "12/31");
    assert!(cells.iter().all(|c| !c.future && !c.today));
    // The one studied day is the busiest of the year: full level. Today's
    // review lies outside the window and does not set the scale.
    let lit: Vec<_> = cells.iter().filter(|c| c.level > 0).collect();
    assert_eq!(lit.len(), 1);
    assert_eq!(
        (lit[0].label.as_str(), lit[0].level, lit[0].count),
        ("03/15", 4, 1)
    );
    let month_cols: u32 = stats.heat_months.iter().map(|m| m.weeks).sum();
    assert_eq!(month_cols, 53);
    assert_eq!(
        stats.heat_months[0].label, "",
        "one December column is blanked"
    );
    assert_eq!(stats.heat_months[1].label, "Jan");
    // The legend covers that year from its first review: Mar 15 to Dec 31.
    assert_eq!(stats.days_learned_pct, 100 / 292);
    // Streaks and totals stay all-time.
    assert_eq!(stats.total_reviews, 2);
    assert_eq!(stats.streak, 1);

    // A year whose Dec 31 is a Sunday needs a 54th column.
    let (services, store, user) = setup_with_store();
    let card = one_card(&store, user);
    let now = at("America/New_York", (2029, 6, 1), 12, 0);
    seed_review(
        &store,
        user,
        card,
        at("America/New_York", (2028, 7, 4), 12, 0),
    );
    seed_review(&store, user, card, now);
    let stats = services
        .stats(&heavy_permit(user), now, Some(2028))
        .unwrap();
    assert_eq!(stats.heat_weeks.len(), 54);
    let cells: Vec<_> = stats.heat_weeks.iter().flatten().collect();
    assert_eq!(cells[0].label, "12/26");
    assert_eq!(cells.last().unwrap().label, "01/06");
}

#[test]
fn stats_year_outside_history_is_the_trailing_view() {
    let (services, store, user) = setup_with_store();
    let card = one_card(&store, user);
    seed_review(&store, user, card, NOW - 2 * 365 * DAY);
    seed_review(&store, user, card, NOW);
    for year in [Some(1999), Some(2023), Some(2030), None] {
        let stats = services.stats(&heavy_permit(user), NOW, year).unwrap();
        assert_eq!(stats.heat_year, None, "{year:?}");
        assert_eq!(stats.heat_years, vec![2021, 2022, 2023]);
        assert_eq!(stats.heat_weeks.len(), 53);
        assert!(stats.heat_weeks.iter().flatten().any(|c| c.today));
    }
}

/// Quarter-hour buckets fold into study days on a zone whose offset is
/// not a whole hour: a review just before the 4am cutoff belongs to the
/// day before, one just after to the day itself.
#[test]
fn stats_buckets_fold_across_the_cutoff_in_kathmandu() {
    let (services, store, user) = setup_with_store();
    store
        .set_study_settings(user, Some("Asia/Kathmandu"), None, None, None)
        .unwrap();
    let card = one_card(&store, user);
    seed_review(
        &store,
        user,
        card,
        at("Asia/Kathmandu", (2023, 11, 14), 3, 59),
    );
    seed_review(
        &store,
        user,
        card,
        at("Asia/Kathmandu", (2023, 11, 14), 4, 1),
    );
    let now = at("Asia/Kathmandu", (2023, 11, 14), 5, 0);

    let stats = services.stats(&heavy_permit(user), now, None).unwrap();
    assert_eq!(stats.streak, 2, "two study days, both learned");
    assert_eq!(stats.days_learned_pct, 100);
    assert_eq!(stats.days[29].count, 1);
    assert_eq!(stats.days[28].count, 1);
}

use flash_server::service::ServiceError;
use flash_store::import::{HistoryEntry, ImportRow};

/// Seeds `n` cards directly at the store level into one deck.
fn seed_cards(store: &Store, user: flash_core::UserId, n: usize) -> Vec<flash_core::CardId> {
    let deck = store.create_deck(user, "Seed", "", NOW).unwrap();
    let cards: Vec<_> = (0..n)
        .map(|i| {
            (
                flash_core::validate_card_text(&format!("f{i}"), "b").unwrap(),
                Vec::new(),
            )
        })
        .collect();
    store.create_cards(user, deck, &cards, None, NOW).unwrap()
}

// ---- progress import (revlog replay) ----

/// A row as the Anki parser makes it: it knows which Anki card it was.
fn anki_row(front: &str, anki_card_id: i64) -> ImportRow {
    let mut row = import_row(front, "b");
    row.anki_card_id = Some(anki_card_id);
    row
}

fn review(at_ms: i64, anki_card_id: i64, rating: Rating) -> HistoryEntry {
    HistoryEntry {
        at_ms,
        anki_card_id,
        rating,
        answer_ms: None,
    }
}

/// One review_log row: (reviewed_at, rating, phase_before, source,
/// session_id, client).
type LoggedReview = (i64, i64, i64, String, Option<i64>, Option<String>);

/// Every review_log row of the user, oldest first.
fn logged(store: &Store, user: flash_core::UserId) -> Vec<LoggedReview> {
    store
        .with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT reviewed_at, rating, phase_before, source, session_id, client
                 FROM review_log WHERE user_id = ?1 ORDER BY reviewed_at",
            )?;
            let rows = stmt.query_map([user.raw()], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .unwrap()
}

#[test]
fn progress_import_writes_the_full_history() {
    let (services, store, user) = setup_with_store();
    let rows = vec![
        anki_row("seasoned", 1),
        anki_row("fresh", 2),
        anki_row("never", 3),
    ];
    let history = vec![
        review(NOW - 30 * DAY, 1, Rating::Good),
        review(NOW - 20 * DAY, 1, Rating::Good),
        review(NOW - 10 * DAY, 1, Rating::Hard),
        review(NOW - 5 * DAY, 2, Rating::Good),
    ];
    services
        .import_cards(user, "Anki", rows, history, None, NOW)
        .unwrap();

    // Every review is in the log, marked as imported, outside any session.
    let log = logged(&store, user);
    assert_eq!(log.len(), 4);
    assert_eq!(
        log.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![
            NOW - 30 * DAY,
            NOW - 20 * DAY,
            NOW - 10 * DAY,
            NOW - 5 * DAY
        ]
    );
    assert_eq!(log[2].1, Rating::Hard.as_i64());
    assert_eq!(log[0].2, flash_core::Phase::New.as_i64());
    assert_ne!(log[1].2, flash_core::Phase::New.as_i64());
    assert!(log
        .iter()
        .all(|r| r.3 == "import" && r.4.is_none() && r.5.is_none()));

    // The card's state is what replaying the same reviews gives.
    let cards = store.list_cards(user, None, Some("seasoned"), 10).unwrap();
    let state = store.get_card_state(user, cards[0].id).unwrap();
    let settings = store.get_settings(user).unwrap();
    let scheduler =
        flash_core::Scheduler::new(settings.fsrs_params.as_deref(), settings.desired_retention)
            .unwrap()
            .on_calendar(&settings.timezone, settings.day_cutoff_hour);
    let mut expected = flash_core::CardState::new_card(NOW - 30 * DAY);
    for (at, rating) in [
        (NOW - 30 * DAY, Rating::Good),
        (NOW - 20 * DAY, Rating::Good),
        (NOW - 10 * DAY, Rating::Hard),
    ] {
        expected = scheduler.review(&expected, rating, at).unwrap().state;
    }
    assert_eq!(state, expected);
    assert_eq!(state.reps, 3);
    assert_eq!(state.last_review_ms, Some(NOW - 10 * DAY));

    // The card without history stays new.
    let never = store.list_cards(user, None, Some("never"), 10).unwrap();
    let never_state = store.get_card_state(user, never[0].id).unwrap();
    assert_eq!(never_state.phase, flash_core::Phase::New);
}

#[test]
fn progress_import_twice_counts_once() {
    let (services, store, user) = setup_with_store();
    let history = vec![
        review(NOW - 3 * DAY, 1, Rating::Good),
        review(NOW - DAY, 1, Rating::Good),
    ];
    services
        .import_cards(
            user,
            "Anki",
            vec![anki_row("a", 1)],
            history.clone(),
            None,
            NOW,
        )
        .unwrap();
    // The same collection again, into another deck: no new log rows, but
    // the new copy of the card still gets its state.
    services
        .import_cards(
            user,
            "Anki again",
            vec![anki_row("a", 1)],
            history,
            None,
            NOW,
        )
        .unwrap();
    assert_eq!(logged(&store, user).len(), 2);
    let cards = store.list_cards(user, None, None, 10).unwrap();
    assert_eq!(cards.len(), 2);
    for card in &cards {
        assert_eq!(store.get_card_state(user, card.id).unwrap().reps, 2);
    }
}

#[test]
fn progress_import_skips_junk() {
    let (services, store, user) = setup_with_store();
    // Out of order in the file, one in the future, one before the epoch,
    // one for a card the package never made, one repeating a timestamp.
    let history = vec![
        review(NOW - 500, 1, Rating::Hard),
        review(NOW - 3000, 1, Rating::Good),
        review(NOW + 1000, 1, Rating::Good),
        review(-5, 1, Rating::Good),
        review(NOW - 2000, 99, Rating::Good),
        review(NOW - 3000, 1, Rating::Again),
    ];
    services
        .import_cards(user, "Anki", vec![anki_row("messy", 1)], history, None, NOW)
        .unwrap();
    let log = logged(&store, user);
    assert_eq!(
        log.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
        vec![
            (NOW - 3000, Rating::Good.as_i64()),
            (NOW - 500, Rating::Hard.as_i64())
        ]
    );
    let cards = store.list_cards(user, None, None, 10).unwrap();
    let state = store.get_card_state(user, cards[0].id).unwrap();
    assert_eq!(state.reps, 2);
    assert_eq!(state.last_review_ms, Some(NOW - 500));
}

#[test]
fn progress_import_opt_out_writes_nothing() {
    let (services, store, user) = setup_with_store();
    services
        .import_cards(
            user,
            "Anki",
            vec![anki_row("seasoned", 1)],
            vec![],
            None,
            NOW,
        )
        .unwrap();
    assert!(logged(&store, user).is_empty());
    let cards = store.list_cards(user, None, None, 10).unwrap();
    let state = store.get_card_state(user, cards[0].id).unwrap();
    assert_eq!(state.phase, flash_core::Phase::New);
    assert_eq!(state.reps, 0);
}

/// A review the uploader did in Anki today is a review done today: it
/// shows on the dashboard and spends today's budget, so nothing gets
/// studied twice.
#[test]
fn progress_import_today_counts_toward_today() {
    let (services, _store, user) = setup_with_store();
    services
        .import_cards(
            user,
            "Anki",
            vec![anki_row("today", 1)],
            vec![review(NOW - 1000, 1, Rating::Good)],
            None,
            NOW,
        )
        .unwrap();
    let counts = services
        .queue_counts(user, &StudyScope::all(), NOW)
        .unwrap();
    assert_eq!(counts.reviewed_today, 1);
}

// ---- full-fidelity import: suspension, rich extras, settings ----

fn import_row(front: &str, back: &str) -> ImportRow {
    ImportRow {
        anki_card_id: None,
        media: vec![],
        front: front.into(),
        back: back.into(),
        front_html: None,
        back_html: None,
        tags: vec![],
        deck: None,
        suspended: false,
        cloze_text: None,
        cloze_index: None,
        type_answer: None,
    }
}

#[test]
fn suspended_import_stays_out_of_the_study_queue() {
    let (services, store, user) = setup_with_store();
    let mut paused = anki_row("paused", 7);
    paused.suspended = true;
    services
        .import_cards(
            user,
            "Anki",
            vec![import_row("active", "b"), paused],
            vec![review(NOW - 1000, 7, Rating::Good)],
            None,
            NOW,
        )
        .unwrap();

    let row = store
        .list_cards(user, None, Some("paused"), 10)
        .unwrap()
        .remove(0);
    assert!(row.suspended, "arrives suspended, as in Anki");
    // Progress replay and suspension coexist: state exists, queue skips it.
    let state = store.get_card_state(user, row.id).unwrap();
    assert_eq!(state.reps, 1);
    let session = services
        .start_session(user, StudyScope::all(), NOW)
        .unwrap();
    assert_eq!(session.cards_due, 1, "only the active card is served");
}

#[test]
fn mcp_edit_makes_card_plain_and_detached() {
    let (services, store, user) = setup_with_store();
    let mut row = import_row("question", "answer");
    row.front_html = Some(flash_store::richtext::sanitize_with_media(
        "<b>question</b>",
    ));
    row.cloze_text = Some("{{c1::question}}\u{1f}".into());
    row.cloze_index = Some(1);
    row.type_answer = Some("answer".into());
    services
        .import_cards(user, "Anki", vec![row], vec![], None, NOW)
        .unwrap();

    let card = store.list_cards(user, None, None, 10).unwrap().remove(0);
    assert_eq!(card.front_html.as_deref(), Some("<b>question</b>"));
    assert_eq!(card.type_answer.as_deref(), Some("answer"));
    let exported = store.export_cards(user).unwrap();
    assert_eq!(exported[0].cloze.as_ref().unwrap().1, 1);

    // A plain-text (MCP) edit makes the card plain again. No stale rich view.
    store
        .update_card(
            user,
            card.id,
            &flash_core::validate_card_text("q2", "a2").unwrap(),
            NOW,
        )
        .unwrap();
    let card = store.get_card(user, card.id).unwrap().unwrap();
    assert!(card.front_html.is_none() && card.type_answer.is_none());
    assert!(store.export_cards(user).unwrap()[0].cloze.is_none());
}

#[test]
fn imported_settings_apply_and_junk_params_are_dropped() {
    let (services, store, user) = setup_with_store();
    let good = flash_store::import::ImportedSettings {
        desired_retention: Some(0.85),
        fsrs_params: None,
        new_per_day: Some(15),
    };
    services.adopt_imported_settings(user, &good).unwrap();
    let settings = store.get_settings(user).unwrap();
    assert!((settings.desired_retention - 0.85).abs() < 1e-6);
    assert_eq!(settings.new_per_day, 15);

    // Params the scheduler rejects never reach the database.
    let bad = flash_store::import::ImportedSettings {
        desired_retention: None,
        fsrs_params: Some(vec![1.0, 2.0]),
        new_per_day: None,
    };
    services.adopt_imported_settings(user, &bad).unwrap();
    assert!(store.get_settings(user).unwrap().fsrs_params.is_none());
}

// ---- notes: the advanced editor ----

mod notes {
    use super::*;
    use flash_core::{CardId, MediaId};
    use flash_server::service::{EditorSeed, NoteInput, ServiceError};
    use flash_store::media::MediaKind;
    use flash_store::notes::NoteType;

    fn input(kind: NoteType, front: &str, back: &str, tags: &[&str]) -> NoteInput {
        NoteInput {
            note_type: kind,
            front_html: front.into(),
            back_html: back.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            speech: Default::default(),
            langs: None,
        }
    }

    fn deck(store: &Store, user: flash_core::UserId) -> flash_core::DeckId {
        store.create_deck(user, "Notes", "", NOW).unwrap()
    }

    #[test]
    fn create_note_per_type_yields_the_right_cards() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        let cases = [
            (NoteType::Basic, "Q", "A", 1),
            (NoteType::BasicReversed, "Q", "A", 2),
            (NoteType::BasicTyped, "Q", "A", 1),
            (NoteType::Cloze, "{{c1::a}} {{c2::b}} {{c3::c}}", "", 3),
        ];
        for (kind, f, b, n) in cases {
            let (_, ids) = services
                .create_note(user, deck, &input(kind, f, b, &["T"]), NOW)
                .unwrap();
            assert_eq!(ids.len(), n, "{kind:?}");
            for id in ids {
                let card = store.get_card(user, id).unwrap().unwrap();
                assert_eq!(card.tags, vec!["t"]);
            }
        }
        assert_eq!(store.count_cards(user, Some(deck), None).unwrap(), 7);
    }

    #[test]
    fn editor_html_is_sanitized_before_anything_is_stored() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        let hostile = concat!(
            "<b onclick=\"x()\">bold</b><script>alert(1)</script>",
            "<span style=\"color:red\" class=\"hl-red evil\">red</span>",
            "<img src=\"https://evil.example/x.png\"><a href=\"javascript:1\">link</a>"
        );
        let (note, ids) = services
            .create_note(user, deck, &input(NoteType::Basic, hostile, "A", &[]), NOW)
            .unwrap();
        let card = store.get_card(user, ids[0]).unwrap().unwrap();
        let html = card.front_html.unwrap();
        assert!(
            !html.contains("script") && !html.contains("onclick"),
            "{html}"
        );
        assert!(!html.contains("style=") && !html.contains("evil"), "{html}");
        assert!(!html.contains("https://") && !html.contains("<a"), "{html}");
        assert!(
            html.contains("<b>bold</b>") && html.contains("class=\"hl-red\""),
            "{html}"
        );
        assert_eq!(card.front, "boldredlink", "plain text has no script body");
        let row = store.get_note(user, note).unwrap().unwrap();
        assert_eq!(row.front_html, html, "the note keeps the sanitized source");
    }

    #[test]
    fn media_must_belong_to_the_user() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        let other = store.create_user("O", None, "member", NOW).unwrap();
        let theirs = store
            .create_media(
                other,
                &"d".repeat(64),
                "d.png",
                "image/png",
                MediaKind::Image,
                5,
                NOW,
            )
            .unwrap();
        let foreign = format!("<img src=\"/media/{}\">", theirs.0);
        let err = services
            .create_note(user, deck, &input(NoteType::Basic, &foreign, "A", &[]), NOW)
            .unwrap_err();
        assert!(
            matches!(err, ServiceError::Refused { code: "media", .. }),
            "{err}"
        );
        let err = services
            .create_note(
                user,
                deck,
                &input(NoteType::Basic, "<img src=\"/media/424242\">", "A", &[]),
                NOW,
            )
            .unwrap_err();
        assert!(matches!(err, ServiceError::Refused { code: "media", .. }));
        assert!(store
            .list_cards(user, Some(deck), None, 10)
            .unwrap()
            .is_empty());

        let mine = store
            .create_media(
                user,
                &"e".repeat(64),
                "e.mp3",
                "audio/mpeg",
                MediaKind::Audio,
                5,
                NOW,
            )
            .unwrap();
        let audio = format!("<audio controls src=\"/media/{}\"></audio>", mine.0);
        let (_, ids) = services
            .create_note(user, deck, &input(NoteType::Basic, "Q", &audio, &[]), NOW)
            .unwrap();
        let card = store.get_card(user, ids[0]).unwrap().unwrap();
        assert_eq!(card.back, "[audio]");
        assert_eq!(
            store.media_for_export_in(user, None).unwrap()[0].id,
            MediaId(mine.0)
        );
    }

    #[test]
    fn invalid_notes_are_refused_with_a_message() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        for (kind, f, b) in [
            (NoteType::Basic, "", "A"),
            (NoteType::Basic, "Q", "<br>"),
            (NoteType::Cloze, "no cloze", ""),
            (NoteType::Cloze, "{{c1::}}", ""),
        ] {
            assert!(
                matches!(
                    services.create_note(user, deck, &input(kind, f, b, &[]), NOW),
                    Err(ServiceError::Invalid(_) | ServiceError::Refused { .. })
                ),
                "{kind:?} {f:?} {b:?}"
            );
        }
        let huge = "x".repeat(40_001);
        assert!(matches!(
            services.create_note(user, deck, &input(NoteType::Basic, &huge, "A", &[]), NOW),
            Err(ServiceError::Refused { code: "long", .. })
        ));
    }

    #[test]
    fn editing_an_imported_cloze_card_reuses_its_siblings() {
        let (services, store, user) = setup_with_store();
        let mut rows = Vec::new();
        for (i, front) in ["[...] b c", "a [...] c", "a b [...]"].iter().enumerate() {
            let mut r = import_row(front, "x");
            r.cloze_text = Some("{{c1::a}} {{c2::b}} {{c3::c}}\u{1f}Extra".into());
            r.cloze_index = Some(i as u32 + 1);
            r.front_html = Some(flash_store::richtext::sanitize_with_media(&format!(
                "<span class=\"cloze-blank\">[...]</span> {i}"
            )));
            rows.push(r);
        }
        services
            .import_cards(user, "Anki", rows, vec![], None, NOW)
            .unwrap();
        let cards = store.list_cards(user, None, None, 10).unwrap();
        assert_eq!(cards.len(), 3);
        let middle = cards.iter().find(|c| c.cloze_index == Some(2)).unwrap();

        let seed = services.editor_seed(user, middle.id).unwrap();
        assert_eq!(
            seed,
            EditorSeed {
                card_id: middle.id,
                deck_id: middle.deck_id,
                note_id: None,
                note_type: NoteType::Cloze,
                front_html: "{{c1::a}} {{c2::b}} {{c3::c}}".into(),
                back_html: "Extra".into(),
                tags: vec![],
                sibling_count: 1,
                langs: Default::default(),
            }
        );

        let (note, ids) = services
            .save_card_editor(
                user,
                middle.id,
                &input(
                    NoteType::Cloze,
                    "{{c1::a}} {{c2::B!}} {{c3::c}}",
                    "<b>More</b>",
                    &["z"],
                ),
                flash_store::RevisionSource::Web,
                NOW,
            )
            .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(
            store.list_cards(user, None, None, 10).unwrap().len(),
            3,
            "no duplicates"
        );
        let seed = services.editor_seed(user, middle.id).unwrap();
        assert_eq!(seed.note_id, Some(note));
        assert_eq!(seed.sibling_count, 3);
        // The imported source the edit replaced is the note's first
        // revision.
        let kept = services.card_revisions(user, middle.id).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].note_type, NoteType::Cloze);
        assert_eq!(kept[0].front_html, "{{c1::a}} {{c2::b}} {{c3::c}}");
        assert_eq!(kept[0].back_html, "Extra");
        assert!(kept[0].tags.is_empty());
        assert_eq!(seed.back_html, "<b>More</b>");
        assert_eq!(seed.tags, vec!["z"]);
        let c2 = store.get_card(user, middle.id).unwrap().unwrap();
        assert_eq!(c2.back, "B!\nMore");
        assert!(c2.back_html.unwrap().contains("<b>More</b>"));
    }

    #[test]
    fn editor_seed_for_plain_and_typed_cards() {
        let (services, store, user) = setup_with_store();
        let ids = services
            .create_cards(
                user,
                "Notes",
                &[("a <b> & c\nd".into(), "e".into(), vec!["t".into()])],
                NOW,
            )
            .unwrap();
        let seed = services.editor_seed(user, ids[0]).unwrap();
        assert_eq!(seed.note_type, NoteType::Basic);
        assert_eq!(
            seed.front_html, "a &lt;b&gt; &amp; c<br>d",
            "escaped, not parsed"
        );
        assert_eq!(seed.tags, vec!["t"]);

        let mut r = import_row("type me", "answer");
        r.type_answer = Some("answer".into());
        services
            .import_cards(user, "Anki", vec![r], vec![], None, NOW)
            .unwrap();
        let typed = store
            .list_cards(user, None, Some("type me"), 10)
            .unwrap()
            .remove(0);
        assert_eq!(
            services.editor_seed(user, typed.id).unwrap().note_type,
            NoteType::BasicTyped
        );

        assert!(matches!(
            services.editor_seed(user, CardId(9999)),
            Err(ServiceError::Store(flash_store::StoreError::NotFound(
                "card"
            )))
        ));
    }

    #[test]
    fn paged_listing_clamps_and_counts() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        let batch: Vec<_> = (0..30)
            .map(|i| (format!("q{i}"), "a".into(), vec![]))
            .collect();
        services.create_cards(user, "Notes", &batch, NOW).unwrap();
        let all = flash_server::deck_cards::DeckFilter::default();
        let page = |filter, n| {
            services
                .deck_cards_filtered(user, deck, filter, n, 25)
                .unwrap()
        };
        let first = page(&all, 1);
        assert_eq!((first.rows.len(), first.total), (25, 30));
        assert_eq!(page(&all, 2).rows.len(), 5);
        assert_eq!(
            page(&all, 99).rows.len(),
            5,
            "past the end clamps to the last page"
        );
        assert_eq!(page(&all, 0).rows.len(), 25, "page 0 clamps to 1");
        let q2 = flash_server::deck_cards::DeckFilter {
            search: Some("q2".into()),
            ..Default::default()
        };
        let found = page(&q2, 1);
        assert_eq!((found.rows.len(), found.total), (11, 11));
    }

    #[test]
    fn an_archived_card_leaves_study_and_waits_in_its_deck() {
        let (services, store, user) = setup_with_store();
        let deck = deck(&store, user);
        let batch: Vec<_> = (0..3)
            .map(|i| (format!("q{i}"), "a".into(), vec![]))
            .collect();
        let ids = services.create_cards(user, "Notes", &batch, NOW).unwrap();
        let session = services
            .start_session(user, StudyScope::all(), NOW)
            .unwrap();
        assert_eq!(session.cards_due, 3);

        services.set_card_archived(user, ids[0], true, NOW).unwrap();
        let head = services
            .session_head(user, session.session_id, NOW)
            .unwrap();
        assert_eq!(head.remaining, 2, "the session goes on without it");
        assert_ne!(head.next_card.map(|c| c.card_id), Some(ids[0]));

        let every = flash_server::deck_cards::DeckFilter {
            status: flash_store::CardStatus::All,
            ..Default::default()
        };
        let page = services
            .deck_cards_filtered(user, deck, &every, 1, 25)
            .unwrap();
        assert_eq!((page.total, page.archived), (3, 1));
        let archived = flash_server::deck_cards::DeckFilter {
            status: flash_store::CardStatus::Archived,
            ..Default::default()
        };
        let page = services
            .deck_cards_filtered(user, deck, &archived, 1, 25)
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].id, ids[0]);
        assert!(page.rows[0].suspended);

        services
            .set_card_archived(user, ids[0], false, NOW)
            .unwrap();
        let head = services
            .session_head(user, session.session_id, NOW)
            .unwrap();
        assert_eq!(head.remaining, 3);

        let other = store.create_user("Other", None, "member", NOW).unwrap();
        assert!(services
            .set_card_archived(other, ids[1], true, NOW)
            .is_err());
        assert!(!services.card(user, ids[1]).unwrap().suspended);
    }
}
