//! The language of a card side's text on screen, from what a real label
//! says: the side's speech blocks or the language its writer picked. A side
//! nothing says the language of is marked unknown (`lang=""`), so a page
//! in another language never claims it; nothing here guesses from the
//! letters.
//!
//! A side in one language says so on its container. A side whose blocks
//! name several languages marks each run whose words its blocks spell
//! exactly (`<span lang>`), and leaves the rest unknown: a block respelled
//! to read aloud (kana for kanji, a formula for its symbols) has no run
//! to mark.

use flash_core::speech::SpeechBlock;

/// A side as a page shows it: its language for the container (empty when
/// unknown or when its runs carry theirs), and, for a side whose runs
/// were marked, the HTML to show instead of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LabeledSide {
    pub lang: String,
    pub html: Option<String>,
}

/// `html` is the side's sanitized HTML, None for a plain side whose words
/// are `text`; `blocks` its speech blocks, `picked` the language its
/// writer picked.
pub fn label_side(
    html: Option<&str>,
    text: &str,
    blocks: Option<&[SpeechBlock]>,
    picked: Option<&str>,
) -> LabeledSide {
    let mut langs: Vec<&str> = Vec::new();
    for b in blocks.unwrap_or_default() {
        if let SpeechBlock::Say { lang, .. } = b {
            if !langs.contains(&lang.as_str()) {
                langs.push(lang);
            }
        }
    }
    match langs.as_slice() {
        [one] => LabeledSide {
            lang: (*one).to_string(),
            html: None,
        },
        [] => LabeledSide {
            lang: picked
                .and_then(flash_core::locale::normalize)
                .unwrap_or_default(),
            html: None,
        },
        _ => {
            let source = match html {
                Some(h) => h.to_string(),
                None => escape(text).replace('\n', "<br>"),
            };
            let runs: Vec<(&str, String)> = blocks
                .unwrap_or_default()
                .iter()
                .filter_map(|b| match b {
                    SpeechBlock::Say { lang, text } if !text.is_empty() => {
                        Some((lang.as_str(), escape(text)))
                    }
                    _ => None,
                })
                .collect();
            LabeledSide {
                lang: String::new(),
                html: mark_runs(&source, &runs),
            }
        }
    }
}

/// Text as sanitized HTML spells it.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\u{a0}', "&nbsp;")
}

/// The spans of `html` that are text, outside any tag (a quoted `>` in an
/// attribute does not end its tag).
fn text_spans(html: &str) -> Vec<(usize, usize)> {
    let bytes = html.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    let mut start = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if i > start {
                spans.push((start, i));
            }
            let mut quote: Option<u8> = None;
            i += 1;
            while i < bytes.len() {
                match (quote, bytes[i]) {
                    (None, b'"' | b'\'') => quote = Some(bytes[i]),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => break,
                    _ => {}
                }
                i += 1;
            }
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < bytes.len() {
        spans.push((start, bytes.len()));
    }
    spans
}

/// `html` with each run found in order, inside one stretch of text,
/// wrapped in its language; None when no run was found.
fn mark_runs(html: &str, runs: &[(&str, String)]) -> Option<String> {
    let spans = text_spans(html);
    let mut marks: Vec<(usize, usize, &str)> = Vec::new();
    let mut at = 0;
    for (lang, needle) in runs {
        let found = spans.iter().find_map(|&(s, e)| {
            let from = s.max(at);
            if from >= e {
                return None;
            }
            html[from..e].find(needle.as_str()).map(|i| from + i)
        });
        if let Some(start) = found {
            let end = start + needle.len();
            marks.push((start, end, lang));
            at = end;
        }
    }
    if marks.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(html.len() + marks.len() * 24);
    let mut last = 0;
    for (start, end, lang) in marks {
        out.push_str(&html[last..start]);
        out.push_str("<span lang=\"");
        out.push_str(lang);
        out.push_str("\">");
        out.push_str(&html[start..end]);
        out.push_str("</span>");
        last = end;
    }
    out.push_str(&html[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_core::speech::Cue;

    fn say(lang: &str, text: &str) -> SpeechBlock {
        SpeechBlock::Say {
            lang: lang.into(),
            text: text.into(),
        }
    }

    #[test]
    fn one_language_names_the_side() {
        let blocks = [say("ja", "ねこ")];
        let side = label_side(Some("<p>猫</p>"), "猫", Some(&blocks), Some("en"));
        assert_eq!(
            side,
            LabeledSide {
                lang: "ja".into(),
                html: None
            }
        );
    }

    #[test]
    fn nothing_real_leaves_it_unknown() {
        let side = label_side(None, "猫", None, None);
        assert_eq!(side, LabeledSide::default());
        let cues = [SpeechBlock::Cue { say: Cue::Look }];
        assert_eq!(label_side(None, "x", Some(&cues), None).lang, "");
        assert_eq!(label_side(None, "x", None, Some("es-mx")).lang, "es-MX");
        assert_eq!(label_side(None, "x", None, Some("nonsense!")).lang, "");
    }

    #[test]
    fn several_languages_mark_the_runs_they_spell() {
        let blocks = [say("ja", "猫"), say("en", "cat & dog")];
        let side = label_side(
            Some(r#"<p title="a>b">猫 (cat &amp; dog)</p>"#),
            "",
            Some(&blocks),
            None,
        );
        assert_eq!(side.lang, "");
        assert_eq!(
            side.html.as_deref(),
            Some(
                r#"<p title="a>b"><span lang="ja">猫</span> (<span lang="en">cat &amp; dog</span>)</p>"#
            )
        );
        // A plain side keeps its lines.
        let side = label_side(
            None,
            "كتاب\nbook",
            Some(&[say("ar", "كتاب"), say("en", "book")]),
            None,
        );
        assert_eq!(
            side.html.as_deref(),
            Some(r#"<span lang="ar">كتاب</span><br><span lang="en">book</span>"#)
        );
        // A block respelled to read aloud has no run; nothing found leaves the side as it is.
        let side = label_side(
            Some("<p>猫</p>"),
            "",
            Some(&[say("ja", "ねこ"), say("en", "cat")]),
            None,
        );
        assert_eq!(
            side,
            LabeledSide {
                lang: String::new(),
                html: None
            }
        );
    }
}
