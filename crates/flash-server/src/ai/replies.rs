//! From the JSON a model wrote to the typed reply the pipeline takes,
//! the same for every provider: a reply is held to its schema here,
//! and what the provider billed rides along whether it parsed or not.

use serde_json::Value;

use super::provider::{
    DialogState, DraftNote, Extraction, LlmError, Reply, Revision, SpokenIntent, SpokenTurn, Usage,
};
use super::validate;

/// A parsed reply with what it cost, or the parse failure carrying the
/// same cost: the provider billed the exchange either way.
pub fn reply<T>(parsed: Result<T, LlmError>, usage: Usage) -> Result<Reply<T>, LlmError> {
    match parsed {
        Ok(value) => Ok(Reply::new(value, usage)),
        Err(e) => Err(e.billed(usage)),
    }
}

fn schema_err(e: impl std::fmt::Display) -> LlmError {
    LlmError::schema(e.to_string())
}

pub fn extraction_from(value: Value) -> Result<Extraction, LlmError> {
    let raw: Extraction = serde_json::from_value(value).map_err(schema_err)?;
    Ok(validate::extraction(raw))
}

/// `{ cards: [...] }` or a bare list. A card that does not parse is
/// skipped rather than failing the batch: one bad type name should not
/// cost the person the other fifty-nine.
pub fn drafts_from(value: Value) -> Result<Vec<DraftNote>, LlmError> {
    let list = match value {
        Value::Array(items) => items,
        Value::Object(mut obj) => match obj.remove("cards") {
            Some(Value::Array(items)) => items,
            _ => return Err(LlmError::schema("no cards list")),
        },
        _ => return Err(LlmError::schema("cards reply is not an object")),
    };
    Ok(list
        .into_iter()
        .filter_map(|item| serde_json::from_value::<DraftNote>(item).ok())
        .collect())
}

/// `{ cards: [...], reply: "..." }`: the sentence, then the cards read
/// the way every card list is (a bad item skipped, not fatal).
pub fn revision_from(mut value: Value) -> Result<Revision, LlmError> {
    let reply = value
        .as_object_mut()
        .and_then(|obj| obj.remove("reply"))
        .and_then(|r| r.as_str().map(str::to_string))
        .unwrap_or_default();
    Ok(Revision {
        reply,
        cards: drafts_from(value)?,
    })
}

