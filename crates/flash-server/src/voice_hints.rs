//! What a speech recognizer is primed with while a card's front is
//! shown: the words of the answer the student is about to say.

/// The most strings one card primes the recognizer with.
pub const MAX_HINTS: usize = 20;
/// The longest hint, in characters.
pub const MAX_HINT_CHARS: usize = 40;

/// Strings worth priming a recognizer with while the front is shown: the
/// spoken back whole when it is short, its words, and each symbol-like
/// word spelled out ("Au" becomes "A U"), since a chemical symbol or an
/// abbreviation is answered letter by letter. They reach the device
/// before the reveal on purpose: a recognizer cannot lean toward words it
/// has not been given, and the hidden back's images travel ahead the
/// same way.
pub fn answer_hints(spoken_back: &str) -> Vec<String> {
    let whole = spoken_back
        .trim()
        .trim_end_matches(['.', ',', ';', ':', '!', '?'])
        .trim();
    let tokens: Vec<&str> = spoken_back
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if s.is_empty() || s.chars().count() > MAX_HINT_CHARS {
            return;
        }
        let lower = s.to_lowercase();
        if out.iter().any(|h| h.to_lowercase() == lower) {
            return;
        }
        out.push(s);
    };
    push(whole.to_string());
    for t in &tokens {
        push(t.to_string());
    }
    for s in spelled_forms(spoken_back) {
        push(s);
    }
    out.truncate(MAX_HINTS);
    out
}

/// The answer's symbol-like words letter by letter ("A U" for "Au."),
/// in order, without repeats: what the grader is shown beside the card
/// so a transcript that sounds like the spelling is credited.
pub fn spelled_forms(spoken_back: &str) -> Vec<String> {
    let tokens: Vec<&str> = spoken_back
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect();
    let last = tokens.len().saturating_sub(1);
    let mut out: Vec<String> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if let Some(s) = spelled(t, i == last) {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// The token letter by letter when it reads like a symbol: every letter
/// uppercase ("DNA", "H2O"), an uppercase letter after the first ("NaCl",
/// "pH"), or two characters capitalised at the end of the answer ("Au",
/// "Fe"; not the "It" that opens a sentence).
fn spelled(token: &str, last: bool) -> Option<String> {
    let n = token.len();
    if !(2..=6).contains(&n) || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let letters: Vec<u8> = token.bytes().filter(u8::is_ascii_alphabetic).collect();
    if letters.is_empty() {
        return None;
    }
    let all_upper = letters.iter().all(u8::is_ascii_uppercase);
    let inner_upper = token.bytes().skip(1).any(|b| b.is_ascii_uppercase());
    let short_symbol = n == 2 && last && token.as_bytes()[0].is_ascii_uppercase();
    if !(all_upper || inner_upper || short_symbol) {
        return None;
    }
    let spelled: Vec<String> = token
        .chars()
        .map(|c| c.to_ascii_uppercase().to_string())
        .collect();
    Some(spelled.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(back: &str) -> Vec<String> {
        answer_hints(back)
    }

    #[test]
    fn a_symbol_is_itself_and_spelled_out() {
        assert_eq!(hints("Au."), ["Au", "A U"]);
        assert_eq!(hints("NaCl"), ["NaCl", "N A C L"]);
        assert_eq!(hints("DNA"), ["DNA", "D N A"]);
        assert_eq!(hints("H2O"), ["H2O", "H 2 O"]);
        assert_eq!(hints("pH"), ["pH", "P H"]);
        assert_eq!(hints("gold (Au)"), ["gold (Au)", "gold", "Au", "A U"]);
    }

    #[test]
    fn a_phrase_is_whole_then_its_words() {
        assert_eq!(hints("Vitamin K"), ["Vitamin K", "Vitamin", "K"]);
        assert_eq!(
            hints("Paris is the capital of France."),
            [
                "Paris is the capital of France",
                "Paris",
                "is",
                "the",
                "capital",
                "of",
                "France"
            ]
        );
    }

    #[test]
    fn only_the_closing_two_letter_word_is_spelled() {
        assert_eq!(hints("It was Au"), ["It was Au", "It", "was", "Au", "A U"]);
        assert_eq!(hints("on"), ["on"]);
    }

    #[test]
    fn the_spelled_forms_alone_are_what_the_grader_sees() {
        assert_eq!(spelled_forms("Au."), ["A U"]);
        assert_eq!(spelled_forms("NaCl and KCl"), ["N A C L", "K C L"]);
        assert_eq!(spelled_forms("DNA DNA"), ["D N A"]);
        assert!(spelled_forms("Paris is the capital of France").is_empty());
        assert!(spelled_forms("").is_empty());
    }

    #[test]
    fn a_long_back_keeps_its_words_but_not_the_whole() {
        let back = "The mitochondrion is the powerhouse of the cell, making ATP";
        let h = hints(back);
        assert!(!h.contains(&back.to_string()), "{h:?}");
        assert_eq!(h[0], "The");
        assert!(h.contains(&"A T P".to_string()), "{h:?}");
    }

    #[test]
    fn hints_are_deduped_capped_and_never_empty_strings() {
        assert_eq!(hints("DNA DNA"), ["DNA DNA", "DNA", "D N A"]);
        assert_eq!(hints("dna DNA"), ["dna DNA", "dna", "D N A"]);
        let many: Vec<String> = (0..30).map(|i| format!("w{i}")).collect();
        assert_eq!(hints(&many.join(" ")).len(), MAX_HINTS);
        assert!(hints("").is_empty());
        assert!(hints(" ... ").is_empty());
        assert_eq!(hints("6"), ["6"]);
    }
}
