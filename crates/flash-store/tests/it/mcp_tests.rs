use flash_core::{CardText, DeckId, UserId};
use flash_store::Store;

fn seed(store: &Store) -> (UserId, DeckId, Vec<flash_core::CardId>) {
    let user = store.create_user("MCP", None, "member", 1).unwrap();
    let deck = store.create_deck(user, "Deck", "", 1).unwrap();
    let cards = store
        .create_cards(
            user,
            deck,
            &[
                (
                    CardText {
                        front: "Q".into(),
                        back: "A".into(),
                    },
                    vec![],
                ),
                (
                    CardText {
                        front: "Q2".into(),
                        back: "A2".into(),
                    },
                    vec![],
                ),
            ],
            None,
            1,
        )
        .unwrap();
    (user, deck, cards)
}

#[test]
fn confirmation_survives_reopen_and_completed_retry() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("flash-mcp-{}-{stamp}.db", std::process::id()));
    let store = Store::open(&path).unwrap();
    let (user, deck, cards) = seed(&store);
    store
        .stage_mcp_delete(user, "client", "selection", None, &cards, "hash", 10)
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    let result = store
        .confirm_mcp_delete(user, "client", "selection", "hash", 20)
        .unwrap();
    assert_eq!(result.card_ids.len(), 2);
    assert_eq!(result.deck_totals, vec![(deck.0, 0)]);
    drop(store);
    let store = Store::open(&path).unwrap();
    let retried = store
        .confirm_mcp_delete(user, "client", "selection", "hash", 700_000)
        .unwrap();
    assert_eq!(retried.card_ids, result.card_ids);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_expired_foreign_and_mismatched_tokens_are_distinct_and_do_not_delete() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    let other = store.create_user("Other", None, "member", 1).unwrap();
    store
        .stage_mcp_delete(user, "client", "selection", None, &cards, "hash", 10)
        .unwrap();
    for (who, client, target, hash, now, code) in [
        (
            user,
            "client",
            "selection",
            "unknown",
            20,
            "confirmation_invalid",
        ),
        (
            other,
            "client",
            "selection",
            "hash",
            20,
            "confirmation_invalid",
        ),
        (
            user,
            "different-client",
            "selection",
            "hash",
            20,
            "confirmation_invalid",
        ),
        (
            user,
            "client",
            "different-target",
            "hash",
            20,
            "confirmation_target_mismatch",
        ),
        (
            user,
            "client",
            "selection",
            "hash",
            600_010,
            "confirmation_expired",
        ),
    ] {
        assert_eq!(
            store
                .confirm_mcp_delete(who, client, target, hash, now)
                .unwrap_err()
                .to_string(),
            code
        );
        assert_eq!(store.count_cards(user, Some(deck), None).unwrap(), 2);
    }
    // A mismatch never consumes the rightful user's confirmation.
    assert_eq!(
        store
            .confirm_mcp_delete(user, "client", "selection", "hash", 30)
            .unwrap()
            .card_ids
            .len(),
        2
    );
}

#[test]
fn changed_targets_roll_back_the_whole_deletion_and_keep_confirmation() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    store
        .stage_mcp_delete(user, "client", "selection", None, &cards, "hash", 10)
        .unwrap();
    store.delete_card(user, cards[1], 11).unwrap();
    assert_eq!(
        store
            .confirm_mcp_delete(user, "client", "selection", "hash", 12)
            .unwrap_err()
            .to_string(),
        "confirmation_target_changed"
    );
    assert!(store.get_card(user, cards[0]).unwrap().is_some());
    assert_eq!(store.count_cards(user, Some(deck), None).unwrap(), 1);
    // The deck preview names the card the deck still shows; the trashed
    // row goes with the deck without being counted.
    let affected = store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "deck-hash", 13)
        .unwrap();
    assert_eq!(affected.card_ids, vec![cards[0].0]);
    let deleted = store
        .confirm_mcp_delete(user, "client", "deck", "deck-hash", 14)
        .unwrap();
    assert_eq!(deleted.card_ids, vec![cards[0].0]);
    assert!(store.find_deck_by_name(user, "Deck").unwrap().is_none());
}

