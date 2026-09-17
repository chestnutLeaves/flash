//! The voices the sidecar offers, as a static table: settings are
//! validated against it without the network, the picker is grouped
//! by it, and a sidecar whose list drifts from it is a boot warning.
//! Ids are the engine's own (`af_heart`); the name a person sees is
//! the part after the underscore, capitalised.
//!
//! The engine also ships Spanish, French, Italian, Portuguese and Hindi
//! voices, and none is offered: those languages have no dictionary of
//! their own in the engine and were trained on next to nothing (its own
//! voice card grades them C or not at all), so they come out garbled or
//! with an English accent. The three languages here read as themselves.

/// The language a voice speaks, from the id's first letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    AmericanEnglish,
    BritishEnglish,
    Japanese,
    Mandarin,
}

impl Lang {
    pub const ALL: &'static [Lang] = &[
        Lang::AmericanEnglish,
        Lang::BritishEnglish,
        Lang::Japanese,
        Lang::Mandarin,
    ];

    /// The engine's one-letter code.
    pub fn code(self) -> &'static str {
        match self {
            Lang::AmericanEnglish => "a",
            Lang::BritishEnglish => "b",
            Lang::Japanese => "j",
            Lang::Mandarin => "z",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::AmericanEnglish => "American English",
            Lang::BritishEnglish => "British English",
            Lang::Japanese => "Japanese",
            Lang::Mandarin => "Mandarin",
        }
    }

    /// The picker's demo line in this language, with `{Name}` where the
    /// voice's spoken name goes. Each voice introduces itself in its own
    /// tongue: the engine's front end for a language reads that language,
    /// and English words through the Japanese one come out as nothing a
    /// listener would keep.
    pub fn sample_line(self) -> &'static str {
        match self {
            Lang::AmericanEnglish | Lang::BritishEnglish => SAMPLE_EN,
            Lang::Japanese => {
                "こんにちは、{Name}です。カードは全部読みました。二回も。始めましょうか？"
            }
            Lang::Mandarin => "你好，我是{Name}。你的卡片我都读过了。读了两遍。开始吧？",
        }
    }

    fn from_code(c: u8) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|l| l.code().as_bytes()[0] == c)
    }
}

/// The demo line of the English voices; the other languages say the
/// same thing in their own words.
pub const SAMPLE_EN: &str = "Hi, I'm {Name}. I've read all your cards. Twice. Shall we?";

/// The names the non-Latin front ends read: the picker still shows the
/// Latin name, but the clip says this one. Every Japanese and Mandarin
/// voice is here; the test below holds the table to the id list.
const SPOKEN_NAMES: &[(&str, &str)] = &[
    ("jf_alpha", "アルファ"),
    ("jf_gongitsune", "ごんぎつね"),
    ("jf_nezumi", "ねずみ"),
    ("jf_tebukuro", "てぶくろ"),
    ("jm_kumo", "くも"),
    ("zf_xiaobei", "晓北"),
    ("zf_xiaoni", "晓妮"),
    ("zf_xiaoxiao", "晓晓"),
    ("zf_xiaoyi", "晓伊"),
    ("zm_yunjian", "云健"),
    ("zm_yunxi", "云希"),
    ("zm_yunxia", "云夏"),
    ("zm_yunyang", "云扬"),
];

/// One voice: its id, its language, and whether the engine lists it
/// as a female voice (the second letter of the id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceInfo {
    pub id: &'static str,
    pub lang: Lang,
    pub female: bool,
}

const IDS: &[&str] = &[
    "af_alloy",
    "af_aoede",
    "af_bella",
    "af_heart",
    "af_jessica",
    "af_kore",
    "af_nicole",
    "af_nova",
    "af_river",
    "af_sarah",
    "af_sky",
    "am_adam",
    "am_echo",
    "am_eric",
    "am_fenrir",
    "am_liam",
    "am_michael",
    "am_onyx",
    "am_puck",
    "am_santa",
    "bf_alice",
    "bf_emma",
    "bf_isabella",
    "bf_lily",
    "bm_daniel",
    "bm_fable",
    "bm_george",
    "bm_lewis",
    "jf_alpha",
    "jf_gongitsune",
    "jf_nezumi",
    "jf_tebukuro",
    "jm_kumo",
    "zf_xiaobei",
    "zf_xiaoni",
    "zf_xiaoxiao",
    "zf_xiaoyi",
    "zm_yunjian",
    "zm_yunxi",
    "zm_yunxia",
    "zm_yunyang",
];

