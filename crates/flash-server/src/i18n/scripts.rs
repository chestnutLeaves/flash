//! The stylesheets' words for each script, written from the table of
//! languages (`flash_core::locale::Script::style`): its fonts, the line
//! height and size it adds to English's, whether it takes tracking, and
//! the case its labels are set in. A page names its script on its root
//! (`<html data-script="Arab">`); every stylesheet builds its own tokens
//! on these properties (`--font-sans: var(--script-sans)`, a leading of
//! `calc(1.55 + var(--script-leading))`), so a script is set as its
//! readers expect without a rule naming it. English's values are the
//! root's, the same as the stylesheets always had.
//!
//! The block sits between two marks in each stylesheet that carries it
//! (the site's, the study panel's) and is written by a test, which fails
//! while a copy is stale:
//!
//!   FLASH_WRITE_SCRIPT_CSS=1 cargo test -p flash-server --test it script_css_tests::

use flash_core::locale::Script;

/// Where the block starts in a stylesheet.
pub const BEGIN: &str =
    "/* ---- scripts: written from the table of languages (i18n::scripts), not by hand ---- */";
/// Where it ends.
pub const END: &str = "/* ---- end of scripts ---- */";

/// The ranges of Geist's two parts per face, as tools/fonts/geist_web.py
/// cut them: the core files (Western Latin) and the extended ones (Latin
/// Extended, Vietnamese, Cyrillic).
const GEIST_RANGES: &str = include_str!("../../static/fonts/geist-ranges.json");

/// The faces with an extended part, and the weight each is.
const FACES: [(&str, &str, u16); 6] = [
    ("Geist", "Geist-Regular", 400),
    ("Geist", "Geist-Medium", 500),
    ("Geist", "Geist-SemiBold", 600),
    ("Geist Mono", "GeistMono-Regular", 400),
    ("Geist Mono", "GeistMono-Medium", 500),
    ("Geist Mono", "GeistMono-SemiBold", 600),
];

/// The extended files' version, in their address.
const EXT_VERSION: u32 = 2;

/// The `@font-face` rules for Geist's extended part, each loaded only for
/// a page whose text holds one of its characters (`unicode-range`); the
/// core part's own rules stand before the block. `origin` is put before
/// each address ("" on the site, the study panel's mark for the server's
/// address).
fn extended_faces(origin: &str, families: &[&str]) -> String {
    let ranges: serde_json::Value =
        serde_json::from_str(GEIST_RANGES).expect("geist-ranges.json parses");
    let mut out = String::new();
    for (family, file, weight) in FACES.iter().filter(|(f, _, _)| families.contains(f)) {
        let ext = ranges[file]["ext"]
            .as_str()
            .unwrap_or_else(|| panic!("geist-ranges.json has no ext range for {file}"));
        out.push_str(&format!(
            "@font-face{{font-family:\"{family}\";font-style:normal;font-weight:{weight};font-display:swap;src:url({origin}/static/fonts/{file}-ext.woff2?v={EXT_VERSION}) format(\"woff2\");unicode-range:{ext}}}\n"
        ));
    }
    out
}

/// The block for the site's stylesheet, whose fonts are at `origin`:
/// Geist's extended faces, then English's values on the root (and on a
/// page in Latin script), every other script's on a page naming it.
pub fn css(origin: &str) -> String {
    let mut out = extended_faces(origin, &["Geist", "Geist Mono"]);
    for script in Script::ALL {
        let style = script.style();
        let selector = if script == Script::Latn {
            format!(":root,[data-script=\"{}\"]", script.code())
        } else {
            format!("[data-script=\"{}\"]", script.code())
        };
        out.push_str(&format!(
            "{selector}{{--script-sans:{};--script-mono:{};--script-leading:{};--script-step:{}px;--script-track:{};--script-case:{}}}\n",
            style.sans_stack(),
            style.mono_stack(),
            if style.leading_tenths == 0 {
                "0".to_string()
            } else {
                format!(".{}", style.leading_tenths)
            },
            style.size_step,
            u8::from(style.cased),
            if style.cased { "uppercase" } else { "none" },
        ));
    }
    out
}

/// The block for the study panel, whose fonts are at `origin` and whose
/// own words are in the host's font: Geist's extended sans faces, for a
/// card in Polish or Russian, and whether the page's script takes
/// tracking. A host loads the page before the panel shows, so it carries
/// no more than that.
pub fn panel_css(origin: &str) -> String {
    let mut out = extended_faces(origin, &["Geist"]);
    for cased in [true, false] {
        let mut selectors: Vec<String> = Script::ALL
            .iter()
            .filter(|s| s.style().cased == cased)
            .map(|s| format!("[data-script=\"{}\"]", s.code()))
            .collect();
        if cased {
            selectors.insert(0, ":root".to_string());
        }
        out.push_str(&format!(
            "{}{{--script-track:{}}}\n",
            selectors.join(","),
            u8::from(cased)
        ));
    }
    out
}

/// `sheet` with its block replaced by `block`, or None when the sheet
/// has no marks.
pub fn splice(sheet: &str, block: &str) -> Option<String> {
    let start = sheet.find(BEGIN)? + BEGIN.len();
    let end = start + sheet[start..].find(END)?;
    Some(format!("{}\n{}{}", &sheet[..start], block, &sheet[end..]))
}
