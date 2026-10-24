//! The catalogs and how a request finds its reader: which language a
//! page is in, what chose it, and the pickers that change it.

use crate::common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use common::*;
use flash_core::locale::UiLocale;
use flash_server::auth::AccountLang;
use flash_server::i18n::lint::Extras;
use flash_server::i18n::{self, Catalog};

fn templates() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .collect()
}

#[test]
fn the_core_catalogs_are_whole() {
    let problems = i18n::lint::check(i18n::CORE, &templates(), Extras::NONE);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Words a template may write as they are, in any language: names.
pub const NAMES: &[&str] = &[
    "Flash", "@Flash", "Claude", "ChatGPT", "Grok", "Mistral", "Gemini", "Anki",
];

/// The templates whose words all come from the catalog. Every template
/// joins this list as its words move; once all have, the list goes and
/// the test reads them all.
const EXTRACTED: &[&str] = &[
    "ai_stack.html",
    "auth_base.html",
    "card_editor.html",
    "card_editor_body.html",
    "card_page.html",
    "card_row.html",
    "card_rows.html",
    "base.html",
    "consent.html",
    "connect.html",
    "consent_done.html",
    "dashboard.html",
    "deck.html",
    "deck_rows.html",
    "deck_settings.html",
    "deck_subdecks.html",
    "decks.html",
    "email_base.html",
    "email_password_reset.html",
    "enroll.html",
    "import.html",
    "login.html",
    "notfound.html",
    "oauth_error.html",
    "reset_confirm.html",
    "reset_request.html",
    "settings.html",
    "study.html",
    "study_card.html",
    "study_more.html",
    "stats.html",
    "study_tags.html",
    "users.html",
];

#[test]
fn extracted_templates_write_no_words_of_their_own() {
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for path in templates() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !EXTRACTED.contains(&name.as_str()) {
            continue;
        }
        seen.push(name.clone());
        let text = std::fs::read_to_string(&path).unwrap();
        for raw in i18n::lint::raw_text(&text, NAMES) {
            problems.push(format!("{name}: {raw}"));
        }
    }
    for name in EXTRACTED.iter().filter(|n| !seen.iter().any(|s| s == *n)) {
        problems.push(format!("{name}: listed, but no such template"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// What each template still writes itself, for the work of moving it.
#[test]
#[ignore = "a report, not a check"]
fn words_left_in_templates() {
    let mut rows: Vec<(usize, String)> = templates()
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            let left = i18n::lint::raw_text(&text, NAMES);
            (
                left.len(),
                path.file_name().unwrap().to_string_lossy().to_string(),
            )
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    rows.sort();
    for (n, name) in &rows {
        println!("{n:4} {name}");
    }
    println!(
        "{} pieces in {} templates",
        rows.iter().map(|r| r.0).sum::<usize>(),
        rows.len()
    );
}

/// A page's islands for its scripts, as `(kind, json)`: `lang` for the
/// header, `i18n` for a folder's messages.
fn script_islands(html: &str) -> Vec<(String, serde_json::Value)> {
    const OPEN: &str = r#"<script type="application/json" data-flash-"#;
    html.split(OPEN)
        .skip(1)
        .map(|rest| {
            let (kind, body) = rest.split_once('>').unwrap();
            let json = body.split_once("</script>").unwrap().0;
            (kind.to_string(), serde_json::from_str(json).unwrap())
        })
        .collect()
}

/// The words a page's scripts carry: a header naming the language, its
/// folders in reading order and the region numbers are written for (none
/// in English); then the messages under the prefixes, a plural as its
/// variants, nothing that could close the script early, each message
/// once, in the folder its reader takes it from.
#[test]
fn a_page_carries_its_scripts_words() {
    let c = Arc::new(
        Catalog::new(&[
            ("en", "# a note\njs-left = { $count ->\n    [one] { $count } card left\n   *[other] { $count } cards left\n}\n# a note\njs-tag = </script> { $x }\n# a note\njs-bye = Bye\n# a note\nother = not carried\n"),
            ("es", "js-left = { $count ->\n    [one] queda { $count } tarjeta\n   *[other] quedan { $count } tarjetas\n}\njs-bye = Chao\n"),
            ("es-ES", "js-bye = Adiós\n"),
        ])
        .english_only(&["js-tag", "other"])
        .shipping(&[spain()])
        .unwrap(),
    );
    let english = i18n::Tr::english(c.clone()).script_island(&["js-"]);
    assert!(!english.contains("</script> "), "{english}");
    let islands = script_islands(&english);
    assert_eq!(islands.len(), 2, "{english}");
    let (kind, header) = &islands[0];
    assert_eq!(kind, "lang");
    assert_eq!(header["lang"], "en");
    assert_eq!(header["folders"], serde_json::json!(["en"]));
    assert_eq!(header["format"], serde_json::Value::Null);
    assert_eq!(header["dir"], "ltr");
    assert_eq!(header["script"], "Latn");
    let (kind, en) = &islands[1];
    assert_eq!(kind, "i18n");
    assert_eq!(en["folder"], "en");
    assert!(en["m"].get("other").is_none());
    assert_eq!(en["m"]["js-left"][0]["s"], "count");
    assert_eq!(en["m"]["js-left"][0]["d"], "other");
    assert_eq!(en["m"]["js-tag"][0], "</script> ");

    let html = i18n::Tr::new(c, spain(), Some("es-ES")).script_island(&["js-"]);
    let islands = script_islands(&html);
    let header = &islands[0].1;
    assert_eq!(header["folders"], serde_json::json!(["en", "es", "es-ES"]));
    assert_eq!(header["format"], "es-ES");
    let folder = |name: &str| {
        islands
            .iter()
            .find(|(k, j)| k == "i18n" && j["folder"] == name)
            .map(|(_, j)| j["m"].as_object().unwrap().clone())
            .unwrap_or_default()
    };
    let keys = |name: &str| folder(name).keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys("en"), ["js-tag"]);
    assert_eq!(keys("es"), ["js-left"]);
    assert_eq!(keys("es-ES"), ["js-bye"]);
    assert!(
        html.contains("quedan") && html.contains("Adiós") && !html.contains("Chao"),
        "{html}"
    );
}

/// A link keeps its reader's language: a public page with an address per
/// language goes to the reader's (`/es/about`); from such a page any
/// other link carries the language (`/login?hl=es-ES`); a form posts to
/// the bare path with it; English links stay as they are.
#[test]
fn links_keep_their_readers_language() {
    use i18n::PublicPage;
    let about = PublicPage::new(Some("/about"), &[("es", "/acerca")]);
    let c = Arc::new(
        Catalog::new(i18n::CORE)
            .shipping(&[spain(), latin_america()])
            .unwrap()
            .localizing(&[
                PublicPage::new(Some("/"), &[("es", "/")]),
                about,
                PublicPage::new(Some("/contact"), &[("es", "/contacto")]),
                PublicPage::english_only("/changelog"),
                PublicPage::new(None, &[("es-ES", "/guia")]),
            ]),
    );
    let en = i18n::Tr::english(c.clone());
    assert_eq!(en.href("/about"), "/about");
    assert_eq!(en.href("/"), "/");
    assert_eq!(en.action("/contact"), "/contact");
    let mx = i18n::Tr::new(c.clone(), latin_america(), Some("es-MX"));
    assert_eq!(mx.href("/about"), "/es/acerca");
    assert_eq!(mx.href("/"), "/es");
    assert_eq!(mx.href("/about?x=1"), "/es/acerca?x=1");
    assert_eq!(mx.href("/changelog"), "/changelog");
    assert_eq!(mx.href("/login"), "/login");
    let spain = i18n::Tr::new(c, spain(), None).on_page(about, None);
    assert_eq!(spain.href("/"), "/es-es");
    assert_eq!(spain.href("/guia"), "/es-es/guia");
    assert_eq!(spain.href("/login"), "/login?hl=es-ES");
    assert_eq!(
        spain.href("/login?next=/decks"),
        "/login?next=/decks&hl=es-ES"
    );
    assert_eq!(spain.action("/contact"), "/contact?hl=es-ES");
}

/// A page's address in each language it is written in.
#[test]
fn a_public_page_has_an_address_in_each_of_its_languages() {
    use i18n::PublicPage;
    let about = PublicPage::new(Some("/about"), &[("es", "/acerca")]);
    assert_eq!(about.path_in(UiLocale::EN).as_deref(), Some("/about"));
    assert_eq!(
        about.path_in(latin_america()).as_deref(),
        Some("/es/acerca")
    );
    assert_eq!(about.path_in(spain()).as_deref(), Some("/es-es/acerca"));
    let front = PublicPage::new(Some("/"), &[("es", "/")]);
    assert_eq!(front.path_in(latin_america()).as_deref(), Some("/es"));
    let english = PublicPage::english_only("/changelog");
    assert_eq!(english.path_in(spain()), None);
    let for_spain = PublicPage::new(None, &[("es-ES", "/guia")]);
    assert_eq!(for_spain.path_in(UiLocale::EN), None);
    assert_eq!(for_spain.path_in(latin_america()), None);
    assert_eq!(for_spain.path_in(spain()).as_deref(), Some("/es-es/guia"));
}

/// Every language keeps its packs' rules (each language's guide), as
/// its reader gets it.
#[test]
fn every_language_keeps_its_style() {
    let problems = i18n::lint::style(i18n::CORE, Extras::NONE);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every English message has its own in every language, and none is
/// left as its English.
#[test]
fn every_language_is_whole() {
    let problems = i18n::lint::missing(i18n::CORE, &[], Extras::NONE);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A folder of a language's chain holds English's messages with
/// English's variables, and a variant rewords only what its base has.
#[test]
fn a_translation_keeps_to_englishs_messages_and_its_base() {
    let resources: &[i18n::Resource] = &[
        ("en", "# a\ngreet = Hi { $name }\n# b\nbye = Bye\n"),
        ("es", "greet = Hola { $nombre }\nextra = Más\n"),
        ("es-ES", "bye = Adiós\n"),
    ];
    let problems = i18n::lint::check(resources, &[], Extras::NONE).join("\n");
    for expected in [
        "es: `greet` uses {\"nombre\"}, English {\"name\"}",
        "es: `extra` is not an English message",
        "es-ES: `bye` rewords a message es lacks",
    ] {
        assert!(problems.contains(expected), "{expected} in:\n{problems}");
    }
}

#[test]
fn a_message_defined_again_says_it_overrides() {
    let check =
        |resources: &[i18n::Resource]| i18n::lint::check(resources, &[], Extras::NONE).join("\n");
    let core = ("en", "# a\ngreet = Hi\n");
    let twice = check(&[core, ("en", "# b\ngreet = Hello\n")]);
    assert!(twice.contains("en: `greet` is defined twice"), "{twice}");
    let said = check(&[core, ("en", "# overrides: its own words\ngreet = Hello\n")]);
    assert!(!said.contains("`greet`"), "{said}");
    let alone = check(&[("en", "# overrides: nothing\ngreet = Hello\n")]);
    assert!(
        alone.contains("en: `greet` overrides no earlier message"),
        "{alone}"
    );
}

/// A plural choice names exactly its language's forms (CLDR's: English's
/// one and other, Spanish's one, many and other), exact numbers besides,
/// and falls back to `other`; a choice on a word is no plural.
#[test]
fn a_plural_names_its_languages_forms() {
    let resources: &[i18n::Resource] = &[
        ("en", "# a\nok = { $n ->\n    [0] none\n    [one] one\n   *[other] many\n}\n# b\nfew = { $n ->\n    [one] one\n    [few] few\n   *[other] many\n}\n# c\nword = { $kind ->\n    [deck] deck\n   *[card] card\n}\n"),
        ("es", "ok = { $n ->\n    [0] nada\n    [one] una\n    [many] de\n   *[other] varias\n}\nfew = { $n ->\n    [one] una\n   *[other] varias\n}\nword = { $kind ->\n    [deck] mazo\n   *[card] tarjeta\n}\n"),
        ("es-ES", "ok = { $n ->\n    [one] una\n    [many] de\n   *[many] varias\n}\n"),
    ];
    let problems = i18n::lint::plural_problems(resources).join("\n");
    for expected in [
        "en: `few` chooses among {\"few\", \"one\", \"other\"}",
        "es: `few` chooses among {\"one\", \"other\"}",
        "es-ES: `ok` falls back to [many], not *[other]",
    ] {
        assert!(problems.contains(expected), "{expected} in:\n{problems}");
    }
    assert!(!problems.contains("en: `ok`"), "{problems}");
    assert!(!problems.contains("es: `ok`"), "{problems}");
    assert!(!problems.contains("`word`"), "{problems}");
}

/// A standard's names (`Extras::standard_ids`) may read as English does.
#[test]
fn a_standards_names_may_read_as_english_does() {
    let resources: &[i18n::Resource] = &[
        ("en", "# a\nplace-ar = Argentina\n"),
        ("es", "place-ar = Argentina\n"),
    ];
    let still = "`place-ar` is still its English";
    let core = i18n::lint::missing(resources, &[], Extras::NONE).join("\n");
    assert!(core.contains(still), "{core}");
    let extras = Extras {
        standard_ids: &["place-"],
        ..Extras::NONE
    };
    let problems = i18n::lint::missing(resources, &[], extras).join("\n");
    assert!(!problems.contains(still), "{problems}");
}

/// A client's messages (`Extras::client_ids`) may choose each platform's
/// word (`$platform`, which the client passes them); no other message
/// may, since nothing passes it there.
#[test]
fn only_a_clients_messages_choose_by_platform() {
    let resources: &[i18n::Resource] = &[
        ("en", "# a\nclient-sign-in = Sign in\n# b\nlogin-submit = Sign in\n"),
        ("es", "client-sign-in = { $platform ->\n    [ios] Iniciar sesión\n   *[android] Acceder\n}\nlogin-submit = { $platform ->\n    [ios] Iniciar sesión\n   *[android] Acceder\n}\n"),
    ];
    let core = i18n::lint::check(resources, &[], Extras::NONE).join("\n");
    assert!(
        core.contains("es: `client-sign-in` uses {\"platform\"}"),
        "{core}"
    );
    let extras = Extras {
        client_ids: &["client-"],
        ..Extras::NONE
    };
    let problems = i18n::lint::check(resources, &[], extras).join("\n");
    assert!(!problems.contains("`client-sign-in`"), "{problems}");
    assert!(
        problems.contains("es: `login-submit` uses {\"platform\"}, English {}"),
        "{problems}"
    );
}

/// A variable English's note offers as spare (`spare: $hour`) a
/// translation may choose on or leave out; one English shows is refused,
/// one no note offers is refused, and a choice on a spare variable picks
/// by its value, never as a plural.
#[test]
fn a_translation_may_use_what_englishs_note_spares() {
    let resources: &[i18n::Resource] = &[
        (
            "en",
            concat!(
                "# At $time.\n# spare: $hour, the hour $time shows.\nat = at { $time }\n",
                "# At $time, again.\nbare = at { $time }\n",
                "# Shows it.\n# spare: $hour, the hour.\nshown = { $hour }:{ $time }\n",
            ),
        ),
        (
            "es",
            concat!(
                "at = a { $hour ->\n    [1] la\n   *[other] las\n} { $time }\n",
                "bare = a { $hour ->\n    [1] la\n   *[other] las\n} { $time }\n",
                "shown = { $hour }:{ $time }\n",
            ),
        ),
        ("es-ES", "at = a las { $time }\n"),
    ];
    let problems = i18n::lint::check(resources, &[], Extras::NONE).join("\n");
    assert!(!problems.contains("`at`"), "{problems}");
    for expected in [
        "es: `bare` uses {\"hour\", \"time\"}, English {\"time\"}",
        "en: `shown` shows $hour, which its note calls spare",
    ] {
        assert!(problems.contains(expected), "{expected} in:\n{problems}");
    }
    let plural = i18n::lint::plural_problems(resources).join("\n");
    assert!(!plural.contains("`at`"), "{plural}");
    assert!(plural.contains("es: `bare` chooses among"), "{plural}");
}

/// The hour a time of day shows, as the code passes it beside the time.
#[test]
fn a_times_hour_is_the_hour_it_shows() {
    use i18n::clock_hour;
    assert_eq!(clock_hour("1:05 p. m."), 1);
    assert_eq!(clock_hour("13:05"), 13);
    assert_eq!(clock_hour("01:05"), 1);
    assert_eq!(clock_hour("12:00 AM"), 12);
    assert_eq!(clock_hour("\u{0661}:05"), -1);
    assert_eq!(clock_hour(""), -1);
}

#[test]
fn a_deployments_names_are_kept_in_translation() {
    let resources: &[i18n::Resource] = &[
        ("en", "# where\npay = Pay with Acme\n"),
        ("es", "pay = Paga con tarjeta\n"),
    ];
    let warning = "es: `pay` names other products or services than English";
    let core = i18n::lint::text::text_problems(resources, Extras::NONE).join("\n");
    assert!(!core.contains(warning), "{core}");
    let extras = Extras {
        names: &["Acme"],
        ..Extras::NONE
    };
    let hosted = i18n::lint::text::text_problems(resources, extras).join("\n");
    assert!(hosted.contains(warning), "{hosted}");
}

/// What a message's text may hold: each rule fires on what it is for,
/// and stays quiet on what is allowed.
#[test]
fn a_messages_text_keeps_to_what_every_reader_can_read() {
    let resources: &[i18n::Resource] = &[
        ("en", concat!(
            "# where; no word on the variable\nnoted = Hi { $name }\n",
            "# where. $count is the cards.\nok = { $count } cards\n",
            "# where\narrow = Settings → Security\n",
            "# where\nmark = a\u{200F}b\n",
            "# where\ndash = one \u{2014} two\n",
            "# where\ndash-entity-html = one ", "&", "mdash;", " two\n",
            "# where\nentity = Tom &amp; Jerry\n",
            "# where\nlink-html = Go <a href=\"/a\" class=\"x\">here</a> and <span id=\"n\">x</span>.\n",
            "# where, max 5\ncapped = Save\n",
            "# where\nlong = Email\n",
            "# where\nnamed = Use Claude\n",
            "# where\npath = Settings › Security\n",
            "# where\nnamed-bare = Use Claude\n",
            "# where\ntight = Hi\n",
        )),
        ("es", concat!(
            "noted = Hola { $name }\n",
            "ok = { $count } tarjetas\n",
            "arrow = Ajustes › Seguridad\n",
            "mark = ab\n",
            "dash = uno, dos\n",
            "entity = Tom y Jerry\n",
            "link-html = Ve <a href=\"/b\" class=\"x\">aquí</a> y <span id=\"n\">x</span> y <span id=\"n\">y</span>.\n",
            "capped = Guardarlo\n",
            "# accept: length: Spanish says email in two words.\nlong = Correo electrónico\n",
            "named = Usa Claude o ChatGPT\n",
            "# accept: names: \npath = Ajustes › Seguridad\n",
            "# accept: names:\nnamed-bare = Usa Claude o Gemini\n",
            "tight =Hola\n",
        )),
        ("es-ES", "# accept: length: no reason needed here, as nothing is long\nok = { $count } tarjetas\n# accept: tone: wrong kind\nentity = Tom y Jerry\n"),
    ];
    let problems = i18n::lint::text::text_problems(resources, Extras::NONE).join("\n");
    for expected in [
        "en: `noted`'s note does not say what $name is",
        "en: `arrow`: \"\u{2192}\" keeps pointing the same way",
        "en: `mark`: a direction mark",
        "en: `dash`: an em dash",
        "en: `dash-entity-html`: an em dash",
        "en: `entity`: &amp; in a message shown as text",
        "es: `link-html`'s markup is not English's",
        "es: `capped` is 9 characters, over the note's max 5",
        "es: `named` names other products or services than English",
        "es: `path` accepts names but raises no such warning",
        "es-ES: `ok` accepts length but raises no such warning",
        "es-ES: `entity` accepts \"tone\", which no warning is called",
        "es: `named-bare`'s accept gives no reason",
        "es: `tight` is written `tight =`; write `tight = ` and its text",
    ] {
        assert!(problems.contains(expected), "{expected} in:\n{problems}");
    }
    for quiet in [
        "en: `ok`",
        "es: `arrow`",
        "en: `path`",
        "es: `long`",
        "en: `link-html`",
    ] {
        assert!(!problems.contains(quiet), "{quiet} in:\n{problems}");
    }
}

/// Each pack is for a folder a language reads, once.
#[test]
fn the_packs_are_for_the_tables_folders() {
    let problems = i18n::lint::pack_problems();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The Spanish packs catch what they are for: a Spain word in Latin
/// America's text and the reverse, the words neither writes, an
/// unpaired question mark, an error that exclaims, a failure that does
/// not start with "No se", a gendered slash, "por favor", and an
/// untranslated message.
#[test]
fn the_spanish_rules_catch_what_they_are_for() {
    let resources: &[i18n::Resource] = &[
        ("en", "# a\nok-a = Add\n# b\nsave-failed = Save failed.\n# c\nleft = Left as is\n# d\nask = Sure?\n"),
        ("es", "ok-a = Añadir mazo\nsave-failed = ¡Error al guardar!\nleft = Left as is\nask = ¿Seguro? Bienvenido/a, por favor\nextra = 3 fichas\n"),
        ("es-ES", "ask = Seguro? Agrega el video\n"),
    ];
    let style = i18n::lint::style(resources, Extras::NONE).join("\n");
    for expected in [
        "es-419: `ok-a`: \"añadir\"",
        "es-ES: `ask`: \"agrega\"",
        "es-ES: `ask`: \"video\"",
        "es-ES: `ask`: ¿ and ? do not pair",
        "es-419: `save-failed`: an error does not exclaim",
        "es-419: `save-failed`: a failure starts with \"No se\"",
        "es-419: `ask`: \"o/a\"",
        "es-419: `ask`: \"por favor\"",
        "es-419: `extra`: \"fichas\"",
    ] {
        assert!(style.contains(expected), "{expected} in:\n{style}");
    }
    assert!(
        !style.contains("es-ES: `ok-a`"),
        "Spain says añadir:\n{style}"
    );
    let chosen: &[i18n::Resource] = &[
        ("en", "# a\nupload-failed = Upload failed.\n"),
        ("es", "upload-failed = { $platform ->\n    [ios] No se pudo cargar.\n   *[android] No se pudo subir.\n}\n"),
    ];
    let style = i18n::lint::style(chosen, Extras::NONE).join("\n");
    assert!(!style.contains("es-419: `upload-failed`"), "{style}");
    let missing = i18n::lint::missing(resources, &[], Extras::NONE).join("\n");
    assert!(missing.contains("`left` is still its English"), "{missing}");
}

/// A pseudo-language turns the catalog's words and nothing else: a
/// person's text set into a message, markup, entities and numbers pass
/// as they are, and a number still picks the message's plural.
#[test]
fn a_pseudo_language_turns_only_the_catalogs_words() {
    let pseudo = |how| UiLocale::all().find(|l| l.pseudo() == Some(how)).unwrap();
    let (accented, bidi) = (
        pseudo(flash_core::locale::Pseudo::Accented),
        pseudo(flash_core::locale::Pseudo::Bidi),
    );
    let c = Arc::new(
        Catalog::new(&[(
            "en",
            concat!(
                "# a\nhi = Hi { $name }\n",
                "# a\nlink-html = See <a href=\"/a\">it</a> &amp; { $name }\n",
                "# a\ncards = { $count ->\n    [one] { $count } card\n   *[other] { $count } cards\n}\n",
            ),
        )])
        .shipping(&[accented, bidi])
        .unwrap(),
    );
    let tr = i18n::Tr::new(c.clone(), accented, None);
    assert_eq!(tr.args("hi", &[("name", "Ana".into())]), "[Ħīī Ana]");
    assert_eq!(
        tr.args("link-html", &[("name", "<b>".into())]),
        "[Şḗḗḗḗ <a href=\"/a\">īīŧ</a> &amp; &#60;b&#62;]"
    );
    assert_eq!(tr.args("cards", &[("count", 1.into())]), "[1 ƈȧȧřḓ]");
    assert_eq!(tr.args("cards", &[("count", 21.into())]), "[21 ƈȧȧřḓş]");
    let tr = i18n::Tr::new(c, bidi, None);
    assert_eq!(
        tr.args("cards", &[("count", 21.into())]),
        "21 \u{200F}\u{202E}cards\u{202C}\u{200F}"
    );
    assert!(UiLocale::real().all(|l| l.pseudo().is_none()));
}

/// A person's words set into a sentence keep their direction to
/// themselves where it could pull the sentence: every value for a
/// right-to-left reader, a right-to-left value for anyone, and none of
/// an English reader's English. A value inside a tag (an address) is
/// never isolated, and a number never is.
#[test]
fn a_value_is_isolated_where_its_direction_could_pull_the_sentence() {
    use flash_server::i18n::bidi::{FSI, PDI};
    let rtl = UiLocale::all()
        .find(|l| l.dir() == flash_core::locale::Dir::Rtl)
        .expect("the table has a right-to-left language");
    let c = Arc::new(
        Catalog::new(&[(
            "en",
            concat!(
                "# a\nhi = Hi { $name }\n",
                "# a\nmail-html = Write to <a href=\"mailto:{ $email }\">{ $email }</a> or { $name }\n",
                "# a\ncards = { $count } cards\n",
            ),
        )])
        .shipping(&[rtl])
        .unwrap(),
    );
    let english = i18n::Tr::new(c.clone(), UiLocale::EN, None);
    assert_eq!(english.args("hi", &[("name", "Ana".into())]), "Hi Ana");
    assert_eq!(
        english.args("hi", &[("name", "שלום".into())]),
        format!("Hi {FSI}שלום{PDI}")
    );
    assert_eq!(
        english.args(
            "mail-html",
            &[("email", "a@b.co".into()), ("name", "Ana".into())]
        ),
        "Write to <a href=\"mailto:a@b.co\">a@b.co</a> or Ana"
    );
    assert_eq!(
        english.args(
            "mail-html",
            &[("email", "a@b.co".into()), ("name", "<שלום>".into())]
        ),
        format!("Write to <a href=\"mailto:a@b.co\">a@b.co</a> or {FSI}&#60;שלום&#62;{PDI}")
    );
    let reader = i18n::Tr::new(c, rtl, None);
    let mail = reader.args(
        "mail-html",
        &[("email", "a@b.co".into()), ("name", "Ana".into())],
    );
    assert!(
        mail.contains(&format!("href=\"mailto:a@b.co\">{FSI}a@b.co{PDI}</a>")),
        "{mail}"
    );
    assert!(mail.contains(&format!("{FSI}Ana{PDI}")), "{mail}");
    let cards = reader.args("cards", &[("count", 21.into())]);
    assert!(!cards.contains(FSI), "{cards}");
}

/// A language ships whole or not at all: one lacking a word for an
/// English message is refused by name, the messages of pages written in
/// English only aside; a variant is whole through its base.
#[test]
fn a_language_ships_whole_or_not_at_all() {
    let resources: &[i18n::Resource] = &[
        ("en", "a = A\nb = B\nself-host-x = Self\n"),
        ("es", "a = A es\n"),
        ("es-ES", "b = B es\n"),
    ];
    let refused = Catalog::new(resources)
        .shipping(&[latin_america()])
        .err()
        .expect("Latin America lacks b");
    assert!(refused.contains("es-419 lacks b, self-host-x"), "{refused}");
    let refused = Catalog::new(resources)
        .english_only(&["self-host"])
        .shipping(&[latin_america(), spain()])
        .err()
        .expect("Latin America still lacks b");
    assert!(refused.contains("es-419 lacks b"), "{refused}");
    assert!(!refused.contains("self-host"), "{refused}");
    assert!(
        !refused.contains("es-ES"),
        "Spain has b in its own folder: {refused}"
    );
    assert!(Catalog::new(resources)
        .english_only(&["self-host"])
        .shipping(&[spain()])
        .is_ok());
}

/// Text written outside the catalogs is read as its reader sees it and
/// held to the messages' rules: each language that reads its folder, a
/// line at a time; a page's quoted label in its own language's.
#[test]
fn prose_keeps_to_the_messages_rules() {
    let page = concat!(
        "{# a note about móvil #}<h1 title=\"Una ficha\">Tu mazo</h1>\n",
        "<p>Abre <strong>Archivo &rarr; Importar</strong> y añade</p>",
        "<p>la tarjeta ",
        "&",
        "mdash; ya. {{ \"x\"|t }}¿Listo?</p>",
        "<p>Pulsa <strong lang=\"es-ES\">Añadir</strong> o ",
        "<strong lang=\"es-419\">Agregar</strong>.</p>",
        "<p lang=\"xx-YY\">Hola</p>",
    );
    assert_eq!(
        i18n::lint::shown_html("<p>Uno <b>dos</b></p><p>tres&nbsp;&#8217;&#x2019;&bogus;</p>"),
        "Uno dos\ntres\u{A0}\u{2019}\u{2019}&bogus;"
    );
    let problems = i18n::lint::prose_html("es", "page-x", page, Extras::NONE).join("\n");
    for expected in [
        "es-419: `page-x`: \"añade\" (rule añad*)",
        "es-419: `page-x`: \"pulsa\" (rule puls*)",
        "es-ES: `page-x`: \"ficha\" (rule ficha)",
        "es: `page-x`: \"\u{2192}\" keeps pointing the same way",
        "es: `page-x`: an em dash",
        "`page-x`: lang=\"xx-YY\" is no language of the table",
    ] {
        assert!(problems.contains(expected), "{expected} in:\n{problems}");
    }
    for quiet in [
        "móvil",
        "es-419: `page-x`: \"añadir\"",
        "es-ES: `page-x`: \"agregar\"",
        "do not pair",
    ] {
        assert!(!problems.contains(quiet), "{quiet} in:\n{problems}");
    }
    let nobody = i18n::lint::prose("xx", "page-x", "Hola", Extras::NONE).join("\n");
    assert!(
        nobody.contains("no language reads the folder xx"),
        "{nobody}"
    );
}

/// Words that skip the catalog say why, everywhere in the core's code
/// (`lint::doors`); the check finds a door with no reason and lets one
/// with a reason pass.
#[test]
fn words_that_skip_the_catalog_say_why() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = i18n::lint::doors(&src);
    assert!(problems.is_empty(), "{}", problems.join("\n"));

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("doors");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.rs"),
        concat!(
            "fn a() {\n",
            "    let bare = Words::theirs(name);\n",
            "    // theirs: the deck's name.\n",
            "    let said = Words::theirs(name);\n",
            "    // for machines:\n",
            "    let empty = Words::for_machines(\"x\");\n",
            "}\n",
        ),
    )
    .unwrap();
    let problems = i18n::lint::doors(&dir).join("\n");
    assert!(problems.contains("a.rs:2: Words::theirs"), "{problems}");
    assert!(!problems.contains("a.rs:4:"), "{problems}");
    assert!(
        problems.contains("a.rs:6: Words::for_machines"),
        "{problems}"
    );
}

/// Every rule of every pack fires: each banned word (a word's start
/// written bare), phrase, gendered form and character in a message of the
/// pack's folder, each mark with the space it refuses before it, a « »
/// without its no-break spaces, and each glossary term left without its
/// word.
#[test]
fn every_pack_rule_fires() {
    for pack in i18n::packs::PACKS {
        let mut english = String::new();
        let mut base = String::new();
        let mut own = String::new();
        let mut expected = Vec::new();
        let words = pack.banned.iter().map(|r| r.trim_end_matches('*'));
        let phrases = pack.banned_phrases.iter().chain(pack.gendered).copied();
        let mut samples: Vec<(String, String)> = words
            .chain(phrases)
            .map(|text| (text.to_string(), format!("\"{text}\"")))
            .collect();
        for word in pack
            .outside_only
            .iter()
            .map(|(r, _)| r.trim_end_matches('*'))
        {
            samples.push((word.into(), format!("\"{word}\" inside the product")));
        }
        for (c, _) in pack.banned_chars {
            samples.push((format!("a{c}b"), format!("U+{:04X}", *c as u32)));
        }
        for mark in pack.nbsp_before {
            let wanted = format!("before \"{mark}\" write a no-break space");
            samples.push((format!("mot {mark}"), wanted.clone()));
            samples.push((format!("mot{mark}"), wanted));
        }
        for mark in pack.tight_before {
            samples.push((
                format!("mot\u{a0}{mark}"),
                format!("no space before \"{mark}\""),
            ));
        }
        if pack.nbsp_in_guillemets {
            samples.push(("\u{ab}mot\u{bb}".into(), "inside \u{ab} \u{bb}".into()));
        }
        for (i, (text, problem)) in samples.iter().enumerate() {
            english.push_str(&format!("# n\nr{i} = x\n"));
            base.push_str(&format!("r{i} = y\n"));
            own.push_str(&format!("r{i} = {text}\n"));
            expected.push(format!("`r{i}`: {problem}"));
        }
        let folder = pack.folder;
        let shared = flash_core::locale::LANGUAGES
            .iter()
            .find(|l| l.chain.contains(&folder))
            .map(|l| l.chain[0])
            .unwrap_or_else(|| panic!("{folder}: no language of the table reads it"));
        let resources: Vec<i18n::Resource> = if folder == shared {
            vec![("en", english.leak()), (folder, own.leak())]
        } else {
            vec![
                ("en", english.leak()),
                (shared, base.leak()),
                (folder, own.leak()),
            ]
        };
        let style = i18n::lint::style(&resources, Extras::NONE).join("\n");
        for expected in expected {
            assert!(
                style.contains(&expected),
                "{folder}: {expected} in:\n{style}"
            );
        }
        if pack.product_word.is_some() {
            let extras = Extras {
                outside_ids: &["outside-"],
                ..Extras::NONE
            };
            for (i, (outside, _)) in pack.outside_only.iter().enumerate() {
                let id = format!("outside-r{i}");
                let english = format!("# n\n{id} = x\n").leak();
                let own = format!("{id} = {}\n", outside.trim_end_matches('*')).leak();
                let style = i18n::lint::style(&[("en", english), (folder, own)], extras).join("\n");
                assert!(!style.contains(&format!("`{id}`")), "{folder}: {style}");
            }
        }

        for (term, word) in pack.glossary {
            let english = format!("# n\nlacks = Your {term}\n# n\nhas = Your {term}\n").leak();
            let own = format!("lacks = zzz\nhas = Tu {word}\n").leak();
            let problems =
                i18n::lint::text::text_problems(&[("en", english), (folder, own)], Extras::NONE);
            let problems = problems.join("\n");
            assert!(
                problems.contains("`lacks` lacks its language's word"),
                "{folder}: {term} in:\n{problems}"
            );
            assert!(
                !problems.contains("`has` lacks"),
                "{folder}: {word} in:\n{problems}"
            );
        }
    }
}

/// Every message a page's script says keeps to what static/i18n.js can
/// word: text, variables and choices.
#[test]
fn script_messages_keep_to_what_i18n_js_words() {
    let problems = i18n::lint::script_problems(i18n::CORE, Extras::NONE);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every file under `locales/` is one the catalog reads.
#[test]
fn every_message_file_is_read() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
    let mut on_disk = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap() {
        let dir = dir.unwrap().path();
        for file in std::fs::read_dir(&dir).unwrap() {
            let file = file.unwrap().path();
            let text = std::fs::read_to_string(&file).unwrap();
            on_disk.push((dir.file_name().unwrap().to_string_lossy().to_string(), text));
        }
    }
    for (dir, text) in &on_disk {
        assert!(
            i18n::CORE.iter().any(|(d, t)| d == dir && t == text),
            "a file in locales/{dir} is not in i18n::CORE"
        );
    }
    assert_eq!(on_disk.len(), i18n::CORE.len());
}

fn catalog() -> Arc<Catalog> {
    Arc::new(
        Catalog::new(i18n::CORE)
            .shipping(&[spain(), latin_america()])
            .unwrap(),
    )
}

fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for (k, v) in pairs {
        h.append(*k, HeaderValue::from_static(v));
    }
    h
}

/// The account's chosen language, then the one it holds, then a `?hl=`,
/// then the visitor's own choice, then the browser; never a guess.
#[test]
fn a_request_is_read_in_the_language_its_person_asked_for() {
    let c = catalog();
    let lang = |h: &HeaderMap, q: Option<&str>, account: Option<AccountLang>| {
        i18n::reader(&c, h, q, account.as_ref()).html_lang()
    };
    let mexico = headers(&[("accept-language", "es-MX,es;q=0.9,en;q=0.8")]);
    assert_eq!(lang(&mexico, None, None), "es-419");
    assert_eq!(
        lang(&headers(&[("accept-language", "es-ES")]), None, None),
        "es-ES"
    );
    assert_eq!(
        lang(&headers(&[("accept-language", "de-DE,it")]), None, None),
        "en"
    );
    assert_eq!(lang(&headers(&[]), None, None), "en");
    assert_eq!(
        lang(&headers(&[("accept-language", "ca")]), None, None),
        "es-ES"
    );
    let spain_zone = headers(&[
        ("accept-language", "es"),
        ("cookie", "flash-tz=Europe/Madrid"),
    ]);
    assert_eq!(lang(&spain_zone, None, None), "es-ES");
    assert_eq!(
        lang(&headers(&[("accept-language", "es")]), None, None),
        "es-419"
    );
    let chose = headers(&[("accept-language", "es-MX"), ("cookie", "flash-lang=en")]);
    assert_eq!(lang(&chose, None, None), "es-419");
    assert_eq!(lang(&chose, Some("next=/x&hl=es-ES"), None), "es-ES");
    let account = AccountLang {
        locale: Some("es-AR".into()),
        chosen: false,
        zone: None,
    };
    assert_eq!(lang(&chose, Some("hl=en"), Some(account)), "es-419");
    let portuguese = AccountLang {
        locale: Some("pt-BR".into()),
        chosen: false,
        zone: None,
    };
    assert_eq!(lang(&mexico, None, Some(portuguese)), "es-419");
    assert_eq!(i18n::reader(&c, &mexico, None, None).format_tag(), "es-MX");
}

/// A language not shipped yet answers in English, whatever was asked.
#[test]
fn an_unshipped_language_reads_english() {
    let c = Arc::new(Catalog::new(i18n::CORE));
    let h = headers(&[("accept-language", "es-MX")]);
    assert_eq!(i18n::reader(&c, &h, None, None).html_lang(), "en");
}

/// The languages this server gives readers decide, never the table: a
/// source naming one it does not give falls to a shipped variant of the
/// same language, else to the next source, never straight to English.
/// The browser's list is read as one list.
#[test]
fn only_what_the_server_ships_answers() {
    let c = Arc::new(
        Catalog::new(i18n::CORE)
            .shipping(&[latin_america()])
            .unwrap(),
    );
    let lang = |h: &HeaderMap, q: Option<&str>, account: Option<AccountLang>| {
        i18n::reader(&c, h, q, account.as_ref()).html_lang()
    };
    let none = headers(&[]);
    let chose_spain = AccountLang {
        locale: Some("es-ES".into()),
        chosen: true,
        zone: None,
    };
    assert_eq!(lang(&none, None, Some(chose_spain)), "es-419");
    let catalan = AccountLang {
        locale: Some("ca".into()),
        chosen: false,
        zone: None,
    };
    let mexico = headers(&[("accept-language", "es-MX")]);
    assert_eq!(lang(&mexico, None, Some(catalan)), "es-419");
    let english = Arc::new(Catalog::new(i18n::CORE));
    let h = headers(&[("accept-language", "es-ES, en-GB")]);
    assert_eq!(i18n::reader(&english, &h, None, None).html_lang(), "en");
    let both = catalog();
    let h = headers(&[("accept-language", "ca, es-MX")]);
    assert_eq!(i18n::reader(&both, &h, None, None).html_lang(), "es-419");
}

/// A message reads in its reader's language and falls back to English,
/// then to its id; a number in it is written as the reader's region
/// writes it.
#[test]
fn messages_read_in_their_readers_language() {
    let c = catalog();
    let tr = i18n::Tr::new(c.clone(), spain(), None);
    assert_eq!(tr.t("language"), "Idioma");
    let en = i18n::Tr::english(c);
    assert_eq!(en.t("language"), "Language");
    assert_eq!(tr.t("no-such-message"), "no-such-message");
    assert!(tr
        .catalog()
        .missing()
        .contains(&"no-such-message".to_string()));

    // Numbers as each region writes them (CLDR); English as its pages
    // always have.
    let c = Arc::new(
        Catalog::new(&[
            ("en", "# a count\ncards = { $count } cards\n"),
            ("es", "cards = { $count } tarjetas\n"),
        ])
        .shipping(&[spain(), latin_america()])
        .unwrap(),
    );
    let n = |tr: i18n::Tr| tr.args("cards", &[("count", i18n::Value::from(12345))]);
    assert_eq!(n(i18n::Tr::english(c.clone())), "12345 cards");
    assert_eq!(
        n(i18n::Tr::new(c.clone(), spain(), Some("es-ES"))),
        "12.345 tarjetas"
    );
    assert_eq!(
        n(i18n::Tr::new(c.clone(), latin_america(), Some("es-MX"))),
        "12,345 tarjetas"
    );
    assert_eq!(
        n(i18n::Tr::new(c.clone(), latin_america(), Some("es-AR"))),
        "12.345 tarjetas"
    );
    assert_eq!(
        i18n::Tr::new(c, spain(), Some("es-ES"))
            .formats()
            .unwrap()
            .integer(1234),
        "1234"
    );
}

/// A page the server words picks a plural form by current CLDR (the
/// patched intl_pluralrules), as a browser and a phone do: Spanish says
/// "de" after an exact million, a form CLDR 36's rules never chose.
#[test]
fn the_server_picks_plural_forms_by_current_cldr() {
    let c = Arc::new(
        Catalog::new(&[
            ("en", "# a count\ncards = { $count ->\n    [one] { $count } card\n   *[other] { $count } cards\n}\n"),
            ("es", "cards = { $count ->\n    [one] { $count } tarjeta\n    [many] { $count } de tarjetas\n   *[other] { $count } tarjetas\n}\n"),
        ])
        .shipping(&[spain()]).unwrap(),
    );
    let es = i18n::Tr::new(c, spain(), Some("es-ES"));
    let n = |count: i64| es.args("cards", &[("count", i18n::Value::from(count))]);
    assert_eq!(n(1), "1 tarjeta");
    assert_eq!(n(2), "2 tarjetas");
    assert_eq!(n(1_000_000), "1.000.000 de tarjetas");
    assert_eq!(n(1_000_001), "1.000.001 tarjetas");
}

/// A count of one reads in the singular, and the words around it agree:
/// a whole number from a template, and the float an import's note
/// carries.
#[test]
fn a_count_of_one_reads_in_the_singular() {
    let c = catalog();
    let en = i18n::Tr::english(c.clone());
    let es = i18n::Tr::new(c, latin_america(), Some("es-MX"));
    let one = |tr: &i18n::Tr, id: &str| tr.args(id, &[("count", i18n::Value::from(1))]);
    assert_eq!(one(&en, "import-submit"), "Import 1 card");
    assert_eq!(one(&es, "import-submit"), "Importar 1 tarjeta");
    assert_eq!(one(&es, "import-added"), "1 tarjeta importada.");
    assert_eq!(one(&es, "import-skipped"), "1 fila omitida");
    assert_eq!(one(&en, "settings-cards-count"), "1 card.");
    assert_eq!(
        one(&es, "today-more-new-boosted"),
        "Más tarjetas nuevas hoy (ya va +1):"
    );
    assert_eq!(
        es.args(
            "import-media-unsupported",
            &[("count", i18n::Value::from(1.0))]
        ),
        "1 archivo no admitido"
    );
    let cloze = |tr: &i18n::Tr, notes: f64, cards: f64| {
        tr.args(
            "import-cloze-expanded",
            &[
                ("notes", i18n::Value::from(notes)),
                ("cards", i18n::Value::from(cards)),
            ],
        )
    };
    assert_eq!(cloze(&en, 1.0, 1.0), "1 cloze note became 1 card");
    assert_eq!(cloze(&en, 1.0, 3.0), "1 cloze note became 3 cards");
    assert_eq!(
        cloze(&es, 2.0, 5.0),
        "2 notas de respuesta anidada se convirtieron en 5 tarjetas"
    );
    assert_eq!(
        cloze(&es, 1.0, 1.0),
        "1 nota de respuesta anidada se convirtió en 1 tarjeta"
    );
}

/// Work for one account (an email) speaks that account's language,
/// whatever request it runs in.
#[tokio::test]
async fn scoped_work_speaks_its_own_readers_language() {
    let c = catalog();
    let spain = i18n::Tr::new(c.clone(), spain(), None);
    assert_eq!(i18n::t("language"), "Language");
    assert_eq!(i18n::scoped(spain, || i18n::t("language")), "Idioma");
    assert_eq!(i18n::t("language"), "Language");
}

/// Scoped work that panics (a mail that fails to render, caught by the
/// server's panic layer) still puts the thread's reader back, so the
/// next request served on that thread is not read in its language.
#[test]
fn scoped_work_that_panics_puts_the_reader_back() {
    let spain = i18n::Tr::new(catalog(), spain(), None);
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        i18n::scoped(spain, || panic!("a mail failed to render"))
    }));
    assert!(caught.is_err());
    assert!(i18n::current().is_none());
    assert_eq!(i18n::t("language"), "Language");
}

