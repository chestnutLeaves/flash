//! Finding a clip for a request: the descriptor a card DTO carries
//! (duration and envelope, when the clip exists), the bytes an audio
//! route sends, the phrase set a voice offers, and the voice listing.
//! A miss queues the work and answers with nothing; no request waits.

use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use flash_core::{CardId, UserId};
use flash_store::{VoiceClipRow, VoiceSide};
use serde::Serialize;

use super::{
    clip_key, clip_worthy, phrase_keys, phrase_text, voices, AnswerLookup, TtsRuntime, PHRASES,
};
use crate::media_store::{Blob, MediaStoreError};
use crate::service::{now_ms, Reveal, Services, StudyCard, VoiceClip};
use crate::state::AppState;

/// Card clips revalidate on every use: the URL is by card and side, and
/// what it plays changes with the account's voice.
pub const CARD_CACHE: &str = "private, no-cache";
/// A phrase in a voice is the same for a day at least.
pub const PHRASE_CACHE: &str = "private, max-age=86400";

/// What a client needs to play a clip: where, how long, and the
/// loudness curve to animate with.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClipDescriptor {
    pub url: String,
    pub duration_ms: u32,
    /// Base64 of one byte per 50 ms.
    pub envelope: String,
}

impl ClipDescriptor {
    pub fn new(url: String, clip: &VoiceClip) -> Self {
        Self {
            url,
            duration_ms: clip.duration_ms,
            envelope: base64::engine::general_purpose::STANDARD.encode(&clip.envelope),
        }
    }
}

/// The clip of one side of the user's card, when it exists; a miss
/// queues both sides first in line. Runs on the blocking pool.
fn card_clip(
    tts: &TtsRuntime,
    s: &Services,
    user: UserId,
    card: CardId,
    side: VoiceSide,
    text: &str,
) -> Option<VoiceClip> {
    let model = tts.model_tag()?;
    if !clip_worthy(text) {
        return None;
    }
    let voice = s.voice_settings(user).ok()?.voice_id;
    let key = clip_key(&model, &voice, text);
    match s.store().find_voice_clip(&key) {
        Ok(Some(row)) => {
            let _ = s
                .store()
                .touch_voice_clip_ref(user, card, side, row.id, now_ms());
            Some(VoiceClip {
                duration_ms: row.duration_ms,
                envelope: row.envelope,
            })
        }
        Ok(None) => {
            if let Ok(Some((front, back))) = s.spoken_sides(user, card) {
                tts.enqueue_card_sides(user, card, &voice, &front, &back, true);
            }
            None
        }
        Err(e) => {
            tracing::warn!("voice clip lookup: {e}");
            None
        }
    }
}

/// Fills the front's clip on a card about to be shown.
pub fn attach_study_card(
    tts: Option<&TtsRuntime>,
    s: &Services,
    user: UserId,
    card: &mut StudyCard,
) {
    if let Some(tts) = tts {
        card.audio_front = card_clip(
            tts,
            s,
            user,
            card.card_id,
            VoiceSide::Front,
            &card.spoken_front,
        );
    }
}

/// Fills both sides' clips on a reveal.
pub fn attach_reveal(tts: Option<&TtsRuntime>, s: &Services, user: UserId, reveal: &mut Reveal) {
    if let Some(tts) = tts {
        reveal.audio_front = card_clip(
            tts,
            s,
            user,
            reveal.card_id,
            VoiceSide::Front,
            &reveal.spoken_front,
        );
        reveal.audio_back = card_clip(
            tts,
            s,
            user,
            reveal.card_id,
            VoiceSide::Back,
            &reveal.spoken_back,
        );
    }
}

