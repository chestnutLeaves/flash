//! From what the model wrote to what the note editor would have
//! accepted. A draft leaves here with sanitized HTML fields, tags the
//! store will take, and figure references resolved to media ids this
//! extraction owns; everything the model could have got wrong (a blank
//! side, a figure number that is not ours, sixty-one cards, a tag the
//! size of a paragraph) is corrected or dropped, never stored.

use std::collections::HashMap;

use flash_core::MediaId;
use flash_store::notes::NoteType;
use flash_store::richtext;

use super::provider::DraftNote;
use crate::bounds;
use crate::service::text_to_html;

/// Figure number, as the extraction numbered it, to the media row its
/// crop was ingested as.
pub type FigureMap = HashMap<u32, MediaId>;

/// Most drafts one generation may produce.
pub const MAX_DRAFTS: usize = 60;

/// Most tags on one generated card. The store allows far more; a card
/// the model wrote gets a few topic words, not a taxonomy.
pub const MAX_TAGS: usize = 3;

/// Most figures one extraction may flag.
pub const MAX_FIGURES: usize = 20;

/// An extraction as the pipeline may use it: figure boxes clamped to
/// the page, figures without area dropped, the list capped, captions
/// bounded, and the word estimate recomputed when the model left it
/// out.
pub fn extraction(mut raw: super::provider::Extraction) -> super::provider::Extraction {
    raw.figures.retain_mut(|f| {
        for v in f.bbox.iter_mut() {
            *v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        f.caption = f.caption.trim().chars().take(bounds::TITLE).collect();
        f.source >= 1 && f.page >= 1 && f.bbox[2] > f.bbox[0] && f.bbox[3] > f.bbox[1]
    });
    raw.figures.truncate(MAX_FIGURES);
    if raw.est_words == 0 {
        raw.est_words = raw.text.split_whitespace().count() as u32;
    }
    raw
}

/// A draft ready for the `ai_drafts` table and, on Keep, for
/// `NoteInput` unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidDraft {
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// Every media id the two sides embed, in order, deduped.
    pub figure_media_ids: Vec<i64>,
}

/// Validates a whole reply. Drafts that cannot become a note are
/// dropped; the rest are bounded and sanitized; at most `MAX_DRAFTS`
/// survive, in the model's order.
pub fn drafts(raw: Vec<DraftNote>, figures: &FigureMap) -> Vec<ValidDraft> {
    raw.into_iter()
        .filter_map(|d| draft(d, figures))
        .take(MAX_DRAFTS)
        .collect()
}

fn draft(d: DraftNote, figures: &FigureMap) -> Option<ValidDraft> {
    let front_html = field_html(&d.front, figures);
    let back_html = field_html(&d.back, figures);
    match d.note_type {
        NoteType::Cloze => {
            if !has_cloze(&front_html) {
                return None;
            }
        }
        NoteType::Basic | NoteType::BasicReversed | NoteType::BasicTyped => {
            // The core refuses an empty side on either end, so a draft
            // missing one would fail at Keep; better to lose it now.
            if side_is_empty(&front_html) || side_is_empty(&back_html) {
                return None;
            }
        }
    }
    let mut figure_media_ids = richtext::media_ids(&front_html);
    for id in richtext::media_ids(&back_html) {
        if !figure_media_ids.contains(&id) {
            figure_media_ids.push(id);
        }
    }
    Some(ValidDraft {
        note_type: d.note_type,
        front_html: front_html.into_string(),
        back_html: back_html.into_string(),
        tags: tags(&d.tags),
        figure_media_ids,
    })
}