/// A region a browser names is remembered for only so many readers:
/// past that, readers of the rest read their row's own numbers, so
/// headers naming a new region each cannot grow memory without end.
#[test]
fn the_regions_readers_come_with_are_bounded() {
    let c = Arc::new(
        Catalog::new(&[
            ("en", "language = Language\n"),
            ("es", "language = Idioma\n"),
        ])
        .shipping(&[latin_america()])
        .unwrap(),
    );
    let mut tags = std::collections::BTreeSet::new();
    for a in 'A'..='Z' {
        for b in 'A'..='Z' {
            let tr = i18n::Tr::new(c.clone(), latin_america(), Some(&format!("es-{a}{b}")));
            tags.insert(tr.format_tag().to_string());
        }
    }
    assert!(tags.len() <= 257, "{} region tags kept", tags.len());
    assert!(tags.contains("es-419"));
    assert_eq!(
        i18n::Tr::new(c, latin_america(), Some("es-AA")).format_tag(),
        "es-AA"
    );
}

/// A reader whose browser names no region, or a region their row lists,
/// reads the row's own plural forms and numbers: Portugal's for a reader
/// of Portugal's Portuguese whose browser says "pt" or "pt-AO" (Brazil's
/// would say "0 cartão"), Spain's for a Basque reader of Spain's Spanish.
#[test]
fn a_reader_counts_as_their_row_does() {
    let portugal = UiLocale::by_tag("pt-PT").unwrap();
    let c = Arc::new(
        Catalog::new(&[
            ("en", "# a count\ncards = { $count ->\n    [one] { $count } card\n   *[other] { $count } cards\n}\n"),
            ("pt", "cards = { $count ->\n    [one] { $count } cartão\n   *[other] { $count } cartões\n}\n"),
            ("es", "cards = { $count ->\n    [one] { $count } tarjeta\n    [many] { $count } de tarjetas\n   *[other] { $count } tarjetas\n}\n"),
        ])
        .shipping(&[portugal, spain()])
        .unwrap(),
    );
    let n = |tr: &i18n::Tr, count: i64| tr.args("cards", &[("count", i18n::Value::from(count))]);
    let row = i18n::Tr::new(c.clone(), portugal, Some("pt-PT"));
    assert_eq!(n(&row, 0), "0 cartões");
    let lisbon = headers(&[
        ("accept-language", "pt"),
        ("cookie", "flash-tz=Europe/Lisbon"),
    ]);
    let pt = i18n::reader(&c, &lisbon, None, None);
    assert_eq!(pt.html_lang(), "pt-PT");
    let angola = i18n::Tr::new(c.clone(), portugal, Some("pt-AO"));
    for tr in [&pt, &angola] {
        assert_eq!(n(tr, 0), "0 cartões");
        assert_eq!(n(tr, 12345), n(&row, 12345));
    }
    let basque = i18n::Tr::new(c, spain(), Some("eu"));
    assert_eq!(n(&basque, 1_000_000), "1.000.000 de tarjetas");
}

