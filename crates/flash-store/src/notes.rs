//! Note types and the pure note -> cards expansion the editor relies on.
//! A note is the editable unit (Front/Back, or cloze Text/Extra); cards
//! are what the scheduler sees. The expansion here mirrors the .apkg
//! importer exactly, so a card authored in the editor is indistinguishable
//! from one imported from Anki: same plain text, same precomputed cloze
//! views, same export grouping.

use flash_core::speech::{Cue, SpeechBlock};
use flash_core::{validate_card_text, CardText};

use crate::import::cloze;
use crate::repo::cards::CardExtras;
use crate::richtext::{self, SanitizedHtml};

/// Ceiling on one field's HTML (matches the importer's rich-side cap).
pub const MAX_FIELD_HTML: usize = 40_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteType {
    Basic,
    BasicReversed,
    BasicTyped,
    Cloze,
}

impl NoteType {
    pub const ALL: [NoteType; 4] = [
        NoteType::Basic,
        NoteType::BasicReversed,
        NoteType::BasicTyped,
        NoteType::Cloze,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NoteType::Basic => "basic",
            NoteType::BasicReversed => "basic_reversed",
            NoteType::BasicTyped => "basic_typed",
            NoteType::Cloze => "cloze",
        }
    }

    pub fn parse(s: &str) -> Option<NoteType> {
        NoteType::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Human label, as shown in the editor's type picker.
    pub fn label(self) -> &'static str {
        match self {
            NoteType::Basic => "Basic",
            NoteType::BasicReversed => "Basic (and reversed card)",
            NoteType::BasicTyped => "Basic (type in the answer)",
            NoteType::Cloze => "Cloze",
        }
    }
}

/// One card a note expands to. `ord` is the stable slot within the note
/// (reversed: 0 front->back, 1 back->front; cloze: index - 1).
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedCard {
    pub ord: u32,
    pub text: CardText,
    pub extras: CardExtras,
}

/// Rich HTML worth storing, or None for a plain side (same rule as the
/// importer: markup-free content stays a plain card).
fn rich(html: &SanitizedHtml) -> Option<SanitizedHtml> {
    if html.trim().contains('<') {
        Some(html.clone().trimmed())
    } else {
        None
    }
}

fn side(html: &str) -> String {
    richtext::html_to_text(html)
}

/// Why a note could not be expanded into cards. `Display` is the English
/// a log, a sync client or an API client reads; a page words it in its
/// reader's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteError {
    /// A field is over `MAX_FIELD_HTML` bytes of HTML.
    FieldTooLong,
    /// A cloze note with no `{{c1::…}}` deletion.
    NoCloze,
    /// The deletion numbered so has an empty answer.
    EmptyCloze(u32),
    /// A card's text was refused.
    Card(flash_core::CardTextError),
    /// The card of the deletion numbered so was refused.
    ClozeCard(u32, flash_core::CardTextError),
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoteError::FieldTooLong => write!(f, "a field exceeds {MAX_FIELD_HTML} bytes of HTML"),
            NoteError::NoCloze => f.write_str("add at least one {{c1::…}} cloze deletion"),
            NoteError::EmptyCloze(idx) => write!(f, "cloze c{idx} has an empty answer"),
            NoteError::Card(e) => e.fmt(f),
            NoteError::ClozeCard(idx, e) => write!(f, "cloze c{idx}: {e}"),
        }
    }
}

impl std::error::Error for NoteError {}

impl From<flash_core::CardTextError> for NoteError {
    fn from(e: flash_core::CardTextError) -> Self {
        NoteError::Card(e)
    }
}

