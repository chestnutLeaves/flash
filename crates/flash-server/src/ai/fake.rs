//! The provider every test talks to. It answers from a script when one
//! is given and with plausible canned values otherwise, and it records
//! what it was asked so a test can assert on the pipeline's behaviour
//! (which step ran, with which instruction, with how many existing
//! drafts) rather than on the model's.

use std::collections::VecDeque;

use flash_store::notes::NoteType;
use parking_lot::Mutex;

use super::provider::{
    Capabilities, CardContext, DialogState, DraftNote, EditInput, ExtractInput, Extraction,
    FigureRef, LlmError, LlmProvider, Reply, Revision, SourceKind, SpokenInput, SpokenIntent,
    SpokenTurn, Usage,
};

/// One scripted answer. The pipeline's next call of the matching step
/// consumes it; a call of a different step is a scripting mistake and
/// answers with a schema error naming the mismatch.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeReply {
    Extract(Result<Extraction, LlmError>),
    /// Answers `generate`, with or without existing drafts.
    Drafts(Result<Vec<DraftNote>, LlmError>),
    /// Answers `revise`: the new open set and the sentence about it.
    Revision(Result<Revision, LlmError>),
    /// Answers `turn`: what the student meant, graded or answered.
    Turn(Result<SpokenTurn, LlmError>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Extract,
    Generate,
    /// `generate` with existing drafts to avoid.
    More,
    Revise,
    Turn,
}

/// What one call asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeCall {
    pub step: Step,
    /// The kinds in the set of an extract call, in order of first use.
    pub kinds: Vec<SourceKind>,
    /// How many sources an extract call was handed, with their labels.
    pub sources: Vec<String>,
    /// The instruction of a generate call, the message of a revise
    /// call, or the transcript of a turn.
    pub instruction: Option<String>,
    /// The recording a turn was handed instead of text: its type and
    /// how many bytes.
    pub audio: Option<(String, usize)>,
    /// Where the conversation was, for a turn.
    pub state: Option<DialogState>,
    /// Drafts handed in: existing ones for generate, the open set for
    /// revise.
    pub existing: usize,
    /// Earlier turns a revise call travelled with.
    pub thread: usize,
    /// Kept cards a revise call saw as context.
    pub kept: usize,
    /// Whether the call carried source text (an extraction, or a card
    /// context with one).
    pub extraction_present: bool,
}

impl FakeCall {
    /// The one kind of a single-source extract call.
    pub fn kind(&self) -> Option<SourceKind> {
        match self.kinds.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }
}

pub struct FakeProvider {
    script: Mutex<VecDeque<FakeReply>>,
    calls: Mutex<Vec<FakeCall>>,
    capabilities: Capabilities,
    /// The model name it answers to; "fake" unless a test names one.
    name: String,
}

impl Default for FakeProvider {
    fn default() -> Self {
        Self::scripted(Vec::new())
    }
}

impl FakeProvider {
    pub fn scripted(replies: Vec<FakeReply>) -> Self {
        Self {
            script: Mutex::new(replies.into()),
            calls: Mutex::new(Vec::new()),
            capabilities: Self::everything(),
            name: "fake".into(),
        }
    }

    /// A fake that goes by `name`, for tests of what hangs off a model's
    /// name.
    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// A fake that claims to read only what `capabilities` says, for
    /// tests of the narrowing at boot.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Every kind, with the media types the editor's own uploads carry,
    /// and limits no test reaches.
    pub fn everything() -> Capabilities {
        Capabilities {
            kinds: SourceKind::ALL.to_vec(),
            mimes: vec![
                (SourceKind::Pdf, vec!["application/pdf"]),
                (
                    SourceKind::Image,
                    vec![
                        "image/png",
                        "image/jpeg",
                        "image/gif",
                        "image/webp",
                        "image/avif",
                        "image/bmp",
                        "image/heic",
                        "image/heif",
                    ],
                ),
                (
                    SourceKind::Video,
                    vec![
                        "video/mp4",
                        "video/webm",
                        "video/x-matroska",
                        "video/quicktime",
                        "video/ogg",
                    ],
                ),
                (
                    SourceKind::Audio,
                    vec![
                        "audio/mpeg",
                        "audio/ogg",
                        "audio/wav",
                        "audio/mp4",
                        "audio/aac",
                        "audio/flac",
                        "audio/3gpp",
                    ],
                ),
            ],
            max_inline_bytes: 1 << 40,
            max_request_bytes: 1 << 40,
            max_file_bytes: Some(1 << 40),
            max_parts: 1_000,
            supports_youtube: true,
            max_media_seconds: None,
        }
    }

