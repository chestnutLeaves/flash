//! Reference vectors for anything that re-implements this crate's
//! scheduling: seeded review sequences with every outcome the scheduler
//! produces (phase, stability, difficulty, due time, reps, lapses) over
//! several parameter sets, retentions and calendars, a few by hand at the
//! edges of the study day, the same toward a goal date,
//! a goal's plans and final windows, and study-day boundaries over time
//! zones, cutoffs and clock changes.
//!
//! `FLASH_WRITE_VECTORS=1 cargo test -p flash-core --test it vector_gen`
//! writes `tests/scheduler_vectors_rs.json`; without the variable the
//! test checks the file still says what this code computes, so a change
//! to the scheduler or to the fsrs crate shows up here first.

use flash_core::day::{next_study_day_ms, study_day_start_ms, study_day_start_of};
use flash_core::goal::{self, GoalWindow};
use flash_core::{CardState, Phase, Rating, ReviewOutcome, Scheduler};
use jiff::civil::date;
use serde_json::{json, Value};

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scheduler_vectors_rs.json"
);

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// The gaps between reviews: inside and around the learning steps, the
/// day boundary, and long ones.
const GAPS: [i64; 13] = [
    0,
    30_000,
    9 * MINUTE,
    11 * MINUTE,
    14 * MINUTE,
    16 * MINUTE,
    HOUR,
    DAY - MINUTE,
    DAY,
    DAY + 1,
    3 * DAY,
    30 * DAY,
    400 * DAY,
];

/// A tiny deterministic generator, so the vectors never change unless
/// the scheduler does.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn rating_of(n: u64) -> Rating {
    match n {
        0 => Rating::Again,
        1 => Rating::Hard,
        2 => Rating::Good,
        _ => Rating::Easy,
    }
}

fn phase_code(p: Phase) -> u8 {
    match p {
        Phase::New => 0,
        Phase::Learning => 1,
        Phase::Review => 2,
        Phase::Relearning => 3,
    }
}

/// The parameter sets, as an account stores them: none (the crate's
/// defaults), an FSRS-4.5 set of 17, a set of 19, a custom set of 21, and
/// one out of range that the crate clips.
fn parameter_sets() -> Vec<(&'static str, Option<Vec<f32>>)> {
    vec![
        ("default", None),
        (
            "fsrs45_17",
            Some(vec![
                0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49, 0.14, 0.94, 2.18, 0.05, 0.34,
                1.26, 0.29, 2.61,
            ]),
        ),
        (
            "fsrs5_19",
            Some(vec![
                0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192,
                1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
            ]),
        ),
        (
            "custom_21",
            Some(vec![
                0.3, 1.1, 2.9, 9.4, 6.9, 0.71, 2.5, 0.004, 1.61, 0.21, 0.88, 1.62, 0.07, 0.31,
                1.51, 0.52, 2.1, 0.61, 0.13, 0.08, 0.21,
            ]),
        ),
        (
            "clipped_21",
            Some(vec![
                200.0, -1.0, 2.3065, 8.2956, 11.0, 0.8334, 3.0194, 0.9, 1.8722, 0.1666, 0.796,
                1.4835, 0.0614, 0.2629, 1.6483, 0.6014, 7.0, 3.0, 3.0, 0.0658, 0.95,
            ]),
        ),
    ]
}

fn step_row(now_ms: i64, rating: Rating, outcome: &ReviewOutcome) -> Value {
    json!([
        now_ms,
        rating as u8,
        phase_code(outcome.phase_before),
        phase_code(outcome.state.phase),
        outcome.elapsed_ms,
        outcome.stability_after as f64,
        outcome.difficulty_after as f64,
        outcome.due_after_ms,
        outcome.state.reps,
        outcome.state.lapses,
    ])
}

/// The calendars days between reviews are counted on, a sequence each in
/// turn: whole, half and three-quarter hour offsets, clocks that change
/// and ones that do not, and cutoffs from midnight to 23:00.
const CALENDARS: [(&str, u8); 4] = [
    ("UTC", 4),
    ("America/New_York", 4),
    ("Asia/Kolkata", 0),
    ("Pacific/Chatham", 23),
];

