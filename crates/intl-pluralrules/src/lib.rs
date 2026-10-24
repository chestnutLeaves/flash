//! The API of crates.io's `intl_pluralrules` (7.x), through which Fluent
//! picks a message's plural variant, answered from ICU4X's compiled CLDR
//! data (`icu_plurals`, CLDR 48) instead of rules generated from CLDR 36
//! (2019). The root manifest patches it in for the crates.io crate, so a
//! page the server words picks the same variant a browser or a phone
//! picks for the same message and number: since CLDR 42, Spanish,
//! French, Italian and Portuguese have a `many` form ("1.000.000 de
//! tarjetas") that the old rules never chose.

pub mod operands;

use unic_langid::LanguageIdentifier;

use crate::operands::{PluralOperands, NOT_A_NUMBER};

/// The CLDR release ICU4X's data comes from (icu_plurals_data 2.3).
pub static CLDR_VERSION: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    ZERO,
    ONE,
    TWO,
    FEW,
    MANY,
    OTHER,
}

/// Cardinal ("3 cards") or ordinal ("3rd card") rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluralRuleType {
    ORDINAL,
    CARDINAL,
}

/// Every language CLDR 48 writes plural rules for (its
/// plurals-type-cardinal, root left out), which Fluent negotiates a
/// bundle's language against: a language missing here would read
/// English's rules. Each is answered from ICU4X's data.
const LANGUAGES: &[&str] = &[
    "af", "ak", "am", "an", "ar", "ars", "as", "asa", "ast", "az", "bal", "be", "bem", "bez", "bg",
    "bho", "blo", "bm", "bn", "bo", "br", "brx", "bs", "ca", "ce", "ceb", "cgg", "chr", "ckb",
    "cs", "csw", "cv", "cy", "da", "de", "doi", "dsb", "dv", "dz", "ee", "el", "en", "eo", "es",
    "et", "eu", "fa", "ff", "fi", "fil", "fo", "fr", "fur", "fy", "ga", "gd", "gl", "gsw", "gu",
    "guw", "gv", "ha", "haw", "he", "hi", "hnj", "hr", "hsb", "hu", "hy", "ia", "id", "ie", "ig",
    "ii", "io", "is", "it", "iu", "ja", "jbo", "jgo", "jmc", "jv", "jw", "ka", "kab", "kaj", "kcg",
    "kde", "kea", "kk", "kkj", "kl", "km", "kn", "ko", "kok", "kok-Latn", "ks", "ksb", "ksh", "ku",
    "kw", "ky", "lag", "lb", "lg", "lij", "lkt", "lld", "ln", "lo", "lt", "lv", "mas", "mg", "mgo",
    "mk", "ml", "mn", "mo", "mr", "ms", "mt", "my", "nah", "naq", "nb", "nd", "ne", "nl", "nn",
    "nnh", "no", "nqo", "nr", "nso", "ny", "nyn", "om", "or", "os", "osa", "pa", "pap", "pcm",
    "pl", "prg", "ps", "pt", "pt-PT", "rm", "ro", "rof", "ru", "rwk", "sah", "saq", "sat", "sc",
    "scn", "sd", "sdh", "se", "seh", "ses", "sg", "sgs", "sh", "shi", "si", "sk", "sl", "sma",
    "smi", "smj", "smn", "sms", "sn", "so", "sq", "sr", "ss", "ssy", "st", "su", "sv", "sw", "syr",
    "ta", "te", "teo", "th", "ti", "tig", "tk", "tl", "tn", "to", "tpi", "tr", "ts", "tzm", "ug",
    "uk", "ur", "uz", "ve", "vec", "vi", "vo", "vun", "wa", "wae", "wo", "xh", "xog", "yi", "yo",
    "yue", "zh", "zu",
];

/// A language's plural rules, of one type.
pub struct PluralRules {
    locale: LanguageIdentifier,
    rules: icu_plurals::PluralRules,
}

