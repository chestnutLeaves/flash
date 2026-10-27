//! Every language Flash's own words may be written in, as data: one row
//! per language, and the only place a language's tag, folders, address,
//! direction or negotiation rules are written down. Everything else asks
//! the table (`UiLocale`), so adding a language is a row and its
//! messages, never a code change.
//!
//! Whether a language is given to readers is the deployment's to say
//! (shipping); a row only says what the language is.
//!
//! The open server's build script includes this file by `#[path]`, so it
//! uses nothing else of this crate (no `crate::`) and no dependency but
//! serde.

use serde::Serialize;

/// One of the languages Flash's own words are written in: an index into
/// `LANGUAGES`. Nothing stores it; an account keeps its language as the
/// tag (`tag()`), and a tag is read back through negotiation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UiLocale(u8);

impl std::fmt::Debug for UiLocale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

/// Which way a language's lines run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Ltr,
    Rtl,
}

impl Dir {
    /// As `<html dir>` writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Ltr => "ltr",
            Dir::Rtl => "rtl",
        }
    }

    /// The side of the page a line starts on, for what knows no
    /// `inline-start` (a mail client's inline styles): "left", or
    /// "right" in a right-to-left language.
    pub fn start_side(self) -> &'static str {
        match self {
            Dir::Ltr => "left",
            Dir::Rtl => "right",
        }
    }
}

/// The script a language is written in (ISO 15924), which decides the
/// fonts and typography it is set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Script {
    Latn,
    Cyrl,
    Grek,
    Arab,
    Hebr,
    Deva,
    Thai,
    Hans,
    Hant,
    Jpan,
    Kore,
}

impl Script {
    /// Every script the table knows, for what is written per script (the
    /// stylesheets' blocks, the app's table).
    pub const ALL: [Script; 11] = [
        Script::Latn,
        Script::Cyrl,
        Script::Grek,
        Script::Arab,
        Script::Hebr,
        Script::Deva,
        Script::Thai,
        Script::Hans,
        Script::Hant,
        Script::Jpan,
        Script::Kore,
    ];

