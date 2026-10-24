//! The language a person reads Flash in: what their browser, their
//! assistant or their phone says they read, and which of the languages a
//! deployment gives readers answers it.
//!
//! The languages themselves are data (`registry`): this module only reads
//! preferences and matches them against the table. The location of a
//! request never decides: only what the person's own software says, and
//! the time zone they study in where the software names no country and a
//! row tells two variants apart by it.

mod registry;

pub use registry::{
    Answers, Dir, Numbers, PluralRule, Pseudo, Regionless, Regions, Script, ScriptStyle, TypeKind,
    UiLanguage, UiLocale, ZoneRule, LANGUAGES,
};

/// How many preferences of an `Accept-Language` header are kept: a
/// header is the client's to fill, and a preference past the eighth
/// never decides anything.
pub const MAX_PREFS: usize = 8;
/// The longest tag read; anything longer is not a language preference.
pub const MAX_TAG: usize = 35;

/// One language preference, its subtags in their canonical case: "zh",
/// "Hant", "TW". Anything after the region is not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pref {
    pub lang: String,
    pub script: Option<String>,
    pub region: Option<String>,
}

impl Pref {
    /// A BCP 47 tag read as a preference: "es-mx" is es, MX; "zh-hant-tw"
    /// is zh, Hant, TW; "es_419" is es, 419. None for anything that is not
    /// a language: "*", "x-klingon", "123", "".
    pub fn parse(tag: &str) -> Option<Pref> {
        let tag = tag.trim();
        if tag.is_empty() || tag.len() > MAX_TAG {
            return None;
        }
        let mut parts = tag.split(['-', '_']);
        let lang = parts.next()?.to_ascii_lowercase();
        if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
            return None;
        }
        // A script may come first; the region is the part after it.
        let mut next = parts.next();
        let script = next
            .filter(|p| p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()))
            .map(|p| {
                let mut s = p.to_ascii_lowercase();
                s[..1].make_ascii_uppercase();
                s
            });
        if script.is_some() {
            next = parts.next();
        }
        let region = next
            .filter(|p| {
                (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()))
                    || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))
            })
            .map(str::to_ascii_uppercase);
        Some(Pref {
            lang,
            script,
            region,
        })
    }

    /// The preference as a tag, every subtag it kept: "zh-Hant-TW".
    pub fn tag(&self) -> String {
        let mut out = self.lang.clone();
        for part in [&self.script, &self.region].into_iter().flatten() {
            out.push('-');
            out.push_str(part);
        }
        out
    }
}

/// A tag cut to its language and region, lowercase and uppercase: "es-mx"
/// is "es-MX", "zh-Hant-TW" is "zh-TW", "es_419" is "es-419". None for
/// anything that is not a language: "*", "x-klingon", "123", "". This is
/// how a card's language is kept (a label names a language and where it
/// is spoken, not how it is written).
pub fn normalize(tag: &str) -> Option<String> {
    let p = Pref::parse(tag)?;
    Some(match p.region {
        Some(r) => format!("{}-{r}", p.lang),
        None => p.lang,
    })
}

/// An `Accept-Language` header's preferences, most wanted first: each
/// written as `Pref::tag` (its script kept), a repeat dropped, a weight of
/// zero left out (it means "not this one"), at most `MAX_PREFS`. A
/// malformed entry is skipped, never the header.
pub fn parse_accept_language(header: &str) -> Vec<String> {
    let mut weighed: Vec<(u16, usize, String)> = Vec::new();
    for (i, entry) in header.split(',').take(MAX_PREFS * 4).enumerate() {
        let mut fields = entry.split(';');
        let Some(tag) = fields.next().and_then(Pref::parse).map(|p| p.tag()) else {
            continue;
        };
        let mut q = 1000;
        for f in fields {
            if let Some(v) = f.trim().strip_prefix("q=") {
                q = parse_q(v).unwrap_or(0);
            }
        }
        if q == 0 || weighed.iter().any(|(_, _, t)| *t == tag) {
            continue;
        }
        weighed.push((q, i, tag));
    }
    weighed.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    weighed
        .into_iter()
        .take(MAX_PREFS)
        .map(|(_, _, t)| t)
        .collect()
}

/// A q-value in thousandths: "0.8" is 800, "1" is 1000. None when it is
/// not one (RFC 9110 allows at most three decimals, 0 to 1).
fn parse_q(v: &str) -> Option<u16> {
    let v = v.trim();
    let (whole, frac) = v.split_once('.').unwrap_or((v, ""));
    if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let thousandths = format!("{frac:0<3}").parse::<u16>().ok()?;
    match whole {
        "0" => Some(thousandths),
        "1" if thousandths == 0 => Some(1000),
        _ => None,
    }
}

