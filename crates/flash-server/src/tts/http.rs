//! The sidecar over HTTP: two GETs and one POST, JSON both ways. The
//! audio rides back as base64 (a clip is tens of kilobytes) so the
//! client stays on the same JSON path as every other outbound call.

use base64::Engine;
use serde::Deserialize;

use super::{Health, Synthesized, TtsBackend, TtsError};
use crate::config::TtsConfig;

/// A reply larger than this is not a clip.
const REPLY_LIMIT: u64 = 4 * 1024 * 1024;
/// Decoded audio bounds: a byte or two is not a recording, two
/// megabytes is not a card.
const MIN_AUDIO: usize = 64;
const MAX_AUDIO: usize = 2 * 1024 * 1024;

pub struct HttpTts {
    base: String,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct HealthReply {
    model_tag: String,
    #[serde(default)]
    voices: serde_json::Value,
}

#[derive(Deserialize)]
struct VoicesReply {
    voices: Vec<VoiceEntry>,
}

#[derive(Deserialize)]
struct VoiceEntry {
    id: String,
}

#[derive(Deserialize)]
struct SynthReply {
    mime: String,
    audio_b64: String,
    duration_ms: u32,
    #[serde(default)]
    envelope_b64: String,
    model_tag: String,
}

#[derive(Deserialize)]
struct ErrorReply {
    error: String,
}

impl HttpTts {
    pub fn new(config: &TtsConfig) -> Self {
        Self {
            base: config.url.clone(),
            agent: crate::http::agent(config.timeout, false),
        }
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, TtsError> {
        let response = self
            .agent
            .get(format!("{}{path}", self.base))
            .call()
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .with_config()
            .limit(REPLY_LIMIT)
            .read_to_string()
            .map_err(transport_error)?;
        if !(200..300).contains(&status) {
            return Err(TtsError::Unavailable(format!("{path} answered {status}")));
        }
        serde_json::from_str(&text).map_err(|e| TtsError::Other(format!("{path}: {e}")))
    }
}

fn transport_error(e: ureq::Error) -> TtsError {
    match e {
        ureq::Error::Timeout(_) => TtsError::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => TtsError::Timeout,
        other => TtsError::Unavailable(other.to_string()),
    }
}

impl TtsBackend for HttpTts {
    fn health(&self) -> Result<Health, TtsError> {
        let health: HealthReply = self.get_json("/health")?;
        // The count in /health is informational; the ids come from /voices.
        let _ = health.voices;
        let voices = self
            .get_json::<VoicesReply>("/voices")
            .map(|v| v.voices.into_iter().map(|e| e.id).collect())
            .unwrap_or_default();
        Ok(Health {
            model_tag: health.model_tag,
            voices,
        })
    }

    fn synthesize(&self, text: &str, voice: &str) -> Result<Synthesized, TtsError> {
        let response = self
            .agent
            .post(format!("{}/synthesize", self.base))
            .send_json(serde_json::json!({ "text": text, "voice": voice }))
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let body = response
            .into_body()
            .with_config()
            .limit(REPLY_LIMIT)
            .read_to_string()
            .map_err(transport_error)?;
        match status {
            200..=299 => {}
            503 => return Err(TtsError::Busy),
            400 => {
                let why = serde_json::from_str::<ErrorReply>(&body)
                    .map(|e| e.error)
                    .unwrap_or_else(|_| "bad request".to_string());
                return Err(TtsError::Rejected(why));
            }
            other => {
                return Err(TtsError::Unavailable(format!(
                    "synthesize answered {other}"
                )))
            }
        }
        let reply: SynthReply =
            serde_json::from_str(&body).map_err(|e| TtsError::Other(format!("reply: {e}")))?;
        let engine = base64::engine::general_purpose::STANDARD;
        let bytes = engine
            .decode(reply.audio_b64.as_bytes())
            .map_err(|e| TtsError::Other(format!("audio: {e}")))?;
        if bytes.len() < MIN_AUDIO || bytes.len() > MAX_AUDIO {
            return Err(TtsError::Other(format!("audio is {} bytes", bytes.len())));
        }
        if reply.mime != "audio/mp4" || bytes.get(4..8) != Some(b"ftyp") {
            return Err(TtsError::Other(format!(
                "audio is not an MP4 ({})",
                reply.mime
            )));
        }
        let envelope = engine
            .decode(reply.envelope_b64.as_bytes())
            .map_err(|e| TtsError::Other(format!("envelope: {e}")))?;
        Ok(Synthesized {
            bytes,
            mime: reply.mime,
            duration_ms: reply.duration_ms,
            envelope,
            model_tag: reply.model_tag,
        })
    }

    fn describe(&self) -> String {
        format!("natural voice at {}", self.base)
    }
}
