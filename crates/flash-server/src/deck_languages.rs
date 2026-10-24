//! The languages a deck's cards are in, as the deck stores them:
//! a short list of two-letter codes, set by an assistant that names a
//! deck's languages or by a downstream extension, and read wherever a
//! card's language matters before its own blocks say.

/// How many languages one deck may hint.
pub const MAX_LANGUAGE_HINTS: usize = 8;

/// The language codes a deck may name (ISO 639-1).
pub const LANGUAGE_CODES: &[&str] = &[
    "af", "ar", "az", "be", "bg", "bn", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es", "et",
    "eu", "fa", "fi", "fr", "gl", "gu", "he", "hi", "hr", "hu", "id", "it", "ja", "kk", "kn", "ko",
    "lt", "lv", "mk", "ml", "mr", "ms", "nl", "no", "pa", "pl", "pt", "ro", "ru", "sk", "sl", "sq",
    "sr", "sv", "sw", "ta", "te", "th", "tl", "tr", "uk", "ur", "vi", "zh",
];

/// Why a deck's hints were refused, as the sentence a person reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HintsError {
    #[error("unknown language code {0:?}")]
    Unknown(String),
    #[error("a deck may hint at most {MAX_LANGUAGE_HINTS} languages")]
    TooMany,
}

/// A deck's hints as they are stored: each code known, lowercased,
/// duplicates dropped keeping the first, at most `MAX_LANGUAGE_HINTS`.
/// An empty list leaves the language to the cards.
pub fn normalize_hints<S: AsRef<str>>(hints: &[S]) -> Result<Vec<String>, HintsError> {
    let mut out: Vec<String> = Vec::new();
    for raw in hints {
        let code = raw.as_ref().trim().to_ascii_lowercase();
        if code.is_empty() {
            continue;
        }
        if !LANGUAGE_CODES.contains(&code.as_str()) {
            return Err(HintsError::Unknown(raw.as_ref().trim().to_string()));
        }
        if !out.contains(&code) {
            out.push(code);
        }
    }
    if out.len() > MAX_LANGUAGE_HINTS {
        return Err(HintsError::TooMany);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_known_codes_deduplicated_in_order() {
        assert_eq!(normalize_hints::<&str>(&[]).unwrap(), Vec::<String>::new());
        assert_eq!(
            normalize_hints(&["FR", " en", "fr", ""]).unwrap(),
            vec!["fr".to_string(), "en".to_string()]
        );
        assert_eq!(
            normalize_hints(&["en", "klingon"]).unwrap_err(),
            HintsError::Unknown("klingon".into())
        );
        let many: Vec<&str> = LANGUAGE_CODES
            .iter()
            .take(MAX_LANGUAGE_HINTS + 1)
            .copied()
            .collect();
        assert_eq!(normalize_hints(&many).unwrap_err(), HintsError::TooMany);
    }

    #[test]
    fn the_code_list_is_sorted_unique_two_letter_lowercase() {
        let mut sorted = LANGUAGE_CODES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, LANGUAGE_CODES);
        assert!(LANGUAGE_CODES
            .iter()
            .all(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_lowercase())));
        assert!(LANGUAGE_CODES
            .iter()
            .all(|c| crate::languages::find(c).is_some()));
    }
}
