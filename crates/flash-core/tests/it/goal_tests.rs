//! A deck's goal date as the scheduler applies it: the final window's
//! days on the account's calendar, and which reviews come back in it.

use flash_core::day::{study_date, study_day_start_of};
use flash_core::goal::GoalWindow;
use flash_core::scheduler::MS_PER_DAY;
use flash_core::{CardState, Phase, Rating, Scheduler};
use jiff::civil::date;

const ZONE: &str = "America/New_York";

/// 10:00 local on a date.
fn at(y: i16, m: i8, d: i8) -> i64 {
    date(y, m, d)
        .at(10, 0, 0, 0)
        .in_tz(ZONE)
        .unwrap()
        .timestamp()
        .as_millisecond()
}

/// A card reviewed once, `stability` days strong, last seen at `last`.
fn known(stability: f32, last: i64) -> CardState {
    CardState {
        phase: Phase::Review,
        stability: Some(stability),
        difficulty: Some(5.0),
        due_ms: last + (stability as i64) * MS_PER_DAY,
        last_review_ms: Some(last),
        reps: 3,
        lapses: 0,
    }
}

#[test]
fn the_window_holds_the_last_days_before_the_goal_from_tomorrow_on() {
    let now = at(2026, 10, 16);
    let goal = date(2026, 11, 16);
    let w = GoalWindow::new(ZONE, 4, goal, now).unwrap();
    assert_eq!(w.goal_ms, study_day_start_of(ZONE, 4, goal));
    let dates: Vec<_> = w
        .days_ahead
        .iter()
        .map(|&ms| study_date(ZONE, 4, ms))
        .collect();
    assert_eq!(
        dates,
        vec![
            date(2026, 11, 12),
            date(2026, 11, 13),
            date(2026, 11, 14),
            date(2026, 11, 15)
        ]
    );
    // Clocks went back on Nov 1: each day still starts at 4am local.
    for &ms in &w.days_ahead {
        let local = jiff::Timestamp::from_millisecond(ms)
            .unwrap()
            .in_tz(ZONE)
            .unwrap();
        assert_eq!(local.hour(), 4);
    }
}

#[test]
fn inside_the_window_only_the_days_still_ahead_count() {
    let goal = date(2026, 11, 16);
    let w = GoalWindow::new(ZONE, 4, goal, at(2026, 11, 14)).unwrap();
    assert_eq!(w.days_ahead.len(), 1, "only Nov 15 is still ahead");
    let w = GoalWindow::new(ZONE, 4, goal, at(2026, 11, 15)).unwrap();
    assert!(w.days_ahead.is_empty());
    assert_eq!(w.day_for(7), None);
    assert_eq!(GoalWindow::new(ZONE, 4, goal, at(2026, 11, 16)), None);
    assert_eq!(GoalWindow::new(ZONE, 4, goal, at(2026, 11, 20)), None);
}

#[test]
fn a_cards_window_day_comes_from_its_id_and_spreads_evenly() {
    let w = GoalWindow::new(ZONE, 4, date(2026, 11, 16), at(2026, 10, 16)).unwrap();
    assert_eq!(w.day_for(5), w.day_for(5));
    let mut load = [0u32; 4];
    for id in 1..=400 {
        let day = w.day_for(id).unwrap();
        let i = w.days_ahead.iter().position(|&d| d == day).unwrap();
        load[i] += 1;
    }
    assert_eq!(load, [100, 100, 100, 100]);
}

#[test]
fn a_review_set_past_the_goal_comes_back_in_the_window() {
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let now = at(2026, 10, 16);
    let goal = GoalWindow::new(ZONE, 4, date(2026, 11, 16), now).unwrap();
    let state = known(15.0, now - 15 * MS_PER_DAY);
    let plain = scheduler.review(&state, Rating::Good, now).unwrap();
    assert!(plain.due_after_ms > goal.goal_ms);
    let days = (goal.goal_ms - now) as f32 / MS_PER_DAY as f32;
    let memory = fsrs::MemoryState {
        stability: plain.stability_after,
        difficulty: plain.difficulty_after,
    };
    assert!(fsrs::current_retrievability(memory, days, fsrs::FSRS6_DEFAULT_DECAY) < 0.95);
    let toward = scheduler
        .review_toward(&state, Rating::Good, now, 42, Some(&goal))
        .unwrap();
    assert_eq!(Some(toward.due_after_ms), goal.day_for(42));
    assert_eq!(toward.state.due_ms, toward.due_after_ms);
    assert!(toward.due_after_ms < goal.goal_ms);
    assert_eq!(toward.stability_after, plain.stability_after);
}

#[test]
fn a_card_safe_through_the_goal_keeps_its_interval() {
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let now = at(2026, 10, 16);
    let goal = GoalWindow::new(ZONE, 4, date(2026, 10, 20), now).unwrap();
    let state = known(60.0, now - 30 * MS_PER_DAY);
    let plain = scheduler.review(&state, Rating::Good, now).unwrap();
    let toward = scheduler
        .review_toward(&state, Rating::Good, now, 9, Some(&goal))
        .unwrap();
    assert!(plain.due_after_ms > goal.goal_ms);
    assert_eq!(toward.due_after_ms, plain.due_after_ms);
}

#[test]
fn learning_steps_and_reviews_before_the_window_are_untouched() {
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let now = at(2026, 10, 16);
    let goal = GoalWindow::new(ZONE, 4, date(2027, 6, 1), now).unwrap();
    let state = known(20.0, now - 20 * MS_PER_DAY);
    let again = scheduler
        .review_toward(&state, Rating::Again, now, 3, Some(&goal))
        .unwrap();
    assert_eq!(again.state.phase, Phase::Relearning);
    assert_eq!(again.due_after_ms - now, 10 * 60_000);
    let fresh = CardState::new_card(now);
    let good = scheduler.review(&fresh, Rating::Good, now).unwrap();
    let toward = scheduler
        .review_toward(&fresh, Rating::Good, now, 3, Some(&goal))
        .unwrap();
    assert_eq!(toward.due_after_ms, good.due_after_ms);
}