    /// Its ISO 15924 code, as a page's `data-script` names it.
    pub fn code(self) -> &'static str {
        match self {
            Script::Latn => "Latn",
            Script::Cyrl => "Cyrl",
            Script::Grek => "Grek",
            Script::Arab => "Arab",
            Script::Hebr => "Hebr",
            Script::Deva => "Deva",
            Script::Thai => "Thai",
            Script::Hans => "Hans",
            Script::Hant => "Hant",
            Script::Jpan => "Jpan",
            Script::Kore => "Kore",
        }
    }

    /// What sets one sentence apart from the next: a space, or nothing
    /// where the sentence's own full-width stop does it (Chinese,
    /// Japanese).
    pub fn sentence_gap(self) -> &'static str {
        match self {
            Script::Hans | Script::Hant | Script::Jpan => "",
            _ => " ",
        }
    }

    /// How text in this script is set: its fonts, its line height and
    /// size against English's, and whether it has case and takes
    /// tracking.
    ///
    /// The categories are Material's (m1.material.io, Typography,
    /// language categories): English-like scripts (Latin, Greek,
    /// Cyrillic, Hebrew) as English; tall ones (Arabic, Devanagari, Thai)
    /// and dense ones (Chinese, Japanese, Korean) a tenth of an em more
    /// line height and a pixel more size, which keeps CJK body text
    /// within the 1.5 to 2.0 line height jlreq and clreq give. The faces
    /// are the ones each system ships for the script (Windows' font list,
    /// Apple's system fonts, Android's fonts.xml), each system's own
    /// first so the browser takes the face that is there; Han faces come
    /// before the system's own face, which would pick its glyphs by the
    /// system's language rather than the page's.
    pub fn style(self) -> ScriptStyle {
        let (kind, brand, cased, faces): (TypeKind, bool, bool, &'static [&'static str]) =
            match self {
                Script::Latn | Script::Cyrl => (TypeKind::EnglishLike, true, true, &[]),
                // Geist holds only six Greek letters: a Greek word would
                // be set in two faces. The system's own Greek instead.
                Script::Grek => (TypeKind::EnglishLike, false, true, &["Roboto", "Noto Sans"]),
                Script::Hebr => (
                    TypeKind::EnglishLike,
                    true,
                    false,
                    &["Segoe UI", "Arial Hebrew", "Noto Sans Hebrew", "Arial"],
                ),
                Script::Arab => (
                    TypeKind::Tall,
                    true,
                    false,
                    &[
                        "Segoe UI",
                        "Geeza Pro",
                        "Noto Naskh Arabic UI",
                        "Noto Sans Arabic",
                        "Tahoma",
                    ],
                ),
                Script::Deva => (
                    TypeKind::Tall,
                    true,
                    false,
                    &[
                        "Nirmala UI",
                        "Kohinoor Devanagari",
                        "Noto Sans Devanagari UI",
                        "Noto Sans Devanagari",
                    ],
                ),
                Script::Thai => (
                    TypeKind::Tall,
                    true,
                    false,
                    &[
                        "Leelawadee UI",
                        "Thonburi",
                        "Noto Sans Thai UI",
                        "Noto Sans Thai",
                    ],
                ),
                Script::Hans => (
                    TypeKind::Dense,
                    true,
                    false,
                    &[
                        "PingFang SC",
                        "Microsoft YaHei UI",
                        "Microsoft YaHei",
                        "Noto Sans CJK SC",
                        "Noto Sans SC",
                    ],
                ),
                Script::Hant => (
                    TypeKind::Dense,
                    true,
                    false,
                    &[
                        "PingFang TC",
                        "Microsoft JhengHei UI",
                        "Microsoft JhengHei",
                        "Noto Sans CJK TC",
                        "Noto Sans TC",
                    ],
                ),
                Script::Jpan => (
                    TypeKind::Dense,
                    true,
                    false,
                    &[
                        "Hiragino Sans",
                        "Yu Gothic UI",
                        "Yu Gothic",
                        "Meiryo",
                        "Noto Sans CJK JP",
                        "Noto Sans JP",
                    ],
                ),
                Script::Kore => (
                    TypeKind::Dense,
                    true,
                    false,
                    &[
                        "Apple SD Gothic Neo",
                        "Malgun Gothic",
                        "Noto Sans CJK KR",
                        "Noto Sans KR",
                    ],
                ),
            };
        let roomy = kind != TypeKind::EnglishLike;
        ScriptStyle {
            kind,
            brand,
            cased,
            faces,
            leading_tenths: u8::from(roomy),
            size_step: u8::from(roomy),
        }
    }
}

/// Material's language categories for type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeKind {
    /// Set as English.
    EnglishLike,
    /// Taller glyphs (Arabic, Devanagari, Thai): more line height.
    Tall,
    /// Denser glyphs (Chinese, Japanese, Korean): more line height and
    /// size.
    Dense,
}

/// How a script's text is set (`Script::style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScriptStyle {
    pub kind: TypeKind,
    /// Whether Geist leads its fonts: it holds the script's letters (or,
    /// for a script it lacks, the Latin letters and digits mixed in, the
    /// system's face taking the rest letter by letter).
    pub brand: bool,
    /// Whether the script has case and so takes the uppercase, spaced
    /// labels: tracking breaks Arabic's joins (Safari still does it,
    /// WebKit bug 195745) and means nothing to a caseless script's
    /// labels, which are set apart by weight instead.
    pub cased: bool,
    /// The faces after Geist, the system's own for the script, in the
    /// order to try them.
    pub faces: &'static [&'static str],
    /// Line height added to English's, in tenths of an em.
    pub leading_tenths: u8,
    /// Pixels added to each of English's sizes.
    pub size_step: u8,
}

impl ScriptStyle {
    /// The sans-serif stack, as CSS writes it.
    pub fn sans_stack(self) -> String {
        let mut names: Vec<&str> = Vec::new();
        if self.brand {
            names.push("Geist");
        }
        names.extend(self.faces);
        names.extend(["-apple-system", "Segoe UI"]);
        stack(&names, "sans-serif")
    }

