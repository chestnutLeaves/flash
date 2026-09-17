//! Natural voice. A sidecar (tools/flash-tts) turns the spoken form of a
//! card into a short recording; this module decides what to make, when,
//! and how it is found again. Clips are made a few cards ahead of the
//! study queue by one background worker, never inside a request: a
//! request only looks a clip up, and a miss queues it and answers with
//! nothing. The apps then wait for the clip, asking again every second,
//! rather than read with the device's own voice.
//!
//! A clip is named by a hash of (model, voice, spoken text) and shared
//! by every account whose card says the same words. The hash stays on
//! the server: a client asks for its own card's audio by card and side.
//! A fixed set of phrases per voice (the verdicts, the prompts, the
//! picker's sample) is made the same way, keyed by phrase. The answer to
//! a question asked mid-session is one-off text: its clip is made first
//! in line and held in memory for a few minutes under a random token,
//! never in the store.

pub mod fake;
pub mod http;
pub mod serve;
pub mod voices;
pub mod worker;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flash_core::{CardId, UserId};
use flash_store::VoiceSide;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};

use crate::config::TtsConfig;
use crate::service::Services;

/// A side longer than this stays with the device voice: a page of text
/// is not a card, and one synthesis would hold the core for a minute.
pub const MAX_CLIP_CHARS: usize = 1000;
/// Cards after the shown one whose clips are made ahead.
pub const LOOKAHEAD_CARDS: usize = 3;
/// Jobs the queue holds at most, across every account.
pub const QUEUE_CAP: usize = 64;
/// Card jobs one account may hold in the queue at once.
pub const PER_USER_CAP: usize = 8;
/// The loudness envelope's rate: one byte per 50 ms.
pub const ENVELOPE_HZ: u32 = 20;
/// How long the worker leaves the sidecar alone after it failed to
/// answer, before trying again.
const BACKOFF: Duration = Duration::from_secs(30);
/// A job that failed on transport is tried this many times in all.
const MAX_ATTEMPTS: u8 = 2;
/// How long an answer's clip is kept once asked for: long enough to be
/// played and repeated, short enough that the process holds a handful.
const ANSWER_TTL: Duration = Duration::from_secs(10 * 60);
/// Answer clips held at once, across every account.
const ANSWER_CAP: usize = 64;

/// The fixed phrases every voice speaks, keyed the way the clients ask
/// for them. The sample's text here is the English line; `phrase_text`
/// swaps in the voice's own language and name.
pub const PHRASES: &[(&str, &str)] = &[
    ("correct", "Correct."),
    ("close", "Close."),
    ("not_quite", "Not quite."),
    ("again_q", "Again?"),
    ("hard_q", "Hard?"),
    ("good_q", "Good?"),
    ("easy_q", "Easy?"),
    ("rate_prompt", "Rate it: again, hard, good, or easy."),
    ("rate_again", "Say again, hard, good, or easy."),
    ("session_complete", "Session complete."),
    ("grading_busy", "Grading is busy right now."),
    ("grading_down", "Grading isn't available right now."),
    ("clarify_down", "I can't answer that right now."),
    ("unclear", "Sorry, I didn't catch that."),
    ("sample", voices::SAMPLE_EN),
];

/// The phrase keys, for `/meta`.
pub fn phrase_keys() -> Vec<&'static str> {
    PHRASES.iter().map(|(k, _)| *k).collect()
}

/// What a phrase says in a voice. The sample is the one phrase that
/// differs by voice: each voice introduces itself in its own language
/// and by the name its front end can read; the study phrases are the
/// English words the recognizer listens for, in every voice. None for
/// a phrase nobody defined, or a sample of a voice nobody has.
pub fn phrase_text(key: &str, voice: &str) -> Option<String> {
    let (_, text) = PHRASES.iter().find(|(k, _)| *k == key)?;
    if key == "sample" {
        let lang = voices::find(voice)?.lang;
        return Some(
            lang.sample_line()
                .replace("{Name}", &voices::spoken_name(voice)),
        );
    }
    Some((*text).to_string())
}

