use std::sync::Arc;

use flash_core::goal::Date;
use flash_core::queue::StudyScope;
use flash_core::{CardState, DeckId, Phase, Rating, UserId};
use flash_server::service::{ReviewOrigin, ServiceError, Services};
use flash_store::Store;

const NOW: i64 = 1_700_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

fn date(text: &str) -> Date {
    flash_core::goal::parse_date(text).unwrap()
}

fn setup() -> (Services, UserId) {
    let store = Store::open_in_memory().unwrap();
    let user = store.create_user("Test", None, "member", NOW).unwrap();
    (Services::new(Arc::new(store)), user)
}

fn cards(services: &Services, user: UserId, deck: &str, n: usize) -> DeckId {
    let rows: Vec<(String, String, Vec<String>)> = (0..n)
        .map(|i| (format!("{deck} {i}"), "b".into(), vec![]))
        .collect();
    services.create_cards(user, deck, &rows, NOW).unwrap();
    services
        .store()
        .find_deck_by_name(user, deck)
        .unwrap()
        .unwrap()
}

fn new_for(services: &Services, user: UserId, scope: StudyScope, at: i64) -> u32 {
    services
        .queue_counts(user, &scope, at)
        .unwrap()
        .new_available
}

fn row(services: &Services, user: UserId, deck: DeckId, at: i64) -> flash_server::service::DeckRow {
    services.deck_by_id(user, deck, at).unwrap()
}

#[test]
fn the_pace_replaces_the_deck_limit_and_comes_on_top_of_the_account() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 120);
    cards(&services, user, "Other", 30);
    services
        .set_deck_goal(user, exam, Some(date("2023-12-15")), NOW)
        .unwrap();
    assert_eq!(new_for(&services, user, StudyScope::deck(exam), NOW), 5);
    assert_eq!(new_for(&services, user, StudyScope::all(), NOW), 25);
    let exam_row = row(&services, user, exam, NOW);
    assert_eq!(exam_row.new_count, 5);
    let goal = exam_row.goal.unwrap();
    assert_eq!((goal.days_left, goal.unseen, goal.total), (31, 120, 120));
    assert_eq!(goal.plan.unwrap().pace, 5);
    assert_eq!(goal.cutoff(), Some(date("2023-12-11")));
    assert_eq!(goal.behind_by, 0);

    let session = services
        .start_session(user, StudyScope::deck(exam), NOW)
        .unwrap();
    let mut next = session.first_card.map(|c| c.card_id);
    let mut at = NOW;
    while let Some(card) = next {
        at += 1_000;
        next = services
            .submit_review(
                user,
                session.session_id,
                card,
                Rating::Good,
                ReviewOrigin::new("web", None),
                at,
            )
            .unwrap()
            .next_card
            .map(|c| c.card_id);
    }
    assert_eq!(new_for(&services, user, StudyScope::deck(exam), at + 1), 0);
    assert_eq!(new_for(&services, user, StudyScope::all(), at + 1), 20);
    assert_eq!(
        row(&services, user, exam, at + 1)
            .goal
            .unwrap()
            .plan
            .unwrap()
            .pace,
        5
    );
}

#[test]
fn a_review_set_past_the_goal_comes_back_before_it() {
    let (services, user) = setup();
    let card = services
        .create_cards(user, "Exam", &[("q".into(), "a".into(), vec![])], NOW)
        .unwrap()[0];
    let exam = services
        .store()
        .find_deck_by_name(user, "Exam")
        .unwrap()
        .unwrap();
    services
        .store()
        .set_card_states(
            user,
            &[(
                card,
                CardState {
                    phase: Phase::Review,
                    stability: Some(15.0),
                    difficulty: Some(5.0),
                    due_ms: NOW - DAY,
                    last_review_ms: Some(NOW - 15 * DAY),
                    reps: 3,
                    lapses: 0,
                },
            )],
        )
        .unwrap();
    let plain_good = services.grade_preview(user, card, NOW).unwrap()[2];
    services
        .set_deck_goal(user, exam, Some(date("2023-12-15")), NOW)
        .unwrap();
    let goal_start = flash_core::day::study_day_start_of("UTC", 4, date("2023-12-15"));
    assert!(NOW + plain_good > goal_start, "the premise: past the goal");
    let toward_good = services.grade_preview(user, card, NOW).unwrap()[2];
    assert!(NOW + toward_good < goal_start);
    assert!(NOW + toward_good >= flash_core::day::study_day_start_of("UTC", 4, date("2023-12-11")));

    let session = services
        .start_session(user, StudyScope::deck(exam), NOW)
        .unwrap();
    services
        .submit_review(
            user,
            session.session_id,
            card,
            Rating::Good,
            ReviewOrigin::new("web", None),
            NOW,
        )
        .unwrap();
    let state = services.store().get_card_state(user, card).unwrap();
    assert_eq!(state.due_ms, NOW + toward_good, "recorded as previewed");
}

#[test]
fn skipped_days_put_a_deck_behind_and_adding_cards_replans() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 52);
    services
        .set_deck_goal(user, exam, Some(date("2023-12-14")), NOW)
        .unwrap();
    assert_eq!(row(&services, user, exam, NOW).goal.unwrap().behind_by, 0);
    let later = NOW + 5 * DAY;
    let goal = row(&services, user, exam, later).goal.unwrap();
    assert_eq!(goal.behind_by, 8);
    assert_eq!(goal.plan.unwrap().pace, 3);
    services
        .create_cards(
            user,
            "Exam",
            &(0..10)
                .map(|i| (format!("more {i}"), "b".to_string(), vec![]))
                .collect::<Vec<_>>(),
            later,
        )
        .unwrap();
    let goal = row(&services, user, exam, later).goal.unwrap();
    assert_eq!(goal.behind_by, 0);
    assert_eq!(goal.total, 62);
}

