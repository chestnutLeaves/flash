//! The sidecar every test talks to: answers at once with a small fake
//! MP4 whose bytes name the text, records what it was asked, and fails
//! on request so the worker's backoff can be seen.

use parking_lot::Mutex;

use super::{Health, Synthesized, TtsBackend, TtsError};

pub const FAKE_MODEL: &str = "fake-voice-v1";

#[derive(Default)]
pub struct FakeTts {
    calls: Mutex<Vec<(String, String)>>,
    fail: Mutex<Option<TtsError>>,
    /// Health probes answered, so a test can see the worker re-probe.
    probes: Mutex<u32>,
}

impl FakeTts {
    /// Every synthesis so far as (voice, text), in order.
    pub fn calls(&self) -> Vec<(String, String)> {
        self.calls.lock().clone()
    }

    pub fn set_failing(&self, error: Option<TtsError>) {
        *self.fail.lock() = error;
    }

    pub fn probes(&self) -> u32 {
        *self.probes.lock()
    }

    /// The bytes a synthesis of `text` answers with, for assertions.
    pub fn bytes_for(text: &str) -> Vec<u8> {
        let mut bytes = b"\0\0\0\x18ftypM4A ".to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes.resize(bytes.len().max(64), b' ');
        bytes
    }
}

impl TtsBackend for FakeTts {
    fn health(&self) -> Result<Health, TtsError> {
        *self.probes.lock() += 1;
        if let Some(e) = self.fail.lock().clone() {
            return Err(e);
        }
        Ok(Health {
            model_tag: FAKE_MODEL.to_string(),
            voices: super::voices::all().map(|v| v.id.to_string()).collect(),
        })
    }

    fn synthesize(&self, text: &str, voice: &str) -> Result<Synthesized, TtsError> {
        if let Some(e) = self.fail.lock().clone() {
            return Err(e);
        }
        self.calls
            .lock()
            .push((voice.to_string(), text.to_string()));
        Ok(Synthesized {
            bytes: Self::bytes_for(text),
            mime: "audio/mp4".to_string(),
            duration_ms: (text.chars().count() as u32) * 60,
            envelope: vec![0, 64, 128, 255],
            model_tag: FAKE_MODEL.to_string(),
        })
    }

    fn describe(&self) -> String {
        "fake voice".to_string()
    }
}
