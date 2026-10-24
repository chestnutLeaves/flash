//! Reading a browser's language preferences, the table of Flash's own
//! languages, and which of them answers a person.

use std::collections::HashSet;

use flash_core::locale::{
    negotiate_tags, normalize, parse_accept_language, score, Pref, Regionless, Regions, UiLocale,
    LANGUAGES, MAX_PREFS,
};
use serde::Deserialize;

#[test]
fn a_tag_keeps_its_language_and_region() {
    assert_eq!(normalize("es-mx").as_deref(), Some("es-MX"));
    assert_eq!(normalize("ES_419").as_deref(), Some("es-419"));
    assert_eq!(normalize("zh-Hant-TW").as_deref(), Some("zh-TW"));
    assert_eq!(normalize("sr-Latn").as_deref(), Some("sr"));
    assert_eq!(normalize(" pt-BR ").as_deref(), Some("pt-BR"));
    assert_eq!(normalize("fil").as_deref(), Some("fil"));
    assert_eq!(normalize("en-US-x-private").as_deref(), Some("en-US"));
    assert_eq!(normalize("es-Mexico").as_deref(), Some("es"));
    for junk in ["*", "", "x-klingon", "123", "e", "english", "<script>"] {
        assert_eq!(normalize(junk), None, "{junk:?}");
    }
    assert_eq!(normalize(&"a".repeat(40)), None);
}

#[test]
fn a_preference_keeps_its_script() {
    let p = Pref::parse("zh-hant-tw").unwrap();
    assert_eq!(
        (p.lang.as_str(), p.script.as_deref(), p.region.as_deref()),
        ("zh", Some("Hant"), Some("TW"))
    );
    assert_eq!(p.tag(), "zh-Hant-TW");
    assert_eq!(Pref::parse("sr_LATN").unwrap().tag(), "sr-Latn");
    assert_eq!(Pref::parse("es-419").unwrap().tag(), "es-419");
    assert_eq!(Pref::parse("x-klingon"), None);
}

#[test]
fn a_header_is_read_most_wanted_first() {
    assert_eq!(
        parse_accept_language("es-MX,es;q=0.9,en-US;q=0.8,en;q=0.7"),
        ["es-MX", "es", "en-US", "en"]
    );
    assert_eq!(
        parse_accept_language("en;q=0.5, fr, de;q=0.5"),
        ["fr", "en", "de"]
    );
    assert_eq!(parse_accept_language("es;q=0, en, EN"), ["en"]);
    assert_eq!(parse_accept_language("*, ;;, es-ES;q=abc, ca"), ["ca"]);
    assert!(parse_accept_language("").is_empty());
    assert_eq!(
        parse_accept_language("zh-Hant-TW, zh"),
        ["zh-Hant-TW", "zh"]
    );
}

