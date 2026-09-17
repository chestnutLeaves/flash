//! Any endpoint that speaks the OpenAI Chat Completions API as an
//! `LlmProvider`: a hosted vendor, a gateway, or a model server on the
//! same machine. One request shape (`RequestBody`) built by a pure
//! function per step, one private `send` that posts it to
//! `{base}/chat/completions`, and the reply read back as the JSON the
//! schema asked for. The protocol has no upload route, so every source
//! travels inside the request, and the request has a ceiling.
//!
//! What the endpoint reads is what this adapter declares (text, photos,
//! recordings); the operator narrows it with `FLASH_AI_INPUTS` for an
//! endpoint that reads less. The reply's shape is asked for the way
//! `FLASH_AI_OUTPUT` says, since endpoints differ in what they honour.
//!
//! The key is sent as a bearer header and never logged; a failed call
//! is logged as its status and a short excerpt of the body.

use std::time::{Duration, Instant};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use super::prompts;
use super::provider::{
    Capabilities, CardContext, DialogState, DraftNote, EditInput, ExtractInput, Extraction,
    LlmError, LlmProvider, Modality, Reply, Revision, SourceKind, SourcePart, SpokenInput,
    SpokenTurn, Usage,
};
use super::replies::{drafts_from, extraction_from, reply, revision_from, turn_from};
use super::schema;
use super::transport::{elapsed_ms, transport_error};
use crate::config::{AiConfig, AiOutput};

/// Raw bytes of one part that travel inside a request, and of all of
/// them together: conservative, the smallest body limits seen across
/// hosted and local endpoints, and there is no upload route to fall
/// back to past them.
pub const INLINE_LIMIT: u64 = 10 * 1024 * 1024;
const REQUEST_LIMIT: u64 = 20_000_000;
/// Binary parts in one request.
const MAX_PARTS: usize = 32;

/// What the protocol carries as parts, and so what any endpoint behind
/// it can be handed: photos as data URLs, recordings as `input_audio`
/// in the two formats the field takes. A PDF, a video or a video link
/// has no part type here, so they are not declared, and the composer
/// never offers them.
pub fn capabilities() -> Capabilities {
    Capabilities {
        kinds: vec![SourceKind::Text, SourceKind::Image, SourceKind::Audio],
        mimes: vec![
            (
                SourceKind::Image,
                vec!["image/png", "image/jpeg", "image/webp"],
            ),
            (
                SourceKind::Audio,
                vec!["audio/wav", "audio/mpeg", "audio/mp3"],
            ),
        ],
        max_inline_bytes: INLINE_LIMIT,
        max_request_bytes: REQUEST_LIMIT,
        max_file_bytes: None,
        max_parts: MAX_PARTS,
        supports_youtube: false,
        max_media_seconds: None,
    }
}

/// One call with text or pictures.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// One call that has to listen to something.
const LONG_CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Most of a reply body we will read.
const REPLY_LIMIT: u64 = 16 * 1024 * 1024;

/// Temperatures per step: reading and grading want the same answer
/// every time; writing cards wants some variety between passes.
const TEMP_EXTRACT: f64 = 0.2;
const TEMP_WRITE: f64 = 0.7;
const TEMP_TURN: f64 = 0.1;

/// Bytes a second of a recording holds, for the ledger's media time
/// when the endpoint reports no per-second rate: 16 kHz 16-bit mono
/// wav, and mp3 at 128 kbit/s. An estimate, said so in the record.
const WAV_BYTES_PER_SECOND: u64 = 32_000;
const MP3_BYTES_PER_SECOND: u64 = 16_000;

pub struct OpenAiProvider {
    base_url: String,
    api_key: String,
    model: String,
    grader_model: String,
    output: AiOutput,
    agent: ureq::Agent,
}

// ---- the request ----

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequestBody {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    pub role: &'static str,
    pub content: Content,
}

/// A system message is one string; the user message is a list of parts.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Parts(Vec<Part>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
    InputAudio { input_audio: InputAudio },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageUrl {
    /// A `data:` URL of the bytes.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputAudio {
    /// Base64 of the bytes.
    pub data: String,
    /// `wav` or `mp3`, the formats the field takes.
    pub format: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    JsonSchema { json_schema: JsonSchema },
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsonSchema {
    pub name: &'static str,
    pub schema: Value,
    pub strict: bool,
}

impl Part {
    fn text(text: impl Into<String>) -> Self {
        Part::Text { text: text.into() }
    }

    fn image(mime: &str, bytes: &[u8]) -> Self {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        Part::ImageUrl {
            image_url: ImageUrl {
                url: format!("data:{mime};base64,{b64}"),
            },
        }
    }

    fn audio(format: &'static str, bytes: &[u8]) -> Self {
        Part::InputAudio {
            input_audio: InputAudio {
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
                format,
            },
        }
    }
}

/// The `format` of an `input_audio` part for a recording's type, None
/// for a type the field does not take (a browser's webm or mp4).
pub fn audio_format(mime: &str) -> Option<&'static str> {
    let bare = mime.split(';').next().unwrap_or("").trim();
    if bare.eq_ignore_ascii_case("audio/wav") {
        Some("wav")
    } else if bare.eq_ignore_ascii_case("audio/mpeg") || bare.eq_ignore_ascii_case("audio/mp3") {
        Some("mp3")
    } else {
        None
    }
}