/// Expands sanitized note fields into cards. The fields arrive as
/// `SanitizedHtml`, so a caller cannot hand the expansion raw editor
/// input.
pub fn generate_cards(
    kind: NoteType,
    front_html: &SanitizedHtml,
    back_html: &SanitizedHtml,
) -> Result<Vec<GeneratedCard>, NoteError> {
    if front_html.len() > MAX_FIELD_HTML || back_html.len() > MAX_FIELD_HTML {
        return Err(NoteError::FieldTooLong);
    }
    match kind {
        NoteType::Basic | NoteType::BasicReversed | NoteType::BasicTyped => {
            let front = side(front_html);
            let back = side(back_html);
            let text = validate_card_text(&front, &back)?;
            let mut out = vec![GeneratedCard {
                ord: 0,
                text: text.clone(),
                extras: CardExtras {
                    front_html: rich(front_html),
                    back_html: rich(back_html),
                    type_answer: (kind == NoteType::BasicTyped).then(|| text.back.clone()),
                    ..CardExtras::default()
                },
            }];
            if kind == NoteType::BasicReversed {
                out.push(GeneratedCard {
                    ord: 1,
                    text: CardText {
                        front: text.back.clone(),
                        back: text.front.clone(),
                    },
                    extras: CardExtras {
                        front_html: rich(back_html),
                        back_html: rich(front_html),
                        ..CardExtras::default()
                    },
                });
            }
            Ok(out)
        }
        NoteType::Cloze => {
            let cz = cloze::parse(front_html).ok_or(NoteError::NoCloze)?;
            let text_plain = side(front_html);
            let extra_plain = side(back_html);
            let source = format!("{text_plain}\u{1f}{extra_plain}");
            let mut out = Vec::new();
            for idx in cz.indices() {
                let front = side(&cz.front(idx, false));
                let front_html = richtext::sanitize_with_media(&cz.front(idx, true));
                let answers = cz.answers(idx).join("; ");
                let answer_text = side(&answers);
                if answer_text.is_empty() {
                    return Err(NoteError::EmptyCloze(idx));
                }
                let back = if extra_plain.is_empty() {
                    answer_text
                } else {
                    format!("{answer_text}\n{extra_plain}")
                };
                let filled = cz.filled(idx);
                let back_html = richtext::sanitize_with_media(&if back_html.trim().is_empty() {
                    filled
                } else {
                    format!("{filled}<br>{back_html}")
                });
                let text =
                    validate_card_text(&front, &back).map_err(|e| NoteError::ClozeCard(idx, e))?;
                out.push(GeneratedCard {
                    ord: idx - 1,
                    text,
                    extras: CardExtras {
                        front_html: Some(front_html),
                        back_html: Some(back_html),
                        cloze_text: Some(source.clone()),
                        cloze_index: Some(idx),
                        type_answer: None,
                    },
                });
            }
            Ok(out)
        }
    }
}

/// The speech blocks one generated card speaks, from its note's blocks
/// (`flash_core::speech`): a basic card's are the note's; a reversed
/// card's second slot swaps them; a cloze card's front reads its own gap
/// as the blank cue and every other gap as its answer, and its back reads
/// the text whole, then the note's extra. A side the note has no blocks
/// for has none, and a cloze card's back needs the text's blocks as much
/// as its front does.
pub fn card_speech(
    kind: NoteType,
    ord: u32,
    front: Option<&[SpeechBlock]>,
    back: Option<&[SpeechBlock]>,
) -> (Option<Vec<SpeechBlock>>, Option<Vec<SpeechBlock>>) {
    match kind {
        NoteType::Basic | NoteType::BasicTyped => {
            (front.map(<[_]>::to_vec), back.map(<[_]>::to_vec))
        }
        NoteType::BasicReversed if ord == 1 => (back.map(<[_]>::to_vec), front.map(<[_]>::to_vec)),
        NoteType::BasicReversed => (front.map(<[_]>::to_vec), back.map(<[_]>::to_vec)),
        NoteType::Cloze => {
            let Some(text) = front else {
                return (None, None);
            };
            let active = ord + 1;
            let asked = cloze_speech(text, Some(active));
            let mut answered = cloze_speech(text, None);
            if let Some(extra) = back {
                answered.extend(extra.iter().cloned());
            }
            (Some(asked), Some(answered))
        }
    }
}

