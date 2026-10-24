//! Fails the build on a style that names a side of the screen: `left`
//! or `right` where a right-to-left reader needs the other one. A page
//! read in Arabic or Hebrew is mirrored by the browser wherever its
//! styles say start and end (`margin-inline-start`, `text-align: start`,
//! `inset-inline-end`); a style saying left stays on the left and breaks
//! the mirror. So the stylesheets, the `<style>` blocks of the templates
//! and their `style="..."` attributes say start and end.
//!
//! Where a side is right in either direction (a chart's time axis runs
//! left to right in every language, a box centred with `left: 50%`), the
//! declaration says why, in a comment just before it:
//! `/* physical: <reason> */` in CSS, `{# physical: <reason> #}` in a
//! template outside its styles. A marker excuses the one declaration
//! that follows it; a marker over a declaration that names no side
//! fails, so none outlives its reason.
//!
//! Two more things a page needs to turn: its `<html>` says its `lang`,
//! `dir` and script (`data-script`), and an arrow pointing a side
//! (`&larr;`, `→`) is the whole of an element classed `fl-dir`, which
//! turns it. The angle quotes ‹ › need neither: Unicode mirrors them in
//! right-to-left text.
//!
//! And what the page's script decides is not set the same for every
//! script: a literal letter-spacing or an uppercase transform is refused,
//! since tracking breaks Arabic's joins and case means nothing to most
//! scripts; both come from the tokens the script sets
//! (`i18n::scripts`).

const MARKER: &str = "physical:";

/// What the scan met, in the order of the file.
#[derive(Debug, PartialEq)]
enum Seen {
    /// A marker comment, with whether it gives a reason.
    Marker { line: usize, reasoned: bool },
    /// A declaration (`margin-left: 4px`), lowercase, trimmed.
    Declaration {
        line: usize,
        property: String,
        value: String,
    },
    /// An HTML `align` attribute's value.
    Align { line: usize, value: String },
}

/// The sides a declaration names, or None for one that names none.
fn side(property: &str, value: &str) -> Option<&'static str> {
    let value = value.trim_end_matches("!important").trim();
    let words = top_level_words(value);
    let says_side = || {
        words
            .iter()
            .any(|w| w.eq_ignore_ascii_case("left") || w.eq_ignore_ascii_case("right"))
    };
    match property {
        "left" | "right" => Some("use inset-inline-start or inset-inline-end"),
        "margin-left" | "margin-right" => Some("use margin-inline-start or margin-inline-end"),
        "padding-left" | "padding-right" => Some("use padding-inline-start or padding-inline-end"),
        "scroll-margin-left" | "scroll-margin-right" => Some("use scroll-margin-inline-*"),
        "scroll-padding-left" | "scroll-padding-right" => Some("use scroll-padding-inline-*"),
        p if p.starts_with("border-left") || p.starts_with("border-right") => {
            Some("use border-inline-start or border-inline-end")
        }
        "border-top-left-radius"
        | "border-top-right-radius"
        | "border-bottom-left-radius"
        | "border-bottom-right-radius" => Some("use border-start-start-radius and its kin"),
        "float" | "clear" if says_side() => Some("use inline-start or inline-end"),
        "text-align" | "text-align-last" if says_side() => Some("use start or end"),
        "background-position" | "background-position-x" | "background" | "mask-position"
            if says_side() =>
        {
            Some("place it from the start, or mark why the side holds")
        }
        "margin" | "padding" | "border-width" | "border-style" | "border-color" | "inset"
        | "scroll-margin" | "scroll-padding"
            if words.len() == 4 && words[1] != words[3] =>
        {
            Some("its right and left differ: give the sides with the -inline- longhands")
        }
        "border-radius" => {
            // The corners before any `/`: `a b c d` is top-left,
            // top-right, bottom-right, bottom-left; it mirrors onto
            // itself when each row's two corners agree.
            let horizontal: Vec<&str> = words.split(|w| *w == "/").next().unwrap_or(&[]).to_vec();
            let symmetric = match horizontal.as_slice() {
                [] | [_] => true,
                [a, b] => a == b,
                [a, b, c] => a == b && b == c,
                [a, b, c, d, ..] => a == b && c == d,
            };
            (!symmetric)
                .then_some("its corners differ by side: use border-start-start-radius and its kin")
        }
        _ => None,
    }
}

