use std::collections::{BTreeMap, BTreeSet};

use fluent_syntax::ast;

pub use super::packs::Extras;
use super::packs::{Pack, PACKS};
use super::UiLocale;

mod doors;
mod html;
mod plurals;
mod style;
pub mod text;

pub use doors::doors;
pub use html::{raw_text, shown_html};
pub use plurals::plural_problems;
use style::words_of;
pub use style::{prose, prose_html, style};

type Messages = BTreeMap<String, BTreeSet<String>>;

/// The variable a client passes each of its messages (`Extras::client_ids`).
const PLATFORM: &str = "platform";

/// English's folder, the one every message starts in.
fn english() -> &'static str {
    UiLocale::EN.chain()[0]
}

/// Every language but English, with its chain of message folders.
fn translations() -> impl Iterator<Item = (UiLocale, &'static [&'static str])> {
    UiLocale::real()
        .filter(|l| *l != UiLocale::EN)
        .map(|l| (l, l.chain()))
}

/// The packs of a language's folders, base first.
fn packs_of(chain: &[&str]) -> Vec<&'static Pack> {
    chain
        .iter()
        .filter_map(|folder| PACKS.iter().find(|p| p.folder == *folder))
        .collect()
}

/// The messages of the files of `resources` in the folder `dir` (in
/// every folder when None), each with its folder. A file that does not
/// parse is passed over: `messages` says so.
fn folder_messages<'a>(
    resources: &'a [super::Resource],
    dir: Option<&'a str>,
) -> impl Iterator<Item = (&'static str, ast::Message<&'static str>)> + 'a {
    resources
        .iter()
        .filter(move |(d, _)| dir.is_none_or(|dir| *d == dir))
        .filter_map(|(d, text)| Some((*d, fluent_syntax::parser::parse(*text).ok()?)))
        .flat_map(|(d, resource)| {
            resource
                .body
                .into_iter()
                .filter_map(move |entry| match entry {
                    ast::Entry::Message(m) => Some((d, m)),
                    _ => None,
                })
        })
}

/// Every message of `resources` in the folder `dir`, with the
/// variables it uses (its attributes as `id.attr`), and the problems
/// found reading them.
fn messages(resources: &[super::Resource], dir: &str, problems: &mut Vec<String>) -> Messages {
    let mut out = Messages::new();
    for (d, text) in resources.iter().filter(|(d, _)| *d == dir) {
        let resource = match fluent_syntax::parser::parse(*text) {
            Ok(r) => r,
            Err((r, errors)) => {
                problems.push(format!("{d}: does not parse: {errors:?}"));
                r
            }
        };
        for entry in &resource.body {
            match entry {
                ast::Entry::Message(m) => {
                    let id = m.id.name.to_string();
                    if dir == english() && m.comment.is_none() {
                        problems.push(format!("{dir}: `{id}` has no note saying where it shows"));
                    }
                    let mut vars = BTreeSet::new();
                    if let Some(p) = &m.value {
                        pattern_vars(p, &mut vars);
                        let markup = p.elements.iter().any(|e| {
                            matches!(e, ast::PatternElement::TextElement { value } if value.contains('<'))
                        });
                        if markup && !super::is_html(&id) {
                            problems.push(format!(
                                "{d}: `{id}` holds markup but its id does not end in -html"
                            ));
                        }
                    }
                    for a in &m.attributes {
                        pattern_vars(&a.value, &mut vars);
                        out.insert(format!("{id}.{}", a.id.name), BTreeSet::new());
                    }
                    let overrides = m.comment.as_ref().is_some_and(|c| {
                        c.content.iter().any(|l| l.trim().starts_with("overrides:"))
                    });
                    match (out.insert(id.clone(), vars).is_some(), overrides) {
                        (true, false) => problems.push(format!("{d}: `{id}` is defined twice")),
                        (false, true) => {
                            problems.push(format!("{d}: `{id}` overrides no earlier message"))
                        }
                        _ => {}
                    }
                }
                ast::Entry::Junk { content } => {
                    problems.push(format!("{d}: unreadable: {content}"));
                }
                _ => {}
            }
        }
    }
    out
}

/// The variables English's note offers a translation beyond its own: a
/// note line `spare: $hour, ...` names one the code passes for languages
/// whose grammar needs it (Spanish writes "la 1" but "las 2") and English
/// does not show.
fn spare_vars(resources: &[super::Resource]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for (_, m) in folder_messages(resources, Some(english())) {
        let spare: BTreeSet<String> = m
            .comment
            .iter()
            .flat_map(|c| c.content.iter())
            .filter_map(|line| line.trim().strip_prefix("spare: $"))
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect()
            })
            .collect();
        if !spare.is_empty() {
            out.insert(m.id.name.to_string(), spare);
        }
    }
    out
}