/// The sentence a source part gets when the protocol has no part for
/// it. Unreachable after the runtime narrowed the inputs; kept so a
/// hole is a configuration error rather than a malformed request.
fn unsupported(what: &str) -> LlmError {
    LlmError::Config(format!("the endpoint does not take {what}"))
}

/// The part a recording travels as, or the configuration error for a
/// format the field does not take.
fn recording(mime: &str, bytes: &[u8]) -> Result<Part, LlmError> {
    match audio_format(mime) {
        Some(format) => Ok(Part::audio(format, bytes)),
        None => Err(LlmError::Config(
            "the endpoint takes wav and mp3 recordings only".into(),
        )),
    }
}

/// One request: the system prompt, the user parts, and the reply's
/// shape asked for the way `output` says. In the modes without a
/// schema field the schema is stated in the prompt instead, so the
/// model is told the same shape either way.
fn request(
    model: &str,
    system: &str,
    parts: Vec<Part>,
    name: &'static str,
    schema: Value,
    temperature: f64,
    output: AiOutput,
) -> RequestBody {
    let (system, response_format) = match output {
        AiOutput::Schema => (
            system.to_string(),
            Some(ResponseFormat::JsonSchema {
                json_schema: JsonSchema {
                    name,
                    schema,
                    strict: true,
                },
            }),
        ),
        AiOutput::Json => (
            with_schema_stated(system, &schema),
            Some(ResponseFormat::JsonObject),
        ),
        AiOutput::Plain => (with_schema_stated(system, &schema), None),
    };
    RequestBody {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system",
                content: Content::Text(system),
            },
            Message {
                role: "user",
                content: Content::Parts(parts),
            },
        ],
        temperature,
        response_format,
    }
}

fn with_schema_stated(system: &str, schema: &Value) -> String {
    format!(
        "{system}\n\nReply with one JSON object that matches this JSON Schema exactly:\n{schema}"
    )
}

/// The extraction request: the notice and the count open the message;
/// each source then follows as its introduction line (with its text
/// fenced, for a text source) and its photo or recording parts in
/// order.
pub fn build_extract_request(
    model: &str,
    input: &ExtractInput,
    output: AiOutput,
) -> Result<RequestBody, LlmError> {
    let mut out = vec![Part::text(prompts::extract_intro(input.sources.len()))];
    for source in &input.sources {
        out.push(Part::text(prompts::source_heading(source)));
        for part in &source.parts {
            match part {
                SourcePart::Text(_) => {}
                SourcePart::Inline { mime, bytes } => match source.kind {
                    SourceKind::Image => out.push(Part::image(mime, bytes)),
                    SourceKind::Audio => out.push(recording(mime, bytes)?),
                    SourceKind::Pdf => return Err(unsupported("a PDF")),
                    SourceKind::Video => return Err(unsupported("a video")),
                    SourceKind::Text | SourceKind::Youtube => {
                        return Err(unsupported("this kind of source"))
                    }
                },
                SourcePart::FileUri { .. } => return Err(unsupported("an uploaded file")),
                SourcePart::YoutubeUrl(_) => return Err(unsupported("a video link")),
            }
        }
    }
    Ok(request(
        model,
        prompts::EXTRACT_SYSTEM,
        out,
        "extraction",
        schema::extraction_schema(),
        TEMP_EXTRACT,
        output,
    ))
}

/// The card-writing request; with `existing` drafts it becomes the
/// "more" step, whose prompt forbids repeating them.
pub fn build_generate_request(
    model: &str,
    extraction: &Extraction,
    instruction: &str,
    existing: &[DraftNote],
    output: AiOutput,
) -> RequestBody {
    let system = if existing.is_empty() {
        prompts::GENERATE_SYSTEM
    } else {
        prompts::MORE_SYSTEM
    };
    request(
        model,
        system,
        vec![Part::text(prompts::generate_user_text(
            extraction,
            instruction,
            existing,
        ))],
        "drafts",
        schema::drafts_schema(),
        TEMP_WRITE,
        output,
    )
}

