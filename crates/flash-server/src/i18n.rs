//! Every word Flash says to a person, in the languages it speaks: one
//! catalog of Project Fluent messages per language (`locales/<tag>/`,
//! plus whatever a deployment adds), and `Tr`, the catalog as one reader
//! reads it.
//!
//! Spanish is one base (`es`) both Spanish variants share, with the few
//! messages Spain and Latin America word differently in `es-ES` and
//! `es-419`; each variant falls back to the base, and anything still
//! missing falls back to English. Simplified Chinese is shipped in `zh-CN`.
//! A message missing in English is a bug
//! the tests catch (`missing`).
//!
//! Pages read their reader's `Tr` from the request (`context` sets it);
//! an email or a job builds one from the account's language and works
//! inside it (`scoped`). A template asks for words through `filters` with the `Tr`
//! passed as askama's runtime value `"tr"` (`render_current`).

pub mod bidi;
/// The template filters that ask the catalog for words. A module whose
/// templates use them brings them in with `use crate::i18n::filters;`.
///
/// - `{{ "decks-title"|t }}`: a message.
/// - `{{ "cards-due"|tn(count) }}`: a message whose `$count` picks its
///   plural.
/// - `{{ "deck-deleted"|ta("name", deck.name) }}`: a message with one
///   variable (`ta2`: two).
/// - `{{ "left"|tna(n, "deck", name) }}`: a count and a variable.
/// - `{{ "delete"|tn2("decks", d, "cards", c) }}`: two counts.
/// - `{{ ""|html_lang }}`, `{{ ""|html_dir }}`, `{{ ""|html_script }}`:
///   the page's `lang`, `dir` and `data-script`.
/// - `{{ ""|start_side }}`: "left" or "right" by the reader's
///   direction, for a message's inline styles.
pub mod filters;
pub mod format;
mod islands;
pub mod lists;
#[cfg(feature = "test-support")]
pub mod packs;
mod pages;
pub mod picker;
pub mod pseudo;
mod request;
#[cfg(feature = "test-support")]
pub mod scripts;
mod words;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use flash_core::locale::{Dir, Numbers, Regions, UiLocale};
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use intl_memoizer::concurrent::IntlLangMemoizer;
use parking_lot::{Mutex, RwLock};

pub use fluent_bundle::FluentValue as Value;
pub(crate) use islands::island;
pub use islands::{folder_island, reading_order};
pub use lists::{list_in, ListKind};
pub use pages::{split_path, PublicPage};
pub use request::{
    blocking, context, current, enter, for_account, for_user, localize, reader, reader_values,
    render_current, scoped, within, Entered, Localized, LANGUAGE_ONLY_PAGE,
};
pub use words::Words;

/// The languages a reader may be given. A language Flash has messages
/// for but has not shipped yet answers in English until every surface
/// speaks it.
pub const SHIPPED: &[UiLocale] = &[UiLocale::EN, UiLocale::ZH_CN];

/// One file of messages: the tag of the folder it lives in (`en`, `es`,
/// `es-ES`, `es-419`) and its text.
pub type Resource = (&'static str, &'static str);

/// The open core's own messages: every file under `locales/<folder>/`,
/// listed by build.rs from the filesystem (one file per surface in
/// English and in each base language, a variant's few rewordings beside
/// its base's).
/// A folder no language of the table reads fails the build.
pub const CORE: &[Resource] = include!(concat!(env!("OUT_DIR"), "/catalog_files.rs"));

type Bundle = FluentBundle<Arc<FluentResource>>;

