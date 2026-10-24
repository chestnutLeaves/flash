//! The pseudo-languages' words (`flash_core::locale::Pseudo`): English
//! turned, so that what the machinery misses shows on the page. A word
//! no message holds stays plain English among accented ones; a label
//! that only fits English is cut once it is 40% longer; a line that
//! does not turn for a right-to-left reader reads backwards.
//!
//! A whole formatted message is turned, not its pieces: a message's
//! markup can span the pieces Fluent formats one by one (`<a href="` |
//! a variable | `">`). Tags, entities and every variable's value pass
//! untouched: a variable is set between `OPEN` and `CLOSE` while the
//! message is formatted, and the marks are taken out here. The same
//! turn is written for the pages' scripts (static/i18n.js) and by a
//! downstream extension; shared vectors hold every copy to the same
//! output (tests/pseudo_vectors.json).

use flash_core::locale::Pseudo;

/// Set before a variable's value while a pseudo-language's message is
/// formatted (Unicode private use: no text holds it).
pub const OPEN: char = '\u{E000}';
/// Set after it.
pub const CLOSE: char = '\u{E001}';

/// The accented letters, A to Z then a to z: each a Latin letter with a
/// mark, so the text stays readable and every letter is visibly turned.
const UPPER: [char; 26] = [
    'Ȧ', 'Ɓ', 'Ƈ', 'Ḓ', 'Ḗ', 'Ƒ', 'Ɠ', 'Ħ', 'Ī', 'Ĵ', 'Ķ', 'Ŀ', 'Ḿ', 'Ƞ', 'Ǿ', 'Ƥ', 'Ɋ', 'Ř', 'Ş',
    'Ŧ', 'Ŭ', 'Ṽ', 'Ẇ', 'Ẋ', 'Ẏ', 'Ẑ',
];
const LOWER: [char; 26] = [
    'ȧ', 'ƀ', 'ƈ', 'ḓ', 'ḗ', 'ƒ', 'ɠ', 'ħ', 'ī', 'ĵ', 'ķ', 'ŀ', 'ḿ', 'ƞ', 'ǿ', 'ƥ', 'ɋ', 'ř', 'ş',
    'ŧ', 'ŭ', 'ṽ', 'ẇ', 'ẋ', 'ẏ', 'ẑ',
];

/// Right-to-left override and its end, and the right-to-left mark: what
/// Android's own `ar-XB` sets around each word.
const RLO: char = '\u{202E}';
const PDF: char = '\u{202C}';
const RLM: char = '\u{200F}';

/// `text`, a formatted message whose variables sit between `OPEN` and
/// `CLOSE`, as the pseudo-language `how` writes it, with the marks
/// taken out.
pub fn turn(text: &str, how: Pseudo) -> String {
    let pieces = pieces(text);
    let mut out = String::with_capacity(text.len() * 2);
    if how == Pseudo::Accented {
        out.push('[');
    }
    for (kind, piece) in pieces {
        match kind {
            Piece::Words => match how {
                Pseudo::Accented => accent(piece, &mut out),
                Pseudo::Bidi => mirror(piece, &mut out),
            },
            Piece::Kept => out.extend(piece.chars().filter(|c| *c != OPEN && *c != CLOSE)),
        }
    }
    if how == Pseudo::Accented {
        out.push(']');
    }
    out
}

/// What a stretch of a message is: words to turn, or what passes as it
/// is (a tag, an entity, a variable's value).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Piece {
    Words,
    Kept,
}

/// `text` cut into its words and what passes untouched, the variables'
/// marks dropped.
fn pieces(text: &str) -> Vec<(Piece, &str)> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = rest.find(['<', '&', OPEN]).unwrap_or(rest.len());
        if next > 0 {
            out.push((Piece::Words, &rest[..next]));
            rest = &rest[next..];
            continue;
        }
        let (kept, after) = if let Some(value) = rest.strip_prefix(OPEN) {
            let end = value.find(CLOSE).unwrap_or(value.len());
            let after = value[end..].strip_prefix(CLOSE).unwrap_or(&value[end..]);
            (&value[..end], after)
        } else if rest.starts_with('<') {
            let end = rest.find('>').map_or(rest.len(), |e| e + 1);
            (&rest[..end], &rest[end..])
        } else {
            match rest[1..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '#')) {
                Some(e) if rest[1 + e..].starts_with(';') && e > 0 => {
                    (&rest[..e + 2], &rest[e + 2..])
                }
                _ => {
                    out.push((Piece::Words, &rest[..1]));
                    rest = &rest[1..];
                    continue;
                }
            }
        };
        out.push((Piece::Kept, kept));
        rest = after;
    }
    out
}

/// Every ASCII letter accented, every vowel twice: about 40% longer.
fn accent(words: &str, out: &mut String) {
    for c in words.chars() {
        let turned = match c {
            'A'..='Z' => UPPER[(c as u8 - b'A') as usize],
            'a'..='z' => LOWER[(c as u8 - b'a') as usize],
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push(turned);
        if "aeiouAEIOU".contains(c) {
            out.push(turned);
        }
    }
}

/// Each word set right to left, as Android's `ar-XB` sets it; a run of
/// digits or signs stays as it reads.
fn mirror(words: &str, out: &mut String) {
    fn flush(word: &mut String, out: &mut String) {
        if word.chars().any(char::is_alphabetic) {
            out.push(RLM);
            out.push(RLO);
            out.push_str(word);
            out.push(PDF);
            out.push(RLM);
        } else {
            out.push_str(word);
        }
        word.clear();
    }
    let mut word = String::new();
    for c in words.chars() {
        if c.is_whitespace() {
            flush(&mut word, out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(v: &str) -> String {
        format!("{OPEN}{v}{CLOSE}")
    }

    #[test]
    fn accented_turns_words_and_keeps_markup_entities_and_variables() {
        let text = format!("Hi {}, see <a href=\"/a\">it</a> &amp; go", var("Ana"));
        assert_eq!(
            turn(&text, Pseudo::Accented),
            "[Ħīī Ana, şḗḗḗḗ <a href=\"/a\">īīŧ</a> &amp; ɠǿǿ]"
        );
    }

    #[test]
    fn bidi_sets_each_word_right_to_left() {
        let text = format!("Hi {}", var("Ana"));
        assert_eq!(
            turn(&text, Pseudo::Bidi),
            "\u{200F}\u{202E}Hi\u{202C}\u{200F} Ana"
        );
    }

    #[test]
    fn a_lone_ampersand_is_a_word() {
        assert_eq!(turn("A & B", Pseudo::Accented), "[ȦȦ & Ɓ]");
    }
}
