//! Work the open server owes someone is a job (`flash_server::jobs`),
//! never held only by a task nobody awaits, a thread or a channel, which
//! a restart loses (`flash_scan::spawns`). A downstream extension runs
//! the same scan over its own crate.

use std::path::Path;

use flash_scan::spawns::detached;

/// (file, function, why): where the open core may start work it does not
/// await, and why a restart losing it costs nobody anything promised.
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "src/jobs.rs",
        "run",
        "the job worker's own pass over the jobs table; its handle is awaited on the next line",
    ),
    (
        "src/main.rs",
        "serve",
        "the binary's loops: housekeeping (idempotent, runs again at boot), and the job worker, whose handle `boot::serve` awaits",
    ),
    (
        "src/service.rs",
        "off_request",
        "a counter or an event written off the request (anonymous counts, account events, page views); a restart losing one costs a count, owed to nobody; tracked so a test waits for them (`settled`)",
    ),
    (
        "src/testing.rs",
        "connect_in_memory",
        "serves one in-memory test connection; test support, never shipped work",
    ),
    (
        "src/testing.rs",
        "send",
        "drives one in-memory test request's connection; test support, never shipped work",
    ),
    (
        "src/testing.rs",
        "router",
        "a memory host's accept loop and its connections, ended with the host; test support, never shipped work",
    ),
    (
        "src/testing.rs",
        "build",
        "the test harness's job worker",
    ),
];

/// The open core's crates, by directory from this one.
const CRATES: &[&str] = &["../flash-core", "../flash-store", "."];

#[test]
fn owed_work_is_never_left_to_a_detached_task() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    let mut listed_seen = Vec::new();
    for dir in CRATES {
        let krate = flash_scan::crate_at(&here.join(dir));
        for file in krate.source_files() {
            for d in detached(&file) {
                let listed = ALLOWED
                    .iter()
                    .find(|(f, func, _)| *f == file.rel && *func == d.function);
                match listed {
                    Some(entry) => listed_seen.push(*entry),
                    None => offenders.push(format!(
                        "{}/{}:{} in `{}`: {}",
                        krate.name, file.rel, d.line, d.function, d.what
                    )),
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "work started and not awaited (make it a job, await it, or list it with why a restart \
         losing it costs nobody anything):\n{}",
        offenders.join("\n")
    );
    let stale: Vec<String> = ALLOWED
        .iter()
        .filter(|entry| !listed_seen.contains(entry))
        .map(|(f, func, _)| format!("{f} `{func}`"))
        .collect();
    assert!(
        stale.is_empty(),
        "listed places that no longer start detached work (remove them):\n{}",
        stale.join("\n")
    );
}