/// Every language's messages, parsed once at start, and a bundle of them
/// for each region that reads them (the region's numbers differ: "12.345"
/// in Spain, "12,345" in Mexico), made the first time a reader from it
/// comes. Fluent's own isolation is off: it would wrap every variable
/// of every message, English included, in marks; `Tr::args` isolates a
/// variable only where its direction can pull the sentence (`bidi`).
pub struct Catalog {
    /// Each folder's parsed files (`en`, `es`, `es-ES`, `es-419`, `zh-CN`).
    layers: Vec<(&'static str, Arc<FluentResource>)>,
    en: Arc<Bundle>,
    /// One bundle per (language, region tag) a reader has come with.
    bundles: RwLock<HashMap<(UiLocale, String), Arc<Bundle>>>,
    /// One set of number and date formatters per region tag.
    formats: RwLock<HashMap<String, Option<Arc<format::Formats>>>>,
    /// Each language's list joiners as a page's scripts read them.
    lists: Mutex<HashMap<UiLocale, serde_json::Value>>,
    /// The languages readers may be given (`SHIPPED`; a test may ship
    /// more).
    shipped: Vec<UiLocale>,
    /// Message ids asked for and found nowhere, English included.
    missing: Mutex<BTreeSet<String>>,
    /// The public pages with an address per language: a deployment's
    /// (`ServerExtension::public_pages`).
    localized: Vec<PublicPage>,
    /// The starts of the ids of messages only English has, for pages
    /// written in English only (`ServerExtension::english_only_messages`).
    english_only: &'static [&'static str],
}

impl Catalog {
    /// The catalog of `resources` (the core's, then a deployment's). A
    /// file that does not parse is a build mistake the tests catch; here
    /// its readable messages are kept and the rest logged.
    pub fn new(resources: &[Resource]) -> Catalog {
        let layers: Vec<(&'static str, Arc<FluentResource>)> = resources
            .iter()
            .map(|(dir, text)| {
                let resource = match FluentResource::try_new((*text).to_string()) {
                    Ok(r) => r,
                    Err((r, errors)) => {
                        tracing::error!(dir, ?errors, "a message file does not parse");
                        r
                    }
                };
                (*dir, Arc::new(resource))
            })
            .collect();
        let en = Arc::new(bundle(&layers, UiLocale::EN, UiLocale::EN.tag()));
        Catalog {
            layers,
            en,
            bundles: RwLock::new(HashMap::new()),
            formats: RwLock::new(HashMap::new()),
            lists: Mutex::new(HashMap::new()),
            shipped: SHIPPED.to_vec(),
            missing: Mutex::new(BTreeSet::new()),
            localized: Vec::new(),
            english_only: &[],
        }
    }

    /// The same catalog with `prefixes` as the starts of the ids of
    /// messages only English has (an English-only page's): a language is
    /// whole without them.
    pub fn english_only(mut self, prefixes: &'static [&'static str]) -> Catalog {
        self.english_only = prefixes;
        self
    }

    /// The same catalog with `pages` as the public pages that have an
    /// address per language.
    pub fn localizing(mut self, pages: &[PublicPage]) -> Catalog {
        self.localized = pages.to_vec();
        self
    }

    /// The same catalog giving readers `shipped` (English is always in),
    /// or why not: a language shipped has a word for every English
    /// message (`english_only` aside), so no reader of it meets English
    /// in the middle of a page. The server refuses to start otherwise.
    pub fn shipping(mut self, shipped: &[UiLocale]) -> Result<Catalog, String> {
        self.shipped = std::iter::once(UiLocale::EN)
            .chain(shipped.iter().copied().filter(|l| *l != UiLocale::EN))
            .collect();
        let unwritten: Vec<String> = self.shipped[1..]
            .iter()
            .filter_map(|&locale| {
                let ids = self.unwritten(locale);
                (!ids.is_empty()).then(|| {
                    let shown: Vec<&str> = ids.iter().take(10).map(String::as_str).collect();
                    let more = ids.len().saturating_sub(shown.len());
                    let more = if more > 0 {
                        format!(" and {more} more")
                    } else {
                        String::new()
                    };
                    format!("{} lacks {}{more}", locale.tag(), shown.join(", "))
                })
            })
            .collect();
        if unwritten.is_empty() {
            Ok(self)
        } else {
            Err(format!(
                "a language shipped is not whole: {}",
                unwritten.join("; ")
            ))
        }
    }

    /// The English messages `locale` has no word for in any folder of its
    /// chain, those only English has aside.
    fn unwritten(&self, locale: UiLocale) -> Vec<String> {
        let ids_in = |folder: &str| -> BTreeSet<&str> {
            self.layers
                .iter()
                .filter(|(dir, _)| *dir == folder)
                .flat_map(|(_, resource)| resource.entries())
                .filter_map(|entry| match entry {
                    fluent_syntax::ast::Entry::Message(m) => Some(m.id.name),
                    _ => None,
                })
                .collect()
        };
        let own: BTreeSet<&str> = locale.chain().iter().flat_map(|f| ids_in(f)).collect();
        ids_in(UiLocale::EN.chain()[0])
            .into_iter()
            .filter(|id| !own.contains(id))
            .filter(|id| !self.english_only.iter().any(|p| id.starts_with(p)))
            .map(str::to_string)
            .collect()
    }