/// What a declaration should say instead, when it sets tracking or case
/// for every script alike: letter-spacing breaks Arabic's joins (Safari
/// still, WebKit bug 195745) and an uppercase label means nothing in a
/// script without case, so both come from the tokens the page's script
/// sets (`i18n::scripts`). A value a template fills (a mail's
/// `{{ "-0.2px"|tracking }}`) has its own say.
fn untyped(property: &str, value: &str) -> Option<&'static str> {
    let value = value.trim_end_matches("!important").trim();
    match property {
        "letter-spacing" => {
            let zero = matches!(
                value,
                "0" | "0px" | "0em" | "normal" | "inherit" | "initial"
            );
            let tokened = value.contains("var(--");
            let literal = value.chars().any(|c| c.is_ascii_digit());
            (literal && !zero && !tokened)
                .then_some("take it from a --track-* token (or, in a mail, the `tracking` filter)")
        }
        "text-transform" => ["uppercase", "capitalize", "lowercase", "full-width"]
            .iter()
            .any(|t| value.contains(t))
            .then_some("take it from var(--label-case)"),
        _ => None,
    }
}

/// A value's words outside parentheses and quotes (`url(...)`, `calc()`
/// stay one word each).
fn top_level_words(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut start: Option<usize> = None;
    for (i, c) in value.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => {
                quote = Some(c);
                start.get_or_insert(i);
            }
            (None, '(') => {
                depth += 1;
                start.get_or_insert(i);
            }
            (None, ')') => depth = depth.saturating_sub(1),
            (None, c) if c.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() {
                    words.push(&value[s..i]);
                }
            }
            (None, '/') if depth == 0 => {
                if let Some(s) = start.take() {
                    words.push(&value[s..i]);
                }
                words.push("/");
            }
            (None, _) => {
                start.get_or_insert(i);
            }
        }
    }
    if let Some(s) = start {
        words.push(&value[s..]);
    }
    words
}

/// Reads CSS starting at `from` in `chars` (a stylesheet, a `<style>`
/// block or a `style` attribute's value, `inline` for the last): its
/// marker comments and its declarations, with their lines counted from
/// `line`. A piece ended by `{` is a selector or an at-rule, never a
/// declaration; `;` inside quotes or parentheses (a data URL) ends
/// nothing.
fn read_css(text: &str, mut line: usize, inline: bool, out: &mut Vec<Seen>) {
    let chars: Vec<char> = text.chars().collect();
    let mut piece = String::new();
    let mut piece_line = line;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut i = 0;
    let flush = |piece: &mut String, piece_line: usize, out: &mut Vec<Seen>| {
        if let Some((property, value)) = piece.split_once(':') {
            let property = property.trim().to_ascii_lowercase();
            if !property.is_empty()
                && property
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                out.push(Seen::Declaration {
                    line: piece_line,
                    property,
                    value: value.trim().to_ascii_lowercase(),
                });
            }
        }
        piece.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
        }
        if quote.is_none() && c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = i + 2;
            let mut end = start;
            while end + 1 < chars.len() && !(chars[end] == '*' && chars[end + 1] == '/') {
                end += 1;
            }
            let comment: String = chars[start..end.min(chars.len())].iter().collect();
            if let Some(at) = comment.find(MARKER) {
                out.push(Seen::Marker {
                    line,
                    reasoned: !comment[at + MARKER.len()..].trim().is_empty(),
                });
            }
            line += comment.matches('\n').count();
            i = end + 2;
            continue;
        }
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, '{') if depth == 0 && !inline => {
                piece.clear();
                i += 1;
                continue;
            }
            (None, ';' | '}') if depth == 0 => {
                flush(&mut piece, piece_line, out);
                i += 1;
                continue;
            }
            _ => {}
        }
        if piece.trim().is_empty() {
            piece.clear();
            piece_line = line;
        }
        piece.push(c);
        i += 1;
    }
    flush(&mut piece, piece_line, out);
}

