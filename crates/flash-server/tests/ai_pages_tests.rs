//! Card generation in the browser: the picker, the reading it gets
//! back, the job page's polling, and the review where drafts are kept,
//! edited or discarded. The model is the scripted fake, so what is
//! asserted here is the pages and their swaps, not the model. A server
//! with no model configured has none of these pages at all.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;
use flash_core::{AiDraftId, AiJobId, DeckId, UserId};
use flash_server::ai::fake::FakeProvider;
use flash_server::ai::runner;
use flash_server::policy::ALL_AI_INPUTS;
use flash_server::service::now_ms;

const PASSAGE: &str = "The heart has four chambers: two atria and two ventricles. Blood enters \
                       the right atrium and leaves for the lungs through the pulmonary valve.";

struct Pages {
    t: TestApp,
    user: UserId,
    cookie: String,
    deck: DeckId,
}

/// A signed-in account on a server whose model is the fake, with one
/// deck to generate into.
fn pages(tag: &str) -> Pages {
    let t = AppBuilder::new(&format!("ai-pages-{tag}"))
        .ai(Arc::new(FakeProvider::default()), ALL_AI_INPUTS.to_vec())
        .build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let deck = t
        .services
        .create_deck(user, "Anatomy", "", now_ms())
        .unwrap();
    Pages {
        t,
        user,
        cookie,
        deck,
    }
}

async fn get_page(p: &Pages, path: &str) -> Reply {
    send(&p.t.app, get(path, Some(&p.cookie))).await
}

async fn post_form(p: &Pages, path: &str, body: &str) -> Reply {
    send(&p.t.app, form_post(path, Some(&p.cookie), body)).await
}