    /// The languages a reader may be given, English first.
    pub fn shipped(&self) -> &[UiLocale] {
        &self.shipped
    }

    /// Whether a reader of `locale` whose software names `tag` counts and
    /// writes numbers as `tag` does, not as the row's own tag: only a
    /// region of the row's language that the row answers among others
    /// ("es-MX" under Latin America's Spanish). Plurals and numbers are
    /// looked up from the tag, so "pt" or "pt-AO" under Portugal's row
    /// would read as Brazil's.
    fn reads_region(&self, locale: UiLocale, tag: &str) -> bool {
        let answers = &locale.language().answers;
        let ours = tag
            .split_once('-')
            .is_some_and(|(lang, _)| lang == answers.lang);
        // Past 256 region tags a reader's numbers read as their language's
        // own tag; raise the cap if real regions come near it.
        let room = || {
            let bundles = self.bundles.read();
            bundles.len() < 256 || bundles.contains_key(&(locale, tag.to_string()))
        };
        ours && !matches!(answers.regions, Regions::Only(_)) && room()
    }

    /// The bundle for `locale` read in the region `tag` ("es-MX").
    fn bundle(&self, locale: UiLocale, tag: &str) -> Arc<Bundle> {
        if locale == UiLocale::EN {
            return self.en.clone();
        }
        let key = (locale, tag.to_string());
        if let Some(b) = self.bundles.read().get(&key) {
            return b.clone();
        }
        let made = Arc::new(bundle(&self.layers, locale, tag));
        self.bundles.write().entry(key).or_insert(made).clone()
    }

    /// The number and date formatters for the region `tag`; None when
    /// ICU4X knows nothing of it.
    fn formats(&self, tag: &str) -> Option<Arc<format::Formats>> {
        if let Some(f) = self.formats.read().get(tag) {
            return f.clone();
        }
        let made = format::Formats::new(tag).map(Arc::new);
        if made.is_none() {
            tracing::warn!(tag, "no number or date formats for a region");
        }
        self.formats
            .write()
            .entry(tag.to_string())
            .or_insert(made)
            .clone()
    }

    /// `locale`'s list joiners as a page's scripts read them
    /// (`lists::joiners` for each kind), worked out once: every page's
    /// script header carries them, and they never change.
    fn script_lists(&self, locale: UiLocale) -> serde_json::Value {
        self.lists
            .lock()
            .entry(locale)
            .or_insert_with(|| {
                serde_json::json!({
                    "and": lists::joiners(locale, ListKind::And),
                    "or": lists::joiners(locale, ListKind::Or),
                    "unit": lists::joiners(locale, ListKind::Unit),
                })
            })
            .clone()
    }

    /// `id` (or `id.attribute`) in `bundle`, else in English; None when
    /// neither has it.
    fn try_format(&self, bundle: &Bundle, id: &str, args: Option<&FluentArgs>) -> Option<String> {
        format_in(bundle, id, args).or_else(|| format_in(&self.en, id, args))
    }

    /// `id` as `try_format` finds it, else the id itself, which is
    /// recorded as missing.
    fn format(&self, bundle: &Bundle, id: &str, args: Option<&FluentArgs>) -> String {
        if let Some(text) = self.try_format(bundle, id, args) {
            return text;
        }
        let mut missing = self.missing.lock();
        if missing.insert(id.to_string()) {
            tracing::warn!(id, "a message is missing from every catalog");
        }
        id.to_string()
    }

    /// The ids asked for and found in no language, English included.
    pub fn missing(&self) -> Vec<String> {
        self.missing.lock().iter().cloned().collect()
    }
}