impl PluralRules {
    /// The rules of `langid`, one of `get_locales`.
    pub fn create<L: Into<LanguageIdentifier>>(
        langid: L,
        prt: PluralRuleType,
    ) -> Result<Self, &'static str> {
        let locale = langid.into();
        let tag = locale.to_string();
        if !LANGUAGES.contains(&tag.as_str()) {
            return Err("unknown locale");
        }
        let icu = icu_locale_core::Locale::try_from_str(&tag).map_err(|_| "unknown locale")?;
        let rules = match prt {
            PluralRuleType::CARDINAL => icu_plurals::PluralRules::try_new_cardinal((&icu).into()),
            PluralRuleType::ORDINAL => icu_plurals::PluralRules::try_new_ordinal((&icu).into()),
        }
        .map_err(|_| "no plural rules for this locale")?;
        Ok(Self { locale, rules })
    }

    /// The category of `number`.
    pub fn select<N: TryInto<PluralOperands>>(
        &self,
        number: N,
    ) -> Result<PluralCategory, &'static str> {
        let operands = number.try_into().map_err(|_| NOT_A_NUMBER)?;
        let decimal =
            fixed_decimal::Decimal::try_from_str(&operands.digits()).map_err(|_| NOT_A_NUMBER)?;
        Ok(match self.rules.category_for(&decimal) {
            icu_plurals::PluralCategory::Zero => PluralCategory::ZERO,
            icu_plurals::PluralCategory::One => PluralCategory::ONE,
            icu_plurals::PluralCategory::Two => PluralCategory::TWO,
            icu_plurals::PluralCategory::Few => PluralCategory::FEW,
            icu_plurals::PluralCategory::Many => PluralCategory::MANY,
            icu_plurals::PluralCategory::Other => PluralCategory::OTHER,
        })
    }

    pub fn get_locales(_prt: PluralRuleType) -> Vec<LanguageIdentifier> {
        LANGUAGES
            .iter()
            .map(|tag| tag.parse().expect("every listed language parses"))
            .collect()
    }

    pub fn get_locale(&self) -> &LanguageIdentifier {
        &self.locale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(tag: &str, prt: PluralRuleType) -> PluralRules {
        PluralRules::create(tag.parse::<LanguageIdentifier>().unwrap(), prt).unwrap()
    }

    #[test]
    fn current_cldr_answers() {
        let es = rules("es", PluralRuleType::CARDINAL);
        assert_eq!(es.select(1), Ok(PluralCategory::ONE));
        assert_eq!(es.select(2), Ok(PluralCategory::OTHER));
        // CLDR 42 and later; CLDR 36 said OTHER.
        assert_eq!(es.select(1_000_000), Ok(PluralCategory::MANY));
        let pl = rules("pl", PluralRuleType::CARDINAL);
        assert_eq!(pl.select("3"), Ok(PluralCategory::FEW));
        assert_eq!(pl.select(12), Ok(PluralCategory::MANY));
        assert_eq!(pl.select("5.0"), Ok(PluralCategory::OTHER));
        assert_eq!(
            rules("ar", PluralRuleType::CARDINAL).select(0),
            Ok(PluralCategory::ZERO)
        );
        assert_eq!(
            rules("ja", PluralRuleType::CARDINAL).select(1),
            Ok(PluralCategory::OTHER)
        );
        // Portugal's Portuguese is not Brazil's.
        assert_eq!(
            rules("pt", PluralRuleType::CARDINAL).select(0),
            Ok(PluralCategory::ONE)
        );
        assert_eq!(
            rules("pt-PT", PluralRuleType::CARDINAL).select(0),
            Ok(PluralCategory::OTHER)
        );
        // "1.0" shows a fraction digit, so it is not English's one.
        let en = rules("en", PluralRuleType::CARDINAL);
        assert_eq!(en.select("1.0"), Ok(PluralCategory::OTHER));
        assert_eq!(en.select(-1), Ok(PluralCategory::ONE));
        assert_eq!(
            rules("en", PluralRuleType::ORDINAL).select(22),
            Ok(PluralCategory::TWO)
        );
    }

    #[test]
    fn only_listed_languages_are_made_and_every_listed_one_is() {
        let unknown = "xx".parse::<LanguageIdentifier>().unwrap();
        assert!(PluralRules::create(unknown, PluralRuleType::CARDINAL).is_err());
        for tag in PluralRules::get_locales(PluralRuleType::CARDINAL) {
            let made = PluralRules::create(tag.clone(), PluralRuleType::CARDINAL).unwrap();
            assert_eq!(made.get_locale(), &tag);
            assert!(made.select(1).is_ok());
        }
        assert!(rules("en", PluralRuleType::CARDINAL).select("one").is_err());
    }

    #[test]
    fn operands_keep_their_visible_digits() {
        let o = PluralOperands::try_from("-1234.50").unwrap();
        assert_eq!((o.i, o.v, o.w, o.f, o.t), (1234, 2, 1, 50, 5));
        assert_eq!(o.digits(), "1234.50");
        assert_eq!(PluralOperands::try_from("0.05").unwrap().digits(), "0.05");
        assert_eq!(PluralOperands::from(7u8).digits(), "7");
        assert_eq!(PluralOperands::try_from(1.5f64).unwrap().digits(), "1.5");
        assert!(PluralOperands::try_from("1e3").is_err());
        assert!(PluralOperands::try_from(".5").is_err());
        assert!(PluralOperands::try_from(f64::NAN).is_err());
        assert!(PluralOperands::try_from(i64::MIN).is_err());
    }
}