/// A cloze text's blocks with its gaps resolved: the gap numbered
/// `active` becomes the blank cue, every other gap its answer.
fn cloze_speech(blocks: &[SpeechBlock], active: Option<u32>) -> Vec<SpeechBlock> {
    let mut out = Vec::new();
    for block in blocks {
        let SpeechBlock::Say { lang, text } = block else {
            out.push(block.clone());
            continue;
        };
        let mut words = String::new();
        let mut rest = text.as_str();
        while let Some(start) = rest.find("{{c") {
            let after = &rest[start + 3..];
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            let body_at = digits.len();
            let (Ok(n), Some(body)) = (digits.parse::<u32>(), after[body_at..].strip_prefix("::"))
            else {
                words.push_str(&rest[..start + 3]);
                rest = after;
                continue;
            };
            let Some(end) = body.find("}}") else {
                break;
            };
            let answer = body[..end].split("::").next().unwrap_or("");
            words.push_str(&rest[..start]);
            if Some(n) == active {
                push_say(&mut out, lang, &words);
                words.clear();
                out.push(SpeechBlock::Cue { say: Cue::Blank });
            } else {
                words.push_str(answer);
            }
            rest = &body[end + 2..];
        }
        words.push_str(rest);
        push_say(&mut out, lang, &words);
    }
    out
}

