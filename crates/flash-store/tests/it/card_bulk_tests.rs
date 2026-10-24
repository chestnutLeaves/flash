//! The deck page's filtered card list and the actions on many cards:
//! the filter's parts (decks, search, status, tags any/all) and how they
//! combine, and each action's guard (another user's card, a card outside
//! the deck) and its note rule (a move or a tag takes the note's other
//! cards along), with what each returns for an Undo.

use flash_core::{validate_card_text, CardId, DeckId, UserId};
use flash_store::notes::NoteType;
use flash_store::richtext::sanitize_with_media;
use flash_store::{CardFilter, CardStatus, DeckFacet, NoteSave, Store, StoreError};

const NOW: i64 = 1_700_000_000_000;

fn store_with_user() -> (Store, UserId) {
    let store = Store::open_in_memory().expect("open + migrate");
    let user = store
        .create_user("Test User", Some("t@example.com"), "member", NOW)
        .unwrap();
    (store, user)
}

fn add(store: &Store, user: UserId, deck: DeckId, front: &str, tags: &[&str]) -> CardId {
    store
        .create_cards(
            user,
            deck,
            &[(
                validate_card_text(front, "back").unwrap(),
                tags.iter().map(|t| t.to_string()).collect(),
            )],
            None,
            NOW,
        )
        .unwrap()[0]
}

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

fn fronts(store: &Store, user: UserId, f: &CardFilter<'_>) -> Vec<String> {
    let mut out: Vec<String> = store
        .list_filtered_cards(user, f, 100, 0, None)
        .unwrap()
        .into_iter()
        .map(|c| c.front)
        .collect();
    out.sort();
    out
}

/// Cardio (two cards) with Murmurs (one) and an archived Old subdeck
/// (one).
struct Tree {
    store: Store,
    user: UserId,
    cardio: DeckId,
    murmurs: DeckId,
    old: DeckId,
}

fn tree() -> Tree {
    let (store, user) = store_with_user();
    let cardio = store.create_deck(user, "Cardio", "", NOW).unwrap();
    let murmurs = store.create_deck(user, "Cardio::Murmurs", "", NOW).unwrap();
    let old = store.create_deck(user, "Cardio::Old", "", NOW).unwrap();
    add(
        &store,
        user,
        cardio,
        "heart rate",
        &["exam-2", "physiology"],
    );
    add(&store, user, cardio, "chambers", &["anatomy"]);
    add(
        &store,
        user,
        murmurs,
        "aortic stenosis",
        &["exam-2", "murmurs"],
    );
    add(&store, user, old, "retired", &["exam-2"]);
    store.set_deck_archived(user, old, Some(NOW)).unwrap();
    Tree {
        store,
        user,
        cardio,
        murmurs,
        old,
    }
}

#[test]
fn a_deck_lists_its_subdecks_cards_but_not_an_archived_subdecks() {
    let t = tree();
    let f = CardFilter::deck(t.cardio);
    assert_eq!(
        fronts(&t.store, t.user, &f),
        ["aortic stenosis", "chambers", "heart rate"]
    );
    assert_eq!(t.store.count_filtered_cards(t.user, &f).unwrap(), 3);
    // The archived subdeck's own page still shows what it holds.
    assert_eq!(
        fronts(&t.store, t.user, &CardFilter::deck(t.old)),
        ["retired"]
    );
}

#[test]
fn the_decks_facet_picks_the_decks_own_cards_or_a_subdeck() {
    let t = tree();
    let own = [DeckFacet::Own];
    let sub = [DeckFacet::Sub(t.murmurs)];
    let both = [DeckFacet::Own, DeckFacet::Sub(t.murmurs)];
    let f = |decks| CardFilter {
        decks,
        ..CardFilter::deck(t.cardio)
    };
    assert_eq!(
        fronts(&t.store, t.user, &f(&own)),
        ["chambers", "heart rate"]
    );
    assert_eq!(fronts(&t.store, t.user, &f(&sub)), ["aortic stenosis"]);
    assert_eq!(fronts(&t.store, t.user, &f(&both)).len(), 3);

    // A deck outside the tree is not found, not silently empty.
    let elsewhere = t.store.create_deck(t.user, "Pharm", "", NOW).unwrap();
    let outside = [DeckFacet::Sub(elsewhere)];
    assert!(matches!(
        t.store.count_filtered_cards(t.user, &f(&outside)),
        Err(StoreError::NotFound("deck"))
    ));
}

