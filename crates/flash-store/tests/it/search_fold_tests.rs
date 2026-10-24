//! Card search ignores case in every alphabet and every accent: a term
//! finds a card however either is written.

use flash_core::validate_card_text;
use flash_store::Store;

const NOW: i64 = 1_700_000_000_000;

fn store_with(fronts: &[&str]) -> (Store, flash_core::UserId) {
    let store = Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    let deck = store.create_deck(user, "Words", "", NOW).unwrap();
    let cards: Vec<_> = fronts
        .iter()
        .map(|f| (validate_card_text(f, "back").unwrap(), Vec::new()))
        .collect();
    store.create_cards(user, deck, &cards, None, NOW).unwrap();
    (store, user)
}

fn found(store: &Store, user: flash_core::UserId, term: &str) -> Vec<String> {
    let mut fronts: Vec<String> = store
        .list_cards(user, None, Some(term), 50)
        .unwrap()
        .into_iter()
        .map(|c| c.front)
        .collect();
    fronts.sort();
    assert_eq!(
        store.count_cards(user, None, Some(term)).unwrap() as usize,
        fronts.len(),
        "the count agrees with the list for {term:?}"
    );
    fronts
}

#[test]
fn case_and_accents_never_hide_a_card() {
    let (store, user) = store_with(&[
        "Ñandú",
        "Corazón",
        "école primaire",
        "ΣΟΦΊΑ",
        "Привет",
        "Straße",
        "cafe\u{301} au lait",
        "100% pure",
        "snake_case",
    ]);
    assert_eq!(found(&store, user, "nandu"), ["Ñandú"]);
    assert_eq!(found(&store, user, "ÑANDÚ"), ["Ñandú"]);
    assert_eq!(found(&store, user, "corazon"), ["Corazón"]);
    assert_eq!(found(&store, user, "ECOLE"), ["école primaire"]);
    assert_eq!(found(&store, user, "σοφια"), ["ΣΟΦΊΑ"]);
    assert_eq!(found(&store, user, "привет"), ["Привет"]);
    assert_eq!(found(&store, user, "strasse"), ["Straße"]);
    // Written decomposed, the card is kept composed, and a term typed
    // either way finds it.
    assert_eq!(found(&store, user, "café"), ["café au lait"]);
    assert_eq!(found(&store, user, "cafe\u{301}"), ["café au lait"]);
    // `%` and `_` are what they are, not wildcards.
    assert_eq!(found(&store, user, "100%"), ["100% pure"]);
    assert_eq!(found(&store, user, "e_c"), ["snake_case"]);
    assert!(found(&store, user, "x%y").is_empty());
}
