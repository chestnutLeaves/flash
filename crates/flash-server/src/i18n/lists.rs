//! Each language's lists as the page scripts and a downstream extension
//! write them, without the browser's or phone's own list formatting,
//! whose CLDR may be older than the server's: the joiners CLDR's
//! patterns put between items
//! (`i18n::list_in` formats with ICU4X itself), and the conditions on
//! which a joiner changes for the item after it.
//!
//! The conditions are ICU4X's own, the only ones CLDR's lists have
//! (icu4x provider/source/src/list/mod.rs): Spanish "y" turns to "e"
//! before an i sound and "o" to "u" before an o sound, and Hebrew's "ו"
//! takes a hyphen before a word not in Hebrew script. They are written
//! here as the app's regular expressions take them; each is checked
//! against ICU4X when the app's table is written, and shared vectors hold
//! the app to the server's lists (tests/list_vectors.json).

use flash_core::locale::UiLocale;
use serde::Serialize;

/// How a list joins its items (CLDR's list patterns).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// All of them: "a, b, and c", "a, b y c".
    And,
    /// One of them: "a, b, or c", "a, b o c".
    Or,
    /// A plain enumeration, as of measures: "a, b, c" in English, "a, b
    /// y c" in Spanish.
    Unit,
}

/// `items` as one list in `locale`, joined as `kind` joins: CLDR's
/// patterns through ICU4X, its conditional forms included (Spanish "e"
/// before an i sound, "u" before an o sound; Hebrew's hyphen before a
/// word in another script). The items are set as they are; the whole
/// list is isolated where a message sets it (`Tr::args`).
pub fn list_in<S: AsRef<str>>(locale: UiLocale, items: &[S], kind: ListKind) -> String {
    use writeable::Writeable;
    // A pseudo-language is English turned: its lists are English's, not
    // those of the language its tag happens to start with (ar-XB).
    let tag = if locale.pseudo().is_some() {
        UiLocale::EN.tag()
    } else {
        locale.tag()
    };
    let locale: icu_locale_core::Locale = tag.parse().unwrap_or(icu_locale_core::Locale::UNKNOWN);
    let prefs = (&locale).into();
    let options = icu_list::options::ListFormatterOptions::default()
        .with_length(icu_list::options::ListLength::Wide);
    let formatter = match kind {
        ListKind::And => icu_list::ListFormatter::try_new_and(prefs, options),
        ListKind::Or => icu_list::ListFormatter::try_new_or(prefs, options),
        ListKind::Unit => icu_list::ListFormatter::try_new_unit(prefs, options),
    }
    .expect("ICU4X's compiled data has every language's lists");
    formatter
        .format(items.iter().map(AsRef::as_ref))
        .write_to_string()
        .into_owned()
}

/// What the items are joined with, by where they stand in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Joiners {
    /// Between the two items of a two-item list.
    pub pair: String,
    /// After the first item of a longer list.
    pub start: String,
    /// Between the items in its middle.
    pub middle: String,
    /// Before its last item.
    pub end: String,
    /// What `pair` and `end` turn to before an item that matches.
    pub special: Option<Special>,
}

/// A joiner that changes for the item after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Special {
    /// The item after it matches this (a JavaScript expression, `u`
    /// flag, matched from the item's start).
    pub when: &'static str,
    /// The joiner then.
    pub then: &'static str,
}

/// One of ICU4X's conditions.
struct Condition {
    /// The language it is for (its primary subtag).
    language: &'static str,
    kinds: &'static [ListKind],
    /// The joiner it changes.
    joiner: &'static str,
    special: Special,
    /// A word it changes the joiner for, and one it leaves alone, to
    /// check it against ICU4X.
    loud: &'static str,
    quiet: &'static str,
}

const CONDITIONS: &[Condition] = &[
    Condition {
        // lang-literal: ICU4X's own conditions name the language they are for.
        language: "es",
        kinds: &[ListKind::And, ListKind::Unit],
        joiner: " y ",
        special: Special {
            when: "^(?:[iI]|[hH][iI](?:[^aeAE]|$))",
            then: " e ",
        },
        loud: "inglés",
        quiet: "hielo",
    },
    Condition {
        // lang-literal: ICU4X's own conditions name the language they are for.
        language: "es",
        kinds: &[ListKind::Or],
        joiner: " o ",
        special: Special {
            // ICU4X's `[^\.,[0-9]]` is a class union in Rust's syntax;
            // in JavaScript's it is `[^.,0-9]`.
            when: "^(?:[oO]|[hH][oO]|8|11(?:[.\u{a0} ]?[0-9]{3})*(?:,[0-9]*)?(?:[^.,0-9]|$))",
            then: " u ",
        },
        loud: "hora",
        quiet: "choco",
    },
    Condition {
        language: "he",
        kinds: &[ListKind::And, ListKind::Or, ListKind::Unit],
        joiner: " \u{05D5}",
        special: Special {
            when: "^[^\\p{Script=Hebrew}]",
            then: " \u{05D5}\u{2011}",
        },
        loud: "Bio",
        quiet: "\u{05D1}",
    },
];

/// Items no condition looks at, to find the joiners between: private
/// use, so no text holds them.
const MARKS: [&str; 3] = ["\u{E020}", "\u{E021}", "\u{E022}"];

/// The text between `before` and `after` in `out`.
fn between<'a>(out: &'a str, before: &str, after: &str) -> &'a str {
    let start = out.find(before).expect("the item is in the list") + before.len();
    let end = start + out[start..].find(after).expect("the item is in the list");
    &out[start..end]
}

/// `locale`'s joiners for `kind`, as ICU4X formats its lists.
pub fn joiners(locale: UiLocale, kind: ListKind) -> Joiners {
    let language = locale.tag().split('-').next().unwrap_or_default();
    let condition = CONDITIONS
        .iter()
        .find(|c| c.language == language && c.kinds.contains(&kind));
    // The last item is one the condition leaves alone, so the joiner
    // before it is the plain one.
    let last = condition.map_or("\u{E023}", |c| c.quiet);
    let [a, b, c] = MARKS;
    let pair_out = list_in(locale, &[a, last], kind);
    let three = list_in(locale, &[a, b, last], kind);
    let four = list_in(locale, &[a, b, c, last], kind);
    for out in [&pair_out, &three, &four] {
        assert!(
            out.starts_with(a) && out.ends_with(last),
            "{}: a list that is not its items with joiners between: {out:?}",
            locale.tag()
        );
    }
    let pair = between(&pair_out, a, last).to_string();
    let end = between(&three, b, last).to_string();
    let special = condition.and_then(|c| {
        if end != c.joiner && pair != c.joiner {
            return None;
        }
        let loud = list_in(locale, &[a, c.loud], kind);
        assert_eq!(
            between(&loud, a, c.loud),
            c.special.then,
            "{}: ICU4X does not turn {:?} before {:?}",
            locale.tag(),
            c.joiner,
            c.loud
        );
        Some(c.special.clone())
    });
    Joiners {
        pair,
        start: between(&three, a, b).to_string(),
        middle: between(&four, b, c).to_string(),
        end,
        special,
    }
}