/// The content key of a clip: its own prefix keeps it apart from every
/// media hash in the same store.
pub fn clip_key(model: &str, voice: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"flash-voice-clip\n");
    hasher.update(model.as_bytes());
    hasher.update(b"\n");
    hasher.update(voice.as_bytes());
    hasher.update(b"\n");
    hasher.update(text.as_bytes());
    crate::media_store::hex(&hasher.finalize())
}

/// One recording as the sidecar made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synthesized {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub duration_ms: u32,
    /// One byte per 50 ms, 0..255.
    pub envelope: Vec<u8>,
    pub model_tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    pub model_tag: String,
    pub voices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TtsError {
    /// The sidecar did not answer.
    #[error("voice service unavailable: {0}")]
    Unavailable(String),
    /// It answered that it is busy; try again shortly.
    #[error("voice service busy")]
    Busy,
    /// It refused the request itself (an unknown voice, empty text).
    #[error("voice service refused: {0}")]
    Rejected(String),
    #[error("voice service timed out")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

impl TtsError {
    /// A failure of the transport, worth a retry after the backoff.
    pub fn is_transport(&self) -> bool {
        matches!(
            self,
            TtsError::Unavailable(_) | TtsError::Busy | TtsError::Timeout
        )
    }
}

/// The sidecar behind one trait; tests install `fake::FakeTts`.
pub trait TtsBackend: Send + Sync {
    fn health(&self) -> Result<Health, TtsError>;
    fn synthesize(&self, text: &str, voice: &str) -> Result<Synthesized, TtsError>;
    fn describe(&self) -> String;
}

/// What a job is for: a card side for an account, a phrase, or the
/// answer to a question, held under its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobKind {
    Card {
        user: UserId,
        card: CardId,
        side: VoiceSide,
    },
    Phrase {
        phrase: &'static str,
    },
    Answer {
        token: String,
    },
}

/// One answer's clip, from the moment it is asked for until it expires.
struct AnswerSlot {
    user: UserId,
    queued: Instant,
    made: Option<Synthesized>,
}

/// What an answer's token names right now.
pub enum AnswerLookup {
    /// No such token, another account's, or expired.
    Missing,
    /// Queued or being made.
    Pending,
    Made {
        mime: String,
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub key: String,
    pub text: String,
    pub voice: String,
    /// The model tag the key was computed with.
    pub model: String,
    pub kind: JobKind,
    /// The card someone is looking at right now goes first.
    pub urgent: bool,
    /// Queued by the warm-up, which makes every phrase in every voice
    /// ahead of anyone asking: last in line behind all live work.
    pub warm: bool,
    attempts: u8,
}

/// The sidecar, the queue in front of it, and what the last health
/// probe said. One per process.
pub struct TtsRuntime {
    pub backend: Arc<dyn TtsBackend>,
    pub default_voice: String,
    /// The model tag from the last good probe: part of every clip key,
    /// so nothing is looked up or made until the sidecar has answered
    /// once.
    model_tag: RwLock<Option<String>>,
    unavailable_until: Mutex<Option<Instant>>,
    queue: Mutex<VecDeque<Job>>,
    /// Keys queued or being made, so a card asked for twice is one job.
    inflight: Mutex<HashSet<String>>,
    /// Voices whose phrase set this process already queued.
    phrases_queued: Mutex<HashSet<String>>,
    /// Answers asked for lately, by token.
    answers: Mutex<HashMap<String, AnswerSlot>>,
    /// Poked when a job is queued, so the worker does not wait out its
    /// idle interval.
    pub wake: tokio::sync::Notify,
}

impl TtsRuntime {
    pub fn with_backend(backend: Arc<dyn TtsBackend>, default_voice: &str) -> Self {
        let runtime = Self {
            backend,
            default_voice: default_voice.to_string(),
            model_tag: RwLock::new(None),
            unavailable_until: Mutex::new(None),
            queue: Mutex::new(VecDeque::new()),
            inflight: Mutex::new(HashSet::new()),
            phrases_queued: Mutex::new(HashSet::new()),
            answers: Mutex::new(HashMap::new()),
            wake: tokio::sync::Notify::new(),
        };
        runtime.probe_health();
        runtime
    }

