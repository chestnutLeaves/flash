//! Tests that cannot flake for reasons of their own. A test that opens a
//! real socket is at the mercy of whatever else the machine is doing to
//! its connections; one that sleeps hopes the work it waits for is done by
//! then, and passes without testing anything when it is not; one that
//! times out on the wall clock fails when the machine is slow. So, in
//! every crate's test code (its `tests/` and its `#[cfg(test)]` items):
//!
//! - no socket: the app and its fakes are reached in memory
//!   (`testing::connect_in_memory`, `testing::MemoryHost`,
//!   `testing::memory_listener`);
//! - no sleep: a test waits for the thing itself (`TestApp::settle`, a
//!   signal, a join), never for a while;
//! - no wall-clock wait or timeout: a test on a paused clock
//!   (`#[tokio::test(start_paused = true)]`) moves time itself and may;
//!   a test that hangs is the runner's to stop and report.
//!
//! A test that truly needs one (a browser needs a socket; a live run
//! against a real service paces itself in real time) says why on the
//! line or on the comment lines right above it: `// real-socket: <why>`,
//! `// real-wait: <why>`. A reason that excuses nothing fails too.

use flash_scan::test_code::TestCode;
use flash_scan::workspace_crates;

const SOCKETS: &[&str] = &[
    "TcpListener::bind",
    "TcpStream::connect",
    "UdpSocket::bind",
    "connect_async",
    "axum::serve(",
];

const WAITS: &[&str] = &[
    "thread::sleep",
    "time::sleep",
    "time::sleep_until",
    "time::timeout",
    "time::interval",
    "recv_timeout",
];

const TOKIO_WAITS: &[&str] = &[
    "time::sleep",
    "time::sleep_until",
    "time::timeout",
    "time::interval",
];

const SOCKET_REASON: &str = "real-socket:";
const WAIT_REASON: &str = "real-wait:";

struct Line {
    code: String,
    comment: Option<String>,
}

fn lex(text: &str) -> Vec<Line> {
    let mut open: Option<Option<usize>> = None;
    let mut out = Vec::new();
    for line in text.lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut code = String::new();
        let mut comment = None;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match open {
                Some(None) => {
                    if c == '\\' {
                        i += 1;
                    } else if c == '"' {
                        open = None;
                        code.push(c);
                    }
                }
                Some(Some(hashes)) => {
                    let closes = c == '"'
                        && chars[i + 1..]
                            .iter()
                            .take(hashes)
                            .filter(|h| **h == '#')
                            .count()
                            == hashes;
                    if closes {
                        open = None;
                        code.push(c);
                        i += hashes;
                    }
                }
                None => {
                    if c == '/' && chars.get(i + 1) == Some(&'/') {
                        comment = Some(chars[i..].iter().collect());
                        break;
                    }
                    if c == '\'' && chars.get(i + 2) == Some(&'\'') {
                        // A character literal: `'"'` opens no string.
                        code.push_str("' '");
                        i += 3;
                        continue;
                    }
                    if c == 'r' && matches!(chars.get(i + 1), Some('"' | '#')) {
                        let hashes = chars[i + 1..].iter().take_while(|h| **h == '#').count();
                        if chars.get(i + 1 + hashes) == Some(&'"') {
                            open = Some(Some(hashes));
                            code.push('"');
                            i += 2 + hashes;
                            continue;
                        }
                    }
                    if c == '"' {
                        open = Some(None);
                    }
                    code.push(c);
                }
            }
            i += 1;
        }
        out.push(Line { code, comment });
    }
    out
}

fn gives(line: &Line, marker: &str) -> bool {
    let Some(comment) = &line.comment else {
        return false;
    };
    if comment.starts_with("///") || comment.starts_with("//!") {
        return false;
    }
    comment[2..]
        .trim_start()
        .strip_prefix(marker)
        .is_some_and(|why| !why.trim().is_empty())
}

fn reason(lines: &[Line], line: usize, marker: &str) -> Option<usize> {
    if gives(&lines[line - 1], marker) {
        return Some(line);
    }
    let mut i = line - 1;
    while i > 0 && lines[i - 1].code.trim().is_empty() && lines[i - 1].comment.is_some() {
        i -= 1;
        if gives(&lines[i], marker) {
            return Some(i + 1);
        }
    }
    None
}