    /// Adds one scripted answer after those not yet consumed, so a test
    /// can script the next step once the earlier ones have run.
    pub fn script_push(&self, reply: FakeReply) {
        self.script.lock().push_back(reply);
    }

    /// Every call so far, oldest first.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.calls.lock().clone()
    }

    /// Scripted replies not yet consumed.
    pub fn remaining(&self) -> usize {
        self.script.lock().len()
    }

    /// The extraction the default answers with: a short passage and
    /// two figures on page 1 of the first source.
    pub fn canned_extraction() -> Extraction {
        Extraction {
            text: "The heart has four chambers: two atria and two ventricles. Blood enters \
                   the right atrium, passes the tricuspid valve into the right ventricle, and \
                   leaves for the lungs through the pulmonary valve."
                .into(),
            figures: vec![
                FigureRef {
                    n: 1,
                    source: 1,
                    page: 1,
                    bbox: [0.1, 0.2, 0.9, 0.6],
                    caption: "The four chambers of the heart".into(),
                },
                FigureRef {
                    n: 2,
                    source: 1,
                    page: 1,
                    bbox: [0.1, 0.65, 0.5, 0.95],
                    caption: "Valves in order of flow".into(),
                },
            ],
            est_words: 32,
        }
    }

    /// Three drafts of mixed types, one placing figure 1.
    pub fn canned_drafts() -> Vec<DraftNote> {
        vec![
            DraftNote {
                note_type: NoteType::Basic,
                front: "How many chambers does the heart have? {{figure:1}}".into(),
                back: "Four: two atria and two ventricles.".into(),
                tags: vec!["anatomy".into(), "heart".into()],
                figures: vec![1],
            },
            DraftNote {
                note_type: NoteType::Cloze,
                front: "Blood passes the {{c1::tricuspid}} valve from the right atrium into the \
                        right ventricle."
                    .into(),
                back: String::new(),
                tags: vec!["anatomy".into()],
                figures: vec![],
            },
            DraftNote {
                note_type: NoteType::BasicTyped,
                front: "Which valve does blood pass on its way from the right ventricle to the \
                        lungs?"
                    .into(),
                back: "Pulmonary valve".into(),
                tags: vec!["anatomy".into(), "valves".into()],
                figures: vec![],
            },
        ]
    }

    /// An unscripted turn on the front: the answer was right.
    pub fn canned_answer() -> SpokenTurn {
        SpokenTurn {
            rating: Some(3),
            feedback: "Correct: four chambers.".into(),
            spoken_feedback: "Good. Four chambers, two atria and two ventricles.".into(),
            ..SpokenTurn::bare(SpokenIntent::Answer)
        }
    }

    /// An unscripted turn in the window: a question, answered.
    pub fn canned_question() -> SpokenTurn {
        SpokenTurn {
            answer: "The atria receive blood; the ventricles pump it out.".into(),
            spoken_answer: "The atria receive blood, and the ventricles pump it out.".into(),
            ..SpokenTurn::bare(SpokenIntent::Question)
        }
    }

    /// A scripted turn that carries only its intent.
    pub fn bare(intent: SpokenIntent) -> FakeReply {
        FakeReply::Turn(Ok(SpokenTurn::bare(intent)))
    }

    /// A minute of media per media member, so a set's metering adds up
    /// the way a real call's would.
    fn usage(input: Option<&ExtractInput>) -> Usage {
        let media = input
            .map(|i| i.sources.iter().filter(|s| s.kind.is_media()).count() as u32 * 60)
            .filter(|s| *s > 0);
        Usage {
            media_seconds: media,
            ms: 1,
            response_id: Some("fake-response".into()),
            model_version: Some("fake-001".into()),
            ..Usage::text(1_200, 300)
        }
    }

    fn record(&self, call: FakeCall) {
        self.calls.lock().push(call);
    }

    fn next(&self) -> Option<FakeReply> {
        self.script.lock().pop_front()
    }

    fn mismatch(step: Step, got: &FakeReply) -> LlmError {
        LlmError::schema(format!(
            "fake script: {step:?} called but next reply is {got:?}"
        ))
    }
}

impl LlmProvider for FakeProvider {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    fn extract(&self, input: &ExtractInput) -> Result<Reply<Extraction>, LlmError> {
        self.record(FakeCall {
            step: Step::Extract,
            kinds: input.kinds(),
            sources: input.sources.iter().map(|s| s.label.clone()).collect(),
            instruction: None,
            audio: None,
            existing: 0,
            thread: 0,
            kept: 0,
            extraction_present: false,
            state: None,
        });
        let value = match self.next() {
            None => Self::canned_extraction(),
            Some(FakeReply::Extract(r)) => r?,
            Some(other) => return Err(Self::mismatch(Step::Extract, &other)),
        };
        Ok(Reply::new(value, Self::usage(Some(input))))
    }