fn push_say(out: &mut Vec<SpeechBlock>, lang: &str, words: &str) {
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if !words.is_empty() {
        out.push(SpeechBlock::Say {
            lang: lang.to_string(),
            text: words,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests write fields as strings; the editor path sanitizes them
    /// before generation, so the tests do the same here.
    fn generate_cards(
        kind: NoteType,
        front: &str,
        back: &str,
    ) -> Result<Vec<GeneratedCard>, NoteError> {
        super::generate_cards(
            kind,
            &richtext::sanitize_with_media(front),
            &richtext::sanitize_with_media(back),
        )
    }

    #[test]
    fn note_type_round_trips() {
        for t in NoteType::ALL {
            assert_eq!(NoteType::parse(t.as_str()), Some(t));
        }
        assert_eq!(NoteType::parse("nope"), None);
    }

    #[test]
    fn basic_plain_and_rich() {
        let cards = generate_cards(NoteType::Basic, "Hello", "World").unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].text.front, "Hello");
        assert!(
            cards[0].extras.front_html.is_none(),
            "no markup = plain card"
        );

        let cards = generate_cards(NoteType::Basic, "<b>Hi</b> there", "a<br>b").unwrap();
        assert_eq!(cards[0].text.front, "Hi there");
        assert_eq!(cards[0].text.back, "a\nb");
        assert_eq!(
            cards[0].extras.front_html.as_deref(),
            Some("<b>Hi</b> there")
        );
        assert_eq!(cards[0].extras.back_html.as_deref(), Some("a<br>b"));
    }

    #[test]
    fn reversed_swaps_sides_and_html() {
        let cards = generate_cards(NoteType::BasicReversed, "<i>Q</i>", "A").unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].ord, cards[1].ord), (0, 1));
        assert_eq!(cards[1].text.front, "A");
        assert_eq!(cards[1].text.back, "Q");
        assert_eq!(cards[1].extras.back_html.as_deref(), Some("<i>Q</i>"));
        assert!(cards[1].extras.front_html.is_none());
    }

    #[test]
    fn typed_sets_answer() {
        let cards = generate_cards(NoteType::BasicTyped, "Q", "<b>Ans</b>").unwrap();
        assert_eq!(cards[0].extras.type_answer.as_deref(), Some("Ans"));
    }

    #[test]
    fn cloze_fans_out_per_index() {
        let cards = generate_cards(
            NoteType::Cloze,
            "{{c1::Paris}} is the capital of {{c2::France::country}}",
            "<i>Geo</i>",
        )
        .unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].ord, 0);
        assert_eq!(cards[0].extras.cloze_index, Some(1));
        assert_eq!(cards[0].text.front, "[...] is the capital of France");
        assert_eq!(cards[0].text.back, "Paris\nGeo");
        let fh = cards[0].extras.front_html.as_deref().unwrap();
        assert!(fh.contains("cloze-blank"), "{fh}");
        let bh = cards[0].extras.back_html.as_deref().unwrap();
        assert!(
            bh.contains("cloze-answer") && bh.contains("<i>Geo</i>"),
            "{bh}"
        );
        assert_eq!(cards[1].text.front, "Paris is the capital of [country]");
        assert_eq!(
            cards[1].extras.cloze_text.as_deref(),
            Some("{{c1::Paris}} is the capital of {{c2::France::country}}\u{1f}Geo")
        );
        assert_eq!(cards[1].extras.cloze_text, cards[0].extras.cloze_text);
    }

    #[test]
    fn cloze_without_markup_is_rejected() {
        let err = generate_cards(NoteType::Cloze, "no blanks here", "").unwrap_err();
        assert_eq!(err, NoteError::NoCloze);
        assert!(err.to_string().contains("c1"), "{err}");
    }

    #[test]
    fn cloze_empty_answer_is_rejected() {
        let err = generate_cards(NoteType::Cloze, "x {{c1::}} y", "").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("empty") || text.contains("c1"), "{err}");
    }

    #[test]
    fn empty_sides_are_rejected() {
        assert!(generate_cards(NoteType::Basic, "", "b").is_err());
        assert!(generate_cards(NoteType::Basic, "<br>", "b").is_err());
        assert!(generate_cards(NoteType::Basic, "a", " ").is_err());
    }

    fn say(lang: &str, text: &str) -> SpeechBlock {
        SpeechBlock::Say {
            lang: lang.into(),
            text: text.into(),
        }
    }

    #[test]
    fn each_card_speaks_its_own_sides() {
        let front = [say("en", "How do you say cat?")];
        let back = [say("ja", "ねこ")];
        let (f, b) = card_speech(NoteType::Basic, 0, Some(&front), Some(&back));
        assert_eq!((f.unwrap(), b.unwrap()), (front.to_vec(), back.to_vec()));
        // The reversed card asks in Japanese and answers in English.
        let (f, b) = card_speech(NoteType::BasicReversed, 1, Some(&front), Some(&back));
        assert_eq!((f.unwrap(), b.unwrap()), (back.to_vec(), front.to_vec()));
        // A side nobody labeled stays unlabeled.
        let (f, b) = card_speech(NoteType::Basic, 0, Some(&front), None);
        assert!(f.is_some() && b.is_none());
    }

    #[test]
    fn a_cloze_card_reads_its_own_gap_as_the_blank() {
        let text = [say(
            "fr",
            "{{c1::Paris}} est la capitale de la {{c2::France::pays}}.",
        )];
        let extra = [say("en", "Geography")];
        let (f, b) = card_speech(NoteType::Cloze, 0, Some(&text), Some(&extra));
        assert_eq!(
            f.unwrap(),
            vec![
                SpeechBlock::Cue { say: Cue::Blank },
                say("fr", "est la capitale de la France.")
            ]
        );
        assert_eq!(
            b.unwrap(),
            vec![
                say("fr", "Paris est la capitale de la France."),
                say("en", "Geography")
            ]
        );
        let (f, _) = card_speech(NoteType::Cloze, 1, Some(&text), None);
        assert_eq!(
            f.unwrap(),
            vec![
                say("fr", "Paris est la capitale de la"),
                SpeechBlock::Cue { say: Cue::Blank },
                say("fr", ".")
            ]
        );
        // Without the text's blocks, neither side can be read.
        assert_eq!(
            card_speech(NoteType::Cloze, 0, None, Some(&extra)),
            (None, None)
        );
        // Text that only looks like a gap passes through.
        let odd = [say("en", "a {{cx::b}} c")];
        assert_eq!(cloze_speech(&odd, Some(1)), odd.to_vec());
    }

    #[test]
    fn media_only_side_gets_a_label() {
        let cards = generate_cards(NoteType::Basic, "<img src=\"/media/7\">", "b").unwrap();
        assert_eq!(cards[0].text.front, "[image]");
        assert_eq!(
            richtext::media_ids(cards[0].extras.front_html.as_deref().unwrap()),
            vec![7]
        );
    }
}