    /// The monospace stack, as CSS writes it. After Geist Mono (Latin
    /// letters and digits) come the script's own faces, before the
    /// system's monospace, which would pick Han glyphs by the system's
    /// language; a script Geist does not lead (Greek) takes the system's
    /// monospace first, which holds it.
    pub fn mono_stack(self) -> String {
        let mut names: Vec<&str> = Vec::new();
        if self.brand {
            names.push("Geist Mono");
            names.extend(self.faces);
            names.extend(["ui-monospace", "SF Mono"]);
        } else {
            names.extend(["ui-monospace", "SF Mono"]);
            names.extend(self.faces);
        }
        stack(&names, "monospace")
    }
}

/// `names` as a CSS font-family list, each once, quoted unless it is a
/// keyword (`-apple-system`, `ui-monospace`), ending in `generic`.
fn stack(names: &[&str], generic: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for name in names {
        if !seen.contains(name) {
            seen.push(name);
        }
    }
    let mut out: Vec<String> = seen
        .iter()
        .map(|name| {
            if name.starts_with('-') || name.starts_with("ui-") {
                (*name).to_string()
            } else {
                format!("\"{name}\"")
            }
        })
        .collect();
    out.push(generic.to_string());
    out.join(",")
}

/// How a pseudo-language turns English: a row that is no language people
/// read, but English turned so a person or a test sees what the
/// machinery misses. Shipped only where a deployment asks for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pseudo {
    /// Every letter accented, every message about 40% longer and in
    /// brackets: words left out of the catalog stay plain, and a label
    /// that will not fit a longer language is cut.
    Accented,
    /// Read right to left: what does not turn with its reader.
    Bidi,
}

/// How a language's numbers are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Numbers {
    /// Digits as they are, no grouping: how English pages have always
    /// written them.
    Plain,
    /// As the reader's region writes them (CLDR).
    Cldr,
}

/// Which regions of its language a row answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "list", rename_all = "snake_case")]
pub enum Regions {
    /// Every region.
    Any,
    /// These regions (ISO 3166 or UN M.49 codes, as BCP 47 writes them).
    Only(&'static [&'static str]),
    /// Every region but these.
    AllBut(&'static [&'static str]),
}

/// Which time zones a regionless tag is read as a region by: a zone
/// starting with one of `prefixes`, or one of `exact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ZoneRule {
    pub prefixes: &'static [&'static str],
    pub exact: &'static [&'static str],
}

impl ZoneRule {
    pub fn matches(&self, zone: &str) -> bool {
        self.prefixes.iter().any(|p| zone.starts_with(p)) || self.exact.contains(&zone)
    }
}

/// Whether a row answers its language with no region named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "zones", rename_all = "snake_case")]
pub enum Regionless {
    /// It does.
    Yes,
    /// It does not (a sibling row does).
    No,
    /// When the person studies in one of these time zones.
    Zone(ZoneRule),
}

/// Which preferences a row answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Answers {
    /// The language subtag ("es", "pt", "zh").
    pub lang: &'static str,
    /// The script subtags it answers when a preference names one
    /// ("Hant"); empty when the language is written in one script.
    pub scripts: &'static [&'static str],
    pub regions: Regions,
    pub regionless: Regionless,
    /// Languages with no row of their own whose readers read this one
    /// (Catalan, Basque and Galician readers read Spain's Spanish).
    pub stands_in_for: &'static [&'static str],
}

/// One of a language's plural forms and when it is used: CLDR's rule,
/// verbatim, its sample numbers after `@integer` and `@decimal` included
/// (tests hold each sample to ICU4X's answer, and the scripts that
/// evaluate the rule to the same answers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PluralRule {
    /// "zero", "one", "two", "few", "many" or "other".
    pub category: &'static str,
    /// When: "i = 1 and v = 0 @integer 1"; empty before the samples for
    /// `other`, which is every number no other rule takes.
    pub rule: &'static str,
}

