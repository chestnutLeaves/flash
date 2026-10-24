//! A deck's language hints.

use flash_core::{CardText, UserId};
use flash_store::{Store, StoreError};

const NOW: i64 = 1_760_000_000_000;

fn store_with_user() -> (Store, UserId) {
    let store = Store::open_in_memory().unwrap();
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

#[test]
fn a_deck_keeps_its_hints_and_a_card_reads_its_own_decks() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "Français", "", NOW).unwrap();
    let other = store.create_deck(user, "Other", "", NOW).unwrap();
    assert!(store.deck_speech_hints(user, deck).unwrap().is_empty());
    store
        .set_deck_speech_hints(user, deck, &["fr".to_string(), "en".to_string()])
        .unwrap();
    assert_eq!(store.deck_speech_hints(user, deck).unwrap(), ["fr", "en"]);
    let text = CardText {
        front: "bonjour".into(),
        back: "hello".into(),
    };
    let card = store
        .create_cards(user, deck, &[(text, vec![])], None, NOW)
        .unwrap()[0];
    assert_eq!(store.card_speech_hints(user, card).unwrap(), ["fr", "en"]);
    assert!(store.deck_speech_hints(user, other).unwrap().is_empty());

    // Another account sees none of it.
    let stranger = store
        .create_user("Other", Some("other@example.com"), "member", NOW)
        .unwrap();
    assert!(matches!(
        store.card_speech_hints(stranger, card),
        Err(StoreError::NotFound("card"))
    ));
    assert!(matches!(
        store.set_deck_speech_hints(stranger, deck, &[]),
        Err(StoreError::NotFound("deck"))
    ));
}
