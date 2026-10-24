//! Text in one form. Unicode can write "é" as one character or as "e"
//! and an accent: a phone's keyboard, a PDF and a web page each pick
//! their own, and the two never compare equal. Everything a person
//! writes into Flash is kept composed (NFC), the form most text already
//! takes, so a duplicate is a duplicate and a typed answer matches.

use unicode_normalization::{is_nfc_quick, IsNormalized, UnicodeNormalization};

/// `text` composed (NFC); unchanged text is not copied twice.
pub fn nfc(text: &str) -> String {
    match is_nfc_quick(text.chars()) {
        IsNormalized::Yes => text.to_string(),
        _ => text.nfc().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposed_letters_compose() {
        assert_eq!(nfc("cafe\u{301}"), "café");
        assert_eq!(nfc("n\u{303}andu\u{301}"), "ñandú");
        assert_eq!(nfc("café"), "café");
        // Hangul jamo compose into syllables.
        assert_eq!(nfc("\u{1100}\u{1161}"), "가");
        assert_eq!(nfc(""), "");
    }
}
