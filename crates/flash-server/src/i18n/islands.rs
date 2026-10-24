//! The words a page's scripts say.

use flash_core::locale::{Numbers, UiLocale};

use super::{Catalog, Tr};

/// One message as a page's script words it (`static/i18n.js`): a list of
/// parts, each a piece of text, a variable (`{"v": name}`), or a choice
/// on one variable (`{"s": name, "k": [[key, parts], ...], "d": key}`)
/// whose keys are plural categories, exact numbers or words, and whose
/// variants are parts again, so a choice may hold another (Spanish picks
/// "la" or "las" by the hour inside each plural form). Anything else (a
/// function, a term) is refused: a message a script says keeps to what
/// i18n.js and the app know.
pub(super) fn script_parts(
    pattern: &fluent_syntax::ast::Pattern<&str>,
) -> Result<serde_json::Value, String> {
    use fluent_syntax::ast::{Expression, InlineExpression, PatternElement, VariantKey};
    use fluent_syntax::unicode::unescape_unicode_to_string;
    let mut parts = Vec::new();
    for element in &pattern.elements {
        match element {
            PatternElement::TextElement { value } => parts.push(serde_json::Value::from(*value)),
            PatternElement::Placeable { expression } => match expression {
                Expression::Inline(InlineExpression::VariableReference { id }) => {
                    parts.push(serde_json::json!({ "v": id.name }));
                }
                Expression::Inline(InlineExpression::StringLiteral { value }) => {
                    parts.push(serde_json::Value::from(unescape_unicode_to_string(value)));
                }
                Expression::Select { selector, variants } => {
                    let InlineExpression::VariableReference { id } = selector else {
                        return Err("a choice must be on a variable".into());
                    };
                    let mut keys = Vec::new();
                    let mut default = String::new();
                    for v in variants {
                        let key = match &v.key {
                            VariantKey::Identifier { name } => name.to_string(),
                            VariantKey::NumberLiteral { value } => value.to_string(),
                        };
                        if v.default {
                            default = key.clone();
                        }
                        keys.push(serde_json::json!([key, script_parts(&v.value)?]));
                    }
                    parts.push(serde_json::json!({ "s": id.name, "k": keys, "d": default }));
                }
                _ => return Err("only text, variables and choices".into()),
            },
        }
    }
    Ok(serde_json::Value::Array(parts))
}

/// The folders `locale`'s words are read from, in order: English's, then
/// the language's own chain, each rewording the ones before it.
pub fn reading_order(locale: UiLocale) -> Vec<&'static str> {
    let root = UiLocale::EN.chain()[0];
    std::iter::once(root)
        .chain(locale.chain().iter().copied().filter(|f| *f != root))
        .collect()
}

/// A language's plural rules as a page's scripts evaluate them
/// (static/i18n.js): each category and CLDR's condition for it, the
/// sample numbers left out.
fn plural_conditions(locale: UiLocale) -> Vec<serde_json::Value> {
    locale
        .plurals()
        .iter()
        .map(|p| {
            let condition = p.rule.split('@').next().unwrap_or_default().trim();
            serde_json::json!({ "category": p.category, "rule": condition })
        })
        .collect()
}

/// A JSON island for a page's scripts: `<script type="application/json"
/// data-flash-{kind}>`, nothing in it able to close the element early.
pub(crate) fn island(kind: &str, json: &serde_json::Value) -> String {
    let text = json.to_string().replace('<', "\\u003c");
    format!(r#"<script type="application/json" data-flash-{kind}>{text}</script>"#)
}

/// A folder's island of messages (`data-flash-i18n`).
pub fn folder_island(folder: &str, messages: serde_json::Map<String, serde_json::Value>) -> String {
    island(
        "i18n",
        &serde_json::json!({ "folder": folder, "m": messages }),
    )
}

impl Catalog {
    /// The messages of one message folder whose ids start with one of
    /// `prefixes`, as a page's scripts word them: the folder's own only,
    /// so a reader's words are their folders' tables read in order
    /// (`reading_order`). A message outside what scripts can word is left
    /// out and logged (the tests hold every one to it).
    pub fn folder_table(
        &self,
        folder: &str,
        prefixes: &[&str],
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut found = serde_json::Map::new();
        for (_, resource) in self.layers.iter().filter(|(dir, _)| *dir == folder) {
            for entry in resource.entries() {
                let fluent_syntax::ast::Entry::Message(m) = entry else {
                    continue;
                };
                if !prefixes.iter().any(|p| m.id.name.starts_with(p)) {
                    continue;
                }
                let Some(value) = &m.value else { continue };
                match script_parts(value) {
                    Ok(parts) => {
                        found.insert(m.id.name.to_string(), parts);
                    }
                    Err(why) => tracing::warn!(id = m.id.name, "a script's message: {why}"),
                }
            }
        }
        found
    }
}

impl Tr {
    /// The island that tells a page's scripts what they read in
    /// (`<script type="application/json" data-flash-lang>`,
    /// static/i18n.js): the language, the folders its words are read
    /// from in order, the region numbers are written for (none where the
    /// language writes them plainly), its direction and its script.
    pub fn script_header(&self) -> String {
        island(
            "lang",
            &serde_json::json!({
                "lang": self.html_lang(),
                "folders": reading_order(self.locale),
                "format": (self.locale.numbers() == Numbers::Cldr).then(|| self.format_tag.clone()),
                "dir": self.locale.dir().as_str(),
                "script": self.locale.script(),
                "plurals": plural_conditions(self.locale),
                "pseudo": self.locale.pseudo(),
                "lists": self.catalog.script_lists(self.locale),
            }),
        )
    }

    /// What gives a page's scripts their words (`static/i18n.js`): its
    /// header (`script_header`) and an island per folder of the
    /// messages whose ids start with one of `prefixes`, each message
    /// carried once, in the folder its reader takes it from.
    pub fn script_island(&self, prefixes: &[&str]) -> String {
        let order = reading_order(self.locale);
        let mut tables: Vec<_> = order
            .iter()
            .map(|folder| (*folder, self.catalog.folder_table(folder, prefixes)))
            .collect();
        let mut later = std::collections::HashSet::new();
        for (_, messages) in tables.iter_mut().rev() {
            messages.retain(|id, _| !later.contains(id));
            later.extend(messages.keys().cloned());
        }
        let mut out = self.script_header();
        for (folder, messages) in tables {
            if !messages.is_empty() {
                out.push_str(&folder_island(folder, messages));
            }
        }
        out
    }
}
