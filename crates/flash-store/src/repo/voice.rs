//! Natural voice: the account's voice settings, and the synthesized
//! clips with the refs that make them reachable. The clip bytes live
//! in the media store under the clip's key; this repo owns the rows.
//! Clips are shared across accounts on purpose (the same words in the
//! same voice are the same audio), so `find_voice_clip` and the sweep
//! are cross-user by design; everything that touches a card is scoped.

use flash_core::settings::{SPEECH_RATE_MAX, SPEECH_RATE_MIN};
use flash_core::{CardId, UserId, VoiceApply, VoiceSettings, VoiceSettingsPatch};
use rusqlite::{params, OptionalExtension};

use crate::{exec_expect_row, Result, Store, StoreError};

/// A card side's clip, or one of the fixed phrases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceClipKind {
    Card,
    Phrase,
}

impl VoiceClipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Phrase => "phrase",
        }
    }

    fn from_db(s: &str) -> Self {
        if s == "phrase" {
            Self::Phrase
        } else {
            Self::Card
        }
    }
}

/// Which side of a card a clip reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceSide {
    Front,
    Back,
}

impl VoiceSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "front" => Some(Self::Front),
            "back" => Some(Self::Back),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceClipRow {
    pub id: i64,
    pub key: String,
    pub kind: VoiceClipKind,
    pub voice: String,
    pub model: String,
    pub phrase: Option<String>,
    pub mime: String,
    pub size: u64,
    pub duration_ms: u32,
    pub envelope: Vec<u8>,
}

/// A clip to record once its bytes are in the store.
#[derive(Debug, Clone)]
pub struct VoiceClipNew<'a> {
    pub key: &'a str,
    pub kind: VoiceClipKind,
    pub voice: &'a str,
    pub model: &'a str,
    pub phrase: Option<&'a str>,
    pub mime: &'a str,
    pub size: u64,
    pub duration_ms: u32,
    pub envelope: &'a [u8],
    pub now_ms: i64,
}

const CLIP_COLS: &str = "id, key, kind, voice, model, phrase, mime, size, duration_ms, envelope";

fn clip_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<VoiceClipRow> {
    Ok(VoiceClipRow {
        id: r.get(0)?,
        key: r.get(1)?,
        kind: VoiceClipKind::from_db(&r.get::<_, String>(2)?),
        voice: r.get(3)?,
        model: r.get(4)?,
        phrase: r.get(5)?,
        mime: r.get(6)?,
        size: r.get::<_, i64>(7)?.max(0) as u64,
        duration_ms: r.get::<_, i64>(8)?.clamp(0, u32::MAX as i64) as u32,
        envelope: r.get(9)?,
    })
}

impl Store {
    // ---- settings ----

    pub fn get_voice_settings(&self, user: UserId) -> Result<VoiceSettings> {
        self.conn()
            .query_row(
                "SELECT voice_id, speech_rate, voice_apply, sound_cues
                 FROM user_settings WHERE user_id = ?1",
                [user.raw()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, f64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(StoreError::NotFound("user_settings"))
            .and_then(|(voice_id, rate, apply, cues)| {
                Ok(VoiceSettings {
                    voice_id,
                    speech_rate: (rate as f32).clamp(SPEECH_RATE_MIN, SPEECH_RATE_MAX),
                    apply: VoiceApply::from_str(&apply)
                        .ok_or_else(|| StoreError::Invalid(format!("bad voice_apply: {apply}")))?,
                    sound_cues: cues != 0,
                })
            })
    }

    /// `None` fields stay as they are. Validation is the caller's job.
    pub fn set_voice_settings(&self, user: UserId, patch: &VoiceSettingsPatch) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "user_settings",
            "UPDATE user_settings SET
               voice_id = COALESCE(?1, voice_id),
               speech_rate = COALESCE(?2, speech_rate),
               voice_apply = COALESCE(?3, voice_apply),
               sound_cues = COALESCE(?4, sound_cues)
             WHERE user_id = ?5",
            params![
                patch.voice_id.as_deref(),
                patch.speech_rate.map(|r| r as f64),
                patch.apply.map(VoiceApply::as_str),
                patch.sound_cues.map(|c| c as i64),
                user.raw()
            ],
        )
    }

    // ---- clips ----