async fn page(app: &axum::Router, path: &str, extra: &[(&str, &str)]) -> Reply {
    let mut req = Request::get(path);
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    send(app, req.body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn a_page_says_its_language_and_what_chose_it() {
    let t = AppBuilder::new("i18n-pages")
        .languages(&[spain(), latin_america()])
        .build();
    let r = page(&t.app, "/login", &[("accept-language", "es-MX,es;q=0.9")]).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.text().contains(r#"<html lang="es-419""#), "{}", r.text());
    assert_eq!(r.header("content-language"), Some("es-419"));
    assert!(r
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("vary") && v.contains("Accept-Language")));
    assert!(!r.text().contains("Español (España)"), "{}", r.text());

    let english = AppBuilder::new("i18n-pages-en").build();
    let r = page(&english.app, "/login", &[("accept-language", "es-MX")]).await;
    assert!(r.text().contains(r#"<html lang="en""#));
}

#[tokio::test]
async fn an_account_chooses_its_language_in_settings() {
    let t = AppBuilder::new("i18n-settings")
        .languages(&[spain(), latin_america()])
        .build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let r = page(&t.app, "/settings", &[("cookie", cookie.as_str())]).await;
    assert!(
        r.text().contains(r#"action="/settings/language""#),
        "{}",
        r.text()
    );

    let r = send(
        &t.app,
        form_post("/settings/language", Some(&cookie), "language=es-ES"),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(
        t.store.account_locale(user).unwrap(),
        Some(("es-ES".into(), flash_store::LocaleSource::Chosen))
    );
    let r = page(
        &t.app,
        "/settings",
        &[("cookie", cookie.as_str()), ("accept-language", "en-US")],
    )
    .await;
    assert!(r.text().contains(r#"<html lang="es-ES""#));

    let r = send(
        &t.app,
        form_post("/settings/language", Some(&cookie), "language="),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(t.store.account_locale(user).unwrap(), None);
}

/// The Language card lists the browser's own languages first, as the
/// server answers them, and the rest by name; so does the signed-out
/// switch.
#[tokio::test]
async fn the_language_card_lists_the_browsers_own_first() {
    let t = AppBuilder::new("i18n-picker")
        .languages(&[spain(), latin_america()])
        .build();
    let (_, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let order = |text: &str, from: &str| -> Vec<String> {
        let start = text.find(from).expect("the picker is on the page");
        text[start..]
            .split(r#"value=""#)
            .skip(1)
            .map(|rest| rest[..rest.find('"').unwrap()].to_string())
            .filter(|v| !v.is_empty())
            .take(3)
            .collect()
    };
    let r = page(
        &t.app,
        "/settings",
        &[
            ("cookie", cookie.as_str()),
            ("accept-language", "es-MX,en;q=0.8"),
        ],
    )
    .await;
    assert_eq!(
        order(&r.text(), r#"name="language""#),
        ["es-419", "en", "es-ES"],
        "{}",
        r.text()
    );
}