/// `locale`'s messages (its folders, each over the one before) for a
/// reader in the region `tag`, numbers formatted as the region writes
/// them.
fn bundle(layers: &[(&'static str, Arc<FluentResource>)], locale: UiLocale, tag: &str) -> Bundle {
    let id: unic_langid::LanguageIdentifier = tag
        .parse()
        .or_else(|_| locale.tag().parse())
        .expect("a UiLocale's tag parses");
    let mut bundle = Bundle::new_concurrent(vec![id]);
    bundle.set_use_isolating(false);
    bundle.set_formatter(Some(match locale.numbers() {
        Numbers::Plain => format_en,
        Numbers::Cldr => format_region,
    }));
    for folder in locale.chain() {
        for (_, resource) in layers.iter().filter(|(dir, _)| dir == folder) {
            bundle.add_resource_overriding(resource.clone());
        }
    }
    bundle
}

fn format_in(bundle: &Bundle, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    let (message, attribute) = match id.split_once('.') {
        Some((m, a)) => (m, Some(a)),
        None => (id, None),
    };
    let message = bundle.get_message(message)?;
    let pattern = match attribute {
        Some(a) => message.get_attribute(a)?.value(),
        None => message.value()?,
    };
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    if !errors.is_empty() {
        tracing::warn!(id, ?errors, "a message did not format cleanly");
    }
    Some(text.into_owned())
}

/// A number as English pages have always written it: digits, no
/// grouping.
fn format_en(value: &FluentValue, _: &IntlLangMemoizer) -> Option<String> {
    match value {
        FluentValue::Number(n) if n.value.fract() == 0.0 => Some(format!("{}", n.value as i64)),
        _ => None,
    }
}

/// A number as the bundle's region writes it (CLDR, through ICU4X):
/// whole numbers grouped as the region groups them, others to at most
/// two places.
fn format_region(value: &FluentValue, intls: &IntlLangMemoizer) -> Option<String> {
    let FluentValue::Number(n) = value else {
        return None;
    };
    let v = n.value;
    intls
        .with_try_get::<RegionNumbers, _, _>((), |f| {
            if v.fract() == 0.0 {
                f.0.integer(v as i64)
            } else {
                f.0.decimal(v, 2)
            }
        })
        .ok()
}

/// A region's number formatter, made once per bundle by the bundle's
/// own memoizer.
struct RegionNumbers(format::Decimals);

impl intl_memoizer::Memoizable for RegionNumbers {
    type Args = ();
    type Error = ();
    fn construct(lang: unic_langid::LanguageIdentifier, _: ()) -> Result<Self, ()> {
        format::Decimals::new(&lang.to_string())
            .map(RegionNumbers)
            .ok_or(())
    }
}

/// The catalog as one reader reads it: their language (one Flash has
/// shipped), and the tag their own software names, for formatting.
#[derive(Clone)]
pub struct Tr {
    catalog: Arc<Catalog>,
    locale: UiLocale,
    format_tag: String,
    bundle: Arc<Bundle>,
    /// On a public page with an address per language: that page, for its
    /// links to itself in its other languages.
    page: Option<PublicPage>,
    /// A language this English page's visitor reads, offered as a link
    /// to the page in it ("Ver en español").
    offer: Option<UiLocale>,
    /// The languages the reader's browser asks for, most wanted first,
    /// and its time zone: what a language picker lists first.
    asked: Vec<String>,
    zone: Option<String>,
}

impl Tr {
    /// `locale` as shipped (an unshipped one reads English), formatted
    /// for `format_tag` ("es-AR"; the locale's own tag when None).
    pub fn new(catalog: Arc<Catalog>, locale: UiLocale, format_tag: Option<&str>) -> Tr {
        let locale = if catalog.shipped.contains(&locale) {
            locale
        } else {
            UiLocale::EN
        };
        let format_tag = format_tag
            .filter(|_| locale.numbers() == Numbers::Cldr)
            .and_then(flash_core::locale::normalize)
            .filter(|tag| catalog.reads_region(locale, tag))
            .unwrap_or_else(|| locale.tag().to_string());
        let bundle = catalog.bundle(locale, &format_tag);
        Tr {
            catalog,
            locale,
            format_tag,
            bundle,
            page: None,
            offer: None,
            asked: Vec::new(),
            zone: None,
        }
    }

    /// The same reader, whose browser asks for `asked` in time zone
    /// `zone`.
    pub fn asking(mut self, asked: Vec<String>, zone: Option<String>) -> Tr {
        self.asked = asked;
        self.zone = zone;
        self
    }

    /// `shipped` in the order a language picker lists them for this
    /// reader (`picker::order`): the browser's own first.
    pub fn picker_order(&self, shipped: &[UiLocale]) -> Vec<UiLocale> {
        picker::order(shipped, &self.asked, self.zone.as_deref())
    }

    /// The same reader on the public page `page`, with `offer` to link to
    /// (when the page is written in it).
    pub fn on_page(mut self, page: PublicPage, offer: Option<UiLocale>) -> Tr {
        self.page = Some(page);
        self.offer = offer.filter(|l| *l != self.locale && page.path_in(*l).is_some());
        self
    }

    /// Where a link to `path` goes for this reader. A template names a
    /// public page by its English address (`/guide`; a page in Spanish
    /// only by its Spanish one), and the link goes to the page in the
    /// reader's language (`/es/guia`) when it is written in it. From a
    /// public page, any other link carries the language
    /// (`/login?hl=es-419`). Otherwise `path` as it is.
    pub fn href(&self, path: &str) -> String {
        let (bare, query) = match path.split_once('?') {
            Some((b, q)) => (b, Some(q)),
            None => (path, None),
        };
        if let Some(at) = self
            .catalog
            .page_named(bare)
            .and_then(|p| p.path_in(self.locale))
        {
            return match query {
                Some(q) => format!("{at}?{q}"),
                None => at,
            };
        }
        if self.locale == UiLocale::EN || self.page.is_none() {
            return path.to_string();
        }
        with_hl(path, self.locale)
    }

    /// Where a form posts for this reader: `path` itself (no page has a
    /// language's address for a POST), with the language on the link when
    /// it is not English, so the answer comes back in it.
    pub fn action(&self, path: &str) -> String {
        if self.locale == UiLocale::EN {
            return path.to_string();
        }
        with_hl(path, self.locale)
    }

    /// The languages Flash ships that this public page is written in,
    /// each with its address there; empty off a public page, or while
    /// English is the only language.
    fn page_languages(&self) -> Vec<(UiLocale, String)> {
        let Some(page) = self.page else {
            return Vec::new();
        };
        if self.catalog.shipped().len() < 2 {
            return Vec::new();
        }
        self.catalog
            .shipped()
            .iter()
            .filter_map(|l| page.path_in(*l).map(|at| (*l, at)))
            .collect()
    }

    /// The `<link rel="alternate" hreflang>` tags of this public page:
    /// one per language it is written in (Google reads no "419": the
    /// Latin American pages are `es`, Spanish for everyone outside Spain),
    /// and `x-default` (English, when there is one).
    pub fn alternates(&self, base: &str) -> String {
        let base = base.trim_end_matches('/');
        let mut out = String::new();
        for (locale, at) in self.page_languages() {
            let hreflang = locale.hreflang();
            out.push_str(&format!(
                r#"<link rel="alternate" hreflang="{hreflang}" href="{base}{at}">"#
            ));
            if locale == UiLocale::EN {
                out.push_str(&format!(
                    r#"<link rel="alternate" hreflang="x-default" href="{base}{at}">"#
                ));
            }
        }
        out
    }

    /// The Open Graph locale of this public page (`og:locale`) and its
    /// other languages (`og:locale:alternate`).
    pub fn og_locale(&self) -> String {
        let languages = self.page_languages();
        if languages.is_empty() {
            return String::new();
        }
        let mut out = format!(
            r#"<meta property="og:locale" content="{}">"#,
            self.locale.og()
        );
        for (l, _) in languages.iter().filter(|(l, _)| *l != self.locale) {
            out.push_str(&format!(
                r#"<meta property="og:locale:alternate" content="{}">"#,
                l.og()
            ));
        }
        out
    }

    /// The link that offers this English page in a language its visitor
    /// reads ("Ver en español"), or nothing.
    pub fn offer_link(&self) -> String {
        let (Some(page), Some(offer)) = (self.page, self.offer) else {
            return String::new();
        };
        let Some(at) = page.path_in(offer) else {
            return String::new();
        };
        let words = self.catalog.format(
            &self.catalog.bundle(offer, offer.tag()),
            "language-offer",
            None,
        );
        format!(
            r#"<p class="lang-offer"><a href="{at}" hreflang="{tag}" lang="{tag}">{words}</a></p>"#,
            tag = offer.tag(),
            words = escape(&words),
        )
    }

    pub fn english(catalog: Arc<Catalog>) -> Tr {
        Tr::new(catalog, UiLocale::EN, None)
    }

    pub fn locale(&self) -> UiLocale {
        self.locale
    }

    /// The page's `lang`.
    pub fn html_lang(&self) -> &'static str {
        self.locale.tag()
    }

    /// The page's `dir`: "ltr", or "rtl" for a language written right to
    /// left, which the browser then mirrors wherever the styles say
    /// start and end.
    pub fn html_dir(&self) -> &'static str {
        self.locale.dir().as_str()
    }

    /// The same reader in `locale`: numbers and dates as their own
    /// software writes them when it reads that language.
    fn reading(&self, locale: UiLocale) -> Tr {
        let format = (self.locale == locale).then_some(self.format_tag.as_str());
        Tr::new(self.catalog.clone(), locale, format)
    }

    /// The tag numbers and dates are formatted for ("es-AR").
    pub fn format_tag(&self) -> &str {
        &self.format_tag
    }

    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// The reader's region's number and date formatters; None for a
    /// language that writes numbers plainly (English, whose pages keep
    /// their own forms, `crate::when`).
    pub fn formats(&self) -> Option<Arc<format::Formats>> {
        if self.locale.numbers() == Numbers::Plain {
            return None;
        }
        self.catalog.formats(&self.format_tag)
    }

    /// A message.
    pub fn t(&self, id: &str) -> Words {
        self.finish(self.catalog.format(&self.bundle, id, None))
    }

    /// A message, or None when no catalog has it (for ids made from
    /// data, such as a language's code, which may name nothing).
    pub fn get(&self, id: &str) -> Option<Words> {
        self.catalog
            .try_format(&self.bundle, id, None)
            .map(|text| self.finish(text))
    }

    /// A formatted message as its reader gets it: as it is, or turned
    /// for a pseudo-language (`pseudo`).
    fn finish(&self, text: String) -> Words {
        Words(match self.locale.pseudo() {
            Some(how) => pseudo::turn(&text, how),
            None => text,
        })
    }

    /// A message with its variables. A message whose id ends in `-html`
    /// holds markup of our own, and its text variables are escaped into
    /// it; any other message is plain text, escaped where it is shown.
    pub fn args(&self, id: &str, args: &[(&str, FluentValue)]) -> Words {
        let html = is_html(id);
        let rtl = self.locale.dir() == Dir::Rtl;
        let turned = self.locale.pseudo().is_some();
        let mut marked_any = false;
        let args: FluentArgs = args
            .iter()
            .map(|(k, v)| {
                let FluentValue::String(text) = v else {
                    return (*k, v.clone());
                };
                let set = if !html {
                    bidi::isolate(text, rtl)
                } else if bidi::needs_isolating(text, rtl) {
                    marked_any = true;
                    bidi::mark(&escape(text))
                } else {
                    escape(text)
                };
                let set = if turned {
                    format!("{}{set}{}", pseudo::OPEN, pseudo::CLOSE)
                } else {
                    set
                };
                (*k, FluentValue::from(set))
            })
            .collect();
        let words = self.finish(self.catalog.format(&self.bundle, id, Some(&args)));
        if marked_any {
            Words(bidi::settle(&words.0))
        } else {
            words
        }
    }
}

