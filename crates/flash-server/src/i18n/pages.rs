use flash_core::locale::UiLocale;

use super::Catalog;

/// A public page with an address in each language it is written in:
/// `/guide` in English, and its slug under each language's prefix
/// (`/es/guia` as Latin America writes Spanish, `/es-es/guia` as
/// Spain does), filed by message folder: a slug filed under a base
/// folder ("es") serves every language that reads it, one under a
/// language's own folder ("es-ES") that language alone, and a reader
/// takes the slug of the nearest folder of their chain. A page with no
/// slugs is written in English only; one with no English address in its
/// slugs' languages only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicPage {
    /// Its English address, when it has one.
    pub en: Option<&'static str>,
    /// Its slug in each folder it is written for ("/guia"; "/" for the
    /// front page), under the reader's language's prefix.
    pub slugs: &'static [(&'static str, &'static str)],
}

impl PublicPage {
    /// A page at `en` in English, and at each of `slugs` in their
    /// folders' languages.
    pub const fn new(
        en: Option<&'static str>,
        slugs: &'static [(&'static str, &'static str)],
    ) -> PublicPage {
        PublicPage { en, slugs }
    }

    /// A page written in English only: English to every reader.
    pub const fn english_only(en: &'static str) -> PublicPage {
        PublicPage {
            en: Some(en),
            slugs: &[],
        }
    }

    /// Its slug for `locale`: the slug of the nearest folder of its
    /// chain, its own first; None when it is not written in it.
    pub fn slug_in(&self, locale: UiLocale) -> Option<&'static str> {
        locale.chain().iter().rev().find_map(|folder| {
            self.slugs
                .iter()
                .find(|(f, _)| f == folder)
                .map(|(_, slug)| *slug)
        })
    }

    /// Its address in `locale`, or None when it is not written in it:
    /// `/es/guia`, `/es` for Latin America's front page.
    pub fn path_in(&self, locale: UiLocale) -> Option<String> {
        if locale == UiLocale::EN {
            return self.en.map(str::to_string);
        }
        if locale.pseudo().is_some() {
            return None;
        }
        let slug = self.slug_in(locale)?;
        let prefix = locale.prefix();
        Some(if slug == "/" {
            prefix.to_string()
        } else {
            format!("{prefix}{slug}")
        })
    }
}

impl Catalog {
    /// The public page at the English address `path`, when it is one.
    pub fn page_at(&self, path: &str) -> Option<PublicPage> {
        self.localized.iter().copied().find(|p| p.en == Some(path))
    }

    /// The public page at `rest` under `locale`'s prefix (`/guia`
    /// under `/es`), when one is written in it.
    pub fn page_in(&self, locale: UiLocale, rest: &str) -> Option<PublicPage> {
        self.localized
            .iter()
            .copied()
            .find(|p| p.slug_in(locale) == Some(rest))
    }

    /// The public page a template names by `path`: its English address,
    /// or, for a page with none, one of its slugs.
    pub(super) fn page_named(&self, path: &str) -> Option<PublicPage> {
        self.page_at(path).or_else(|| {
            self.localized
                .iter()
                .copied()
                .find(|p| p.en.is_none() && p.slugs.iter().any(|(_, slug)| *slug == path))
        })
    }
}

/// The language a path's prefix names and the path without it:
/// `/es/guide` is Latin America's `/guide`, `/es-es` Spain's `/`. The
/// longest prefix that fits decides, so `/es-es` is never read as `/es`.
pub fn split_path(path: &str) -> Option<(UiLocale, String)> {
    let mut prefixed: Vec<UiLocale> = UiLocale::real()
        .filter(|l| !l.prefix().is_empty())
        .collect();
    prefixed.sort_by_key(|l| std::cmp::Reverse(l.prefix().len()));
    for locale in prefixed {
        if let Some(rest) = path.strip_prefix(locale.prefix()) {
            if rest.is_empty() {
                return Some((locale, "/".to_string()));
            }
            if rest.starts_with('/') {
                return Some((locale, rest.to_string()));
            }
        }
    }
    None
}