/// A source, a job, and one pass of the runner over it: what every
/// review-page test starts from.
async fn generated(p: &Pages) -> i64 {
    let r = post_form(p, "/ai/sources", &format!("text={}", enc(PASSAGE))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let source = source_id(&r.text());
    let r = post_form(
        p,
        "/ai/jobs",
        &format!("source_ids={source}&deck_id={}&instruction=", p.deck.0),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text());
    let job = r
        .location()
        .trim_start_matches("/ai/jobs/")
        .parse::<i64>()
        .unwrap();
    assert!(
        runner::run_once(&p.t.state).await >= 1,
        "runner started nothing"
    );
    job
}

/// The id in the JSON a source upload answers with.
fn source_id(json: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(json).expect("the upload answers JSON")["id"]
        .as_i64()
        .expect("the upload names a source")
}

fn drafts(p: &Pages, job: i64) -> Vec<flash_store::AiDraftRow> {
    p.t.services.ai_drafts(p.user, AiJobId(job)).unwrap()
}

fn cards_in_deck(p: &Pages) -> u32 {
    p.t.services
        .deck_cards_page(p.user, p.deck, None, 1, 25, now_ms())
        .unwrap()
        .total
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---- the picker ----

#[tokio::test]
async fn the_generate_page_offers_every_input_this_server_reads() {
    let p = pages("picker");
    let r = get_page(&p, "/generate").await;
    assert_eq!(r.status, StatusCode::OK);
    let html = r.text();
    // The composer's rules travel as escaped JSON in one attribute.
    let at = html
        .find("data-gen-caps=\"")
        .expect("the composer carries its caps");
    let rest = &html[at + "data-gen-caps=\"".len()..];
    let raw = &rest[..rest.find('"').unwrap()];
    let unescaped = raw
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let caps: serde_json::Value = serde_json::from_str(&unescaped).expect("caps parse");
    let kinds: Vec<&str> = caps["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["text", "pdf", "image", "youtube", "video", "audio"]);
    assert!(caps["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .all(|k| k["withheld"].is_null()));
    assert!(html.contains("name=\"deck_id\""));
    assert!(html.contains("id=\"gen-instruction\""));
    assert!(html.contains("name=\"source_ids\""));
    assert!(html.contains("id=\"gen-chip\""));
    assert!(html.contains("data-gen-link-toggle"), "links are offered");
    assert!(html.contains("accept=\".txt"), "{html}");
    // The core never says what a plan costs; only the extension's slot
    // could, and there is none here.
    assert!(
        !html.contains("Pro"),
        "the open server named a plan:\n{html}"
    );

    // The entry points are on both deck surfaces.
    let decks = get_page(&p, "/decks").await.text();
    assert!(decks.contains("href=\"/generate\""), "{decks}");
    let deck = get_page(&p, &format!("/decks/{}", p.deck.0)).await.text();
    assert!(
        deck.contains(&format!("/decks/{}/generate", p.deck.0)),
        "{deck}"
    );
}

#[tokio::test]
async fn a_server_without_a_model_has_no_generation_pages() {
    let t = AppBuilder::new("ai-pages-off").build();
    let (_user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    let deck = t.store.create_deck(_user, "Anatomy", "", now_ms()).unwrap();

    for path in ["/generate", "/ai/jobs/1", "/ai/jobs/1/review"] {
        let r = send(&t.app, get(path, Some(&cookie))).await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "{path} answered {}",
            r.status
        );
    }
    let r = send(
        &t.app,
        get(&format!("/decks/{}/generate", deck.0), Some(&cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // And the pages that would link to it don't.
    let decks = send(&t.app, get("/decks", Some(&cookie))).await.text();
    assert!(!decks.contains("/generate"), "{decks}");
    let page = send(&t.app, get(&format!("/decks/{}", deck.0), Some(&cookie))).await;
    assert!(!page.text().contains("Generate cards"));
}

// ---- the source and its reading ----

#[tokio::test]
async fn pasted_text_comes_back_as_a_reading_that_allows_a_generation() {
    let p = pages("preflight");
    let r = post_form(&p, "/ai/sources", &format!("text={}", enc(PASSAGE))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    let json = r.json();
    assert_eq!(json["kind"], "text");
    assert_eq!(json["preflight"]["allowed"], true, "{json}");
    assert_eq!(json["line"], "~24 words");
    let id = source_id(&r.text());
    assert!(id > 0);

    // The set's reading, as the page asks for it before enabling
    // Generate.
    let req = Request::post("/ai/preflight")
        .header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!("{{\"source_ids\":[{id}]}}")))
        .unwrap();
    let r = send(&p.t.app, req).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["allowed"], true);
    assert_eq!(r.json()["line"], "~24 words");

    // A chip taken off the composer is gone.
    let req = Request::delete(format!("/ai/sources/{id}"))
        .header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .body(Body::empty())
        .unwrap();
    let r = send(&p.t.app, req).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    let req = Request::post("/ai/preflight")
        .header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!("{{\"source_ids\":[{id}]}}")))
        .unwrap();
    assert_eq!(send(&p.t.app, req).await.status, StatusCode::NOT_FOUND);
    // Nothing to read is a sentence, not a stack trace.
    let r = post_form(&p, "/ai/sources", "text=").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.text());
    assert!(r.text().contains("Add something"), "{}", r.text());
}

#[tokio::test]
async fn the_pages_own_multipart_body_carries_a_file_or_text() {
    let p = pages("multipart");
    let boundary = "flashboundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\ntext\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"text\"\r\n\r\n{PASSAGE}\r\n\
         --{boundary}--\r\n"
    );
    let req = Request::post("/ai/sources")
        .header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let r = send(&p.t.app, req).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    assert_eq!(r.json()["preflight"]["allowed"], true, "{}", r.text());

    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"notes.pdf\"\r\n\
         Content-Type: application/pdf\r\n\r\n%PDF-1.7 hello\r\n\
         --{boundary}--\r\n"
    );
    let req = Request::post("/ai/sources")
        .header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let r = send(&p.t.app, req).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.text());
    assert_eq!(r.json()["kind"], "pdf");
    assert_eq!(r.json()["filename"], "notes.pdf");
}

