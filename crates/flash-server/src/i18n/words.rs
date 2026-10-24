use fluent_bundle::FluentValue;

use super::{list, tr, ListKind};

/// Words a person is shown, made only here: by the catalog (`t`, `ta`,
/// `Tr::t`, `Tr::args`), or by one of two doors each caller names, a
/// person's own text (`Words::theirs`: a deck's name, an address) and
/// English kept so by design for a machine to read (`Words::for_machines`:
/// a protocol's error, a note an AI assistant reads). A field a person
/// reads takes `Words`, so a sentence typed into the code is a type error
/// there, not a page left in English. It reads as its text (`Display`,
/// `Deref<Target = str>`) and serializes as it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct Words(pub(super) String);

impl Words {
    /// A person's own text, shown back to them or to others as they wrote
    /// it: a deck's or a card's text, a name, an address, a file's name.
    pub fn theirs(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// English that stays English for every reader because a machine
    /// reads it: a protocol's error (OAuth, a malformed request no app
    /// sends), a note an AI assistant reads through MCP.
    pub fn for_machines(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// English for the people who run the server (the admin pages, the
    /// mail sent to admins), which stays English by decision.
    pub fn for_admins(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// Words the catalog made earlier that the database kept (a job's
    /// error, say), shown as they were saved.
    pub fn kept(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// Words a test stands in for the catalog with (a fake policy's
    /// sentence). Absent from a production build.
    #[cfg(any(test, feature = "test-support"))]
    pub fn test(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// A sign standing for an empty value (a stat with nothing to show
    /// yet): a sign, which no language words.
    pub fn sign(sign: &'static str) -> Words {
        debug_assert!(!sign.chars().any(char::is_alphabetic), "a sign: {sign:?}");
        Words(sign.to_string())
    }

    /// A number, a date or a time as the reader's region writes it
    /// (`format`), shown on its own: a chart's day, a count.
    pub fn formatted(text: impl Into<String>) -> Words {
        tr().finish(text.into())
    }

    /// A refusal the store words in English for people
    /// (`StoreError::Invalid`): the store cannot reach the catalog, so
    /// `service::store_refusal` words the ones the pages have words for and
    /// passes the rest through here, English to every reader.
    pub fn from_store(text: impl Into<String>) -> Words {
        Words(text.into())
    }

    /// This sentence, then `next`, set apart as the reader's script sets
    /// sentences apart (`Script::sentence_gap`).
    pub fn then(self, next: Words) -> Words {
        let gap = tr().locale().script().sentence_gap();
        Words(format!("{}{gap}{}", self.0, next.0))
    }

    /// Pieces of one line side by side with a middle dot between them
    /// ("12 cards · 3 due"): a sign, which no language words. Items a
    /// sentence lists are a `list`, which each language joins its own way.
    pub fn dotted(parts: Vec<Words>) -> Words {
        Words(
            parts
                .into_iter()
                .map(|w| w.0)
                .collect::<Vec<_>>()
                .join(" · "),
        )
    }

    /// Words a sentence lists, joined as the reader's language joins
    /// `kind` (`list`).
    pub fn list(parts: Vec<Words>, kind: ListKind) -> Words {
        Words(list(&parts, kind))
    }

    /// The first `chars` characters, for a field with a length limit (a
    /// mail's subject line).
    pub fn truncated(self, chars: usize) -> Words {
        Words(self.0.chars().take(chars).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Words {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::ops::Deref for Words {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<Words> for String {
    fn from(words: Words) -> String {
        words.0
    }
}

impl std::borrow::Borrow<str> for Words {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Words {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Words {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Words {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Words> for &str {
    fn eq(&self, other: &Words) -> bool {
        *self == other.0
    }
}

impl PartialEq<String> for Words {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Words> for String {
    fn eq(&self, other: &Words) -> bool {
        *self == other.0
    }
}

/// Words set into another message as a variable.
impl From<Words> for FluentValue<'_> {
    fn from(words: Words) -> Self {
        FluentValue::from(words.0)
    }
}

/// Words as a whole response's plain-text body.
impl axum::response::IntoResponse for Words {
    fn into_response(self) -> axum::response::Response {
        self.0.into_response()
    }
}
