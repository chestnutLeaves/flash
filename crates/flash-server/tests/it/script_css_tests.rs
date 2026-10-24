//! The stylesheets' block for each script (`i18n::scripts`), written from
//! the table of languages into every sheet that carries it, and failing
//! while a copy is stale:
//!
//!   FLASH_WRITE_SCRIPT_CSS=1 cargo test -p flash-server --test it script_css_tests::

use std::path::PathBuf;

use flash_core::locale::{Script, TypeKind};
use flash_server::i18n::scripts::{css, panel_css, splice};

#[test]
fn every_sheet_carries_the_tables_block() {
    let write = std::env::var("FLASH_WRITE_SCRIPT_CSS").as_deref() == Ok("1");
    // The site's sheet, and the study panel's (its fonts at the server's
    // address, which the server writes in).
    for (sheet, block) in [
        ("static/app.css", css("")),
        ("panel/panel.html", panel_css("__FLASH_ORIGIN__")),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(sheet);
        let held = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        let fresh = splice(&held, &block).unwrap_or_else(|| panic!("{sheet} has no marks"));
        if write {
            std::fs::write(&path, &fresh).unwrap();
        } else {
            assert!(
                held == fresh,
                "{sheet}'s scripts block is stale. Rerun with FLASH_WRITE_SCRIPT_CSS=1"
            );
        }
    }
}

/// English keeps the fonts the site always had, and nothing is added to
/// its sizes, line heights or tracking.
#[test]
fn english_is_set_as_it_always_was() {
    let block = css("");
    let latin = block
        .lines()
        .find(|l| l.starts_with(":root,[data-script=\"Latn\"]"))
        .expect("the root carries Latin");
    for part in [
        "--script-sans:\"Geist\",-apple-system,\"Segoe UI\",sans-serif;",
        "--script-mono:\"Geist Mono\",ui-monospace,\"SF Mono\",monospace;",
        "--script-leading:0;",
        "--script-step:0px;",
        "--script-track:1;",
        "--script-case:uppercase}",
    ] {
        assert!(latin.contains(part), "{latin} lacks {part}");
    }
}

/// Each script the table knows has its own rule, and what Material's
/// categories and the scripts' own nature ask for: tall and dense
/// scripts a tenth of an em more line height and a pixel more size,
/// caseless ones no tracking and no uppercase labels, Greek out of
/// Geist, and Han faces before the system's own.
#[test]
fn each_script_is_set_for_its_readers() {
    let block = css("");
    for script in Script::ALL {
        let style = script.style();
        let rule = block
            .lines()
            .find(|l| l.contains(&format!("[data-script=\"{}\"]", script.code())))
            .unwrap_or_else(|| panic!("no rule for {}", script.code()));
        let roomy = style.kind != TypeKind::EnglishLike;
        assert_eq!(rule.contains("--script-leading:.1;"), roomy, "{rule}");
        assert_eq!(rule.contains("--script-step:1px;"), roomy, "{rule}");
        assert_eq!(rule.contains("--script-track:1;"), style.cased, "{rule}");
        assert_eq!(
            rule.contains("--script-case:uppercase"),
            style.cased,
            "{rule}"
        );
        assert_eq!(rule.contains("\"Geist\""), style.brand, "{rule}");
        for face in style.faces {
            assert!(rule.contains(&format!("\"{face}\"")), "{rule} lacks {face}");
        }
    }
    let greek = Script::Grek.style();
    assert!(!greek.brand && greek.cased);
    for script in [Script::Arab, Script::Hebr, Script::Deva, Script::Thai] {
        assert!(!script.style().cased, "{script:?}");
    }
    // A Han face first, before the system's own face, which would take
    // its glyphs from the system's language rather than the page's.
    for script in [Script::Hans, Script::Hant, Script::Jpan, Script::Kore] {
        let style = script.style();
        let sans = style.sans_stack();
        let mono = style.mono_stack();
        assert!(
            sans.find(style.faces[0]).unwrap() < sans.find("-apple-system").unwrap(),
            "{sans}"
        );
        assert!(
            mono.find(style.faces[0]).unwrap() < mono.find("ui-monospace").unwrap(),
            "{mono}"
        );
    }
}