fn sequences() -> Value {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut out = Vec::new();
    for (name, params) in parameter_sets() {
        for retention in [0.8f32, 0.9, 0.95] {
            for n in 0..10 {
                let (zone, cutoff) = CALENDARS[n % CALENDARS.len()];
                let scheduler = Scheduler::new(params.as_deref(), retention)
                    .unwrap()
                    .on_calendar(zone, cutoff);
                let mut state = CardState::new_card(1_700_000_000_000);
                let mut now_ms = 1_700_000_000_000i64;
                let mut steps = Vec::new();
                let len = 1 + rng.below(30);
                for _ in 0..len {
                    now_ms += GAPS[rng.below(GAPS.len() as u64) as usize];
                    let rating = rating_of(rng.below(4));
                    let outcome = scheduler.review(&state, rating, now_ms).unwrap();
                    steps.push(step_row(now_ms, rating, &outcome));
                    state = outcome.state;
                }
                out.push(json!({
                    "name": format!("{name}/{retention}/{n}"),
                    "params": params.as_ref().map(|p| p.iter().map(|w| *w as f64).collect::<Vec<_>>()),
                    "desired_retention": retention as f64,
                    "created_ms": 1_700_000_000_000i64,
                    "tz": zone,
                    "cutoff": cutoff,
                    "steps": steps,
                }));
            }
        }
    }
    out.extend(day_sequences());
    Value::Array(out)
}

/// Reviews at the edges of the study day, by hand: the next morning
/// after an evening, more the same evening, across both changes of
/// clocks (the fall's with the cutoff inside the repeated hour), and a
/// millisecond either side of the cutoff.
fn day_sequences() -> Vec<Value> {
    use Rating::{Good, Hard};
    let at = |zone: &str, (y, m, d): (i16, i8, i8), (h, min): (i8, i8)| {
        date(y, m, d)
            .at(h, min, 0, 0)
            .in_tz(zone)
            .unwrap()
            .timestamp()
            .as_millisecond()
    };
    let ny = |day, time| at("America/New_York", day, time);
    let berlin = |day, time| at("Europe/Berlin", day, time);
    let kolkata = |day, time| at("Asia/Kolkata", day, time);
    let cases = [
        (
            "evening_then_morning",
            "America/New_York",
            4,
            vec![
                (ny((2026, 10, 12), (21, 0)), Good),
                (ny((2026, 10, 13), (7, 0)), Good),
                (ny((2026, 10, 14), (6, 30)), Good),
            ],
        ),
        (
            "same_evening",
            "America/New_York",
            4,
            vec![
                (ny((2026, 10, 12), (21, 0)), Good),
                (ny((2026, 10, 12), (23, 30)), Good),
                (ny((2026, 10, 13), (3, 30)), Good),
                (ny((2026, 10, 13), (4, 30)), Good),
            ],
        ),
        (
            "spring_forward",
            "America/New_York",
            4,
            vec![
                (ny((2026, 3, 7), (4, 30)), Good),
                (ny((2026, 3, 8), (4, 15)), Good),
                (ny((2026, 3, 9), (3, 45)), Hard),
                (ny((2026, 3, 9), (4, 0)), Good),
            ],
        ),
        (
            "fall_back",
            "Europe/Berlin",
            2,
            vec![
                (berlin((2026, 10, 24), (2, 30)), Good),
                (berlin((2026, 10, 25), (2, 30)), Good),
                // The same wall time an hour later, once the clocks went back.
                (berlin((2026, 10, 25), (2, 30)) + HOUR, Good),
                (berlin((2026, 10, 26), (1, 59)), Good),
                (berlin((2026, 10, 26), (2, 0)), Good),
            ],
        ),
        (
            "at_the_cutoff",
            "Asia/Kolkata",
            0,
            vec![
                (kolkata((2026, 10, 12), (12, 0)), Good),
                (kolkata((2026, 10, 13), (0, 0)) - 1, Good),
                (kolkata((2026, 10, 13), (0, 0)), Good),
            ],
        ),
    ];
    cases
        .into_iter()
        .map(|(name, zone, cutoff, reviews)| {
            let scheduler = Scheduler::new(None, 0.9).unwrap().on_calendar(zone, cutoff);
            let created = reviews[0].0;
            let mut state = CardState::new_card(created);
            let mut steps = Vec::new();
            for (now_ms, rating) in reviews {
                let outcome = scheduler.review(&state, rating, now_ms).unwrap();
                steps.push(step_row(now_ms, rating, &outcome));
                state = outcome.state;
            }
            json!({
                "name": format!("day/{name}"),
                "params": null,
                "desired_retention": 0.9,
                "created_ms": created,
                "tz": zone,
                "cutoff": cutoff,
                "steps": steps,
            })
        })
        .collect()
}