/// "one" for 1 alone, written as a whole number (English's, Portugal's).
const ONE_INTEGER: PluralRule = PluralRule {
    category: "one",
    rule: "i = 1 and v = 0 @integer 1",
};

/// "many" for an exact million and its multiples (Spanish, Portuguese
/// and French alike).
const MANY_MILLIONS: PluralRule = PluralRule {
    category: "many",
    rule: "e = 0 and i != 0 and i % 1000000 = 0 and v = 0 or e != 0..5 @integer 1000000, 1c6, 2c6, 3c6, 4c6, 5c6, 6c6, \u{2026} @decimal 1.0000001c6, 1.1c6, 2.0000001c6, 2.1c6, 3.0000001c6, 3.1c6, \u{2026}",
};

/// "other" from 2 up, when 0 and 1 are "one" (Brazil's Portuguese,
/// French).
const OTHER_FROM_TWO: PluralRule = PluralRule {
    category: "other",
    rule: " @integer 2~17, 100, 1000, 10000, 100000, 1c3, 2c3, 3c3, 4c3, 5c3, 6c3, \u{2026} @decimal 2.0~3.5, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 1.0001c3, 1.1c3, 2.0001c3, 2.1c3, 3.0001c3, 3.1c3, \u{2026}",
};

/// English's cardinal forms, CLDR 48.
const ENGLISH_PLURALS: &[PluralRule] = &[
    ONE_INTEGER,
    PluralRule {
        category: "other",
        rule: " @integer 0, 2~16, 100, 1000, 10000, 100000, 1000000, \u{2026} @decimal 0.0~1.5, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, \u{2026}",
    },
];

/// Chinese has one cardinal form: every number uses `other`.
const CHINESE_PLURALS: &[PluralRule] = &[PluralRule {
    category: "other",
    rule: " @integer 0~15, 100, 1000, 10000, 100000, 1000000, … @decimal 0.0~1.5, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, …",
}];

/// Spanish's cardinal forms, CLDR 48: "many" for an exact million and
/// its multiples ("1.000.000 de tarjetas").
const SPANISH_PLURALS: &[PluralRule] = &[
    PluralRule {
        category: "one",
        rule: "n = 1 @integer 1 @decimal 1.0, 1.00, 1.000, 1.0000",
    },
    MANY_MILLIONS,
    PluralRule {
        category: "other",
        rule: " @integer 0, 2~16, 100, 1000, 10000, 100000, 1c3, 2c3, 3c3, 4c3, 5c3, 6c3, \u{2026} @decimal 0.0~0.9, 1.1~1.6, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 1.0001c3, 1.1c3, 2.0001c3, 2.1c3, 3.0001c3, 3.1c3, \u{2026}",
    },
];

/// Brazil's Portuguese cardinal forms, CLDR 48: 0 and 1 are "one"
/// ("0 cartão"), "many" an exact million and its multiples.
const PORTUGUESE_PLURALS: &[PluralRule] = &[
    PluralRule {
        category: "one",
        rule: "i = 0..1 @integer 0, 1 @decimal 0.0~1.5",
    },
    MANY_MILLIONS,
    OTHER_FROM_TWO,
];

/// Portugal's Portuguese cardinal forms, CLDR 48: only 1 is "one"
/// ("0 cartões").
const PORTUGAL_PLURALS: &[PluralRule] = &[
    ONE_INTEGER,
    MANY_MILLIONS,
    PluralRule {
        category: "other",
        rule: " @integer 0, 2~16, 100, 1000, 10000, 100000, 1c3, 2c3, 3c3, 4c3, 5c3, 6c3, \u{2026} @decimal 0.0~1.5, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0, 1.0001c3, 1.1c3, 2.0001c3, 2.1c3, 3.0001c3, 3.1c3, \u{2026}",
    },
];

