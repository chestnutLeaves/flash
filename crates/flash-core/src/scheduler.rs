//! FSRS scheduling behind domain types. The fsrs crate never leaks past
//! this module, so the algorithm can be swapped or hand-rolled without
//! touching stores, services, or handlers.

use crate::day::study_days_between;
use crate::goal::{recall_target, GoalWindow};
use crate::rating::{Phase, Rating};

pub const MS_PER_DAY: i64 = 86_400_000;
pub const MS_PER_MINUTE: i64 = 60_000;

/// Sub-day learning steps. FSRS proper is day-granularity; failed or hard
/// cards in a learning phase re-appear within the session instead.
const AGAIN_STEP_MS: i64 = 10 * MS_PER_MINUTE;
const HARD_STEP_MS: i64 = 15 * MS_PER_MINUTE;

/// Mutable scheduling state for one card (persisted in card_state).
#[derive(Debug, Clone, PartialEq)]
pub struct CardState {
    pub phase: Phase,
    /// None until the first review.
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    /// Epoch ms when the card is next due.
    pub due_ms: i64,
    pub last_review_ms: Option<i64>,
    pub reps: u32,
    pub lapses: u32,
}

impl CardState {
    /// State for a freshly created card: due immediately.
    pub fn new_card(created_at_ms: i64) -> Self {
        Self {
            phase: Phase::New,
            stability: None,
            difficulty: None,
            due_ms: created_at_ms,
            last_review_ms: None,
            reps: 0,
            lapses: 0,
        }
    }
}

/// Everything a single review produces: the updated state plus the
/// immutable log entry describing what happened.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewOutcome {
    pub state: CardState,
    pub rating: Rating,
    pub phase_before: Phase,
    pub elapsed_ms: i64,
    pub stability_after: f32,
    pub difficulty_after: f32,
    pub due_after_ms: i64,
}

#[derive(Debug)]
pub struct SchedulerError(pub String);

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scheduler error: {}", self.0)
    }
}

impl std::error::Error for SchedulerError {}

pub struct Scheduler {
    fsrs: fsrs::FSRS,
    desired_retention: f32,
    /// The forgetting curve's decay (`w[20]`), as `FSRS::new` clamps it:
    /// the crate keeps its parameters to itself.
    decay: f32,
    /// The calendar the days between reviews are counted on (`on_calendar`).
    timezone: String,
    cutoff_hour: u8,
}

impl Scheduler {
    /// `params`: fitted FSRS parameters, or None for the crate defaults.
    pub fn new(params: Option<&[f32]>, desired_retention: f32) -> Result<Self, SchedulerError> {
        let params = params.unwrap_or(&[]);
        let fsrs = fsrs::FSRS::new(params)
            .map_err(|e| SchedulerError(format!("invalid FSRS parameters: {e}")))?;
        let decay = fsrs::check_and_fill_parameters(params)
            .ok()
            .and_then(|w| w.get(20).copied())
            .unwrap_or(fsrs::FSRS6_DEFAULT_DECAY)
            .clamp(0.1, 0.8);
        Ok(Self {
            fsrs,
            desired_retention,
            decay,
            timezone: "UTC".into(),
            cutoff_hour: 4,
        })
    }

    /// Counts the days between reviews in an account's study days: in
    /// `timezone`, each starting at `cutoff_hour` (`day`). Unset, they are
    /// the default settings' days, UTC from 4am.
    pub fn on_calendar(mut self, timezone: &str, cutoff_hour: u8) -> Self {
        self.timezone = timezone.to_string();
        self.cutoff_hour = cutoff_hour;
        self
    }

    /// Applies one review at `now_ms` and returns the updated state.
    pub fn review(
        &self,
        state: &CardState,
        rating: Rating,
        now_ms: i64,
    ) -> Result<ReviewOutcome, SchedulerError> {
        self.review_toward(state, rating, now_ms, 0, None)
    }