/// Sequences toward a goal date: gaps of days around a goal six weeks
/// out, in a zone whose clocks change before it, so reviews land before
/// its final window, inside it and after the goal. Each card has its own
/// id, which picks its window day.
fn goal_sequences() -> Value {
    const ZONE: &str = "America/New_York";
    const CUTOFF: u8 = 4;
    let goal = date(2026, 11, 16);
    let start = 1_790_863_200_000i64;
    let gaps = [
        HOUR,
        DAY - MINUTE,
        DAY,
        2 * DAY,
        3 * DAY,
        5 * DAY,
        9 * DAY,
        16 * DAY,
    ];
    let mut rng = XorShift(0xD1B5_4A32_D192_ED03);
    let mut out = Vec::new();
    for (name, params) in parameter_sets() {
        for retention in [0.85f32, 0.9, 0.95] {
            let scheduler = Scheduler::new(params.as_deref(), retention)
                .unwrap()
                .on_calendar(ZONE, CUTOFF);
            for n in 0..8 {
                let card_id = 1 + rng.below(1_000) as i64;
                let mut state = CardState::new_card(start);
                let mut now_ms = start;
                let mut steps = Vec::new();
                let len = 2 + rng.below(18);
                for _ in 0..len {
                    now_ms += gaps[rng.below(gaps.len() as u64) as usize];
                    let rating = rating_of(rng.below(4));
                    let window = GoalWindow::new(ZONE, CUTOFF, goal, now_ms);
                    let outcome = scheduler
                        .review_toward(&state, rating, now_ms, card_id, window.as_ref())
                        .unwrap();
                    steps.push(step_row(now_ms, rating, &outcome));
                    state = outcome.state;
                }
                out.push(json!({
                    "name": format!("goal/{name}/{retention}/{n}"),
                    "params": params.as_ref().map(|p| p.iter().map(|w| *w as f64).collect::<Vec<_>>()),
                    "desired_retention": retention as f64,
                    "created_ms": start,
                    "card_id": card_id,
                    "tz": ZONE,
                    "cutoff": CUTOFF,
                    "goal": goal.to_string(),
                    "steps": steps,
                }));
            }
        }
    }
    Value::Array(out)
}

/// A goal's plan over the days left and unseen counts that matter, and
/// its final window seen from before, inside and after it, across a
/// change of clocks and with the day starting at midnight and at 23:00.
fn goal_plans() -> Value {
    let mut plans = Vec::new();
    for days_left in [
        -1i64, 0, 1, 2, 3, 4, 5, 8, 12, 25, 30, 31, 60, 100, 116, 117, 365, 1500,
    ] {
        for unseen in [0u32, 1, 7, 120, 5000] {
            let plan = goal::plan(days_left, unseen);
            plans.push(json!([
                days_left,
                unseen,
                plan.map(|p| p.window_days),
                plan.map(|p| p.learn_days),
                plan.map(|p| p.pace),
                plan.map(|p| goal::behind_by(&p, unseen, 3)),
            ]));
        }
    }
    let mut windows = Vec::new();
    for zone in ["America/New_York", "Europe/Berlin", "Asia/Kathmandu", "UTC"] {
        for cutoff in [0u8, 4, 23] {
            for goal in [date(2026, 11, 3), date(2026, 3, 30)] {
                let goal_ms = study_day_start_of(zone, cutoff, goal);
                for back in [40i64, 10, 4, 2, 1, 0, -1] {
                    let now = goal_ms - back * DAY + 5 * HOUR;
                    let window = GoalWindow::new(zone, cutoff, goal, now);
                    windows.push(json!([
                        zone,
                        cutoff,
                        goal.to_string(),
                        now,
                        window.as_ref().map(|w| w.goal_ms),
                        window.as_ref().map(|w| w.days_ahead.clone()),
                    ]));
                }
            }
        }
    }
    json!({ "plans": plans, "windows": windows })
}

/// Instants around the clock changes of 2026 and ordinary days, in zones
/// with whole, half and quarter hour offsets, both hemispheres, and one
/// the tz database does not know.
fn days() -> Value {
    let zones = [
        "UTC",
        "America/New_York",
        "America/Los_Angeles",
        "Europe/London",
        "Europe/Berlin",
        "Asia/Kolkata",
        "Asia/Kathmandu",
        "Australia/Sydney",
        "Pacific/Chatham",
        "America/Sao_Paulo",
        "Not/AZone",
    ];
    // 2026-03-08 (US spring), 2026-03-29 (EU spring), 2026-10-25 (EU
    // fall), 2026-11-01 (US fall), 2026-04-05 (Sydney fall), 2026-10-04
    // (Sydney spring), and an ordinary week in June.
    let anchors: [i64; 7] = [
        1_772_928_000_000,
        1_774_742_400_000,
        1_792_886_400_000,
        1_793_491_200_000,
        1_775_347_200_000,
        1_790_985_600_000,
        1_781_136_000_000,
    ];
    let mut out = Vec::new();
    for zone in zones {
        for cutoff in [0u8, 2, 3, 4, 23] {
            let mut cases = Vec::new();
            for anchor in anchors {
                let mut t = anchor - DAY;
                while t < anchor + 2 * DAY {
                    cases.push(json!([
                        t,
                        study_day_start_ms(zone, cutoff, t),
                        next_study_day_ms(zone, cutoff, t),
                    ]));
                    t += 173 * MINUTE;
                }
            }
            out.push(json!({ "tz": zone, "cutoff": cutoff, "cases": cases }));
        }
    }
    Value::Array(out)
}

