//! The browser errors the pages report: counted per day and page and
//! browser, new only after a quiet week, capped per day, summed per
//! error, and gone after the purge.

use flash_store::{
    ClientErrorNew, ClientErrorRecorded, Store, CLIENT_ERROR_QUIET_MS, CLIENT_ERROR_ROWS_PER_DAY,
};

const DAY: i64 = 24 * 60 * 60 * 1000;
/// Noon on some day, so an hour either way stays on it.
const NOON: i64 = 19_000 * DAY + DAY / 2;

fn report<'a>(message: &'a str, page: &'a str, browser: &'a str) -> ClientErrorNew<'a> {
    ClientErrorNew {
        kind: "error",
        file: "/static/study.js",
        line: 120,
        message,
        page,
        browser,
    }
}

#[test]
fn an_error_is_new_once_then_counted() {
    let store = Store::open_in_memory().unwrap();
    let e = report("TypeError: x is null", "/study", "Safari (mobile)");
    assert_eq!(
        store.record_client_error(&e, NOON).unwrap(),
        ClientErrorRecorded::New
    );
    assert_eq!(
        store.record_client_error(&e, NOON + 1).unwrap(),
        ClientErrorRecorded::Again
    );
    // Another page or browser is another row of the same error, not a
    // new error.
    let elsewhere = report("TypeError: x is null", "/decks/{id}", "Chrome");
    assert_eq!(
        store.record_client_error(&elsewhere, NOON + 2).unwrap(),
        ClientErrorRecorded::Again
    );
    let all = store.client_errors_since(NOON - DAY).unwrap();
    assert_eq!(all.len(), 1);
    let one = &all[0];
    assert_eq!(one.count, 3);
    assert_eq!(one.browsers, ["Chrome", "Safari (mobile)"]);
    // The page reported most comes first.
    assert_eq!(one.pages, ["/study", "/decks/{id}"]);
    assert_eq!((one.first_at, one.last_at), (NOON, NOON + 2));
    assert_eq!((one.file.as_str(), one.line), ("/static/study.js", 120));
}

#[test]
fn an_error_quiet_for_a_week_is_new_again() {
    let store = Store::open_in_memory().unwrap();
    let e = report("ReferenceError: y", "/study", "Firefox");
    store.record_client_error(&e, NOON).unwrap();
    // Within the week: still known.
    assert_eq!(
        store.record_client_error(&e, NOON + 3 * DAY).unwrap(),
        ClientErrorRecorded::Again
    );
    // A week after it was last seen: new, and an alert for the moment
    // names it, though its older rows are still kept.
    let again = NOON + 3 * DAY + CLIENT_ERROR_QUIET_MS + 1;
    assert_eq!(
        store.record_client_error(&e, again).unwrap(),
        ClientErrorRecorded::New
    );
    let new = store
        .new_client_errors_between(again - 1, again + 1)
        .unwrap();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].count, 1, "counted from the window on");
}

#[test]
fn a_full_day_counts_what_it_has_and_takes_nothing_new() {
    let store = Store::open_in_memory().unwrap();
    let messages: Vec<String> = (0..CLIENT_ERROR_ROWS_PER_DAY)
        .map(|n| format!("invented {n}"))
        .collect();
    for message in &messages {
        store
            .record_client_error(&report(message, "/study", "Chrome"), NOON)
            .unwrap();
    }
    assert_eq!(
        store
            .record_client_error(&report("one more", "/study", "Chrome"), NOON)
            .unwrap(),
        ClientErrorRecorded::Dropped
    );
    // One already in today's rows still counts.
    assert_eq!(
        store
            .record_client_error(&report(&messages[0], "/study", "Chrome"), NOON)
            .unwrap(),
        ClientErrorRecorded::Again
    );
    // Tomorrow has room again.
    assert_eq!(
        store
            .record_client_error(&report("one more", "/study", "Chrome"), NOON + DAY)
            .unwrap(),
        ClientErrorRecorded::New
    );
}

#[test]
fn the_new_errors_of_a_window_are_the_ones_first_seen_in_it() {
    let store = Store::open_in_memory().unwrap();
    let window = 15 * 60 * 1000;
    let old = report("old", "/study", "Chrome");
    let fresh = report("fresh", "/study", "Chrome");
    let later = report("later", "/study", "Chrome");
    store.record_client_error(&old, NOON - 2 * DAY).unwrap();
    // Seen before the window: not new in it, though it comes back in it.
    store.record_client_error(&old, NOON + 1).unwrap();
    store.record_client_error(&fresh, NOON + 2).unwrap();
    store.record_client_error(&later, NOON + window).unwrap();
    let new: Vec<String> = store
        .new_client_errors_between(NOON, NOON + window)
        .unwrap()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(new, ["fresh"]);
}

#[test]
fn the_purge_takes_rows_last_counted_before_its_cutoff() {
    let store = Store::open_in_memory().unwrap();
    store
        .record_client_error(&report("gone", "/study", "Chrome"), NOON - 40 * DAY)
        .unwrap();
    store
        .record_client_error(&report("kept", "/study", "Chrome"), NOON)
        .unwrap();
    assert_eq!(store.purge_client_errors(NOON - 30 * DAY).unwrap(), 1);
    let left: Vec<String> = store
        .client_errors_since(0)
        .unwrap()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(left, ["kept"]);
}