/// A message that holds markup of our own: its id (before any
/// attribute) ends in `-html`, and a page shows it with `|safe`.
pub fn is_html(id: &str) -> bool {
    id.split('.').next().is_some_and(|m| m.ends_with("-html"))
}

fn escape(text: &str) -> String {
    askama::filters::escape(text, askama::filters::Html)
        .map(|e| e.to_string())
        .unwrap_or_default()
}

/// `path` with the reader's language on the link (`?hl=es-419`, `&hl=`
/// after a query).
fn with_hl(path: &str, locale: UiLocale) -> String {
    let joiner = if path.contains('?') { '&' } else { '?' };
    format!("{path}{joiner}hl={}", locale.tag())
}

/// What a deployment adds to the open core's words: its messages, its
/// public pages with an address per language, and the starts of the ids
/// of its messages written in English only. One value, so a server and
/// anything that must render what that server renders build the same
/// catalog (`server_catalog`).
pub struct DeploymentWords {
    /// Read after the core's: a message the core has, defined again with
    /// a note line `overrides: <why>`, is worded the deployment's way.
    pub resources: &'static [Resource],
    pub public_pages: &'static [PublicPage],
    pub english_only: &'static [&'static str],
}

impl DeploymentWords {
    /// The open core alone.
    pub const NONE: DeploymentWords = DeploymentWords {
        resources: &[],
        public_pages: &[],
        english_only: &[],
    };
}