/// What an audio route found.
pub enum Resolved {
    Ready {
        row: Box<VoiceClipRow>,
        blob: Blob,
        etag: String,
    },
    NotModified {
        etag: String,
    },
    /// A clip made for one request and held in memory: an answer.
    Fresh {
        mime: String,
        bytes: Vec<u8>,
    },
    /// The clip is not made yet (it is queued now).
    NotReady,
    /// The text is not a clip's worth (empty, or a page of it): no clip
    /// will ever come, and nothing waits for one.
    Unvoiced,
    /// No such card for this user, or no such phrase, voice or answer.
    NotFound,
    /// No sidecar on this server.
    Off,
}

fn etag_matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|e| e.trim() == etag))
}

fn fetch(state: &AppState, row: VoiceClipRow, etag: String, headers: &HeaderMap) -> Resolved {
    if etag_matches(headers, &etag) {
        return Resolved::NotModified { etag };
    }
    match state.media.get(&row.key, None) {
        Ok(blob) => Resolved::Ready {
            row: Box::new(row),
            blob,
            etag,
        },
        Err(MediaStoreError::NotFound) => {
            // The row outlived its bytes: forget it so it is made again.
            let _ = state.services.store().forget_voice_clip(&row.key);
            if let (Some(tts), Some(phrase)) = (state.tts.as_deref(), row.phrase.as_deref()) {
                let _ = phrase;
                tts.forget_phrases(&row.voice);
            }
            Resolved::NotReady
        }
        Err(e) => {
            tracing::warn!("voice clip get: {e}");
            Resolved::NotReady
        }
    }
}

/// One side of the user's own card. Runs on the blocking pool.
pub fn resolve_card_clip(
    state: &AppState,
    user: UserId,
    card: CardId,
    side: VoiceSide,
    headers: &HeaderMap,
) -> Resolved {
    let Some(tts) = state.tts.as_deref() else {
        return Resolved::Off;
    };
    let s = &state.services;
    let (front, back) = match s.spoken_sides(user, card) {
        Ok(Some(sides)) => sides,
        Ok(None) => return Resolved::NotFound,
        Err(e) => {
            tracing::warn!("voice clip card: {e}");
            return Resolved::NotFound;
        }
    };
    let text = match side {
        VoiceSide::Front => &front,
        VoiceSide::Back => &back,
    };
    if !clip_worthy(text) {
        return Resolved::Unvoiced;
    }
    let Some(model) = tts.model_tag() else {
        return Resolved::NotReady;
    };
    let voice = match s.voice_settings(user) {
        Ok(settings) => settings.voice_id,
        Err(_) => return Resolved::NotFound,
    };
    let key = clip_key(&model, &voice, text);
    match s.store().find_voice_clip(&key) {
        Ok(Some(row)) => {
            let Ok(ref_id) = s
                .store()
                .touch_voice_clip_ref(user, card, side, row.id, now_ms())
            else {
                return Resolved::NotFound;
            };
            fetch(state, row, format!("\"v{ref_id}\""), headers)
        }
        Ok(None) => {
            tts.enqueue_card_sides(user, card, &voice, &front, &back, true);
            Resolved::NotReady
        }
        Err(e) => {
            tracing::warn!("voice clip lookup: {e}");
            Resolved::NotReady
        }
    }
}

/// The clip of a phrase in a voice, by the words the phrase says today.
/// A row filed under the same phrase name with other words is the phrase
/// as it used to read (a voice's demo line moved to its own language,
/// say): it is dropped, bytes and all, so the caller queues the phrase
/// again and nobody hears the old words. Blocking: it reads the store.
pub fn phrase_row(
    state: &AppState,
    voice: &str,
    model: &str,
    phrase: &str,
) -> flash_store::Result<Option<VoiceClipRow>> {
    let Some(text) = phrase_text(phrase, voice) else {
        return Ok(None);
    };
    let key = clip_key(model, voice, &text);
    let mut hit = None;
    for row in state
        .services
        .store()
        .find_phrase_clips_of(voice, model, phrase)?
    {
        if row.key == key {
            hit = Some(row);
        } else {
            forget_stale(state, &row);
        }
    }
    Ok(hit)
}

