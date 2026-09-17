//! The contract between the generation pipeline and whatever model
//! serves it. Everything here is plain data plus one trait; nothing
//! knows about HTTP, the database or the request that started the job.
//!
//! Every method is synchronous on purpose. A call to a model takes
//! seconds to minutes, so callers run it under `spawn_blocking` the way
//! the mailer, the captcha verifier and the media bucket are called;
//! the trait then needs no async runtime and a fake can answer inline.

use flash_store::notes::NoteType;
use serde::{Deserialize, Serialize};

/// What kind of thing the person handed us. Decides which model input
/// the bytes become, what the extraction prompt asks for, and how the
/// reply's token count converts to media minutes. The store's kind,
/// under the name this module speaks.
pub type SourceKind = flash_store::AiSourceKind;

/// What the configured model can be handed. A provider states it once;
/// the runtime narrows it by what the operator listed, the ingest
/// checks every upload's type against it, and the clients build their
/// pickers from what is published of it. Nothing outside the provider
/// knows a model's own limits by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// The source kinds the model reads at all.
    pub kinds: Vec<SourceKind>,
    /// Per kind, the MIME types accepted as binary parts. Text and a
    /// video link carry none: text travels in the prompt, a link is a
    /// URL the provider fetches.
    pub mimes: Vec<(SourceKind, Vec<&'static str>)>,
    /// Bytes of one part that may travel inside the request itself.
    pub max_inline_bytes: u64,
    /// The whole request's budget: inline parts after encoding plus
    /// the prompt.
    pub max_request_bytes: u64,
    /// The largest single part by any route (a file upload the
    /// provider holds for the call); None when there is no such route.
    pub max_file_bytes: Option<u64>,
    /// Binary parts in one request.
    pub max_parts: usize,
    /// Whether a public video URL may be handed over as a source.
    pub supports_youtube: bool,
    /// The model's own ceiling on a recording's length, when it has one.
    pub max_media_seconds: Option<u32>,
}

impl Capabilities {
    pub fn accepts(&self, kind: SourceKind) -> bool {
        self.kinds.contains(&kind)
    }

    /// Whether `mime` may be sent as a part of `kind`. Kinds that carry
    /// no binary part (text, a link) accept any MIME, since none is
    /// ever sent.
    pub fn accepts_mime(&self, kind: SourceKind, mime: &str) -> bool {
        if !self.accepts(kind) {
            return false;
        }
        match self.mimes.iter().find(|(k, _)| *k == kind) {
            Some((_, list)) => list.iter().any(|m| m.eq_ignore_ascii_case(mime)),
            None => true,
        }
    }

    /// The MIME types of `kind`, empty for a kind that carries none.
    pub fn mimes_of(&self, kind: SourceKind) -> &[&'static str] {
        self.mimes
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, list)| list.as_slice())
            .unwrap_or(&[])
    }

    /// The same capabilities kept to the kinds in `wanted`, in the
    /// provider's own order. Err names the first wanted kind the
    /// provider does not read: an operator listed something the model
    /// cannot take, which is a boot error.
    pub fn narrowed_to(&self, wanted: &[SourceKind]) -> Result<Capabilities, SourceKind> {
        if let Some(missing) = wanted.iter().find(|k| !self.accepts(**k)) {
            return Err(*missing);
        }
        let mut out = self.clone();
        out.kinds.retain(|k| wanted.contains(k));
        out.mimes.retain(|(k, _)| wanted.contains(k));
        if !wanted.contains(&SourceKind::Youtube) {
            out.supports_youtube = false;
        }
        Ok(out)
    }

    /// The same capabilities without `kind`.
    pub fn without(&self, kind: SourceKind) -> Capabilities {
        let mut out = self.clone();
        out.kinds.retain(|k| *k != kind);
        out.mimes.retain(|(k, _)| *k != kind);
        if kind == SourceKind::Youtube {
            out.supports_youtube = false;
        }
        out
    }
}