#[test]
fn pending_confirmations_cannot_delete_replacements_with_reused_ids() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "deck-hash", 10)
        .unwrap();
    store
        .stage_mcp_delete(user, "client", "cards", None, &cards, "cards-hash", 10)
        .unwrap();
    store.delete_deck(user, deck).unwrap();
    // A deleted row's id is never given again (`repo::ids`), and the
    // name and timestamp can match; the deletion invalidates the approval
    // all the same, so nothing rests on the ids alone.
    let replacement = store.create_deck(user, "Deck", "", 1).unwrap();
    assert_ne!(replacement, deck);
    let replacements = store
        .create_cards(
            user,
            replacement,
            &[
                (
                    CardText {
                        front: "New Q".into(),
                        back: "New A".into(),
                    },
                    vec![],
                ),
                (
                    CardText {
                        front: "New Q2".into(),
                        back: "New A2".into(),
                    },
                    vec![],
                ),
            ],
            None,
            1,
        )
        .unwrap();
    assert!(replacements.iter().all(|c| !cards.contains(c)));
    for (target, hash) in [("deck", "deck-hash"), ("cards", "cards-hash")] {
        assert_eq!(
            store
                .confirm_mcp_delete(user, "client", target, hash, 20)
                .unwrap_err()
                .to_string(),
            "confirmation_target_changed"
        );
        assert_eq!(store.count_cards(user, Some(replacement), None).unwrap(), 2);
    }
}

#[test]
fn empty_deck_replacement_invalidates_approval_but_unrelated_deletion_does_not() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, _) = seed(&store);
    let empty = store.create_deck(user, "Empty", "", 1).unwrap();
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "deck-hash", 10)
        .unwrap();
    store
        .stage_mcp_delete(user, "client", "empty", Some(empty), &[], "empty-hash", 10)
        .unwrap();
    store.delete_deck(user, empty).unwrap();
    assert_ne!(store.create_deck(user, "Empty", "", 1).unwrap(), empty);
    assert_eq!(
        store
            .confirm_mcp_delete(user, "client", "empty", "empty-hash", 20)
            .unwrap_err()
            .to_string(),
        "confirmation_target_changed"
    );
    let result = store
        .confirm_mcp_delete(user, "client", "deck", "deck-hash", 20)
        .unwrap();
    assert_eq!(result.card_ids.len(), 2);
    // Invalidating pending approvals during a successful deck deletion must
    // not prevent storing or replaying that operation's completed receipt.
    assert_eq!(
        store
            .confirm_mcp_delete(user, "client", "deck", "deck-hash", 700_000)
            .unwrap()
            .card_ids,
        result.card_ids
    );
}

#[test]
fn a_deck_preview_and_receipt_name_only_the_cards_the_deck_shows() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    store.delete_card(user, cards[0], 5).unwrap();
    let shown = store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "hash", 10)
        .unwrap();
    assert_eq!(shown.card_ids, vec![cards[1].0]);
    assert_eq!(shown.decks, vec!["Deck"]);
    let receipt = store
        .confirm_mcp_delete(user, "client", "deck", "hash", 20)
        .unwrap();
    assert_eq!(receipt.card_ids, vec![cards[1].0]);
    assert_eq!(store.count_cards(user, None, None).unwrap(), 0);
}

fn one_card(store: &Store, user: UserId, deck: DeckId, front: &str) -> i64 {
    let text = CardText {
        front: front.into(),
        back: "back".into(),
    };
    store
        .create_cards(user, deck, &[(text, vec![])], None, 2)
        .unwrap()[0]
        .0
}