/// A template with its Askama tags blanked out, newlines kept, so CSS
/// around `{{ value }}` reads as CSS; `{# ... #}` comments become CSS
/// marker comments when they are markers.
fn blank_tags(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('{') {
        let (before, tail) = rest.split_at(at);
        out.push_str(before);
        let close = match tail.get(..2) {
            Some("{{") => "}}",
            Some("{%") => "%}",
            Some("{#") => "#}",
            _ => {
                out.push('{');
                rest = &tail[1..];
                continue;
            }
        };
        let end = tail.find(close).map_or(tail.len(), |e| e + 2);
        let tag = &tail[..end];
        if close == "#}" && tag.contains(MARKER) {
            out.push_str("/*");
            out.push_str(&tag[2..tag.len().saturating_sub(2)]);
            out.push_str("*/");
        } else {
            out.push_str(if close == "}}" { "x" } else { "" });
        }
        out.extend(std::iter::repeat_n('\n', tag.matches('\n').count()));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// What an HTML file (a template, the panel) says in CSS: its `<style>`
/// blocks, its `style` attributes and its `align` attributes, with the
/// markers between them, in order.
fn read_html(source: &str) -> Vec<Seen> {
    let text = blank_tags(source);
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut i = 0;
    let line_at = |at: usize| text[..at].matches('\n').count() + 1;
    while i < text.len() {
        let next_style_block = lower[i..].find("<style").map(|p| p + i);
        let next_attr = find_attr(&lower, i);
        let next_marker = text[i..].find("/*").map(|p| p + i);
        let next = [
            next_style_block.map(|p| (p, 0)),
            next_attr.map(|(p, _)| (p, 1)),
            next_marker.map(|p| (p, 2)),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some((at, kind)) = next else { break };
        match kind {
            0 => {
                let open_end = lower[at..].find('>').map_or(text.len(), |p| at + p + 1);
                let close = lower[open_end..]
                    .find("</style")
                    .map_or(text.len(), |p| open_end + p);
                read_css(&text[open_end..close], line_at(open_end), false, &mut out);
                i = close + 1;
            }
            1 => {
                let (value_start, name) = next_attr.expect("found above");
                let quote = text[value_start..].chars().next().unwrap_or('"');
                let body_start = value_start + 1;
                let end = text[body_start..]
                    .find(quote)
                    .map_or(text.len(), |p| body_start + p);
                let value = &text[body_start..end];
                if name == "style" {
                    read_css(value, line_at(body_start), true, &mut out);
                } else {
                    out.push(Seen::Align {
                        line: line_at(body_start),
                        value: value.trim().to_ascii_lowercase(),
                    });
                }
                i = end + 1;
            }
            _ => {
                let end = text[at..].find("*/").map_or(text.len(), |p| at + p + 2);
                read_css(&text[at..end], line_at(at), true, &mut out);
                i = end;
            }
        }
    }
    out
}

/// The next ` style="` or ` align="` attribute at or after `from` in
/// `lower`: where its quoted value starts (at the quote), and its name.
fn find_attr(lower: &str, from: usize) -> Option<(usize, &'static str)> {
    ["style", "align"]
        .iter()
        .filter_map(|name| {
            let mut at = from;
            loop {
                let p = lower[at..].find(name)? + at;
                let before = lower[..p].chars().next_back();
                let after = lower[p + name.len()..].trim_start();
                let quoted = after
                    .strip_prefix('=')
                    .map(str::trim_start)
                    .and_then(|v| v.chars().next())
                    .is_some_and(|q| q == '"' || q == '\'');
                if before.is_some_and(char::is_whitespace) && quoted {
                    let eq = lower[p + name.len()..].find('=')? + p + name.len();
                    let value = lower[eq + 1..].len() - lower[eq + 1..].trim_start().len();
                    return Some((eq + 1 + value, *name));
                }
                at = p + name.len();
            }
        })
        .min_by_key(|(p, _)| *p)
}

/// The arrows that point a side of the screen, which Unicode does not
/// mirror in right-to-left text (it mirrors ‹ › by itself).
const ARROWS: [&str; 4] = ["&larr;", "&rarr;", "\u{2190}", "\u{2192}"];

/// Each arrow in an HTML file that is not the whole of an element
/// classed `fl-dir`, which turns it for a right-to-left reader.
fn loose_arrows(rel: &str, source: &str) -> Vec<String> {
    let text = blank_tags(source);
    let mut problems = Vec::new();
    for arrow in ARROWS {
        for (at, _) in text.match_indices(arrow) {
            let open = text[..at].rfind('<').map(|lt| &text[lt..at]);
            let closes = text[at + arrow.len()..].trim_start().starts_with("</");
            let wrapped = open.is_some_and(|tag| {
                !tag.starts_with("</") && tag.ends_with('>') && tag.contains("fl-dir")
            });
            if !(wrapped && closes) {
                let line = text[..at].matches('\n').count() + 1;
                problems.push(format!(
                    "{rel}:{line}: `{arrow}` points a side of the screen, which a \
                     right-to-left reader needs turned: make it the whole of \
                     <span class=\"fl-dir\" aria-hidden=\"true\">"
                ));
            }
        }
    }
    problems
}

/// Each `<html>` that does not say its page's language and direction:
/// the browser mirrors a page only where its root says `dir="rtl"`. A
/// page also says its script (`data-script`), which sets its fonts and
/// type; a mail's layout (`email_*`) carries its styles inline instead.
fn undirected_pages(rel: &str, source: &str) -> Vec<String> {
    let lower = blank_tags(source).to_ascii_lowercase();
    let mail = rel
        .rsplit('/')
        .next()
        .is_some_and(|f| f.starts_with("email_"));
    lower
        .match_indices("<html")
        .filter(|(at, _)| {
            lower[at + 5..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '>')
        })
        .filter_map(|(at, _)| {
            let tag = &lower[at..lower[at..].find('>').map_or(lower.len(), |e| at + e)];
            let says = tag.contains(" lang=")
                && tag.contains(" dir=")
                && (mail || tag.contains(" data-script="));
            (!says).then(|| {
                let line = lower[..at].matches('\n').count() + 1;
                format!(
                    "{rel}:{line}: <html> says no lang, dir and data-script: \
                     <html lang=\"{{{{ \"\"|html_lang }}}}\" dir=\"{{{{ \"\"|html_dir }}}}\" \
                     data-script=\"{{{{ \"\"|html_script }}}}\">"
                )
            })
        })
        .collect()
}

/// The problems of one file, `rel`, with `source`: a side no marker
/// explains, a marker with no reason, a marker over no side, an arrow
/// that does not turn.
pub fn problems_in(rel: &str, source: &str) -> Vec<String> {
    let seen = if rel.ends_with(".css") {
        let mut out = Vec::new();
        read_css(source, 1, false, &mut out);
        out
    } else {
        read_html(source)
    };
    let mut problems = if rel.ends_with(".html") {
        let mut found = loose_arrows(rel, source);
        found.extend(undirected_pages(rel, source));
        found
    } else {
        Vec::new()
    };
    let mut pending: Option<usize> = None;
    for item in &seen {
        match item {
            Seen::Marker { line, reasoned } => {
                if let Some(earlier) = pending {
                    problems.push(format!(
                        "{rel}:{earlier}: `{MARKER}` stands over no side; remove it"
                    ));
                }
                if !reasoned {
                    problems.push(format!("{rel}:{line}: `{MARKER}` gives no reason"));
                }
                pending = Some(*line);
            }
            Seen::Declaration {
                line,
                property,
                value,
            } => {
                if let Some(fix) = untyped(property, value) {
                    problems.push(format!(
                        "{rel}:{line}: `{property}: {value}` is set the same for every \
                         script, which a script without case or a cursive one cannot \
                         take: {fix}"
                    ));
                }
                let found = side(property, value);
                match (found, pending.take()) {
                    (Some(_), Some(_)) => {}
                    (Some(fix), None) => problems.push(format!(
                        "{rel}:{line}: `{property}: {value}` names a side of the screen, \
                         which a right-to-left reader needs mirrored: {fix}, or say why \
                         just before it: /* {MARKER} <reason> */"
                    )),
                    (None, Some(marker)) => problems.push(format!(
                        "{rel}:{marker}: `{MARKER}` stands over `{property}`, which names \
                         no side; remove it"
                    )),
                    (None, None) => {}
                }
            }
            Seen::Align { line, value } => {
                let physical = value == "left" || value == "right";
                match (physical, pending.take()) {
                    (true, Some(_)) | (false, None) => {}
                    (true, None) => problems.push(format!(
                        "{rel}:{line}: `align=\"{value}\"` names a side of the screen: \
                         take it from the reader's direction, or say why just before it"
                    )),
                    (false, Some(marker)) => problems.push(format!(
                        "{rel}:{marker}: `{MARKER}` stands over an `align` naming no \
                         side; remove it"
                    )),
                }
            }
        }
    }
    if let Some(line) = pending {
        problems.push(format!(
            "{rel}:{line}: `{MARKER}` stands over no side; remove it"
        ));
    }
    problems
}

/// Scans the crate's stylesheets and HTML under `roots` (`static`,
/// `templates`) and fails the build on a side no marker explains.
/// `exempt` names files or folders (`static/katex/`) the scan leaves
/// alone, each with why.
pub fn scan(roots: &[&str], exempt: &[(&str, &str)]) {
    crate::tree::scan(roots, &["css", "html"], exempt, problems_in);
}
