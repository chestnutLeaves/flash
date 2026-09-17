//! The Gemini API as an `LlmProvider`. One request shape
//! (`RequestBody`) built by a pure function per step, one private
//! `send` that carries it over HTTPS, and the reply read back as JSON
//! in the shape `schema` asked for. Sources up to `INLINE_LIMIT` travel
//! inside the request; larger ones go through the File API first and
//! are deleted from it afterwards, so nothing of the person's material
//! stays on the provider's side longer than the call.
//!
//! The key is sent as a header and never logged; a failed call is
//! logged as its status and a short excerpt of the body.

use std::time::{Duration, Instant};

use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use super::prompts;
use super::provider::{
    Capabilities, CardContext, DialogState, DraftNote, EditInput, ExtractInput, ExtractSource,
    Extraction, LlmError, LlmProvider, Modality, Reply, Revision, SourceKind, SourcePart,
    SpokenInput, SpokenTurn, Usage,
};
use super::replies::{drafts_from, extraction_from, reply, revision_from, turn_from};
use super::schema;
use super::transport::{elapsed_ms, transport_error};
use crate::config::AiConfig;

const API: &str = "https://generativelanguage.googleapis.com";

/// The models used when the operator names none.
pub const DEFAULT_MODEL: &str = "gemini-3.8-flash";
/// The smaller model for grading. There is no "lite" of the 3.8 line:
/// 3.5 Flash-Lite is the current one, and the only Lite that prices
/// audio the same as text (checked against the models list 2026-09-13).
pub const DEFAULT_GRADER_MODEL: &str = "gemini-3.5-flash-lite";

/// Raw bytes of source that travel inside one request, over every
/// inline part. The API takes 20 MB of request; base64 makes 14 MiB
/// into about 18.7 MiB, and the prompt takes the rest. Anything past it
/// goes through the File API first.
pub const INLINE_LIMIT: usize = 14 * 1024 * 1024;

/// What the API will take in one request, before encoding.
const REQUEST_LIMIT: u64 = 20_000_000;
/// The File API's ceiling on one uploaded file.
const FILE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
/// Binary parts the API accepts in one request.
const MAX_PARTS: usize = 100;

/// The types the Gemini models read, by kind, as the API documents
/// them. `gif`, `bmp` and `avif`, which the editor's own uploads know,
/// are left out on purpose so a photo in one of them is refused with a
/// sentence at upload rather than by the API at extraction.
pub fn capabilities() -> Capabilities {
    Capabilities {
        kinds: SourceKind::ALL.to_vec(),
        mimes: vec![
            (SourceKind::Pdf, vec!["application/pdf"]),
            (
                SourceKind::Image,
                vec![
                    "image/png",
                    "image/jpeg",
                    "image/webp",
                    "image/heic",
                    "image/heif",
                ],
            ),
            (
                SourceKind::Video,
                vec![
                    "video/mp4",
                    "video/mpeg",
                    "video/quicktime",
                    "video/webm",
                    "video/x-msvideo",
                    "video/x-flv",
                    "video/mpg",
                    "video/wmv",
                    "video/3gpp",
                ],
            ),
            (
                SourceKind::Audio,
                vec![
                    "audio/wav",
                    "audio/mpeg",
                    "audio/mp3",
                    "audio/aiff",
                    "audio/aac",
                    "audio/ogg",
                    "audio/flac",
                    "audio/mp4",
                ],
            ),
        ],
        max_inline_bytes: INLINE_LIMIT as u64,
        max_request_bytes: REQUEST_LIMIT,
        max_file_bytes: Some(FILE_LIMIT),
        max_parts: MAX_PARTS,
        supports_youtube: true,
        max_media_seconds: None,
    }
}

/// One call with text, a document or pictures.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// One call that has to watch or listen to something.
const LONG_CALL_TIMEOUT: Duration = Duration::from_secs(600);
/// Pushing a large file to the File API.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// A quick administrative call (file status, delete).
const ADMIN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long an uploaded file may sit in PROCESSING before we give up.
const FILE_READY_MAX: Duration = Duration::from_secs(300);
const FILE_READY_POLL: Duration = Duration::from_secs(2);
/// Most of a reply body we will read.
const REPLY_LIMIT: u64 = 16 * 1024 * 1024;

const MEDIA_RESOLUTION_LOW: &str = "MEDIA_RESOLUTION_LOW";

/// Temperatures per step: reading and grading want the same answer
/// every time; writing cards wants some variety between passes.
const TEMP_EXTRACT: f64 = 0.2;
const TEMP_WRITE: f64 = 0.7;
const TEMP_TURN: f64 = 0.1;

pub struct GeminiProvider {
    api_key: String,
    model: String,
    grader_model: String,
    agent: ureq::Agent,
}

// ---- the request ----

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    pub system_instruction: Content,
    pub contents: Vec<Content>,
    pub generation_config: GenerationConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub mime_type: String,
    /// Base64 of the bytes.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    /// Absent for a public video URL, which the provider types itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    pub file_uri: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    pub response_mime_type: String,
    pub response_schema: Value,
    pub temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<String>,
}

impl Part {
    fn text(text: impl Into<String>) -> Self {
        Part {
            text: Some(text.into()),
            inline_data: None,
            file_data: None,
        }
    }

