use std::collections::BTreeMap;

use super::html::{cut, shown_html};
use super::{packs_of, text, texts, translations};
use crate::i18n::packs::{Extras, Pack};
use crate::i18n::UiLocale;

/// The words of `text`, lowercase.
pub(super) fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn matches(word: &str, rule: &str) -> bool {
    match rule.strip_suffix('*') {
        Some(start) => word.starts_with(start),
        None => word == rule,
    }
}

/// Every language's text as its reader gets it (its chain's folders,
/// each rewording the one before), held to the packs of those
/// folders together (`packs`) and `extras`: its words, its punctuation,
/// its error rules. A language whose folders have no pack is held to
/// nothing here.
pub fn style(resources: &[crate::i18n::Resource], extras: Extras) -> Vec<String> {
    let mut problems = Vec::new();
    for (language, chain) in translations() {
        let packs = packs_of(chain);
        if packs.is_empty() {
            continue;
        }
        let mut read = BTreeMap::new();
        for folder in chain {
            read.extend(texts(resources, folder));
        }
        for (id, pieces) in &read {
            for piece in pieces {
                style_of(language.tag(), id, piece, &packs, extras, &mut problems);
            }
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

/// Text written outside the catalogs in the folder `folder` (a guide, a
/// privacy page, a phone's permission prompt), as its
/// reader sees it, held to what a message is: what every reader can read
/// (`text::unreadable`) and the packs of every language that reads the
/// folder. One line at a time, a line being a paragraph, a heading or a
/// field. `id` names the text in the problems and, as a message's id
/// does, says whether it speaks to readers outside the product
/// (`extras`).
pub fn prose(folder: &str, id: &str, text: &str, extras: Extras) -> Vec<String> {
    let readers: Vec<UiLocale> = UiLocale::real()
        .filter(|l| l.chain().contains(&folder))
        .collect();
    if readers.is_empty() {
        return vec![format!("`{id}`: no language reads the folder {folder}")];
    }
    let mut problems = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        for what in text::unreadable(line) {
            problems.push(format!("{folder}: `{id}`: {what}"));
        }
        for language in &readers {
            let packs = packs_of(language.chain());
            style_of(language.tag(), id, line, &packs, extras, &mut problems);
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

/// `prose` over a page in `folder`, read as `shown_html` reads it. An
/// element that says its own language (`<strong lang="es-ES">`, another
/// product's label as one variant ships it) is held to that language's
/// folder instead, and the rest of the page to `folder`'s. Code is left
/// out, as in a message shown as markup: it is no language's words (a
/// cookie's name, "flash-decks-folded", a command).
pub fn prose_html(folder: &str, id: &str, html: &str, extras: Extras) -> Vec<String> {
    let mut problems = Vec::new();
    let mut page = String::new();
    let html = cut(html, "<code", "</code>", " ");
    let mut rest = html.as_str();
    while let Some(at) = rest.find(" lang=\"") {
        let Some(open) = rest[..at].rfind('<') else {
            break;
        };
        let name: String = rest[open + 1..]
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        let value = &rest[at + 7..];
        let lang = &value[..value.find('"').unwrap_or(value.len())];
        let close = format!("</{name}>");
        let (Some(start), Some(end)) = (rest[at..].find('>'), rest[at..].find(&close)) else {
            break;
        };
        page.push_str(&rest[..open]);
        let inner = &rest[at + start + 1..at + end];
        let own_folder = UiLocale::real()
            .find(|l| l.tag() == lang)
            .and_then(|own| own.chain().last().copied())
            .or_else(|| UiLocale::folders().into_iter().find(|f| *f == lang));
        match own_folder {
            Some(own_folder) => {
                problems.extend(prose(own_folder, id, &shown_html(inner), extras));
            }
            None => problems.push(format!(
                "`{id}`: lang=\"{lang}\" is no language of the table"
            )),
        }
        page.push(' ');
        rest = &rest[at + end + close.len()..];
    }
    page.push_str(rest);
    problems.extend(prose(folder, id, &shown_html(&page), extras));
    problems.sort();
    problems.dedup();
    problems
}

fn style_of(
    tag: &str,
    id: &str,
    text: &str,
    packs: &[&Pack],
    extras: Extras,
    problems: &mut Vec<String>,
) {
    let mut say = |what: String| problems.push(format!("{tag}: `{id}`: {what}"));
    let lower = text.to_lowercase();
    let quoted = id.starts_with("ui-");
    let shown = if crate::i18n::is_html(id) {
        shown_html(&cut(text, "<code", "</code>", " "))
    } else {
        text.to_string()
    };
    let product_word = packs.iter().find_map(|p| p.product_word.as_ref());
    let prose: Vec<&str> = shown
        .split(' ')
        .filter(|t| {
            !t.char_indices()
                .any(|(i, c)| c == ',' && t[i + 1..].starts_with(char::is_alphanumeric))
        })
        .collect();
    for word in words_of(&prose.join(" ")).into_iter().filter(|_| !quoted) {
        let banned = packs.iter().flat_map(|p| p.banned.iter());
        if let Some(rule) = banned.into_iter().find(|r| matches(&word, r)) {
            say(format!("\"{word}\" (rule {rule})"));
        }
        if let Some(p) = product_word {
            let inside = !extras.is_outside(id);
            let instead = if word.starts_with(p.outside) {
                Some(p.inside)
            } else {
                packs
                    .iter()
                    .flat_map(|k| k.outside_only.iter())
                    .find(|(r, _)| matches(&word, r))
                    .map(|(_, w)| *w)
            };
            if let Some(instead) = instead.filter(|_| inside) {
                say(format!("\"{word}\" inside the product is \"{instead}\""));
            }
        }
    }
    for phrase in packs.iter().flat_map(|p| p.banned_phrases.iter()) {
        if lower.contains(phrase) {
            say(format!("\"{phrase}\""));
        }
    }
    if packs.iter().any(|p| p.inverted_marks) {
        inverted_marks(text, &mut say);
    }
    if !quoted {
        typography(&shown, packs, &mut say);
    }
    let error = id.contains("error") || id.ends_with("-failed");
    if error && packs.iter().any(|p| p.calm_errors) && (text.contains('!') || text.contains('¡')) {
        say("an error does not exclaim".into());
    }
    if let Some(start) = packs.iter().find_map(|p| p.failure_start) {
        if id.ends_with("-failed") && !lower.trim_start().starts_with(start) {
            say(format!("a failure starts with \"{}\"", capitalized(start)));
        }
    }
    for form in packs.iter().flat_map(|p| p.gendered.iter()) {
        if lower.contains(form) {
            say(format!("\"{form}\": reword for everyone"));
        }
    }
    let gendered = text.split_whitespace().any(|w| {
        let c: Vec<char> = w.chars().collect();
        !w.contains('.')
            && c.windows(3)
                .any(|t| t[0].is_alphabetic() && t[1] == '@' && t[2].is_alphabetic())
    });
    if gendered && packs.iter().any(|p| p.at_gender) {
        say("a word with @: reword for everyone".into());
    }
}

/// `text` with its first letter capital, as a rule names it.
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The no-break space, the only space French sets beside a mark.
const NBSP: char = '\u{a0}';

/// The characters `packs` refuse, and the space before the marks and
/// inside « » they ask for: a no-break space (`nbsp_before`), none
/// (`tight_before`). A mark followed by a letter, a digit, `/` or `=` is
/// part of an address or a time ("https://", "?deck=", "14:30"), a doubled
/// colon is syntax ("c1::"), and a mark that starts the text has nothing
/// before it to judge.
fn typography(text: &str, packs: &[&Pack], say: &mut impl FnMut(String)) {
    for (c, instead) in packs.iter().flat_map(|p| p.banned_chars.iter()) {
        if text.contains(*c) {
            say(format!("U+{:04X} \"{c}\": write {instead}", *c as u32));
        }
    }
    let nbsp: Vec<char> = packs
        .iter()
        .flat_map(|p| p.nbsp_before.iter().copied())
        .collect();
    let tight: Vec<char> = packs
        .iter()
        .flat_map(|p| p.tight_before.iter().copied())
        .collect();
    let guillemets = packs.iter().any(|p| p.nbsp_in_guillemets);
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let before = i.checked_sub(1).map(|b| chars[b]);
        let after = chars.get(i + 1).copied();
        let doubled = c == ':' && (before == Some(':') || after == Some(':'));
        let mark = !doubled && !after.is_some_and(|n| n.is_alphanumeric() || n == '/' || n == '=');
        if let Some(b) = before.filter(|_| mark) {
            if nbsp.contains(&c) && b != NBSP {
                say(format!("before \"{c}\" write a no-break space (U+00A0)"));
            }
            if tight.contains(&c) && b.is_whitespace() {
                say(format!("no space before \"{c}\""));
            }
        }
        let open = c == '\u{ab}' && after.is_some_and(|n| n != NBSP);
        let close = c == '\u{bb}' && before.is_some_and(|b| b != NBSP);
        if guillemets && (open || close) {
            say("inside \u{ab} \u{bb} write a no-break space (U+00A0)".into());
        }
    }
}

/// Each ¿ closed by a ? and each ¡ by a !, counted.
fn inverted_marks(text: &str, say: &mut impl FnMut(String)) {
    let chars: Vec<char> = text.chars().collect();
    let closing = |mark: char| {
        chars
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                **c == mark
                    && !chars
                        .get(i + 1)
                        .is_some_and(|n| n.is_alphanumeric() || *n == '=')
            })
            .count()
    };
    let opening = |mark: char| chars.iter().filter(|c| **c == mark).count();
    if opening('¿') != closing('?') {
        say("¿ and ? do not pair".into());
    }
    if opening('¡') != closing('!') {
        say("¡ and ! do not pair".into());
    }
}