/// One piece of a source as it goes to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcePart {
    /// Pasted or extracted text.
    Text(String),
    /// Bytes sent with the request (a PDF, a photo, a short clip).
    Inline { mime: String, bytes: Vec<u8> },
    /// Bytes already uploaded to the provider, referenced by its URI.
    FileUri { mime: String, uri: String },
    /// A public video URL the provider fetches itself.
    YoutubeUrl(String),
}

impl SourcePart {
    /// The bytes this part adds to a request before encoding.
    pub fn len(&self) -> u64 {
        match self {
            SourcePart::Text(t) => t.len() as u64,
            SourcePart::Inline { bytes, .. } => bytes.len() as u64,
            SourcePart::FileUri { .. } | SourcePart::YoutubeUrl(_) => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One member of the set a job reads, as the model is handed it:
/// numbered from 1 in the order the person attached them, named as
/// they named it, with its parts in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractSource {
    pub index: u32,
    pub kind: SourceKind,
    /// A file's name, a link, or "Pasted text": what the prompt calls
    /// the source and what a figure reference points back to.
    pub label: String,
    pub parts: Vec<SourcePart>,
    /// A recording's playing time when ingest could read it; None for a
    /// link, whose length only the model learns.
    pub duration_s: Option<u32>,
}

/// The whole set, ready for one extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractInput {
    pub sources: Vec<ExtractSource>,
}

impl ExtractInput {
    /// One source on its own.
    pub fn single(kind: SourceKind, label: impl Into<String>, parts: Vec<SourcePart>) -> Self {
        ExtractInput {
            sources: vec![ExtractSource {
                index: 1,
                kind,
                label: label.into(),
                parts,
                duration_s: None,
            }],
        }
    }

    /// Every kind in the set, deduplicated, in order of first use.
    pub fn kinds(&self) -> Vec<SourceKind> {
        let mut out = Vec::new();
        for s in &self.sources {
            if !out.contains(&s.kind) {
                out.push(s.kind);
            }
        }
        out
    }

    /// Whether any member is watched or listened to rather than read.
    pub fn is_media(&self) -> bool {
        self.sources.iter().any(|s| s.kind.is_media())
    }

    pub fn has_video(&self) -> bool {
        self.sources.iter().any(|s| s.kind.is_video())
    }

    /// The dearest kind in the set: what the call is charged as.
    pub fn costliest(&self) -> SourceKind {
        SourceKind::costliest(self.sources.iter().map(|s| s.kind)).unwrap_or(SourceKind::Text)
    }

    /// Bytes of every part before encoding.
    pub fn bytes(&self) -> u64 {
        self.sources
            .iter()
            .flat_map(|s| s.parts.iter())
            .map(SourcePart::len)
            .sum()
    }
}

/// A diagram or table in a source worth showing on a card. `source` is
/// the 1-based number of the set member it is in; `page` is 1-based
/// within that member; `bbox` is `[x0, y0, x1, y1]` as fractions of the
/// page, so it survives any rendering resolution. A reading cached
/// before sets existed names no source, and that meant the only one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FigureRef {
    pub n: u32,
    #[serde(default = "one")]
    pub source: u32,
    pub page: u32,
    pub bbox: [f32; 4],
    pub caption: String,
}

fn one() -> u32 {
    1
}

/// The model's reading of a source: clean study text plus the figures
/// it flagged. Cached per user, hash, model and prompt version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extraction {
    pub text: String,
    #[serde(default)]
    pub figures: Vec<FigureRef>,
    #[serde(default)]
    pub est_words: u32,
}

/// A card as the model wrote it: plain text sides, a note type of its
/// choosing, and figure numbers it placed with `{{figure:N}}`. Not yet
/// trusted; `validate::drafts` turns these into editor fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftNote {
    #[serde(with = "note_type_serde")]
    pub note_type: NoteType,
    pub front: String,
    #[serde(default)]
    pub back: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub figures: Vec<u32>,
}