    /// `review` for card `card_id` of a deck with a goal date ahead: a
    /// review FSRS would set after the goal, for a card that would by
    /// then be less likely than `goal::recall_target` to be remembered,
    /// comes back on one of the goal's last days instead (`goal`).
    pub fn review_toward(
        &self,
        state: &CardState,
        rating: Rating,
        now_ms: i64,
        card_id: i64,
        goal: Option<&GoalWindow>,
    ) -> Result<ReviewOutcome, SchedulerError> {
        let elapsed_ms = state
            .last_review_ms
            .map(|last| (now_ms - last).max(0))
            .unwrap_or(0);
        // Study days begun, as FSRS and Anki count them: 21:00 to 07:00
        // the next morning is a day, not a same-day review.
        let elapsed_days = state.last_review_ms.map_or(0, |last| {
            study_days_between(&self.timezone, self.cutoff_hour, last, now_ms)
        });

        let memory = match (state.stability, state.difficulty) {
            (Some(stability), Some(difficulty)) => Some(fsrs::MemoryState {
                stability,
                difficulty,
            }),
            _ => None,
        };
        let next = self
            .fsrs
            .next_states(memory, self.desired_retention, elapsed_days)
            .map_err(|e| SchedulerError(format!("next_states failed: {e}")))?;
        let chosen = match rating {
            Rating::Again => next.again,
            Rating::Hard => next.hard,
            Rating::Good => next.good,
            Rating::Easy => next.easy,
        };

        let in_learning = matches!(
            state.phase,
            Phase::New | Phase::Learning | Phase::Relearning
        );
        let (phase, due_ms, lapses) = match (state.phase, rating) {
            // Lapse: a known card was forgotten; relearn within the session.
            (Phase::Review, Rating::Again) => {
                (Phase::Relearning, now_ms + AGAIN_STEP_MS, state.lapses + 1)
            }
            (Phase::Review, _) => (
                Phase::Review,
                now_ms + interval_ms(chosen.interval),
                state.lapses,
            ),
            // Learning steps: failed/hard cards stay sub-day.
            (_, Rating::Again) if in_learning => (
                learning_phase(state.phase),
                now_ms + AGAIN_STEP_MS,
                state.lapses,
            ),
            (_, Rating::Hard) if in_learning => (
                learning_phase(state.phase),
                now_ms + HARD_STEP_MS,
                state.lapses,
            ),
            // Good/Easy graduates to the FSRS day-interval.
            _ => (
                Phase::Review,
                now_ms + interval_ms(chosen.interval),
                state.lapses,
            ),
        };
        let due_ms = match goal {
            Some(goal) if phase == Phase::Review && due_ms >= goal.goal_ms => {
                let days = (goal.goal_ms - now_ms).max(0) as f32 / MS_PER_DAY as f32;
                let recall = fsrs::current_retrievability(chosen.memory, days, self.decay);
                match goal.day_for(card_id) {
                    Some(day) if recall < recall_target(self.desired_retention) => day,
                    _ => due_ms,
                }
            }
            _ => due_ms,
        };

        let new_state = CardState {
            phase,
            stability: Some(chosen.memory.stability),
            difficulty: Some(chosen.memory.difficulty),
            due_ms,
            last_review_ms: Some(now_ms),
            reps: state.reps + 1,
            lapses,
        };
        Ok(ReviewOutcome {
            rating,
            phase_before: state.phase,
            elapsed_ms,
            stability_after: chosen.memory.stability,
            difficulty_after: chosen.memory.difficulty,
            due_after_ms: due_ms,
            state: new_state,
        })
    }
}

/// FSRS intervals are fractional days; scheduled reviews are at least one day out.
fn interval_ms(interval_days: f32) -> i64 {
    ((interval_days.max(1.0) as f64) * MS_PER_DAY as f64) as i64
}

fn learning_phase(current: Phase) -> Phase {
    match current {
        Phase::Relearning => Phase::Relearning,
        _ => Phase::Learning,
    }
}
