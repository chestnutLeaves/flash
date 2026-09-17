//! The one loop that makes clips: takes a job, asks the sidecar, puts
//! the bytes in the media store, records the row and the ref. One job
//! at a time on purpose: the sidecar has one core, and two syntheses
//! at once would only make both late. A sidecar that stops answering
//! is left alone for a while and probed again on the next idle tick.
//!
//! Beside it, the warm-up: every phrase in every voice is made once,
//! at the back of the queue, so a picker's sample and a first grade in
//! any voice play from the cache. It only queues what the store lacks,
//! a few at a time, and goes quiet once nothing is missing.

use std::time::Duration;

use flash_store::{VoiceClipKind, VoiceClipNew};

use super::serve::phrase_row;
use super::{voices, Job, JobKind, TtsError, TtsRuntime, PHRASES};
use crate::service::now_ms;
use crate::state::AppState;

/// How long the loop sleeps between looks at the queue when nothing
/// wakes it; also how often a silent sidecar is probed.
const IDLE_TICK: Duration = Duration::from_secs(5);
/// Warm-up jobs held in the queue at once: enough to keep the sidecar
/// busy between passes, few enough that a card never waits long.
pub const WARM_BATCH: usize = 8;
/// How long the warm-up rests once nothing is missing, before it looks
/// again (an engine upgrade renames every clip).
const WARM_REST: Duration = Duration::from_secs(6 * 60 * 60);

/// What one job came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A clip was made and recorded.
    Made,
    /// Nothing to do: the clip existed, or the request was refused.
    Skipped,
    /// The sidecar is not answering; the job went back and the loop
    /// should wait.
    Paused,
}

/// The loop every binary spawns when a sidecar is configured. Never
/// returns.
pub async fn run_worker(state: AppState) {
    let Some(tts) = state.tts.clone() else {
        return;
    };
    loop {
        let _ = run_once(&state).await;
        tokio::select! {
            _ = tts.wake.notified() => {}
            _ = tokio::time::sleep(IDLE_TICK) => {}
        }
    }
}

/// What one look at the warm-up found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WarmPass {
    /// Jobs this pass put in the queue.
    pub queued: usize,
    /// Clips the store still lacks, queued or not.
    pub missing: usize,
    /// Nothing is missing any more (and the model is known).
    pub done: bool,
}

/// Passes in a row with nothing made before the warm-up steps back
/// (the sidecar is down, or refuses what is left) and rests a while
/// instead of asking every tick.
const STALL_PASSES: u32 = 12;
const STALL_REST: Duration = Duration::from_secs(10 * 60);

/// One look: the phrases no voice has yet, samples first so pickers
/// are ready soonest, queued up to the batch. Blocking: it reads the
/// store. Tests call it directly, between drains.
pub fn warm_pass(state: &AppState) -> WarmPass {
    let Some(tts) = state.tts.as_deref() else {
        return WarmPass {
            queued: 0,
            missing: 0,
            done: true,
        };
    };
    let Some(model) = tts.model_tag() else {
        return WarmPass::default();
    };
    let mut room = WARM_BATCH.saturating_sub(tts.warm_queued());
    let mut queued = 0;
    let mut missing = 0;
    let phrases = std::iter::once("sample").chain(
        PHRASES
            .iter()
            .map(|(key, _)| *key)
            .filter(|key| *key != "sample"),
    );
    for phrase in phrases {
        for voice in voices::all() {
            match phrase_row(state, voice.id, &model, phrase) {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!("voice warm-up lookup: {e}");
                    return WarmPass::default();
                }
            }
            missing += 1;
            if room > 0 && tts.enqueue_warm_phrase(voice.id, phrase) {
                room -= 1;
                queued += 1;
            }
        }
    }
    WarmPass {
        queued,
        missing,
        done: missing == 0,
    }
}

/// The warm-up loop every binary spawns beside the worker. Never
/// returns.
pub async fn run_warmup(state: AppState) {
    if state.tts.is_none() {
        return;
    }
    let mut announced = false;
    let mut last_missing = usize::MAX;
    let mut stalled: u32 = 0;
    loop {
        let pass = {
            let state = state.clone();
            tokio::task::spawn_blocking(move || warm_pass(&state))
                .await
                .unwrap_or_default()
        };
        if pass.done {
            if announced {
                tracing::info!("natural voice: every phrase is made in every voice");
            }
            announced = false;
            last_missing = usize::MAX;
            stalled = 0;
            tokio::time::sleep(WARM_REST).await;
            continue;
        }
        if pass.queued > 0 && !announced {
            tracing::info!(
                missing = pass.missing,
                "natural voice: making the phrases every voice is missing"
            );
            announced = true;
        }
        if pass.missing < last_missing {
            stalled = 0;
        } else {
            stalled += 1;
        }
        last_missing = pass.missing;
        if stalled >= STALL_PASSES {
            tracing::info!(
                missing = pass.missing,
                "natural voice: warm-up is not getting anywhere; resting"
            );
            stalled = 0;
            tokio::time::sleep(STALL_REST).await;
            continue;
        }
        tokio::time::sleep(IDLE_TICK).await;
    }
}

