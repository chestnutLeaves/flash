//! A deck's goal date: the local calendar day a learner wants to know the
//! deck by (an exam, a trip, a deadline). Two things follow from it, and
//! both live here so the server, a sync replay and the app's port of this
//! file agree to the card:
//!
//! - the pace: how many new cards a day bring the whole deck in before
//!   the date, with the last stretch before it kept for review only;
//! - the final window: a review FSRS would set after the date, for a card
//!   that would by then be less likely than `recall_target` to be
//!   remembered, comes back on one of the window's days instead.
//!
//! Recall on the day depends far more on every card having one review in
//! the days just before it than on how the earlier gaps were spaced, so
//! nothing else changes: cards FSRS already brings back before the date,
//! and cards safe through it, keep their intervals. There is no interval
//! cap, which would hold easy cards down and pile every review onto the
//! last days.

pub use jiff::civil::Date;
use jiff::ToSpan;

use crate::day::{study_date, study_day_start_of};

/// The final window's share of the days left, and its bounds in days.
const WINDOW_SHARE: f64 = 0.12;
const MIN_WINDOW_DAYS: i64 = 3;
const MAX_WINDOW_DAYS: i64 = 14;
/// A goal this many days away or closer has no window of its own: every
/// day left is for new cards and last reviews at once.
const NO_WINDOW_WITHIN: i64 = 3;
/// How far ahead a goal date may be.
pub const MAX_DAYS_AHEAD: i64 = 5 * 366;

/// The recall a card should have on the goal day, for an account that
/// keeps `desired_retention` the rest of the time. It has to be above
/// the desired retention: FSRS sets each review for the moment recall
/// falls to that, so a review it sets after the goal leaves the card at
/// least that likely to be remembered on the day already. One review in
/// the last days lifts a card to about 97-99%; only cards strong enough
/// to stay above this on their own skip it.
pub fn recall_target(desired_retention: f32) -> f32 {
    (desired_retention + 0.05).clamp(0.95, 0.99)
}

/// A goal date as text: `YYYY-MM-DD`.
pub fn parse_date(text: &str) -> Option<Date> {
    let text = text.trim();
    if text.len() != 10 {
        return None;
    }
    Date::strptime("%Y-%m-%d", text).ok()
}

/// Days of the final window for a goal `days_left` study days away.
///
/// The window never takes more than the days past `NO_WINDOW_WITHIN`, so
/// a later goal never leaves fewer days for new cards: without that, a
/// goal four days away would get a three-day window and one day to learn
/// the whole deck, three times the pace of a goal three days away.
pub fn window_days(days_left: i64) -> i64 {
    if days_left <= NO_WINDOW_WITHIN {
        return 0;
    }
    ((days_left as f64 * WINDOW_SHARE).round() as i64)
        .clamp(MIN_WINDOW_DAYS, MAX_WINDOW_DAYS)
        .min(days_left - NO_WINDOW_WITHIN)
}

/// How a deck gets to its goal from today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Study days from today to the goal date: 0 on the day itself.
    pub days_left: i64,
    /// The last days before the goal, kept for review.
    pub window_days: i64,
    /// The days, from today, that bring new cards in.
    pub learn_days: i64,
    /// New cards a day.
    pub pace: u32,
}

/// The plan for `unseen` new cards and a goal `days_left` days away;
/// None once the goal date is behind.
pub fn plan(days_left: i64, unseen: u32) -> Option<Plan> {
    if days_left < 0 {
        return None;
    }
    let window_days = window_days(days_left);
    let learn_days = (days_left - window_days).max(1);
    let pace = (unseen as i64 + learn_days - 1) / learn_days;
    Some(Plan {
        days_left,
        window_days,
        learn_days,
        pace: pace.clamp(0, u32::MAX as i64) as u32,
    })
}

/// Study days from `today` to `goal` (negative once it has passed).
pub fn days_between(today: Date, goal: Date) -> i64 {
    today.until(goal).map(|s| s.get_days() as i64).unwrap_or(0)
}

/// How many new cards a deck is behind its plan: those left over once
/// the pace set when the goal was chosen (`baseline_pace`) has run over
/// the learning days still left. Adding cards re-plans (the caller sets
/// a new baseline); only skipped days put a deck behind.
pub fn behind_by(plan: &Plan, unseen: u32, baseline_pace: u32) -> u32 {
    let covered = baseline_pace as i64 * plan.learn_days;
    (unseen as i64 - covered).clamp(0, u32::MAX as i64) as u32
}