/// Drops a phrase row whose words are no longer the phrase's, and its
/// bytes with it. Both deletes are a statement of intent: a miss on
/// either is fine, the row is looked up by key next time either way.
fn forget_stale(state: &AppState, row: &VoiceClipRow) {
    tracing::info!(
        "voice phrase {} in {} reads differently now; dropping the old clip",
        row.phrase.as_deref().unwrap_or("?"),
        row.voice
    );
    let _ = state.services.store().forget_voice_clip(&row.key);
    let _ = state.media.delete(&row.key);
}

/// One phrase in one voice. Runs on the blocking pool.
pub fn resolve_phrase_clip(
    state: &AppState,
    voice: &str,
    phrase: &str,
    headers: &HeaderMap,
) -> Resolved {
    let Some(tts) = state.tts.as_deref() else {
        return Resolved::Off;
    };
    if voices::find(voice).is_none() || !phrase_keys().contains(&phrase) {
        return Resolved::NotFound;
    }
    let Some(model) = tts.model_tag() else {
        return Resolved::NotReady;
    };
    match phrase_row(state, voice, &model, phrase) {
        Ok(Some(row)) => {
            let etag = format!("\"p{}\"", row.id);
            fetch(state, row, etag, headers)
        }
        Ok(None) => {
            tts.forget_phrases(voice);
            tts.enqueue_phrases(voice);
            Resolved::NotReady
        }
        Err(e) => {
            tracing::warn!("voice phrase lookup: {e}");
            Resolved::NotReady
        }
    }
}

/// The answer to a question, for the account that asked. Runs on the
/// blocking pool.
pub fn resolve_answer_clip(state: &AppState, user: UserId, token: &str) -> Resolved {
    let Some(tts) = state.tts.as_deref() else {
        return Resolved::Off;
    };
    match tts.answer_clip(user, token) {
        AnswerLookup::Missing => Resolved::NotFound,
        AnswerLookup::Pending => Resolved::NotReady,
        AnswerLookup::Made { mime, bytes } => Resolved::Fresh { mime, bytes },
    }
}

/// The bytes as a response: sandboxed, never sniffed, tagged.
pub fn clip_response(row: &VoiceClipRow, blob: Blob, etag: &str, cache_control: &str) -> Response {
    audio_response(blob.bytes, &row.mime, cache_control, Some(etag))
}

/// A clip held in memory for one asking: the same headers, never cached.
pub fn fresh_response(mime: &str, bytes: Vec<u8>) -> Response {
    audio_response(bytes, mime, "no-store", None)
}

fn audio_response(
    body: impl IntoResponse,
    mime: &str,
    cache_control: &str,
    etag: Option<&str>,
) -> Response {
    let mut response = body.into_response();
    let h = response.headers_mut();
    let set = |h: &mut HeaderMap, name: header::HeaderName, value: String| {
        if let Ok(v) = value.parse() {
            h.insert(name, v);
        }
    };
    set(h, header::CONTENT_TYPE, mime.to_string());
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff".into());
    set(
        h,
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; sandbox".into(),
    );
    set(h, header::CACHE_CONTROL, cache_control.to_string());
    if let Some(etag) = etag {
        set(h, header::ETAG, etag.to_string());
    }
    set(h, header::VARY, "Cookie, Authorization".into());
    response
}

/// A 304 carrying the tag it matched.
pub fn not_modified(etag: &str, cache_control: &str) -> Response {
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    let h = response.headers_mut();
    if let Ok(v) = etag.parse() {
        h.insert(header::ETAG, v);
    }
    if let Ok(v) = cache_control.parse() {
        h.insert(header::CACHE_CONTROL, v);
    }
    response
}

