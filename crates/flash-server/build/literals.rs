//! Fails the build on a language written into code: a string literal in
//! `src/` that is a language of the table (its tag, a message folder, its
//! hreflang or Open Graph locale) or one of its addresses (`/es`,
//! `/es/guia`). Code asks the table (`UiLocale`) instead, so adding a
//! language is a row and its messages, never a hunt through branches.
//!
//! Where a literal is right, the line says why, on itself or the line
//! above: `// lang-literal: <reason>`. A whole file of such literals (the
//! codes a deck's cards may be spoken in) is named by
//! the build script with its reason. A marker with no literal under it
//! fails too, so none outlives its reason. Comments are not read, nor
//! test-only code (`#[cfg(test)]` and the like).

use crate::registry::LANGUAGES;

const MARKER: &str = "lang-literal:";

/// The literals that name a language or its address, lowercase: whole
/// ones, and the starts of an address (`/es/`).
pub fn names() -> (Vec<String>, Vec<String>) {
    let mut exact = Vec::new();
    let mut prefixes = Vec::new();
    for language in LANGUAGES {
        exact.push(language.tag.to_ascii_lowercase());
        exact.push(language.hreflang.to_ascii_lowercase());
        exact.push(language.og.to_ascii_lowercase());
        exact.extend(language.chain.iter().map(|f| f.to_ascii_lowercase()));
        if !language.prefix.is_empty() {
            exact.push(language.prefix.to_ascii_lowercase());
            prefixes.push(format!("{}/", language.prefix.to_ascii_lowercase()));
        }
    }
    exact.sort();
    exact.dedup();
    (exact, prefixes)
}

/// A string literal and the line it starts on.
struct Literal {
    line: usize,
    text: String,
}

/// What a file says outside its comments and its test-only code: its
/// string literals, and the lines a marker comment sits on (with whether
/// it gives a reason).
#[derive(Default)]
struct Read {
    literals: Vec<Literal>,
    markers: Vec<(usize, bool)>,
}

