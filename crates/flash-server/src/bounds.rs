//! Every limit on what a request may carry, in one place, and the serde
//! hooks that apply them at the edge. A request struct's string or list
//! field names its bound with `#[serde(deserialize_with = "bounds::…")]`,
//! and `bounded_input_tests` fails the build for any such field that
//! does not, so a handler cannot receive an unbounded value by omission.
//! The limits that depend on stored state (decks per account, media
//! objects per account) live here too and are enforced by the service.
//!
//! The numbers are generous for people and tight for scripts: a deck
//! name of two hundred bytes, fifty tags on a card, five hundred cards
//! in one call. Beyond them a request is refused with a sentence, not
//! parsed and then regretted.

use serde::de::{Deserialize, Deserializer, Error};

/// Per-field byte limits. A deck's name is its whole path, so the store
/// holds a subdeck to the same bound.
pub const DECK_NAME: usize = flash_core::deck_path::MAX_LEN;
pub const DECK_DESCRIPTION: usize = 2_000;
/// The store refuses longer tags and larger lists on every write path;
/// the edge refuses them first so the sentence names the field.
pub const TAG: usize = flash_store::MAX_TAG_LEN;
pub const DISPLAY_NAME: usize = 120;
pub const EMAIL: usize = 254;
/// Long enough for any passphrase; short enough that hashing it is
/// bounded work.
pub const PASSWORD: usize = 1_024;
pub const DEVICE_LABEL: usize = 120;
/// Opaque tokens and ids the server minted: ceremony ids, refresh and
/// import tokens, client ids, authorization codes.
pub const TOKEN: usize = 256;
/// A typed answer on the study screen, matched against a card side.
pub const TYPED_ANSWER: usize = flash_core::MAX_SIDE_LEN;
/// A plain card side (front or back) from the API or MCP.
pub const CARD_SIDE: usize = flash_core::MAX_SIDE_LEN;
/// One editor field of HTML, before sanitizing.
pub const FIELD_HTML: usize = flash_store::notes::MAX_FIELD_HTML;
/// A search term against card text. Anything longer than a card side
/// cannot match; anything longer than this makes LIKE expensive.
pub const SEARCH: usize = 100;
pub const TIMEZONE: usize = 64;
/// A deck's goal date, `YYYY-MM-DD`, with room for stray spaces.
pub const GOAL_DATE: usize = 16;
/// A BCP 47 language tag as a client sends it ("es-MX", "zh-Hant-TW").
pub const LOCALE: usize = flash_core::locale::MAX_TAG;
/// Enum-shaped strings: a theme, a grading mode, a note type, a colour
/// choice, a decision, a saved-flag.
pub const KEYWORD: usize = 32;
/// A local path to return to after sign-in. A signed-out connector's
/// whole `/oauth/authorize` request lands here, so it is as long as any
/// request `URI` may be.
pub const NEXT_PATH: usize = URI;
/// OAuth request parameters (RFC 6749 leaves these unbounded). A
/// client's `state` can be a signed blob well past a kilobyte.
pub const OAUTH_STATE: usize = 4_096;
pub const OAUTH_SCOPE: usize = 256;
pub const OAUTH_RESOURCE: usize = 500;
/// RFC 7636: 43 to 128 characters of unreserved ASCII.
pub const CODE_CHALLENGE: usize = 128;
pub const CODE_VERIFIER: usize = 128;
pub const REDIRECT_URI: usize = 2_048;
pub const CLIENT_NAME: usize = 200;
/// A comma-separated tag list typed into a web form.
pub const TAG_LIST_TEXT: usize = TAGS_PER_CARD * (TAG + 1);
/// The tags a study scope names, comma-joined into one query parameter.
pub const SCOPE_TAG_TEXT: usize = TAGS_PER_SCOPE * (TAG + 1);
/// An opaque value a client mints and echoes back (an install id).
pub const OPAQUE: usize = 1_024;
/// A title of something a person names.
pub const TITLE: usize = 200;
/// Free text a person writes about something (a reason).
pub const REASON: usize = 2_000;
/// A captcha widget's response token, up to 2,048 characters.
pub const CAPTCHA_TOKEN: usize = 2_048;
/// One field of a page's error report (the message, the script, the
/// page): the page sends at most 300 characters of each, which the
/// server shortens again before it counts them.
pub const CLIENT_ERROR_TEXT: usize = 2_048;
/// A page's whole error report.
pub const CLIENT_ERROR_BODY_BYTES: usize = 8 * 1024;
/// A browser's policy violation report: its own JSON, which carries the
/// whole policy and a sample of the blocked code, a few to the request.
pub const CSP_REPORT_BODY_BYTES: usize = 32 * 1024;
/// Path plus query of any request, enforced by `middleware::uri_length`
/// before routing: the bound on every `Path<String>` and `RawQuery`.
/// The longest legitimate URL is a signed-out connector's sign-in link,
/// `/login?next=` plus its whole `/oauth/authorize` request encoded, with
/// a directory's `state` blob inside; nginx's default is the same size.
pub const URI: usize = 8_192;
/// A filename a client attaches to an upload, before the store shortens
/// it to what it keeps.
pub const FILENAME: usize = 255;
/// Ids the server minted that a client echoes back in one request, as a
/// JSON list or, from a web form, comma-joined into one field.
pub const IDS_PER_CALL: usize = 20;
pub const ID_LIST: usize = IDS_PER_CALL * 21;

