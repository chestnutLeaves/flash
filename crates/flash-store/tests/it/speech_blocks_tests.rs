//! Speech blocks: how each side reads aloud, and in which language.
//! A note keeps the blocks its writer gave its fields, each of its cards
//! derives its own, and a field whose words change loses them.

use flash_core::speech::SpeechBlock;
use flash_core::{validate_card_text, CardId, DeckId, NoteId, UserId};
use flash_store::notes::{generate_cards, NoteType};
use flash_store::richtext::{sanitize_with_media, SanitizedHtml};
use flash_store::{NoteSave, NoteSpeech, RevisionSource, Store};

const NOW: i64 = 1_700_000_000_000;

fn say(lang: &str, text: &str) -> SpeechBlock {
    SpeechBlock::Say {
        lang: lang.into(),
        text: text.into(),
    }
}

fn setup() -> (Store, UserId, DeckId) {
    let store = Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    let deck = store.create_deck(user, "Japanese", "", NOW).unwrap();
    (store, user, deck)
}

struct Fields {
    front: SanitizedHtml,
    back: SanitizedHtml,
    generated: Vec<flash_store::notes::GeneratedCard>,
}

fn fields(kind: NoteType, front: &str, back: &str) -> Fields {
    let front = sanitize_with_media(front).trimmed();
    let back = sanitize_with_media(back).trimmed();
    let generated = generate_cards(kind, &front, &back).unwrap();
    Fields {
        front,
        back,
        generated,
    }
}

fn save<'a>(kind: NoteType, f: &'a Fields, speech: NoteSpeech, now: i64) -> NoteSave<'a> {
    NoteSave {
        note_type: kind,
        front_html: &f.front,
        back_html: &f.back,
        generated: &f.generated,
        tags: &[],
        cap: None,
        origin: None,
        speech,
        langs: None,
        now_ms: now,
    }
}

fn speech_of(
    store: &Store,
    user: UserId,
    card: CardId,
) -> (Option<Vec<SpeechBlock>>, Option<Vec<SpeechBlock>>) {
    let row = store.get_card(user, card).unwrap().unwrap();
    (row.front_speech, row.back_speech)
}

fn speech_by(store: &Store, card: CardId) -> Option<String> {
    store
        .with_conn(|c| {
            Ok(
                c.query_row("SELECT speech_by FROM cards WHERE id = ?1", [card.0], |r| {
                    r.get(0)
                })?,
            )
        })
        .unwrap()
}

fn cat_note(store: &Store, user: UserId, deck: DeckId) -> (NoteId, Vec<CardId>, Fields) {
    let f = fields(
        NoteType::BasicReversed,
        "How do you say cat in Japanese?",
        "猫（ねこ）",
    );
    let speech = NoteSpeech {
        front: Some(vec![say("en", "How do you say cat in Japanese?")]),
        back: Some(vec![say("ja", "ねこ")]),
    };
    let (note, cards) = store
        .create_note(user, deck, &save(NoteType::BasicReversed, &f, speech, NOW))
        .unwrap();
    (note, cards, f)
}

#[test]
fn an_assistants_blocks_reach_every_card_of_the_note() {
    let (store, user, deck) = setup();
    let (_, cards, _) = cat_note(&store, user, deck);
    let (front, back) = speech_of(&store, user, cards[0]);
    assert_eq!(
        front.unwrap(),
        vec![say("en", "How do you say cat in Japanese?")]
    );
    assert_eq!(back.unwrap(), vec![say("ja", "ねこ")]);
    // The reversed card asks in Japanese.
    let (front, back) = speech_of(&store, user, cards[1]);
    assert_eq!(front.unwrap(), vec![say("ja", "ねこ")]);
    assert_eq!(
        back.unwrap(),
        vec![say("en", "How do you say cat in Japanese?")]
    );
    assert_eq!(speech_by(&store, cards[0]).as_deref(), Some("assistant"));
}

#[test]
fn a_field_keeps_its_blocks_until_its_words_change() {
    let (store, user, deck) = setup();
    let (note, cards, f) = cat_note(&store, user, deck);
    // A save that changes no words (a new tag, say) keeps every block.
    store
        .update_note(
            user,
            note,
            &save(NoteType::BasicReversed, &f, NoteSpeech::default(), NOW + 1),
            RevisionSource::Web,
        )
        .unwrap();
    let (front, back) = speech_of(&store, user, cards[0]);
    assert!(front.is_some() && back.is_some());
    // New words on the back: the back loses its blocks, the front keeps.
    let g = fields(
        NoteType::BasicReversed,
        "How do you say cat in Japanese?",
        "ねこ",
    );
    store
        .update_note(
            user,
            note,
            &save(NoteType::BasicReversed, &g, NoteSpeech::default(), NOW + 2),
            RevisionSource::Sync,
        )
        .unwrap();
    let (front, back) = speech_of(&store, user, cards[0]);
    assert!(front.is_some());
    assert_eq!(back, None);
    // An edit that brings its own blocks sets them.
    store
        .update_note(
            user,
            note,
            &save(
                NoteType::BasicReversed,
                &g,
                NoteSpeech {
                    front: None,
                    back: Some(vec![say("ja", "ねこ")]),
                },
                NOW + 3,
            ),
            RevisionSource::Mcp,
        )
        .unwrap();
    assert_eq!(
        speech_of(&store, user, cards[1]).0.unwrap(),
        vec![say("ja", "ねこ")]
    );
    // Blocks in a language nobody speaks are refused with the edit.
    let err = store
        .update_note(
            user,
            note,
            &save(
                NoteType::BasicReversed,
                &g,
                NoteSpeech {
                    front: Some(vec![say("klingon", "x")]),
                    back: None,
                },
                NOW + 4,
            ),
            RevisionSource::Mcp,
        )
        .unwrap_err();
    assert!(err.to_string().contains("unknown speech language"), "{err}");
}

