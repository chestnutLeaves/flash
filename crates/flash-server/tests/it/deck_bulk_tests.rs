use crate::common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::*;
use flash_core::{CardId, DeckId, UserId};
use flash_server::service::now_ms;
use flash_store::Store;

struct Harness {
    app: axum::Router,
    store: Arc<Store>,
    user: UserId,
    cookie: String,
}

fn harness(tag: &str) -> Harness {
    let t = AppBuilder::new(&format!("deck-bulk-{tag}")).build();
    let (user, cookie) = signed_in(&t.store, "FlashTester", "flashtester@example.com");
    Harness {
        app: t.app,
        store: t.store,
        user,
        cookie,
    }
}

fn add(h: &Harness, deck: DeckId, front: &str, tags: &[&str]) -> CardId {
    h.store
        .create_cards(
            h.user,
            deck,
            &[(
                flash_core::validate_card_text(front, "back").unwrap(),
                tags.iter().map(|t| t.to_string()).collect(),
            )],
            None,
            now_ms(),
        )
        .unwrap()[0]
}

async fn send(h: &Harness, req: Request<Body>) -> (StatusCode, Vec<(String, String)>, String) {
    let r = common::send(&h.app, req).await;
    let text = r.text();
    (r.status, r.headers, text)
}

fn get(h: &Harness, path: &str) -> Request<Body> {
    common::get(path, Some(&h.cookie))
}

fn post(h: &Harness, path: &str, body: &str) -> Request<Body> {
    common::form_post(path, Some(&h.cookie), body)
}

struct Cardio {
    cardio: DeckId,
    murmurs: DeckId,
    rate: CardId,
    chambers: CardId,
    stenosis: CardId,
}

fn cardio(h: &Harness) -> Cardio {
    let cardio = h.store.create_deck(h.user, "Cardio", "", now_ms()).unwrap();
    let murmurs = h
        .store
        .create_deck(h.user, "Cardio::Murmurs", "", now_ms())
        .unwrap();
    Cardio {
        cardio,
        murmurs,
        rate: add(h, cardio, "heart rate", &["exam-2", "physiology"]),
        chambers: add(h, cardio, "chambers", &["anatomy"]),
        stenosis: add(h, murmurs, "aortic stenosis", &["exam-2", "murmurs"]),
    }
}

fn rows(body: &str) -> usize {
    body.matches("<tr id=\"card-").count()
}