    fn inline(mime: &str, bytes: &[u8]) -> Self {
        Part {
            text: None,
            inline_data: Some(InlineData {
                mime_type: mime.to_string(),
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
            file_data: None,
        }
    }

    fn file(mime: Option<&str>, uri: &str) -> Self {
        Part {
            text: None,
            inline_data: None,
            file_data: Some(FileData {
                mime_type: mime.map(str::to_string),
                file_uri: uri.to_string(),
            }),
        }
    }
}

/// The standard schema in the dialect `generateContent` takes as
/// `responseSchema`: type names in capitals, `propertyOrdering` in
/// place of the closing `additionalProperties` (the order is the one
/// `required` lists, which is the writing order), everything else as
/// it is.
fn response_schema(standard: &Value) -> Value {
    match standard {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                match key.as_str() {
                    "additionalProperties" => {}
                    "type" => {
                        let upper = value
                            .as_str()
                            .map(|t| Value::String(t.to_ascii_uppercase()))
                            .unwrap_or_else(|| value.clone());
                        out.insert(key.clone(), upper);
                    }
                    "properties" => {
                        let props = value
                            .as_object()
                            .map(|p| {
                                p.iter()
                                    .map(|(k, v)| (k.clone(), response_schema(v)))
                                    .collect::<serde_json::Map<_, _>>()
                            })
                            .map(Value::Object)
                            .unwrap_or_else(|| value.clone());
                        out.insert(key.clone(), props);
                    }
                    "items" => {
                        out.insert(key.clone(), response_schema(value));
                    }
                    _ => {
                        out.insert(key.clone(), value.clone());
                    }
                }
            }
            if let Some(required) = map.get("required") {
                out.insert("propertyOrdering".into(), required.clone());
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn request(
    system: &str,
    parts: Vec<Part>,
    standard_schema: Value,
    temperature: f64,
    media_resolution: Option<&str>,
) -> RequestBody {
    RequestBody {
        system_instruction: Content {
            role: None,
            parts: vec![Part::text(system)],
        },
        contents: vec![Content {
            role: Some("user".into()),
            parts,
        }],
        generation_config: GenerationConfig {
            response_mime_type: "application/json".into(),
            response_schema: response_schema(&standard_schema),
            temperature,
            media_resolution: media_resolution.map(str::to_string),
        },
    }
}

/// Decides, part by part in request order, which inline parts travel
/// inside the request and which go through the File API first: a part
/// is uploaded when it is over `INLINE_LIMIT` on its own or when the
/// parts already kept inline would pass it together, so five documents
/// of a few megabytes each do not add up past what one request takes.
/// Returns `(source index, part index)` of every part to upload.
pub fn plan_uploads(input: &ExtractInput) -> Vec<(usize, usize)> {
    let mut inline: usize = 0;
    let mut uploads = Vec::new();
    for (si, source) in input.sources.iter().enumerate() {
        for (pi, part) in source.parts.iter().enumerate() {
            let SourcePart::Inline { bytes, .. } = part else {
                continue;
            };
            if bytes.len() > INLINE_LIMIT || inline + bytes.len() > INLINE_LIMIT {
                uploads.push((si, pi));
            } else {
                inline += bytes.len();
            }
        }
    }
    uploads
}

/// The extraction request. The notice and the count open the message;
/// each source then follows as its introduction line (with its text
/// fenced, for a text source) and its binary or file parts in order.
/// Video is read at low resolution: one frame's detail is not what a
/// lecture is about, and it is a quarter of the tokens.
pub fn build_extract_request(input: &ExtractInput) -> RequestBody {
    let mut out = vec![Part::text(prompts::extract_intro(input.sources.len()))];
    for source in &input.sources {
        out.push(Part::text(prompts::source_heading(source)));
        for part in &source.parts {
            match part {
                SourcePart::Text(_) => {}
                SourcePart::Inline { mime, bytes } => out.push(Part::inline(mime, bytes)),
                SourcePart::FileUri { mime, uri } => out.push(Part::file(Some(mime), uri)),
                SourcePart::YoutubeUrl(url) => out.push(Part::file(None, url)),
            }
        }
    }
    let resolution = input.has_video().then_some(MEDIA_RESOLUTION_LOW);
    request(
        prompts::EXTRACT_SYSTEM,
        out,
        schema::extraction_schema(),
        TEMP_EXTRACT,
        resolution,
    )
}

/// The card-writing request; with `existing` drafts it becomes the
/// "more" step, whose prompt forbids repeating them.
pub fn build_generate_request(
    extraction: &Extraction,
    instruction: &str,
    existing: &[DraftNote],
) -> RequestBody {
    let system = if existing.is_empty() {
        prompts::GENERATE_SYSTEM
    } else {
        prompts::MORE_SYSTEM
    };
    request(
        system,
        vec![Part::text(prompts::generate_user_text(
            extraction,
            instruction,
            existing,
        ))],
        schema::drafts_schema(),
        TEMP_WRITE,
        None,
    )
}

/// One message of the review's thread, answered with the new open set
/// and a sentence.
pub fn build_revise_request(edit: &EditInput<'_>) -> RequestBody {
    request(
        prompts::EDIT_SYSTEM,
        vec![Part::text(prompts::edit_user_text(edit))],
        schema::revision_schema(),
        TEMP_WRITE,
        None,
    )
}

/// The spoken turn's request: the state, the card and the transcript as
/// one text part, or the state and the card as text and the recording
/// as the inline part after it.
pub fn build_turn_request(
    card: &CardContext,
    state: DialogState,
    spoken: SpokenInput<'_>,
) -> RequestBody {
    let parts = match spoken {
        SpokenInput::Transcript(text) => {
            vec![Part::text(prompts::turn_user_text(card, state, text))]
        }
        SpokenInput::Audio { mime, bytes } => vec![
            Part::text(prompts::turn_audio_user_text(card, state)),
            Part::inline(mime, bytes),
        ],
    };
    request(
        prompts::TURN_SYSTEM,
        parts,
        schema::turn_schema(),
        TEMP_TURN,
        None,
    )
}

// ---- the Interactions API: long video ----

/// A video at or past this length is read agentically: the model
/// navigates the timeline and loads the frames, audio and transcript
/// the prompt needs, rather than sampling the whole thing at one frame
/// a second. Google's own guidance draws the line at long-form
/// material; below it the sampling is cheaper and faster.
pub const AGENTIC_VIDEO_SECONDS: u32 = 10 * 60;

/// Whether a source is read by navigating it. A file whose length
/// ingest could not read, and a link, whose length only the model
/// learns, are taken to be long: a lecture is the common case, and
/// the navigation costs little on a short one.
pub fn reads_agentically(source: &ExtractSource) -> bool {
    match source.kind {
        SourceKind::Youtube => true,
        SourceKind::Video => source
            .duration_s
            .is_none_or(|seconds| seconds >= AGENTIC_VIDEO_SECONDS),
        _ => false,
    }
}

/// One `POST /v1beta/interactions` body: the same words and the same
/// schema as the `generateContent` request, in the shape that API
/// takes, with `processing: agentic` on the videos that earn it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InteractionRequest {
    pub model: String,
    pub system_instruction: String,
    pub input: Vec<InputItem>,
    pub response_format: ResponseFormat,
    pub generation_config: InteractionConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputItem {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Base64 of the bytes, for a part that travels in the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub mime_type: &'static str,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InteractionConfig {
    pub temperature: f64,
}

impl InputItem {
    fn text(text: impl Into<String>) -> Self {
        InputItem {
            kind: "text",
            text: Some(text.into()),
            data: None,
            uri: None,
            mime_type: None,
            processing: None,
        }
    }

    /// The item type a part of `kind` is sent as.
    fn media_kind(kind: SourceKind) -> &'static str {
        match kind {
            SourceKind::Pdf => "document",
            SourceKind::Image => "image",
            SourceKind::Video | SourceKind::Youtube => "video",
            SourceKind::Audio => "audio",
            SourceKind::Text => "text",
        }
    }
}

/// The extraction as an interaction: intro, then per source its
/// introduction line and its parts, each video marked agentic when it
/// reads that way.
pub fn build_interaction_request(model: &str, input: &ExtractInput) -> InteractionRequest {
    let mut items = vec![InputItem::text(prompts::extract_intro(input.sources.len()))];
    for source in &input.sources {
        items.push(InputItem::text(prompts::source_heading(source)));
        let processing = reads_agentically(source).then_some("agentic");
        let kind = InputItem::media_kind(source.kind);
        for part in &source.parts {
            let item = match part {
                SourcePart::Text(_) => continue,
                SourcePart::Inline { mime, bytes } => InputItem {
                    kind,
                    text: None,
                    data: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                    uri: None,
                    mime_type: Some(mime.clone()),
                    processing,
                },
                SourcePart::FileUri { mime, uri } => InputItem {
                    kind,
                    text: None,
                    data: None,
                    uri: Some(uri.clone()),
                    mime_type: Some(mime.clone()),
                    processing,
                },
                SourcePart::YoutubeUrl(url) => InputItem {
                    kind: "video",
                    text: None,
                    data: None,
                    uri: Some(url.clone()),
                    mime_type: None,
                    processing,
                },
            };
            items.push(item);
        }
    }
    InteractionRequest {
        model: model.to_string(),
        system_instruction: prompts::EXTRACT_SYSTEM.to_string(),
        input: items,
        response_format: ResponseFormat {
            kind: "text",
            mime_type: "application/json",
            schema: schema::extraction_schema(),
        },
        generation_config: InteractionConfig {
            temperature: TEMP_EXTRACT,
        },
    }
}

/// The JSON the model wrote, out of an interaction's envelope: the text
/// of the `model_output` step. An interaction that did not complete is
/// a schema error naming its status.
pub fn parse_interaction_reply(raw: &Value) -> Result<Value, LlmError> {
    let status = raw["status"].as_str().unwrap_or("");
    if !status.is_empty() && status != "completed" {
        let detail = raw["error"]["message"]
            .as_str()
            .or_else(|| raw["incomplete_details"]["reason"].as_str())
            .unwrap_or("");
        if matches!(detail, d if d.to_ascii_lowercase().contains("safety")) {
            return Err(LlmError::refused());
        }
        return Err(LlmError::schema(format!("interaction {status}: {detail}")));
    }
    let text: String = raw["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|step| step["type"].as_str() == Some("model_output"))
        .flat_map(|step| step["content"].as_array().cloned().unwrap_or_default())
        .filter_map(|item| item["text"].as_str().map(str::to_string))
        .collect();
    if text.trim().is_empty() {
        return Err(LlmError::schema("no model output"));
    }
    serde_json::from_str(&text).map_err(|e| LlmError::schema(format!("reply is not JSON: {e}")))
}

/// A `[{modality, <count_key>}]` list as (kind, tokens) pairs, one pair
/// per kind. A kind this vocabulary lacks is counted as text and said
/// in the journal, so no billed token goes unrecorded.
fn tokens_by_modality(list: &Value, count_key: &str) -> Vec<(Modality, u32)> {
    let mut out: Vec<(Modality, u32)> = Vec::new();
    for entry in list.as_array().into_iter().flatten() {
        let tokens = entry[count_key].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32;
        let name = entry["modality"].as_str().unwrap_or("");
        let modality = Modality::parse(name).unwrap_or_else(|| {
            tracing::warn!(
                modality = name,
                tokens,
                "unknown token modality counted as text"
            );
            Modality::Text
        });
        match out.iter_mut().find(|(m, _)| *m == modality) {
            Some((_, n)) => *n = n.saturating_add(tokens),
            None => out.push((modality, tokens)),
        }
    }
    out
}

fn tokens_of(list: &[(Modality, u32)], modality: Modality) -> u32 {
    list.iter()
        .find(|(m, _)| *m == modality)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

/// A per-kind list made to add up to the provider's own total: the
/// shortfall is text (a record without the list is all text), and an
/// excess is said in the journal and left as listed, since the list is
/// what the provider bills by.
fn reconciled(mut list: Vec<(Modality, u32)>, total: u32, what: &str) -> Vec<(Modality, u32)> {
    let listed = list.iter().fold(0u32, |n, (_, t)| n.saturating_add(*t));
    if listed < total {
        let short = total - listed;
        match list.iter_mut().find(|(m, _)| *m == Modality::Text) {
            Some((_, n)) => *n = n.saturating_add(short),
            None => list.push((Modality::Text, short)),
        }
    } else if listed > total {
        tracing::warn!(
            listed,
            total,
            what,
            "token kinds add up to more than the total"
        );
    }
    list
}

/// An interaction's `usage` as a `Usage`. What the model loaded while
/// navigating (frames, audio, transcript) is billed as input, and its
/// navigation reasoning as output, so both are counted that way. The
/// media time is `known` (the lengths ingest read) plus, when a member's
/// length was not known, the time the loaded frames and audio stand for.
pub fn interaction_usage(raw: &Value, known: u32, some_unknown: bool, ms: u32) -> Usage {
    let usage = &raw["usage"];
    let count = |k: &str| usage[k].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32;
    let prompt_total = count("total_input_tokens");
    let prompt = reconciled(
        tokens_by_modality(&usage["input_tokens_by_modality"], "tokens"),
        prompt_total,
        "interaction input",
    );
    let tool_use = reconciled(
        tokens_by_modality(&usage["tool_use_tokens_by_modality"], "tokens"),
        count("total_tool_use_tokens"),
        "interaction tool use",
    );
    let loaded = tokens_of(&tool_use, Modality::Image) / VIDEO_TOKENS_PER_SECOND
        + tokens_of(&tool_use, Modality::Video) / VIDEO_TOKENS_PER_SECOND
        + tokens_of(&tool_use, Modality::Audio) / AUDIO_TOKENS_PER_SECOND;
    let media_seconds = known.saturating_add(if some_unknown { loaded } else { 0 });
    Usage {
        prompt,
        cached: tokens_by_modality(&usage["cached_tokens_by_modality"], "tokens"),
        tool_use,
        prompt_total,
        candidates: count("total_output_tokens"),
        thoughts: count("total_thought_tokens"),
        media_seconds: Some(media_seconds),
        ms,
        response_id: raw["id"].as_str().map(str::to_string),
        model_version: raw["model"].as_str().map(str::to_string),
        raw: usage.is_object().then(|| usage.to_string()),
    }
}

// ---- the reply ----

/// The JSON the model wrote, out of the API's envelope. A blocked
/// prompt or a safety stop is a refusal; a missing or truncated answer
/// is a schema error; thought parts (reasoning models) are skipped.
pub fn parse_reply(raw: &Value) -> Result<Value, LlmError> {
    if raw["promptFeedback"]["blockReason"]
        .as_str()
        .is_some_and(|r| !r.is_empty())
    {
        return Err(LlmError::refused());
    }
    let candidate = raw["candidates"]
        .get(0)
        .ok_or_else(|| LlmError::schema("no candidates"))?;
    match candidate["finishReason"].as_str() {
        Some("SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" | "RECITATION") => {
            return Err(LlmError::refused())
        }
        Some("MAX_TOKENS") => return Err(LlmError::schema("reply truncated")),
        _ => {}
    }
    let text: String = candidate["content"]["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p["thought"].as_bool() != Some(true))
                .filter_map(|p| p["text"].as_str())
                .collect()
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(LlmError::schema("empty reply"));
    }
    serde_json::from_str(&text).map_err(|e| LlmError::schema(format!("reply is not JSON: {e}")))
}

/// Tokens a second of media costs: low-resolution video about 100,
/// audio 32.
const VIDEO_TOKENS_PER_SECOND: u32 = 100;
const AUDIO_TOKENS_PER_SECOND: u32 = 32;

/// Media time the prompt's token count stands for when the reply does
/// not say which tokens were which: the whole prompt at the rate of
/// the dearest kind in the set.
pub fn media_seconds(kind: SourceKind, prompt_tokens: u32) -> Option<u32> {
    match kind {
        SourceKind::Video | SourceKind::Youtube => Some(prompt_tokens / VIDEO_TOKENS_PER_SECOND),
        SourceKind::Audio => Some(prompt_tokens / AUDIO_TOKENS_PER_SECOND),
        SourceKind::Text | SourceKind::Pdf | SourceKind::Image => None,
    }
}

/// Media time from the prompt's per-kind counts, which say how many
/// prompt tokens were video and how many audio, so a PDF beside a
/// recording is not metered as if it were watched.
fn media_seconds_by_modality(prompt: &[(Modality, u32)]) -> u32 {
    (tokens_of(prompt, Modality::Video) / VIDEO_TOKENS_PER_SECOND)
        .saturating_add(tokens_of(prompt, Modality::Audio) / AUDIO_TOKENS_PER_SECOND)
}

/// `usageMetadata` as a `Usage`: every count the provider bills by,
/// kept apart the way it prices them. Reasoning tokens count as
/// output: they are billed as such. `kind` is the dearest kind of the
/// set, for the media estimate a reply without per-kind counts falls
/// back to; None for a call that read no source.
pub fn usage_of(raw: &Value, kind: Option<SourceKind>, ms: u32) -> Usage {
    let meta = &raw["usageMetadata"];
    let count = |k: &str| meta[k].as_u64().unwrap_or(0).min(u32::MAX as u64) as u32;
    let prompt_total = count("promptTokenCount");
    let detailed = meta["promptTokensDetails"].is_array();
    let prompt = reconciled(
        tokens_by_modality(&meta["promptTokensDetails"], "tokenCount"),
        prompt_total,
        "prompt",
    );
    let cached = reconciled(
        tokens_by_modality(&meta["cacheTokensDetails"], "tokenCount"),
        count("cachedContentTokenCount"),
        "cached content",
    );
    let tool_use = reconciled(
        tokens_by_modality(&meta["toolUsePromptTokensDetails"], "tokenCount"),
        count("toolUsePromptTokenCount"),
        "tool use",
    );
    let media_seconds = match kind {
        Some(k) if k.is_media() && detailed => Some(media_seconds_by_modality(&prompt)),
        Some(k) if k.is_media() => media_seconds(k, prompt_total),
        _ => None,
    };
    Usage {
        prompt,
        cached,
        tool_use,
        prompt_total,
        candidates: count("candidatesTokenCount"),
        thoughts: count("thoughtsTokenCount"),
        media_seconds,
        ms,
        response_id: raw["responseId"].as_str().map(str::to_string),
        model_version: raw["modelVersion"].as_str().map(str::to_string),
        raw: meta.is_object().then(|| meta.to_string()),
    }
}

// ---- transport ----

/// A name that may become a path segment of the API URL.
fn valid_model_name(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 100
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
}

/// A file the File API holds for us: its resource name (for status and
/// delete) and the URI a request refers to it by.
struct UploadedFile {
    resource: String,
    uri: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String, grader_model: String) -> Self {
        Self {
            api_key,
            model,
            grader_model,
            // Every request sets its own overall timeout; the agent's is
            // the default for the short ones.
            agent: crate::http::agent(CALL_TIMEOUT, false),
        }
    }

    pub fn from_config(config: &AiConfig) -> Self {
        Self::new(
            config.api_key.clone(),
            config.model.clone(),
            config.grader_model.clone(),
        )
    }

    /// One `generateContent` call. The reply comes back as the JSON the
    /// model wrote plus what it cost; `kind` is the dearest kind of the
    /// set an extraction read, None for a call that read no source.
    fn send(
        &self,
        model: &str,
        body: &RequestBody,
        long: bool,
        kind: Option<SourceKind>,
    ) -> Result<(Value, Usage), LlmError> {
        if !valid_model_name(model) {
            return Err(LlmError::Config("model name is not valid".into()));
        }
        if self.api_key.is_empty() {
            return Err(LlmError::Config("no API key".into()));
        }
        let url = format!("{API}/v1beta/models/{model}:generateContent");
        let timeout = if long {
            LONG_CALL_TIMEOUT
        } else {
            CALL_TIMEOUT
        };
        let started = Instant::now();
        let response = self
            .agent
            .post(&url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("x-goog-api-key", &self.api_key)
            .send_json(body)
            .map_err(transport_error)?;
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
        let usage = usage_of(&raw, kind, ms);
        let value = parse_reply(&raw).map_err(|e| e.billed(usage.clone()))?;
        Ok((value, usage))
    }

    /// Resumable upload of one blob: start, push everything in one go,
    /// then wait for the provider to finish processing it.
    fn upload(&self, mime: &str, bytes: &[u8]) -> Result<UploadedFile, LlmError> {
        let start = self
            .agent
            .post(format!("{API}/upload/v1beta/files"))
            .config()
            .timeout_global(Some(ADMIN_TIMEOUT))
            .build()
            .header("x-goog-api-key", &self.api_key)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("X-Goog-Upload-Command", "start")
            .header(
                "X-Goog-Upload-Header-Content-Length",
                bytes.len().to_string(),
            )
            .header("X-Goog-Upload-Header-Content-Type", mime)
            .send_json(serde_json::json!({ "file": { "display_name": "source" } }))
            .map_err(transport_error)?;
        let status = start.status().as_u16();
        if !(200..300).contains(&status) {
            let text = start
                .into_body()
                .with_config()
                .limit(64 * 1024)
                .read_to_string()
                .unwrap_or_default();
            tracing::warn!(status, "file upload could not start");
            return Err(LlmError::http(status, &text));
        }
        let upload_url = start
            .headers()
            .get("x-goog-upload-url")
            .and_then(|v| v.to_str().ok())
            .filter(|u| u.starts_with("https://"))
            .map(str::to_string)
            .ok_or_else(|| LlmError::schema("upload start gave no upload URL"))?;

        let response = self
            .agent
            .post(&upload_url)
            .config()
            .timeout_global(Some(UPLOAD_TIMEOUT))
            .build()
            .header("X-Goog-Upload-Offset", "0")
            .header("X-Goog-Upload-Command", "upload, finalize")
            .send(bytes)
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .map_err(transport_error)?;
        if !(200..300).contains(&status) {
            tracing::warn!(status, "file upload failed");
            return Err(LlmError::http(status, &text));
        }
        let json: Value = serde_json::from_str(&text)
            .map_err(|e| LlmError::schema(format!("upload reply is not JSON: {e}")))?;
        let file = &json["file"];
        let resource = file["name"]
            .as_str()
            .filter(|n| n.starts_with("files/"))
            .map(str::to_string)
            .ok_or_else(|| LlmError::schema("upload reply has no file name"))?;
        let uri = file["uri"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| LlmError::schema("upload reply has no file uri"))?;
        let mut state = file["state"].as_str().unwrap_or("").to_string();
        let uploaded = UploadedFile { resource, uri };

        let deadline = Instant::now() + FILE_READY_MAX;
        while state != "ACTIVE" {
            if state == "FAILED" {
                return Err(LlmError::http(0, "uploaded file failed processing"));
            }
            if Instant::now() >= deadline {
                return Err(LlmError::Timeout);
            }
            std::thread::sleep(FILE_READY_POLL);
            state = self.file_state(&uploaded.resource)?;
        }
        Ok(uploaded)
    }

    fn file_state(&self, resource: &str) -> Result<String, LlmError> {
        let response = self
            .agent
            .get(format!("{API}/v1beta/{resource}"))
            .config()
            .timeout_global(Some(ADMIN_TIMEOUT))
            .build()
            .header("x-goog-api-key", &self.api_key)
            .call()
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .map_err(transport_error)?;
        if !(200..300).contains(&status) {
            return Err(LlmError::http(status, &text));
        }
        let json: Value = serde_json::from_str(&text)
            .map_err(|e| LlmError::schema(format!("file status is not JSON: {e}")))?;
        Ok(json["state"].as_str().unwrap_or("").to_string())
    }

    /// Best effort: the provider expires files on its own after two
    /// days; this only shortens that.
    fn delete_file(&self, resource: &str) {
        let outcome = self
            .agent
            .delete(format!("{API}/v1beta/{resource}"))
            .config()
            .timeout_global(Some(ADMIN_TIMEOUT))
            .build()
            .header("x-goog-api-key", &self.api_key)
            .call();
        match outcome {
            Ok(response) if response.status().is_success() => {}
            Ok(response) => {
                let status = response.status().as_u16();
                tracing::debug!(status, "uploaded file not deleted");
            }
            Err(e) => {
                let err = transport_error(e);
                tracing::debug!(error = ?err, "uploaded file not deleted");
            }
        }
    }

    fn extract_inner(
        &self,
        input: &ExtractInput,
        uploaded: &mut Vec<String>,
    ) -> Result<Reply<Extraction>, LlmError> {
        let to_upload = plan_uploads(input);
        let mut sent = input.clone();
        for (si, pi) in to_upload {
            let SourcePart::Inline { mime, bytes } = &input.sources[si].parts[pi] else {
                continue;
            };
            let file = self.upload(mime, bytes)?;
            uploaded.push(file.resource);
            sent.sources[si].parts[pi] = SourcePart::FileUri {
                mime: mime.clone(),
                uri: file.uri,
            };
        }
        if sent.sources.iter().any(reads_agentically) {
            let body = build_interaction_request(&self.model, &sent);
            let (value, usage) = self.send_interaction(&body, &sent)?;
            return reply(extraction_from(value), usage);
        }
        let body = build_extract_request(&sent);
        let (value, usage) = self.send(
            &self.model,
            &body,
            input.is_media(),
            Some(input.costliest()),
        )?;
        reply(extraction_from(value), usage)
    }

    /// One `POST /v1beta/interactions` call, the long timeout since a
    /// video is being navigated.
    fn send_interaction(
        &self,
        body: &InteractionRequest,
        input: &ExtractInput,
    ) -> Result<(Value, Usage), LlmError> {
        if !valid_model_name(&body.model) {
            return Err(LlmError::Config("model name is not valid".into()));
        }
        if self.api_key.is_empty() {
            return Err(LlmError::Config("no API key".into()));
        }
        let started = Instant::now();
        let response = self
            .agent
            .post(format!("{API}/v1beta/interactions"))
            .config()
            .timeout_global(Some(LONG_CALL_TIMEOUT))
            .build()
            .header("x-goog-api-key", &self.api_key)
            .send_json(body)
            .map_err(transport_error)?;
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
                tracing::warn!(status, excerpt = ?excerpt, "interaction failed");
            }
            return Err(err);
        }
        let raw: Value = serde_json::from_str(&text)
            .map_err(|e| LlmError::schema(format!("envelope is not JSON: {e}")))?;
        let known: u32 = input
            .sources
            .iter()
            .filter(|s| s.kind.is_media())
            .filter_map(|s| s.duration_s)
            .sum();
        let some_unknown = input
            .sources
            .iter()
            .any(|s| s.kind.is_media() && s.duration_s.is_none());
        let usage = interaction_usage(&raw, known, some_unknown, ms);
        let value = parse_interaction_reply(&raw).map_err(|e| e.billed(usage.clone()))?;
        Ok((value, usage))
    }
}

impl LlmProvider for GeminiProvider {
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn extract(&self, input: &ExtractInput) -> Result<Reply<Extraction>, LlmError> {
        let mut uploaded = Vec::new();
        let result = self.extract_inner(input, &mut uploaded);
        for resource in &uploaded {
            self.delete_file(resource);
        }
        result
    }

    fn generate(
        &self,
        extraction: &Extraction,
        instruction: &str,
        existing: &[DraftNote],
    ) -> Result<Reply<Vec<DraftNote>>, LlmError> {
        let body = build_generate_request(extraction, instruction, existing);
        let (value, usage) = self.send(&self.model, &body, false, None)?;
        reply(drafts_from(value), usage)
    }

    fn revise(&self, edit: &EditInput<'_>) -> Result<Reply<Revision>, LlmError> {
        let body = build_revise_request(edit);
        let (value, usage) = self.send(&self.model, &body, false, None)?;
        reply(revision_from(value), usage)
    }

    fn turn(
        &self,
        card: &CardContext,
        state: DialogState,
        spoken: SpokenInput<'_>,
    ) -> Result<Reply<SpokenTurn>, LlmError> {
        let body = build_turn_request(card, state, spoken);
        // A recording is metered as audio so the ledger sees its seconds.
        let kind = match spoken {
            SpokenInput::Transcript(_) => None,
            SpokenInput::Audio { .. } => Some(SourceKind::Audio),
        };
        let (value, usage) = self.send(&self.grader_model, &body, false, kind)?;
        reply(turn_from(value, state), usage)
    }

    fn describe(&self) -> String {
        format!("gemini ({}; grader {})", self.model, self.grader_model)
    }

    fn model_names(&self) -> (String, String) {
        (self.model.clone(), self.grader_model.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::fake::FakeProvider;
    use crate::ai::provider::{ExtractSource, FigureRef};
    use flash_store::notes::NoteType;
    use serde_json::json;

    fn config_of(body: &RequestBody) -> Value {
        serde_json::to_value(body).unwrap()["generationConfig"].clone()
    }

    fn parts_of(body: &RequestBody) -> Vec<Value> {
        serde_json::to_value(body).unwrap()["contents"][0]["parts"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn single(kind: SourceKind, parts: Vec<SourcePart>) -> ExtractInput {
        ExtractInput::single(kind, "notes", parts)
    }

    fn inline(mime: &str, bytes: Vec<u8>) -> SourcePart {
        SourcePart::Inline {
            mime: mime.into(),
            bytes,
        }
    }

    #[test]
    fn extract_request_for_text_is_the_golden_shape() {
        let input = single(SourceKind::Text, vec![SourcePart::Text("notes".into())]);
        let body = build_extract_request(&input);
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({
                "systemInstruction": { "parts": [{ "text": prompts::EXTRACT_SYSTEM }] },
                "contents": [{
                    "role": "user",
                    "parts": [
                        { "text": prompts::extract_intro(1) },
                        { "text": prompts::source_heading(&input.sources[0]) }
                    ]
                }],
                "generationConfig": {
                    "responseMimeType": "application/json",
                    "responseSchema": response_schema(&schema::extraction_schema()),
                    "temperature": 0.2
                }
            })
        );
    }

    /// The dialect form: capital type names, `propertyOrdering` from
    /// `required`, no `additionalProperties`, nothing else changed.
    #[test]
    fn the_dialect_form_uppercases_types_orders_properties_and_drops_the_closing_key() {
        fn check(v: &Value) {
            const KNOWN: &[&str] = &[
                "type",
                "properties",
                "required",
                "items",
                "enum",
                "propertyOrdering",
                "description",
            ];
            const TYPES: &[&str] = &["OBJECT", "ARRAY", "STRING", "INTEGER", "NUMBER", "BOOLEAN"];
            let obj = v.as_object().unwrap();
            for key in obj.keys() {
                assert!(KNOWN.contains(&key.as_str()), "unknown schema key {key}");
            }
            assert!(TYPES.contains(&obj["type"].as_str().unwrap()));
            if let Some(props) = obj.get("properties") {
                assert_eq!(obj["propertyOrdering"], obj["required"]);
                for child in props.as_object().unwrap().values() {
                    check(child);
                }
            }
            if let Some(items) = obj.get("items") {
                check(items);
            }
        }
        for s in [
            schema::extraction_schema(),
            schema::drafts_schema(),
            schema::revision_schema(),
            schema::turn_schema(),
        ] {
            check(&response_schema(&s));
        }
        let extraction = response_schema(&schema::extraction_schema());
        assert_eq!(
            extraction["propertyOrdering"],
            json!(["text", "figures", "est_words"])
        );
        assert_eq!(extraction["properties"]["text"]["type"], "STRING");
        assert_eq!(
            extraction["properties"]["figures"]["items"]["properties"]["bbox"]["items"]["type"],
            "NUMBER"
        );
        assert_eq!(
            extraction["properties"]["figures"]["items"]["properties"]["source"]["description"],
            schema::extraction_schema()["properties"]["figures"]["items"]["properties"]["source"]
                ["description"]
        );
    }

    #[test]
    fn a_set_is_introduced_source_by_source_in_order() {
        let input = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "notes.pdf".into(),
                    parts: vec![inline("application/pdf", b"%PDF-1.7".to_vec())],
                    duration_s: None,
                },
                ExtractSource {
                    index: 2,
                    kind: SourceKind::Text,
                    label: "Pasted text".into(),
                    parts: vec![SourcePart::Text("hello".into())],
                    duration_s: None,
                },
                ExtractSource {
                    index: 3,
                    kind: SourceKind::Image,
                    label: "page.png".into(),
                    parts: vec![inline("image/png", vec![1, 2])],
                    duration_s: None,
                },
            ],
        };
        let parts = parts_of(&build_extract_request(&input));
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[0]["text"], prompts::extract_intro(3));
        assert!(parts[1]["text"].as_str().unwrap().starts_with("Source 1:"));
        assert_eq!(
            parts[2],
            json!({ "inlineData": { "mimeType": "application/pdf", "data": "JVBERi0xLjc=" } })
        );
        assert!(parts[3]["text"]
            .as_str()
            .unwrap()
            .contains("<<<SOURCE_2\nhello\n>>>"));
        assert!(parts[4]["text"].as_str().unwrap().starts_with("Source 3:"));
        assert_eq!(parts[5]["inlineData"]["mimeType"], "image/png");
    }

    #[test]
    fn parts_travel_inline_until_the_request_would_be_too_large() {
        let small = single(
            SourceKind::Pdf,
            vec![inline("application/pdf", vec![0x25; INLINE_LIMIT])],
        );
        assert!(plan_uploads(&small).is_empty());
        let large = single(
            SourceKind::Pdf,
            vec![inline("application/pdf", vec![0x25; INLINE_LIMIT + 1])],
        );
        assert_eq!(plan_uploads(&large), [(0, 0)]);
        let text = single(
            SourceKind::Text,
            vec![SourcePart::Text("x".repeat(INLINE_LIMIT + 1))],
        );
        assert!(plan_uploads(&text).is_empty());
        let link = single(
            SourceKind::Youtube,
            vec![SourcePart::YoutubeUrl("https://youtu.be/x".into())],
        );
        assert!(plan_uploads(&link).is_empty());

        // Three parts of a third of the budget each: the first two ride
        // inline, the third would tip the request over and is uploaded,
        // and a small fourth still fits inline after it.
        let third = INLINE_LIMIT / 3 + 1;
        let set = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "a".into(),
                    parts: vec![
                        inline("application/pdf", vec![0; third]),
                        inline("application/pdf", vec![0; third]),
                    ],
                    duration_s: None,
                },
                ExtractSource {
                    index: 2,
                    kind: SourceKind::Image,
                    label: "b".into(),
                    parts: vec![
                        inline("image/png", vec![0; third]),
                        inline("image/png", vec![0; 10]),
                    ],
                    duration_s: None,
                },
            ],
        };
        assert_eq!(plan_uploads(&set), [(1, 0)]);