/// Reads Rust source well enough to find its string literals: line and
/// block comments (nested), char literals and lifetimes, plain, byte and
/// raw strings (`r#"..."#`), and the items an attribute naming `test`
/// keeps out of a shipped build.
fn read(source: &str) -> Read {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Read::default();
    let mut i = 0;
    let mut line = 1;
    let mut depth: i64 = 0;
    let mut parens: i64 = 0;
    // An attribute keeping the next item to tests was read (at this
    // parenthesis depth); the item's braces, once open, are skipped to
    // their close. An item that ends first (`use x;`, a field `x: u8,`)
    // takes the attribute with it.
    let mut armed: Option<i64> = None;
    let mut skip_to: Option<i64> = None;
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < chars.len() {
        let c = chars[i];
        let skipping = skip_to.is_some();
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            '/' if at(i + 1) == '/' => {
                let start = i;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                let comment: String = chars[start..i].iter().collect();
                if let Some(after) = comment.find(MARKER) {
                    let reason = comment[after + MARKER.len()..].trim();
                    if !skipping {
                        out.markers.push((line, !reason.is_empty()));
                    }
                }
            }
            '/' if at(i + 1) == '*' => {
                let mut nested = 0;
                while i < chars.len() {
                    if chars[i] == '/' && at(i + 1) == '*' {
                        nested += 1;
                        i += 2;
                    } else if chars[i] == '*' && at(i + 1) == '/' {
                        nested -= 1;
                        i += 2;
                        if nested == 0 {
                            break;
                        }
                    } else {
                        if chars[i] == '\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
            }
            '#' if at(i + 1) == '[' => {
                let end = chars[i..]
                    .iter()
                    .position(|c| *c == ']')
                    .map_or(chars.len(), |p| i + p);
                let attribute: String = chars[i..end]
                    .iter()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let names_test = attribute.starts_with("#[cfg(")
                    && !attribute.contains("not(")
                    && ["(test)", "(test,", ",test)", ",test,"]
                        .iter()
                        .any(|t| attribute.contains(t));
                if names_test && !skipping {
                    armed = Some(parens);
                }
                i += 2;
            }
            '(' => {
                parens += 1;
                i += 1;
            }
            ')' => {
                parens -= 1;
                i += 1;
            }
            '\'' => {
                // A char literal ('"', '\'', '\u{e9}') or a lifetime ('a).
                if at(i + 1) == '\\' {
                    i += 3;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if at(i + 2) == '\'' {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            'r' | 'b' if is_raw_start(&chars, i) => {
                let (text, next, lines) = raw_string(&chars, i);
                if !skipping {
                    out.literals.push(Literal { line, text });
                }
                line += lines;
                i = next;
            }
            '"' => {
                let start_line = line;
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        text.push(chars[i]);
                        i += 1;
                    }
                    if chars.get(i) == Some(&'\n') {
                        line += 1;
                    }
                    if let Some(&ch) = chars.get(i) {
                        text.push(ch);
                    }
                    i += 1;
                }
                i += 1;
                if !skipping {
                    out.literals.push(Literal {
                        line: start_line,
                        text,
                    });
                }
            }
            '{' => {
                if armed.is_some() && skip_to.is_none() {
                    skip_to = Some(depth);
                    armed = None;
                }
                depth += 1;
                i += 1;
            }
            '}' => {
                depth -= 1;
                if skip_to == Some(depth) {
                    skip_to = None;
                }
                armed = None;
                i += 1;
            }
            ';' | ',' if armed == Some(parens) => {
                armed = None;
                i += 1;
            }
            c if c.is_alphanumeric() || c == '_' => {
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Whether a raw string (`r"`, `r#"`, `br"`) starts at `i`.
fn is_raw_start(chars: &[char], i: usize) -> bool {
    if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        return false;
    }
    let mut j = i;
    if chars[j] == 'b' {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return false;
    }
    j += 1;
    while chars.get(j) == Some(&'#') {
        j += 1;
    }
    chars.get(j) == Some(&'"')
}

/// A raw string starting at `i`: its text, where reading goes on, and
/// the lines it spans.
fn raw_string(chars: &[char], mut i: usize) -> (String, usize, usize) {
    if chars[i] == 'b' {
        i += 1;
    }
    i += 1;
    let mut hashes = 0;
    while chars.get(i) == Some(&'#') {
        hashes += 1;
        i += 1;
    }
    i += 1;
    let mut text = String::new();
    let mut lines = 0;
    while i < chars.len() {
        if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
            return (text, i + 1 + hashes, lines);
        }
        if chars[i] == '\n' {
            lines += 1;
        }
        text.push(chars[i]);
        i += 1;
    }
    (text, i, lines)
}

/// Scans the crate's `src/` and fails the build on a language literal
/// no marker explains. `exempt` names files or folders (`src/i18n/packs.rs`)
/// whose literals are languages by nature, each with why.
pub fn scan(exempt: &[(&str, &str)]) {
    let (exact, prefixes) = names();
    crate::tree::scan(&["src"], &["rs"], exempt, |rel, source| {
        problems_in(rel, source, &exact, &prefixes)
    });
}

/// The problems of one file, `rel`, with `source`: a literal naming a
/// language with no marker, a marker with no reason, a marker over no
/// such literal.
pub fn problems_in(rel: &str, source: &str, exact: &[String], prefixes: &[String]) -> Vec<String> {
    let found = read(source);
    let named = |text: &str| {
        // Two capitals are a region ("ES"), never a language's tag.
        let region = text.len() == 2 && text.chars().all(|c| c.is_ascii_uppercase());
        let lower = text.to_ascii_lowercase();
        !region && (exact.contains(&lower) || prefixes.iter().any(|p| lower.starts_with(p)))
    };
    let mut problems = Vec::new();
    let mut used = vec![false; found.markers.len()];
    for literal in found.literals.iter().filter(|l| named(&l.text)) {
        let marker = found
            .markers
            .iter()
            .position(|(line, _)| *line == literal.line || *line + 1 == literal.line);
        match marker {
            Some(at) => used[at] = true,
            None => problems.push(format!(
                "{rel}:{}: {:?} names a language: ask the table (flash_core::locale::UiLocale), \
                 or say why on the line or the one above: // {MARKER} <reason>",
                literal.line, literal.text
            )),
        }
    }
    for (at, (line, reasoned)) in found.markers.iter().enumerate() {
        if !reasoned {
            problems.push(format!("{rel}:{line}: `// {MARKER}` gives no reason"));
        }
        if !used[at] {
            problems.push(format!(
                "{rel}:{line}: `// {MARKER}` stands over no language literal; remove it"
            ));
        }
    }
    problems
}