#[test]
fn tags_match_any_by_default_and_all_when_asked() {
    let t = tree();
    let wanted = tags(&["Physiology", "murmurs"]);
    let any = CardFilter {
        tags: &wanted,
        ..CardFilter::deck(t.cardio)
    };
    assert_eq!(
        fronts(&t.store, t.user, &any),
        ["aortic stenosis", "heart rate"]
    );
    let all = CardFilter {
        match_all: true,
        ..any
    };
    assert!(fronts(&t.store, t.user, &all).is_empty());

    let pair = tags(&["exam-2", "murmurs"]);
    let all_pair = CardFilter {
        tags: &pair,
        match_all: true,
        ..CardFilter::deck(t.cardio)
    };
    assert_eq!(fronts(&t.store, t.user, &all_pair), ["aortic stenosis"]);
}

#[test]
fn search_status_and_tags_narrow_each_other() {
    let t = tree();
    let chambers = t
        .store
        .list_filtered_cards(t.user, &CardFilter::deck(t.cardio), 100, 0, None)
        .unwrap()
        .into_iter()
        .find(|c| c.front == "chambers")
        .unwrap()
        .id;
    t.store
        .set_cards_suspended(t.user, Some(t.cardio), &[chambers], true, NOW)
        .unwrap();
    let base = CardFilter::deck(t.cardio);
    assert_eq!(
        fronts(&t.store, t.user, &base).len(),
        2,
        "archived leaves Active"
    );
    let archived = CardFilter {
        status: CardStatus::Archived,
        ..base
    };
    assert_eq!(fronts(&t.store, t.user, &archived), ["chambers"]);
    let all = CardFilter {
        status: CardStatus::All,
        ..base
    };
    assert_eq!(fronts(&t.store, t.user, &all).len(), 3);
    let exam = tags(&["exam-2"]);
    let narrowed = CardFilter {
        search: Some("AORTIC"),
        tags: &exam,
        ..base
    };
    assert_eq!(fronts(&t.store, t.user, &narrowed), ["aortic stenosis"]);
}

#[test]
fn every_matching_id_is_the_lists_set() {
    let t = tree();
    let exam = tags(&["exam-2"]);
    let f = CardFilter {
        tags: &exam,
        ..CardFilter::deck(t.cardio)
    };
    let mut ids = t.store.filtered_card_ids(t.user, &f).unwrap();
    let mut listed: Vec<CardId> = t
        .store
        .list_filtered_cards(t.user, &f, 100, 0, None)
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    ids.sort();
    listed.sort();
    assert_eq!(ids, listed);
}

#[test]
fn an_action_refuses_the_whole_set_for_one_card_not_in_the_deck_or_not_the_users() {
    let t = tree();
    let mine = add(&t.store, t.user, t.cardio, "mine", &[]);
    let pharm = t.store.create_deck(t.user, "Pharm", "", NOW).unwrap();
    let elsewhere = add(&t.store, t.user, pharm, "elsewhere", &[]);
    let other = t
        .store
        .create_user("Other", Some("o@example.com"), "member", NOW)
        .unwrap();
    let other_deck = t.store.create_deck(other, "Theirs", "", NOW).unwrap();
    let theirs = add(&t.store, other, other_deck, "theirs", &[]);

    for stranger in [elsewhere, theirs] {
        let set = [mine, stranger];
        assert!(matches!(
            t.store
                .set_cards_suspended(t.user, Some(t.cardio), &set, true, NOW),
            Err(StoreError::NotFound("card"))
        ));
        assert!(t
            .store
            .delete_cards(t.user, Some(t.cardio), &set, NOW)
            .is_err());
        assert!(t
            .store
            .add_card_tags(t.user, Some(t.cardio), &set, &tags(&["x"]), NOW)
            .is_err());
    }
    let card = t.store.get_card(t.user, mine).unwrap().unwrap();
    assert!(
        !card.suspended && card.tags.is_empty(),
        "nothing was written"
    );
    assert!(t.store.get_card(other, theirs).unwrap().is_some());
}

