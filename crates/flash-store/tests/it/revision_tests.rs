//! Note revisions: an edit that changes a note keeps the text it
//! replaced, a save that changes nothing keeps nothing, a note keeps its
//! newest twenty and nothing past ninety days, the first edit of a
//! note-less card keeps the card's own text, a restore puts a version
//! back in place (same cards, same schedule, same review log) and is
//! itself undoable, and revisions go with their note and their account.

use flash_core::{validate_card_text, CardId, NoteId, Rating, RevisionId, Scheduler, UserId};
use flash_store::notes::{GeneratedCard, NoteType};
use flash_store::richtext::{sanitize_with_media, SanitizedHtml};
use flash_store::{
    CardExtras, NoteSave, RevisionSource, Store, StoreError, REVISIONS_KEPT, REVISION_MAX_AGE_MS,
};

const NOW: i64 = 1_700_000_000_000;

/// Sanitized and leaked, as in `notes_tests`: a `NoteSave` borrows its
/// fields for the test's lifetime.
fn sanitized(html: &str) -> &'static SanitizedHtml {
    Box::leak(Box::new(sanitize_with_media(html)))
}

fn generated(kind: NoteType, front: &str, back: &str) -> &'static [GeneratedCard] {
    let cards = flash_store::notes::generate_cards(kind, sanitized(front), sanitized(back))
        .expect("valid note");
    Box::leak(cards.into_boxed_slice())
}

fn leaked_tags(list: &[&str]) -> &'static [String] {
    Box::leak(
        list.iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn save(kind: NoteType, front: &str, back: &str, tags: &[&str], now: i64) -> NoteSave<'static> {
    NoteSave {
        note_type: kind,
        front_html: sanitized(front),
        back_html: sanitized(back),
        generated: generated(kind, front, back),
        tags: leaked_tags(tags),
        cap: None,
        origin: None,
        speech: Default::default(),
        langs: None,
        now_ms: now,
    }
}

fn store_with_user() -> (Store, UserId) {
    let store = Store::open_in_memory().expect("open + migrate");
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    (store, user)
}

/// A Basic note "Q" / "A" tagged `one`, and its card.
fn basic_note(store: &Store, user: UserId) -> (NoteId, CardId) {
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let (note, ids) = store
        .create_note(user, deck, &save(NoteType::Basic, "Q", "A", &["one"], NOW))
        .unwrap();
    (note, ids[0])
}

fn count(store: &Store, sql: &str) -> i64 {
    store
        .with_conn(|conn| Ok(conn.query_row(sql, [], |r| r.get(0))?))
        .unwrap()
}

#[test]
fn an_edit_keeps_what_the_note_said_and_an_identical_save_keeps_nothing() {
    let (store, user) = store_with_user();
    let (note, _) = basic_note(&store, user);
    assert!(store.list_note_revisions(user, note).unwrap().is_empty());

    let edit = store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "<b>Q2</b>", "A", &["one"], NOW + 10),
            RevisionSource::Mcp,
        )
        .unwrap();
    let kept = store.list_note_revisions(user, note).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(edit.revision, Some(kept[0].id));
    let was = &kept[0];
    assert_eq!(
        (
            was.note_type,
            was.front_html.as_str(),
            was.back_html.as_str()
        ),
        (NoteType::Basic, "Q", "A")
    );
    assert_eq!(was.tags, vec!["one"]);
    assert_eq!(was.source, RevisionSource::Mcp);
    assert_eq!(was.at, NOW + 10);
    assert_eq!(
        store.note_revision(user, was.id).unwrap().as_ref(),
        Some(was)
    );

    // The same text again, tags recased and reordered: nothing changed.
    let same = store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "<b>Q2</b>", "A", &[" ONE "], NOW + 20),
            RevisionSource::Web,
        )
        .unwrap();
    assert_eq!(same.revision, None);
    assert_eq!(store.list_note_revisions(user, note).unwrap().len(), 1);

    // A tag change alone is a change, and so is a type change; newest
    // first.
    store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "<b>Q2</b>", "A", &["one", "two"], NOW + 30),
            RevisionSource::Api,
        )
        .unwrap();
    store
        .update_note(
            user,
            note,
            &save(
                NoteType::BasicTyped,
                "<b>Q2</b>",
                "A",
                &["one", "two"],
                NOW + 40,
            ),
            RevisionSource::Sync,
        )
        .unwrap();
    let kept = store.list_note_revisions(user, note).unwrap();
    let sources: Vec<_> = kept.iter().map(|r| r.source).collect();
    assert_eq!(
        sources,
        [
            RevisionSource::Sync,
            RevisionSource::Api,
            RevisionSource::Mcp
        ]
    );
    assert_eq!(kept[0].tags, vec!["one", "two"]);
    assert_eq!(kept[1].tags, vec!["one"]);
    assert_eq!(kept[0].note_type, NoteType::Basic);
}

