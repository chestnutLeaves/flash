//! The reply shapes the model is held to, as standard JSON Schema
//! (`type`, `properties`, `required`, `additionalProperties`, `items`,
//! `enum`, `description`), the subset every structured-output mode
//! takes. Every object lists all of its properties as required and
//! allows no others, which is what a strict mode demands; the order of
//! `required` is the order the model is steered to write in, for the
//! adapters that can use it. A provider whose API speaks its own
//! dialect converts from this form inside its own module. The server
//! still checks every value it receives (`validate`); the schema is
//! what makes the reply parse at all.

use serde_json::{json, Value};

fn string() -> Value {
    json!({ "type": "string" })
}

fn integer() -> Value {
    json!({ "type": "integer" })
}

fn array_of(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

/// An object whose every listed property is required, in the listed
/// order, and which admits no other.
fn object(props: &[(&str, Value)]) -> Value {
    let names: Vec<&str> = props.iter().map(|(k, _)| *k).collect();
    let properties: serde_json::Map<String, Value> = props
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": names,
        "additionalProperties": false,
    })
}

/// `Extraction`: text, figures with page and normalized bbox, word count.
pub fn extraction_schema() -> Value {
    object(&[
        ("text", string()),
        (
            "figures",
            array_of(object(&[
                ("n", integer()),
                (
                    "source",
                    json!({ "type": "integer", "description": "The number of the source the figure is in, counting from 1" }),
                ),
                (
                    "page",
                    json!({ "type": "integer", "description": "Page number within that source, counting from 1" }),
                ),
                ("bbox", array_of(json!({ "type": "number" }))),
                ("caption", string()),
            ])),
        ),
        ("est_words", integer()),
    ])
}

/// One `DraftNote`, as every card-writing reply lists them.
fn card_schema() -> Value {
    let note_types: Vec<&str> = flash_store::notes::NoteType::ALL
        .iter()
        .map(|t| t.as_str())
        .collect();
    object(&[
        ("note_type", json!({ "type": "string", "enum": note_types })),
        ("front", string()),
        ("back", string()),
        ("tags", array_of(string())),
        ("figures", array_of(integer())),
    ])
}

/// `{ cards: DraftNote[] }`: the generate and more replies.
pub fn drafts_schema() -> Value {
    object(&[("cards", array_of(card_schema()))])
}

/// `Revision`: the new open set, then the sentence about it (written
/// after the cards, so it describes what was actually done).
pub fn revision_schema() -> Value {
    object(&[
        ("cards", array_of(card_schema())),
        (
            "reply",
            json!({ "type": "string", "description": "One short sentence saying what was changed" }),
        ),
    ])
}

/// `SpokenTurn`: the intent first, since it decides the rest; then the
/// rating and the texts each intent fills, empty for the others.
pub fn turn_schema() -> Value {
    let intents: Vec<&str> = super::provider::SpokenIntent::ALL
        .iter()
        .map(|i| i.as_str())
        .collect();
    object(&[
        (
            "intent",
            json!({ "type": "string", "enum": intents, "description": "What the student meant" }),
        ),
        (
            "rating",
            json!({ "type": "integer", "description": "1 to 4 for answer and grade; 0 for the other intents" }),
        ),
        ("feedback", string()),
        ("spoken_feedback", string()),
        ("answer", string()),
        ("spoken_answer", string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every node is standard JSON Schema in strict form: known keys
    /// only, lowercase types, every object closed with all of its
    /// properties required, every array with its items.
    fn check(v: &Value) {
        const KNOWN: &[&str] = &[
            "type",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "enum",
            "description",
        ];
        const TYPES: &[&str] = &["object", "array", "string", "integer", "number", "boolean"];
        let obj = v.as_object().expect("schema node is an object");
        for key in obj.keys() {
            assert!(KNOWN.contains(&key.as_str()), "unknown schema key {key}");
        }
        let t = obj["type"].as_str().unwrap();
        assert!(TYPES.contains(&t), "unknown type {t}");
        if t == "object" {
            assert_eq!(
                obj["additionalProperties"],
                json!(false),
                "object not closed"
            );
            let props = obj["properties"].as_object().expect("properties");
            let mut names: Vec<&str> = obj["required"]
                .as_array()
                .expect("required")
                .iter()
                .map(|n| n.as_str().unwrap())
                .collect();
            assert!(!names.is_empty(), "an object with nothing required");
            let mut listed: Vec<&str> = props.keys().map(String::as_str).collect();
            names.sort_unstable();
            listed.sort_unstable();
            assert_eq!(names, listed, "required and properties differ");
            for child in props.values() {
                check(child);
            }
        }
        if t == "array" {
            check(obj.get("items").expect("array items"));
        }
    }

    #[test]
    fn schemas_are_standard_and_strict() {
        for s in [
            extraction_schema(),
            drafts_schema(),
            revision_schema(),
            turn_schema(),
        ] {
            check(&s);
        }
        assert_eq!(
            turn_schema()["properties"]["intent"]["enum"],
            json!(["answer", "question", "grade", "repeat", "skip", "undo", "unclear"])
        );
        let cards = &drafts_schema()["properties"]["cards"]["items"];
        assert_eq!(
            cards["properties"]["note_type"]["enum"],
            json!(["basic", "basic_reversed", "basic_typed", "cloze"])
        );
        // The order of `required` is the writing order.
        let revision = revision_schema();
        assert_eq!(revision["required"], json!(["cards", "reply"]));
        assert_eq!(revision["properties"]["cards"]["items"], *cards);
        assert_eq!(revision["properties"]["reply"]["type"], "string");
        let extraction = extraction_schema();
        assert_eq!(
            extraction["required"],
            json!(["text", "figures", "est_words"])
        );
        assert_eq!(
            extraction["properties"]["figures"]["items"]["properties"]["bbox"]["items"]["type"],
            "number"
        );
        assert_eq!(
            turn_schema()["required"],
            json!([
                "intent",
                "rating",
                "feedback",
                "spoken_feedback",
                "answer",
                "spoken_answer"
            ])
        );
    }
}