    /// The runtime the configuration describes. A sidecar that is down
    /// at boot is a warning, not an error: the worker keeps probing.
    pub fn from_config(config: &TtsConfig) -> Self {
        let backend: Arc<dyn TtsBackend> = Arc::new(http::HttpTts::new(config));
        Self::with_backend(backend, &config.default_voice)
    }

    pub fn describe(&self) -> String {
        match self.model_tag() {
            Some(tag) => format!("{} ({tag})", self.backend.describe()),
            None => format!("{} (not reachable yet)", self.backend.describe()),
        }
    }

    /// Asks the sidecar how it is; records the model tag on success and
    /// warns about voices the table does not know or lacks.
    pub fn probe_health(&self) -> bool {
        match self.backend.health() {
            Ok(health) => {
                let mut known: Vec<&str> = voices::all().map(|v| v.id).collect();
                known.sort_unstable();
                let mut listed: Vec<&str> = health.voices.iter().map(String::as_str).collect();
                listed.sort_unstable();
                if !listed.is_empty() && listed != known {
                    tracing::warn!(
                        "voice service lists {} voices, the server knows {}; the server's \
                         table decides what is offered",
                        listed.len(),
                        known.len()
                    );
                }
                let changed = self.model_tag.read().as_deref() != Some(health.model_tag.as_str());
                if changed {
                    tracing::info!(model = %health.model_tag, "voice service ready");
                }
                *self.model_tag.write() = Some(health.model_tag);
                *self.unavailable_until.lock() = None;
                true
            }
            Err(e) => {
                tracing::warn!("voice service: {e}");
                self.set_unavailable();
                false
            }
        }
    }

    pub fn model_tag(&self) -> Option<String> {
        self.model_tag.read().clone()
    }

    /// The worker leaves the sidecar alone until this passes.
    pub fn unavailable(&self) -> bool {
        self.unavailable_until
            .lock()
            .is_some_and(|until| Instant::now() < until)
    }

    pub(crate) fn set_unavailable(&self) {
        *self.unavailable_until.lock() = Some(Instant::now() + BACKOFF);
    }

    /// Queues a job unless the same key is already queued or being made,
    /// the queue is full, or the account already holds its share. True
    /// when it was taken.
    pub fn enqueue(&self, job: Job) -> bool {
        let mut inflight = self.inflight.lock();
        if inflight.contains(&job.key) {
            return false;
        }
        let mut queue = self.queue.lock();
        if queue.len() >= QUEUE_CAP {
            return false;
        }
        if let JobKind::Card { user, .. } = job.kind {
            let held = queue
                .iter()
                .filter(|j| matches!(j.kind, JobKind::Card { user: u, .. } if u == user))
                .count();
            if held >= PER_USER_CAP {
                return false;
            }
        }
        inflight.insert(job.key.clone());
        // The card someone is looking at, then the cards coming up, then
        // the phrases; within a rank, arrival order, so a card's front
        // is made before its back.
        let rank = job.rank();
        let at = queue
            .iter()
            .position(|j| j.rank() > rank)
            .unwrap_or(queue.len());
        queue.insert(at, job);
        drop(queue);
        drop(inflight);
        self.wake.notify_one();
        true
    }

    /// Both sides of a card for an account in a voice, when the model
    /// is known and the text is a card's worth. Sides too long or empty
    /// are left to the device voice.
    pub fn enqueue_card_sides(
        &self,
        user: UserId,
        card: CardId,
        voice: &str,
        front: &str,
        back: &str,
        urgent: bool,
    ) {
        let Some(model) = self.model_tag() else {
            return;
        };
        for (side, text) in [(VoiceSide::Front, front), (VoiceSide::Back, back)] {
            if !clip_worthy(text) {
                continue;
            }
            self.enqueue(Job {
                key: clip_key(&model, voice, text),
                text: text.to_string(),
                voice: voice.to_string(),
                model: model.clone(),
                kind: JobKind::Card { user, card, side },
                urgent,
                warm: false,
                attempts: 0,
            });
        }
    }