/// Drains the queue serially: every job until it is empty or the
/// sidecar stops answering. Returns how many clips were made. Tests
/// call it directly; the loop calls it every wake.
pub async fn run_once(state: &AppState) -> u32 {
    let Some(tts) = state.tts.clone() else {
        return 0;
    };
    let mut made = 0;
    loop {
        if tts.model_tag().is_none() && !tts.unavailable() {
            let tts = tts.clone();
            let _ = tokio::task::spawn_blocking(move || tts.probe_health()).await;
        }
        let Some(job) = tts.pop_job() else {
            break;
        };
        let outcome = {
            let state = state.clone();
            let tts = tts.clone();
            tokio::task::spawn_blocking(move || synthesize_job(&state, &tts, job))
                .await
                .unwrap_or(Outcome::Skipped)
        };
        match outcome {
            Outcome::Made => made += 1,
            Outcome::Skipped => {}
            Outcome::Paused => break,
        }
    }
    made
}

/// One job, start to finish, on a blocking thread.
pub fn synthesize_job(state: &AppState, tts: &TtsRuntime, job: Job) -> Outcome {
    if tts.unavailable() || tts.model_tag().is_none() {
        tts.requeue_untried(job);
        return Outcome::Paused;
    }
    let store = state.services.store();
    // An answer is held in memory under its token, never in the store:
    // nothing to find, nothing to record.
    if !matches!(job.kind, JobKind::Answer { .. }) {
        match store.find_voice_clip(&job.key) {
            Ok(Some(row)) => {
                touch(state, &job, row.id);
                tts.release(&job.key);
                return Outcome::Skipped;
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("voice clip lookup: {e}");
                tts.release(&job.key);
                return Outcome::Skipped;
            }
        }
    }
    let made = match tts.backend.synthesize(&job.text, &job.voice) {
        Ok(made) => made,
        Err(e) if e.is_transport() => {
            tracing::warn!("voice service: {e}");
            tts.set_unavailable();
            tts.requeue(job);
            return Outcome::Paused;
        }
        Err(TtsError::Rejected(why)) => {
            tracing::info!(voice = %job.voice, "voice clip refused: {why}");
            tts.release(&job.key);
            return Outcome::Skipped;
        }
        Err(e) => {
            tracing::warn!("voice clip: {e}");
            tts.release(&job.key);
            return Outcome::Skipped;
        }
    };
    let (kind, phrase) = match &job.kind {
        JobKind::Answer { token } => {
            tts.finish_answer(token, made);
            tts.release(&job.key);
            return Outcome::Made;
        }
        JobKind::Card { .. } => (VoiceClipKind::Card, None),
        JobKind::Phrase { phrase } => (VoiceClipKind::Phrase, Some(*phrase)),
    };
    if made.model_tag != job.model {
        // The sidecar changed models under us: the key names the old
        // one. Record what was made under the new tag on the next pass.
        tracing::info!(model = %made.model_tag, "voice service changed model");
        tts.probe_health();
        tts.release(&job.key);
        return Outcome::Skipped;
    }
    if let Err(e) = state.media.put(&job.key, &made.mime, &made.bytes) {
        tracing::warn!("voice clip store: {e}");
        tts.release(&job.key);
        return Outcome::Skipped;
    }
    let inserted = store.insert_voice_clip(&VoiceClipNew {
        key: &job.key,
        kind,
        voice: &job.voice,
        model: &job.model,
        phrase,
        mime: &made.mime,
        size: made.bytes.len() as u64,
        duration_ms: made.duration_ms,
        envelope: &made.envelope,
        now_ms: now_ms(),
    });
    let outcome = match inserted {
        Ok(clip_id) => {
            touch(state, &job, clip_id);
            Outcome::Made
        }
        Err(e) => {
            tracing::warn!("voice clip row: {e}");
            Outcome::Skipped
        }
    };
    tts.release(&job.key);
    outcome
}

fn touch(state: &AppState, job: &Job, clip_id: i64) {
    if let JobKind::Card { user, card, side } = job.kind {
        if let Err(e) =
            state
                .services
                .store()
                .touch_voice_clip_ref(user, card, side, clip_id, now_ms())
        {
            // A card deleted between the queue and now: nothing to note.
            tracing::debug!("voice clip ref: {e}");
        }
    }
}
