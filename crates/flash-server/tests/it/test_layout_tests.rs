//! Every crate's integration tests are one binary. Cargo builds each
//! top-level `tests/*.rs` file, and each `tests/<dir>/main.rs`, as a
//! binary of its own, with its own link and its own few hundred
//! megabytes on disk; a new test file that lands beside `tests/it/`
//! instead of inside it still passes, and quietly brings that cost back.

use flash_scan::{relative, workspace_crates};

#[test]
fn every_crate_has_at_most_one_test_binary() {
    let mut offenders = Vec::new();
    for krate in workspace_crates() {
        let tests = krate.dir.join("tests");
        let Ok(entries) = std::fs::read_dir(&tests) else {
            continue;
        };
        let binaries: Vec<String> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().is_some_and(|e| e == "rs") || path.join("main.rs").is_file()
            })
            .map(|path| relative(&krate.dir, &path))
            .collect();
        if binaries.len() > 1 {
            offenders.push(format!("{}: {binaries:?}", krate.name));
        }
    }
    assert!(
        offenders.is_empty(),
        "one test binary per crate: make each file a `mod` in tests/it/main.rs\n{}",
        offenders.join("\n")
    );
}