#[test]
fn a_note_keeps_its_newest_twenty_and_nothing_past_ninety_days() {
    let (store, user) = store_with_user();
    let (note, _) = basic_note(&store, user);
    for i in 0..(REVISIONS_KEPT as i64 + 5) {
        store
            .update_note(
                user,
                note,
                &save(NoteType::Basic, &format!("Q{i}"), "A", &["one"], NOW + i),
                RevisionSource::Web,
            )
            .unwrap();
    }
    let kept = store.list_note_revisions(user, note).unwrap();
    assert_eq!(kept.len(), REVISIONS_KEPT);
    // The newest is what the last edit replaced; the oldest five went.
    assert_eq!(kept[0].front_html, format!("Q{}", REVISIONS_KEPT + 3));
    assert_eq!(kept[REVISIONS_KEPT - 1].front_html, "Q4");

    // An edit ninety days and a moment later drops every one of them.
    store
        .update_note(
            user,
            note,
            &save(
                NoteType::Basic,
                "later",
                "A",
                &["one"],
                NOW + 30 + REVISION_MAX_AGE_MS,
            ),
            RevisionSource::Web,
        )
        .unwrap();
    let kept = store.list_note_revisions(user, note).unwrap();
    assert_eq!(kept.len(), 1, "only the revision this edit wrote");
    assert_eq!(kept[0].front_html, format!("Q{}", REVISIONS_KEPT + 4));
}

#[test]
fn the_first_edit_of_a_note_less_card_keeps_the_cards_own_text() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[(
                validate_card_text("old front", "old back").unwrap(),
                vec!["t".into()],
            )],
            None,
            NOW,
        )
        .unwrap();
    // The caller adopts with what the card says now (the editor's seed),
    // then saves the new text over it.
    let note = store
        .adopt_card_into_note(
            user,
            ids[0],
            NoteType::Basic,
            sanitized("old front"),
            sanitized("old back"),
            NOW,
        )
        .unwrap();
    let edit = store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "new front", "old back", &["t"], NOW + 1),
            RevisionSource::Mcp,
        )
        .unwrap();
    assert_eq!(edit.cards, ids);
    let kept = store.list_note_revisions(user, note).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].front_html, "old front");
    assert_eq!(kept[0].back_html, "old back");
    assert_eq!(kept[0].tags, vec!["t"]);
}

#[test]
fn an_imported_cloze_cards_source_is_kept_on_its_first_edit() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let source = "{{c1::A}} {{c2::B}}\u{1f}";
    let ids = store
        .create_cards(
            user,
            deck,
            &[
                (validate_card_text("[...] B", "A").unwrap(), vec![]),
                (validate_card_text("A [...]", "B").unwrap(), vec![]),
            ],
            None,
            NOW,
        )
        .unwrap();
    let extras: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            (
                *id,
                CardExtras {
                    cloze_text: Some(source.to_string()),
                    cloze_index: Some(i as u32 + 1),
                    ..CardExtras::default()
                },
            )
        })
        .collect();
    store.set_card_extras(user, &extras).unwrap();
    let note = store
        .adopt_card_into_note(
            user,
            ids[0],
            NoteType::Cloze,
            sanitized("{{c1::A}} {{c2::B}}"),
            sanitized(""),
            NOW,
        )
        .unwrap();
    let edit = store
        .update_note(
            user,
            note,
            &save(NoteType::Cloze, "{{c1::A2}} {{c2::B}}", "", &[], NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    assert_eq!(edit.cards, ids, "both siblings rewritten in place");
    let kept = store.list_note_revisions(user, note).unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].note_type, NoteType::Cloze);
    assert_eq!(kept[0].front_html, "{{c1::A}} {{c2::B}}");
}