/// Every voice, in the engine's order.
pub fn all() -> impl Iterator<Item = VoiceInfo> {
    IDS.iter().filter_map(|id| parse(id))
}

fn parse(id: &'static str) -> Option<VoiceInfo> {
    let b = id.as_bytes();
    if b.len() < 4 || b[2] != b'_' {
        return None;
    }
    Some(VoiceInfo {
        id,
        lang: Lang::from_code(b[0])?,
        female: b[1] == b'f',
    })
}

pub fn find(id: &str) -> Option<VoiceInfo> {
    IDS.iter()
        .find(|known| **known == id)
        .and_then(|known| parse(known))
}

/// "af_heart" is "Heart".
pub fn display_name(id: &str) -> String {
    let name = id.split_once('_').map(|(_, n)| n).unwrap_or(id);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The name a voice's own front end reads aloud: the Latin name where
/// the language writes in Latin letters, the native spelling elsewhere.
pub fn spoken_name(id: &str) -> String {
    SPOKEN_NAMES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| display_name(id))
}

/// The voices by language, in the order the picker lists them.
pub fn grouped() -> Vec<(Lang, Vec<VoiceInfo>)> {
    Lang::ALL
        .iter()
        .map(|&lang| (lang, all().filter(|v| v.lang == lang).collect()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_forty_one_well_formed_unique_voices() {
        let voices: Vec<VoiceInfo> = all().collect();
        assert_eq!(voices.len(), 41);
        assert_eq!(voices.len(), IDS.len());
        let mut ids: Vec<&str> = voices.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 41);
        for v in &voices {
            let b = v.id.as_bytes();
            assert!(b"abjz".contains(&b[0]), "{}", v.id);
            assert!(b"fm".contains(&b[1]), "{}", v.id);
            assert!(b[3..].iter().all(|c| c.is_ascii_lowercase()), "{}", v.id);
        }
        for (_, group) in grouped() {
            let mut names: Vec<String> = group.iter().map(|v| display_name(v.id)).collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), group.len());
        }
    }

    #[test]
    fn names_and_lookups() {
        assert_eq!(display_name("af_heart"), "Heart");
        assert_eq!(display_name("bm_fable"), "Fable");
        assert_eq!(find("af_heart").unwrap().lang, Lang::AmericanEnglish);
        assert!(find("af_heart").unwrap().female);
        assert!(!find("bm_george").unwrap().female);
        assert!(find("zz_nobody").is_none());
        assert!(find("AF_HEART").is_none());
        // The engine's espeak languages are not offered.
        assert!(find("ef_dora").is_none());
        assert!(find("ff_siwis").is_none());
        assert!(find("hf_alpha").is_none());
        assert_eq!(grouped().len(), 4);
    }

    #[test]
    fn every_language_has_a_demo_line_and_every_non_latin_voice_a_spoken_name() {
        for lang in Lang::ALL {
            let line = lang.sample_line();
            assert!(line.contains("{Name}"), "{lang:?}");
            assert!(!line.trim().is_empty(), "{lang:?}");
        }
        assert_eq!(Lang::AmericanEnglish.sample_line(), SAMPLE_EN);
        assert_eq!(Lang::BritishEnglish.sample_line(), SAMPLE_EN);
        for v in all() {
            let latin = matches!(v.lang, Lang::AmericanEnglish | Lang::BritishEnglish);
            let spoken = spoken_name(v.id);
            if latin {
                assert_eq!(spoken, display_name(v.id), "{}", v.id);
            } else {
                assert_ne!(spoken, display_name(v.id), "{} has no native name", v.id);
                assert!(spoken.chars().all(|c| !c.is_ascii()), "{}", v.id);
            }
        }
        // The table names only voices that exist.
        for (id, _) in SPOKEN_NAMES {
            assert!(find(id).is_some(), "{id}");
        }
        assert_eq!(spoken_name("zz_nobody"), "Nobody");
    }
}