/// A deck goes with the decks under it: the preview and the receipt name
/// them and their cards, and the whole subtree is what is deleted.
#[test]
fn a_deck_deletion_asks_about_and_removes_the_decks_under_it() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    let inner = store.create_deck(user, "Deck::Inner", "", 2).unwrap();
    let innermost = store.create_deck(user, "Deck::Inner::Most", "", 2).unwrap();
    let beside = store.create_deck(user, "Deck2", "", 2).unwrap();
    let in_inner = one_card(&store, user, inner, "inner");
    let in_most = one_card(&store, user, innermost, "most");
    let kept = one_card(&store, user, beside, "beside");

    let shown = store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "hash", 10)
        .unwrap();
    assert_eq!(
        shown.decks,
        vec!["Deck", "Deck::Inner", "Deck::Inner::Most"]
    );
    assert_eq!(
        shown.card_ids,
        vec![cards[0].0, cards[1].0, in_inner, in_most]
    );
    let receipt = store
        .confirm_mcp_delete(user, "client", "deck", "hash", 20)
        .unwrap();
    assert_eq!(receipt.decks, shown.decks);
    assert_eq!(receipt.card_ids, shown.card_ids);
    assert_eq!(
        receipt.deck_totals,
        vec![(deck.0, 0), (inner.0, 0), (innermost.0, 0)]
    );
    let left: Vec<String> = store
        .list_decks(user, flash_core::queue::Due::exact(30))
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(left, vec!["Deck2"]);
    assert_eq!(store.count_cards(user, None, None).unwrap(), 1);
    assert!(store
        .get_card(user, flash_core::CardId(kept))
        .unwrap()
        .is_some());
    // The retry of a finished deletion says what it said.
    let retried = store
        .confirm_mcp_delete(user, "client", "deck", "hash", 700_000)
        .unwrap();
    assert_eq!(retried.decks, receipt.decks);
}

/// The approval pins the decks as well as the cards. A subdeck that
/// arrives afterwards may be empty, so no pinned card changes; a subdeck
/// that leaves takes none with it either. Both are a different deletion.
#[test]
fn a_deck_approval_does_not_stretch_to_a_subdeck_that_came_later() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, _) = seed(&store);
    let changed = |hash: &str| {
        assert_eq!(
            store
                .confirm_mcp_delete(user, "client", "deck", hash, 20)
                .unwrap_err()
                .to_string(),
            "confirmation_target_changed"
        );
        assert_eq!(store.count_cards(user, None, None).unwrap(), 2);
    };

    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "made", 10)
        .unwrap();
    let late = store.create_deck(user, "Deck::Late", "", 11).unwrap();
    changed("made");

    // Moved in from elsewhere, empty, after the approval.
    store.move_deck(user, late, None, 12).unwrap();
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "moved-in", 13)
        .unwrap();
    store.move_deck(user, late, Some(deck), 14).unwrap();
    changed("moved-in");

    // Moved out: what is left is less than what was approved.
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "moved-out", 15)
        .unwrap();
    store.move_deck(user, late, None, 16).unwrap();
    changed("moved-out");

    // Deleting a pinned subdeck by itself voids the approval for good,
    // even if a deck of that name comes back.
    store.move_deck(user, late, Some(deck), 17).unwrap();
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "deleted", 18)
        .unwrap();
    store.delete_deck(user, late).unwrap();
    store.create_deck(user, "Deck::Late", "", 19).unwrap();
    changed("deleted");
}

#[test]
fn a_deck_approval_covers_a_card_trashed_afterwards_but_not_a_new_one() {
    let store = Store::open_in_memory().unwrap();
    let (user, deck, cards) = seed(&store);
    store
        .stage_mcp_delete(user, "client", "deck", Some(deck), &[], "hash", 10)
        .unwrap();
    // Trashing shrinks what was approved; nothing unapproved is at stake.
    store.delete_card(user, cards[0], 15).unwrap();
    let receipt = store
        .confirm_mcp_delete(user, "client", "deck", "hash", 20)
        .unwrap();
    assert_eq!(receipt.card_ids, vec![cards[1].0]);

    let deck = store.create_deck(user, "Second", "", 1).unwrap();
    store
        .stage_mcp_delete(user, "client", "second", Some(deck), &[], "hash2", 30)
        .unwrap();
    let text = CardText {
        front: "late".into(),
        back: "arrival".into(),
    };
    store
        .create_cards(user, deck, &[(text, vec![])], None, 31)
        .unwrap();
    assert_eq!(
        store
            .confirm_mcp_delete(user, "client", "second", "hash2", 32)
            .unwrap_err()
            .to_string(),
        "confirmation_target_changed"
    );
}