/// The store's note type as the model names it: its `as_str` token.
mod note_type_serde {
    use flash_store::notes::NoteType;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(t: &NoteType, s: S) -> Result<S::Ok, S::Error> {
        t.as_str().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NoteType, D::Error> {
        let raw = String::deserialize(d)?;
        NoteType::parse(raw.trim())
            .ok_or_else(|| serde::de::Error::custom(format!("unknown note type {:?}", raw.trim())))
    }
}

/// Who said what in a review's thread: the store's role, under the
/// name this module speaks.
pub type TurnRole = flash_store::AiTurnRole;

/// One earlier turn of the conversation about a generation's cards: a
/// message the person sent, or the sentence the model answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub role: TurnRole,
    pub text: String,
}

/// What the model answers a message with: the complete new set of open
/// drafts and one sentence saying what it did. Not yet trusted;
/// `validate::drafts` and `validate::reply_sentence` bound both.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Revision {
    #[serde(default)]
    pub reply: String,
    #[serde(default)]
    pub cards: Vec<DraftNote>,
}

/// One message of the thread, with everything the model sees to act on
/// it: the reading, the turns so far, the cards already kept (context
/// only: the model never returns them) and the open drafts it may
/// rewrite, add to, drop from, retype or retag.
#[derive(Debug, Clone, PartialEq)]
pub struct EditInput<'a> {
    pub extraction: &'a Extraction,
    pub thread: &'a [Turn],
    pub message: &'a str,
    pub open: &'a [DraftNote],
    pub kept: &'a [DraftNote],
}

/// The card a student is answering, with the source it came from when
/// the card was generated (so the grader can check against it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardContext {
    pub front: String,
    pub back: String,
    pub note_type: NoteType,
    pub extraction: Option<String>,
    /// The answer's symbol-like words letter by letter ("A U" for
    /// "Au."), empty when it has none; see `voice_hints::spelled_forms`.
    pub spelled: Vec<String>,
}

/// What the student said, as the grader gets it: the device's own
/// transcript, or the recording itself for the model to listen to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpokenInput<'a> {
    Transcript(&'a str),
    Audio { mime: &'a str, bytes: &'a [u8] },
}

/// Where the conversation is when the student speaks: the card's front
/// was read and the answer is awaited, or the verdict was given and the
/// grade is about to land. The client owns the dialog; the model is
/// told the state so it knows what the student can mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogState {
    Front,
    Window,
}

impl DialogState {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "front" => Some(DialogState::Front),
            "window" => Some(DialogState::Window),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DialogState::Front => "front",
            DialogState::Window => "window",
        }
    }

    /// Whether a turn in this state can mean `intent`: nothing is undone
    /// before a verdict, and nothing is answered after one.
    pub fn allows(self, intent: SpokenIntent) -> bool {
        match self {
            DialogState::Front => intent != SpokenIntent::Undo,
            DialogState::Window => intent != SpokenIntent::Answer,
        }
    }
}

/// What the student meant by one spoken turn. The words are the
/// client's business: the model reads the meaning in any wording, or
/// hears it in a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpokenIntent {
    /// An answer to the card, to be graded.
    Answer,
    /// A question about the card, to be answered from it.
    Question,
    /// A grade named instead of an answer, or a change of grade.
    Grade,
    /// Hear the card again.
    Repeat,
    /// Does not know it; move on.
    Skip,
    /// Take the grade back.
    Undo,
    /// Nothing intelligible, or nothing meant for the session.
    Unclear,
}

impl SpokenIntent {
    pub const ALL: [SpokenIntent; 7] = [
        SpokenIntent::Answer,
        SpokenIntent::Question,
        SpokenIntent::Grade,
        SpokenIntent::Repeat,
        SpokenIntent::Skip,
        SpokenIntent::Undo,
        SpokenIntent::Unclear,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            SpokenIntent::Answer => "answer",
            SpokenIntent::Question => "question",
            SpokenIntent::Grade => "grade",
            SpokenIntent::Repeat => "repeat",
            SpokenIntent::Skip => "skip",
            SpokenIntent::Undo => "undo",
            SpokenIntent::Unclear => "unclear",
        }
    }
}