// ---- the job ----

#[tokio::test]
async fn a_finished_job_sends_the_poll_to_the_review() {
    let p = pages("job");
    let job = generated(&p).await;

    // The page while it runs and the page once it is done are the same
    // route; only the answer differs.
    let r = send(
        &p.t.app,
        Request::get(format!("/ai/jobs/{job}"))
            .header(header::COOKIE, p.cookie.clone())
            .header("HX-Request", "true")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(
        r.header("hx-redirect"),
        Some(format!("/ai/jobs/{job}/review").as_str())
    );

    // A browser that opens the page late is redirected the same way.
    let r = get_page(&p, &format!("/ai/jobs/{job}")).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), format!("/ai/jobs/{job}/review"));
}

// ---- the review ----

#[tokio::test]
async fn the_review_groups_the_drafts_by_note_type() {
    let p = pages("review");
    let job = generated(&p).await;
    let r = get_page(&p, &format!("/ai/jobs/{job}/review")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();

    assert_eq!(html.matches("class=\"gen-row\"").count(), 3, "{html}");
    for label in ["Basic", "Cloze", "Basic (type in the answer)"] {
        assert!(html.contains(label), "no group for {label}:\n{html}");
    }
    // The rich sides render through the deck page's own markup, so
    // cloze, media and math look the way they will on the card.
    assert!(html.contains("card-rich"), "{html}");
    assert!(html.contains("Keep all (3)"), "{html}");
    assert!(html.contains("Discard all (3)"), "{html}");
    assert!(html.contains("Uses 1 generation"), "{html}");
    assert!(html.contains("Nothing here is in a deck until you keep it"));
}

#[tokio::test]
async fn keeping_one_row_swaps_it_and_leaves_a_note_behind() {
    let p = pages("keep-row");
    let job = generated(&p).await;
    let draft = drafts(&p, job)[0].id;
    assert_eq!(cards_in_deck(&p), 0);

    let r = post_form(
        &p,
        &format!("/ai/drafts/{}/keep", draft.0),
        &format!("deck_id={}", p.deck.0),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(
        html.contains(&format!("id=\"draft-{}\"", draft.0)),
        "{html}"
    );
    assert!(html.contains("data-status=\"kept\""), "{html}");
    // A decided row keeps no buttons: there is nothing left to decide.
    assert!(!html.contains("/discard"), "{html}");
    assert!(cards_in_deck(&p) >= 1, "the keep created no card");
}

#[tokio::test]
async fn discarding_one_row_swaps_it_and_creates_nothing() {
    let p = pages("discard-row");
    let job = generated(&p).await;
    let draft = drafts(&p, job)[1].id;

    let r = post_form(&p, &format!("/ai/drafts/{}/discard", draft.0), "").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(
        r.text().contains("data-status=\"discarded\""),
        "{}",
        r.text()
    );
    assert_eq!(cards_in_deck(&p), 0);
}

#[tokio::test]
async fn the_edit_dialog_opens_over_the_draft_it_names() {
    let p = pages("edit");
    let job = generated(&p).await;
    let rows = drafts(&p, job);
    let cloze = rows
        .iter()
        .find(|d| d.note_type == flash_store::notes::NoteType::Cloze)
        .expect("the fake writes a cloze draft");

    let r = get_page(&p, &format!("/ai/drafts/{}/edit", cloze.id.0)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(html.contains("Edit draft"), "{html}");
    assert!(
        html.contains(&format!("hx-post=\"/ai/drafts/{}/edit\"", cloze.id.0)),
        "{html}"
    );
    assert!(
        html.contains("tricuspid"),
        "the dialog is not seeded:\n{html}"
    );
    assert!(html.contains("value=\"cloze\" selected"), "{html}");

    // Saving swaps the row and closes the dialog in one reply.
    let r = post_form(
        &p,
        &format!("/ai/drafts/{}/edit", cloze.id.0),
        &format!(
            "note_type=cloze&front_html={}&back_html=&tags=anatomy",
            enc("Blood passes the {{c1::tricuspid}} valve on the right.")
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(
        html.contains(&format!("id=\"draft-{}\"", cloze.id.0)),
        "{html}"
    );
    assert!(
        html.contains("hx-swap-oob"),
        "the dialog stayed open:\n{html}"
    );
    assert!(html.contains("on the right"), "{html}");
}

#[tokio::test]
async fn a_refused_edit_comes_back_as_the_dialog_not_the_row() {
    let p = pages("edit-refused");
    let job = generated(&p).await;
    let cloze = drafts(&p, job)
        .into_iter()
        .find(|d| d.note_type == flash_store::notes::NoteType::Cloze)
        .unwrap();

    // A cloze note without a deletion is what the editor refuses.
    let r = post_form(
        &p,
        &format!("/ai/drafts/{}/edit", cloze.id.0),
        &format!(
            "note_type=cloze&front_html={}&back_html=&tags=",
            enc("No deletion here at all.")
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.header("hx-retarget"), Some("#editor-dialog"));
    assert!(r.text().contains("editor-error"), "{}", r.text());
}

// ---- the whole review at once ----

#[tokio::test]
async fn keeping_everything_lands_on_the_deck() {
    let p = pages("keep-all");
    let job = generated(&p).await;

    let r = post_form(
        &p,
        &format!("/ai/jobs/{job}/keep"),
        &format!("deck_id={}", p.deck.0),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text());
    assert_eq!(r.location(), format!("/decks/{}", p.deck.0));
    assert!(cards_in_deck(&p) >= 3, "not every draft was kept");

    // Nothing is left waiting on the review afterwards.
    let html = get_page(&p, &format!("/ai/jobs/{job}/review")).await.text();
    assert!(html.contains("<strong>0</strong> waiting on you"), "{html}");
}

#[tokio::test]
async fn discarding_everything_leaves_the_review_empty_handed() {
    let p = pages("discard-all");
    let job = generated(&p).await;

    let r = post_form(&p, &format!("/ai/jobs/{job}/discard"), "").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text());
    assert_eq!(r.location(), format!("/ai/jobs/{job}/review"));
    assert_eq!(cards_in_deck(&p), 0);
    let html = get_page(&p, &format!("/ai/jobs/{job}/review")).await.text();
    assert!(html.contains("3 discarded"), "{html}");
}

// ---- the thread ----

/// A request the way htmx sends it: same origin, signed in, marked.
fn hx(req: axum::http::request::Builder, p: &Pages) -> axum::http::request::Builder {
    req.header(header::ORIGIN, BASE)
        .header(header::COOKIE, p.cookie.clone())
        .header("HX-Request", "true")
}

async fn hx_post(p: &Pages, path: &str, body: &str) -> Reply {
    let req = hx(Request::post(path), p)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(&p.t.app, req).await
}

async fn hx_get(p: &Pages, path: &str) -> Reply {
    let req = hx(Request::get(path), p).body(Body::empty()).unwrap();
    send(&p.t.app, req).await
}

#[tokio::test]
async fn a_message_on_the_review_runs_a_pass_and_the_drafts_change_in_place() {
    let p = pages("thread");
    let job = generated(&p).await;

    // The review carries an empty thread and no separate forms.
    let html = get_page(&p, &format!("/ai/jobs/{job}/review")).await.text();
    assert!(html.contains("id=\"thread\""), "{html}");
    assert!(html.contains("data-active=\"false\""), "{html}");
    assert!(html.contains("data-turns=\"0\""), "{html}");
    assert!(!html.contains("hx-trigger=\"every 2s\""), "{html}");
    assert!(
        !html.contains("gen-again") && !html.contains("/regenerate"),
        "{html}"
    );
    assert!(html.contains("id=\"thread-live\""), "{html}");

    // A send from htmx: the block comes back with the message as a
    // bubble, the pending bubble, the box closed, and polling on.
    let r = hx_post(
        &p,
        &format!("/ai/jobs/{job}/edit"),
        &format!("message={}", enc("Shorter answers")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(
        html.trim_start().starts_with("<section id=\"thread\""),
        "{html}"
    );
    assert!(html.contains("gen-msg-user\">Shorter answers<"), "{html}");
    assert!(html.contains("id=\"thread-pending\""), "{html}");
    assert!(html.contains("data-active=\"true\""), "{html}");
    assert!(html.contains("data-turns=\"1\""), "{html}");
    assert!(html.contains("hx-trigger=\"every 2s\""), "{html}");
    assert!(html.contains("disabled></textarea>"), "{html}");
    assert!(
        !html.contains("hx-swap-oob"),
        "the drafts have not changed yet"
    );

    // A poll while the pass is queued: the same block, still polling.
    let r = hx_get(&p, &format!("/ai/jobs/{job}/thread")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(r.text().contains("hx-trigger=\"every 2s\""), "{}", r.text());
    assert!(!r.text().contains("hx-swap-oob"), "{}", r.text());

    // The pass runs; the poll that finds it done brings the reply and
    // the rewritten rows out of band.
    assert!(runner::run_once(&p.t.state).await >= 1);
    let r = hx_get(&p, &format!("/ai/jobs/{job}/thread")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(html.contains("data-active=\"false\""), "{html}");
    assert!(html.contains("data-turns=\"2\""), "{html}");
    assert!(html.contains("gen-msg-model\">Rewrote 3 cards.<"), "{html}");
    assert!(!html.contains("thread-pending"), "{html}");
    assert!(!html.contains("hx-trigger"), "{html}");
    assert!(
        html.contains("<div id=\"review\" style=\"display:contents\" hx-swap-oob=\"true\">"),
        "{html}"
    );
    assert_eq!(html.matches("class=\"gen-row\"").count(), 3, "{html}");
    assert!(html.contains("(Shorter answers)"), "{html}");
    assert!(html.contains("Keep all (3)"), "{html}");

    // Reopened later, the review shows the whole thread.
    let html = get_page(&p, &format!("/ai/jobs/{job}/review")).await.text();
    assert!(html.contains("gen-msg-user\">Shorter answers<"), "{html}");
    assert!(html.contains("gen-msg-model\">Rewrote 3 cards.<"), "{html}");

    // Without htmx the send lands back on the review, where the thread
    // polls for the pass it queued.
    let r = post_form(
        &p,
        &format!("/ai/jobs/{job}/edit"),
        &format!("message={}", enc("Add five on the valves")),
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.text());
    assert_eq!(r.location(), format!("/ai/jobs/{job}/review"));
    let html = get_page(&p, &format!("/ai/jobs/{job}/review")).await.text();
    assert!(html.contains("data-active=\"true\""), "{html}");
    assert!(html.contains("hx-trigger=\"every 2s\""), "{html}");
}

#[tokio::test]
async fn a_message_that_is_not_taken_comes_back_in_the_box_with_the_reason() {
    let p = pages("thread-refused");
    let job = generated(&p).await;

    // Blank: the box stays, the reason is beside it.
    let r = hx_post(&p, &format!("/ai/jobs/{job}/edit"), "message=+").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(
        html.contains("role=\"alert\">Say what you&#39;d like changed.<")
            || html.contains("role=\"alert\">Say what you'd like changed.<"),
        "{html}"
    );
    assert!(html.contains("data-active=\"false\""), "{html}");
    assert!(!html.contains("hx-trigger"), "{html}");

    // A second message while the first pass runs: the typed text is
    // echoed back into the box with the reason.
    let r = hx_post(
        &p,
        &format!("/ai/jobs/{job}/edit"),
        &format!("message={}", enc("shorter")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let r = hx_post(
        &p,
        &format!("/ai/jobs/{job}/edit"),
        &format!("message={}", enc("and longer")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let html = r.text();
    assert!(html.contains("A generation is already running."), "{html}");
    assert!(html.contains(">and longer</textarea>"), "{html}");
    assert!(
        html.contains("data-turns=\"1\""),
        "only the first message was taken: {html}"
    );

    // Without htmx the refusal is the whole review with the reason in
    // the thread.
    let r = post_form(&p, &format!("/ai/jobs/{job}/edit"), "message=").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert!(
        r.text().contains("<title>Review generated cards"),
        "{}",
        r.text()
    );
    assert!(r.text().contains("like changed."), "{}", r.text());
}

// ---- other people's ids, and other people's pages ----

#[tokio::test]
async fn another_accounts_job_and_drafts_are_not_found() {
    let p = pages("foreign");
    let job = generated(&p).await;
    let draft = drafts(&p, job)[0].id;
    let (_other, other_cookie) = signed_in(&p.t.store, "Someone", "someone@example.com");

    for path in [
        format!("/ai/jobs/{job}"),
        format!("/ai/jobs/{job}/review"),
        format!("/ai/jobs/{job}/thread"),
        format!("/ai/drafts/{}/edit", draft.0),
    ] {
        let r = send(&p.t.app, get(&path, Some(&other_cookie))).await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "{path} answered {}",
            r.status
        );
    }
    for path in [
        format!("/ai/jobs/{job}/keep"),
        format!("/ai/jobs/{job}/discard"),
        format!("/ai/jobs/{job}/edit"),
        format!("/ai/drafts/{}/keep", draft.0),
        format!("/ai/drafts/{}/discard", draft.0),
    ] {
        let r = send(&p.t.app, form_post(&path, Some(&other_cookie), "message=x")).await;
        assert_eq!(
            r.status,
            StatusCode::NOT_FOUND,
            "{path} answered {}",
            r.status
        );
    }
    // And nothing of the owner's moved.
    assert_eq!(
        p.t.services
            .ai_job_detail(p.user, AiJobId(job))
            .unwrap()
            .counts
            .pending,
        3
    );
    assert_eq!(
        p.t.services
            .ai_draft(p.user, AiDraftId(draft.0))
            .unwrap()
            .status,
        flash_store::AiDraftStatus::Pending
    );
}

#[tokio::test]
async fn every_generation_post_refuses_a_cross_origin_request() {
    let p = pages("cross-origin");
    let job = generated(&p).await;
    let draft = drafts(&p, job)[0].id;

    for path in [
        "/ai/sources".to_string(),
        "/ai/preflight".to_string(),
        "/ai/jobs".to_string(),
        format!("/ai/jobs/{job}/keep"),
        format!("/ai/jobs/{job}/discard"),
        format!("/ai/jobs/{job}/edit"),
        format!("/ai/drafts/{}/keep", draft.0),
        format!("/ai/drafts/{}/discard", draft.0),
        format!("/ai/drafts/{}/edit", draft.0),
    ] {
        let req = Request::post(&path)
            .header(header::ORIGIN, "https://attacker.example")
            .header(header::COOKIE, p.cookie.clone())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::empty())
            .unwrap();
        let r = send(&p.t.app, req).await;
        assert_eq!(
            r.status,
            StatusCode::FORBIDDEN,
            "{path} answered {}",
            r.status
        );
        assert!(r.text().contains("cross-origin"), "{path}: {}", r.text());
    }
    let req = Request::delete("/ai/sources/1")
        .header(header::ORIGIN, "https://attacker.example")
        .header(header::COOKIE, p.cookie.clone())
        .body(Body::empty())
        .unwrap();
    let r = send(&p.t.app, req).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.text());
    // Nothing was decided by any of them.
    assert_eq!(
        p.t.services
            .ai_job_detail(p.user, AiJobId(job))
            .unwrap()
            .counts
            .pending,
        3
    );
}
