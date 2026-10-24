//! What a message's text may hold, beyond its words: characters that
//! would read wrong for some readers, markup that must stay English's,
//! the notes and caps English sets for translators, and the warnings a
//! translation answers for (each fails until the translated file accepts
//! it, saying why).

use std::collections::{BTreeMap, BTreeSet};

use super::{english, folder_messages, messages, packs_of, texts, Extras};
use crate::i18n::UiLocale;

/// A kind of warning, as an `# accept:` comment names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Warning {
    /// Longer than English by more than its length band allows.
    Length,
    /// Names a product or service English does not, or leaves one out.
    Names,
    /// Lacks the word its language's glossary gives for an English term.
    Glossary,
}

impl Warning {
    /// Its name and what it says of a message.
    fn says(self) -> (&'static str, &'static str) {
        match self {
            Warning::Length => ("length", "runs longer than English's length band allows"),
            Warning::Names => ("names", "names other products or services than English"),
            Warning::Glossary => ("glossary", "lacks its language's word for an English term"),
        }
    }

    fn name(self) -> &'static str {
        self.says().0
    }

    fn parse(name: &str) -> Option<Warning> {
        [Warning::Length, Warning::Names, Warning::Glossary]
            .into_iter()
            .find(|w| w.name() == name)
    }
}

/// Every message's note (its `#` comment), by id, in the folder `dir`.
fn notes(resources: &[crate::i18n::Resource], dir: &str) -> BTreeMap<String, String> {
    folder_messages(resources, Some(dir))
        .map(|(_, m)| {
            let note = m
                .comment
                .as_ref()
                .map(|c| c.content.join("\n"))
                .unwrap_or_default();
            (m.id.name.to_string(), note)
        })
        .collect()
}

/// The arrows that keep pointing the same way for a reader whose lines
/// run right to left (Unicode's Arrows block and the long arrows; none is
/// Bidi_Mirrored). The angle quotes (‹ › « ») and < > are mirrored, so a
/// menu path written with › turns with its line.
fn is_unmirrored_arrow(c: char) -> bool {
    matches!(c, '\u{2190}'..='\u{21FF}' | '\u{27F5}'..='\u{27FF}')
}

/// The marks that set a direction by hand. Isolating a reader's own text
/// is the page's to do when it is shown, never the catalog's.
fn is_direction_mark(c: char) -> bool {
    matches!(
        c,
        '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

/// What no reader takes as meant, whatever the language: an arrow that
/// keeps pointing the same way, a direction mark, an em dash.
pub(super) fn unreadable(piece: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(c) = piece.chars().find(|c| is_unmirrored_arrow(*c)) {
        out.push(format!(
            "\"{c}\" keeps pointing the same way when lines run right to left; write \u{203A} or words"
        ));
    }
    if piece.chars().any(is_direction_mark) {
        out.push("a direction mark; isolating text is the page's to do".into());
    }
    let spelled = entities(piece).into_iter().any(|e| {
        let e = e.to_ascii_lowercase();
        ["mdash", "#8212", "#x2014", "horbar"].contains(&e.as_str())
    });
    if piece.contains(['\u{2014}', '\u{2015}']) || spelled {
        out.push("an em dash".into());
    }
    out
}

/// The entities a message shown as markup may write.
const KNOWN_ENTITIES: &[&str] = &["amp", "lt", "gt", "quot", "apos", "nbsp", "#39", "#34"];

/// The entities `text` writes (`&name;`), each as its name.
fn entities(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices('&') {
        let rest = &text[at + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '#')
            .collect();
        if !name.is_empty() && rest[name.len()..].starts_with(';') {
            out.push(name);
        }
    }
    out
}

/// The kinds of tag `text` holds, each as its name and its attributes
/// (`href` and `id` with their values, since a link and a script hook
/// must stay English's; any other by name only): what a translation of a
/// message shown as markup keeps of English's. A kind may repeat (a
/// sentence split in two dims both periods), never be added or dropped.
fn tags(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            break;
        };
        let inner = &after[..close];
        rest = &after[close + 1..];
        let closing = inner.starts_with('/');
        let inner = inner.trim_start_matches('/').trim_end_matches('/');
        let mut parts = inner.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default().to_ascii_lowercase();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            continue;
        }
        let mut attributes: Vec<String> = Vec::new();
        let mut attrs = parts.next().unwrap_or_default().trim();
        while !attrs.is_empty() {
            let key: String = attrs
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '=')
                .collect();
            attrs = attrs[key.len()..].trim_start();
            let mut value = None;
            if let Some(after_eq) = attrs.strip_prefix('=') {
                let after_eq = after_eq.trim_start();
                if let Some(quoted) = after_eq.strip_prefix('"') {
                    let end = quoted.find('"').unwrap_or(quoted.len());
                    value = Some(quoted[..end].to_string());
                    attrs = quoted.get(end + 1..).unwrap_or_default().trim_start();
                } else {
                    let end = after_eq.find(char::is_whitespace).unwrap_or(after_eq.len());
                    value = Some(after_eq[..end].to_string());
                    attrs = after_eq[end..].trim_start();
                }
            }
            if key.is_empty() {
                break;
            }
            let key = key.to_ascii_lowercase();
            attributes.push(match value {
                Some(v) if key == "href" || key == "id" => format!("{key}={v}"),
                _ => key,
            });
        }
        attributes.sort();
        out.insert(format!(
            "{}{name}[{}]",
            if closing { "/" } else { "" },
            attributes.join(",")
        ));
    }
    out
}

