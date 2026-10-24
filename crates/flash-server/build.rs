//! Lists the open core's message files (`i18n::CORE`) from `locales/`,
//! and fails the build on a language written into its code
//! (build/literals.rs) or a style naming a side of the screen
//! (build/physical.rs).

#[path = "build/i18n.rs"]
mod i18n;
#[path = "build/literals.rs"]
mod literals;
#[path = "build/physical.rs"]
mod physical;
#[allow(dead_code, reason = "the build reads only the table's rows")]
#[path = "../flash-core/src/locale/registry.rs"]
mod registry;
#[path = "build/tree.rs"]
mod tree;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build");
    println!("cargo::rerun-if-changed=../flash-core/src/locale/registry.rs");
    i18n::catalogs("catalog_files.rs");
    literals::scan(&[
        (
            "src/deck_languages.rs",
            "the codes a deck's cards may be spoken in: languages by nature, never the reader's",
        ),
        (
            "src/i18n/packs.rs",
            "a style pack is data named by the message folder it is for",
        ),
        (
            "src/testing.rs",
            "the test harness names the languages its tests read",
        ),
    ]);
    physical::scan(
        &["static", "templates", "panel"],
        &[(
            "static/katex/",
            "KaTeX's own stylesheet, vendored as it ships: math is laid out left to right in every language",
        )],
    );
}