/// One message of the review's thread, answered with the new open set
/// and a sentence.
pub fn build_revise_request(model: &str, edit: &EditInput<'_>, output: AiOutput) -> RequestBody {
    request(
        model,
        prompts::EDIT_SYSTEM,
        vec![Part::text(prompts::edit_user_text(edit))],
        "revision",
        schema::revision_schema(),
        TEMP_WRITE,
        output,
    )
}

/// The spoken turn's request: the state, the card and the transcript as
/// one text part, or the state and the card as text and the recording
/// as the audio part after it.
pub fn build_turn_request(
    model: &str,
    card: &CardContext,
    state: DialogState,
    spoken: SpokenInput<'_>,
    output: AiOutput,
) -> Result<RequestBody, LlmError> {
    let parts = match spoken {
        SpokenInput::Transcript(text) => {
            vec![Part::text(prompts::turn_user_text(card, state, text))]
        }
        SpokenInput::Audio { mime, bytes } => vec![
            Part::text(prompts::turn_audio_user_text(card, state)),
            recording(mime, bytes)?,
        ],
    };
    Ok(request(
        model,
        prompts::TURN_SYSTEM,
        parts,
        "turn",
        schema::turn_schema(),
        TEMP_TURN,
        output,
    ))
}

// ---- the reply ----

/// The text of the first choice: a string, or a list of text parts
/// (some local servers answer that way) joined.
fn content_text(message: &Value) -> String {
    match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p["type"].as_str() == Some("text"))
            .filter_map(|p| p["text"].as_str())
            .collect(),
        _ => String::new(),
    }
}

/// The JSON object in a reply's text: a fenced block is unwrapped, and
/// anything around the outermost braces is dropped, since a model in
/// the plain mode may talk around its answer.
fn json_in(text: &str) -> Option<&str> {
    let mut body = text.trim();
    if let Some(rest) = body.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or("");
        body = rest.trim_end().strip_suffix("```").unwrap_or(rest).trim();
    }
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    (end >= start).then(|| &body[start..=end])
}

/// The JSON the model wrote, out of the API's envelope. A refusal or a
/// content filter is a refusal; a missing or truncated answer is a
/// schema error.
pub fn parse_reply(raw: &Value) -> Result<Value, LlmError> {
    let choice = raw["choices"]
        .get(0)
        .ok_or_else(|| LlmError::schema("no choices"))?;
    let message = &choice["message"];
    if message["refusal"].as_str().is_some_and(|r| !r.is_empty()) {
        return Err(LlmError::refused());
    }
    match choice["finish_reason"].as_str() {
        Some("content_filter") => return Err(LlmError::refused()),
        Some("length") => return Err(LlmError::schema("reply truncated")),
        _ => {}
    }
    let text = content_text(message);
    if text.trim().is_empty() {
        return Err(LlmError::schema("empty reply"));
    }
    let json = json_in(&text).ok_or_else(|| LlmError::schema("reply is not JSON"))?;
    serde_json::from_str(json).map_err(|e| LlmError::schema(format!("reply is not JSON: {e}")))
}

/// `usage` as a `Usage`: prompt tokens by kind where the record says
/// (audio apart from text), the cached share, and reasoning tokens as
/// reply, which is how they are billed. `media_seconds` is what the
/// caller knows of the recordings sent.
pub fn usage_of(raw: &Value, media_seconds: Option<u32>, ms: u32) -> Usage {
    let usage = &raw["usage"];
    let count = |v: &Value| v.as_u64().unwrap_or(0).min(u32::MAX as u64) as u32;
    let prompt_total = count(&usage["prompt_tokens"]);
    let audio = count(&usage["prompt_tokens_details"]["audio_tokens"]).min(prompt_total);
    let cached = count(&usage["prompt_tokens_details"]["cached_tokens"]);
    let completion = count(&usage["completion_tokens"]);
    let thoughts = count(&usage["completion_tokens_details"]["reasoning_tokens"]).min(completion);
    let mut prompt = vec![(Modality::Text, prompt_total - audio)];
    if audio > 0 {
        prompt.push((Modality::Audio, audio));
    }
    Usage {
        prompt,
        cached: if cached > 0 {
            vec![(Modality::Text, cached)]
        } else {
            Vec::new()
        },
        tool_use: Vec::new(),
        prompt_total,
        candidates: completion - thoughts,
        thoughts,
        media_seconds,
        ms,
        response_id: raw["id"].as_str().map(str::to_string),
        model_version: raw["model"].as_str().map(str::to_string),
        raw: usage.is_object().then(|| usage.to_string()),
    }
}