/// Per-request count limits.
pub const TAGS_PER_CARD: usize = flash_store::MAX_TAGS_PER_CARD;
/// Tags one study scope may name; more than this is a filter nobody
/// picked by hand, and every extra name costs the queue a lookup.
pub const TAGS_PER_SCOPE: usize = flash_core::queue::MAX_SCOPE_TAGS;
pub const CARDS_PER_CALL: usize = 500;
/// Decks one deck page's filter names (`own` or a subdeck's id each).
pub const DECKS_PER_FILTER: usize = 50;
pub const DECK_FILTER_TEXT: usize = DECKS_PER_FILTER * 21;
/// Card ids one action on a deck page's cards names: the store's cap,
/// comma-joined.
pub const BULK_ID_LIST: usize = flash_store::MAX_BULK_CARDS * 21;
/// What an Undo carries back: the cards an action changed and, for a
/// move or a tag, where each was or which tag. An action whose record
/// is larger offers no Undo.
pub const UNDO_RECORD: usize = 512 * 1024;
pub const REDIRECT_URIS_PER_CLIENT: usize = 10;

/// Per-account limits on stored state, enforced where the row is
/// created: decks in the store's `create_deck`, so an import naming a
/// new deck meets the same cap as a form; media in the server's ingest
/// paths, the only writers of a blob.
pub const DECKS_PER_USER: u32 = flash_store::MAX_DECKS_PER_USER;
pub const MEDIA_OBJECTS_PER_USER: u64 = 20_000;

pub fn too_long<E: Error>(what: &str, max: usize) -> E {
    E::custom(format!("{what} is longer than {max} bytes"))
}

pub fn too_many<E: Error>(what: &str, max: usize) -> E {
    E::custom(format!("more than {max} {what}"))
}

/// A string of at most `max` bytes, named `what` when refused.
pub fn text<'de, D: Deserializer<'de>>(d: D, what: &str, max: usize) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s.len() > max {
        return Err(too_long(what, max));
    }
    Ok(s)
}

/// `text`, when the field may be absent.
pub fn opt_text<'de, D: Deserializer<'de>>(
    d: D,
    what: &str,
    max: usize,
) -> Result<Option<String>, D::Error> {
    match Option::<String>::deserialize(d)? {
        Some(s) if s.len() > max => Err(too_long(what, max)),
        other => Ok(other),
    }
}

fn texts<'de, D: Deserializer<'de>>(
    d: D,
    what: &str,
    count: usize,
    each: usize,
) -> Result<Vec<String>, D::Error> {
    let v = Vec::<String>::deserialize(d)?;
    if v.len() > count {
        return Err(too_many(what, count));
    }
    if let Some(long) = v.iter().find(|s| s.len() > each) {
        let _ = long;
        return Err(too_long(what, each));
    }
    Ok(v)
}

// One named hook per kind of field. The names are what the request
// structs cite and what the inventory test looks for.

macro_rules! text_hook {
    ($name:ident, $what:literal, $max:expr) => {
        pub fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
            text(d, $what, $max)
        }
    };
}

macro_rules! opt_text_hook {
    ($name:ident, $what:literal, $max:expr) => {
        pub fn $name<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
            opt_text(d, $what, $max)
        }
    };
}

text_hook!(deck_name, "deck name", DECK_NAME);
text_hook!(deck_description, "deck description", DECK_DESCRIPTION);
text_hook!(tag, "tag", TAG);
text_hook!(display_name, "display name", DISPLAY_NAME);
text_hook!(email, "email address", EMAIL);
text_hook!(password, "password", PASSWORD);
text_hook!(device_label, "device label", DEVICE_LABEL);
text_hook!(token, "token", TOKEN);
text_hook!(card_side, "card side", CARD_SIDE);
text_hook!(field_html, "field", FIELD_HTML);
text_hook!(keyword, "value", KEYWORD);
text_hook!(tag_list_text, "tag list", TAG_LIST_TEXT);
text_hook!(redirect_uri, "redirect_uri", REDIRECT_URI);
text_hook!(client_name, "client_name", CLIENT_NAME);
text_hook!(title, "title", TITLE);
text_hook!(reason, "reason", REASON);
text_hook!(description, "description", DECK_DESCRIPTION);
text_hook!(captcha_token, "captcha token", CAPTCHA_TOKEN);
text_hook!(next_path, "next", NEXT_PATH);
text_hook!(timezone, "timezone", TIMEZONE);
text_hook!(filename, "file name", FILENAME);