    fn generate(
        &self,
        extraction: &Extraction,
        instruction: &str,
        existing: &[DraftNote],
    ) -> Result<Reply<Vec<DraftNote>>, LlmError> {
        let step = if existing.is_empty() {
            Step::Generate
        } else {
            Step::More
        };
        self.record(FakeCall {
            step,
            kinds: Vec::new(),
            sources: Vec::new(),
            instruction: Some(instruction.to_string()),
            audio: None,
            existing: existing.len(),
            thread: 0,
            kept: 0,
            extraction_present: !extraction.text.is_empty(),
            state: None,
        });
        let value = match self.next() {
            None => Self::canned_drafts(),
            Some(FakeReply::Drafts(r)) => r?,
            Some(other) => return Err(Self::mismatch(step, &other)),
        };
        Ok(Reply::new(value, Self::usage(None)))
    }

    fn revise(&self, edit: &EditInput<'_>) -> Result<Reply<Revision>, LlmError> {
        self.record(FakeCall {
            step: Step::Revise,
            kinds: Vec::new(),
            sources: Vec::new(),
            instruction: Some(edit.message.to_string()),
            audio: None,
            existing: edit.open.len(),
            thread: edit.thread.len(),
            kept: edit.kept.len(),
            extraction_present: !edit.extraction.text.is_empty(),
            state: None,
        });
        let value = match self.next() {
            // Unscripted: the same cards, "edited" by the message, and a
            // sentence that counts them.
            None => Revision {
                cards: edit
                    .open
                    .iter()
                    .cloned()
                    .map(|mut d| {
                        d.back = format!("{} ({})", d.back.trim(), edit.message.trim())
                            .trim()
                            .to_string();
                        d
                    })
                    .collect(),
                reply: format!("Rewrote {} cards.", edit.open.len()),
            },
            Some(FakeReply::Revision(r)) => r?,
            Some(other) => return Err(Self::mismatch(Step::Revise, &other)),
        };
        Ok(Reply::new(value, Self::usage(None)))
    }

    fn turn(
        &self,
        card: &CardContext,
        state: DialogState,
        spoken: SpokenInput<'_>,
    ) -> Result<Reply<SpokenTurn>, LlmError> {
        let (instruction, audio) = match spoken {
            SpokenInput::Transcript(text) => (Some(text.to_string()), None),
            SpokenInput::Audio { mime, bytes } => (None, Some((mime.to_string(), bytes.len()))),
        };
        self.record(FakeCall {
            step: Step::Turn,
            kinds: Vec::new(),
            sources: Vec::new(),
            instruction,
            audio: audio.clone(),
            existing: 0,
            thread: 0,
            kept: 0,
            extraction_present: card.extraction.is_some(),
            state: Some(state),
        });
        let value = match self.next() {
            // Unscripted: an answer on the front is right, and what is
            // said in the window is a question.
            None => match state {
                DialogState::Front => Self::canned_answer(),
                DialogState::Window => Self::canned_question(),
            },
            Some(FakeReply::Turn(r)) => r?,
            Some(other) => return Err(Self::mismatch(Step::Turn, &other)),
        };
        let mut usage = Self::usage(None);
        // A recording is metered by its length: a second of 16 kHz
        // 16-bit audio is 32,000 bytes, and never less than a second.
        if let Some((_, bytes)) = audio {
            usage.media_seconds = Some((bytes / 32_000).max(1) as u32);
        }
        Ok(Reply::new(value, usage))
    }