/// One spoken turn as the model understood it. `rating` is on the
/// scheduler's scale (1 Again, 2 Hard, 3 Good, 4 Easy): the verdict for
/// an `answer`, the grade the student named for a `grade`, absent
/// otherwise. `feedback` and `spoken_feedback` belong to an `answer`
/// (the spoken one opens with the rating word so the student hears the
/// verdict first); `answer` and `spoken_answer` to a `question`. The
/// other intents carry nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpokenTurn {
    pub intent: SpokenIntent,
    #[serde(default)]
    pub rating: Option<u8>,
    #[serde(default)]
    pub feedback: String,
    #[serde(default)]
    pub spoken_feedback: String,
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub spoken_answer: String,
}

impl SpokenTurn {
    /// A turn that carries only its intent.
    pub fn bare(intent: SpokenIntent) -> Self {
        SpokenTurn {
            intent,
            rating: None,
            feedback: String::new(),
            spoken_feedback: String::new(),
            answer: String::new(),
            spoken_answer: String::new(),
        }
    }
}

/// What kind of tokens a prompt was billed as. The provider's own
/// vocabulary, one name per kind it prices apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
    Document,
}

impl Modality {
    pub const ALL: [Modality; 5] = [
        Modality::Text,
        Modality::Image,
        Modality::Video,
        Modality::Audio,
        Modality::Document,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Modality::Text => "text",
            Modality::Image => "image",
            Modality::Video => "video",
            Modality::Audio => "audio",
            Modality::Document => "document",
        }
    }

    /// The provider's name for a modality, in either of the spellings
    /// its two APIs use (`TEXT`, `text`). None for a name this
    /// vocabulary lacks.
    pub fn parse(name: &str) -> Option<Modality> {
        match name.to_ascii_lowercase().as_str() {
            "text" => Some(Modality::Text),
            "image" => Some(Modality::Image),
            "video" => Some(Modality::Video),
            "audio" => Some(Modality::Audio),
            "document" => Some(Modality::Document),
            _ => None,
        }
    }
}

/// What one call consumed, as the provider reported it: every count it
/// bills by, kept apart the way it prices them, plus the exchange
/// around them. `media_seconds` is derived from the prompt's per-kind
/// token counts for timeline sources, so minutes are metered from what
/// was actually processed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage {
    /// Prompt tokens by kind, as the provider's usage record lists
    /// them. A record without the list is all text.
    pub prompt: Vec<(Modality, u32)>,
    /// The share of `prompt` served from the provider's cache, by kind.
    pub cached: Vec<(Modality, u32)>,
    /// Tokens the provider's own tools read on the call's behalf, by
    /// kind (an agentic reading of a long video).
    pub tool_use: Vec<(Modality, u32)>,
    /// The whole prompt, as the provider counted it.
    pub prompt_total: u32,
    /// Reply tokens, thinking apart.
    pub candidates: u32,
    /// Thinking tokens, billed as reply.
    pub thoughts: u32,
    pub media_seconds: Option<u32>,
    /// Wall-clock time of the exchange.
    pub ms: u32,
    /// The provider's id for the exchange.
    pub response_id: Option<String>,
    /// The model version the provider says answered.
    pub model_version: Option<String>,
    /// The provider's usage record as it was sent, for the audit.
    pub raw: Option<String>,
}

impl Usage {
    /// An all-text exchange of `input` prompt and `output` reply
    /// tokens: the shape a fake reports.
    pub fn text(input: u32, output: u32) -> Usage {
        Usage {
            prompt: vec![(Modality::Text, input)],
            prompt_total: input,
            candidates: output,
            ..Default::default()
        }
    }

    /// Prompt tokens, tool-use included.
    pub fn input_tokens(&self) -> u32 {
        self.prompt_total.saturating_add(self.tool_tokens())
    }

    /// Reply tokens, thinking included.
    pub fn output_tokens(&self) -> u32 {
        self.candidates.saturating_add(self.thoughts)
    }

    pub fn cached_tokens(&self) -> u32 {
        sum_tokens(&self.cached)
    }

    pub fn tool_tokens(&self) -> u32 {
        sum_tokens(&self.tool_use)
    }
}

fn sum_tokens(list: &[(Modality, u32)]) -> u32 {
    list.iter().fold(0u32, |n, (_, t)| n.saturating_add(*t))
}