#[test]
fn archiving_returns_only_the_cards_it_changed() {
    let t = tree();
    let a = add(&t.store, t.user, t.cardio, "a", &[]);
    let b = add(&t.store, t.user, t.cardio, "b", &[]);
    t.store
        .set_cards_suspended(t.user, Some(t.cardio), &[a], true, NOW)
        .unwrap();
    let changed = t
        .store
        .set_cards_suspended(t.user, Some(t.cardio), &[a, b, b], true, NOW)
        .unwrap();
    assert_eq!(changed, vec![b], "a was archived already; b once");
}

#[test]
fn deleting_many_takes_each_once() {
    let t = tree();
    let a = add(&t.store, t.user, t.cardio, "a", &[]);
    let b = add(&t.store, t.user, t.cardio, "b", &[]);
    let n = t
        .store
        .delete_cards(t.user, Some(t.cardio), &[a, b, a], NOW)
        .unwrap();
    assert_eq!(n, 2);
    assert!(t.store.get_card(t.user, a).unwrap().is_none());
    assert!(t.store.get_card(t.user, b).unwrap().is_none());
}

fn cloze_note(store: &Store, user: UserId, deck: DeckId) -> Vec<CardId> {
    let text = "{{c1::Paris}} is in {{c2::France}}";
    let front = Box::leak(Box::new(sanitize_with_media(text)));
    let back = Box::leak(Box::new(sanitize_with_media("")));
    let generated = flash_store::notes::generate_cards(NoteType::Cloze, front, back).unwrap();
    let (_, ids) = store
        .create_note(
            user,
            deck,
            &NoteSave {
                note_type: NoteType::Cloze,
                front_html: front,
                back_html: back,
                generated: &generated,
                tags: &[],
                cap: None,
                origin: None,
                speech: Default::default(),
                langs: None,
                now_ms: NOW,
            },
        )
        .unwrap();
    ids
}

#[test]
fn a_move_takes_the_notes_other_cards_and_undo_puts_them_back() {
    let t = tree();
    let siblings = cloze_note(&t.store, t.user, t.cardio);
    let state_before = t.store.get_card_state(t.user, siblings[1]).unwrap();
    let moved = t
        .store
        .move_cards(t.user, Some(t.cardio), &siblings[..1], t.murmurs, NOW)
        .unwrap();
    assert_eq!(moved.len(), 2, "both cloze cards moved: {moved:?}");
    for id in &siblings {
        assert_eq!(
            t.store.get_card(t.user, *id).unwrap().unwrap().deck_id,
            t.murmurs
        );
    }
    let note = t
        .store
        .get_card(t.user, siblings[0])
        .unwrap()
        .unwrap()
        .note_id
        .unwrap();
    assert_eq!(
        t.store.get_note(t.user, note).unwrap().unwrap().deck_id,
        t.murmurs
    );
    assert_eq!(
        t.store.get_card_state(t.user, siblings[1]).unwrap(),
        state_before,
        "a move keeps the schedule"
    );

    t.store.return_cards(t.user, &moved, NOW).unwrap();
    for id in &siblings {
        assert_eq!(
            t.store.get_card(t.user, *id).unwrap().unwrap().deck_id,
            t.cardio
        );
    }
}

