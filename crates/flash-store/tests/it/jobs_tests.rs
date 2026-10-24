//! The jobs table: a promise written once, claimed under a lease, and
//! reported done, retried or failed.

use flash_store::{JobCounts, JobState, NewJob, Store};

const NOW: i64 = 1_700_000_000_000;
const LEASE: i64 = 60_000;

fn job<'a>(kind: &'a str, key: Option<&'a str>, run_at: i64) -> NewJob<'a> {
    NewJob {
        kind,
        dedupe_key: key,
        user: None,
        payload: "{}",
        run_at,
    }
}

#[test]
fn a_promise_made_twice_is_one_job() {
    let store = Store::open_in_memory().unwrap();
    assert!(store
        .enqueue_job(&job("task", Some("task:1"), NOW), NOW)
        .unwrap());
    assert!(!store
        .enqueue_job(&job("task", Some("task:1"), NOW), NOW)
        .unwrap());
    // Without a key, every write is its own job.
    assert!(store.enqueue_job(&job("note", None, NOW), NOW).unwrap());
    assert!(store.enqueue_job(&job("note", None, NOW), NOW).unwrap());
    assert_eq!(store.claim_due_jobs(NOW, LEASE, 10).unwrap().len(), 3);
}

/// A job under a prefix is written once per window, whatever the rest
/// of its key, and again once the window has passed.
#[test]
fn a_prefixed_job_is_written_once_per_window() {
    const HOUR: i64 = 60 * 60 * 1000;
    let store = Store::open_in_memory().unwrap();
    let write = |key: &str, now: i64| {
        store
            .enqueue_job_unless_recent(&job("alarm", Some(key), now), "alarm:a:", now - HOUR, now)
            .unwrap()
    };
    assert!(write("alarm:a:1", NOW));
    assert!(!write("alarm:a:2", NOW + HOUR - 1));
    // Another prefix is its own window.
    assert!(store
        .enqueue_job_unless_recent(
            &job("alarm", Some("alarm:b:1"), NOW),
            "alarm:b:",
            NOW - HOUR,
            NOW
        )
        .unwrap());
    assert!(write("alarm:a:3", NOW + HOUR));
    // The same key twice is still one job.
    assert!(!write("alarm:a:3", NOW + 3 * HOUR));
}

#[test]
fn a_job_waits_for_its_time_and_runs_oldest_first() {
    let store = Store::open_in_memory().unwrap();
    store
        .enqueue_job(&job("later", None, NOW + 1_000), NOW)
        .unwrap();
    store.enqueue_job(&job("first", None, NOW), NOW).unwrap();
    store.enqueue_job(&job("second", None, NOW), NOW).unwrap();

    let kinds = |jobs: Vec<flash_store::JobRow>| -> Vec<String> {
        jobs.into_iter().map(|j| j.kind).collect()
    };
    assert_eq!(
        kinds(store.claim_due_jobs(NOW, LEASE, 10).unwrap()),
        ["first", "second"]
    );
    assert_eq!(
        kinds(store.claim_due_jobs(NOW + 1_000, LEASE, 10).unwrap()),
        ["later"]
    );
}

/// A worker that dies holding a job holds it only until its lease runs
/// out; then the next pass takes it, as a second attempt.
#[test]
fn a_claimed_job_is_held_until_its_lease_runs_out() {
    let store = Store::open_in_memory().unwrap();
    store.enqueue_job(&job("task", None, NOW), NOW).unwrap();
    let first = store.claim_due_jobs(NOW, LEASE, 10).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].attempts, 1);

    assert!(store
        .claim_due_jobs(NOW + LEASE - 1, LEASE, 10)
        .unwrap()
        .is_empty());
    let again = store.claim_due_jobs(NOW + LEASE, LEASE, 10).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, first[0].id);
    assert_eq!(again[0].attempts, 2);
}

#[test]
fn done_retried_and_failed_jobs() {
    let store = Store::open_in_memory().unwrap();
    for kind in ["done", "retry", "fail"] {
        store.enqueue_job(&job(kind, None, NOW), NOW).unwrap();
    }
    let jobs = store.claim_due_jobs(NOW, LEASE, 10).unwrap();
    let id = |kind: &str| jobs.iter().find(|j| j.kind == kind).unwrap().id;
    store.finish_job(id("done"), NOW).unwrap();
    store.retry_job(id("retry"), NOW + 60_000, "down").unwrap();
    store.fail_job(id("fail"), NOW, "gone").unwrap();

    assert_eq!(
        store.job_counts(NOW).unwrap(),
        JobCounts {
            due: 0,
            retrying: 1,
            failed: 1
        }
    );
    assert_eq!(
        store.job_state(id("fail")).unwrap(),
        Some(JobState {
            done_at: None,
            failed_at: Some(NOW),
            last_error: Some("gone".to_string())
        })
    );
    // A retry waits for its time, and neither finished job comes back.
    assert!(store
        .claim_due_jobs(NOW + 59_999, LEASE, 10)
        .unwrap()
        .is_empty());
    let retried = store.claim_due_jobs(NOW + 60_000, LEASE, 10).unwrap();
    assert_eq!(retried.len(), 1);
    assert_eq!(retried[0].kind, "retry");

    // Finished jobs go after a while; a failed one stays.
    assert_eq!(store.purge_done_jobs(NOW + 1).unwrap(), 1);
    assert!(store.job_state(id("done")).unwrap().is_none());
    assert!(store.job_state(id("fail")).unwrap().is_some());
}

#[test]
fn an_accounts_jobs_go_with_it() {
    let store = Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    store
        .enqueue_job(
            &NewJob {
                user: Some(user),
                ..job("task", Some("task:x"), NOW)
            },
            NOW,
        )
        .unwrap();
    store.delete_user_account(user).unwrap();
    assert!(store.claim_due_jobs(NOW, LEASE, 10).unwrap().is_empty());
}
