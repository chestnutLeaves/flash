use crate::service::ServiceError;

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteCardsArgs {
    /// The cards to delete, 1-500 IDs from list_cards, e.g. [1042, 1043].
    #[serde(deserialize_with = "crate::bounds::cards")]
    pub card_ids: Vec<i64>,
    /// Omit on the first call; pass back the token the first call returned,
    /// after the user has confirmed.
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    pub confirm_token: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DeleteDeckArgs {
    /// The deck's full name, e.g. "Pharm::Cardio" (list_decks has the
    /// names).
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub deck: String,
    /// Omit on the first call; pass back the token the first call returned,
    /// after the user has confirmed.
    #[serde(default, deserialize_with = "crate::bounds::opt_token")]
    pub confirm_token: Option<String>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct SavedOut {
    pub saved: bool,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct StartSessionArgs {
    /// Deck name to study (omit to study everything due). A deck includes
    /// its subdecks; name Parent::Child to study one subdeck.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    pub deck: Option<String>,
    /// One tag to study, e.g. "exam-2": the same as a `tags` of one.
    #[serde(default, deserialize_with = "crate::bounds::opt_tag")]
    pub tag: Option<String>,
    /// Tags the cards must all carry, e.g. ["exam-2", "cardio"], at most
    /// ten. With a deck they narrow that deck. Names come from list_tags.
    #[serde(default, deserialize_with = "crate::bounds::opt_scope_tags")]
    pub tags: Option<Vec<String>>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ListTagsArgs {
    /// Deck whose tags to list, as a path (Parent::Child), with its
    /// subdecks. Omit for every deck.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    pub deck: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SubmitReviewArgs {
    /// The session_id start_study_session returned.
    pub session_id: i64,
    /// The card just asked: the card_id of the front the user answered.
    pub card_id: i64,
    /// 1=Again 2=Hard 3=Good 4=Easy
    pub rating: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct CardIdArgs {
    /// The card whose answer to show: the card_id of the front just asked.
    pub card_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SessionIdArgs {
    /// The session_id start_study_session returned.
    pub session_id: i64,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct CreateDeckArgs {
    /// The deck's full name, e.g. "Biology" or "Biology::Cell cycle" for a
    /// subdeck.
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub name: String,
    /// Optional: what the deck covers, in a sentence.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_description")]
    pub description: Option<String>,
    #[schemars(description = LANGUAGES_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_languages")]
    pub languages: Option<Vec<String>>,
    #[schemars(description = GOAL_DATE_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_goal_date")]
    pub goal_date: Option<String>,
}

const GOAL_DATE_NOTE: &str = "Optional: the date the user needs to know \
    this deck by (an exam, a test, a trip, a deadline), YYYY-MM-DD on their \
    calendar. Flash then spreads the deck's new cards so all are learned \
    before it and brings back, in the last days, the cards that need it.";

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct DeckNameArgs {
    /// The deck's full name, e.g. "Pharm::Cardio" (list_decks has the
    /// names).
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub deck: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameDeckArgs {
    /// The deck's current full name, e.g. "Cardio" or "Pharm::Cardio".
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub deck: String,
    /// Its new full name. "Pharm::Cardio" puts it inside Pharm; a name
    /// with no "::" puts it at the top level.
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub new_name: String,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct RenamedOut {
    pub renamed: bool,
    /// The deck's full name now.
    pub name: String,
    /// Every deck that has a new name: the deck and its subdecks.
    pub decks_renamed: u32,
}

// How to write a card that is good to learn from, for any subject: the
// minimum-information principle of spaced repetition, said where every
// client delivers it (a card field's schema description). create_cards
// and update_cards carry the same text from here.

const FRONT_HTML_NOTE: &str = "The prompt, answerable on its own: one \
question, cue or cloze sentence carrying everything needed to answer it \
(the choices of a multiple-choice question, the context a case depends \
on). One idea per card: a list, a multi-part rule or a comparison becomes \
several cards, or a cloze with one blank per part.";

const BACK_HTML_NOTE: &str = "The answer to exactly what the front asks, \
as short as it can be while complete. Anything more (why, an example, a \
source) comes after the answer, set apart.";

const CARD_TEXT_NOTE: &str = "On both sides keep the material's own terms, \
names and numbers. Write what plain text loses as HTML (10<sup>5</sup>, \
not 105; H<sub>2</sub>O; <br> between lines), and write each text in one \
script, as the material or the user writes it, never switching alphabets \
inside a word.";

const NOTE_TYPE_NOTE: &str = "basic (default): one question, one answer. \
basic_reversed: both directions are worth asking (a word and its \
translation, a term and its definition). basic_typed: the exact spelling, \
number or formula matters. cloze: a sentence whose blanks are each their \
own fact, {{c1::...}}, {{c2::...}}.";

const TAGS_NOTE: &str = "A few short lowercase topic words (a chapter, a \
lesson, a theme), so the user can study one part later.";

/// How to say what is read aloud of a side: the rules every writer of
/// speech blocks follows, given to an assistant through the schema.
pub const SPEECH_NOTE: &str = "What is read aloud of this side, \
in order, as blocks: [{\"lang\":\"en\",\"text\":\"How do you say cat in \
Japanese?\"},{\"lang\":\"ja\",\"text\":\"ねこ\"}]. Follow the side as it is \
written, in its order, never rewording it. Each block is one language, by \
ISO 639-1 code, with a region where the accent matters (es-MX, es-ES, \
pt-BR, pt-PT, en-GB); a word in another language inside a sentence is its \
own block where it stands. Leave out what is not read aloud: citations and \
sources, a romanization next to the script it spells (romaji, pinyin), \
emoji, images. Write a word as it is said when its spelling is ambiguous \
(Japanese kanji in kana, 猫 as ねこ; Chinese in simplified characters). A \
short formula of a few symbols in words (F = ma as \"F equals m a\"); a \
long one as {\"say\":\"equation\"}; anything that cannot be said aloud, \
Cantonese included, as {\"say\":\"look\"}.";

const LANGUAGES_NOTE: &str = "The languages the deck's cards are in, most \
used first, by ISO 639-1 code (\"ja\", \"en\").";

/// One block of what is read aloud: words in one language, or a cue.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum SpeechBlockArg {
    Say(SayBlock),
    Cue(CueBlock),
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SayBlock {
    /// The language the words are in: an ISO 639-1 code, with a region
    /// where the accent matters ("es-MX").
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub lang: String,
    /// The words, as they are said.
    #[serde(deserialize_with = "crate::bounds::card_side")]
    pub text: String,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CueBlock {
    /// "equation" for a formula too long to read, "look" for anything
    /// that cannot be said aloud.
    #[serde(deserialize_with = "crate::bounds::keyword")]
    pub say: String,
}

/// A side's blocks as the store keeps them; an unknown cue is refused.
pub(super) fn speech_blocks(
    blocks: Option<Vec<SpeechBlockArg>>,
) -> Result<Option<Vec<flash_core::speech::SpeechBlock>>, ServiceError> {
    use flash_core::speech::{Cue, SpeechBlock};
    blocks
        .map(|list| {
            list.into_iter()
                .map(|b| match b {
                    SpeechBlockArg::Say(s) => Ok(SpeechBlock::Say {
                        lang: s.lang,
                        text: s.text,
                    }),
                    SpeechBlockArg::Cue(c) => match c.say.as_str() {
                        "equation" => Ok(SpeechBlock::Cue { say: Cue::Equation }),
                        "look" => Ok(SpeechBlock::Cue { say: Cue::Look }),
                        other => Err(ServiceError::Malformed(format!(
                            "unknown speech cue {other:?}: use \"equation\" or \"look\""
                        ))),
                    },
                })
                .collect()
        })
        .transpose()
}

/// The speech fields are asked of every card the schema describes, so
/// an assistant writes them as it writes the sides. The server takes a
/// card without them all the same.
fn speech_fields_required(schema: &mut schemars::Schema) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let required = object
        .entry("required")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(list) = required.as_array_mut() {
        for name in ["front_speech", "back_speech"] {
            if !list.iter().any(|v| v == name) {
                list.push(serde_json::Value::String(name.to_string()));
            }
        }
    }
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = speech_fields_required)]
pub struct NewCard {
    #[schemars(description = NOTE_TYPE_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_keyword")]
    pub note_type: Option<String>,
    // `front`/`back` were these fields' names before rich content, and a
    // client holding the earlier definition still sends them; remove both
    // aliases (and the test
    // `create_cards_still_takes_the_earlier_front_and_back_names`) once
    // none does. The aliases are not in the advertised schema.
    #[schemars(description = FRONT_HTML_NOTE)]
    #[serde(alias = "front", deserialize_with = "crate::bounds::field_html")]
    pub front_html: String,
    #[schemars(description = BACK_HTML_NOTE)]
    #[serde(alias = "back", deserialize_with = "crate::bounds::field_html")]
    pub back_html: String,
    #[schemars(description = TAGS_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::tags")]
    pub tags: Vec<String>,
    #[schemars(description = SPEECH_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_speech")]
    pub front_speech: Option<Vec<SpeechBlockArg>>,
    #[schemars(description = SPEECH_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_speech")]
    pub back_speech: Option<Vec<SpeechBlockArg>>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateCardsArgs {
    /// The deck to add to, by full name, e.g. "Spanish" or "Anatomy::Bones":
    /// an existing deck (list_decks has the names), or a new one, created
    /// with any missing parents.
    #[serde(deserialize_with = "crate::bounds::deck_name")]
    pub deck: String,
    #[schemars(description = CARD_TEXT_NOTE)]
    #[serde(deserialize_with = "crate::bounds::cards")]
    pub cards: Vec<NewCard>,
    #[schemars(description = LANGUAGES_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_languages")]
    pub languages: Option<Vec<String>>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardPatch {
    /// The card to change, from list_cards.
    pub card_id: i64,
    #[schemars(description = FRONT_HTML_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_field_html")]
    pub front_html: Option<String>,
    #[schemars(description = BACK_HTML_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_field_html")]
    pub back_html: Option<String>,
    #[schemars(description = TAGS_NOTE)]
    #[serde(default, deserialize_with = "crate::bounds::opt_tags")]
    pub tags: Option<Vec<String>>,
    /// Every card made from this card's source, this card included,
    /// exactly as list_cards shows them in source.sibling_card_ids. Needed
    /// when the source makes more than one card (a reversed pair, a cloze
    /// with several gaps).
    #[serde(default, deserialize_with = "crate::bounds::cards")]
    pub sibling_card_ids: Vec<i64>,
    /// With a front_html it changes: the new front's speech blocks (see
    /// create_cards).
    #[serde(default, deserialize_with = "crate::bounds::opt_speech")]
    pub front_speech: Option<Vec<SpeechBlockArg>>,
    /// With a back_html it changes: the new back's speech blocks.
    #[serde(default, deserialize_with = "crate::bounds::opt_speech")]
    pub back_speech: Option<Vec<SpeechBlockArg>>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateCardsArgs {
    #[schemars(description = CARD_TEXT_NOTE)]
    #[serde(deserialize_with = "crate::bounds::cards")]
    pub cards: Vec<CardPatch>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreCardsArgs {
    /// The card whose earlier versions to list or restore, from
    /// list_cards. A version covers the card's whole source, siblings
    /// included.
    pub card_id: i64,
    /// The version to put back: a revision_id this tool listed for the
    /// card, or one update_cards returned. Omit to list the versions.
    #[serde(default)]
    pub revision_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct ListCardsArgs {
    /// Filter to one deck by name.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    pub deck: Option<String>,
    /// Substring search over fronts and backs.
    #[serde(default, deserialize_with = "crate::bounds::opt_search")]
    pub search: Option<String>,
    /// Cards per page, 1-500; 50 when omitted.
    pub limit: Option<u32>,
    /// For the next page: the next_cursor the previous page returned.
    pub before_card_id: Option<i64>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct SetLimitsArgs {
    /// Deck name to set an override for; omit to set the account defaults.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    pub deck: Option<String>,
    /// New cards per day. Omit to leave unchanged; for a deck, -1 clears
    /// the override so it inherits the account default.
    pub new_per_day: Option<i64>,
    /// Max reviews per day. Omit to leave unchanged; for a deck, -1 clears
    /// the override so it inherits the account default.
    pub reviews_per_day: Option<i64>,
    /// With a deck only: the date the user needs to know it by (an exam, a
    /// test, a trip, a deadline), YYYY-MM-DD on their calendar. Flash paces
    /// the deck's new cards to finish before it and brings back, in the
    /// last days, the cards that need it. An empty string clears it; omit
    /// to leave it unchanged.
    #[serde(default, deserialize_with = "crate::bounds::opt_goal_date")]
    pub goal_date: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct BoostArgs {
    /// Deck name; omit to boost the account-wide allowance.
    #[serde(default, deserialize_with = "crate::bounds::opt_deck_name")]
    pub deck: Option<String>,
    /// Extra new cards to allow today (1-500). Today only.
    pub extra: u32,
}
