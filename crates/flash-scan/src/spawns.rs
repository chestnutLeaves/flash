//! The detached-work scan: work the server owes someone is written down
//! before it is done (a job), never held only by a task nobody awaits, a
//! thread, or a channel, which a restart or a crash loses. This finds
//! every place a file starts work it does not wait for: a `tokio::spawn`
//! or `spawn_blocking` whose statement never awaits it, the same through
//! a value (`handle.spawn(..)`, a task tracker's `spawn_blocking(..)`),
//! any `std::thread::spawn`, and any `std::sync::mpsc` channel. Each crate's
//! test lists the places that are allowed, with why a restart losing the
//! work costs nobody anything they were promised.

use crate::{Call, SourceFile};

/// One place that starts work it does not wait for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Detached {
    pub line: usize,
    /// The enclosing function, empty at module level.
    pub function: String,
    /// What starts it: the callee, or `std::sync::mpsc`.
    pub what: String,
}

const SPAWNS: &[&[&str]] = &[
    &["tokio", "spawn"],
    &["task", "spawn_blocking"],
    &["thread", "spawn"],
];

fn is_spawn(call: &Call) -> bool {
    SPAWNS.iter().any(|tail| call.ends_with(tail))
}

/// Whether the statement awaits what the call returns: an `.await` in
/// the statement outside the call's own arguments (the spawned block
/// awaiting its own work does not wait for the task).
fn awaited(stmt: &str, args: &[String]) -> bool {
    let mut rest = stmt.to_string();
    for arg in args {
        rest = rest.replacen(arg.as_str(), "", 1);
    }
    rest.contains(".await")
}

/// The places in `file` that start work nobody waits for.
pub fn detached(file: &SourceFile) -> Vec<Detached> {
    let mut out = Vec::new();
    for call in file.calls.iter().filter(|c| is_spawn(c)) {
        // A thread is never awaited; a task is when its statement is.
        if call.ends_with(&["thread", "spawn"]) || !awaited(&call.stmt, &call.args) {
            out.push(Detached {
                line: call.line,
                function: call.function.clone().unwrap_or_default(),
                what: call.callee.clone(),
            });
        }
    }
    // A scoped thread (`std::thread::scope(|s| s.spawn(..))`) is joined
    // before the scope returns: the closure's parameter, in its function.
    let scopes: Vec<(Option<String>, String)> = file
        .calls
        .iter()
        .filter(|c| c.ends_with(&["thread", "scope"]))
        .filter_map(|c| {
            let param = c.args.first()?.strip_prefix('|')?.split('|').next()?;
            Some((c.function.clone(), param.trim().to_string()))
        })
        .collect();
    for call in &file.method_calls {
        let scoped = scopes
            .iter()
            .any(|(f, name)| *f == call.function && *name == call.receiver);
        if matches!(call.method.as_str(), "spawn" | "spawn_blocking")
            && !awaited(&call.stmt, &call.args)
            && !scoped
        {
            out.push(Detached {
                line: call.line,
                function: call.function.clone().unwrap_or_default(),
                what: format!("{}.{}", call.receiver, call.method),
            });
        }
    }
    for path in &file.paths {
        if path.contains_run(&["sync", "mpsc"]) && !path.contains_run(&["tokio", "sync", "mpsc"]) {
            out.push(Detached {
                line: path.line,
                function: path.function.clone().unwrap_or_default(),
                what: "std::sync::mpsc".into(),
            });
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_source;

    #[test]
    fn awaited_work_is_told_from_detached_work() {
        let file = parse_source(
            "snippet.rs",
            r#"
            async fn f(state: AppState) {
                tokio::spawn(send_mail(state.clone()));
                let n = tokio::task::spawn_blocking(move || count()).await;
                let _ = task::spawn_blocking(move || write());
                std::thread::spawn(move || loop {});
                let (tx, rx) = std::sync::mpsc::channel::<u32>();
                handle.spawn(watch());
                let done = tracker.spawn_blocking(move || write()).await;
                std::thread::scope(|scope| { scope.spawn(|| read()); });
                let (up, down) = tokio::sync::mpsc::channel::<u32>(1);
                tokio::spawn(async move { send(up).await });
            }
            "#,
        );
        let found = detached(&file);
        let whats: Vec<&str> = found.iter().map(|d| d.what.as_str()).collect();
        assert_eq!(found.len(), 6, "{whats:?}");
        assert!(whats.contains(&"handle.spawn"), "{whats:?}");
        assert!(found.iter().all(|d| d.function == "f"));
        assert!(whats.contains(&"std::sync::mpsc"));
    }
}
