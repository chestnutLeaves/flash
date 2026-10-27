//! The worker for the jobs table: work owed to someone that must
//! survive a restart or a crash. Whatever makes the promise writes the
//! job (`flash_store::enqueue_job`, inside its own transaction when it
//! has one); this loop claims due jobs under a lease, runs each kind's
//! handler on a blocking thread, and records how it went: done, retried
//! later, or failed for good.
//!
//! A job whose worker died mid-run is claimed again when its lease runs
//! out, so a handler may run twice: every handler checks whether its work
//! is already done, and hands external calls a key that makes a repeat a
//! no-op (a mail's `idempotency_key`).

use std::collections::HashMap;
use std::time::Duration;

use flash_store::JobRow;

use crate::service::now_ms;
use crate::state::AppState;

/// How a handler's run went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// Worth trying again later (a mail provider that was down).
    Retry(String),
    /// Will never work (the account is gone): kept, not retried.
    Fail(String),
}

/// What runs one kind of job, on a blocking thread, at the worker's
/// `now` (ms): a handler judges "too late" and "still owed" by it, not
/// by the wall clock, so a test can run the worker at any instant.
pub type Handler = fn(&AppState, &JobRow, i64) -> Outcome;

/// How long a claimed job is held before another pass may take it,
/// longer than any handler runs (a mail send gives up after 15 s).
pub const LEASE_MS: i64 = 5 * 60 * 1000;

/// Waits after the 1st, 2nd, … failed try; after the last, the job fails.
const BACKOFF_MS: [i64; 5] = [
    60 * 1000,
    5 * 60 * 1000,
    30 * 60 * 1000,
    2 * 60 * 60 * 1000,
    12 * 60 * 60 * 1000,
];

/// Jobs taken per pass.
const BATCH: u32 = 16;

/// How often an idle worker looks for due jobs in production.
pub const POLL: Duration = Duration::from_secs(5);

/// When a job that failed its `attempts`-th try runs next, or None when
/// that was its last.
pub fn retry_at(attempts: u32, now_ms: i64) -> Option<i64> {
    let i = attempts.checked_sub(1)? as usize;
    BACKOFF_MS.get(i).map(|wait| now_ms + wait)
}

/// Whether a failure of this run fails the job for good, for a handler
/// that must undo a claim rather than leave it standing.
pub fn is_last_try(job: &JobRow) -> bool {
    retry_at(job.attempts, 0).is_none()
}

/// The kinds the open server itself owes.
fn core_handlers() -> Vec<(&'static str, Handler)> {
    vec![
        (
            flash_store::JOB_DELETE_BLOBS,
            crate::media::delete_blobs_job as Handler,
        ),
        (
            crate::oauth::JOB_OAUTH_CODE_CHECK,
            crate::oauth::oauth_code_check_job as Handler,
        ),
        (
            flash_store::JOB_EXTERNAL_MEDIA,
            crate::external_media::job as Handler,
        ),
    ]
}

fn handlers(state: &AppState) -> HashMap<&'static str, Handler> {
    core_handlers()
        .into_iter()
        .chain(state.ext.job_handlers())
        .collect()
}

/// One pass: claims what is due and runs it. Returns how many jobs ran.
pub fn run_due(state: &AppState) -> u32 {
    run_due_at(state, now_ms())
}

/// `run_due` as of `now` (ms), which the handlers see too.
pub fn run_due_at(state: &AppState, now: i64) -> u32 {
    let handlers = handlers(state);
    let store = state.services.store();
    let jobs = match store.claim_due_jobs(now, LEASE_MS, BATCH) {
        Ok(jobs) => jobs,
        Err(e) => {
            tracing::error!("jobs: claim: {e}");
            return 0;
        }
    };
    let ran = jobs.len() as u32;
    for job in jobs {
        let outcome = match handlers.get(job.kind.as_str()) {
            Some(handler) => handler(state, &job, now),
            None => Outcome::Fail(format!("no handler for {:?}", job.kind)),
        };
        let mut failed_for_good = None;
        let recorded = match &outcome {
            Outcome::Done => store.finish_job(job.id, now),
            Outcome::Retry(why) => match retry_at(job.attempts, now) {
                Some(at) => {
                    tracing::warn!(kind = ?job.kind, id = job.id, attempts = job.attempts, why = ?why, "job will retry");
                    store.retry_job(job.id, at, why)
                }
                None => {
                    tracing::error!(kind = ?job.kind, id = job.id, why = ?why, "job failed after its last try");
                    failed_for_good = Some(why);
                    store.fail_job(job.id, now, why)
                }
            },
            Outcome::Fail(why) => {
                tracing::error!(kind = ?job.kind, id = job.id, why = ?why, "job failed");
                failed_for_good = Some(why);
                store.fail_job(job.id, now, why)
            }
        };
        match recorded {
            Ok(()) => {
                if let Some(why) = failed_for_good {
                    state.ext.job_failed(store, &job, why, now);
                }
            }
            // The lease runs out and the job is claimed again.
            Err(e) => tracing::error!(id = job.id, "jobs: record outcome: {e}"),
        }
    }
    ran
}

/// The loop every binary spawns. Returns once the process is stopping
/// (`state.shutdown`), after the pass in hand; a job cut off by the stop
/// is claimed again when its lease runs out.
pub async fn run(state: AppState, poll: Duration) {
    loop {
        let pass = {
            let state = state.clone();
            tokio::task::spawn_blocking(move || run_due(&state))
        };
        // A full batch may have left more due behind it.
        if matches!(pass.await, Ok(BATCH)) && !state.shutdown.is_cancelled() {
            continue;
        }
        tokio::select! {
            _ = state.jobs_wake.notified() => {}
            _ = tokio::time::sleep(poll) => {}
            _ = state.shutdown.cancelled() => return,
        }
    }
}

/// For request paths that just wrote a job: the worker looks now rather
/// than at its next poll.
pub fn wake(state: &AppState) {
    state.jobs_wake.notify_one();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_then_stop() {
        let now = 1_000;
        assert_eq!(retry_at(1, now), Some(now + 60_000));
        assert_eq!(retry_at(5, now), Some(now + 12 * 60 * 60 * 1000));
        assert_eq!(retry_at(6, now), None);
        assert_eq!(retry_at(0, now), None);
    }
}