/// The playing time a recording stands for, from its size.
pub fn estimated_seconds(mime: &str, bytes: u64) -> u32 {
    let per_second = match audio_format(mime) {
        Some("wav") => WAV_BYTES_PER_SECOND,
        _ => MP3_BYTES_PER_SECOND,
    };
    (bytes / per_second).max(1).min(u32::MAX as u64) as u32
}

/// The recordings' time in a set: the lengths ingest read, or an
/// estimate from the bytes where it read none; None for a set with no
/// recording.
fn set_media_seconds(input: &ExtractInput) -> Option<u32> {
    let mut total: u32 = 0;
    let mut any = false;
    for source in input.sources.iter().filter(|s| s.kind == SourceKind::Audio) {
        any = true;
        total = total.saturating_add(match source.duration_s {
            Some(seconds) => seconds,
            None => source
                .parts
                .iter()
                .map(|part| match part {
                    SourcePart::Inline { mime, bytes } => {
                        estimated_seconds(mime, bytes.len() as u64)
                    }
                    _ => 0,
                })
                .sum(),
        });
    }
    any.then_some(total)
}

// ---- transport ----

impl OpenAiProvider {
    pub fn new(
        base_url: String,
        api_key: String,
        model: String,
        grader_model: String,
        output: AiOutput,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            grader_model,
            output,
            // Every request sets its own overall timeout; the agent's is
            // the default for the short ones.
            agent: crate::http::agent(CALL_TIMEOUT, false),
        }
    }

    pub fn from_config(config: &AiConfig) -> Result<Self, String> {
        let base_url = config
            .base_url
            .clone()
            .ok_or_else(|| "FLASH_AI_BASE_URL is not set".to_string())?;
        Ok(Self::new(
            base_url,
            config.api_key.clone(),
            config.model.clone(),
            config.grader_model.clone(),
            config.output,
        ))
    }

    /// The host the endpoint is at, with its port when the address names
    /// one (a local endpoint is only identifiable by it): nothing of the
    /// path or the query, which may carry a key of their own.
    fn host(&self) -> String {
        match url::Url::parse(&self.base_url) {
            Ok(url) => match (url.host_str(), url.port()) {
                (Some(host), Some(port)) => format!("{host}:{port}"),
                (Some(host), None) => host.to_string(),
                (None, _) => "?".to_string(),
            },
            Err(_) => "?".to_string(),
        }
    }

    /// One `chat/completions` call. The reply comes back as the JSON
    /// the model wrote plus what it cost.
    fn send(
        &self,
        body: &RequestBody,
        long: bool,
        media_seconds: Option<u32>,
    ) -> Result<(Value, Usage), LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let timeout = if long {
            LONG_CALL_TIMEOUT
        } else {
            CALL_TIMEOUT
        };
        let started = Instant::now();
        let mut request = self
            .agent
            .post(&url)
            .config()
            .timeout_global(Some(timeout))
            .build();
        if !self.api_key.is_empty() {
            request = request.header("Authorization", &format!("Bearer {}", self.api_key));
        }
        let response = request.send_json(body).map_err(transport_error)?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .with_config()
            .limit(REPLY_LIMIT)
            .read_to_string()
            .map_err(transport_error)?;
        let ms = elapsed_ms(started);
        if !(200..300).contains(&status) {
            let err = LlmError::http(status, &text);
            if let LlmError::Http { excerpt, .. } = &err {
                tracing::warn!(status, excerpt = ?excerpt, "model call failed");
            }
            return Err(err);
        }
        let raw: Value = serde_json::from_str(&text)
            .map_err(|e| LlmError::schema(format!("envelope is not JSON: {e}")))?;
        let usage = usage_of(&raw, media_seconds, ms);
        let value = parse_reply(&raw).map_err(|e| e.billed(usage.clone()))?;
        Ok((value, usage))
    }
}