/// French's cardinal forms, CLDR 48 (France's and Canada's alike): 0 and
/// 1 are "one" ("0 carte"), "many" an exact million and its multiples
/// ("1 million de cartes").
const FRENCH_PLURALS: &[PluralRule] = &[
    PluralRule {
        category: "one",
        rule: "i = 0,1 @integer 0, 1 @decimal 0.0~1.5",
    },
    MANY_MILLIONS,
    OTHER_FROM_TWO,
];

/// One language Flash's words are written in. It serializes as the app
/// and the study panel read it, so they negotiate from the same table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UiLanguage {
    /// Its BCP 47 tag: the folder its own messages are filed under, the
    /// page's `lang`, `?hl=`, the language cookie and an account's choice.
    pub tag: &'static str,
    /// The folders its messages come from, base first and its own last
    /// ("es", "es-ES"); English is under every chain without being named.
    pub chain: &'static [&'static str],
    /// Its name as its own readers write it, for a language switcher.
    pub native_name: &'static str,
    /// Its name in English.
    pub english_name: &'static str,
    pub dir: Dir,
    pub script: Script,
    /// The path prefix of its public pages ("/es"; "" for English).
    pub prefix: &'static str,
    /// What `<link rel=alternate hreflang>` calls it (Google reads no
    /// "419", so Latin America's Spanish is "es").
    pub hreflang: &'static str,
    /// What Open Graph calls it ("es_LA").
    pub og: &'static str,
    pub answers: Answers,
    pub numbers: Numbers,
    /// Its cardinal plural forms, CLDR's rules for its language (the
    /// forms its messages choose among, and how the scripts that word
    /// them choose, the same on every phone and browser).
    pub plurals: &'static [PluralRule],
    /// None for a language people read; how English is turned for a
    /// pseudo-language.
    pub pseudo: Option<Pseudo>,
}

/// The time zones Spanish with no country is read as Spain's in: Europe,
/// and Spain's own outside it (the Canaries, Ceuta and Melilla).
const SPAIN_ZONES: ZoneRule = ZoneRule {
    prefixes: &["Europe/"],
    exact: &["Atlantic/Canary", "Africa/Ceuta"],
};

/// The time zones Portuguese with no country is read as Portugal's in:
/// Europe (Portugal and its emigrants), Portugal's islands, and the
/// countries whose Portuguese follows Portugal's norm.
const PORTUGAL_ZONES: ZoneRule = ZoneRule {
    prefixes: &["Europe/"],
    exact: &[
        "Atlantic/Madeira",
        "Atlantic/Azores",
        "Africa/Luanda",
        "Africa/Maputo",
        "Atlantic/Cape_Verde",
        "Africa/Bissau",
        "Africa/Sao_Tome",
        "Africa/Malabo",
        "Asia/Dili",
        "Asia/Macau",
        "Asia/Macao",
    ],
};

/// The time zones French with no country is read as Canada's in: all of
/// Canada's (Quebec's own zone is Toronto's, shared with Ontario), with
/// their old names, and none of the neighbours' (Miquelon is France's).
const CANADA_ZONES: ZoneRule = ZoneRule {
    prefixes: &["Canada/"],
    exact: &[
        "America/St_Johns",
        "America/Halifax",
        "America/Glace_Bay",
        "America/Moncton",
        "America/Goose_Bay",
        "America/Blanc-Sablon",
        "America/Toronto",
        "America/Montreal",
        "America/Nipigon",
        "America/Thunder_Bay",
        "America/Iqaluit",
        "America/Pangnirtung",
        "America/Atikokan",
        "America/Coral_Harbour",
        "America/Winnipeg",
        "America/Rainy_River",
        "America/Resolute",
        "America/Rankin_Inlet",
        "America/Regina",
        "America/Swift_Current",
        "America/Edmonton",
        "America/Yellowknife",
        "America/Cambridge_Bay",
        "America/Inuvik",
        "America/Vancouver",
        "America/Creston",
        "America/Dawson_Creek",
        "America/Fort_Nelson",
        "America/Whitehorse",
        "America/Dawson",
    ],
};