const STEP_COLUMNS: [&str; 10] = [
    "now_ms",
    "rating",
    "phase_before",
    "phase_after",
    "elapsed_ms",
    "stability",
    "difficulty",
    "due_ms",
    "reps",
    "lapses",
];
const DAY_COLUMNS: [&str; 3] = ["now_ms", "start_ms", "next_ms"];
const GOAL_PLAN_COLUMNS: [&str; 6] = [
    "days_left",
    "unseen",
    "window_days",
    "learn_days",
    "pace",
    "behind_by_from_baseline_3",
];
const GOAL_WINDOW_COLUMNS: [&str; 6] = ["tz", "cutoff", "goal", "now_ms", "goal_ms", "days_ahead"];

fn document() -> String {
    let doc = json!({
        "about": "Written by crates/flash-core/tests/it/vector_gen.rs; do not edit by hand.",
        "step_columns": STEP_COLUMNS,
        "day_columns": DAY_COLUMNS,
        "sequences": sequences(),
        "goal_sequences": goal_sequences(),
        "goal_plan_columns": GOAL_PLAN_COLUMNS,
        "goal_window_columns": GOAL_WINDOW_COLUMNS,
        "goals": goal_plans(),
        "days": days(),
    });
    let mut text = serde_json::to_string(&doc).unwrap();
    text.push('\n');
    text
}

#[test]
fn vectors_match_what_the_scheduler_computes() {
    let fresh = document();
    if std::env::var("FLASH_WRITE_VECTORS").as_deref() == Ok("1") {
        std::fs::write(PATH, &fresh).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(PATH).unwrap_or_default();
    let committed: Value = serde_json::from_str(&committed).unwrap_or(Value::Null);
    let fresh: Value = serde_json::from_str(&fresh).unwrap();
    let mut misses = Vec::new();
    same(&committed, &fresh, "", &mut misses);
    assert!(
        misses.is_empty(),
        "tests/scheduler_vectors_rs.json is stale: rerun with FLASH_WRITE_VECTORS=1\n{}",
        misses
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// f32 math differs between platforms' libm in the last digits (Linux
/// and Windows disagree on about 1 value in 10^7), so a file written on
/// one must still pass on the other: floats compare to 1e-5, as the
/// app's port of these vectors does, and everything else exactly.
const REL: f64 = 1e-5;
const DUE_MS: usize = 7;

fn same(want: &Value, got: &Value, at: &str, misses: &mut Vec<String>) {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) if w.len() == g.len() => {
            for (k, wv) in w {
                match g.get(k) {
                    Some(gv) if k == "steps" => same_steps(wv, gv, &format!("{at}.{k}"), misses),
                    Some(gv) => same(wv, gv, &format!("{at}.{k}"), misses),
                    None => misses.push(format!("{at}.{k}: missing")),
                }
            }
        }
        (Value::Array(w), Value::Array(g)) if w.len() == g.len() => {
            for (i, (wv, gv)) in w.iter().zip(g).enumerate() {
                same(wv, gv, &format!("{at}[{i}]"), misses);
            }
        }
        (Value::Number(w), Value::Number(g)) if w.is_f64() || g.is_f64() => {
            let (w, g) = (w.as_f64().unwrap(), g.as_f64().unwrap());
            if (g - w).abs() > REL * w.abs().max(1e-6) {
                misses.push(format!("{at}: {g} vs {w}"));
            }
        }
        _ if want == got => {}
        _ => misses.push(format!("{at}: {got} vs {want}")),
    }
}

/// A review's due time carries its f32 interval, so it may differ by
/// the same share of that interval (plus a few ms of rounding).
fn same_steps(want: &Value, got: &Value, at: &str, misses: &mut Vec<String>) {
    let (Some(w), Some(g)) = (want.as_array(), got.as_array()) else {
        return same(want, got, at, misses);
    };
    if w.len() != g.len() {
        return misses.push(format!("{at}: {} steps vs {}", g.len(), w.len()));
    }
    for (i, (wr, gr)) in w.iter().zip(g).enumerate() {
        let at = format!("{at}[{i}]");
        let due = |r: &Value| r.get(DUE_MS).and_then(Value::as_i64);
        let now = wr.get(0).and_then(Value::as_i64);
        match (due(wr), due(gr), now) {
            (Some(wd), Some(gd), Some(now)) => {
                if (gd - wd).abs() as f64 > REL * (wd - now).abs() as f64 + 8.0 {
                    misses.push(format!("{at}: due {gd} vs {wd}"));
                }
                let strip = |r: &Value| {
                    let mut r = r.clone();
                    r[DUE_MS] = Value::Null;
                    r
                };
                same(&strip(wr), &strip(gr), &at, misses);
            }
            _ => same(wr, gr, &at, misses),
        }
    }
}
