//! Text as card search compares it: case folded in every alphabet, and
//! every accent and mark dropped, so "corazon" finds "Corazón", "nandu"
//! finds "Ñandú" and "привет" finds "Привет". Spanish's ñ is folded to
//! n with the rest: one rule for every language, no language needed.
//!
//! Registered on every connection as the SQL function `flash_fold` (see
//! `pragmas::apply`), which a query applies to the column and the term
//! alike. Never stored in the schema (no index, no generated column), so
//! the database opens anywhere without it.

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

pub const SQL_NAME: &str = "flash_fold";

/// `text` folded for comparison. It runs on every card a search reads,
/// so ASCII, most of most cards, takes the short way; any other letter
/// is decomposed on its own (dropping marks, the result is NFD's).
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
        } else {
            decompose_canonical(c, |d| {
                if !is_combining_mark(d) {
                    push_folded(d, &mut out);
                }
            });
        }
    }
    out
}

fn push_folded(c: char, out: &mut String) {
    match c {
        // Letters with no decomposition of their own.
        'ß' | 'ẞ' => out.push_str("ss"),
        'Æ' | 'æ' => out.push_str("ae"),
        'Œ' | 'œ' => out.push_str("oe"),
        'Ø' | 'ø' => out.push('o'),
        'Ł' | 'ł' => out.push('l'),
        'Đ' | 'đ' => out.push('d'),
        'Ð' | 'ð' => out.push('d'),
        'Þ' | 'þ' => out.push_str("th"),
        'ı' => out.push('i'),
        // Greek's final sigma reads as sigma.
        'ς' => out.push('σ'),
        _ if c.is_ascii() => out.push(c.to_ascii_lowercase()),
        _ => out.extend(c.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_fold_to_their_base() {
        assert_eq!(fold("Corazón"), "corazon");
        assert_eq!(fold("Ñandú"), "nandu");
        assert_eq!(fold("ÉCOLE"), "ecole");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("ΣΟΦΊΑ"), "σοφια");
        assert_eq!(fold("σοφίας"), "σοφιασ");
        assert_eq!(fold("Привет"), "привет");
        assert_eq!(fold("İstanbul"), "istanbul");
        // NFD content and NFC terms meet.
        assert_eq!(fold("cafe\u{301}"), fold("café"));
        assert_eq!(fold("猫といぬ"), "猫といぬ");
        assert_eq!(fold("كتاب"), "كتاب");
    }
}