/// The sentence the model wrote about an edit, as the thread shows it:
/// trimmed, one line, cut to the reason bound; `fallback` when the
/// model said nothing.
pub fn reply_sentence(raw: &str, fallback: &str) -> String {
    let mut out: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_at_boundary(&mut out, bounds::REASON);
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// Cuts `text` to at most `max` bytes without splitting a character.
fn truncate_at_boundary(text: &mut String, max: usize) {
    if text.len() <= max {
        return;
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
}

/// A side with neither text nor media.
fn side_is_empty(html: &str) -> bool {
    richtext::html_to_text(html).trim().is_empty()
}

/// The model's plain text as an editor field: escaped, paragraphs as
/// `<p>` when there are several, line breaks as `<br>`, figure tokens
/// as images of our own media, bounded, and sanitized last.
fn field_html(text: &str, figures: &FigureMap) -> richtext::SanitizedHtml {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut html = match paragraphs.as_slice() {
        [] => String::new(),
        [one] => text_to_html(one),
        many => many
            .iter()
            .map(|p| format!("<p>{}</p>", text_to_html(p)))
            .collect::<Vec<_>>()
            .join(""),
    };
    html = place_figures(&html, figures);
    truncate_at_boundary(&mut html, bounds::FIELD_HTML);
    // Sanitizing last rebalances anything the cut opened and is the
    // only way a value of this type comes to exist.
    richtext::sanitize_with_media(&html).trimmed()
}

/// `{{figure:N}}` becomes `<img src="/media/{id}">` for a figure this
/// extraction produced, and nothing at all for any other number.
fn place_figures(html: &str, figures: &FigureMap) -> String {
    const OPEN: &str = "{{figure:";
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        let after = &rest[at + OPEN.len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let tail = &after[digits.len()..];
        if !digits.is_empty() && tail.starts_with("}}") {
            if let Some(id) = digits.parse::<u32>().ok().and_then(|n| figures.get(&n)) {
                out.push_str(&format!(r#"<img src="/media/{}">"#, id.0));
            }
            rest = &tail[2..];
        } else {
            // Not a token after all; keep the text as written.
            out.push_str(OPEN);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// True when the text carries at least one `{{cN::answer}}` with a
/// non-empty answer, the same shape the store's cloze parser accepts.
/// Nested or hinted forms count as long as the first answer has text.
fn has_cloze(text: &str) -> bool {
    let mut rest = text;
    while let Some(at) = rest.find("{{c") {
        let after = &rest[at + 3..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let tail = &after[digits.len()..];
        if !digits.is_empty() && tail.starts_with("::") {
            let inner = &tail[2..];
            if let Some(end) = inner.find("}}") {
                let answer = inner[..end].split("::").next().unwrap_or("");
                if !answer.trim().is_empty() {
                    return true;
                }
            }
        }
        rest = after;
    }
    false
}

/// Tags the store takes: trimmed, lowercased, cut to the tag bound at a
/// character boundary, deduped, and at most `MAX_TAGS` of them.
fn tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let mut t = t.trim().to_lowercase();
        if t.len() > bounds::TAG {
            truncate_at_boundary(&mut t, bounds::TAG);
            t = t.trim_end().to_string();
        }
        if t.is_empty() || out.contains(&t) {
            continue;
        }
        out.push(t);
        if out.len() == MAX_TAGS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(note_type: NoteType, front: &str, back: &str) -> DraftNote {
        DraftNote {
            note_type,
            front: front.into(),
            back: back.into(),
            tags: vec![],
            figures: vec![],
        }
    }

    fn map(pairs: &[(u32, i64)]) -> FigureMap {
        pairs.iter().map(|(n, id)| (*n, MediaId(*id))).collect()
    }

    #[test]
    fn figures_become_our_images_or_nothing() {
        let figures = map(&[(1, 41), (2, 42)]);
        let out = drafts(
            vec![note(
                NoteType::Basic,
                "Label the valves {{figure:1}}",
                "See {{figure:2}} and {{figure:9}} and {{figure:x}}",
            )],
            &figures,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].front_html,
            r#"Label the valves <img src="/media/41">"#
        );
        assert_eq!(
            out[0].back_html,
            r#"See <img src="/media/42"> and  and {{figure:x}}"#
        );
        assert_eq!(out[0].figure_media_ids, vec![41, 42]);
    }

    #[test]
    fn a_figure_only_side_is_not_empty() {
        let figures = map(&[(1, 7)]);
        let out = drafts(
            vec![note(NoteType::Basic, "{{figure:1}}", "the mitral valve")],
            &figures,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].figure_media_ids, vec![7]);
        let gone = drafts(
            vec![note(NoteType::Basic, "{{figure:3}}", "the mitral valve")],
            &figures,
        );
        assert!(gone.is_empty(), "an unknown figure leaves nothing behind");
    }

    #[test]
    fn empty_sides_are_dropped_and_the_rest_capped() {
        let mut raw = vec![
            note(NoteType::Basic, "", "answer"),
            note(NoteType::Basic, "  \n ", "answer"),
            note(NoteType::BasicTyped, "question", ""),
            note(NoteType::Cloze, "no blanks here", ""),
            note(NoteType::Cloze, "{{c1::}} empty", ""),
        ];
        for i in 0..70 {
            raw.push(note(NoteType::Basic, &format!("q{i}"), "a"));
        }
        let out = drafts(raw, &FigureMap::new());
        assert_eq!(out.len(), MAX_DRAFTS);
        assert_eq!(out[0].front_html, "q0");
        assert_eq!(
            out[MAX_DRAFTS - 1].front_html,
            format!("q{}", MAX_DRAFTS - 1)
        );
    }

    #[test]
    fn tags_are_few_short_lowercase_and_unique() {
        let long = "x".repeat(bounds::TAG + 10);
        let d = DraftNote {
            tags: vec![
                " Anatomy ".into(),
                "anatomy".into(),
                "".into(),
                long.clone(),
                "heart".into(),
                "valves".into(),
            ],
            ..note(NoteType::Basic, "q", "a")
        };
        let out = drafts(vec![d], &FigureMap::new());
        let tags = &out[0].tags;
        assert_eq!(tags.len(), MAX_TAGS);
        assert_eq!(tags[0], "anatomy");
        assert_eq!(tags[1], "x".repeat(bounds::TAG));
        assert_eq!(tags[2], "heart");
    }

    #[test]
    fn cloze_markup_survives_and_is_required() {
        let out = drafts(
            vec![
                note(
                    NoteType::Cloze,
                    "{{c1::Paris}} is the capital of {{c2::France::country}}",
                    "",
                ),
                note(
                    NoteType::Cloze,
                    "{{c1::x}} <b>bold</b> & <script>alert(1)</script>",
                    "",
                ),
            ],
            &FigureMap::new(),
        );
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].front_html,
            "{{c1::Paris}} is the capital of {{c2::France::country}}"
        );
        assert_eq!(out[0].back_html, "");
        assert_eq!(
            out[1].front_html,
            "{{c1::x}} &lt;b&gt;bold&lt;/b&gt; &amp; &lt;script&gt;alert(1)&lt;/script&gt;"
        );
        assert!(!out[1].front_html.contains("<script"));
    }

    #[test]
    fn plain_text_becomes_paragraphs_and_breaks() {
        let out = drafts(
            vec![note(
                NoteType::Basic,
                "First line\nsecond line\n\nSecond paragraph <i>escaped</i>",
                "one\r\ntwo",
            )],
            &FigureMap::new(),
        );
        assert_eq!(
            out[0].front_html,
            "<p>First line<br>second line</p><p>Second paragraph &lt;i&gt;escaped&lt;/i&gt;</p>"
        );
        assert_eq!(out[0].back_html, "one<br>two");
    }

    #[test]
    fn extractions_keep_only_figures_with_area_on_a_page() {
        use crate::ai::provider::{Extraction, FigureRef};
        let fig = |n, page, bbox| FigureRef {
            n,
            source: 1,
            page,
            bbox,
            caption: format!(" figure {n} "),
        };
        let mut figures = vec![
            fig(1, 1, [0.1, 0.1, 0.9, 0.9]),
            fig(2, 0, [0.1, 0.1, 0.9, 0.9]),
            fig(3, 1, [0.5, 0.1, 0.5, 0.9]),
            fig(4, 1, [-1.0, 0.2, 2.0, f32::NAN]),
            FigureRef {
                source: 0,
                ..fig(5, 1, [0.1, 0.1, 0.9, 0.9])
            },
        ];
        for n in 6..40 {
            figures.push(fig(n, 2, [0.0, 0.0, 1.0, 1.0]));
        }
        let out = extraction(Extraction {
            text: "one two three".into(),
            figures,
            est_words: 0,
        });
        assert_eq!(out.figures.len(), MAX_FIGURES);
        assert_eq!(out.figures[0].caption, "figure 1");
        assert_eq!(
            out.figures[1].n, 6,
            "no page, no area, no box, no source: dropped"
        );
        assert_eq!(out.est_words, 3);
        let clamped = extraction(Extraction {
            text: String::new(),
            figures: vec![fig(1, 3, [-1.0, 0.2, 2.0, 0.8])],
            est_words: 7,
        });
        assert_eq!(clamped.figures[0].bbox, [0.0, 0.2, 1.0, 0.8]);
        assert_eq!(clamped.est_words, 7);
    }

    #[test]
    fn a_reply_sentence_is_one_bounded_line_or_the_fallback() {
        assert_eq!(
            reply_sentence("  Rewrote two\ncards.\n\n", "Rewrote 2 cards."),
            "Rewrote two cards."
        );
        assert_eq!(
            reply_sentence(" \n ", "Rewrote 2 cards."),
            "Rewrote 2 cards."
        );
        let long = "é".repeat(bounds::REASON);
        let cut = reply_sentence(&long, "x");
        assert!(cut.len() <= bounds::REASON);
        assert!(cut.chars().all(|c| c == 'é'));
    }

    #[test]
    fn oversized_fields_are_cut_at_the_bound_and_still_sanitized() {
        let long = "word ".repeat(bounds::FIELD_HTML / 4);
        let out = drafts(vec![note(NoteType::Basic, &long, "a")], &FigureMap::new());
        assert!(out[0].front_html.len() <= bounds::FIELD_HTML);
        assert!(out[0].front_html.starts_with("word word"));
    }
}
