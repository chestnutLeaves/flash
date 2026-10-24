//! The build's scan for languages written into code (build/literals.rs):
//! what it reads as a literal, what it leaves alone, and what a marker
//! must be. Each case is a file's source, as the build reads one.

use crate::literals::{names, problems_in};

fn problems(source: &str) -> Vec<String> {
    let (exact, prefixes) = names();
    problems_in("src/x.rs", source, &exact, &prefixes)
}

#[test]
fn a_language_or_its_address_in_code_fails() {
    let found = problems(concat!(
        "fn a() -> &'static str { \"es-ES\" }\n",
        "fn b() -> &'static str { \"/es/guia\" }\n",
        "fn c() -> &'static str { r#\"es-419\"# }\n",
        // Any case: a tag is the same tag.
        "fn d() -> &'static str { \"Es-es\" }\n",
    ));
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(
        found[0].starts_with("src/x.rs:1: \"es-ES\" names a language"),
        "{found:#?}"
    );
    assert!(
        found[1].starts_with("src/x.rs:2: \"/es/guia\""),
        "{found:#?}"
    );
    assert!(found[2].starts_with("src/x.rs:3: \"es-419\""), "{found:#?}");
    assert!(found[3].starts_with("src/x.rs:4: \"Es-es\""), "{found:#?}");
}

#[test]
fn what_is_no_language_in_code_passes() {
    let found = problems(concat!(
        "// \"es\" in a comment\n",
        "/* \"es\" /* nested */ \"en\" */\n",
        "/// \"es-ES\" in a doc comment\n",
        "fn a(x: &'static str) -> char { '\"' }\n",
        "fn b() -> char { '\\'' }\n",
        "fn c() -> &'static str { \"ES\" }\n",
        "fn d() -> &'static str { \"/estudio\" }\n",
        "fn e() -> &'static str { \"spanish\" }\n",
    ));
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn test_only_code_is_not_read_and_the_code_after_it_is() {
    let found = problems(concat!(
        "#[cfg(test)]\n",
        "mod tests { fn a() -> &'static str { \"es\" } }\n",
        "#[cfg(any(test, feature = \"test-support\"))]\n",
        "pub fn b() -> &'static str { \"es-ES\" }\n",
        "struct S {\n",
        "    #[cfg(test)]\n",
        "    field: u8,\n",
        "}\n",
        "fn after() -> &'static str { \"en\" }\n",
        "#[cfg(not(test))]\n",
        "fn shipped() -> &'static str { \"es\" }\n",
    ));
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].starts_with("src/x.rs:9: \"en\""), "{found:#?}");
    assert!(found[1].starts_with("src/x.rs:11: \"es\""), "{found:#?}");
}

#[test]
fn a_marker_says_why_over_its_literal_and_never_stands_alone() {
    let found = problems(concat!(
        "// lang-literal: Spain's own page, at its own address.\n",
        "fn a() -> &'static str { \"/es-es/oposiciones\" }\n",
        "fn b() -> &'static str { \"es\" } // lang-literal: on its own line\n",
        "// lang-literal:\n",
        "fn c() -> &'static str { \"es-419\" }\n",
        "// lang-literal: over nothing\n",
        "fn d() {}\n",
    ));
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(
        found[0].contains("src/x.rs:4: `// lang-literal:` gives no reason"),
        "{found:#?}"
    );
    assert!(
        found[1].contains("src/x.rs:6: `// lang-literal:` stands over no language literal"),
        "{found:#?}"
    );
}