/// A value the model returned together with what it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply<T> {
    pub value: T,
    pub usage: Usage,
}

impl<T> Reply<T> {
    pub fn new(value: T, usage: Usage) -> Self {
        Self { value, usage }
    }
}

/// Longest piece of a provider's reply body kept in an error, so a log
/// line says what went wrong without carrying the whole exchange.
pub const EXCERPT_LEN: usize = 300;

/// Why a call produced nothing usable. The Display form is for the
/// journal; `user_message` is the sentence a person sees. A refusal
/// and a malformed reply come after the provider has read the prompt
/// and billed for it, so they carry the usage the ledger must still
/// record; an error status, a timeout and a configuration fault bill
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    /// The provider answered with an error status, or could not be
    /// reached at all (status 0).
    #[error("provider HTTP {status}: {excerpt}")]
    Http { status: u16, excerpt: String },
    /// The provider declined to answer this content.
    #[error("provider refused the content")]
    Refused { usage: Usage },
    /// The reply did not have the shape that was asked for.
    #[error("reply did not match the schema: {detail}")]
    Schema { detail: String, usage: Usage },
    #[error("provider call timed out")]
    Timeout,
    /// This server cannot make the call at all (no key, bad model name).
    #[error("provider configuration: {0}")]
    Config(String),
}

impl LlmError {
    /// An HTTP-level error with the body cut to `EXCERPT_LEN` characters.
    /// Control characters become spaces on the way in: the excerpt ends
    /// up on one line of the journal, and an endpoint that echoes a
    /// person's text back in its error must not be able to write a
    /// second line there.
    pub fn http(status: u16, body: &str) -> Self {
        let mut excerpt: String = body
            .chars()
            .take(EXCERPT_LEN)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        if body.chars().count() > EXCERPT_LEN {
            excerpt.push('…');
        }
        LlmError::Http { status, excerpt }
    }

    /// A refusal with nothing billed yet known.
    pub fn refused() -> Self {
        LlmError::Refused {
            usage: Usage::default(),
        }
    }

    /// A malformed reply with nothing billed yet known.
    pub fn schema(detail: impl Into<String>) -> Self {
        LlmError::Schema {
            detail: detail.into(),
            usage: Usage::default(),
        }
    }

    /// The same error, carrying what the provider billed for the
    /// exchange that produced it. Errors that bill nothing are left
    /// alone.
    pub fn billed(self, usage: Usage) -> Self {
        match self {
            LlmError::Refused { .. } => LlmError::Refused { usage },
            LlmError::Schema { detail, .. } => LlmError::Schema { detail, usage },
            other => other,
        }
    }

    /// What the provider billed for the exchange, when it billed.
    pub fn usage(&self) -> Option<&Usage> {
        match self {
            LlmError::Refused { usage } | LlmError::Schema { usage, .. } => Some(usage),
            LlmError::Http { .. } | LlmError::Timeout | LlmError::Config(_) => None,
        }
    }

    /// One safe sentence per variant: nothing from the provider's body,
    /// nothing about keys or models, something a person can act on.
    pub fn user_message(&self) -> &'static str {
        match self {
            LlmError::Http { .. } => {
                "The generation service did not answer. Please try again in a moment."
            }
            LlmError::Refused { .. } => {
                "The generation service declined this source. Try a different one."
            }
            LlmError::Schema { .. } => {
                "The generation service returned something unexpected. Please try again."
            }
            LlmError::Timeout => "The generation service took too long. Please try again.",
            LlmError::Config(_) => "Card generation is not configured on this server.",
        }
    }
}

/// A model that can read a source, write cards from it, and talk a
/// student through one. `existing` on `generate` is what the model must
/// not repeat (an "ask for more" pass); `revise` answers one message of
/// the review's thread with the whole new set of open drafts.
pub trait LlmProvider: Send + Sync {
    /// What the configured model reads. Asked once at boot; the answer
    /// decides which uploads the server takes at all.
    fn capabilities(&self) -> Capabilities;

    fn extract(&self, input: &ExtractInput) -> Result<Reply<Extraction>, LlmError>;