opt_text_hook!(opt_deck_name, "deck name", DECK_NAME);
opt_text_hook!(opt_deck_description, "deck description", DECK_DESCRIPTION);
opt_text_hook!(opt_tag, "tag", TAG);
opt_text_hook!(opt_password, "password", PASSWORD);
opt_text_hook!(opt_token, "token", TOKEN);
opt_text_hook!(opt_typed_answer, "typed answer", TYPED_ANSWER);
opt_text_hook!(opt_card_side, "card side", CARD_SIDE);
opt_text_hook!(opt_search, "search", SEARCH);
opt_text_hook!(opt_timezone, "timezone", TIMEZONE);
opt_text_hook!(opt_locale, "locale", LOCALE);
opt_text_hook!(opt_keyword, "value", KEYWORD);
opt_text_hook!(opt_next_path, "next", NEXT_PATH);
opt_text_hook!(opt_oauth_state, "state", OAUTH_STATE);
opt_text_hook!(opt_oauth_scope, "scope", OAUTH_SCOPE);
opt_text_hook!(opt_oauth_resource, "resource", OAUTH_RESOURCE);
opt_text_hook!(opt_code_challenge, "code_challenge", CODE_CHALLENGE);
opt_text_hook!(opt_code_verifier, "code_verifier", CODE_VERIFIER);
opt_text_hook!(opt_redirect_uri, "redirect_uri", REDIRECT_URI);
opt_text_hook!(opt_client_name, "client_name", CLIENT_NAME);
opt_text_hook!(opt_opaque, "token", OPAQUE);
opt_text_hook!(opt_display_name, "name", DISPLAY_NAME);
opt_text_hook!(opt_email, "email address", EMAIL);
opt_text_hook!(opt_field_html, "field", FIELD_HTML);
opt_text_hook!(opt_client_error_text, "error report", CLIENT_ERROR_TEXT);
opt_text_hook!(opt_id_list, "ids", ID_LIST);
opt_text_hook!(opt_scope_tag_text, "tags", SCOPE_TAG_TEXT);
opt_text_hook!(opt_goal_date, "goal date", GOAL_DATE);
opt_text_hook!(opt_deck_filter, "decks", DECK_FILTER_TEXT);
opt_text_hook!(opt_tag_list_text, "tags", TAG_LIST_TEXT);
opt_text_hook!(opt_bulk_ids, "ids", BULK_ID_LIST);
opt_text_hook!(opt_undo_record, "undo", UNDO_RECORD);

/// A goal date a client may leave out (keep the one stored: an app from
/// before goals), send as null (clear it) or send: absent is None, null
/// `Some(None)`. Pair it with `#[serde(default)]`.
pub fn present_goal_date<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    opt_text(d, "goal date", GOAL_DATE).map(Some)
}

/// Ids a client echoes back in one request.
pub fn ids<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    let v = Vec::<i64>::deserialize(d)?;
    if v.len() > IDS_PER_CALL {
        return Err(too_many("ids", IDS_PER_CALL));
    }
    Ok(v)
}

/// A card's tags: at most `TAGS_PER_CARD`, each at most `TAG` bytes.
pub fn tags<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    texts(d, "tags", TAGS_PER_CARD, TAG)
}

/// The tags a study scope names: at most `TAGS_PER_SCOPE`, each at most
/// `TAG` bytes.
pub fn scope_tags<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    texts(d, "tags", TAGS_PER_SCOPE, TAG)
}

pub fn opt_scope_tags<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    match Option::<Vec<String>>::deserialize(d)? {
        None => Ok(None),
        Some(v) => scope_tags(serde::de::value::SeqDeserializer::<_, D::Error>::new(
            v.into_iter()
                .map(serde::de::value::StringDeserializer::<D::Error>::new),
        ))
        .map(Some),
    }
}

pub fn opt_tags<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    match Option::<Vec<String>>::deserialize(d)? {
        None => Ok(None),
        Some(v) => {
            if v.len() > TAGS_PER_CARD {
                return Err(too_many("tags", TAGS_PER_CARD));
            }
            if v.iter().any(|s| s.len() > TAG) {
                return Err(too_long("tag", TAG));
            }
            Ok(Some(v))
        }
    }
}