    fn describe(&self) -> String {
        self.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::{ExtractSource, SourcePart, Turn, TurnRole};

    fn input(kind: SourceKind) -> ExtractInput {
        ExtractInput::single(kind, "notes", vec![SourcePart::Text("notes".into())])
    }

    #[test]
    fn the_default_answers_every_step_and_records_it() {
        let fake = FakeProvider::default();
        let ex = fake.extract(&input(SourceKind::Video)).unwrap();
        assert_eq!(ex.value.figures.len(), 2);
        assert_eq!(ex.usage.media_seconds, Some(60));
        let ex = fake.extract(&input(SourceKind::Pdf)).unwrap();
        assert_eq!(ex.usage.media_seconds, None);
        let set = ExtractInput {
            sources: vec![
                ExtractSource {
                    index: 1,
                    kind: SourceKind::Pdf,
                    label: "a.pdf".into(),
                    parts: vec![],
                    duration_s: None,
                },
                ExtractSource {
                    index: 2,
                    kind: SourceKind::Audio,
                    label: "talk.m4a".into(),
                    parts: vec![],
                    duration_s: None,
                },
                ExtractSource {
                    index: 3,
                    kind: SourceKind::Video,
                    label: "lecture.mp4".into(),
                    parts: vec![],
                    duration_s: None,
                },
            ],
        };
        let ex = fake.extract(&set).unwrap();
        assert_eq!(
            ex.usage.media_seconds,
            Some(120),
            "a minute per media member"
        );

        let drafts = fake.generate(&ex.value, "", &[]).unwrap().value;
        assert_eq!(drafts.len(), 3);
        let more = fake.generate(&ex.value, "more", &drafts).unwrap().value;
        assert_eq!(more.len(), 3);
        let thread = [
            Turn {
                role: TurnRole::User,
                text: "longer".into(),
            },
            Turn {
                role: TurnRole::Model,
                text: "Rewrote 3 cards.".into(),
            },
        ];
        let revised = fake
            .revise(&EditInput {
                extraction: &ex.value,
                thread: &thread,
                message: "shorter",
                open: &drafts,
                kept: &drafts[..1],
            })
            .unwrap()
            .value;
        assert_eq!(revised.cards.len(), drafts.len());
        assert!(revised.cards[0].back.ends_with("(shorter)"));
        assert_eq!(revised.reply, "Rewrote 3 cards.");

        let card = CardContext {
            front: "q".into(),
            back: "a".into(),
            note_type: NoteType::Basic,
            extraction: Some("src".into()),
            spelled: vec![],
        };
        assert_eq!(
            fake.turn(&card, DialogState::Front, SpokenInput::Transcript("four"))
                .unwrap()
                .value
                .rating,
            Some(3)
        );
        let heard = fake
            .turn(
                &card,
                DialogState::Front,
                SpokenInput::Audio {
                    mime: "audio/wav",
                    bytes: &[0; 64_000],
                },
            )
            .unwrap();
        assert_eq!(heard.usage.media_seconds, Some(2));
        let last = fake.calls().last().cloned().unwrap();
        assert_eq!(last.audio, Some(("audio/wav".into(), 64_000)));
        assert_eq!(last.instruction, None);
        assert_eq!(last.state, Some(DialogState::Front));
        // Unscripted, what is said in the window is a question.
        let asked = fake
            .turn(&card, DialogState::Window, SpokenInput::Transcript("why"))
            .unwrap()
            .value;
        assert_eq!(asked.intent, SpokenIntent::Question);
        assert!(!asked.spoken_answer.is_empty());

        let steps: Vec<Step> = fake.calls().iter().map(|c| c.step).collect();
        assert_eq!(
            steps,
            [
                Step::Extract,
                Step::Extract,
                Step::Extract,
                Step::Generate,
                Step::More,
                Step::Revise,
                Step::Turn,
                Step::Turn,
                Step::Turn
            ]
        );
        let calls = fake.calls();
        assert_eq!(calls[0].kind(), Some(SourceKind::Video));
        assert_eq!(
            calls[2].kinds,
            [SourceKind::Pdf, SourceKind::Audio, SourceKind::Video]
        );
        assert_eq!(calls[2].sources, ["a.pdf", "talk.m4a", "lecture.mp4"]);
        assert_eq!(calls[2].kind(), None);
        assert_eq!(calls[4].existing, 3);
        assert_eq!(calls[4].instruction.as_deref(), Some("more"));
        assert_eq!(
            (calls[5].existing, calls[5].thread, calls[5].kept),
            (3, 2, 1)
        );
        assert_eq!(calls[5].instruction.as_deref(), Some("shorter"));
        assert!(calls[6].extraction_present);
        assert_eq!(fake.capabilities().kinds.len(), 6);
    }

    #[test]
    fn a_script_is_consumed_in_order_and_a_mismatch_is_loud() {
        let fake = FakeProvider::scripted(vec![
            FakeReply::Extract(Err(LlmError::refused())),
            FakeReply::Turn(Ok(FakeProvider::canned_answer())),
        ]);
        assert_eq!(
            fake.extract(&input(SourceKind::Text)).unwrap_err(),
            LlmError::refused()
        );
        let err = fake
            .generate(&FakeProvider::canned_extraction(), "", &[])
            .unwrap_err();
        assert!(matches!(err, LlmError::Schema { detail, .. } if detail.contains("Generate")));
        assert_eq!(fake.remaining(), 0);
        // Past the script, the defaults take over.
        assert_eq!(
            fake.generate(&FakeProvider::canned_extraction(), "", &[])
                .unwrap()
                .value
                .len(),
            3
        );
    }

    #[test]
    fn a_fake_can_claim_a_narrower_model() {
        let fake = FakeProvider::default().with_capabilities(Capabilities {
            kinds: vec![SourceKind::Text],
            ..FakeProvider::everything()
        });
        assert_eq!(fake.capabilities().kinds, [SourceKind::Text]);
    }
}
