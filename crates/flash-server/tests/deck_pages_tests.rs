//! The deck pages over HTTP: the never-gated export downloads, deck
//! deletion, the settings page with its limits and rename, and the study
//! boosts. A plan cap belongs to a downstream extension and is tested
//! there.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_server::service::now_ms;
use flash_store::Store;

struct Harness {
    app: axum::Router,
    store: Arc<Store>,
    user: flash_core::UserId,
    session_cookie: String,
}

fn harness() -> Harness {
    // One data directory per test: an export is a file in it for as long
    // as the download runs, and a parallel test's fresh harness wipes its
    // own directory on construction.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let t = AppBuilder::new(&format!("deck-pages-{n}")).build();
    let (user, session_cookie) = signed_in(&t.store, "Cap", "cap@example.com");
    Harness {
        app: t.app,
        store: t.store,
        user,
        session_cookie,
    }
}

fn seed_cards(h: &Harness, n: usize) -> flash_core::DeckId {
    let deck = h.store.create_deck(h.user, "Seed", "", now_ms()).unwrap();
    let cards: Vec<_> = (0..n)
        .map(|i| {
            (
                flash_core::validate_card_text(&format!("f{i}"), "b").unwrap(),
                Vec::new(),
            )
        })
        .collect();
    h.store
        .create_cards(h.user, deck, &cards, None, now_ms())
        .unwrap();
    deck
}

/// Whether the page shows a deck's row as it loads. Every row is in the
/// page either way (showing archived decks and folding are the page's
/// script's to do, with no request), so what the server decides is which
/// rows start `hidden`.
fn shows(page: &str, deck: impl std::fmt::Display) -> bool {
    let from = page
        .find(&format!("<tr data-deck=\"{deck}\""))
        .unwrap_or_else(|| panic!("no row for deck {deck}: {page}"));
    let tag = &page[from..from + page[from..].find('>').unwrap()];
    !tag.ends_with(" hidden")
}

async fn request(h: &Harness, req: Request<Body>) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let r = send(&h.app, req).await;
    (r.status, r.headers, r.bytes)
}