#[tokio::test]
async fn the_head_counts_and_the_subdeck_rows_go_into_their_decks() {
    let h = harness("head");
    let c = cardio(&h);
    let (status, _, body) = send(&h, get(&h, &format!("/decks/{}", c.cardio))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<b>3</b> new"), "{body}");
    assert!(body.contains("<b>3</b> cards"), "{body}");
    assert!(body.contains(&format!("href=\"/study?deck={}\"", c.cardio)));
    assert!(body.contains(&format!(
        "href=\"/decks/{}/export.apkg\" download",
        c.cardio
    )));
    assert!(body.contains(&format!("href=\"/decks/{}/export.csv\" download", c.cardio)));
    assert!(body.contains(&format!(
        "<a class=\"deck-sub-link\" href=\"/decks/{}\">",
        c.murmurs
    )));
    assert!(body.contains("<span>1 card</span>"));
    assert!(body.contains(&format!(
        "<a class=\"deck-sub-study\" href=\"/study?deck={}\"",
        c.murmurs
    )));
    assert_eq!(rows(&body), 3);
    assert_eq!(body.matches("class=\"deck-from\"").count(), 1);
    assert!(body.contains(&format!("data-from=\"{}\"", c.murmurs)));
}

#[tokio::test]
async fn the_filter_is_in_the_address_and_the_answer_names_it() {
    let h = harness("filter");
    let c = cardio(&h);
    let list = |q: &str| format!("/decks/{}/cards?{q}", c.cardio);

    let (_, headers, body) = send(&h, get(&h, &list("tags=exam-2"))).await;
    assert_eq!(rows(&body), 2, "{body}");
    assert_eq!(
        header_of(&headers, "hx-push-url"),
        Some(&*format!("/decks/{}?tags=exam-2", c.cardio))
    );
    assert!(body.contains("data-unfilter=\"tag\" data-value=\"exam-2\""));

    let (_, _, body) = send(&h, get(&h, &list("tags=physiology,murmurs"))).await;
    assert_eq!(rows(&body), 2);
    assert!(body.contains("data-match=\"any\" aria-current=\"true\""));
    let (_, _, body) = send(&h, get(&h, &list("tags=physiology,murmurs&match=all"))).await;
    assert_eq!(rows(&body), 0);
    assert!(body.contains("No cards match."), "{body}");

    let (_, _, body) = send(&h, get(&h, &list("decks=own"))).await;
    assert_eq!(rows(&body), 2);
    let (_, _, body) = send(&h, get(&h, &list(&format!("decks={}", c.murmurs)))).await;
    assert_eq!(rows(&body), 1);
    assert!(body.contains("aortic stenosis"));

    let (_, _, body) = send(
        &h,
        get(&h, &format!("/decks/{}?decks=own&tags=anatomy", c.cardio)),
    )
    .await;
    assert_eq!(rows(&body), 1, "{body}");
    assert!(body.contains("chambers"));
    assert!(body.contains("data-facet=\"own\" checked"));
    assert!(body.contains("data-tag=\"anatomy\" checked"));
}

#[tokio::test]
async fn archiving_takes_cards_out_of_the_list_and_undo_puts_them_back() {
    let h = harness("archive");
    let c = cardio(&h);
    let bulk = format!("/decks/{}/cards/bulk?page=1", c.cardio);
    let (status, _, body) = send(
        &h,
        post(
            &h,
            &bulk,
            &format!("action=archive&ids={},{}", c.rate, c.chambers),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(rows(&body), 1, "the active list keeps the other card");
    assert!(body.contains("2 cards archived"), "{body}");
    assert!(body.contains("id=\"deck-toast\"") && body.contains("hx-swap-oob=\"true\""));
    assert!(h.store.get_card(h.user, c.rate).unwrap().unwrap().suspended);

    let (_, _, archived) = send(
        &h,
        get(&h, &format!("/decks/{}/cards?status=archived", c.cardio)),
    )
    .await;
    assert_eq!(rows(&archived), 2);
    let (_, _, legacy) = send(
        &h,
        get(&h, &format!("/decks/{}/cards?archived=true", c.cardio)),
    )
    .await;
    assert_eq!(rows(&legacy), 2);

    let record = undo_record(&body);
    let (status, _, body) = send(
        &h,
        post(&h, &bulk, &format!("action=undo&undo={}", enc(&record))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(rows(&body), 3);
    assert!(body.contains("Undone"));
    assert!(!h.store.get_card(h.user, c.rate).unwrap().unwrap().suspended);
}

#[tokio::test]
async fn every_card_that_matches_is_acted_on_when_the_count_still_holds() {
    let h = harness("all");
    let c = cardio(&h);
    let bulk = format!("/decks/{}/cards/bulk?tags=exam-2&page=1", c.cardio);
    let (status, _, body) = send(&h, post(&h, &bulk, "action=archive&all=true&expect=3")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("The list changed"), "{body}");
    assert!(!h.store.get_card(h.user, c.rate).unwrap().unwrap().suspended);
    let (status, _, body) = send(&h, post(&h, &bulk, "action=archive&all=true&expect=2")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(h.store.get_card(h.user, c.rate).unwrap().unwrap().suspended);
    assert!(
        h.store
            .get_card(h.user, c.stenosis)
            .unwrap()
            .unwrap()
            .suspended
    );
    assert!(
        !h.store
            .get_card(h.user, c.chambers)
            .unwrap()
            .unwrap()
            .suspended
    );
}

#[tokio::test]
async fn moving_tagging_and_deleting_from_the_bar() {
    let h = harness("actions");
    let c = cardio(&h);
    let pharm = h.store.create_deck(h.user, "Pharm", "", now_ms()).unwrap();
    let bulk = format!("/decks/{}/cards/bulk?page=1", c.cardio);

    let (status, _, body) = send(
        &h,
        post(&h, &bulk, &format!("action=move&ids={}&to={pharm}", c.rate)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("1 card moved to Pharm"), "{body}");
    assert_eq!(
        h.store.get_card(h.user, c.rate).unwrap().unwrap().deck_id,
        pharm
    );
    let (_, _, back) = send(
        &h,
        post(
            &h,
            &bulk,
            &format!("action=undo&undo={}", enc(&undo_record(&body))),
        ),
    )
    .await;
    assert!(back.contains("heart rate"), "{back}");
    assert_eq!(
        h.store.get_card(h.user, c.rate).unwrap().unwrap().deck_id,
        c.cardio
    );

    let (_, _, body) = send(
        &h,
        post(
            &h,
            &bulk,
            &format!(
                "action=tag&ids={},{}&tags=Cardiac,+exam-2",
                c.rate, c.chambers
            ),
        ),
    )
    .await;
    assert!(body.contains("Tags added to 2 cards"), "{body}");
    assert_eq!(
        h.store.get_card(h.user, c.chambers).unwrap().unwrap().tags,
        ["anatomy", "cardiac", "exam-2"]
    );
    let (_, _, _) = send(
        &h,
        post(
            &h,
            &bulk,
            &format!("action=undo&undo={}", enc(&undo_record(&body))),
        ),
    )
    .await;
    assert_eq!(
        h.store.get_card(h.user, c.chambers).unwrap().unwrap().tags,
        ["anatomy"]
    );
    assert_eq!(
        h.store.get_card(h.user, c.rate).unwrap().unwrap().tags,
        ["exam-2", "physiology"],
        "a tag the card had stays after the undo"
    );

    let (status, _, body) = send(
        &h,
        post(&h, &bulk, &format!("action=delete&ids={}", c.chambers)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("1 card deleted"), "{body}");
    assert!(!body.contains("name=\"undo\""), "a delete has no undo");
    assert!(h.store.get_card(h.user, c.chambers).unwrap().is_none());
}

#[tokio::test]
async fn a_forged_tag_undo_is_held_to_the_rules_of_adding_a_tag() {
    let h = harness("forged-undo");
    let c = cardio(&h);
    let bulk = format!("/decks/{}/cards/bulk?page=1", c.cardio);
    let relink = |pairs: String| {
        format!(
            "action=undo&undo={}",
            enc(&format!(
                r#"{{"k":"tags","pairs":[{pairs}],"added":false}}"#
            ))
        )
    };
    let tags = |card| h.store.get_card(h.user, card).unwrap().unwrap().tags;

    let long = format!("A,B{}", "x".repeat(300));
    send(
        &h,
        post(&h, &bulk, &relink(format!("[{},\"{long}\"]", c.rate))),
    )
    .await;
    assert_eq!(tags(c.rate), ["exam-2", "physiology"], "a tag too long");

    let many: Vec<String> = (0..60).map(|i| format!("[{},\"t{i}\"]", c.rate)).collect();
    send(&h, post(&h, &bulk, &relink(many.join(",")))).await;
    assert_eq!(tags(c.rate), ["exam-2", "physiology"], "too many tags");

    send(
        &h,
        post(&h, &bulk, &relink(format!("[{},\"  Cardiac \"]", c.rate))),
    )
    .await;
    assert_eq!(tags(c.rate), ["cardiac", "exam-2", "physiology"]);
}

#[tokio::test]
async fn an_action_naming_a_card_that_is_not_the_users_does_nothing() {
    let h = harness("foreign");
    let c = cardio(&h);
    let (other, _) = signed_in(&h.store, "Other", "other@example.com");
    let theirs_deck = h.store.create_deck(other, "Theirs", "", now_ms()).unwrap();
    let theirs = h
        .store
        .create_cards(
            other,
            theirs_deck,
            &[(
                flash_core::validate_card_text("theirs", "b").unwrap(),
                vec![],
            )],
            None,
            now_ms(),
        )
        .unwrap()[0];
    let bulk = format!("/decks/{}/cards/bulk?page=1", c.cardio);
    let (status, _, _) = send(
        &h,
        post(&h, &bulk, &format!("action=delete&ids={},{theirs}", c.rate)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        h.store.get_card(h.user, c.rate).unwrap().is_some(),
        "nothing of the set went"
    );
    assert!(h.store.get_card(other, theirs).unwrap().is_some());
    let (status, _, _) = send(
        &h,
        post(
            &h,
            &format!("/decks/{theirs_deck}/cards/bulk"),
            &format!("action=archive&ids={theirs}"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!h.store.get_card(other, theirs).unwrap().unwrap().suspended);
}

#[tokio::test]
async fn a_decks_export_holds_its_tree_named_from_the_deck() {
    let h = harness("export");
    let c = cardio(&h);
    let elsewhere = h.store.create_deck(h.user, "Pharm", "", now_ms()).unwrap();
    add(&h, elsewhere, "warfarin", &[]);

    let (status, headers, csv) =
        send(&h, get(&h, &format!("/decks/{}/export.csv", c.murmurs))).await;
    assert_eq!(status, StatusCode::OK);
    let disposition = header_of(&headers, "content-disposition").unwrap();
    assert!(
        disposition.starts_with("attachment; filename=\"Murmurs "),
        "{disposition}"
    );
    assert!(
        disposition.contains("filename*=UTF-8''Murmurs%20"),
        "{disposition}"
    );
    assert!(csv.contains("aortic stenosis"));
    assert!(
        !csv.contains("heart rate") && !csv.contains("warfarin"),
        "{csv}"
    );
    assert!(csv.contains(",Murmurs,"), "named from the deck: {csv}");

    let (status, _, csv) = send(&h, get(&h, &format!("/decks/{}/export.csv", c.cardio))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(csv.contains("heart rate") && csv.contains("aortic stenosis"));
    assert!(csv.contains("Cardio::Murmurs"));
    assert!(!csv.contains("warfarin"));

    let (status, headers, _) = send(&h, get(&h, &format!("/decks/{}/export.apkg", c.cardio))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(header_of(&headers, "content-disposition")
        .unwrap()
        .contains(".apkg"));
}

/// The Undo record the toast's form carries, as the browser sends it
/// back (entities undone).
fn undo_record(body: &str) -> String {
    let at = body
        .find("name=\"undo\" value=\"")
        .unwrap_or_else(|| panic!("no undo: {body}"));
    let rest = &body[at + "name=\"undo\" value=\"".len()..];
    rest[..rest.find('"').unwrap()]
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