    /// The clip for a content key, whoever first made it. Cross-user by
    /// design: identical words in one voice are one recording, and the
    /// caller already holds the words.
    pub fn find_voice_clip(&self, key: &str) -> Result<Option<VoiceClipRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CLIP_COLS} FROM voice_clips WHERE key = ?1"),
                [key],
                clip_from,
            )
            .optional()?)
    }

    /// The clips recorded as one of the fixed phrases in a voice, for the
    /// model that made them. Usually one; more than one when the phrase's
    /// words changed and the old recording is still here. The caller
    /// knows the words and picks by key.
    pub fn find_phrase_clips_of(
        &self,
        voice: &str,
        model: &str,
        phrase: &str,
    ) -> Result<Vec<VoiceClipRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CLIP_COLS} FROM voice_clips
             WHERE voice = ?1 AND model = ?2 AND phrase = ?3"
        ))?;
        let rows = stmt.query_map(params![voice, model, phrase], clip_from)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// One of the fixed phrases in every voice that has it, for the
    /// model that made it: the picker's samples in one query.
    pub fn find_phrase_clips(&self, model: &str, phrase: &str) -> Result<Vec<VoiceClipRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {CLIP_COLS} FROM voice_clips
             WHERE model = ?1 AND phrase = ?2"
        ))?;
        let rows = stmt.query_map(params![model, phrase], clip_from)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Records a clip; a second insert of the same key is the first
    /// one's id. The bytes are already in the store.
    pub fn insert_voice_clip(&self, clip: &VoiceClipNew<'_>) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO voice_clips
               (key, kind, voice, model, phrase, mime, size, duration_ms, envelope, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (key) DO NOTHING",
            params![
                clip.key,
                clip.kind.as_str(),
                clip.voice,
                clip.model,
                clip.phrase,
                clip.mime,
                clip.size as i64,
                clip.duration_ms as i64,
                clip.envelope,
                clip.now_ms
            ],
        )?;
        Ok(conn.query_row(
            "SELECT id FROM voice_clips WHERE key = ?1",
            [clip.key],
            |r| r.get(0),
        )?)
    }

    /// Notes that the user's card side resolved to a clip, now; the ref's
    /// id comes back as the clip's tag for that user. The card must be
    /// the user's, live or not (a ref on a card just deleted is swept).
    pub fn touch_voice_clip_ref(
        &self,
        user: UserId,
        card: CardId,
        side: VoiceSide,
        clip_id: i64,
        now_ms: i64,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO voice_clip_refs (user_id, card_id, side, clip_id, created_at, last_used_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?5
             WHERE EXISTS (SELECT 1 FROM cards WHERE id = ?2 AND user_id = ?1)
             ON CONFLICT (user_id, card_id, side, clip_id)
               DO UPDATE SET last_used_at = excluded.last_used_at",
            params![user.raw(), card.0, side.as_str(), clip_id, now_ms],
        )?;
        conn.query_row(
            "SELECT id FROM voice_clip_refs
             WHERE user_id = ?1 AND card_id = ?2 AND side = ?3 AND clip_id = ?4",
            params![user.raw(), card.0, side.as_str(), clip_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(StoreError::NotFound("card"))
    }

    /// Housekeeping over every account: refs whose card is gone or
    /// deleted, and refs unused since `unused_before_ms`, are dropped;
    /// then card clips nobody references, made before
    /// `orphan_before_ms`, are dropped too and their keys returned for
    /// the caller to remove from the store. Phrase clips never go.
    pub fn sweep_voice_clips(
        &self,
        unused_before_ms: i64,
        orphan_before_ms: i64,
    ) -> Result<Vec<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM voice_clip_refs
             WHERE last_used_at < ?1
                OR NOT EXISTS (SELECT 1 FROM cards c
                               WHERE c.id = voice_clip_refs.card_id
                                 AND c.user_id = voice_clip_refs.user_id
                                 AND c.deleted_at IS NULL)",
            [unused_before_ms],
        )?;
        let orphans: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT key FROM voice_clips v
                 WHERE v.kind = 'card' AND v.created_at < ?1
                   AND NOT EXISTS (SELECT 1 FROM voice_clip_refs r WHERE r.clip_id = v.id)",
            )?;
            let rows = stmt.query_map([orphan_before_ms], |r| r.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for key in &orphans {
            tx.execute("DELETE FROM voice_clips WHERE key = ?1", [key])?;
        }
        tx.commit()?;
        Ok(orphans)
    }

    /// Drops a clip whose bytes turned out to be missing from the store,
    /// so it is made again. Refs go with it.
    pub fn forget_voice_clip(&self, key: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM voice_clip_refs
             WHERE clip_id IN (SELECT id FROM voice_clips WHERE key = ?1)",
            [key],
        )?;
        conn.execute("DELETE FROM voice_clips WHERE key = ?1", [key])?;
        Ok(())
    }

    /// How many clips exist, for the boot log and the admin's view.
    pub fn voice_clip_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM voice_clips", [], |r| r.get(0))?;
        Ok(n.max(0) as u64)
    }
}