#[tokio::test]
async fn export_endpoints_download_every_card() {
    let h = harness();
    seed_cards(&h, 12);

    let (status, headers, bytes) = request(
        &h,
        Request::get("/export.apkg")
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(header_of(&headers, "content-disposition")
        .unwrap()
        .contains("attachment; filename=\"flash-"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert!(zip.by_name("collection.anki2").is_ok());
    assert!(zip.by_name("media").is_ok());

    let (status, _, bytes) = request(
        &h,
        Request::get("/export.csv")
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let csv = String::from_utf8_lossy(&bytes);
    assert!(csv.starts_with("front,back,deck,tags"));
    assert_eq!(csv.lines().count(), 1 + 12);
}

#[tokio::test]
async fn export_requires_auth() {
    let h = harness();
    let (status, headers, _) = request(
        &h,
        Request::get("/export.apkg").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(header_of(&headers, "location")
        .unwrap()
        .starts_with("/login"));
}

#[tokio::test]
async fn deck_delete_button_removes_deck_and_redirects() {
    let h = harness();
    let deck = seed_cards(&h, 3);
    let (status, headers, _) = request(
        &h,
        Request::post(format!("/decks/{deck}/delete"))
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(header_of(&headers, "hx-redirect"), Some("/decks"));

    // The deck page is gone; an unknown id 404s the same way.
    let (status, _, _) = request(
        &h,
        Request::get(format!("/decks/{deck}"))
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = request(
        &h,
        Request::post("/decks/999999/delete")
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Archive from the settings page, see the deck leave the list, show it
/// again in its place with no study link, unarchive from its row, and see
/// it return.
#[tokio::test]
async fn archive_moves_the_deck_out_of_study_until_unarchived() {
    let h = harness();
    let deck = seed_cards(&h, 3);
    // A neighbour on each side of "Seed", so its place in the list shows.
    for name in ["Roots", "Trees"] {
        h.store.create_deck(h.user, name, "", now_ms()).unwrap();
    }
    let page = |path: String| {
        Request::get(path)
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap()
    };
    let post = |path: String, body: &'static str| {
        Request::post(path)
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();

    let (status, headers, _) = request(&h, post(format!("/decks/{deck}/archive"), "")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{deck}/settings?saved=archive"))
    );

    // The list hides the deck as it loads; its row is in the page all the
    // same, in its place, because showing it is the page's script's to do,
    // at once and with no request. The page head offers to.
    let (_, _, body) = request(&h, page("/decks".into())).await;
    let decks = text(body);
    assert!(decks.contains("action=\"/decks/archived\""), "{decks}");
    assert!(decks.contains("Show archived"), "{decks}");
    assert!(!shows(&decks, deck), "{decks}");
    assert!(decks.contains("data-show-archived=\"0\""), "{decks}");

    // Showing them is this browser's choice, kept in a cookie and not in
    // the address: the page is /decks either way, a reload keeps the
    // choice, and it holds until it is turned off. The script writes the
    // cookie itself, so it is one a script may write; the form that posts
    // without script sets the same one.
    let (_, _, body) = request(&h, page("/decks?archived=1".into())).await;
    assert!(!shows(&text(body), deck), "the address decides nothing");
    let (status, headers, _) = request(&h, post("/decks/archived".into(), "show=1")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(header_of(&headers, "location"), Some("/decks"));
    let cookie = header_of(&headers, "set-cookie").expect("the choice is remembered");
    assert!(cookie.starts_with("flash-decks-archived=1;"), "{cookie}");
    assert!(cookie.contains("Max-Age=31536000"), "{cookie}");
    assert!(!cookie.contains("HttpOnly"), "{cookie}");
    let showing = format!("{}; flash-decks-archived=1", h.session_cookie);
    let page_showing = |path: String| {
        Request::get(path)
            .header(header::COOKIE, showing.clone())
            .body(Body::empty())
            .unwrap()
    };
    let (_, _, body) = request(&h, page_showing("/decks".into())).await;
    let decks = text(body);
    assert!(decks.contains("Hide archived"), "{decks}");
    assert!(shows(&decks, deck), "{decks}");
    assert!(decks.contains("data-show-archived=\"1\""), "{decks}");
    let at = |needle: &str| {
        decks
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {decks}"))
    };
    assert!(at(">Roots<") < at(">Seed<") && at(">Seed<") < at(">Trees<"));
    assert!(at(">Seed<") < at(">(archived)<") && at(">(archived)<") < at(">Trees<"));
    assert_eq!(decks.matches("<table").count(), 1, "one list: {decks}");
    assert!(
        !decks.contains(&format!("/study?deck={deck}")),
        "no study link"
    );
    assert!(decks.contains(&format!("/decks/{deck}/unarchive")));
    let (_, _, body) = request(&h, page("/".into())).await;
    assert!(
        !text(body).contains("Seed"),
        "the dashboard shows today's decks only"
    );
    let (_, _, body) = request(&h, page(format!("/decks/{deck}"))).await;
    let deck_page = text(body);
    assert!(!deck_page.contains("Study this deck"));
    assert!(deck_page.contains(&format!("/decks/{deck}/unarchive")));
    let (_, _, body) = request(&h, page(format!("/decks/{deck}/settings"))).await;
    assert!(text(body).contains("Archived deck"));

    // Turning it off clears the cookie, and the deck is out of the list.
    let (_, headers, _) = request(&h, post("/decks/archived".into(), "show=0")).await;
    let cookie = header_of(&headers, "set-cookie").expect("the choice is forgotten");
    assert!(cookie.starts_with("flash-decks-archived=;"), "{cookie}");
    assert!(cookie.contains("Max-Age=0"), "{cookie}");

    // Unarchiving from its row returns to the list; with nothing left
    // archived the head no longer offers the choice, whatever the cookie
    // says, and the deck is back in study.
    let (status, headers, _) =
        request(&h, post(format!("/decks/{deck}/unarchive"), "back=decks")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(header_of(&headers, "location"), Some("/decks"));
    let (_, _, body) = request(&h, page_showing("/decks".into())).await;
    let decks = text(body);
    assert!(!decks.contains("/decks/archived"), "{decks}");
    assert!(!decks.contains("Hide archived"));
    assert!(decks.contains(&format!("/study?deck={deck}")));

    let (status, _, _) = request(&h, post("/decks/999999/archive".into(), "")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deck_settings_page_holds_limits_and_delete() {
    let h = harness();
    let deck = seed_cards(&h, 1);
    let get = |path: String| {
        Request::get(path)
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap()
    };
    let (status, _, body) = request(&h, get(format!("/decks/{deck}"))).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("Add a card"));
    assert!(html.contains(&format!("/decks/{deck}/settings")));
    assert!(!html.contains("Daily limits"));
    assert!(!html.contains("Delete deck"));

    let (status, _, body) = request(&h, get(format!("/decks/{deck}/settings"))).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(body).unwrap();
    assert!(html.contains("Daily limits"));
    assert!(html.contains("Delete deck"));
    assert!(html.contains(&format!("/decks/{deck}/rename")));

    let (status, _, _) = request(&h, get("/decks/999999/settings".into())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rename_deck_updates_name_and_rejects_bad_names() {
    let h = harness();
    let deck = seed_cards(&h, 1);
    h.store.create_deck(h.user, "Other", "", now_ms()).unwrap();
    let post = |name: &str| {
        Request::post(format!("/decks/{deck}/rename"))
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("name={name}")))
            .unwrap()
    };

    let (status, headers, _) = request(&h, post("Renamed")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{deck}/settings?saved=name"))
    );
    let (_, _, body) = request(
        &h,
        Request::get(format!("/decks/{deck}/settings"))
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(String::from_utf8(body)
        .unwrap()
        .contains("value=\"Renamed\""));

    let (status, headers, _) = request(&h, post("Other")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{deck}/settings?err=taken"))
    );

    let (status, headers, _) = request(&h, post("+++")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{deck}/settings?err=empty"))
    );

    let (status, _, _) = request(
        &h,
        Request::post("/decks/999999/rename")
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("name=x"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// A deck that has subdecks folds. Every deck starts open; a folded one
/// hides what is under it, keeps its counts, and stays folded in this
/// browser (a cookie, as "Show archived" is kept) until it is opened.
#[tokio::test]
async fn a_deck_with_subdecks_folds_and_the_fold_is_remembered() {
    let h = harness();
    let pharm = seed_cards(&h, 2);
    h.store
        .rename_deck(h.user, pharm, "Pharm", now_ms())
        .unwrap();
    let cardio = h
        .store
        .create_deck(h.user, "Pharm::Cardio::Arrhythmias", "", now_ms())
        .unwrap();
    let plain = h.store.create_deck(h.user, "Zebra", "", now_ms()).unwrap();
    let card = flash_core::validate_card_text("deep", "b").unwrap();
    h.store
        .create_cards(h.user, cardio, &[(card, Vec::new())], None, now_ms())
        .unwrap();
    let get = |path: &str, cookies: &str| {
        Request::get(path.to_string())
            .header(header::COOKIE, format!("{}{cookies}", h.session_cookie))
            .body(Body::empty())
            .unwrap()
    };
    let post = |path: String, body: String, cookies: &str| {
        Request::post(path)
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, format!("{}{cookies}", h.session_cookie))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();

    // All open to begin with: the whole tree shows, a deck with subdecks
    // has a fold that says it is open, and a deck without has none.
    let inner = h
        .store
        .find_deck_by_name(h.user, "Pharm::Cardio")
        .unwrap()
        .unwrap();
    let (_, _, body) = request(&h, get("/decks", "")).await;
    let decks = text(body);
    for deck in [pharm, inner, cardio, plain] {
        assert!(shows(&decks, deck), "{deck}: {decks}");
    }
    // The fold is a drawn chevron, not a character.
    assert!(decks.contains("<path d=\"m6 9 6 6 6-6\"/>"), "{decks}");
    assert!(decks.contains(&format!("action=\"/decks/{pharm}/fold\"")));
    assert!(!decks.contains(&format!("action=\"/decks/{plain}/fold\"")));
    assert!(!decks.contains("aria-expanded=\"false\""), "{decks}");

    // Folding it: the choice goes in a cookie and the page is /decks.
    let (status, headers, _) = request(
        &h,
        post(
            format!("/decks/{pharm}/fold"),
            "collapsed=1&back=decks".into(),
            "",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(header_of(&headers, "location"), Some("/decks"));
    let cookie = header_of(&headers, "set-cookie").expect("remembered");
    assert!(
        cookie.starts_with(&format!("flash-decks-folded={pharm};")),
        "{cookie}"
    );
    assert!(cookie.contains("Max-Age=31536000"), "{cookie}");
    assert!(
        !cookie.contains("HttpOnly"),
        "the page's script writes it too"
    );

    // With it, what is under the deck loads hidden (in the page, for the
    // script to show again at once), the deck still counts all three
    // cards, and its fold says it is shut. Today folds the same deck.
    let folded = format!("; flash-decks-folded={pharm}");
    for path in ["/decks", "/"] {
        let (_, _, body) = request(&h, get(path, &folded)).await;
        let page = text(body);
        assert!(shows(&page, pharm) && shows(&page, plain), "{path}: {page}");
        assert!(!shows(&page, inner), "{path}: {page}");
        assert!(!shows(&page, cardio), "{path}: {page}");
        assert!(page.contains("aria-expanded=\"false\""), "{path}: {page}");
        let row = &page[page.find(">Pharm</a>").unwrap()..page.find(">Zebra</a>").unwrap()];
        assert!(row.contains("<td class=\"num\">3</td>"), "{path}: {row}");
    }

    // A second fold joins the first; opening one leaves the other; opening
    // the last forgets the cookie. An id that is no deck of the caller's
    // is never kept.
    let fold = |deck: String, collapsed: &'static str, cookies: String| {
        let req = post(
            format!("/decks/{deck}/fold"),
            format!("collapsed={collapsed}&back=decks"),
            &cookies,
        );
        async {
            let (_, headers, _) = request(&h, req).await;
            header_of(&headers, "set-cookie").unwrap().to_string()
        }
    };
    let both = fold(inner.to_string(), "1", folded.clone()).await;
    assert!(
        both.starts_with(&format!("flash-decks-folded={pharm}.{inner};")),
        "{both}"
    );
    let stranger = fold("999999".into(), "1", folded.clone()).await;
    assert!(
        stranger.starts_with(&format!("flash-decks-folded={pharm};")),
        "{stranger}"
    );
    let none = fold(pharm.to_string(), "0", folded.clone()).await;
    assert!(none.starts_with("flash-decks-folded=;"), "{none}");
    assert!(none.contains("Max-Age=0"), "{none}");

    // A cookie someone wrote by hand is read for the ids in it and no more.
    let (status, _, body) = request(
        &h,
        get(
            "/decks",
            &format!("; flash-decks-folded=x.-4.{pharm}.99999999999999999999."),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!shows(&text(body), inner));

    // A deck whose only subdecks are archived and out of sight has a fold
    // that offers nothing until they are shown: it keeps its place, off.
    h.store
        .set_deck_archived(h.user, inner, Some(now_ms()))
        .unwrap();
    let (_, _, body) = request(&h, get("/decks", "")).await;
    let decks = text(body);
    assert!(decks.contains("class=\"deck-fold is-off\""), "{decks}");
    let (_, _, body) = request(&h, get("/decks", "; flash-decks-archived=1")).await;
    let decks = text(body);
    assert!(!decks.contains("class=\"deck-fold is-off\""), "{decks}");
    assert!(shows(&decks, inner) && shows(&decks, cardio), "{decks}");
}

/// Subdecks over the pages: made from a deck's page, listed under it with
/// their own name and the deck's counts holding theirs, moved with Parent
/// deck, and named in what archiving and deleting say they will take.
#[tokio::test]
async fn subdecks_are_made_listed_moved_and_counted_on_the_pages() {
    let h = harness();
    let pharm = seed_cards(&h, 2);
    h.store
        .rename_deck(h.user, pharm, "Pharm", now_ms())
        .unwrap();
    let page = |path: String| {
        Request::get(path)
            .header(header::COOKIE, h.session_cookie.clone())
            .body(Body::empty())
            .unwrap()
    };
    let post = |path: String, body: String| {
        Request::post(path)
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
    let id_of = |name: &str| h.store.find_deck_by_name(h.user, name).unwrap().unwrap();

    // The subdecks are a panel under the page head, above "Add a card",
    // opened and closed by a "Subdecks" button in the head, to the left of
    // Settings. It loads closed, whatever is in it, and nothing about it
    // is remembered. For a deck with none it offers to make one; making it
    // opens the new deck.
    let subdecks_button = |html: &str, label: &str| {
        let button = format!(
            "aria-expanded=\"false\" aria-controls=\"subdecks\" data-subdecks-toggle>{label}</a>"
        );
        let at = html
            .find(&button)
            .unwrap_or_else(|| panic!("{button}: {html}"));
        assert!(at < html.find(">Settings</a>").unwrap(), "left of Settings");
        let panel = html
            .find("<section class=\"deck-subdecks\" id=\"subdecks\" hidden>")
            .unwrap_or_else(|| panic!("no closed subdecks panel: {html}"));
        assert!(panel < html.find("Add a card").unwrap(), "above Add a card");
        assert!(!html.contains("<details class=\"fl-advanced deck-subdecks"));
    };
    let (_, _, body) = request(&h, page(format!("/decks/{pharm}"))).await;
    let deck_page = text(body);
    subdecks_button(&deck_page, "Subdecks");
    assert!(deck_page.contains(&format!("action=\"/decks/{pharm}/subdecks\"")));
    assert!(!deck_page.contains("deck-crumbs"), "a top-level deck");
    let (status, headers, _) = request(
        &h,
        post(format!("/decks/{pharm}/subdecks"), "name=Cardio".into()),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let cardio = id_of("Pharm::Cardio");
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{cardio}"))
    );
    let text_card = flash_core::validate_card_text("cardio-front", "b").unwrap();
    h.store
        .create_cards(h.user, cardio, &[(text_card, Vec::new())], None, now_ms())
        .unwrap();

    // The subdeck's page: its own name as the heading, the deck it sits
    // in as a crumb above, the whole path in the title.
    let (_, _, body) = request(&h, page(format!("/decks/{cardio}"))).await;
    let sub_page = text(body);
    assert!(sub_page.contains("<h1>Cardio</h1>"), "{sub_page}");
    assert!(sub_page.contains(&format!("<a href=\"/decks/{pharm}\">Pharm</a>")));
    assert!(sub_page.contains("<title>Pharm › Cardio - Flash</title>"));
    assert!(
        !sub_page.contains("Pharm::Cardio"),
        "a path is never shown as it is stored"
    );

    // The list: the subdeck under its deck, one step in, by its own name;
    // the deck's new count holds the subdeck's card.
    let (_, _, body) = request(&h, page("/decks".into())).await;
    let decks = text(body);
    let at = |needle: &str| {
        decks
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {decks}"))
    };
    assert!(at("style=\"--depth:0\"") < at("style=\"--depth:1\""));
    assert!(decks.contains("title=\"Pharm › Cardio\">Cardio</a>"));
    let pharm_row = &decks[at(">Pharm</a>")..at(">Cardio</a>")];
    assert!(
        pharm_row.contains("<td class=\"num\">3</td>"),
        "{pharm_row}"
    );
    // Its own page lists the subdeck too, in the same panel, still closed
    // as the page loads, counted on the button; and the subdeck's card
    // among its own.
    let (_, _, body) = request(&h, page(format!("/decks/{pharm}"))).await;
    let deck_page = text(body);
    subdecks_button(&deck_page, "Subdecks (1)");
    assert!(deck_page.contains(">Cardio</a>"));
    assert!(deck_page.contains("cardio-front"), "{deck_page}");

    // An archived subdeck is unarchived from its row there, as on the deck
    // list, and the page it comes back to is the one the row was on.
    h.store
        .set_deck_archived(h.user, cardio, Some(now_ms()))
        .unwrap();
    let (_, _, body) = request(&h, page(format!("/decks/{pharm}"))).await;
    let deck_page = text(body);
    assert!(deck_page.contains(">(archived)<"), "{deck_page}");
    assert!(deck_page.contains(&format!("action=\"/decks/{cardio}/unarchive\"")));
    assert!(deck_page.contains(&format!("name=\"from\" value=\"{pharm}\"")));
    let (status, headers, _) = request(
        &h,
        post(
            format!("/decks/{cardio}/unarchive"),
            format!("back=deck&from={pharm}"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{pharm}"))
    );
    let (_, _, body) = request(&h, page(format!("/decks/{pharm}"))).await;
    assert!(!text(body).contains(">(archived)<"));

    // Settings: the name field holds the deck's own name and refuses a
    // path; Parent deck offers every deck that is not under this one.
    let other = h.store.create_deck(h.user, "Other", "", now_ms()).unwrap();
    let (_, _, body) = request(&h, page(format!("/decks/{pharm}/settings"))).await;
    let settings = text(body);
    assert!(
        settings.contains("this deck's subdeck moves with it"),
        "{settings}"
    );
    assert!(settings.contains(&format!("<option value=\"{other}\">Other</option>")));
    assert!(
        !settings.contains(&format!("<option value=\"{cardio}\"")),
        "a deck cannot move under itself, so it is not offered"
    );
    assert!(settings.contains("its 1 subdeck and all 3 cards?"));
    let (_, headers, _) = request(
        &h,
        post(format!("/decks/{cardio}/rename"), "name=A%3A%3AB".into()),
    )
    .await;
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{cardio}/settings?err=nest"))
    );
    let (_, _, body) = request(&h, page(format!("/decks/{cardio}/settings?err=nest"))).await;
    let settings = text(body);
    assert!(settings.contains("use Parent deck to nest this deck"));
    // Where the whole path is shown, it reads as a person reads one.
    assert!(settings.contains("Deck settings &middot; Pharm › Cardio"));
    assert!(!settings.contains("Pharm::Cardio"), "{settings}");
    assert!(
        settings.contains("value=\"Cardio\""),
        "the leaf, not the path"
    );
    assert!(settings.contains(&format!(
        "<option value=\"{pharm}\" selected>Pharm</option>"
    )));

    // A rename keeps the deck where it is; a move keeps its name.
    request(
        &h,
        post(format!("/decks/{cardio}/rename"), "name=Heart".into()),
    )
    .await;
    assert_eq!(id_of("Pharm::Heart"), cardio);
    let (_, headers, _) = request(
        &h,
        post(format!("/decks/{pharm}/move"), format!("parent={other}")),
    )
    .await;
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{pharm}/settings?saved=parent"))
    );
    assert_eq!(id_of("Other::Pharm::Heart"), cardio);
    let (_, headers, _) = request(
        &h,
        post(format!("/decks/{other}/move"), format!("parent={cardio}")),
    )
    .await;
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{other}/settings?moverr=self"))
    );
    request(&h, post(format!("/decks/{pharm}/move"), "parent=".into())).await;
    assert_eq!(id_of("Pharm::Heart"), cardio);

    // Deleting the deck takes the subdeck.
    request(&h, post(format!("/decks/{pharm}/delete"), String::new())).await;
    assert!(h
        .store
        .find_deck_by_name(h.user, "Pharm::Heart")
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn boost_from_study_returns_to_the_queue() {
    let h = harness();
    let deck = seed_cards(&h, 1);
    let post = |path: String, body: &'static str| {
        Request::post(path)
            .header(header::ORIGIN, BASE)
            .header(header::COOKIE, h.session_cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .unwrap()
    };
    let (status, headers, _) = request(
        &h,
        post(format!("/decks/{deck}/boost"), "extra=5&back=study"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/study?deck={deck}"))
    );
    let (status, headers, _) = request(&h, post(format!("/decks/{deck}/boost"), "extra=5")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        header_of(&headers, "location"),
        Some(&*format!("/decks/{deck}/settings"))
    );
    let (status, headers, _) = request(&h, post("/study/boost".into(), "extra=5&back=study")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(header_of(&headers, "location"), Some("/study"));
}