/// The phrase set a voice offers, with the clips that exist. Missing
/// ones are queued. `url_for` turns a phrase key into the route the
/// surface serves it at.
#[derive(Debug, Serialize)]
pub struct PhraseSet {
    pub voice_id: String,
    pub model: Option<String>,
    pub phrases: serde_json::Map<String, serde_json::Value>,
}

pub fn phrase_set(
    state: &AppState,
    voice: &str,
    url_for: impl Fn(&str) -> String,
) -> Option<PhraseSet> {
    let tts = state.tts.as_deref()?;
    voices::find(voice)?;
    let model = tts.model_tag();
    let mut phrases = serde_json::Map::new();
    let mut missing = false;
    for (key, _) in PHRASES {
        let found = match &model {
            Some(model) => phrase_row(state, voice, model, key).ok().flatten(),
            None => None,
        };
        let value = match found {
            Some(row) => {
                let clip = VoiceClip {
                    duration_ms: row.duration_ms,
                    envelope: row.envelope,
                };
                serde_json::to_value(ClipDescriptor::new(url_for(key), &clip)).unwrap_or_default()
            }
            None => {
                missing = true;
                serde_json::Value::Null
            }
        };
        phrases.insert((*key).to_string(), value);
    }
    if missing && model.is_some() {
        tts.forget_phrases(voice);
        tts.enqueue_phrases(voice);
    }
    Some(PhraseSet {
        voice_id: voice.to_string(),
        model,
        phrases,
    })
}

#[derive(Debug, Serialize)]
pub struct VoiceListing {
    pub default: String,
    pub groups: Vec<VoiceGroup>,
}

#[derive(Debug, Serialize)]
pub struct VoiceGroup {
    pub lang: &'static str,
    pub label: &'static str,
    pub voices: Vec<VoiceEntry>,
}

#[derive(Debug, Serialize)]
pub struct VoiceEntry {
    pub id: &'static str,
    pub name: String,
    pub female: bool,
    /// The voice's sample line, once the warm-up has made it; null
    /// until then, when a client asks the phrase set and waits.
    pub sample: Option<ClipDescriptor>,
}

/// Every voice, grouped by language, with the account's default and
/// each voice's sample clip where one exists. `url_for` turns a voice
/// id into the route the surface serves its sample at. None without a
/// sidecar. Runs on the blocking pool: it reads the store once.
pub fn voice_listing(state: &AppState, url_for: impl Fn(&str) -> String) -> Option<VoiceListing> {
    let tts = state.tts.as_deref()?;
    // One query for every voice's sample; a row whose words are not the
    // voice's demo line any more is dropped, as `phrase_row` would.
    let mut samples: std::collections::HashMap<String, VoiceClipRow> = Default::default();
    if let Some(model) = tts.model_tag() {
        let rows = state
            .services
            .store()
            .find_phrase_clips(&model, "sample")
            .unwrap_or_else(|e| {
                tracing::warn!("voice samples lookup: {e}");
                Vec::new()
            });
        for row in rows {
            let wanted =
                phrase_text("sample", &row.voice).map(|text| clip_key(&model, &row.voice, &text));
            if wanted.as_deref() == Some(row.key.as_str()) {
                samples.insert(row.voice.clone(), row);
            } else {
                forget_stale(state, &row);
            }
        }
    }
    Some(VoiceListing {
        default: tts.default_voice.clone(),
        groups: voices::grouped()
            .into_iter()
            .map(|(lang, list)| VoiceGroup {
                lang: lang.code(),
                label: lang.label(),
                voices: list
                    .into_iter()
                    .map(|v| VoiceEntry {
                        id: v.id,
                        name: voices::display_name(v.id),
                        female: v.female,
                        sample: samples.remove(v.id).map(|row| {
                            let clip = VoiceClip {
                                duration_ms: row.duration_ms,
                                envelope: row.envelope,
                            };
                            ClipDescriptor::new(url_for(v.id), &clip)
                        }),
                    })
                    .collect(),
            })
            .collect(),
    })
}