fn pattern_vars(p: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
    for e in &p.elements {
        if let ast::PatternElement::Placeable { expression } = e {
            expression_vars(expression, out);
        }
    }
}

fn expression_vars(e: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
    match e {
        ast::Expression::Select { selector, variants } => {
            inline_vars(selector, out);
            for v in variants {
                pattern_vars(&v.value, out);
            }
        }
        ast::Expression::Inline(i) => inline_vars(i, out),
    }
}

fn inline_vars(i: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match i {
        ast::InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        ast::InlineExpression::Placeable { expression } => expression_vars(expression, out),
        _ => {}
    }
}

/// The problems with `resources`, given the templates in `templates`
/// (every `"id"|t`, `|tn` or `|ta` they hold must exist in English) and
/// what a deployment's `extras` add.
/// Every folder of every language's chain: each message one English
/// has, with English's variables; a folder after the first of its
/// chain (a variant) rewords only messages the folders before it have.
pub fn check(
    resources: &[super::Resource],
    templates: &[std::path::PathBuf],
    extras: Extras,
) -> Vec<String> {
    let mut problems = Vec::new();
    let en = messages(resources, english(), &mut problems);
    let spare = spare_vars(resources);
    for (id, names) in &spare {
        for name in names {
            if en.get(id).is_some_and(|v| v.contains(name)) {
                problems.push(format!(
                    "en: `{id}` shows ${name}, which its note calls spare"
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for (_, chain) in translations() {
        for (at, &dir) in chain.iter().enumerate() {
            if !seen.insert(dir) {
                continue;
            }
            let local = messages(resources, dir, &mut problems);
            let below: Vec<Messages> = chain[..at]
                .iter()
                .map(|d| messages(resources, d, &mut Vec::new()))
                .collect();
            for (id, vars) in &local {
                let mut vars = vars.clone();
                if extras.is_client(id) {
                    vars.remove(PLATFORM);
                }
                for name in spare.get(id).into_iter().flatten() {
                    vars.remove(name);
                }
                match en.get(id) {
                    None => problems.push(format!("{dir}: `{id}` is not an English message")),
                    Some(en_vars) if *en_vars != vars => {
                        problems.push(format!("{dir}: `{id}` uses {vars:?}, English {en_vars:?}"))
                    }
                    _ => {}
                }
                if at > 0 && !below.iter().any(|m| m.contains_key(id)) {
                    problems.push(format!(
                        "{dir}: `{id}` rewords a message {} lacks",
                        chain[..at].join(", ")
                    ));
                }
            }
        }
    }
    problems.extend(plural_problems(resources));
    problems.extend(text::text_problems(resources, extras));
    let asked = asked_ids(templates);
    for (file, id, safe) in asked {
        if !en.contains_key(&id) {
            problems.push(format!("{file}: asks for `{id}`, which English lacks"));
        }
        if super::is_html(&id) != safe {
            problems.push(format!(
                "{file}: `{id}` is shown {} but its id {} in -html",
                if safe { "with |safe" } else { "escaped" },
                if safe { "does not end" } else { "ends" },
            ));
        }
    }
    problems
}

/// The script messages (`js-*`, `panel-*`, a client's) of `resources`
/// that a script could not word: anything beyond text, variables and
/// choices.
pub fn script_problems(resources: &[super::Resource], extras: Extras) -> Vec<String> {
    const SCRIPT_PREFIXES: &[&str] = &["js-", "panel-", "interval-"];
    let mut problems = Vec::new();
    for (dir, m) in folder_messages(resources, None) {
        let id = m.id.name;
        if !SCRIPT_PREFIXES.iter().any(|p| id.starts_with(p)) && !extras.is_client(id) {
            continue;
        }
        if let Some(value) = &m.value {
            if let Err(why) = super::islands::script_parts(value) {
                problems.push(format!("{dir}: `{id}`: {why}"));
            }
        }
    }
    problems
}

/// The text of every message (and attribute, as `id.attr`) of
/// `resources` in the folder `dir`: its pieces of text and string
/// literals, each variant of a choice as its own piece, variables
/// left out.
fn texts(resources: &[super::Resource], dir: &str) -> BTreeMap<String, Vec<String>> {
    fn pieces(p: &ast::Pattern<&str>, out: &mut Vec<String>) {
        let mut current = String::new();
        let mut own = String::new();
        let mut chooses = false;
        for e in &p.elements {
            match e {
                ast::PatternElement::TextElement { value } => {
                    current.push_str(value);
                    own.push_str(value);
                }
                ast::PatternElement::Placeable { expression } => match expression {
                    ast::Expression::Inline(ast::InlineExpression::StringLiteral { value }) => {
                        let value = fluent_syntax::unicode::unescape_unicode_to_string(value);
                        current.push_str(&value);
                        own.push_str(&value);
                    }
                    ast::Expression::Select { variants, .. } => {
                        chooses = true;
                        for v in variants {
                            pieces(&v.value, out);
                        }
                        current.push_str("{}");
                    }
                    _ => {
                        current.push_str("{}");
                        own.push_str("{}");
                    }
                },
            }
        }
        if !chooses || !own.trim().is_empty() {
            out.push(current);
        }
    }
    let mut out = BTreeMap::new();
    for (_, m) in folder_messages(resources, Some(dir)) {
        let id = m.id.name.to_string();
        if let Some(p) = &m.value {
            let mut found = Vec::new();
            pieces(p, &mut found);
            out.insert(id.clone(), found);
        }
        for a in &m.attributes {
            let mut found = Vec::new();
            pieces(&a.value, &mut found);
            out.insert(format!("{id}.{}", a.id.name), found);
        }
    }
    out
}

/// What each language still lacks of `resources`, read as its reader
/// gets it (its chain's folders together): an English message with
/// none of its own, and one left as its English (a word of four
/// letters or more the two share that is no word its packs or `extras`
/// allow, `same_as_english`). The messages whose ids start with one of
/// `english_only` belong to pages written in English only, and have
/// none.
pub fn missing(
    resources: &[super::Resource],
    english_only: &[&str],
    extras: Extras,
) -> Vec<String> {
    let en = texts(resources, english());
    let mut problems = Vec::new();
    for (language, chain) in translations() {
        let tag = language.tag();
        let mut read = BTreeMap::new();
        for folder in chain {
            read.extend(texts(resources, folder));
        }
        let same_ok: Vec<&str> = packs_of(chain)
            .iter()
            .flat_map(|p| p.same_as_english.iter().copied())
            .chain(extras.same_as_english.iter().copied())
            .collect();
        for (id, english) in &en {
            if english_only.iter().any(|p| id.starts_with(p)) {
                if read.contains_key(id) {
                    problems.push(format!(
                        "{tag}: `{id}` is on a page written in English only"
                    ));
                }
                continue;
            }
            let Some(own) = read.get(id) else {
                problems.push(format!("{tag}: `{id}` has no message of its own"));
                continue;
            };
            // Another product's label may ship untranslated (`ui-*`),
            // and a standard's names are its own (Argentina is Argentina).
            let same = english == own
                && !id.starts_with("ui-")
                && !extras.is_standard(id)
                && english.iter().any(|piece| {
                    words_of(piece)
                        .iter()
                        .any(|w| w.len() >= 4 && !same_ok.contains(&w.as_str()))
                });
            if same {
                problems.push(format!("{tag}: `{id}` is still its English"));
            }
        }
    }
    problems
}

/// What is wrong with the packs themselves: one named twice, or one
/// for a folder no language reads.
pub fn pack_problems() -> Vec<String> {
    let known = UiLocale::folders();
    let mut problems = Vec::new();
    for (at, pack) in PACKS.iter().enumerate() {
        if PACKS[..at].iter().any(|p| p.folder == pack.folder) {
            problems.push(format!("a pack for {} twice", pack.folder));
        }
        if !known.contains(&pack.folder) {
            problems.push(format!(
                "a pack for {}, which no language reads",
                pack.folder
            ));
        }
    }
    problems
}

/// Every message id a template asks for: a string literal right
/// before `|t`, `|tn(` or `|ta(`.
/// Each with whether the expression it sits in ends in `|safe`.
fn asked_ids(templates: &[std::path::PathBuf]) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for path in templates {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("\"|t") {
            let before = &rest[..at];
            if let Some(open) = before.rfind('"') {
                let id = &before[open + 1..];
                let after = &rest[at + 3..];
                let filter_ends = after.starts_with(' ')
                    || after.starts_with('}')
                    || after.starts_with('|')
                    || after.starts_with("n(")
                    || after.starts_with("a(")
                    || after.starts_with("a2(")
                    || after.starts_with("n2(")
                    || after.starts_with("n3(")
                    || after.starts_with("na(")
                    || after.starts_with("na2(");
                if filter_ends && !id.is_empty() {
                    let expression = &after[..after.find("}}").unwrap_or(after.len())];
                    let safe = expression.contains("|safe");
                    out.push((name.clone(), id.to_string(), safe));
                }
            }
            rest = &rest[at + 3..];
        }
    }
    out
}