#[test]
fn a_goal_passed_paces_nothing_and_says_so() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 40);
    services
        .set_deck_goal(user, exam, Some(date("2023-11-20")), NOW)
        .unwrap();
    let on_the_day = NOW + 6 * DAY;
    let goal = row(&services, user, exam, on_the_day).goal.unwrap();
    assert_eq!((goal.days_left, goal.plan), (0, None));
    assert_eq!(
        new_for(&services, user, StudyScope::deck(exam), on_the_day),
        20
    );
    let after = NOW + 8 * DAY;
    let goal = row(&services, user, exam, after).goal.unwrap();
    assert!(goal.passed());
    assert_eq!(new_for(&services, user, StudyScope::deck(exam), after), 20);
    services.set_deck_goal(user, exam, None, after).unwrap();
    assert_eq!(row(&services, user, exam, after).goal, None);
}

#[test]
fn a_subdeck_is_paced_by_the_goal_of_the_deck_it_sits_in() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 0);
    let part = cards(&services, user, "Exam::Part", 60);
    services
        .set_deck_goal(user, exam, Some(date("2023-12-15")), NOW)
        .unwrap();
    assert_eq!(new_for(&services, user, StudyScope::deck(part), NOW), 3);
    assert_eq!(row(&services, user, part, NOW).goal, None);
    let goal = row(&services, user, exam, NOW).goal.unwrap();
    assert_eq!((goal.unseen, goal.plan.unwrap().pace), (60, 3));
}

#[test]
fn a_subdeck_whose_goal_passed_is_paced_by_the_deck_it_sits_in() {
    let (services, user) = setup();
    let med = cards(&services, user, "Med", 0);
    let cardio = cards(&services, user, "Med::Cardio", 60);
    services
        .set_deck_goal(user, cardio, Some(date("2023-11-20")), NOW)
        .unwrap();
    services
        .set_deck_goal(user, med, Some(date("2023-12-15")), NOW)
        .unwrap();
    let after = NOW + 8 * DAY;
    assert!(row(&services, user, cardio, after).goal.unwrap().passed());
    // Med's plan counts Cardio's cards: Cardio comes at its pace, and its
    // reviews are scheduled toward its date.
    let goal = row(&services, user, med, after).goal.unwrap();
    assert_eq!((goal.days_left, goal.unseen), (23, 60));
    assert_eq!(
        new_for(&services, user, StudyScope::deck(cardio), after),
        goal.plan.unwrap().pace
    );
    let settings = services.store().get_settings(user).unwrap();
    let window = services
        .goal_window(user, &settings, cardio, after)
        .unwrap()
        .expect("Med's window");
    assert_eq!(
        window.goal_ms,
        flash_core::day::study_day_start_of("UTC", 4, date("2023-12-15"))
    );
}

#[test]
fn the_goal_date_for_a_deck_skips_a_goal_already_passed() {
    let decks = [
        (DeckId(1), "Med".to_string(), Some(date("2023-12-15"))),
        (
            DeckId(2),
            "Med::Cardio".to_string(),
            Some(date("2023-11-20")),
        ),
    ];
    let on = |today: &str| flash_server::goals::goal_date_for(DeckId(2), &decks, date(today));
    assert_eq!(on("2023-11-14"), Some(date("2023-11-20")));
    assert_eq!(on("2023-11-20"), Some(date("2023-11-20")), "on the day");
    assert_eq!(on("2023-11-22"), Some(date("2023-12-15")));
    assert_eq!(on("2023-12-16"), None);
}

#[test]
fn a_goal_date_is_after_today_and_within_five_years() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 3);
    for bad in ["2023-11-14", "2023-11-01", "2029-01-01"] {
        let err = services
            .set_deck_goal(user, exam, Some(date(bad)), NOW)
            .unwrap_err();
        assert!(matches!(err, ServiceError::Invalid(_)), "{bad}: {err}");
    }
    services
        .set_deck_goal(user, exam, Some(date("2023-11-15")), NOW)
        .unwrap();
}

#[test]
fn the_preview_says_how_a_date_would_go() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 120);
    let preview = services
        .goal_preview(user, exam, date("2023-12-15"), NOW)
        .unwrap();
    assert_eq!((preview.unseen, preview.plan.pace), (120, 5));
    assert_eq!(preview.cutoff(), date("2023-12-11"));
    assert_eq!(row(&services, user, exam, NOW).goal, None);
}

#[test]
fn a_goal_set_after_studying_today_is_on_track() {
    let (services, user) = setup();
    let exam = cards(&services, user, "Exam", 60);
    let session = services
        .start_session(user, StudyScope::deck(exam), NOW)
        .unwrap();
    let mut next = session.first_card.map(|c| c.card_id);
    let mut at = NOW;
    for _ in 0..12 {
        let Some(card) = next else { break };
        at += 1_000;
        next = services
            .submit_review(
                user,
                session.session_id,
                card,
                Rating::Good,
                ReviewOrigin::new("web", None),
                at,
            )
            .unwrap()
            .next_card
            .map(|c| c.card_id);
    }
    services
        .set_deck_goal(user, exam, Some(date("2023-11-27")), at)
        .unwrap();
    let goal = row(&services, user, exam, at + 1).goal.unwrap();
    assert_eq!(goal.unseen, 60, "as the day began");
    assert_eq!(goal.behind_by, 0);
    let preview = services
        .goal_preview(user, exam, date("2023-11-27"), at)
        .unwrap();
    assert_eq!(preview.plan.pace, goal.plan.unwrap().pace);
}