/// The model's turn, held to the state it was asked in: an intent that
/// state cannot mean is `unclear`, a grade without a rating too; an
/// answer's rating is pulled into 1 to 4 and needs its feedback, a
/// question needs its answer, and the spoken text falls back to the
/// screen text. Every intent carries only its own fields.
pub fn turn_from(value: Value, state: DialogState) -> Result<SpokenTurn, LlmError> {
    let mut turn: SpokenTurn = serde_json::from_value(value).map_err(schema_err)?;
    if !state.allows(turn.intent) {
        turn.intent = SpokenIntent::Unclear;
    }
    let rating = turn.rating.filter(|r| *r != 0);
    let mut kept = SpokenTurn::bare(turn.intent);
    match turn.intent {
        SpokenIntent::Answer => {
            if turn.feedback.trim().is_empty() && turn.spoken_feedback.trim().is_empty() {
                return Err(LlmError::schema("answer without feedback"));
            }
            kept.rating = Some(rating.unwrap_or(1).clamp(1, 4));
            kept.spoken_feedback = if turn.spoken_feedback.trim().is_empty() {
                turn.feedback.clone()
            } else {
                turn.spoken_feedback
            };
            kept.feedback = turn.feedback;
        }
        SpokenIntent::Question => {
            if turn.answer.trim().is_empty() && turn.spoken_answer.trim().is_empty() {
                return Err(LlmError::schema("question without an answer"));
            }
            kept.spoken_answer = if turn.spoken_answer.trim().is_empty() {
                turn.answer.clone()
            } else {
                turn.spoken_answer
            };
            kept.answer = turn.answer;
        }
        SpokenIntent::Grade => match rating {
            Some(r @ 1..=4) => kept.rating = Some(r),
            _ => kept.intent = SpokenIntent::Unclear,
        },
        SpokenIntent::Repeat | SpokenIntent::Skip | SpokenIntent::Undo | SpokenIntent::Unclear => {}
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::FigureRef;
    use flash_store::notes::NoteType;
    use serde_json::json;

    #[test]
    fn an_extraction_is_validated_as_it_is_read() {
        let extraction = extraction_from(json!({
            "text": "The heart has four chambers.",
            "figures": [
                { "n": 1, "page": 2, "bbox": [0.1, 0.2, 0.9, 0.6], "caption": "Chambers" },
                { "n": 2, "page": 2, "bbox": [0.5, 0.5, 0.5, 0.9], "caption": "Empty" }
            ],
            "est_words": 5
        }))
        .unwrap();
        assert_eq!(extraction.text, "The heart has four chambers.");
        assert_eq!(
            extraction.figures,
            vec![FigureRef {
                n: 1,
                source: 1,
                page: 2,
                bbox: [0.1, 0.2, 0.9, 0.6],
                caption: "Chambers".into()
            }],
            "a zero-width figure is dropped"
        );
        assert_eq!(extraction.est_words, 5);
        assert!(extraction_from(json!({ "figures": [] })).is_err());
        // The billed usage rides on a parse failure and on a success alike.
        let usage = Usage::text(40, 0);
        let err = reply(extraction_from(json!({})), usage.clone()).unwrap_err();
        assert_eq!(err.usage(), Some(&usage));
        let ok = reply(extraction_from(json!({ "text": "x" })), usage.clone()).unwrap();
        assert_eq!(ok.usage, usage);
    }

    #[test]
    fn card_replies_skip_bad_items_and_voice_replies_are_bounded() {
        let drafts = drafts_from(json!({ "cards": [
            { "note_type": "basic", "front": "Q", "back": "A", "tags": [], "figures": [] },
            { "note_type": "poem", "front": "Q2", "back": "A2" },
            { "note_type": "cloze", "front": "{{c1::x}}" }
        ] }))
        .unwrap();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[1].note_type, NoteType::Cloze);
        assert_eq!(drafts_from(json!([])).unwrap().len(), 0);
        assert!(drafts_from(json!({ "notcards": 1 })).is_err());

        let revision = revision_from(json!({ "cards": [
            { "note_type": "basic", "front": "Q", "back": "A" },
            { "note_type": "poem", "front": "Q2", "back": "A2" }
        ], "reply": "Rewrote one card." }))
        .unwrap();
        assert_eq!(revision.cards.len(), 1);
        assert_eq!(revision.reply, "Rewrote one card.");
        let quiet = revision_from(json!({ "cards": [] })).unwrap();
        assert!(quiet.reply.is_empty() && quiet.cards.is_empty());
        assert!(
            revision_from(json!({ "reply": "x" })).is_err(),
            "no cards list"
        );

        let front = DialogState::Front;
        let window = DialogState::Window;
        // An answer: the rating is pulled into range, the spoken text
        // falls back to the screen text, and feedback is required.
        let answer = turn_from(
            json!({ "intent": "answer", "rating": 9, "feedback": "Right.", "spoken_feedback": "",
                    "answer": "stray", "spoken_answer": "" }),
            front,
        )
        .unwrap();
        assert_eq!(answer.intent, SpokenIntent::Answer);
        assert_eq!(answer.rating, Some(4));
        assert_eq!(answer.spoken_feedback, "Right.");
        assert_eq!(answer.answer, "", "only the intent's own fields survive");
        assert_eq!(
            turn_from(
                json!({ "intent": "answer", "rating": 0, "feedback": "x" }),
                front
            )
            .unwrap()
            .rating,
            Some(1)
        );
        assert!(turn_from(json!({ "intent": "answer", "rating": 3 }), front).is_err());
        // An answer where none is expected is unclear.
        assert_eq!(
            turn_from(
                json!({ "intent": "answer", "rating": 3, "feedback": "x" }),
                window
            )
            .unwrap(),
            SpokenTurn::bare(SpokenIntent::Unclear)
        );
        // A question needs its answer; the spoken one falls back.
        let question = turn_from(
            json!({ "intent": "question", "rating": 0, "answer": "Because.", "spoken_answer": "" }),
            window,
        )
        .unwrap();
        assert_eq!(question.spoken_answer, "Because.");
        assert_eq!(question.rating, None);
        assert!(turn_from(json!({ "intent": "question" }), front).is_err());
        // A grade keeps its rating in range and is unclear without one.
        assert_eq!(
            turn_from(json!({ "intent": "grade", "rating": 2 }), window).unwrap(),
            SpokenTurn {
                rating: Some(2),
                ..SpokenTurn::bare(SpokenIntent::Grade)
            }
        );
        assert_eq!(
            turn_from(json!({ "intent": "grade", "rating": 0 }), front)
                .unwrap()
                .intent,
            SpokenIntent::Unclear
        );
        // The bare intents carry nothing, and undo before a verdict is unclear.
        assert_eq!(
            turn_from(
                json!({ "intent": "repeat", "rating": 3, "feedback": "noise" }),
                front
            )
            .unwrap(),
            SpokenTurn::bare(SpokenIntent::Repeat)
        );
        assert_eq!(
            turn_from(json!({ "intent": "undo" }), front)
                .unwrap()
                .intent,
            SpokenIntent::Unclear
        );
        assert_eq!(
            turn_from(json!({ "intent": "undo" }), window)
                .unwrap()
                .intent,
            SpokenIntent::Undo
        );
        assert!(turn_from(json!({ "intent": "dance" }), front).is_err());
    }
}
