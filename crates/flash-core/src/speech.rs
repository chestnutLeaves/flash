//! How a card side is spoken, and in which language: an ordered list of
//! blocks. A block is words in one language, with a region where the
//! accent matters (`{"lang":"es-MX","text":"¿Dónde está?"}`), or a cue
//! said in the listener's own language in place of something
//! (`{"say":"equation"}`).
//!
//! The blocks follow the card as it is written: they never reword it or
//! change its order. What they leave out is what is not to be read aloud
//! (a citation, a romanization next to the script it spells, an emoji,
//! an image), and what they change is only how a word is said (a kanji
//! written in kana when its reading is ambiguous, a short formula in
//! words). A side may have no blocks.

use serde::{Deserialize, Serialize};

use crate::languages;

/// Blocks one side may hold. A card side is at most `MAX_SIDE_LEN`
/// characters, and a block per word of a long side is still far fewer.
pub const MAX_BLOCKS: usize = 64;

/// A cue said in the listener's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cue {
    /// A formula too long to read: "Equation on screen."
    Equation,
    /// Something not read aloud: "Look on screen."
    Look,
    /// A cloze gap on the side that asks for it.
    Blank,
}

impl Cue {
    pub fn as_str(self) -> &'static str {
        match self {
            Cue::Equation => "equation",
            Cue::Look => "look",
            Cue::Blank => "blank",
        }
    }
}

/// One block of a side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpeechBlock {
    Say { lang: String, text: String },
    Cue { say: Cue },
}

/// Cantonese, by its own code or as the Chinese of Hong Kong or Macau.
/// The table has no code for it, and its characters read as Mandarin
/// would teach the wrong sounds; it is looked at instead.
fn is_cantonese(tag: &str) -> bool {
    let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    match (parts.next(), parts.next()) {
        (Some("yue"), _) => true,
        (Some("zh"), Some("yue" | "hk" | "mo")) => true,
        (Some("zh"), Some("hant")) => matches!(parts.next(), Some("hk" | "mo")),
        _ => false,
    }
}

/// Why a side's blocks were refused, as a sentence an assistant can act
/// on. Words in Cantonese become the look cue.
pub fn validate(blocks: &[SpeechBlock]) -> Result<Vec<SpeechBlock>, String> {
    if blocks.len() > MAX_BLOCKS {
        return Err(format!(
            "a side may have at most {MAX_BLOCKS} speech blocks"
        ));
    }
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        match block {
            SpeechBlock::Say { lang, text } => {
                if is_cantonese(lang) {
                    if out.last() != Some(&SpeechBlock::Cue { say: Cue::Look }) {
                        out.push(SpeechBlock::Cue { say: Cue::Look });
                    }
                    continue;
                }
                let tag = languages::normalize_tag(lang).ok_or_else(|| {
                    format!("unknown speech language {lang:?}: use a code like \"en\", \"ja\" or \"es-MX\"")
                })?;
                let text = crate::text::nfc(&text.split_whitespace().collect::<Vec<_>>().join(" "));
                if text.is_empty() {
                    continue;
                }
                if text.chars().count() > crate::MAX_SIDE_LEN {
                    return Err("a speech block is longer than a card side".to_string());
                }
                out.push(SpeechBlock::Say { lang: tag, text });
            }
            SpeechBlock::Cue { say } => out.push(SpeechBlock::Cue { say: *say }),
        }
    }
    Ok(out)
}

/// The blocks as stored: JSON, or None for no blocks.
pub fn to_json(blocks: &[SpeechBlock]) -> String {
    serde_json::to_string(blocks).unwrap_or_else(|_| "[]".to_string())
}

/// Stored blocks read back; None for a column that is empty or no
/// longer reads as blocks (the side then has none).
pub fn from_json(json: Option<&str>) -> Option<Vec<SpeechBlock>> {
    let blocks: Vec<SpeechBlock> = serde_json::from_str(json?).ok()?;
    validate(&blocks).ok()
}

/// The languages a side speaks, as tags, in the order they first occur.
pub fn languages_of(blocks: &[SpeechBlock]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for block in blocks {
        if let SpeechBlock::Say { lang, .. } = block {
            if !out.iter().any(|l| l == lang) {
                out.push(lang.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn say(lang: &str, text: &str) -> SpeechBlock {
        SpeechBlock::Say {
            lang: lang.into(),
            text: text.into(),
        }
    }

    #[test]
    fn blocks_read_and_write_as_plain_json() {
        let json = r#"[{"lang":"en","text":"How do you say cat?"},{"lang":"ja","text":"ねこ"},{"say":"equation"}]"#;
        let blocks: Vec<SpeechBlock> = serde_json::from_str(json).unwrap();
        assert_eq!(
            blocks,
            vec![
                say("en", "How do you say cat?"),
                say("ja", "ねこ"),
                SpeechBlock::Cue { say: Cue::Equation }
            ]
        );
        assert_eq!(to_json(&blocks), json);
        assert_eq!(from_json(Some(json)), Some(blocks));
        assert_eq!(from_json(Some("not json")), None);
        assert_eq!(from_json(None), None);
        // A cue nobody knows does not read.
        assert!(serde_json::from_str::<Vec<SpeechBlock>>(r#"[{"say":"sing"}]"#).is_err());
    }

    #[test]
    fn validation_normalizes_tags_and_words_and_refuses_strangers() {
        let got = validate(&[
            say("ES-mx", "  ¿Dónde   está? "),
            say("en", "   "),
            SpeechBlock::Cue { say: Cue::Look },
        ])
        .unwrap();
        assert_eq!(
            got,
            vec![
                say("es-MX", "¿Dónde está?"),
                SpeechBlock::Cue { say: Cue::Look }
            ]
        );
        assert!(validate(&[say("klingon", "Qapla'")])
            .unwrap_err()
            .starts_with("unknown speech language"));
        // Cantonese is looked at, however it is tagged, and two pieces
        // of it in a row are one cue.
        for tag in ["yue", "yue-HK", "zh-HK", "zh_MO", "zh-yue", "zh-Hant-HK"] {
            assert_eq!(
                validate(&[say("en", "Thank you is"), say(tag, "唔該"), say(tag, "晒")]).unwrap(),
                vec![
                    say("en", "Thank you is"),
                    SpeechBlock::Cue { say: Cue::Look }
                ],
                "{tag}"
            );
        }
        assert_eq!(
            validate(&[say("zh-TW", "腳踏車")]).unwrap(),
            vec![say("zh-TW", "腳踏車")],
            "Taiwan's Chinese is Mandarin"
        );
        let many = vec![say("en", "a"); MAX_BLOCKS + 1];
        assert!(validate(&many).is_err());
        assert_eq!(
            languages_of(&[say("en", "a"), say("ja", "b"), say("en", "c")]),
            vec!["en", "ja"]
        );
    }
}
