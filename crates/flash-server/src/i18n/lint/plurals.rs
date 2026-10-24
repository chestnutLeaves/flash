use std::collections::BTreeSet;

use fluent_syntax::ast;

use super::{folder_messages, spare_vars};
use crate::i18n::UiLocale;

/// CLDR's plural categories.
const CATEGORIES: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// The plural categories of the languages that read `folder`; None
/// when they disagree (a folder shared by languages whose numbers
/// take different forms cannot word a count for both).
fn categories_of(folder: &str) -> Option<BTreeSet<&'static str>> {
    let mut found: Option<BTreeSet<&'static str>> = None;
    for locale in UiLocale::real().filter(|l| l.chain().contains(&folder)) {
        let own: BTreeSet<&'static str> = locale.plural_categories().collect();
        match &found {
            Some(seen) if *seen != own => return None,
            _ => found = Some(own),
        }
    }
    found
}

/// The plural choices of `p`, nested ones included, each as its
/// variants' keys (a category or an exact number) and its default's:
/// every choice on a number but one on a variable in `exact` (a spare
/// variable chosen on by its value, such as the hour in Spanish "la 1",
/// "las 2").
fn plural_choices(
    p: &ast::Pattern<&str>,
    exact: &BTreeSet<String>,
    out: &mut Vec<(Vec<String>, String)>,
) {
    for e in &p.elements {
        let ast::PatternElement::Placeable { expression } = e else {
            continue;
        };
        let ast::Expression::Select { selector, variants } = expression else {
            continue;
        };
        if let ast::InlineExpression::VariableReference { id } = selector {
            if exact.contains(id.name) {
                for v in variants {
                    plural_choices(&v.value, exact, out);
                }
                continue;
            }
        }
        let key = |v: &ast::Variant<&str>| match &v.key {
            ast::VariantKey::Identifier { name } => name.to_string(),
            ast::VariantKey::NumberLiteral { value } => value.to_string(),
        };
        let keys: Vec<String> = variants.iter().map(key).collect();
        let default = variants
            .iter()
            .find(|v| v.default)
            .map(key)
            .unwrap_or_default();
        let numeric = |k: &String| k.parse::<f64>().is_ok();
        if keys
            .iter()
            .all(|k| numeric(k) || CATEGORIES.contains(&k.as_str()))
        {
            out.push((keys, default));
        }
        for v in variants {
            plural_choices(&v.value, exact, out);
        }
    }
}

/// Every plural choice of every folder names exactly its language's
/// plural categories (CLDR's, as the table carries them: Spanish's
/// one, many and other; Arabic's six), exact numbers besides, and
/// falls back to `other`.
pub fn plural_problems(resources: &[crate::i18n::Resource]) -> Vec<String> {
    let mut problems = Vec::new();
    let spare = spare_vars(resources);
    let none = BTreeSet::new();
    for folder in UiLocale::folders() {
        let Some(wanted) = categories_of(folder) else {
            problems.push(format!(
                "{folder}: the languages that read it take different plural forms"
            ));
            continue;
        };
        for (_, m) in folder_messages(resources, Some(folder)) {
            let mut choices = Vec::new();
            let exact = spare.get(m.id.name).unwrap_or(&none);
            if let Some(p) = &m.value {
                plural_choices(p, exact, &mut choices);
            }
            for a in &m.attributes {
                plural_choices(&a.value, exact, &mut choices);
            }
            for (keys, default) in choices {
                let named: BTreeSet<&str> = keys
                    .iter()
                    .map(String::as_str)
                    .filter(|k| CATEGORIES.contains(k))
                    .collect();
                if named != wanted {
                    problems.push(format!(
                        "{folder}: `{}` chooses among {named:?}; its language's forms are {wanted:?}",
                        m.id.name
                    ));
                }
                if default != "other" {
                    problems.push(format!(
                        "{folder}: `{}` falls back to [{default}], not *[other]",
                        m.id.name
                    ));
                }
            }
        }
    }
    problems
}