    fn generate(
        &self,
        extraction: &Extraction,
        instruction: &str,
        existing: &[DraftNote],
    ) -> Result<Reply<Vec<DraftNote>>, LlmError>;

    fn revise(&self, edit: &EditInput<'_>) -> Result<Reply<Revision>, LlmError>;

    /// One spoken turn of voice study: what the student meant, given
    /// where the conversation is, graded or answered when that is what
    /// they meant.
    fn turn(
        &self,
        card: &CardContext,
        state: DialogState,
        spoken: SpokenInput<'_>,
    ) -> Result<Reply<SpokenTurn>, LlmError>;

    /// Human-readable name for the boot log (never the key).
    fn describe(&self) -> String;

    /// The names the ledger records: the model that reads and writes
    /// cards, and the one that hears the study turns. A provider with one
    /// model names it twice.
    fn model_names(&self) -> (String, String) {
        let name = self.describe();
        (name.clone(), name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_http_excerpt_stays_on_one_line() {
        let err = LlmError::http(400, "bad request\r\nWARN forged line\ttab\u{85}");
        match err {
            LlmError::Http { status, excerpt } => {
                assert_eq!(status, 400);
                assert_eq!(excerpt, "bad request  WARN forged line tab ");
            }
            other => panic!("{other:?}"),
        }
        let long = "x".repeat(EXCERPT_LEN + 5);
        match LlmError::http(500, &long) {
            LlmError::Http { excerpt, .. } => {
                assert_eq!(excerpt.chars().count(), EXCERPT_LEN + 1);
                assert!(excerpt.ends_with('…'));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn source_kinds_round_trip_and_know_their_media() {
        for k in SourceKind::ALL {
            assert_eq!(SourceKind::parse(k.as_str()), Some(*k));
        }
        assert_eq!(SourceKind::parse("scroll"), None);
        assert!(SourceKind::Video.is_media() && SourceKind::Youtube.is_media());
        assert!(SourceKind::Audio.is_media());
        assert!(!SourceKind::Pdf.is_media() && !SourceKind::Text.is_media());
        assert_eq!(
            SourceKind::costliest([SourceKind::Text, SourceKind::Pdf, SourceKind::Image]),
            Some(SourceKind::Pdf)
        );
        assert_eq!(
            SourceKind::costliest([SourceKind::Audio, SourceKind::Youtube]),
            Some(SourceKind::Youtube)
        );
        assert_eq!(SourceKind::costliest([]), None);
    }

    fn caps() -> Capabilities {
        Capabilities {
            kinds: vec![
                SourceKind::Text,
                SourceKind::Pdf,
                SourceKind::Image,
                SourceKind::Youtube,
            ],
            mimes: vec![
                (SourceKind::Pdf, vec!["application/pdf"]),
                (SourceKind::Image, vec!["image/png", "image/jpeg"]),
            ],
            max_inline_bytes: 10,
            max_request_bytes: 20,
            max_file_bytes: None,
            max_parts: 4,
            supports_youtube: true,
            max_media_seconds: None,
        }
    }

    #[test]
    fn capabilities_narrow_to_what_the_operator_listed_and_refuse_what_the_model_cannot() {
        let all = caps();
        assert!(all.accepts_mime(SourceKind::Image, "image/PNG"));
        assert!(!all.accepts_mime(SourceKind::Image, "image/gif"));
        assert!(
            all.accepts_mime(SourceKind::Text, "text/plain"),
            "no list: anything"
        );
        assert!(
            !all.accepts_mime(SourceKind::Video, "video/mp4"),
            "not read at all"
        );
        assert_eq!(all.mimes_of(SourceKind::Pdf), ["application/pdf"]);
        assert!(all.mimes_of(SourceKind::Text).is_empty());

        let narrowed = all
            .narrowed_to(&[SourceKind::Image, SourceKind::Text])
            .unwrap();
        assert_eq!(narrowed.kinds, [SourceKind::Text, SourceKind::Image]);
        assert_eq!(narrowed.mimes.len(), 1);
        assert!(!narrowed.supports_youtube);
        assert_eq!(
            all.narrowed_to(&[SourceKind::Text, SourceKind::Video]),
            Err(SourceKind::Video)
        );
        let without = all.without(SourceKind::Pdf);
        assert!(!without.accepts(SourceKind::Pdf) && without.accepts(SourceKind::Image));
        assert!(without.supports_youtube);
    }

    #[test]
    fn an_extract_input_knows_its_kinds_its_cost_and_its_size() {
        let input = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "a.pdf".into(),
                    parts: vec![SourcePart::Inline {
                        mime: "application/pdf".into(),
                        bytes: vec![0; 8],
                    }],
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
                    kind: SourceKind::Pdf,
                    label: "b.pdf".into(),
                    parts: vec![SourcePart::FileUri {
                        mime: "application/pdf".into(),
                        uri: "https://x/1".into(),
                    }],
                    duration_s: None,
                },
            ],
        };
        assert_eq!(input.kinds(), [SourceKind::Pdf, SourceKind::Text]);
        assert_eq!(input.costliest(), SourceKind::Pdf);
        assert!(!input.is_media() && !input.has_video());
        assert_eq!(input.bytes(), 13);
        let one = ExtractInput::single(SourceKind::Youtube, "link", vec![]);
        assert!(one.is_media() && one.has_video());
        assert_eq!(one.sources[0].index, 1);
        let figure: FigureRef =
            serde_json::from_str(r#"{"n":1,"page":2,"bbox":[0,0,1,1],"caption":"c"}"#).unwrap();
        assert_eq!(figure.source, 1, "an old reading names the only source");
    }

    #[test]
    fn draft_notes_name_their_type_by_token() {
        let json =
            r#"{"note_type":"basic_typed","front":"Q","back":"A","tags":["x"],"figures":[1]}"#;
        let d: DraftNote = serde_json::from_str(json).unwrap();
        assert_eq!(d.note_type, NoteType::BasicTyped);
        assert_eq!(serde_json::to_string(&d).unwrap(), json);
        assert!(serde_json::from_str::<DraftNote>(r#"{"note_type":"essay","front":"Q"}"#).is_err());
        let sparse: DraftNote =
            serde_json::from_str(r#"{"note_type":"cloze","front":"{{c1::x}}"}"#).unwrap();
        assert!(sparse.back.is_empty() && sparse.tags.is_empty() && sparse.figures.is_empty());
    }

    #[test]
    fn a_revision_reads_with_either_field_missing() {
        let full: Revision = serde_json::from_str(
            r#"{"cards":[{"note_type":"basic","front":"Q","back":"A"}],"reply":"Rewrote 1 card."}"#,
        )
        .unwrap();
        assert_eq!(full.cards.len(), 1);
        assert_eq!(full.reply, "Rewrote 1 card.");
        let bare: Revision = serde_json::from_str(r#"{"cards":[]}"#).unwrap();
        assert!(bare.reply.is_empty());
        let none: Revision = serde_json::from_str("{}").unwrap();
        assert!(none.cards.is_empty());
    }

    #[test]
    fn http_errors_keep_an_excerpt_only() {
        let body = "x".repeat(1_000);
        let LlmError::Http { status, excerpt } = LlmError::http(503, &body) else {
            panic!("http variant");
        };
        assert_eq!(status, 503);
        assert_eq!(excerpt.chars().count(), EXCERPT_LEN + 1);
        assert!(excerpt.ends_with('…'));
        assert_eq!(
            LlmError::http(400, "short"),
            LlmError::Http {
                status: 400,
                excerpt: "short".into()
            }
        );
    }

    #[test]
    fn every_error_has_a_sentence_that_names_no_internals() {
        let all = [
            LlmError::http(500, "secret body"),
            LlmError::refused(),
            LlmError::schema("candidates[0] missing"),
            LlmError::Timeout,
            LlmError::Config("no key".into()),
        ];
        for e in all {
            let msg = e.user_message();
            assert!(msg.ends_with('.'), "{msg}");
            assert!(!msg.contains("secret") && !msg.contains("candidates") && !msg.contains("key"));
        }
    }
}
