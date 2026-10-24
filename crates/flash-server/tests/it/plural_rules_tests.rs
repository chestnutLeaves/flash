//! The plural rules in the table of languages (flash_core::locale,
//! `plurals`: CLDR's rules, verbatim) against ICU4X's data, the same CLDR
//! release the server picks forms by (crates/intl-pluralrules): each
//! language names the categories ICU4X has for it, and every sample
//! number CLDR writes beside a rule is ICU4X's answer for that rule.
//!
//! The scripts that word a page or the app evaluate the rules themselves
//! (static/i18n.js, and a downstream extension's own), so a phone or a
//! browser with older CLDR picks the same form as the server; both are
//! held to `plural_vectors.json`, ICU4X's answers for numbers around
//! every rule, written here and failing while stale:
//!
//!   FLASH_WRITE_PLURAL_VECTORS=1 cargo test -p flash-server --test it plural_rules_tests::

use std::path::PathBuf;

use fixed_decimal::{CompactDecimal, Decimal};
use flash_core::locale::UiLocale;
use icu_plurals::{PluralCategory, PluralOperands, PluralRules};

fn rules_of(locale: UiLocale) -> PluralRules {
    let tag = icu_locale_core::Locale::try_from_str(locale.tag()).unwrap();
    PluralRules::try_new_cardinal((&tag).into()).unwrap()
}

fn name(category: PluralCategory) -> &'static str {
    match category {
        PluralCategory::Zero => "zero",
        PluralCategory::One => "one",
        PluralCategory::Two => "two",
        PluralCategory::Few => "few",
        PluralCategory::Many => "many",
        PluralCategory::Other => "other",
    }
}

/// ICU4X's category for a number as CLDR writes one: digits, a
/// fraction's visible digits, and a compact exponent (`1.1c6`).
fn category(rules: &PluralRules, number: &str) -> &'static str {
    let operands: PluralOperands = if number.contains('c') {
        (&CompactDecimal::try_from_str(number).unwrap()).into()
    } else {
        (&Decimal::try_from_str(number).unwrap()).into()
    };
    name(rules.category_for(operands))
}

/// The sample numbers a CLDR rule lists after `@integer` and `@decimal`,
/// its ranges (`2~16`, `0.0~1.5`) spelled out and its `…` left out.
fn samples(rule: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in rule.split('@').skip(1) {
        let (kind, list) = part.split_once(' ').unwrap();
        for item in list.split(',').map(str::trim) {
            if item.is_empty() || item == "\u{2026}" {
                continue;
            }
            let Some((from, to)) = item.split_once('~') else {
                out.push(item.to_string());
                continue;
            };
            let digits = from.split_once('.').map_or(0, |(_, f)| f.len());
            assert_eq!(kind == "integer", digits == 0, "{item}");
            let scale = 10u64.pow(digits as u32);
            let whole = |s: &str| -> u64 { s.replace('.', "").parse().unwrap() };
            for n in whole(from)..=whole(to) {
                out.push(if digits == 0 {
                    n.to_string()
                } else {
                    format!("{}.{:0>digits$}", n / scale, n % scale)
                });
            }
        }
    }
    out
}

#[test]
fn every_language_names_icu4xs_categories() {
    for locale in UiLocale::real() {
        let rules = rules_of(locale);
        let mut ours: Vec<&str> = locale.plural_categories().collect();
        let mut theirs: Vec<&str> = rules.categories().map(name).collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{locale:?}");
    }
}

#[test]
fn every_sample_is_icu4xs_answer_for_its_rule() {
    for locale in UiLocale::real() {
        let rules = rules_of(locale);
        for plural in locale.plurals() {
            let samples = samples(plural.rule);
            assert!(
                !samples.is_empty(),
                "{locale:?} {} has samples",
                plural.category
            );
            for sample in samples {
                assert_eq!(
                    category(&rules, &sample),
                    plural.category,
                    "{locale:?}: {sample}"
                );
            }
        }
    }
}

/// The numbers the scripts are held to, per language: every sample, and
/// the numbers around the rules' edges (each integer to 200, the powers
/// of ten and the millions, fractions with one and two visible digits).
fn sweep(locale: UiLocale) -> Vec<String> {
    let mut out: Vec<String> = (0..=200).map(|n| n.to_string()).collect();
    for n in [
        999u64,
        1000,
        1001,
        10_000,
        100_000,
        999_999,
        1_000_000,
        1_000_001,
        2_000_000,
        3_500_000,
        1_000_000_000,
    ] {
        out.push(n.to_string());
    }
    for tenth in 0..=35 {
        out.push(format!("{}.{}", tenth / 10, tenth % 10));
    }
    for s in ["0.05", "1.00", "1.50", "2.25", "10.0", "100.5", "1000000.0"] {
        out.push(s.to_string());
    }
    for plural in locale.plurals() {
        out.extend(samples(plural.rule));
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|s| seen.insert(s.clone()));
    out
}

fn vectors() -> String {
    let mut cases = serde_json::Map::new();
    for locale in UiLocale::real() {
        let rules = rules_of(locale);
        let pairs: Vec<serde_json::Value> = sweep(locale)
            .into_iter()
            .map(|n| {
                let c = category(&rules, &n);
                serde_json::json!([n, c])
            })
            .collect();
        cases.insert(locale.tag().to_string(), serde_json::Value::Array(pairs));
    }
    let json = serde_json::json!({
        "note": "ICU4X's plural category for each number, per language of the table. Written by crates/flash-server/tests/it/plural_rules_tests.rs; rerun with FLASH_WRITE_PLURAL_VECTORS=1.",
        "cases": cases,
    });
    serde_json::to_string_pretty(&json).unwrap() + "\n"
}

#[test]
fn the_scripts_vectors_are_icu4xs_answers() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plural_vectors.json");
    let fresh = vectors();
    if std::env::var("FLASH_WRITE_PLURAL_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let held = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        held.replace("\r\n", "\n") == fresh,
        "{} is stale. Rerun with FLASH_WRITE_PLURAL_VECTORS=1",
        path.display()
    );
}