/// A language that answers a person's preferences, and the preference
/// that chose it, as a tag: the region numbers and dates are written for
/// ("es-AR" for Latin America's Spanish read in Argentina).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    pub locale: UiLocale,
    pub format_tag: String,
}

/// How well `row` answers `pref`, or None when it is another language.
/// A script named and answered counts most, then a region named and
/// answered, then a regionless preference read by the time zone; a row
/// of the right language that answers none of that still answers, last
/// (a sibling variant beats reading another language). Public so the
/// table's test can prove no two rows ever tie.
pub fn score(row: &UiLanguage, pref: &Pref, zone: Option<&str>) -> Option<i32> {
    let a = &row.answers;
    if a.lang != pref.lang {
        return None;
    }
    let mut s = 0;
    if let Some(script) = &pref.script {
        if a.scripts.iter().any(|x| x.eq_ignore_ascii_case(script)) {
            s += 8;
        } else if !a.scripts.is_empty() {
            s -= 8;
        }
    }
    s += match &pref.region {
        Some(r) => match a.regions {
            Regions::Any => 1,
            Regions::Only(list) if list.contains(&r.as_str()) => 4,
            Regions::Only(_) => -4,
            Regions::AllBut(list) if list.contains(&r.as_str()) => -4,
            Regions::AllBut(_) => 1,
        },
        None => match a.regionless {
            Regionless::Yes => 1,
            Regionless::No => -4,
            Regionless::Zone(rule) if zone.is_some_and(|z| rule.matches(z)) => 2,
            Regionless::Zone(_) => 0,
        },
    };
    Some(s)
}

/// The shipped language that best answers `pref`; on a tie, the one
/// earlier in the table.
fn best(pref: &Pref, zone: Option<&str>, shipped: &[UiLocale]) -> Option<UiLocale> {
    let mut found: Option<(i32, UiLocale)> = None;
    for locale in UiLocale::all().filter(|l| shipped.contains(l)) {
        if let Some(s) = score(locale.language(), pref, zone) {
            if found.is_none_or(|(best, _)| s > best) {
                found = Some((s, locale));
            }
        }
    }
    found.map(|(_, l)| l)
}

/// Which of the `shipped` languages answers these preferences (most
/// wanted first), given the time zone the person studies in when one was
/// learned from them (never a default); None when none does:
///
/// - the first preference a shipped language answers decides, by
///   `score` (a script, then a region, then the zone);
/// - a preference no shipped language answers is passed over, so the
///   next one may decide;
/// - a language with no row that a row stands in for (Catalan for
///   Spain's Spanish) chooses that row unless a later preference is
///   answered by a language of its own, English excepted.
pub fn negotiate(prefs: &[Pref], zone: Option<&str>, shipped: &[UiLocale]) -> Option<Negotiated> {
    let mut stand_in: Option<Negotiated> = None;
    for pref in prefs {
        if let Some(locale) = best(pref, zone, shipped) {
            if stand_in.is_some() && locale == UiLocale::EN {
                continue;
            }
            return Some(Negotiated {
                locale,
                format_tag: pref.tag(),
            });
        }
        if stand_in.is_none() {
            stand_in = UiLocale::all()
                .filter(|l| shipped.contains(l))
                .find(|l| {
                    l.language()
                        .answers
                        .stands_in_for
                        .contains(&pref.lang.as_str())
                })
                .map(|locale| Negotiated {
                    locale,
                    format_tag: pref.tag(),
                });
        }
    }
    stand_in
}

/// `negotiate` over tags as they are kept or sent ("es-MX", "zh-Hant-TW");
/// one that is not a language is skipped.
pub fn negotiate_tags<S: AsRef<str>>(
    tags: &[S],
    zone: Option<&str>,
    shipped: &[UiLocale],
) -> Option<Negotiated> {
    let prefs: Vec<Pref> = tags
        .iter()
        .filter_map(|t| Pref::parse(t.as_ref()))
        .collect();
    negotiate(&prefs, zone, shipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_values() {
        assert_eq!(parse_q("1"), Some(1000));
        assert_eq!(parse_q("1.000"), Some(1000));
        assert_eq!(parse_q("0.8"), Some(800));
        assert_eq!(parse_q("0.05"), Some(50));
        assert_eq!(parse_q("0"), Some(0));
        assert_eq!(parse_q("1.5"), None);
        assert_eq!(parse_q("0.1234"), None);
        assert_eq!(parse_q("abc"), None);
    }
}