/// The variables a note names (`$count`).
fn named_in(note: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (at, _) in note.match_indices('$') {
        let name: String = note[at + 1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

/// The cap a note sets on a message's length ("max 24"), in characters.
fn cap(note: &str) -> Option<usize> {
    let lower = note.to_ascii_lowercase();
    let at = lower.find("max ")?;
    let digits: String = lower[at + 4..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// A piece of text as a reader sees its length: a placeable (`{}`)
/// counts as nothing, since what fills it is the same in every language.
fn shown_length(piece: &str) -> usize {
    piece.replace("{}", "").chars().count()
}

/// How much longer than English a translation may run before it answers
/// for it: the upper end of the expansion IBM's Guidelines to design
/// global solutions give for text translated from English into European
/// languages, by the English text's length (as the W3C's "Text size in
/// translation" reproduces it: up to 10 characters 200-300%, 11-20
/// 180-200%, 21-30 160-180%, 31-50 140-160%, 51-70 151-170%, over 70
/// 130%).
fn length_allowed(english: usize) -> f64 {
    let factor = match english {
        0..=10 => 3.0,
        11..=20 => 2.0,
        21..=30 => 1.8,
        31..=50 => 1.6,
        51..=70 => 1.7,
        _ => 1.3,
    };
    english as f64 * factor
}

/// The products and services a message may name, which a translation
/// names exactly as English does.
const NAMES: &[&str] = &[
    "Flash", "Anki", "ChatGPT", "Claude", "Gemini", "Grok", "Mistral", "Le Chat", "Quizlet",
    "Google", "Apple", "YouTube", "FSRS", "iPhone", "Android", "Codex", "Cursor",
];

/// The names `text` holds (whole words).
fn names_in(text: &str, extra: &[&'static str]) -> BTreeSet<&'static str> {
    NAMES
        .iter()
        .chain(extra)
        .copied()
        .filter(|name| {
            text.match_indices(name).any(|(at, _)| {
                let before = text[..at].chars().next_back();
                let after = text[at + name.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            })
        })
        .collect()
}

/// The `# accept: <kind>: <reason>` lines of a note, as (warning, kind as
/// written, whether a reason is given); a kind no warning has is None.
fn accepts(note: &str) -> Vec<(Option<Warning>, String, bool)> {
    note.lines()
        .filter_map(|line| line.trim().strip_prefix("accept:"))
        .map(|rest| {
            let (kind, reason) = rest.split_once(':').unwrap_or((rest, ""));
            let kind = kind.trim().to_string();
            (Warning::parse(&kind), kind, !reason.trim().is_empty())
        })
        .collect()
}

/// The problems of what the messages of `resources` hold: errors, and
/// warnings no `# accept:` answers.
pub fn text_problems(resources: &[crate::i18n::Resource], extras: Extras) -> Vec<String> {
    let mut problems = Vec::new();
    for (dir, text) in resources {
        for line in text.lines() {
            let Some((id, rest)) = line.split_once(" =") else {
                continue;
            };
            let is_id = id.starts_with(|c: char| c.is_ascii_lowercase())
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if is_id && !rest.is_empty() && !rest.starts_with(' ') {
                problems.push(format!(
                    "{dir}: `{id}` is written `{id} =`; write `{id} = ` and its text"
                ));
            }
        }
    }
    let en_texts = texts(resources, english());
    let en_notes = notes(resources, english());
    let en_vars = messages(resources, english(), &mut Vec::new());

    for (id, vars) in &en_vars {
        if id.contains('.') {
            continue;
        }
        let note = en_notes.get(id).map(String::as_str).unwrap_or_default();
        let named = named_in(note);
        let unnamed: Vec<&String> = vars.iter().filter(|v| !named.contains(*v)).collect();
        if !unnamed.is_empty() {
            problems.push(format!(
                "{}: `{id}`'s note does not say what {} is",
                english(),
                unnamed
                    .iter()
                    .map(|v| format!("${v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    for dir in UiLocale::folders() {
        let own = texts(resources, dir);
        let own_notes = notes(resources, dir);
        let translated = dir != english();
        let packs: Vec<_> = UiLocale::real()
            .filter(|l| l.chain().contains(&dir))
            .flat_map(|l| packs_of(l.chain()))
            .collect();
        let mut raised: BTreeSet<(String, Warning)> = BTreeSet::new();
        for (id, pieces) in &own {
            let message = id.split('.').next().unwrap_or(id);
            let html = crate::i18n::is_html(message);
            for piece in pieces {
                for what in unreadable(piece) {
                    problems.push(format!("{dir}: `{id}`: {what}"));
                }
                for entity in entities(piece) {
                    if !html {
                        problems.push(format!(
                            "{dir}: `{id}`: &{entity}; in a message shown as text reads as written"
                        ));
                    } else if !KNOWN_ENTITIES.contains(&entity.to_ascii_lowercase().as_str()) {
                        problems.push(format!("{dir}: `{id}`: an unknown entity, &{entity};"));
                    }
                }
            }
            if let Some(max) = en_notes.get(message).and_then(|n| cap(n)) {
                if let Some(long) = pieces.iter().find(|p| shown_length(p) > max) {
                    problems.push(format!(
                        "{dir}: `{id}` is {} characters, over the note's max {max}: \"{long}\"",
                        shown_length(long)
                    ));
                }
            }
            let Some(english_pieces) = en_texts.get(id).filter(|_| translated) else {
                continue;
            };
            if html {
                let wanted: BTreeSet<BTreeSet<String>> =
                    english_pieces.iter().map(|p| tags(p)).collect();
                if let Some(piece) = pieces.iter().find(|p| !wanted.contains(&tags(p))) {
                    problems.push(format!(
                        "{dir}: `{id}`'s markup is not English's: {:?}, English {:?}",
                        tags(piece),
                        wanted.iter().next().cloned().unwrap_or_default()
                    ));
                }
            }
            if extras.is_standard(message) {
                continue;
            }
            let longest = |ps: &[String]| ps.iter().map(|p| shown_length(p)).max().unwrap_or(0);
            let (en_len, own_len) = (longest(english_pieces), longest(pieces));
            if en_len > 0 && own_len as f64 > length_allowed(en_len) {
                raised.insert((message.to_string(), Warning::Length));
            }
            let joined = |ps: &[String]| ps.join(" ");
            if names_in(&joined(english_pieces), extras.names)
                != names_in(&joined(pieces), extras.names)
            {
                raised.insert((message.to_string(), Warning::Names));
            }
            let en_words = super::words_of(&joined(english_pieces).to_lowercase()).join(" ");
            let own_lower = joined(pieces).to_lowercase();
            let lacking = packs
                .iter()
                .flat_map(|p| p.glossary.iter())
                .any(|(term, word)| {
                    en_words.contains(&term.to_lowercase())
                        && !own_lower.contains(&word.to_lowercase())
                });
            if lacking {
                raised.insert((message.to_string(), Warning::Glossary));
            }
        }
        for (id, note) in &own_notes {
            for (kind, written, reasoned) in accepts(note) {
                match kind {
                    None => problems.push(format!(
                        "{dir}: `{id}` accepts \"{written}\", which no warning is called"
                    )),
                    Some(kind) if !raised.contains(&(id.clone(), kind)) => problems.push(format!(
                        "{dir}: `{id}` accepts {} but raises no such warning; remove the accept",
                        kind.name()
                    )),
                    Some(_) if !reasoned => {
                        problems.push(format!("{dir}: `{id}`'s accept gives no reason"))
                    }
                    Some(kind) => {
                        raised.remove(&(id.clone(), kind));
                    }
                }
            }
        }
        for (id, kind) in raised {
            let (name, why) = kind.says();
            problems.push(format!(
                "{dir}: `{id}` {why} (warning: reword it, or accept it in the file above the message: # accept: {name}: <why>)"
            ));
        }
    }
    problems
}
