//! The build's scan for styles that name a side of the screen
//! (build/physical.rs): what it reads as a side, what it leaves alone,
//! what a marker must be, and the arrows that must turn. Each case is a
//! file's source, as the build reads one.

use crate::physical::problems_in;

fn css(source: &str) -> Vec<String> {
    problems_in("static/x.css", source)
}

fn html(source: &str) -> Vec<String> {
    problems_in("templates/x.html", source)
}

#[test]
fn a_side_in_a_stylesheet_fails() {
    let found = css(concat!(
        ".a{margin-left:4px}\n",
        ".b{padding-right:2px;right:0}\n",
        ".c{border-left:3px solid red}\n",
        ".d{text-align:left;float:right;clear:left}\n",
        ".e{background-position:right 10px center}\n",
        ".f{margin:0 4px 0 2px}\n",
        ".g{border-radius:6px 0 0 6px}\n",
        ".h{border-top-left-radius:4px}\n",
        ".i{LEFT:0 !important}\n",
    ));
    let lines: Vec<&str> = found
        .iter()
        .map(|p| p.split(": `").next().unwrap())
        .collect();
    assert_eq!(
        lines,
        [
            "static/x.css:1",
            "static/x.css:2",
            "static/x.css:2",
            "static/x.css:3",
            "static/x.css:4",
            "static/x.css:4",
            "static/x.css:4",
            "static/x.css:5",
            "static/x.css:6",
            "static/x.css:7",
            "static/x.css:8",
            "static/x.css:9",
        ],
        "{found:#?}"
    );
    assert!(found[0].contains("`margin-left: 4px`"), "{found:#?}");
    assert!(found[0].contains("margin-inline-start"), "{found:#?}");
}

#[test]
fn start_end_and_what_is_the_same_both_ways_pass() {
    let found = css(concat!(
        ".a{margin-inline-start:4px;padding-inline:2px 8px;inset-inline-end:0}\n",
        ".b{text-align:start;text-align:center;float:inline-end}\n",
        ".c{margin:0 4px;padding:1px 2px 3px 2px;margin:4px}\n",
        ".d{border-radius:6px;border-radius:2px 2px 0 0;border-radius:50%/10%}\n",
        ".e:hover,.e::after{color:red}\n",
        "@media (min-width:721px){.f{top:0;bottom:0}}\n",
        ".g{background:url('data:image/svg+xml;utf8,<svg left=\"1\"/>') no-repeat center}\n",
        ".h{transition:left 1s;--edge-left:3px}\n",
        "/* margin-left: in a comment */\n",
    ));
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_marker_excuses_the_one_declaration_after_it() {
    let found = css(concat!(
        ".a{/* physical: centred either way */left:50%;right:0}\n",
        ".b{/* physical: */margin-left:0}\n",
        ".c{/* physical: no side here */margin-inline-start:0}\n",
        ".d{color:red}\n",
        "/* physical: at the end */\n",
    ));
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(
        found[0].starts_with("static/x.css:1: `right: 0`"),
        "{found:#?}"
    );
    assert_eq!(found[1], "static/x.css:2: `physical:` gives no reason");
    assert!(
        found[2].starts_with("static/x.css:3: `physical:` stands over `margin-inline-start`"),
        "{found:#?}"
    );
    assert!(
        found[3].starts_with("static/x.css:5: `physical:` stands over no side"),
        "{found:#?}"
    );
}

#[test]
fn a_template_is_read_in_its_styles_and_its_attributes() {
    let found = html(concat!(
        "<style>\n",
        ".a{color:red}\n",
        ".b{padding-left:2px}\n",
        "</style>\n",
        "<td style=\"text-align:right;white-space:nowrap\">{{ n }}</td>\n",
        "<td align=\"left\">x</td>\n",
        "<link rel=\"stylesheet\" href=\"/static/app.css\">\n",
        "<div class=\"x\" data-style=\"left:0\">x</div>\n",
    ));
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(
        found[0].starts_with("templates/x.html:3: `padding-left: 2px`"),
        "{found:#?}"
    );
    assert!(
        found[1].starts_with("templates/x.html:5: `text-align: right`"),
        "{found:#?}"
    );
    assert!(
        found[2].starts_with("templates/x.html:6: `align=\"left\"`"),
        "{found:#?}"
    );
}

#[test]
fn a_template_says_its_side_by_the_reader_or_with_a_reason() {
    let found = html(concat!(
        // A mail client's inline style, its side from the reader's direction.
        "<td style=\"padding-{{ \"\"|start_side }}:9px\">Flash</td>\n",
        "<td align=\"center\" style=\"--depth:{{ depth }}\">x</td>\n",
        "{# physical: the safe areas are the device's own #}\n",
        "<main style=\"padding-left:{{ inset }}px\">\n",
        "{# not a marker #}\n",
        "<p style=\"margin-right:0\">x</p>\n",
    ));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].starts_with("templates/x.html:6: `margin-right: 0`"),
        "{found:#?}"
    );
}

