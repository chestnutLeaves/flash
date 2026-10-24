//! The languages a card can be in, by ISO 639-1 code. A
//! card's speech blocks, a deck's languages and an account's own
//! language all name one of these, sometimes with a region after a
//! hyphen ("es-MX", "pt-PT").

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

const fn lang(code: &'static str, name: &'static str) -> Language {
    Language { code, name }
}

/// Every language, by code.
pub const LANGUAGES: &[Language] = &[
    lang("af", "Afrikaans"),
    lang("ar", "Arabic"),
    lang("az", "Azerbaijani"),
    lang("be", "Belarusian"),
    lang("bg", "Bulgarian"),
    lang("bn", "Bengali"),
    lang("bs", "Bosnian"),
    lang("ca", "Catalan"),
    lang("cs", "Czech"),
    lang("cy", "Welsh"),
    lang("da", "Danish"),
    lang("de", "German"),
    lang("el", "Greek"),
    lang("en", "English"),
    lang("es", "Spanish"),
    lang("et", "Estonian"),
    lang("eu", "Basque"),
    lang("fa", "Persian"),
    lang("fi", "Finnish"),
    lang("fr", "French"),
    lang("gl", "Galician"),
    lang("gu", "Gujarati"),
    lang("he", "Hebrew"),
    lang("hi", "Hindi"),
    lang("hr", "Croatian"),
    lang("hu", "Hungarian"),
    lang("id", "Indonesian"),
    lang("is", "Icelandic"),
    lang("it", "Italian"),
    lang("ja", "Japanese"),
    lang("kk", "Kazakh"),
    lang("kn", "Kannada"),
    lang("ko", "Korean"),
    lang("lt", "Lithuanian"),
    lang("lv", "Latvian"),
    lang("mk", "Macedonian"),
    lang("ml", "Malayalam"),
    lang("mr", "Marathi"),
    lang("ms", "Malay"),
    lang("nl", "Dutch"),
    lang("no", "Norwegian"),
    lang("pa", "Punjabi"),
    lang("pl", "Polish"),
    lang("pt", "Portuguese"),
    lang("ro", "Romanian"),
    lang("ru", "Russian"),
    lang("sk", "Slovak"),
    lang("sl", "Slovenian"),
    lang("sq", "Albanian"),
    lang("sr", "Serbian"),
    lang("su", "Sundanese"),
    lang("sv", "Swedish"),
    lang("sw", "Swahili"),
    lang("ta", "Tamil"),
    lang("te", "Telugu"),
    lang("th", "Thai"),
    lang("tl", "Tagalog"),
    lang("tr", "Turkish"),
    lang("uk", "Ukrainian"),
    lang("ur", "Urdu"),
    lang("uz", "Uzbek"),
    lang("vi", "Vietnamese"),
    lang("zh", "Chinese"),
];

/// A language by its code, in any case.
pub fn find(code: &str) -> Option<&'static Language> {
    let code = code.trim();
    LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code))
}

/// A language tag split into its code and region: "es-MX" is ("es",
/// Some("MX")), "ja" is ("ja", None). The code is lowercased and the
/// region uppercased; nothing is checked.
pub fn split(tag: &str) -> (String, Option<String>) {
    let tag = tag.trim();
    match tag.split_once(['-', '_']) {
        Some((code, region)) if !region.is_empty() => {
            (code.to_ascii_lowercase(), Some(region.to_ascii_uppercase()))
        }
        _ => (tag.to_ascii_lowercase(), None),
    }
}

/// The code of a tag: "pt-BR" is "pt".
pub fn base(tag: &str) -> String {
    split(tag).0
}

/// A tag as it is kept, when its code is a language the table knows:
/// "es-mx" is "es-MX", "JA" is "ja". A region must be two letters or
/// the three digits of a UN area ("419" for Latin America).
pub fn normalize_tag(tag: &str) -> Option<String> {
    let (code, region) = split(tag);
    let known = find(&code)?;
    match region {
        None => Some(known.code.to_string()),
        Some(r) => {
            let ok = (r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
                || (r.len() == 3 && r.bytes().all(|b| b.is_ascii_digit()));
            ok.then(|| format!("{}-{r}", known.code))
        }
    }
}

/// The English name of a code; the code itself for one the table does
/// not know.
pub fn name(code: &str) -> String {
    find(&base(code))
        .map(|l| l.name.to_string())
        .unwrap_or_else(|| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_unique() {
        let codes: Vec<&str> = LANGUAGES.iter().map(|l| l.code).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, codes);
        assert_eq!(LANGUAGES.len(), 63);
        assert!(codes.iter().all(|c| c.len() == 2));
    }

    #[test]
    fn tags_split_and_normalize() {
        assert_eq!(split("es-MX"), ("es".into(), Some("MX".into())));
        assert_eq!(split("pt_br"), ("pt".into(), Some("BR".into())));
        assert_eq!(split(" JA "), ("ja".into(), None));
        assert_eq!(base("en-GB"), "en");
        assert_eq!(normalize_tag("es-mx").as_deref(), Some("es-MX"));
        assert_eq!(normalize_tag("es-419").as_deref(), Some("es-419"));
        assert_eq!(normalize_tag("JA").as_deref(), Some("ja"));
        assert_eq!(normalize_tag("xx"), None);
        assert_eq!(normalize_tag("es-Mexico"), None);
        assert_eq!(name("pt-PT"), "Portuguese");
        assert_eq!(name("xx"), "xx");
    }
}