    /// Every phrase in a voice, once per process per voice; the worker
    /// skips the ones that already exist.
    pub fn enqueue_phrases(&self, voice: &str) {
        let Some(model) = self.model_tag() else {
            return;
        };
        if voices::find(voice).is_none() || !self.phrases_queued.lock().insert(voice.to_string()) {
            return;
        }
        for (key, _) in PHRASES {
            let Some(text) = phrase_text(key, voice) else {
                continue;
            };
            self.enqueue(Job {
                key: clip_key(&model, voice, &text),
                text,
                voice: voice.to_string(),
                model: model.clone(),
                kind: JobKind::Phrase { phrase: key },
                urgent: false,
                warm: false,
                attempts: 0,
            });
        }
    }

    /// One phrase in one voice for the warm-up, behind every live job.
    /// True when it was taken; false when the key is already queued or
    /// being made, the queue is full, or the model is not known yet.
    pub fn enqueue_warm_phrase(&self, voice: &str, phrase: &'static str) -> bool {
        let Some(model) = self.model_tag() else {
            return false;
        };
        if voices::find(voice).is_none() {
            return false;
        }
        let Some(text) = phrase_text(phrase, voice) else {
            return false;
        };
        self.enqueue(Job {
            key: clip_key(&model, voice, &text),
            text,
            voice: voice.to_string(),
            model,
            kind: JobKind::Phrase { phrase },
            urgent: false,
            warm: true,
            attempts: 0,
        })
    }

    /// Lets a voice's phrase set be queued again (a phrase went missing
    /// from the store).
    pub fn forget_phrases(&self, voice: &str) {
        self.phrases_queued.lock().remove(voice);
    }

    /// The answer to a question, first in line in the account's voice,
    /// under a fresh token the account fetches it by. None when the
    /// model is not known yet or the text is not a clip's worth, and
    /// the client shows the answer without hearing it.
    pub fn enqueue_answer(&self, user: UserId, voice: &str, text: &str) -> Option<String> {
        let model = self.model_tag()?;
        if !clip_worthy(text) {
            return None;
        }
        let token = answer_token();
        {
            let mut answers = self.answers.lock();
            let now = Instant::now();
            answers.retain(|_, slot| now.duration_since(slot.queued) < ANSWER_TTL);
            while answers.len() >= ANSWER_CAP {
                let Some(oldest) = answers
                    .iter()
                    .min_by_key(|(_, slot)| slot.queued)
                    .map(|(token, _)| token.clone())
                else {
                    break;
                };
                answers.remove(&oldest);
            }
            answers.insert(
                token.clone(),
                AnswerSlot {
                    user,
                    queued: now,
                    made: None,
                },
            );
        }
        let taken = self.enqueue(Job {
            key: format!("answer:{token}"),
            text: text.to_string(),
            voice: voice.to_string(),
            model,
            kind: JobKind::Answer {
                token: token.clone(),
            },
            urgent: true,
            warm: false,
            attempts: 0,
        });
        if !taken {
            self.answers.lock().remove(&token);
            return None;
        }
        Some(token)
    }

    /// The worker made an answer's clip; a token that expired meanwhile
    /// drops it.
    pub(crate) fn finish_answer(&self, token: &str, made: Synthesized) {
        if let Some(slot) = self.answers.lock().get_mut(token) {
            slot.made = Some(made);
        }
    }

    /// An answer's clip for the account that asked, if it is there yet.
    pub fn answer_clip(&self, user: UserId, token: &str) -> AnswerLookup {
        let answers = self.answers.lock();
        let Some(slot) = answers.get(token) else {
            return AnswerLookup::Missing;
        };
        if slot.user != user || slot.queued.elapsed() >= ANSWER_TTL {
            return AnswerLookup::Missing;
        }
        match &slot.made {
            Some(made) => AnswerLookup::Made {
                mime: made.mime.clone(),
                bytes: made.bytes.clone(),
            },
            None => AnswerLookup::Pending,
        }
    }

    pub fn queued(&self) -> usize {
        self.queue.lock().len()
    }

    /// Warm-up jobs waiting in the queue.
    pub fn warm_queued(&self) -> usize {
        self.queue.lock().iter().filter(|j| j.warm).count()
    }

    pub(crate) fn pop_job(&self) -> Option<Job> {
        self.queue.lock().pop_front()
    }

    /// The job is done with, made or dropped: its key may be queued again.
    pub(crate) fn release(&self, key: &str) {
        self.inflight.lock().remove(key);
    }