/// The catalog a server with `words` gives its readers in, giving them
/// `languages` (English and the languages FLASH_LANGUAGES names).
pub fn server_catalog(
    words: &DeploymentWords,
    languages: &[UiLocale],
) -> Result<Arc<Catalog>, String> {
    let mut resources = CORE.to_vec();
    resources.extend_from_slice(words.resources);
    Ok(Arc::new(
        Catalog::new(&resources)
            .english_only(words.english_only)
            .shipping(languages)
            .map_err(|e| format!("FLASH_LANGUAGES: {e}"))?
            .localizing(words.public_pages),
    ))
}

/// The open core's own messages, English only: what `tr` reads for work
/// that names no reader (no request, no `scoped` or `within`). Never a
/// server's catalog: a deployment's words reach work outside a request
/// through the reader that work is given (`Site::english`, `for_user`),
/// so a mail worded with no reader shows its message ids, every time.
pub fn core_catalog() -> Arc<Catalog> {
    static CORE_ONLY: std::sync::LazyLock<Arc<Catalog>> =
        std::sync::LazyLock::new(|| Arc::new(Catalog::new(CORE)));
    CORE_ONLY.clone()
}

/// The reader of the request being served, or of the work `scoped` or
/// `within` names: what a handler, or the code it calls, speaks in. A job
/// or an email names its reader (`for_user`, `Site::english`); with none,
/// the open core's English (`core_catalog`).
pub fn tr() -> Tr {
    current().unwrap_or_else(|| Tr::english(core_catalog()))
}