fn all_paused(code_: &TestCode, lines: &[Line]) -> bool {
    let tests: Vec<&str> = code_
        .lines()
        .map(|(n, _)| lines[n - 1].code.trim())
        .filter(|l| l.starts_with("#[tokio::test"))
        .collect();
    !tests.is_empty() && tests.iter().all(|l| l.contains("start_paused = true"))
}

fn offenders_in(code_: &TestCode) -> Vec<String> {
    let lines = lex(&code_.text);
    let paused = all_paused(code_, &lines);
    let mut out = Vec::new();
    let mut excused = Vec::new();
    for (n, _) in code_.lines() {
        let found = lines[n - 1].code.as_str();
        let socket = SOCKETS.iter().find(|s| found.contains(*s));
        let wait = WAITS.iter().find(|w| found.contains(*w));
        if let Some(what) = socket {
            match reason(&lines, n, SOCKET_REASON) {
                Some(at) => excused.push(at),
                None => out.push(format!(
                    "{}:{n}: `{what}` opens a real socket; reach it in memory, or say why on the line above (`// {SOCKET_REASON} <why>`)",
                    code_.rel
                )),
            }
        }
        if let Some(what) = wait {
            let on_paused_clock = paused && TOKIO_WAITS.iter().any(|w| found.contains(*w));
            if !on_paused_clock {
                match reason(&lines, n, WAIT_REASON) {
                    Some(at) => excused.push(at),
                    None => out.push(format!(
                        "{}:{n}: `{what}` waits on the wall clock; wait for the thing itself (`settle`, a signal), run on a paused clock, or say why on the line above (`// {WAIT_REASON} <why>`)",
                        code_.rel
                    )),
                }
            }
        }
    }
    for (n, _) in code_.lines() {
        let line = &lines[n - 1];
        let marked = gives(line, SOCKET_REASON) || gives(line, WAIT_REASON);
        if marked && !excused.contains(&n) {
            out.push(format!(
                "{}:{n}: a reason that excuses nothing below it",
                code_.rel
            ));
        }
    }
    out
}

#[test]
fn no_test_opens_a_socket_or_waits_on_the_clock_without_a_reason() {
    let mut offenders = Vec::new();
    let mut files = 0;
    for krate in workspace_crates() {
        for code_ in krate.test_code() {
            files += 1;
            offenders.extend(offenders_in(&code_));
        }
    }
    assert!(files > 50, "read only {files} files of test code");
    assert!(
        offenders.is_empty(),
        "tests that can flake:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_rules_catch_what_they_ban_and_pass_what_they_allow() {
    let code_ = |text: &str| TestCode {
        path: "probe.rs".into(),
        rel: "probe.rs".into(),
        text: text.into(),
        ranges: vec![(1, text.lines().count())],
    };
    let real = code_(
        "#[tokio::test]\n\
         async fn a() {\n\
         let l = TcpListener::bind(\"127.0.0.1:0\");\n\
         tokio::time::sleep(d).await;\n\
         std::thread::sleep(d);\n\
         // real-wait: paced to a live service\n\
         std::thread::sleep(d);\n\
         // real-socket: nothing below\n\
         }\n",
    );
    let found = offenders_in(&real);
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(found[0].contains(":3:") && found[0].contains("TcpListener::bind"));
    assert!(found[1].contains(":4:") && found[2].contains(":5:"));
    assert!(found[3].contains(":8:") && found[3].contains("excuses nothing"));

    let mentions = code_(
        "/// TcpListener::bind is never called here.\n\
         const S: &str = \"std::thread::sleep\";\n\
         let r = r#\"tokio::time::sleep\"#;\n\
         let q = '\"'; let t = TcpStream::connect(a);\n",
    );
    let found = offenders_in(&mentions);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains(":4:") && found[0].contains("TcpStream::connect"));

    let paused = code_(
        "#[tokio::test(start_paused = true)]\n\
         async fn a() {\n\
         tokio::time::sleep(d).await;\n\
         let _ = tokio::time::timeout(d, f).await;\n\
         }\n",
    );
    assert!(offenders_in(&paused).is_empty());
    // A paused clock is tokio's: a thread still sleeps on the wall's.
    let thread = code_(
        "#[tokio::test(start_paused = true)]\n\
         async fn a() {\n\
         std::thread::sleep(d);\n\
         }\n",
    );
    assert_eq!(offenders_in(&thread).len(), 1);
}