    /// Back of the queue after a failed try, or dropped once it has had
    /// its tries.
    pub(crate) fn requeue(&self, mut job: Job) {
        job.attempts += 1;
        if job.attempts >= MAX_ATTEMPTS {
            tracing::warn!(voice = %job.voice, "voice clip dropped after {} attempts", job.attempts);
            self.release(&job.key);
            return;
        }
        self.queue.lock().push_back(job);
    }

    /// Front of the queue again, untouched: the sidecar was not asked.
    pub(crate) fn requeue_untried(&self, job: Job) {
        self.queue.lock().push_front(job);
    }
}

impl Job {
    /// Lower goes first: the shown card and an answer someone is waiting
    /// on, the cards coming up, then the phrases of a voice someone
    /// picked, then the warm-up, which is for nobody in particular.
    fn rank(&self) -> u8 {
        match (&self.kind, self.urgent, self.warm) {
            (JobKind::Answer { .. }, _, _) => 0,
            (JobKind::Card { .. }, true, _) => 0,
            (JobKind::Card { .. }, false, _) => 1,
            (JobKind::Phrase { .. }, _, false) => 2,
            (JobKind::Phrase { .. }, _, true) => 3,
        }
    }
}

/// An answer's token: 128 random bits, base64url, short enough for a
/// path keyword.
fn answer_token() -> String {
    use base64::Engine;
    let bytes: [u8; 16] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Text a clip is made for: something to say, and not a page of it.
pub fn clip_worthy(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= MAX_CLIP_CHARS
}

/// Queues the shown card's sides first and the next few cards' after,
/// in the account's voice, plus the voice's phrase set once. Called
/// inside the blocking call that started or advanced a session.
pub fn prefetch_for_study(
    tts: &TtsRuntime,
    s: &Services,
    user: UserId,
    shown: Option<CardId>,
    upcoming: &[CardId],
) {
    if tts.model_tag().is_none() {
        return;
    }
    let voice = match s.voice_settings(user) {
        Ok(settings) => settings.voice_id,
        Err(e) => {
            tracing::warn!("voice settings: {e}");
            return;
        }
    };
    tts.enqueue_phrases(&voice);
    let cards = shown
        .into_iter()
        .map(|c| (c, true))
        .chain(upcoming.iter().take(LOOKAHEAD_CARDS).map(|c| (*c, false)));
    for (card, urgent) in cards {
        match s.spoken_sides(user, card) {
            Ok(Some((front, back))) => {
                tts.enqueue_card_sides(user, card, &voice, &front, &back, urgent)
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("voice prefetch: {e}"),
        }
    }
}

/// Queues clips for the head of the account's whole queue before any
/// session starts: the Today page is where a session comes from, so by
/// the time Study opens, its first card is usually made.
pub fn prefetch_queue_head(tts: &TtsRuntime, s: &Services, user: UserId, now_ms: i64) {
    if tts.model_tag().is_none() {
        return;
    }
    match s.queue_head(user, LOOKAHEAD_CARDS + 1, now_ms) {
        Ok(head) => prefetch_for_study(
            tts,
            s,
            user,
            head.first().copied(),
            head.get(1..).unwrap_or(&[]),
        ),
        Err(e) => tracing::warn!("voice prefetch: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_apart_from_media_hashes() {
        let a = clip_key("m1", "af_heart", "Four.");
        assert_eq!(a, clip_key("m1", "af_heart", "Four."));
        assert_ne!(a, clip_key("m2", "af_heart", "Four."));
        assert_ne!(a, clip_key("m1", "bm_fable", "Four."));
        assert_ne!(a, clip_key("m1", "af_heart", "Four"));
        assert_eq!(a.len(), 64);
        assert_ne!(
            a,
            crate::media_store::hex(&Sha256::digest(b"m1\naf_heart\nFour."))
        );
    }

    #[test]
    fn phrases_are_unique_url_safe_and_named() {
        let mut keys: Vec<&str> = phrase_keys();
        assert!(keys
            .iter()
            .all(|k| k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')));
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), PHRASES.len());
        assert_eq!(
            phrase_text("sample", "af_heart").unwrap(),
            "Hi, I'm Heart. I've read all your cards. Twice. Shall we?"
        );
        assert_eq!(
            phrase_text("correct", "bm_fable").as_deref(),
            Some("Correct.")
        );
        assert!(phrase_text("nope", "af_heart").is_none());
    }

    #[test]
    fn a_voice_introduces_itself_in_its_own_language() {
        let japanese = phrase_text("sample", "jf_alpha").unwrap();
        assert!(
            japanese.starts_with("こんにちは、アルファです。"),
            "{japanese}"
        );
        assert!(!japanese.contains("Alpha"), "{japanese}");
        let british = phrase_text("sample", "bm_lewis").unwrap();
        assert!(british.starts_with("Hi, I'm Lewis."), "{british}");
        let mandarin = phrase_text("sample", "zf_xiaobei").unwrap();
        assert!(mandarin.contains("晓北"), "{mandarin}");
        // The study phrases are the recognizer's English words in every voice.
        assert_eq!(
            phrase_text("again_q", "jf_alpha").as_deref(),
            Some("Again?")
        );
        // A sample needs a voice to speak it.
        assert!(phrase_text("sample", "zz_nobody").is_none());
        assert_ne!(
            clip_key("m1", "jf_alpha", &japanese),
            clip_key(
                "m1",
                "jf_alpha",
                &voices::SAMPLE_EN.replace("{Name}", "Alpha")
            )
        );
    }

    #[test]
    fn the_queue_dedupes_orders_and_caps() {
        let runtime = TtsRuntime::with_backend(Arc::new(fake::FakeTts::default()), "af_heart");
        let user = UserId::from_db(1);
        let job = |n: u32, urgent: bool| Job {
            key: format!("k{n}"),
            text: "t".into(),
            voice: "af_heart".into(),
            model: fake::FAKE_MODEL.into(),
            kind: JobKind::Card {
                user,
                card: CardId(n as i64),
                side: VoiceSide::Front,
            },
            urgent,
            warm: false,
            attempts: 0,
        };
        assert!(runtime.enqueue(job(1, false)));
        assert!(!runtime.enqueue(job(1, false)), "same key twice");
        assert!(runtime.enqueue(job(2, true)));
        assert_eq!(runtime.pop_job().unwrap().key, "k2", "urgent goes first");
        for n in 3..3 + PER_USER_CAP as u32 {
            runtime.enqueue(job(n, false));
        }
        assert_eq!(runtime.queued(), PER_USER_CAP);
        assert!(!runtime.enqueue(job(99, false)), "per-user cap");
        let other = Job {
            kind: JobKind::Phrase { phrase: "correct" },
            ..job(100, false)
        };
        assert!(runtime.enqueue(other), "phrases are not per-user");
        // A phrase queued first still waits behind the cards that follow.
        let fresh = TtsRuntime::with_backend(Arc::new(fake::FakeTts::default()), "af_heart");
        fresh.enqueue(Job {
            kind: JobKind::Phrase { phrase: "correct" },
            ..job(1, false)
        });
        fresh.enqueue(job(2, false));
        fresh.enqueue(job(3, true));
        let order: Vec<String> = std::iter::from_fn(|| fresh.pop_job().map(|j| j.key)).collect();
        assert_eq!(order, ["k3", "k2", "k1"]);
        // The warm-up waits behind a picked voice's phrases, whatever
        // the arrival order.
        assert!(fresh.enqueue_warm_phrase("bm_fable", "correct"));
        assert!(
            !fresh.enqueue_warm_phrase("bm_fable", "correct"),
            "same clip twice"
        );
        assert!(!fresh.enqueue_warm_phrase("zz_nobody", "correct"));
        assert!(!fresh.enqueue_warm_phrase("bm_fable", "nope"));
        fresh.enqueue(Job {
            kind: JobKind::Phrase { phrase: "correct" },
            ..job(4, false)
        });
        assert_eq!(fresh.warm_queued(), 1);
        let order: Vec<bool> = std::iter::from_fn(|| fresh.pop_job().map(|j| j.warm)).collect();
        assert_eq!(order, [false, true]);
        assert!(clip_worthy("Four."));
        assert!(!clip_worthy("   "));
        assert!(!clip_worthy(&"x".repeat(MAX_CLIP_CHARS + 1)));
    }
}