/// The table. Row 0 is English, the root every chain falls back to.
pub const LANGUAGES: &[UiLanguage] = &[
    UiLanguage {
        tag: "en",
        chain: &["en"],
        native_name: "English",
        english_name: "English",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "",
        hreflang: "en",
        og: "en_US",
        answers: Answers {
            lang: "en",
            scripts: &[],
            regions: Regions::Any,
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Plain,
        plurals: ENGLISH_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "es-ES",
        chain: &["es", "es-ES"],
        native_name: "Español (España)",
        english_name: "Spanish (Spain)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/es-es",
        hreflang: "es-ES",
        og: "es_ES",
        answers: Answers {
            lang: "es",
            scripts: &[],
            regions: Regions::Only(&["ES"]),
            regionless: Regionless::Zone(SPAIN_ZONES),
            stands_in_for: &["ca", "eu", "gl"],
        },
        numbers: Numbers::Cldr,
        plurals: SPANISH_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "es-419",
        chain: &["es", "es-419"],
        native_name: "Español (Latinoamérica)",
        english_name: "Spanish (Latin America)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/es",
        hreflang: "es",
        og: "es_LA",
        answers: Answers {
            lang: "es",
            scripts: &[],
            regions: Regions::Any,
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: SPANISH_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "pt-BR",
        chain: &["pt", "pt-BR"],
        native_name: "Português (Brasil)",
        english_name: "Portuguese (Brazil)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/pt",
        hreflang: "pt",
        og: "pt_BR",
        answers: Answers {
            lang: "pt",
            scripts: &[],
            regions: Regions::Any,
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: PORTUGUESE_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "pt-PT",
        chain: &["pt", "pt-PT"],
        native_name: "Português (Portugal)",
        english_name: "Portuguese (Portugal)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/pt-pt",
        hreflang: "pt-PT",
        og: "pt_PT",
        answers: Answers {
            lang: "pt",
            scripts: &[],
            // Portugal, and the regions CLDR files under Portugal's
            // Portuguese (its parent locales of pt_PT).
            regions: Regions::Only(&[
                "PT", "AO", "CV", "GW", "MZ", "ST", "TL", "MO", "GQ", "CH", "FR", "LU",
            ]),
            regionless: Regionless::Zone(PORTUGAL_ZONES),
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: PORTUGAL_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "fr-FR",
        chain: &["fr", "fr-FR"],
        native_name: "Français (France)",
        english_name: "French (France)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/fr",
        hreflang: "fr",
        og: "fr_FR",
        answers: Answers {
            lang: "fr",
            scripts: &[],
            regions: Regions::Any,
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: FRENCH_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "fr-CA",
        chain: &["fr", "fr-CA"],
        native_name: "Français (Canada)",
        english_name: "French (Canada)",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/fr-ca",
        hreflang: "fr-CA",
        og: "fr_CA",
        answers: Answers {
            lang: "fr",
            scripts: &[],
            regions: Regions::Only(&["CA"]),
            regionless: Regionless::Zone(CANADA_ZONES),
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: FRENCH_PLURALS,
        pseudo: None,
    },
    UiLanguage {
        tag: "zh-CN",
        chain: &["zh-CN"],
        native_name: "简体中文",
        english_name: "Chinese (Simplified)",
        dir: Dir::Ltr,
        script: Script::Hans,
        prefix: "/zh-cn",
        hreflang: "zh-CN",
        og: "zh_CN",
        answers: Answers {
            lang: "zh",
            scripts: &["Hans"],
            regions: Regions::Only(&["CN"]),
            regionless: Regionless::Yes,
            stands_in_for: &[],
        },
        numbers: Numbers::Cldr,
        plurals: CHINESE_PLURALS,
        pseudo: None,
    },
    // The pseudo-languages: English's messages, turned (`Pseudo`). They
    // answer only their own made-up regions (XA, XB), so no reader is
    // given one by their browser or phone.
    UiLanguage {
        tag: "en-XA",
        chain: &["en"],
        native_name: "English (accented)",
        english_name: "Pseudo: accented",
        dir: Dir::Ltr,
        script: Script::Latn,
        prefix: "/en-xa",
        hreflang: "en-XA",
        og: "en_XA",
        answers: Answers {
            lang: "en",
            scripts: &[],
            regions: Regions::Only(&["XA"]),
            regionless: Regionless::No,
            stands_in_for: &[],
        },
        numbers: Numbers::Plain,
        plurals: ENGLISH_PLURALS,
        pseudo: Some(Pseudo::Accented),
    },
    UiLanguage {
        tag: "ar-XB",
        chain: &["en"],
        native_name: "English (right to left)",
        english_name: "Pseudo: right to left",
        dir: Dir::Rtl,
        script: Script::Latn,
        prefix: "/ar-xb",
        hreflang: "ar-XB",
        og: "ar_XB",
        answers: Answers {
            lang: "ar",
            scripts: &[],
            regions: Regions::Only(&["XB"]),
            regionless: Regionless::No,
            stands_in_for: &[],
        },
        numbers: Numbers::Plain,
        plurals: ENGLISH_PLURALS,
        pseudo: Some(Pseudo::Bidi),
    },
];

impl UiLocale {
    /// English: the root every language falls back to, and what a reader
    /// whose languages Flash does not speak reads.
    pub const EN: UiLocale = UiLocale(0);
    /// Simplified Chinese, shipped with the core catalog.
    pub const ZH_CN: UiLocale = UiLocale(7);

    /// Every language in the table, English first.
    pub fn all() -> impl Iterator<Item = UiLocale> {
        (0..LANGUAGES.len()).map(|i| UiLocale(i as u8))
    }

    /// Every language people read, English first: the table without its
    /// pseudo-languages.
    pub fn real() -> impl Iterator<Item = UiLocale> {
        Self::all().filter(|l| l.pseudo().is_none())
    }

    /// Every message folder a language of the table reads, each once, in
    /// the table's order.
    pub fn folders() -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for folder in LANGUAGES.iter().flat_map(|l| l.chain.iter().copied()) {
            if !out.contains(&folder) {
                out.push(folder);
            }
        }
        out
    }

    /// How it turns English, for a pseudo-language; None for a real one.
    pub fn pseudo(self) -> Option<Pseudo> {
        self.language().pseudo
    }

    /// The row whose tag is `tag` (any case); None for a tag no row has.
    pub fn by_tag(tag: &str) -> Option<UiLocale> {
        LANGUAGES
            .iter()
            .position(|l| l.tag.eq_ignore_ascii_case(tag))
            .map(|i| UiLocale(i as u8))
    }

    /// Its row of the table.
    pub fn language(self) -> &'static UiLanguage {
        &LANGUAGES[usize::from(self.0)]
    }

    /// The tag its messages are filed under, and the page's `lang`.
    pub fn tag(self) -> &'static str {
        self.language().tag
    }

    /// Its name as its own readers write it, for a language switcher.
    pub fn native_name(self) -> &'static str {
        self.language().native_name
    }

    /// Its name in English, which a picker's search also matches.
    pub fn english_name(self) -> &'static str {
        self.language().english_name
    }

    /// Its message folders, base first; English is beneath all of them.
    pub fn chain(self) -> &'static [&'static str] {
        self.language().chain
    }

    pub fn dir(self) -> Dir {
        self.language().dir
    }

    pub fn script(self) -> Script {
        self.language().script
    }

    /// The path prefix of its public pages ("/es"; "" for English).
    pub fn prefix(self) -> &'static str {
        self.language().prefix
    }

    pub fn hreflang(self) -> &'static str {
        self.language().hreflang
    }

    pub fn og(self) -> &'static str {
        self.language().og
    }

    pub fn numbers(self) -> Numbers {
        self.language().numbers
    }

    /// Its cardinal plural forms, CLDR's rules.
    pub fn plurals(self) -> &'static [PluralRule] {
        self.language().plurals
    }

    /// The categories its plural forms are named by ("one", "many",
    /// "other"), in CLDR's order.
    pub fn plural_categories(self) -> impl Iterator<Item = &'static str> {
        self.plurals().iter().map(|p| p.category)
    }
}