#[test]
fn a_restore_puts_the_text_back_on_the_same_cards_and_is_undoable() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let (note, ids) = store
        .create_note(
            user,
            deck,
            &save(NoteType::BasicReversed, "Q", "A", &["one"], NOW),
        )
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(user, ids[0]).unwrap(),
            Rating::Good,
            NOW,
        )
        .unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    let state = store.get_card_state(user, ids[0]).unwrap();

    store
        .update_note(
            user,
            note,
            &save(
                NoteType::BasicReversed,
                "Wrong",
                "Worse",
                &["two"],
                NOW + 10,
            ),
            RevisionSource::Mcp,
        )
        .unwrap();
    let kept = store.list_note_revisions(user, note).unwrap()[0].id;

    let restored = store
        .restore_note_revision(user, kept, None, NOW + 20)
        .unwrap();
    assert_eq!(restored.note, note);
    assert_eq!(restored.update.cards, ids, "the same cards, in place");
    assert_eq!(restored.created, 0);
    assert_eq!(restored.deck_totals, vec![(deck.0, 2)]);
    let card = store.get_card(user, ids[0]).unwrap().unwrap();
    assert_eq!((card.front.as_str(), card.back.as_str()), ("Q", "A"));
    assert_eq!(card.tags, vec!["one"]);
    assert_eq!(store.get_card_state(user, ids[0]).unwrap(), state);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM review_log"), 1);

    // The text the restore replaced is a revision too, by `restore`.
    let history = store.list_note_revisions(user, note).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].source, RevisionSource::Restore);
    assert_eq!(history[0].front_html, "Wrong");
    assert_eq!(restored.update.revision, Some(history[0].id));
    // Restoring the same version again changes nothing.
    let again = store
        .restore_note_revision(user, kept, None, NOW + 30)
        .unwrap();
    assert_eq!(again.update.revision, None);
    assert_eq!(store.list_note_revisions(user, note).unwrap().len(), 2);
}

#[test]
fn a_restore_that_would_drop_cards_or_is_not_the_users_is_refused() {
    let (store, user) = store_with_user();
    let other = store
        .create_user("Other", Some("other@example.com"), "member", NOW)
        .unwrap();
    let (note, card) = basic_note(&store, user);
    // Basic, then reversed: the Basic version would drop the second card.
    store
        .update_note(
            user,
            note,
            &save(NoteType::BasicReversed, "Q", "A", &["one"], NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    let basic = store.list_note_revisions(user, note).unwrap()[0].id;
    let err = store
        .restore_note_revision(user, basic, None, NOW + 2)
        .unwrap_err();
    assert!(err.to_string().contains("implicit_card_removal"), "{err}");
    assert_eq!(store.get_note(user, note).unwrap().unwrap().cards.len(), 2);

    // Another user can neither read nor restore it.
    assert!(store.list_note_revisions(other, note).unwrap().is_empty());
    assert_eq!(store.note_revision(other, basic).unwrap(), None);
    assert!(matches!(
        store.restore_note_revision(other, basic, None, NOW + 3),
        Err(StoreError::NotFound("revision"))
    ));
    assert!(matches!(
        store.restore_note_revision(user, RevisionId(9_999), None, NOW + 3),
        Err(StoreError::NotFound("revision"))
    ));
    assert_eq!(
        store.get_card(user, card).unwrap().unwrap().front,
        "Q",
        "nothing changed"
    );
}

#[test]
fn a_restore_that_adds_a_card_meets_the_cap() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let (note, _) = store
        .create_note(
            user,
            deck,
            &save(NoteType::BasicReversed, "Q", "A", &[], NOW),
        )
        .unwrap();
    // Reversed back to Basic needs the second card deleted first, which
    // the editor does by itself: its type change soft-deletes the slot.
    store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "Q", "A", &[], NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    let reversed = store.list_note_revisions(user, note).unwrap()[0].id;
    assert!(matches!(
        store.restore_note_revision(user, reversed, Some(1), NOW + 2),
        Err(StoreError::CapExceeded { current: 1, cap: 1 })
    ));
    let restored = store
        .restore_note_revision(user, reversed, None, NOW + 3)
        .unwrap();
    assert_eq!(restored.created, 1);
    assert_eq!(restored.update.cards.len(), 2);
}

#[test]
fn revisions_go_with_their_deck_and_their_account() {
    let (store, user) = store_with_user();
    let (note, _) = basic_note(&store, user);
    store
        .update_note(
            user,
            note,
            &save(NoteType::Basic, "Q2", "A", &["one"], NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    let deck = store.create_deck(user, "Gone", "", NOW).unwrap();
    let (doomed, _) = store
        .create_note(user, deck, &save(NoteType::Basic, "X", "Y", &[], NOW))
        .unwrap();
    store
        .update_note(
            user,
            doomed,
            &save(NoteType::Basic, "X2", "Y", &[], NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM note_revisions"), 2);

    store.delete_deck(user, deck).unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM note_revisions"), 1);

    store.delete_user_account(user).unwrap();
    assert_eq!(count(&store, "SELECT COUNT(*) FROM note_revisions"), 0);
}