        let body = build_extract_request(&single(
            SourceKind::Pdf,
            vec![inline("application/pdf", b"%PDF-1.7".to_vec())],
        ));
        let parts = parts_of(&body);
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[2],
            json!({ "inlineData": { "mimeType": "application/pdf", "data": "JVBERi0xLjc=" } })
        );
        assert!(parts[1]["text"]
            .as_str()
            .unwrap()
            .contains("a PDF document"));
        assert!(config_of(&body).get("mediaResolution").is_none());

        let uploaded = build_extract_request(&single(
            SourceKind::Video,
            vec![SourcePart::FileUri {
                mime: "video/mp4".into(),
                uri: "https://generativelanguage.googleapis.com/v1beta/files/abc".into(),
            }],
        ));
        assert_eq!(
            parts_of(&uploaded)[2],
            json!({ "fileData": {
                "mimeType": "video/mp4",
                "fileUri": "https://generativelanguage.googleapis.com/v1beta/files/abc"
            } })
        );
    }

    #[test]
    fn youtube_is_a_file_uri_without_a_type_and_video_reads_at_low_resolution() {
        let body = build_extract_request(&single(
            SourceKind::Youtube,
            vec![SourcePart::YoutubeUrl(
                "https://www.youtube.com/watch?v=abc".into(),
            )],
        ));
        assert_eq!(
            parts_of(&body)[2],
            json!({ "fileData": { "fileUri": "https://www.youtube.com/watch?v=abc" } })
        );
        assert_eq!(config_of(&body)["mediaResolution"], "MEDIA_RESOLUTION_LOW");

        let video = build_extract_request(&single(
            SourceKind::Video,
            vec![inline("video/mp4", vec![0; 8])],
        ));
        assert_eq!(config_of(&video)["mediaResolution"], "MEDIA_RESOLUTION_LOW");

        for kind in [
            SourceKind::Audio,
            SourceKind::Image,
            SourceKind::Pdf,
            SourceKind::Text,
        ] {
            let body = build_extract_request(&single(
                kind,
                vec![inline("application/octet-stream", vec![0; 8])],
            ));
            assert!(
                config_of(&body).get("mediaResolution").is_none(),
                "{kind} needs no media resolution"
            );
        }
        // A video beside a PDF still reads at low resolution.
        let mixed = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "a".into(),
                    parts: vec![],
                    duration_s: None,
                },
                ExtractSource {
                    index: 2,
                    kind: SourceKind::Video,
                    label: "b".into(),
                    parts: vec![],
                    duration_s: None,
                },
            ],
        };
        assert_eq!(
            config_of(&build_extract_request(&mixed))["mediaResolution"],
            "MEDIA_RESOLUTION_LOW"
        );
    }

    fn video(index: u32, duration_s: Option<u32>, part: SourcePart) -> ExtractSource {
        ExtractSource {
            index,
            kind: SourceKind::Video,
            label: format!("clip{index}.mp4"),
            parts: vec![part],
            duration_s,
        }
    }

    #[test]
    fn a_long_video_is_navigated_and_a_short_one_is_sampled() {
        let short = video(
            1,
            Some(AGENTIC_VIDEO_SECONDS - 1),
            inline("video/mp4", vec![0; 4]),
        );
        let long = video(
            2,
            Some(AGENTIC_VIDEO_SECONDS),
            inline("video/mp4", vec![0; 4]),
        );
        let unknown = video(3, None, inline("video/mp4", vec![0; 4]));
        assert!(!reads_agentically(&short));
        assert!(reads_agentically(&long));
        assert!(
            reads_agentically(&unknown),
            "an unknown length is taken to be long"
        );
        assert!(reads_agentically(
            &ExtractInput::single(
                SourceKind::Youtube,
                "link",
                vec![SourcePart::YoutubeUrl("https://youtu.be/x".into())]
            )
            .sources[0]
        ));
        for kind in [
            SourceKind::Audio,
            SourceKind::Pdf,
            SourceKind::Image,
            SourceKind::Text,
        ] {
            let source = ExtractSource {
                index: 1,
                kind,
                label: "x".into(),
                parts: vec![],
                duration_s: Some(3 * 60 * 60),
            };
            assert!(!reads_agentically(&source), "{kind} is never navigated");
        }
    }

    #[test]
    fn an_interaction_carries_the_same_words_with_agentic_on_the_long_videos() {
        let input = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "notes.pdf".into(),
                    parts: vec![inline("application/pdf", b"%PDF-1.7".to_vec())],
                    duration_s: None,
                },
                video(
                    2,
                    Some(AGENTIC_VIDEO_SECONDS - 60),
                    inline("video/mp4", vec![1]),
                ),
                video(
                    3,
                    Some(90 * 60),
                    SourcePart::FileUri {
                        mime: "video/mp4".into(),
                        uri: "https://generativelanguage.googleapis.com/v1beta/files/abc".into(),
                    },
                ),
                ExtractSource {
                    index: 4,
                    kind: SourceKind::Youtube,
                    label: "https://www.youtube.com/watch?v=abc".into(),
                    parts: vec![SourcePart::YoutubeUrl(
                        "https://www.youtube.com/watch?v=abc".into(),
                    )],
                    duration_s: None,
                },
                ExtractSource {
                    index: 5,
                    kind: SourceKind::Text,
                    label: "Pasted text".into(),
                    parts: vec![SourcePart::Text("hello".into())],
                    duration_s: None,
                },
            ],
        };
        let body = build_interaction_request("gemini-3.8-flash", &input);
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["model"], "gemini-3.8-flash");
        assert_eq!(v["system_instruction"], prompts::EXTRACT_SYSTEM);
        assert_eq!(v["generation_config"], json!({ "temperature": 0.2 }));
        assert_eq!(v["response_format"]["type"], "text");
        assert_eq!(v["response_format"]["mime_type"], "application/json");
        assert_eq!(v["response_format"]["schema"]["type"], "object");
        assert!(v["response_format"]["schema"]
            .get("propertyOrdering")
            .is_none());
        let items = v["input"].as_array().unwrap();
        // intro; pdf heading + document; short video heading + video;
        // long video heading + video; link heading + video; text heading
        // with its text fenced inside.
        assert_eq!(items.len(), 10);
        assert_eq!(items[0]["text"], prompts::extract_intro(5));
        assert_eq!(
            items[2],
            json!({ "type": "document", "data": "JVBERi0xLjc=", "mime_type": "application/pdf" })
        );
        assert_eq!(items[4]["type"], "video");
        assert!(
            items[4].get("processing").is_none(),
            "a short clip is sampled"
        );
        assert_eq!(
            items[6],
            json!({ "type": "video", "uri": "https://generativelanguage.googleapis.com/v1beta/files/abc",
                    "mime_type": "video/mp4", "processing": "agentic" })
        );
        assert_eq!(
            items[8],
            json!({ "type": "video", "uri": "https://www.youtube.com/watch?v=abc", "processing": "agentic" })
        );
        assert!(items[9]["text"]
            .as_str()
            .unwrap()
            .contains("<<<SOURCE_5\nhello\n>>>"));
    }

    #[test]
    fn an_interactions_reply_is_read_from_its_model_output_step() {
        let raw = json!({
            "status": "completed",
            "usage": {
                "total_input_tokens": 89, "total_output_tokens": 115,
                "total_tool_use_tokens": 2192, "total_thought_tokens": 317,
                "tool_use_tokens_by_modality": [
                    { "modality": "image", "tokens": 1980 },
                    { "modality": "text", "tokens": 212 }
                ]
            },
            "steps": [
                { "type": "processing_call", "id": "call_1" },
                { "type": "processing_result", "call_id": "call_1" },
                { "type": "thought" },
                { "type": "model_output", "content": [
                    { "type": "text", "text": "{\"text\": \"The heart has four chambers.\", " },
                    { "type": "text", "text": "\"figures\": [], \"est_words\": 5}" }
                ] }
            ]
        });
        let value = parse_interaction_reply(&raw).unwrap();
        assert_eq!(extraction_from(value).unwrap().est_words, 5);
        // Loaded frames count as input, navigation as output; the media
        // time is what was known plus what the loaded frames stand for
        // when a length was not.
        let usage = interaction_usage(&raw, 600, true, 9);
        assert_eq!(usage.input_tokens(), 89 + 2192);
        assert_eq!(usage.output_tokens(), 115 + 317);
        assert_eq!(usage.media_seconds, Some(600 + 19));
        assert_eq!(usage.ms, 9);
        // The input list is absent, so the input is all text; the tool
        // use is kept by kind, the reasoning apart from the answer.
        assert_eq!(usage.prompt, [(Modality::Text, 89)]);
        assert_eq!(
            usage.tool_use,
            [(Modality::Image, 1980), (Modality::Text, 212)]
        );
        assert_eq!((usage.candidates, usage.thoughts), (115, 317));
        assert!(usage
            .raw
            .as_deref()
            .unwrap()
            .contains("total_tool_use_tokens"));
        assert_eq!(
            interaction_usage(&raw, 600, false, 9).media_seconds,
            Some(600),
            "every length known: nothing estimated"
        );
        assert!(matches!(
            parse_interaction_reply(&json!({ "status": "completed", "steps": [] })).unwrap_err(),
            LlmError::Schema { detail, .. } if detail.contains("no model output")
        ));
        assert!(matches!(
            parse_interaction_reply(&json!({ "status": "failed", "error": { "message": "boom" } }))
                .unwrap_err(),
            LlmError::Schema { detail, .. } if detail.contains("failed")
        ));
        assert_eq!(
            parse_interaction_reply(&json!({ "status": "incomplete",
                "incomplete_details": { "reason": "safety" } }))
            .unwrap_err(),
            LlmError::refused()
        );
    }

    #[test]
    fn the_model_reads_what_the_documentation_says_it_reads() {
        let caps = capabilities();
        assert_eq!(caps.kinds.len(), 6);
        assert!(caps.accepts_mime(SourceKind::Pdf, "application/pdf"));
        assert!(caps.accepts_mime(SourceKind::Image, "image/heic"));
        assert!(!caps.accepts_mime(SourceKind::Image, "image/gif"));
        assert!(caps.accepts_mime(SourceKind::Video, "video/quicktime"));
        assert!(!caps.accepts_mime(SourceKind::Video, "video/x-matroska"));
        assert!(caps.accepts_mime(SourceKind::Audio, "audio/mpeg"));
        assert!(caps.supports_youtube);
        assert_eq!(caps.max_inline_bytes, INLINE_LIMIT as u64);
        assert!(caps.max_request_bytes > caps.max_inline_bytes);
    }

    #[test]
    fn writing_requests_pick_their_prompt_and_schema() {
        let extraction = FakeProvider::canned_extraction();
        let drafts = FakeProvider::canned_drafts();

        let generate = build_generate_request(&extraction, "focus on valves", &[]);
        let v = serde_json::to_value(&generate).unwrap();
        assert_eq!(
            v["systemInstruction"]["parts"][0]["text"],
            prompts::GENERATE_SYSTEM
        );
        assert_eq!(
            v["contents"][0]["parts"][0]["text"],
            prompts::generate_user_text(&extraction, "focus on valves", &[])
        );
        assert_eq!(
            v["generationConfig"],
            json!({
                "responseMimeType": "application/json",
                "responseSchema": response_schema(&schema::drafts_schema()),
                "temperature": 0.7
            })
        );

        let more = build_generate_request(&extraction, "", &drafts);
        let v = serde_json::to_value(&more).unwrap();
        assert_eq!(
            v["systemInstruction"]["parts"][0]["text"],
            prompts::MORE_SYSTEM
        );
        assert!(v["contents"][0]["parts"][0]["text"]
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
        let revise = build_revise_request(&edit);
        let v = serde_json::to_value(&revise).unwrap();
        assert_eq!(
            v["systemInstruction"]["parts"][0]["text"],
            prompts::EDIT_SYSTEM
        );
        assert_eq!(
            v["contents"][0]["parts"][0]["text"],
            prompts::edit_user_text(&edit)
        );
        assert_eq!(
            v["generationConfig"]["responseSchema"],
            response_schema(&schema::revision_schema())
        );
        assert_eq!(v["generationConfig"]["temperature"], 0.7);
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
        let turn = build_turn_request(&card, front, SpokenInput::Transcript("four I think"));
        assert_eq!(
            serde_json::to_value(&turn).unwrap(),
            json!({
                "systemInstruction": { "parts": [{ "text": prompts::TURN_SYSTEM }] },
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": prompts::turn_user_text(&card, front, "four I think") }]
                }],
                "generationConfig": {
                    "responseMimeType": "application/json",
                    "responseSchema": response_schema(&schema::turn_schema()),
                    "temperature": 0.1
                }
            })
        );
        // The state is in the message: the model is told an answer is not
        // expected once the verdict was given.
        let window = prompts::turn_user_text(&card, DialogState::Window, "why is that");
        assert!(window.contains("verdict was spoken") && window.contains("why is that"));
        assert!(prompts::turn_user_text(&card, front, "x").contains("answer is awaited"));
        // A recording: the card as text, then the clip as the inline part.
        let heard = build_turn_request(
            &card,
            front,
            SpokenInput::Audio {
                mime: "audio/wav",
                bytes: b"RIFF",
            },
        );
        assert_eq!(
            serde_json::to_value(&heard).unwrap()["contents"][0]["parts"],
            json!([
                { "text": prompts::turn_audio_user_text(&card, front) },
                { "inlineData": { "mimeType": "audio/wav", "data": "UklGRg==" } }
            ])
        );
    }

    #[test]
    fn usage_meters_media_from_prompt_tokens() {
        let raw = json!({ "usageMetadata": {
            "promptTokenCount": 12_000, "candidatesTokenCount": 800, "thoughtsTokenCount": 200
        } });
        let video = usage_of(&raw, Some(SourceKind::Video), 1_500);
        assert_eq!(video.input_tokens(), 12_000);
        assert_eq!(video.output_tokens(), 1_000);
        assert_eq!((video.candidates, video.thoughts), (800, 200));
        assert_eq!(video.media_seconds, Some(120));
        assert_eq!(video.ms, 1_500);
        assert_eq!(
            video.prompt,
            [(Modality::Text, 12_000)],
            "no per-kind list: the prompt is all text"
        );
        assert!(video.cached.is_empty() && video.tool_use.is_empty());
        assert_eq!(
            usage_of(&raw, Some(SourceKind::Youtube), 1).media_seconds,
            Some(120)
        );
        assert_eq!(
            usage_of(&raw, Some(SourceKind::Audio), 1).media_seconds,
            Some(375)
        );
        assert_eq!(usage_of(&raw, Some(SourceKind::Pdf), 1).media_seconds, None);
        assert_eq!(usage_of(&raw, None, 1).media_seconds, None);
        let nothing = usage_of(&json!({}), Some(SourceKind::Video), 1);
        assert_eq!(nothing.input_tokens(), 0);
        assert_eq!(nothing.raw, None);

        // With per-modality counts, only the media tokens are metered:
        // the PDF beside the video does not count as watched.
        let detailed = json!({ "usageMetadata": {
            "promptTokenCount": 12_000, "candidatesTokenCount": 800,
            "promptTokensDetails": [
                { "modality": "TEXT", "tokenCount": 500 },
                { "modality": "DOCUMENT", "tokenCount": 5_500 },
                { "modality": "VIDEO", "tokenCount": 3_000 },
                { "modality": "AUDIO", "tokenCount": 3_200 }
            ]
        } });
        assert_eq!(
            usage_of(&detailed, Some(SourceKind::Video), 1).media_seconds,
            Some(30 + 100)
        );
        assert_eq!(
            usage_of(&detailed, Some(SourceKind::Pdf), 1).media_seconds,
            None,
            "a set without media is not metered"
        );
    }

    #[test]
    fn usage_keeps_every_billed_count_apart_and_loses_none() {
        let raw = json!({
            "responseId": "abc123",
            "modelVersion": "gemini-3.8-flash-001",
            "usageMetadata": {
                "promptTokenCount": 10_000,
                "cachedContentTokenCount": 4_000,
                "candidatesTokenCount": 800,
                "thoughtsTokenCount": 200,
                "toolUsePromptTokenCount": 50,
                "totalTokenCount": 11_050,
                "promptTokensDetails": [
                    { "modality": "TEXT", "tokenCount": 6_000 },
                    { "modality": "AUDIO", "tokenCount": 3_000 }
                ],
                "cacheTokensDetails": [
                    { "modality": "TEXT", "tokenCount": 4_000 }
                ],
                "toolUsePromptTokensDetails": [
                    { "modality": "HOLOGRAM", "tokenCount": 50 }
                ]
            }
        });
        let usage = usage_of(&raw, Some(SourceKind::Audio), 7);
        assert_eq!(usage.response_id.as_deref(), Some("abc123"));
        assert_eq!(usage.model_version.as_deref(), Some("gemini-3.8-flash-001"));
        // The kinds listed came to 9,000 of 10,000: the shortfall is text.
        assert_eq!(
            usage.prompt,
            [(Modality::Text, 7_000), (Modality::Audio, 3_000)]
        );
        assert_eq!(usage.cached, [(Modality::Text, 4_000)]);
        assert_eq!(
            usage.tool_use,
            [(Modality::Text, 50)],
            "a kind this vocabulary lacks is counted as text, not dropped"
        );
        assert_eq!(usage.prompt_total, 10_000);
        assert_eq!(usage.input_tokens(), 10_050);
        assert_eq!(usage.cached_tokens(), 4_000);
        assert_eq!(usage.tool_tokens(), 50);
        assert_eq!(usage.output_tokens(), 1_000);
        assert_eq!(usage.media_seconds, Some(3_000 / 32));
        let kept: Value = serde_json::from_str(usage.raw.as_deref().unwrap()).unwrap();
        assert_eq!(kept["cachedContentTokenCount"], 4_000, "the record as sent");
    }

    #[test]
    fn refusals_and_truncations_map_to_their_errors() {
        let blocked = json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" } });
        assert_eq!(parse_reply(&blocked).unwrap_err(), LlmError::refused());
        let safety =
            json!({ "candidates": [{ "finishReason": "SAFETY", "content": { "parts": [] } }] });
        assert_eq!(parse_reply(&safety).unwrap_err(), LlmError::refused());
        let cut = json!({ "candidates": [{ "finishReason": "MAX_TOKENS",
            "content": { "parts": [{ "text": "{\"cards\": [" }] } }] });
        assert!(matches!(
            parse_reply(&cut).unwrap_err(),
            LlmError::Schema { detail, .. } if detail.contains("truncated")
        ));
        assert!(matches!(
            parse_reply(&json!({ "candidates": [] })).unwrap_err(),
            LlmError::Schema { .. }
        ));
        let empty = json!({ "candidates": [{ "content": { "parts": [{ "text": "  " }] } }] });
        assert!(matches!(
            parse_reply(&empty).unwrap_err(),
            LlmError::Schema { .. }
        ));
        let junk = json!({ "candidates": [{ "content": { "parts": [{ "text": "not json" }] } }] });
        assert!(matches!(
            parse_reply(&junk).unwrap_err(),
            LlmError::Schema { .. }
        ));
        // A refusal after the prompt was read carries what was billed.
        let usage = usage_of(
            &json!({ "usageMetadata": { "promptTokenCount": 40 } }),
            None,
            1,
        );
        let err = parse_reply(&safety).unwrap_err().billed(usage.clone());
        assert_eq!(err.usage(), Some(&usage));
        assert_eq!(LlmError::http(503, "x").billed(usage).usage(), None);
    }

    #[test]
    fn a_successful_reply_parses_into_an_extraction() {
        let raw = json!({
            "candidates": [{
                "content": { "role": "model", "parts": [
                    { "thought": true, "text": "Let me read this." },
                    { "text": "{\"text\": \"The heart has four chambers.\", \"figures\": [\
                        {\"n\": 1, \"page\": 2, \"bbox\": [0.1, 0.2, 0.9, 0.6], \"caption\": \"Chambers\"},\
                        {\"n\": 2, \"page\": 2, \"bbox\": [0.5, 0.5, 0.5, 0.9], \"caption\": \"Empty\"}\
                    ], \"est_words\": 5}" }
                ] },
                "finishReason": "STOP"
            }],
            "usageMetadata": { "promptTokenCount": 40, "candidatesTokenCount": 60 }
        });
        let value = parse_reply(&raw).unwrap();
        let extraction = extraction_from(value).unwrap();
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
        assert_eq!(usage_of(&raw, Some(SourceKind::Pdf), 9).output_tokens(), 60);
    }

    #[test]
    fn the_provider_names_its_models_but_never_its_key() {
        let p = GeminiProvider::new(
            "not-a-real-key".into(),
            "gemini-3.8-flash".into(),
            "gemini-3.8-flash-lite".into(),
        );
        assert_eq!(
            p.describe(),
            "gemini (gemini-3.8-flash; grader gemini-3.8-flash-lite)"
        );
        assert!(valid_model_name("gemini-3.8-flash"));
        assert!(!valid_model_name("../models") && !valid_model_name(""));
    }
}