#[test]
fn an_arrow_turns_only_as_the_whole_of_a_dir_element() {
    let found = html(concat!(
        "<a href=\"/\"><span class=\"fl-dir\" aria-hidden=\"true\">&larr;</span> Back</a>\n",
        "<a href=\"/\">&larr; Back</a>\n",
        "<button>Next \u{2192}</button>\n",
        "<span class=\"fl-dir\">&rarr; more</span>\n",
        // The angle quotes mirror by themselves.
        "<nav>Decks &rsaquo; Biology \u{203a} Cells</nav>\n",
        "{# &larr; in a comment #}\n",
    ));
    assert_eq!(found.len(), 3, "{found:#?}");
    let lines: Vec<&str> = found
        .iter()
        .map(|p| p.split(": `").next().unwrap())
        .collect();
    assert_eq!(
        lines,
        [
            "templates/x.html:2",
            "templates/x.html:4",
            "templates/x.html:3"
        ],
        "{found:#?}"
    );
    // A stylesheet holds no arrows to turn.
    assert!(css(".a::before{content:\"\u{2192}\"}").is_empty());
}

#[test]
fn a_page_says_its_language_direction_and_script() {
    let found = html(concat!(
        "<!doctype html>\n",
        "<html lang=\"{{ \"\"|html_lang }}\" dir=\"{{ \"\"|html_dir }}\" data-script=\"{{ \"\"|html_script }}\">\n",
        "<html lang=\"en\">\n",
        "<HTML>\n",
        "<p>An <htmlish> word</p>\n",
        "<html lang=\"en\" dir=\"ltr\">\n",
    ));
    assert_eq!(found.len(), 3, "{found:#?}");
    for (at, line) in found.iter().zip([3, 4, 6]) {
        assert!(
            at.starts_with(&format!(
                "templates/x.html:{line}: <html> says no lang, dir and data-script"
            )),
            "{found:#?}"
        );
    }
    // A mail's layout carries its styles inline: no script on its root.
    let mail = problems_in(
        "templates/email_base.html",
        "<html lang=\"{{ \"\"|html_lang }}\" dir=\"{{ \"\"|html_dir }}\">\n",
    );
    assert!(mail.is_empty(), "{mail:#?}");
}

#[test]
fn tracking_and_case_come_from_the_scripts_tokens() {
    let found = css(concat!(
        ".a{letter-spacing:.06em}\n",
        ".b{letter-spacing:-0.4px;text-transform:uppercase}\n",
        ".c{text-transform:capitalize}\n",
    ));
    assert_eq!(found.len(), 4, "{found:#?}");
    assert!(
        found
            .iter()
            .all(|p| p.contains("is set the same for every script")),
        "{found:#?}"
    );
    let fine = css(concat!(
        ".a{letter-spacing:var(--track-wide);text-transform:var(--label-case)}\n",
        ".b{letter-spacing:0;letter-spacing:normal;text-transform:none}\n",
        ".c{letter-spacing:calc(-.01em * var(--script-track))}\n",
    ));
    assert!(fine.is_empty(), "{fine:#?}");
    // A mail's inline style takes the filter's value.
    let mail = html("<td style=\"letter-spacing:{{ \"-0.2px\"|tracking }}\">x</td>\n");
    assert!(mail.is_empty(), "{mail:#?}");
}