/// A goal as the scheduler sees it at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalWindow {
    /// Start of the goal date's study day: a review set at or after it
    /// would come after the goal.
    pub goal_ms: i64,
    /// Starts of the final window's study days still ahead (from
    /// tomorrow on), soonest first. Empty when none is left.
    pub days_ahead: Vec<i64>,
}

impl GoalWindow {
    /// The window of a goal on `goal` for an account in `timezone` with
    /// its day starting at `cutoff_hour`, seen at `now_ms`; None from the
    /// goal's own day on, when the deck is scheduled as any other.
    pub fn new(timezone: &str, cutoff_hour: u8, goal: Date, now_ms: i64) -> Option<Self> {
        let today = study_date(timezone, cutoff_hour, now_ms);
        let days_left = days_between(today, goal);
        if days_left <= 0 {
            return None;
        }
        let window = window_days(days_left).max(1);
        let first_ahead = window.min(days_left - 1);
        let days_ahead = (1..=first_ahead)
            .rev()
            .filter_map(|back| goal.checked_sub(back.days()).ok())
            .map(|date| study_day_start_of(timezone, cutoff_hour, date))
            .collect();
        Some(Self {
            goal_ms: study_day_start_of(timezone, cutoff_hour, goal),
            days_ahead,
        })
    }

    /// The window day a card comes back on: picked from its id, so a
    /// replay picks the same day and the window's load comes out even.
    pub fn day_for(&self, card_id: i64) -> Option<i64> {
        if self.days_ahead.is_empty() {
            return None;
        }
        let at = card_id.rem_euclid(self.days_ahead.len() as i64) as usize;
        self.days_ahead.get(at).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn the_window_grows_with_the_days_left_within_its_bounds() {
        assert_eq!(window_days(0), 0);
        assert_eq!(window_days(3), 0);
        assert_eq!(window_days(4), 1);
        assert_eq!(window_days(5), 2);
        assert_eq!(window_days(6), 3);
        assert_eq!(window_days(7), 3);
        assert_eq!(window_days(30), 4);
        assert_eq!(window_days(100), 12);
        assert_eq!(window_days(365), 14);
    }

    #[test]
    fn the_pace_spreads_the_unseen_cards_over_the_learning_days() {
        let p = plan(31, 120).unwrap();
        assert_eq!((p.window_days, p.learn_days, p.pace), (4, 27, 5));
        assert_eq!(plan(2, 10).unwrap().pace, 5);
        assert_eq!(plan(0, 7).unwrap().pace, 7);
        assert_eq!(plan(10, 0).unwrap().pace, 0);
        assert_eq!(plan(-1, 7), None);
    }

    #[test]
    fn a_later_goal_never_asks_more_a_day() {
        for unseen in [1, 7, 30, 400, 9_000] {
            let mut last = u32::MAX;
            for days_left in 0..=MAX_DAYS_AHEAD {
                let pace = plan(days_left, unseen).unwrap().pace;
                assert!(
                    pace <= last,
                    "{unseen} cards, {days_left} days: {pace} > {last}"
                );
                last = pace;
            }
        }
    }

    #[test]
    fn only_skipped_days_put_a_deck_behind() {
        let start = plan(30, 52).unwrap();
        assert_eq!(behind_by(&start, 52, start.pace), 0);
        let later = plan(25, 42).unwrap();
        assert_eq!(behind_by(&later, 42, start.pace), 0);
        assert_eq!(
            behind_by(&later, 52, start.pace),
            52 - 2 * later.learn_days as u32
        );
    }

    #[test]
    fn the_goal_day_asks_more_than_the_desired_retention() {
        for (desired, target) in [(0.9, 0.95), (0.92, 0.97), (0.97, 0.99), (0.7, 0.95)] {
            assert!((recall_target(desired) - target).abs() < 1e-6, "{desired}");
        }
    }

    #[test]
    fn a_date_is_read_only_in_full() {
        assert_eq!(parse_date("2026-11-16"), Some(date(2026, 11, 16)));
        assert_eq!(parse_date(" 2026-11-16 "), Some(date(2026, 11, 16)));
        assert_eq!(parse_date("2026-11-6"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("tomorrow"), None);
    }
}