#[test]
fn a_plain_edit_clears_only_the_side_it_rewrote() {
    let (store, user, deck) = setup();
    let ids = store
        .create_cards(
            user,
            deck,
            &[(validate_card_text("perro", "dog").unwrap(), vec![])],
            None,
            NOW,
        )
        .unwrap();
    store
        .with_conn(|c| {
            c.execute(
                "UPDATE cards SET front_speech = ?1, back_speech = ?2, speech_by = ?3
                 WHERE id = ?4",
                rusqlite::params![
                    flash_core::speech::to_json(&[say("es", "perro")]),
                    flash_core::speech::to_json(&[say("en", "dog")]),
                    flash_store::SPEECH_BY_ASSISTANT,
                    ids[0].0
                ],
            )?;
            Ok(())
        })
        .unwrap();
    store
        .update_card(
            user,
            ids[0],
            &validate_card_text("perro", "a dog").unwrap(),
            NOW + 1,
        )
        .unwrap();
    let (front, back) = speech_of(&store, user, ids[0]);
    assert_eq!(front.unwrap(), vec![say("es", "perro")]);
    assert_eq!(back, None);
    assert_eq!(
        speech_by(&store, ids[0]).as_deref(),
        Some(flash_store::SPEECH_BY_ASSISTANT)
    );
}

/// The language a person picks for each field: kept on the note,
/// checked, read by each card through its note (a reversed card's front
/// is the note's back), left alone by a save that says nothing of it,
/// and a field whose language changes loses its blocks.
#[test]
fn a_picked_language_is_kept_read_through_cards_and_clears_its_field() {
    use flash_store::{CardSide, NoteLangs};
    let (store, user, deck) = setup();
    let (note, cards, f) = cat_note(&store, user, deck);
    let picked = |front: Option<&str>, back: Option<&str>| {
        Some(NoteLangs {
            front: front.map(str::to_string),
            back: back.map(str::to_string),
        })
    };
    let mut edit = save(NoteType::BasicReversed, &f, NoteSpeech::default(), NOW + 1);
    edit.langs = picked(Some("en"), Some("ja"));
    store
        .update_note(user, note, &edit, RevisionSource::Web)
        .unwrap();
    let row = store.get_note(user, note).unwrap().unwrap();
    assert_eq!(row.langs, picked(Some("en"), Some("ja")).unwrap());
    // Picking a language for each field changed both: they lose their blocks.
    assert_eq!(speech_of(&store, user, cards[0]), (None, None));
    let lang = |card: CardId, side: CardSide| store.card_side_lang(user, card, side).unwrap();
    assert_eq!(lang(cards[0], CardSide::Front).as_deref(), Some("en"));
    assert_eq!(lang(cards[0], CardSide::Back).as_deref(), Some("ja"));
    assert_eq!(lang(cards[1], CardSide::Front).as_deref(), Some("ja"));
    assert_eq!(lang(cards[1], CardSide::Back).as_deref(), Some("en"));
    // A save that says nothing of them leaves them, and so the blocks the
    // same words were given.
    let mut resave = save(
        NoteType::BasicReversed,
        &f,
        NoteSpeech {
            front: Some(vec![say("en", "How do you say cat in Japanese?")]),
            back: Some(vec![say("ja", "ねこ")]),
        },
        NOW + 2,
    );
    resave.langs = None;
    store
        .update_note(user, note, &resave, RevisionSource::Web)
        .unwrap();
    let quiet = save(NoteType::BasicReversed, &f, NoteSpeech::default(), NOW + 3);
    store
        .update_note(user, note, &quiet, RevisionSource::Web)
        .unwrap();
    assert_eq!(
        store.get_note(user, note).unwrap().unwrap().langs,
        picked(Some("en"), Some("ja")).unwrap()
    );
    assert!(speech_of(&store, user, cards[0]).1.is_some());
    // Only the field whose language changed loses its blocks.
    let mut back_only = save(NoteType::BasicReversed, &f, NoteSpeech::default(), NOW + 4);
    back_only.langs = picked(Some("en"), Some("ja-JP"));
    store
        .update_note(user, note, &back_only, RevisionSource::Web)
        .unwrap();
    let (front, back) = speech_of(&store, user, cards[0]);
    assert!(front.is_some());
    assert!(back.is_none());
    // New words in a field drop its picked language, to be worked out
    // afresh from them; the field whose words stayed keeps its own.
    let reworded = fields(
        NoteType::BasicReversed,
        "Comment dit-on chat en japonais ?",
        "猫（ねこ）",
    );
    store
        .update_note(
            user,
            note,
            &save(
                NoteType::BasicReversed,
                &reworded,
                NoteSpeech::default(),
                NOW + 5,
            ),
            RevisionSource::Web,
        )
        .unwrap();
    assert_eq!(
        store.get_note(user, note).unwrap().unwrap().langs,
        picked(None, Some("ja-JP")).unwrap()
    );
    // Something that is not a language is refused.
    let mut bad = save(NoteType::BasicReversed, &f, NoteSpeech::default(), NOW + 6);
    bad.langs = picked(Some("klingon"), None);
    assert!(store
        .update_note(user, note, &bad, RevisionSource::Web)
        .is_err());
}