/// A deck's language hints, before they are checked against the known
/// codes (`deck_languages::LANGUAGE_CODES`): a short list of short codes
/// (twice the stored cap, so a list with duplicates still reaches the
/// check that names the problem).
pub fn language_hints<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    texts(
        d,
        "language hints",
        crate::deck_languages::MAX_LANGUAGE_HINTS * 2,
        KEYWORD,
    )
}

/// A side's speech blocks: at most `flash_core::speech::MAX_BLOCKS` (each
/// block bounds its own fields).
pub fn opt_speech<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Vec<T>>, D::Error> {
    match Option::<Vec<T>>::deserialize(d)? {
        Some(v) if v.len() > flash_core::speech::MAX_BLOCKS => {
            Err(too_many("speech blocks", flash_core::speech::MAX_BLOCKS))
        }
        other => Ok(other),
    }
}

/// The languages a deck is declared with: a short list of short tags.
pub fn opt_languages<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    match Option::<Vec<String>>::deserialize(d)? {
        None => Ok(None),
        Some(v) => {
            if v.len() > crate::deck_languages::MAX_LANGUAGE_HINTS * 2 {
                return Err(too_many(
                    "languages",
                    crate::deck_languages::MAX_LANGUAGE_HINTS * 2,
                ));
            }
            if v.iter().any(|s| s.len() > KEYWORD) {
                return Err(too_long("language", KEYWORD));
            }
            Ok(Some(v))
        }
    }
}

/// A client registration's redirect URIs.
pub fn redirect_uris<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    texts(d, "redirect_uris", REDIRECT_URIS_PER_CLIENT, REDIRECT_URI)
}

/// A list of at most `CARDS_PER_CALL` items of any deserializable type
/// (each item bounds its own fields).
pub fn cards<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    let v = Vec::<T>::deserialize(d)?;
    if v.len() > CARDS_PER_CALL {
        return Err(too_many("cards in one call", CARDS_PER_CALL));
    }
    Ok(v)
}

/// Most bytes a JSON value taken whole may take once encoded: room for
/// a card with both rich sides and its cloze text at their own bounds.
pub const JSON_VALUE: usize = 256 * 1024;
/// Most numbers in one list of parameters.
pub const NUMBERS_PER_FIELD: usize = 64;

/// A JSON value taken whole, for a handler that reads it through its
/// own bounded types once it knows which (a downstream extension's
/// changes carry one per kind), at most `JSON_VALUE` bytes encoded.
pub fn opt_json<'de, D: Deserializer<'de>>(d: D) -> Result<Option<serde_json::Value>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    if let Some(value) = &v {
        let size = serde_json::to_vec(value).map_or(usize::MAX, |b| b.len());
        if size > JSON_VALUE {
            return Err(too_long("value", JSON_VALUE));
        }
    }
    Ok(v)
}

/// A list of at most `NUMBERS_PER_FIELD` numbers (scheduler parameters).
pub fn opt_numbers<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<f32>>, D::Error> {
    let v = Option::<Vec<f32>>::deserialize(d)?;
    if v.as_ref().is_some_and(|n| n.len() > NUMBERS_PER_FIELD) {
        return Err(too_many("numbers", NUMBERS_PER_FIELD));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct Probe {
        #[serde(deserialize_with = "deck_name")]
        name: String,
        #[serde(default, deserialize_with = "tags")]
        tags: Vec<String>,
        #[serde(default, deserialize_with = "opt_search")]
        q: Option<String>,
    }

    #[test]
    fn a_field_past_its_bound_is_refused_with_a_sentence() {
        let long = "x".repeat(DECK_NAME + 1);
        let err = serde_json::from_str::<Probe>(&format!(r#"{{"name":"{long}"}}"#)).unwrap_err();
        assert!(err
            .to_string()
            .contains("deck name is longer than 200 bytes"));

        let many: Vec<String> = (0..=TAGS_PER_CARD).map(|i| i.to_string()).collect();
        let body = serde_json::json!({"name": "ok", "tags": many}).to_string();
        let err = serde_json::from_str::<Probe>(&body).unwrap_err();
        assert!(err.to_string().contains("more than 50 tags"));

        let ok: Probe = serde_json::from_str(r#"{"name":"Pharm","tags":["a"],"q":"x"}"#).unwrap();
        assert_eq!(ok.name, "Pharm");
        assert_eq!(ok.tags, ["a"]);
        assert_eq!(ok.q.as_deref(), Some("x"));
    }
}
