//! No hidden state shared by everything in the process. A `static`
//! that can change (a lock, a cell, an atomic, a thread or task local)
//! is read by every server, job and test the process runs, so one of
//! them can change what another sees. State belongs to what owns it
//! (`AppState`, `Services`, the extension), passed to whoever needs it.
//!
//! A static that must change stays only with a reason beside it, on the
//! comment lines right above it: `// global-state: <why>`. Nothing may be
//! `static mut`. A value built once and never changed (`OnceLock`,
//! `LazyLock` of a plain value) is a constant and needs no reason. A
//! marker that excuses nothing fails too, so none outlives its static.
//! Every crate of the workspace is read, by its parsed items rather than
//! by text, so a static inside a function counts as one at the top.

use flash_scan::workspace_crates;

const CHANGEABLE: &[&str] = &[
    "Mutex",
    "RwLock",
    "RefCell",
    "Cell",
    "UnsafeCell",
    "OnceCell",
    "Notify",
    "Semaphore",
    "ArcSwap",
    "DashMap",
];

const LOCALS: &[&str] = &["thread_local", "task_local"];

const MARKER: &str = "// global-state:";

fn changeable(idents: &[String]) -> bool {
    idents
        .iter()
        .any(|i| CHANGEABLE.contains(&i.as_str()) || i.starts_with("Atomic"))
}

fn lines_above(text: &[&str], line: usize) -> std::ops::Range<usize> {
    let mut start = line - 1;
    while start > 0 {
        let above = text[start - 1].trim_start();
        if above.starts_with("//") || above.starts_with("#[") {
            start -= 1;
        } else {
            break;
        }
    }
    start..line - 1
}

fn reason(text: &[&str], above: &std::ops::Range<usize>) -> Option<usize> {
    above.clone().find(|&i| {
        text[i]
            .trim_start()
            .strip_prefix(MARKER)
            .is_some_and(|why| !why.trim().is_empty())
    })
}

#[test]
fn no_static_changes_without_a_reason_beside_it() {
    let mut offenders = Vec::new();
    let mut seen = 0;
    for krate in workspace_crates() {
        for file in krate.source_files() {
            let rel = format!("{}/{}", krate.name, file.rel);
            let source = std::fs::read_to_string(&file.path).unwrap();
            let text: Vec<&str> = source.lines().collect();
            let mut excused = Vec::new();
            let mut needs = |what: String, line: usize, offenders: &mut Vec<String>| {
                match reason(&text, &lines_above(&text, line)) {
                    Some(at) => excused.push(at),
                    None => offenders.push(format!(
                        "{rel}:{line}: {what} is shared by the whole process; give it to what owns it, or say why not on the line above (`{MARKER} <why>`)"
                    )),
                }
            };
            for s in &file.statics {
                seen += 1;
                if s.mutable {
                    offenders.push(format!(
                        "{rel}:{}: `static mut {}` (never; own the value instead)",
                        s.line, s.name
                    ));
                } else if changeable(&s.ty.idents) {
                    needs(
                        format!("`static {}: {}`", s.name, s.ty.compact),
                        s.line,
                        &mut offenders,
                    );
                }
            }
            for m in &file.macros {
                if LOCALS.contains(&m.last()) {
                    seen += 1;
                    needs(format!("`{}!`", m.last()), m.line, &mut offenders);
                }
            }
            for (i, l) in text.iter().enumerate() {
                if l.trim_start().starts_with(MARKER) && !excused.contains(&i) {
                    offenders.push(format!(
                        "{rel}:{}: a `{MARKER}` reason with no changeable static below it",
                        i + 1
                    ));
                }
            }
        }
    }
    assert!(seen > 5, "found only {seen} statics; is the scan reading?");
    assert!(
        offenders.is_empty(),
        "process-wide state:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_scan_finds_what_it_bans() {
    let file = flash_scan::parse_source(
        "probe.rs",
        "static A: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);\n\
         static mut B: u8 = 0;\n\
         fn f() { static C: parking_lot::Mutex<u8> = parking_lot::Mutex::new(0); }\n\
         static D: std::sync::LazyLock<String> = std::sync::LazyLock::new(String::new);\n\
         thread_local! { static E: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }\n",
    );
    let found: Vec<(&str, bool, bool, Option<&str>)> = file
        .statics
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.mutable,
                changeable(&s.ty.idents),
                s.function.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        found,
        vec![
            ("A", false, true, None),
            ("B", true, false, None),
            ("C", false, true, Some("f")),
            ("D", false, false, None),
        ]
    );
    assert!(file.macros.iter().any(|m| m.last() == "thread_local"));
}