#[test]
fn a_move_into_an_archived_deck_is_refused() {
    let t = tree();
    let a = add(&t.store, t.user, t.cardio, "a", &[]);
    let err = t
        .store
        .move_cards(t.user, Some(t.cardio), &[a], t.old, NOW)
        .unwrap_err();
    assert!(matches!(err, StoreError::Invalid(_)), "{err:?}");
}

#[test]
fn tags_go_on_every_card_of_a_note_and_undo_removes_only_what_was_added() {
    let t = tree();
    let siblings = cloze_note(&t.store, t.user, t.cardio);
    t.store
        .add_card_tags(t.user, Some(t.cardio), &siblings[1..], &tags(&["geo"]), NOW)
        .unwrap();
    let added = t
        .store
        .add_card_tags(
            t.user,
            Some(t.cardio),
            &siblings[..1],
            &tags(&[" GEO ", "capitals"]),
            NOW,
        )
        .unwrap();
    // geo was on both already; capitals is new on both.
    assert_eq!(added.len(), 2, "{added:?}");
    assert!(added.iter().all(|(_, tag)| tag == "capitals"));
    for id in &siblings {
        let card = t.store.get_card(t.user, *id).unwrap().unwrap();
        assert_eq!(card.tags, ["capitals", "geo"]);
    }
    t.store.unlink_card_tags(t.user, &added, NOW).unwrap();
    for id in &siblings {
        let card = t.store.get_card(t.user, *id).unwrap().unwrap();
        assert_eq!(card.tags, ["geo"], "the tag that was there stays");
    }
}

#[test]
fn removing_tags_returns_the_links_an_undo_puts_back() {
    let t = tree();
    let a = add(&t.store, t.user, t.cardio, "a", &["one", "two"]);
    let removed = t
        .store
        .remove_card_tags(t.user, Some(t.cardio), &[a], &tags(&["two", "three"]), NOW)
        .unwrap();
    assert_eq!(removed, vec![(a, "two".to_string())]);
    t.store.link_card_tags(t.user, &removed, NOW).unwrap();
    assert_eq!(
        t.store.get_card(t.user, a).unwrap().unwrap().tags,
        ["one", "two"]
    );
}

#[test]
fn a_tag_past_the_per_card_cap_is_refused_whole() {
    let t = tree();
    let many: Vec<String> = (0..flash_store::MAX_TAGS_PER_CARD)
        .map(|i| format!("t{i}"))
        .collect();
    let a = add(&t.store, t.user, t.cardio, "a", &[]);
    t.store
        .add_card_tags(t.user, Some(t.cardio), &[a], &many, NOW)
        .unwrap();
    assert!(t
        .store
        .add_card_tags(t.user, Some(t.cardio), &[a], &tags(&["one-more"]), NOW)
        .is_err());
    assert_eq!(
        t.store.get_card(t.user, a).unwrap().unwrap().tags.len(),
        flash_store::MAX_TAGS_PER_CARD
    );
}

#[test]
fn card_counts_by_deck_count_each_decks_own_active_cards() {
    let t = tree();
    let counts = t.store.card_counts_by_deck(t.user, t.cardio).unwrap();
    assert_eq!(counts.get(&t.cardio), Some(&2));
    assert_eq!(counts.get(&t.murmurs), Some(&1));
}

#[test]
fn a_decks_export_holds_its_tree_named_from_the_deck() {
    let t = tree();
    t.store.create_deck(t.user, "Pharm", "", NOW).unwrap();
    let cards = t.store.export_cards_in(t.user, Some(t.murmurs)).unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].deck, "Murmurs");
    assert_eq!(cards[0].tags, ["exam-2", "murmurs"]);
    let whole = t.store.export_cards_in(t.user, Some(t.cardio)).unwrap();
    let mut decks: Vec<&str> = whole.iter().map(|c| c.deck.as_str()).collect();
    decks.sort();
    decks.dedup();
    assert_eq!(decks, ["Cardio", "Cardio::Murmurs", "Cardio::Old"]);
}