/// A message in the language of the request being served (`tr`).
pub fn t(id: &str) -> Words {
    tr().t(id)
}

/// A message with its variables in the language of the request being
/// served (`tr`).
pub fn ta(id: &str, args: &[(&str, FluentValue)]) -> Words {
    tr().args(id, args)
}

/// `items` as one list in the language of the request being served
/// (`list_in`).
pub fn list<S: AsRef<str>>(items: &[S], kind: ListKind) -> String {
    list_in(tr().locale(), items, kind)
}

/// The hour a written time of day shows: 1 for "1:05 pm", 13 for
/// "13:05"; -1 when it starts with no digit. A message passes it beside
/// the time for a language whose words agree with it (Spanish "la 1" but
/// "las 2"), which its English note offers as a spare variable.
pub fn clock_hour(time: &str) -> i64 {
    let digits: String = time.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(-1)
}

/// Whether the work at hand is read in English: the request's reader
/// reads it, or no reader was set (a job, a test calling a function
/// directly).
pub fn reads_english() -> bool {
    tr().locale() == UiLocale::EN
}

/// Words a piece of code holds in English, in the reader's language:
/// `english` as it is for an English reader (and for work with no
/// reader), the message `id` for any other. For a sentence a test or a
/// client already holds as written, one made where no request's catalog
/// may be at hand, or one a library words in English.
pub fn t_or(id: &str, english: &str) -> Words {
    if reads_english() {
        Words(english.to_string())
    } else {
        t(id)
    }
}

/// A number a message's plural is picked by, of whatever integer type a
/// template holds it in.
pub trait Count {
    fn value(&self) -> Value<'static>;
}

macro_rules! counts {
    ($($t:ty),*) => {
        $(impl Count for $t {
            fn value(&self) -> Value<'static> {
                Value::from(*self)
            }
        })*
    };
}
counts!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl<C: Count + ?Sized> Count for &C {
    fn value(&self) -> Value<'static> {
        (**self).value()
    }
}

/// What the tests hold every catalog to (the core's and a deployment's):
/// each file parses, each English message says where it shows, each
/// translation names only messages English has and uses the same
/// variables, a Spanish variant only rewords messages the shared base
/// has, and every message a template asks for exists in English.
#[cfg(feature = "test-support")]
pub mod lint;