impl LlmProvider for OpenAiProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn extract(&self, input: &ExtractInput) -> Result<Reply<Extraction>, LlmError> {
        if input.bytes() > REQUEST_LIMIT {
            return Err(LlmError::http(
                413,
                "the set is larger than one request to the endpoint takes",
            ));
        }
        let body = build_extract_request(&self.model, input, self.output)?;
        let (value, usage) = self.send(&body, input.is_media(), set_media_seconds(input))?;
        reply(extraction_from(value), usage)
    }

    fn generate(
        &self,
        extraction: &Extraction,
        instruction: &str,
        existing: &[DraftNote],
    ) -> Result<Reply<Vec<DraftNote>>, LlmError> {
        let body =
            build_generate_request(&self.model, extraction, instruction, existing, self.output);
        let (value, usage) = self.send(&body, false, None)?;
        reply(drafts_from(value), usage)
    }

    fn revise(&self, edit: &EditInput<'_>) -> Result<Reply<Revision>, LlmError> {
        let body = build_revise_request(&self.model, edit, self.output);
        let (value, usage) = self.send(&body, false, None)?;
        reply(revision_from(value), usage)
    }

    fn turn(
        &self,
        card: &CardContext,
        state: DialogState,
        spoken: SpokenInput<'_>,
    ) -> Result<Reply<SpokenTurn>, LlmError> {
        let body = build_turn_request(&self.grader_model, card, state, spoken, self.output)?;
        // A recording is metered as audio so the ledger sees its seconds.
        let media_seconds = match spoken {
            SpokenInput::Transcript(_) => None,
            SpokenInput::Audio { mime, bytes } => Some(estimated_seconds(mime, bytes.len() as u64)),
        };
        let (value, usage) = self.send(&body, false, media_seconds)?;
        reply(turn_from(value, state), usage)
    }

    fn describe(&self) -> String {
        format!(
            "openai-compatible ({}; {}; grader {})",
            self.host(),
            self.model,
            self.grader_model
        )
    }

    fn model_names(&self) -> (String, String) {
        (self.model.clone(), self.grader_model.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::fake::FakeProvider;
    use crate::ai::provider::ExtractSource;
    use flash_store::notes::NoteType;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn single(kind: SourceKind, parts: Vec<SourcePart>) -> ExtractInput {
        ExtractInput::single(kind, "notes", parts)
    }

    fn inline(mime: &str, bytes: Vec<u8>) -> SourcePart {
        SourcePart::Inline {
            mime: mime.into(),
            bytes,
        }
    }

    fn body_of(body: &RequestBody) -> Value {
        serde_json::to_value(body).unwrap()
    }

    fn user_parts(body: &RequestBody) -> Vec<Value> {
        body_of(body)["messages"][1]["content"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn extract_request_for_text_is_the_golden_shape() {
        let input = single(SourceKind::Text, vec![SourcePart::Text("notes".into())]);
        let body = build_extract_request("m", &input, AiOutput::Schema).unwrap();
        assert_eq!(
            body_of(&body),
            json!({
                "model": "m",
                "messages": [
                    { "role": "system", "content": prompts::EXTRACT_SYSTEM },
                    { "role": "user", "content": [
                        { "type": "text", "text": prompts::extract_intro(1) },
                        { "type": "text", "text": prompts::source_heading(&input.sources[0]) }
                    ] }
                ],
                "temperature": 0.2,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "extraction",
                        "schema": schema::extraction_schema(),
                        "strict": true
                    }
                }
            })
        );
    }

    #[test]
    fn photos_are_data_urls_and_recordings_are_input_audio() {
        let input = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Image,
                    label: "page.png".into(),
                    parts: vec![inline("image/png", vec![1, 2])],
                    duration_s: None,
                },
                ExtractSource {
                    index: 2,
                    kind: SourceKind::Audio,
                    label: "lecture.wav".into(),
                    parts: vec![inline("audio/wav", b"RIFF".to_vec())],
                    duration_s: Some(90),
                },
                ExtractSource {
                    index: 3,
                    kind: SourceKind::Audio,
                    label: "clip.mp3".into(),
                    parts: vec![inline("audio/mpeg", vec![0; 4])],
                    duration_s: None,
                },
            ],
        };
        let body = build_extract_request("m", &input, AiOutput::Schema).unwrap();
        let parts = user_parts(&body);
        assert_eq!(parts.len(), 7);
        assert_eq!(
            parts[2],
            json!({ "type": "image_url", "image_url": { "url": "data:image/png;base64,AQI=" } })
        );
        assert_eq!(
            parts[4],
            json!({ "type": "input_audio", "input_audio": { "data": "UklGRg==", "format": "wav" } })
        );
        assert_eq!(parts[6]["input_audio"]["format"], "mp3");
        assert_eq!(set_media_seconds(&input), Some(91), "90 read, 1 estimated");
        assert_eq!(
            set_media_seconds(&single(SourceKind::Text, vec![])),
            None,
            "no recording, no media time"
        );

        // What the protocol has no part for is a configuration error,
        // never a malformed request.
        let webm = single(SourceKind::Audio, vec![inline("audio/webm", vec![0; 4])]);
        assert!(matches!(
            build_extract_request("m", &webm, AiOutput::Schema).unwrap_err(),
            LlmError::Config(msg) if msg.contains("wav and mp3")
        ));
        for (kind, part) in [
            (SourceKind::Pdf, inline("application/pdf", b"%PDF".to_vec())),
            (SourceKind::Video, inline("video/mp4", vec![0; 4])),
            (
                SourceKind::Youtube,
                SourcePart::YoutubeUrl("https://youtu.be/x".into()),
            ),
            (
                SourceKind::Video,
                SourcePart::FileUri {
                    mime: "video/mp4".into(),
                    uri: "https://files.example/abc".into(),
                },
            ),
        ] {
            assert!(
                matches!(
                    build_extract_request("m", &single(kind, vec![part]), AiOutput::Schema),
                    Err(LlmError::Config(_))
                ),
                "{kind} is refused as configuration"
            );
        }
        assert_eq!(audio_format("audio/WAV; codecs=1"), Some("wav"));
        assert_eq!(audio_format("audio/mp4"), None);
    }

    #[test]
    fn the_output_modes_ask_for_the_shape_their_own_way() {
        let input = single(SourceKind::Text, vec![SourcePart::Text("notes".into())]);
        let schema_mode = body_of(&build_extract_request("m", &input, AiOutput::Schema).unwrap());
        assert_eq!(
            schema_mode["messages"][0]["content"],
            prompts::EXTRACT_SYSTEM
        );
        assert_eq!(schema_mode["response_format"]["type"], "json_schema");

        let json_mode = body_of(&build_extract_request("m", &input, AiOutput::Json).unwrap());
        assert_eq!(
            json_mode["response_format"],
            json!({ "type": "json_object" })
        );
        let system = json_mode["messages"][0]["content"].as_str().unwrap();
        assert!(system.starts_with(prompts::EXTRACT_SYSTEM));
        assert!(system.ends_with(&schema::extraction_schema().to_string()));

        let plain = body_of(&build_extract_request("m", &input, AiOutput::Plain).unwrap());
        assert!(plain.get("response_format").is_none());
        assert_eq!(plain["messages"][0]["content"], system);
    }

    #[test]
    fn writing_requests_pick_their_prompt_and_schema() {
        let extraction = FakeProvider::canned_extraction();
        let drafts = FakeProvider::canned_drafts();

        let generate = body_of(&build_generate_request(
            "m",
            &extraction,
            "focus on valves",
            &[],
            AiOutput::Schema,
        ));
        assert_eq!(generate["messages"][0]["content"], prompts::GENERATE_SYSTEM);
        assert_eq!(
            generate["messages"][1]["content"][0]["text"],
            prompts::generate_user_text(&extraction, "focus on valves", &[])
        );
        assert_eq!(generate["response_format"]["json_schema"]["name"], "drafts");
        assert_eq!(
            generate["response_format"]["json_schema"]["schema"],
            schema::drafts_schema()
        );
        assert_eq!(generate["temperature"], 0.7);

        let more = body_of(&build_generate_request(
            "m",
            &extraction,
            "",
            &drafts,
            AiOutput::Schema,
        ));
        assert_eq!(more["messages"][0]["content"], prompts::MORE_SYSTEM);
        assert!(more["messages"][1]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("<<<EXISTING_CARDS"));

        let edit = EditInput {
            extraction: &extraction,
            thread: &[],
            message: "shorter",
            open: &drafts,
            kept: &[],
        };
        let revise = body_of(&build_revise_request("m", &edit, AiOutput::Schema));
        assert_eq!(revise["messages"][0]["content"], prompts::EDIT_SYSTEM);
        assert_eq!(
            revise["messages"][1]["content"][0]["text"],
            prompts::edit_user_text(&edit)
        );
        assert_eq!(
            revise["response_format"]["json_schema"]["schema"],
            schema::revision_schema()
        );
    }

    #[test]
    fn voice_requests_are_the_golden_shape() {
        let card = CardContext {
            front: "How many chambers?".into(),
            back: "Four".into(),
            note_type: NoteType::Basic,
            extraction: Some("The heart has four chambers.".into()),
            spelled: vec![],
        };
        let front = DialogState::Front;
        let turn = build_turn_request(
            "g",
            &card,
            front,
            SpokenInput::Transcript("four I think"),
            AiOutput::Schema,
        )
        .unwrap();
        assert_eq!(
            body_of(&turn),
            json!({
                "model": "g",
                "messages": [
                    { "role": "system", "content": prompts::TURN_SYSTEM },
                    { "role": "user", "content": [
                        { "type": "text", "text": prompts::turn_user_text(&card, front, "four I think") }
                    ] }
                ],
                "temperature": 0.1,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": { "name": "turn", "schema": schema::turn_schema(), "strict": true }
                }
            })
        );
        let heard = build_turn_request(
            "g",
            &card,
            front,
            SpokenInput::Audio {
                mime: "audio/wav",
                bytes: b"RIFF",
            },
            AiOutput::Schema,
        )
        .unwrap();
        assert_eq!(
            user_parts(&heard),
            vec![
                json!({ "type": "text", "text": prompts::turn_audio_user_text(&card, front) }),
                json!({ "type": "input_audio", "input_audio": { "data": "UklGRg==", "format": "wav" } })
            ]
        );
        assert!(matches!(
            build_turn_request(
                "g",
                &card,
                front,
                SpokenInput::Audio {
                    mime: "audio/webm",
                    bytes: b"x"
                },
                AiOutput::Schema
            ),
            Err(LlmError::Config(_))
        ));
        assert_eq!(estimated_seconds("audio/wav", 320_000), 10);
        assert_eq!(estimated_seconds("audio/mpeg", 160_000), 10);
        assert_eq!(estimated_seconds("audio/wav", 10), 1, "never zero");
    }

    #[test]
    fn a_reply_is_read_from_the_first_choice_with_its_usage() {
        let raw = json!({
            "id": "chatcmpl-1",
            "model": "m-2026",
            "choices": [{
                "message": { "role": "assistant", "content": "{\"text\": \"Four chambers.\", \"figures\": [], \"est_words\": 2}" },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100, "completion_tokens": 30,
                "prompt_tokens_details": { "cached_tokens": 40, "audio_tokens": 25 },
                "completion_tokens_details": { "reasoning_tokens": 10 }
            }
        });
        let value = parse_reply(&raw).unwrap();
        assert_eq!(extraction_from(value).unwrap().est_words, 2);
        let usage = usage_of(&raw, Some(12), 7);
        assert_eq!(usage.prompt, [(Modality::Text, 75), (Modality::Audio, 25)]);
        assert_eq!(usage.cached, [(Modality::Text, 40)]);
        assert_eq!((usage.candidates, usage.thoughts), (20, 10));
        assert_eq!(usage.input_tokens(), 100);
        assert_eq!(usage.output_tokens(), 30);
        assert_eq!(usage.media_seconds, Some(12));
        assert_eq!(usage.ms, 7);
        assert_eq!(usage.response_id.as_deref(), Some("chatcmpl-1"));
        assert_eq!(usage.model_version.as_deref(), Some("m-2026"));
        assert!(usage.raw.as_deref().unwrap().contains("prompt_tokens"));
        let bare = usage_of(
            &json!({ "usage": { "prompt_tokens": 5, "completion_tokens": 1 } }),
            None,
            1,
        );
        assert_eq!(bare.prompt, [(Modality::Text, 5)]);
        assert!(bare.cached.is_empty());
        assert_eq!(usage_of(&json!({}), None, 1).raw, None);

        // Text parts, a fenced block, and prose around the object all
        // read to the same JSON.
        let parts = json!({ "choices": [{ "message": { "content": [
            { "type": "text", "text": "{\"cards\": " }, { "type": "text", "text": "[]}" }
        ] } }] });
        assert_eq!(parse_reply(&parts).unwrap(), json!({ "cards": [] }));
        let fenced =
            json!({ "choices": [{ "message": { "content": "```json\n{\"cards\": []}\n```" } }] });
        assert_eq!(parse_reply(&fenced).unwrap(), json!({ "cards": [] }));
        let chatty = json!({ "choices": [{ "message": { "content": "Sure! {\"cards\": []} Hope that helps." } }] });
        assert_eq!(parse_reply(&chatty).unwrap(), json!({ "cards": [] }));
    }

    #[test]
    fn refusals_truncations_and_junk_map_to_their_errors() {
        let refusal =
            json!({ "choices": [{ "message": { "refusal": "I can't help with that." } }] });
        assert_eq!(parse_reply(&refusal).unwrap_err(), LlmError::refused());
        let filtered = json!({ "choices": [{ "message": { "content": "" }, "finish_reason": "content_filter" }] });
        assert_eq!(parse_reply(&filtered).unwrap_err(), LlmError::refused());
        let cut = json!({ "choices": [{ "message": { "content": "{\"cards\": [" }, "finish_reason": "length" }] });
        assert!(matches!(
            parse_reply(&cut).unwrap_err(),
            LlmError::Schema { detail, .. } if detail.contains("truncated")
        ));
        for junk in [
            json!({ "choices": [] }),
            json!({ "choices": [{ "message": { "content": "   " } }] }),
            json!({ "choices": [{ "message": { "content": "not json" } }] }),
            json!({ "choices": [{ "message": { "content": "{not json}" } }] }),
        ] {
            assert!(matches!(
                parse_reply(&junk).unwrap_err(),
                LlmError::Schema { .. }
            ));
        }
        // A refusal after the prompt was read carries what was billed.
        let usage = usage_of(&json!({ "usage": { "prompt_tokens": 40 } }), None, 1);
        let err = parse_reply(&refusal).unwrap_err().billed(usage.clone());
        assert_eq!(err.usage(), Some(&usage));
        assert_eq!(LlmError::http(503, "x").billed(usage).usage(), None);
    }

    #[test]
    fn the_provider_names_its_host_and_models_but_never_its_key_or_path() {
        let p = OpenAiProvider::new(
            "https://api.example.com/v1/secret?token=x".into(),
            "not-a-real-key".into(),
            "m".into(),
            "g".into(),
            AiOutput::Schema,
        );
        assert_eq!(
            p.describe(),
            "openai-compatible (api.example.com; m; grader g)"
        );
        assert_eq!(p.model_names(), ("m".to_string(), "g".to_string()));
        let local = OpenAiProvider::new(
            "http://127.0.0.1:11434/v1/".into(),
            String::new(),
            "local".into(),
            "local".into(),
            AiOutput::Json,
        );
        assert_eq!(
            local.describe(),
            "openai-compatible (127.0.0.1:11434; local; grader local)"
        );
        assert!(
            local.base_url.ends_with("/v1"),
            "the trailing slash is dropped"
        );
        assert!(!p.describe().contains("secret") && !p.describe().contains("not-a-real-key"));
        assert_eq!(
            capabilities().kinds,
            [SourceKind::Text, SourceKind::Image, SourceKind::Audio]
        );
        assert!(!capabilities().supports_youtube);
    }

    /// One request as the loopback server saw it.
    struct Seen {
        path: String,
        authorization: Option<String>,
        body: Value,
    }

    /// A loopback server that answers each connection with the next of
    /// `replies` (status and body), recording what it was sent.
    fn serve(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<Seen>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, reply) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head_end, content_length) = loop {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0, "connection closed before the head");
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_string();
                        let length = head
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            })
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        break (pos + 4, length);
                    }
                };
                while buf.len() < head_end + content_length {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0, "connection closed before the body");
                    buf.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("")
                    .to_string();
                let authorization = head.lines().find_map(|l| {
                    l.split_once(':')
                        .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                        .map(|(_, v)| v.trim().to_string())
                });
                let body: Value =
                    serde_json::from_slice(&buf[head_end..head_end + content_length]).unwrap();
                seen.push(Seen {
                    path,
                    authorization,
                    body,
                });
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
            seen
        });
        (format!("http://127.0.0.1:{port}/v1"), handle)
    }

    #[test]
    fn a_call_crosses_loopback_with_the_bearer_and_maps_statuses() {
        let ok = json!({
            "id": "chatcmpl-9",
            "choices": [{ "message": { "content": "{\"text\": \"Four chambers.\", \"figures\": [], \"est_words\": 2}" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 12, "completion_tokens": 8 }
        });
        let (base, server) = serve(vec![
            (200, ok.to_string()),
            (
                429,
                json!({ "error": { "message": "slow down" } }).to_string(),
            ),
            (200, ok.to_string()),
        ]);
        let keyed = OpenAiProvider::new(
            base.clone(),
            "k".into(),
            "m".into(),
            "m".into(),
            AiOutput::Schema,
        );
        let input = single(SourceKind::Text, vec![SourcePart::Text("notes".into())]);
        let first = keyed.extract(&input).unwrap();
        assert_eq!(first.value.text, "Four chambers.");
        assert_eq!(first.usage.input_tokens(), 12);
        assert_eq!(first.usage.response_id.as_deref(), Some("chatcmpl-9"));
        assert!(matches!(
            keyed.extract(&input).unwrap_err(),
            LlmError::Http { status: 429, excerpt } if excerpt.contains("slow down")
        ));
        let bare = OpenAiProvider::new(
            base,
            String::new(),
            "m".into(),
            "m".into(),
            AiOutput::Schema,
        );
        bare.extract(&input).unwrap();

        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].path, "/v1/chat/completions");
        assert_eq!(seen[0].authorization.as_deref(), Some("Bearer k"));
        assert_eq!(seen[0].body["model"], "m");
        assert_eq!(seen[0].body["response_format"]["type"], "json_schema");
        assert_eq!(seen[2].authorization, None, "an empty key sends no bearer");
    }

    #[test]
    fn a_set_past_the_request_ceiling_is_refused_before_any_call() {
        let p = OpenAiProvider::new(
            "http://127.0.0.1:9/v1".into(),
            String::new(),
            "m".into(),
            "m".into(),
            AiOutput::Schema,
        );
        let huge = single(
            SourceKind::Image,
            vec![inline("image/png", vec![0; REQUEST_LIMIT as usize + 1])],
        );
        assert!(matches!(
            p.extract(&huge).unwrap_err(),
            LlmError::Http { status: 413, .. }
        ));
    }
}
