//! Keeping a variable's direction to itself. A person's own words set
//! into a sentence (a deck named in Hebrew in an English line, an English
//! name in an Arabic one) can pull the punctuation and the words around
//! them to the wrong side: the Unicode bidi algorithm reads the sentence
//! as one run. A value set between FIRST STRONG ISOLATE and POP
//! DIRECTIONAL ISOLATE is laid out on its own, its direction taken from
//! its first strong letter, and the sentence around it keeps its order
//! (Unicode Standard Annex #9, isolates; W3C, "Inline markup and bidirectional
//! text in HTML").
//!
//! A value is isolated where it can go wrong: for a reader whose
//! language runs right to left, every text value, and for any other
//! reader, a value holding right-to-left letters. An English sentence
//! with an English value is left byte for byte as it was. Numbers are
//! never isolated: a number's digits keep their order in either
//! direction.
//!
//! The same rule is written for the pages' scripts (static/i18n.js) and
//! by a downstream extension; shared vectors hold every copy to the same
//! output (tests/bidi_vectors.json).

/// FIRST STRONG ISOLATE: what follows, to its `PDI`, is laid out on its
/// own, in the direction of its first strong letter.
pub const FSI: char = '\u{2068}';
/// POP DIRECTIONAL ISOLATE: the end of an isolate.
pub const PDI: char = '\u{2069}';

/// Set before a variable's value in a message holding markup of our own
/// while it is formatted (Unicode private use: no text holds it), so a
/// value inside a tag (`href="{ $href }"`) is told from one in the
/// sentence: only the second is isolated (`settle`).
const OPEN: char = '\u{E002}';
/// Set after it.
const CLOSE: char = '\u{E003}';

/// Whether `text` holds a right-to-left letter: one in the blocks
/// Unicode sets aside for right-to-left scripts (Hebrew, Arabic, Syriac,
/// Thaana, N'Ko and the rest, their presentation forms, the historic
/// scripts of the Supplementary Multilingual Plane), whose letters are
/// all of bidi class R or AL (DerivedBidiClass.txt), or the
/// right-to-left mark.
pub fn right_to_left(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '\u{0590}'..='\u{08FF}'
                | '\u{200F}'
                | '\u{FB1D}'..='\u{FDFF}'
                | '\u{FE70}'..='\u{FEFF}'
                | '\u{10800}'..='\u{10FFF}'
                | '\u{1E800}'..='\u{1EFFF}'
        )
    })
}

/// Whether a reader of a right-to-left language (`rtl`) needs `value`
/// isolated where it sits in a sentence. An empty value has nothing to
/// isolate.
pub fn needs_isolating(value: &str, rtl: bool) -> bool {
    !value.is_empty() && (rtl || right_to_left(value))
}

/// `value` as a plain-text message sets it for a reader of a
/// right-to-left language (`rtl`) or not: isolated when it can go
/// wrong, else as it is.
pub fn isolate(value: &str, rtl: bool) -> String {
    if needs_isolating(value, rtl) {
        format!("{FSI}{value}{PDI}")
    } else {
        value.to_string()
    }
}

/// `value` marked for `settle`, in a message holding markup.
pub fn mark(value: &str) -> String {
    format!("{OPEN}{value}{CLOSE}")
}

/// A formatted message holding markup, its marked values isolated where
/// they sit in the sentence and left bare inside a tag (an `href`, a
/// `datetime`), where an isolate would be part of the address.
pub fn settle(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(c);
            }
            '>' => {
                in_tag = false;
                out.push(c);
            }
            OPEN if !in_tag => out.push(FSI),
            CLOSE if !in_tag => out.push(PDI),
            OPEN | CLOSE => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_in_a_tag_stays_bare_and_one_in_the_sentence_is_isolated() {
        let text = format!(
            "<a href=\"{}\">{}</a> and {}",
            mark("/es/x"),
            mark("שלום"),
            mark("Ana")
        );
        assert_eq!(
            settle(&text),
            format!("<a href=\"/es/x\">{FSI}שלום{PDI}</a> and {FSI}Ana{PDI}")
        );
    }
}