#[test]
fn a_long_header_is_cut() {
    let header = (0..1000)
        .map(|i| {
            format!(
                "x{}{}",
                (b'a' + (i % 26) as u8) as char,
                (b'a' + (i / 26 % 26) as u8) as char
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    assert!(parse_accept_language(&header).len() <= MAX_PREFS);
}

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    prefs: Vec<String>,
    zone: Option<String>,
    shipped: Vec<String>,
    want: Option<String>,
    format: Option<String>,
}

/// The cases the Rust, app and panel implementations are all held to.
#[test]
fn every_shared_case_answers_as_written() {
    let text = include_str!("../locale_vectors.json");
    let vectors: Vectors = serde_json::from_str(text).expect("the vectors parse");
    assert!(vectors.cases.len() >= 30, "the cases are all there");
    for case in &vectors.cases {
        let shipped: Vec<UiLocale> = case
            .shipped
            .iter()
            .map(|t| UiLocale::by_tag(t).unwrap_or_else(|| panic!("no row for {t}")))
            .collect();
        let got = negotiate_tags(&case.prefs, case.zone.as_deref(), &shipped);
        assert_eq!(
            got.as_ref().map(|n| n.locale.tag()),
            case.want.as_deref(),
            "{:?} in {:?}, shipped {:?}",
            case.prefs,
            case.zone,
            case.shipped
        );
        assert_eq!(
            got.as_ref().map(|n| n.format_tag.as_str()),
            case.format.as_deref(),
            "format of {:?}",
            case.prefs
        );
    }
}

/// Every row is whole and the table is unambiguous: English first,
/// unique tags, prefixes and names, chains that end in the row's own
/// folder, and no two rows answering a preference equally well.
#[test]
fn the_table_is_whole_and_unambiguous() {
    assert_eq!(UiLocale::EN.tag(), "en");
    assert_eq!(LANGUAGES[0].tag, "en");
    assert_eq!(LANGUAGES[0].prefix, "");
    let mut tags = HashSet::new();
    let mut prefixes = HashSet::new();
    let mut hreflangs = HashSet::new();
    let mut ogs = HashSet::new();
    for l in LANGUAGES {
        assert!(tags.insert(l.tag.to_ascii_lowercase()), "{} twice", l.tag);
        assert!(prefixes.insert(l.prefix), "prefix {} twice", l.prefix);
        assert!(
            hreflangs.insert(l.hreflang),
            "hreflang {} twice",
            l.hreflang
        );
        assert!(ogs.insert(l.og), "og {} twice", l.og);
        assert_eq!(Pref::parse(l.tag).map(|p| p.tag()).as_deref(), Some(l.tag));
        match l.pseudo {
            None => assert_eq!(l.chain.last(), Some(&l.tag), "{}", l.tag),
            Some(_) => assert_eq!(l.chain, ["en"], "{}", l.tag),
        }
        if l.tag != "en" {
            assert!(
                l.prefix.starts_with('/')
                    && l.prefix[1..]
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b == b'-'),
                "prefix {}",
                l.prefix
            );
        }
        assert!(!l.native_name.is_empty() && !l.english_name.is_empty());
        for text in [l.native_name, l.english_name] {
            assert!(!text.contains(['\u{2014}', '\u{2015}']), "{text}");
        }
        for other in l.answers.stands_in_for {
            assert!(
                LANGUAGES.iter().all(|r| r.answers.lang != *other),
                "{} stands in for {other}, which has a row",
                l.tag
            );
        }
    }
    for l in LANGUAGES.iter().filter(|l| l.pseudo.is_none()) {
        for folder in &l.chain[..l.chain.len() - 1] {
            assert!(!tags.contains(&folder.to_ascii_lowercase()), "{folder}");
            let sharing = LANGUAGES
                .iter()
                .filter(|r| r.chain.contains(folder))
                .count();
            assert!(sharing >= 2, "base {folder} is shared by one row");
        }
    }
}

/// No two rows ever answer a preference equally well, so the table's
/// order never decides: every preference the table can tell apart (each
/// region, script and zone any row names, and ones none names) is tried.
#[test]
fn no_two_rows_tie_on_any_preference() {
    let mut regions: Vec<Option<String>> = vec![None, Some("ZZ".into()), Some("999".into())];
    let mut scripts: Vec<Option<String>> = vec![None, Some("Zyyy".into())];
    let mut zones: Vec<Option<String>> = vec![None, Some("Pacific/Nowhere".into())];
    for l in LANGUAGES {
        let a = &l.answers;
        if let Regions::Only(list) | Regions::AllBut(list) = a.regions {
            regions.extend(list.iter().map(|r| Some((*r).to_string())));
        }
        scripts.extend(a.scripts.iter().map(|s| Some((*s).to_string())));
        if let Regionless::Zone(rule) = a.regionless {
            zones.extend(rule.prefixes.iter().map(|p| Some(format!("{p}Somewhere"))));
            zones.extend(rule.exact.iter().map(|z| Some((*z).to_string())));
        }
    }
    let langs: HashSet<&str> = LANGUAGES.iter().map(|l| l.answers.lang).collect();
    for lang in langs {
        for region in &regions {
            for script in &scripts {
                for zone in &zones {
                    let pref = Pref {
                        lang: lang.to_string(),
                        script: script.clone(),
                        region: region.clone(),
                    };
                    let scores: Vec<(i32, &str)> = LANGUAGES
                        .iter()
                        .filter_map(|l| score(l, &pref, zone.as_deref()).map(|s| (s, l.tag)))
                        .collect();
                    let top = scores.iter().map(|(s, _)| *s).max().unwrap();
                    let tied: Vec<&str> = scores
                        .iter()
                        .filter(|(s, _)| *s == top)
                        .map(|(_, t)| *t)
                        .collect();
                    assert_eq!(tied.len(), 1, "{} in {zone:?} ties {tied:?}", pref.tag());
                }
            }
        }
    }
}

/// A script named in a preference outranks its region: Traditional
/// Chinese asked for by script is Traditional wherever the person is.
#[test]
fn a_script_outranks_a_region() {
    use flash_core::locale::{Answers, Dir, Numbers, PluralRule, Script, UiLanguage};
    let row = |tag: &'static str, scripts: &'static [&'static str], regions| UiLanguage {
        tag,
        chain: &[],
        native_name: tag,
        english_name: tag,
        dir: Dir::Ltr,
        script: Script::Hans,
        prefix: "",
        hreflang: tag,
        og: tag,
        answers: Answers {
            lang: "zh",
            scripts,
            regions,
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        pseudo: None,
        // Chinese has one form; this test reads no count.
        plurals: &[PluralRule {
            category: "other",
            rule: " @integer 0~15, 100, 1000, 10000, 100000, 1000000, \u{2026}",
        }],
    };
    let hant = row("zh-Hant", &["Hant"], Regions::Only(&["TW", "HK", "MO"]));
    let hans = row("zh-Hans", &["Hans"], Regions::Any);
    let wins = |tag: &str| {
        let pref = Pref::parse(tag).unwrap();
        let (a, b) = (score(&hant, &pref, None), score(&hans, &pref, None));
        if a > b {
            "zh-Hant"
        } else {
            "zh-Hans"
        }
    };
    assert_eq!(wins("zh-TW"), "zh-Hant");
    assert_eq!(wins("zh-Hant"), "zh-Hant");
    assert_eq!(wins("zh-Hant-CN"), "zh-Hant");
    assert_eq!(wins("zh-CN"), "zh-Hans");
    assert_eq!(wins("zh-Hans-TW"), "zh-Hans");
    assert_eq!(wins("zh"), "zh-Hans");
}

/// Every row's own tag, read back, is that row.
#[test]
fn a_rows_own_tag_answers_with_it() {
    let all: Vec<UiLocale> = UiLocale::all().collect();
    for l in &all {
        let got = negotiate_tags(&[l.tag()], None, &all).unwrap();
        assert_eq!(got.locale, *l);
        assert_eq!(UiLocale::by_tag(&l.tag().to_ascii_uppercase()), Some(*l));
    }
    assert_eq!(UiLocale::by_tag("tlh"), None);
}
