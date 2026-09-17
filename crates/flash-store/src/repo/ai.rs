//! Card generation: the sources a user hands over, the model's cached
//! readings of them, the jobs that turn a reading into drafts, the
//! drafts awaiting keep or discard, and the ledger of every model call.
//! Every read and write is ownership-scoped in SQL; the handful of
//! functions the runner and the scheduler call across all accounts are
//! named as such and listed as unscoped by design in the scan.
//!
//! Text columns that hold structured data (`figures`, `tags`,
//! `figure_media_ids`) are JSON: figures come back raw for the server
//! to parse, the two list columns are decoded here.

use flash_core::{AiDraftId, AiJobId, AiSourceId, DeckId, MediaId, NoteId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::notes::NoteType;
use crate::richtext::SanitizedHtml;
use crate::{exec_expect_row, Result, Store, StoreError};

use super::cards::require_deck;

macro_rules! str_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

str_enum!(
    /// What a source is; also the input kind a call is charged as.
    AiSourceKind {
        Text => "text",
        Pdf => "pdf",
        Image => "image",
        Youtube => "youtube",
        Video => "video",
        Audio => "audio",
    }
);

impl AiSourceKind {
    /// Whether the kind is consumed as playing time rather than pages
    /// or words: a video link is fetched and watched like a video.
    pub fn is_video(self) -> bool {
        matches!(self, AiSourceKind::Video | AiSourceKind::Youtube)
    }

    pub fn is_audio(self) -> bool {
        matches!(self, AiSourceKind::Audio)
    }

    /// A source the model consumes as a timeline rather than a page:
    /// these take longer, are metered in minutes, and are gated apart.
    pub fn is_media(self) -> bool {
        self.is_video() || self.is_audio()
    }

    /// Where the kind stands on the cost ladder, so a set of several
    /// kinds is charged as its dearest member.
    pub fn cost_rank(self) -> u8 {
        match self {
            AiSourceKind::Video => 5,
            AiSourceKind::Youtube => 4,
            AiSourceKind::Audio => 3,
            AiSourceKind::Pdf => 2,
            AiSourceKind::Image => 1,
            AiSourceKind::Text => 0,
        }
    }

    /// The dearest kind among `kinds`; None for an empty set.
    pub fn costliest(kinds: impl IntoIterator<Item = AiSourceKind>) -> Option<AiSourceKind> {
        kinds.into_iter().max_by_key(|k| k.cost_rank())
    }
}

str_enum!(
    /// A job's place in the pipeline. `queued` is waiting for the
    /// runner; the two middle states are its progress; the last two are
    /// final.
    AiJobState {
        Queued => "queued",
        Extracting => "extracting",
        Generating => "generating",
        Done => "done",
        Failed => "failed",
    }
);

impl AiJobState {
    /// Whether the runner still has work to do on a job in this state.
    pub fn is_active(self) -> bool {
        !matches!(self, AiJobState::Done | AiJobState::Failed)
    }
}

str_enum!(
    /// What a job asks of the model: a first set of drafts, the current
    /// drafts revised under an instruction, or more drafts beside them.
    AiJobOp {
        Generate => "generate",
        Revise => "revise",
        More => "more",
    }
);

str_enum!(
    /// Where a draft stands. `edited` is still awaiting the user's
    /// decision; the last two are what the user decided.
    AiDraftStatus {
        Pending => "pending",
        Edited => "edited",
        Kept => "kept",
        Discarded => "discarded",
    }
);

impl AiDraftStatus {
    /// Whether the draft still awaits keep or discard.
    pub fn is_open(self) -> bool {
        matches!(self, AiDraftStatus::Pending | AiDraftStatus::Edited)
    }
}

str_enum!(
    /// Which stage a model call served. The three generation steps are
    /// the ones a quota counts. `Turn` is one spoken turn of voice
    /// study, whatever the student meant by it; `Grade` and `Clarify`
    /// are what earlier voice rows were recorded as.
    AiCallStep {
        Extract => "extract",
        Generate => "generate",
        Revise => "revise",
        More => "more",
        Grade => "grade",
        Clarify => "clarify",
        Turn => "turn",
    }
);

impl AiCallStep {
    /// Whether a call of this step served voice study rather than a
    /// generation job.
    pub fn is_voice(self) -> bool {
        matches!(
            self,
            AiCallStep::Grade | AiCallStep::Clarify | AiCallStep::Turn
        )
    }

    /// Whether a successful call of this step is one generation for
    /// quota purposes.
    pub fn is_generation(self) -> bool {
        matches!(
            self,
            AiCallStep::Generate | AiCallStep::Revise | AiCallStep::More
        )
    }
}

str_enum!(
    /// Who wrote a turn of a review's thread.
    AiTurnRole {
        User => "user",
        Model => "model",
    }
);

fn parse_col<T>(what: &'static str, s: &str, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    parse(s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("bad {what}: {s}").into(),
        )
    })
}

// ---- sources ----

#[derive(Debug, Clone, PartialEq)]
pub struct AiSourceRow {
    pub id: AiSourceId,
    pub kind: AiSourceKind,
    pub sha256: String,
    pub mime: String,
    pub size: u64,
    pub filename: String,
    pub url: Option<String>,
    pub text: Option<String>,
    pub pages: Option<u32>,
    pub duration_s: Option<u32>,
    pub est_words: Option<u32>,
    pub image_count: Option<u32>,
    /// The type the file was uploaded as, when the stored bytes are a
    /// conversion of it (a slide deck that became a PDF).
    pub origin_mime: Option<String>,
    pub created_at: i64,
    pub last_used_at: i64,
}

/// A source as ingest records it: the blob is already in storage under
/// `sha256`, and the preflight facts are whatever ingest could compute.
#[derive(Debug, Clone)]
pub struct AiSourceNew<'a> {
    pub kind: AiSourceKind,
    pub sha256: &'a str,
    pub mime: &'a str,
    pub size: u64,
    pub filename: &'a str,
    pub url: Option<&'a str>,
    pub text: Option<&'a str>,
    pub pages: Option<u32>,
    pub duration_s: Option<u32>,
    pub est_words: Option<u32>,
    pub image_count: Option<u32>,
    pub origin_mime: Option<&'a str>,
    pub now_ms: i64,
}

const SOURCE_COLS: &str = "id, kind, sha256, mime, size, filename, url, text, pages, duration_s, \
                           est_words, image_count, created_at, last_used_at, origin_mime";

fn source_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiSourceRow> {
    source_row_at(r, 0)
}

/// `SOURCE_COLS` read from column `base` on, for a query that selects
/// something in front of them.
fn source_row_at(r: &rusqlite::Row<'_>, base: usize) -> rusqlite::Result<AiSourceRow> {
    let opt_u32 = |i: usize| -> rusqlite::Result<Option<u32>> {
        Ok(r.get::<_, Option<i64>>(base + i)?.map(|v| v.max(0) as u32))
    };
    Ok(AiSourceRow {
        id: AiSourceId(r.get(base)?),
        kind: parse_col("kind", &r.get::<_, String>(base + 1)?, AiSourceKind::parse)?,
        sha256: r.get(base + 2)?,
        mime: r.get(base + 3)?,
        size: r.get::<_, i64>(base + 4)?.max(0) as u64,
        filename: r.get(base + 5)?,
        url: r.get(base + 6)?,
        text: r.get(base + 7)?,
        pages: opt_u32(8)?,
        duration_s: opt_u32(9)?,
        est_words: opt_u32(10)?,
        image_count: opt_u32(11)?,
        created_at: r.get(base + 12)?,
        last_used_at: r.get(base + 13)?,
        origin_mime: r.get(base + 14)?,
    })
}

// ---- extractions ----

#[derive(Debug, Clone, PartialEq)]
pub struct AiExtractionRow {
    pub id: i64,
    pub source_id: AiSourceId,
    pub sha256: String,
    pub model: String,
    pub prompt_version: u32,
    pub text: String,
    /// The figure list as the model described it, JSON, uninterpreted
    /// here.
    pub figures: String,
    pub est_words: u32,
    pub created_at: i64,
    pub last_used_at: i64,
}

#[derive(Debug, Clone)]
pub struct AiExtractionNew<'a> {
    pub source_id: AiSourceId,
    pub sha256: &'a str,
    pub model: &'a str,
    pub prompt_version: u32,
    pub text: &'a str,
    pub figures: &'a str,
    pub est_words: u32,
    pub now_ms: i64,
}

const EXTRACTION_COLS: &str = "id, source_id, sha256, model, prompt_version, text, figures, \
                               est_words, created_at, last_used_at";

fn extraction_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiExtractionRow> {
    Ok(AiExtractionRow {
        id: r.get(0)?,
        source_id: AiSourceId(r.get(1)?),
        sha256: r.get(2)?,
        model: r.get(3)?,
        prompt_version: r.get::<_, i64>(4)?.max(0) as u32,
        text: r.get(5)?,
        figures: r.get(6)?,
        est_words: r.get::<_, i64>(7)?.max(0) as u32,
        created_at: r.get(8)?,
        last_used_at: r.get(9)?,
    })
}

// ---- jobs ----

#[derive(Debug, Clone, PartialEq)]
pub struct AiJobRow {
    pub id: AiJobId,
    /// The first member of the job's source set, kept for readers
    /// written before sets existed; `list_ai_job_source_ids` is the
    /// set. Kept even after the source is housekept away: the job then
    /// renders as expired.
    pub source_id: AiSourceId,
    /// None once the deck was deleted; the user picks another at keep.
    pub deck_id: Option<DeckId>,
    pub extraction_id: Option<i64>,
    pub op: AiJobOp,
    pub parent_job_id: Option<AiJobId>,
    pub instruction: String,
    pub state: AiJobState,
    /// 0–100, the runner's own estimate.
    pub progress: u8,
    pub error: Option<String>,
    /// How many times the runner was interrupted while working on it.
    pub attempts: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AiJobNew<'a> {
    /// The set the job reads, in order; at least one.
    pub source_ids: &'a [AiSourceId],
    pub deck_id: Option<DeckId>,
    /// A child job (revise, more) reuses its parent's extraction.
    pub extraction_id: Option<i64>,
    pub op: AiJobOp,
    pub parent_job_id: Option<AiJobId>,
    pub instruction: &'a str,
    pub now_ms: i64,
}

const JOB_COLS: &str = "id, source_id, deck_id, extraction_id, op, parent_job_id, instruction, \
                        state, progress, error, attempts, created_at, updated_at, finished_at";

fn job_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiJobRow> {
    Ok(AiJobRow {
        id: AiJobId(r.get(0)?),
        source_id: AiSourceId(r.get(1)?),
        deck_id: r.get::<_, Option<i64>>(2)?.map(DeckId),
        extraction_id: r.get(3)?,
        op: parse_col("op", &r.get::<_, String>(4)?, AiJobOp::parse)?,
        parent_job_id: r.get::<_, Option<i64>>(5)?.map(AiJobId),
        instruction: r.get(6)?,
        state: parse_col("state", &r.get::<_, String>(7)?, AiJobState::parse)?,
        progress: r.get::<_, i64>(8)?.clamp(0, 100) as u8,
        error: r.get(9)?,
        attempts: r.get::<_, i64>(10)?.max(0) as u32,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        finished_at: r.get(13)?,
    })
}

// ---- drafts ----

#[derive(Debug, Clone, PartialEq)]
pub struct AiDraftRow {
    pub id: AiDraftId,
    pub job_id: AiJobId,
    pub ord: u32,
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// The extraction's figure crops this draft embeds.
    pub figure_media_ids: Vec<MediaId>,
    pub status: AiDraftStatus,
    /// The note a keep created.
    pub note_id: Option<NoteId>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A draft as the validator hands it over: sanitized, with the figure
/// ids already checked against the extraction's own.
#[derive(Debug, Clone, PartialEq)]
pub struct AiDraftNew {
    pub note_type: NoteType,
    pub front_html: SanitizedHtml,
    pub back_html: SanitizedHtml,
    pub tags: Vec<String>,
    pub figure_media_ids: Vec<MediaId>,
}

/// How a job's drafts stand, for the review page's counts and the
/// "keep all" confirmation. `pending` includes edited drafts: both
/// still await a decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct AiDraftCounts {
    pub pending: u32,
    pub kept: u32,
    pub discarded: u32,
}

const DRAFT_COLS: &str = "id, job_id, ord, note_type, front_html, back_html, tags, \
                          figure_media_ids, status, note_id, created_at, updated_at";

fn ai_call_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiCallRow> {
    let u32_at = |i: usize| -> rusqlite::Result<u32> { Ok(r.get::<_, i64>(i)?.max(0) as u32) };
    Ok(AiCallRow {
        id: r.get(0)?,
        job_id: r.get::<_, Option<i64>>(1)?.map(AiJobId),
        step: parse_col("step", &r.get::<_, String>(2)?, AiCallStep::parse)?,
        model: r.get(3)?,
        model_version: r.get(4)?,
        response_id: r.get(5)?,
        input_kind: parse_col("input kind", &r.get::<_, String>(6)?, AiSourceKind::parse)?,
        input_tokens: u32_at(7)?,
        cached_tokens: u32_at(8)?,
        tool_tokens: u32_at(9)?,
        output_tokens: u32_at(10)?,
        thoughts_tokens: u32_at(11)?,
        media_seconds: u32_at(12)?,
        ms: u32_at(13)?,
        usage_json: r.get(14)?,
        ok: r.get::<_, i64>(15)? != 0,
        error: r.get(16)?,
        started_at: r.get(17)?,
    })
}

fn draft_row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiDraftRow> {
    let tags: Vec<String> = serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default();
    let figures: Vec<i64> = serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default();
    Ok(AiDraftRow {
        id: AiDraftId(r.get(0)?),
        job_id: AiJobId(r.get(1)?),
        ord: r.get::<_, i64>(2)?.max(0) as u32,
        note_type: parse_col("note_type", &r.get::<_, String>(3)?, NoteType::parse)?,
        front_html: r.get(4)?,
        back_html: r.get(5)?,
        tags,
        figure_media_ids: figures.into_iter().map(MediaId).collect(),
        status: parse_col("status", &r.get::<_, String>(8)?, AiDraftStatus::parse)?,
        note_id: r.get::<_, Option<i64>>(9)?.map(NoteId),
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

fn json_list<T: serde::Serialize>(items: &[T]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

// ---- turns ----

/// One turn of a review's thread, with the state of the pass it ran.
#[derive(Debug, Clone, PartialEq)]
pub struct AiTurnRow {
    pub id: i64,
    /// The root job: the review the thread belongs to.
    pub job_id: AiJobId,
    pub role: AiTurnRole,
    pub text: String,
    /// The pass that ran a person's turn or wrote the model's.
    pub child_job_id: Option<AiJobId>,
    /// That pass's state, so a client can show a failed one as such;
    /// None when the turn ran no job.
    pub child_state: Option<AiJobState>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AiTurnNew<'a> {
    pub job_id: AiJobId,
    pub role: AiTurnRole,
    pub text: &'a str,
    pub child_job_id: Option<AiJobId>,
    pub now_ms: i64,
}

// ---- calls ----

/// One model call for the ledger: the provider's own count of what it
/// read and wrote, and the exchange around them.
#[derive(Debug, Clone)]
pub struct AiCallNew<'a> {
    pub job_id: Option<AiJobId>,
    pub step: AiCallStep,
    /// The model as configured.
    pub model: &'a str,
    /// The version the provider says answered, when the reply names one.
    pub model_version: Option<&'a str>,
    /// The provider's id for the exchange, for tracing a line on its
    /// side.
    pub response_id: Option<&'a str>,
    pub input_kind: AiSourceKind,
    /// Prompt tokens, tool-use tokens included.
    pub input_tokens: u32,
    /// The share of the prompt the provider served from its cache.
    pub cached_tokens: u32,
    /// Tokens the provider's own tools read on the call's behalf.
    pub tool_tokens: u32,
    /// Reply tokens, thinking included.
    pub output_tokens: u32,
    /// The share of the reply that was thinking.
    pub thoughts_tokens: u32,
    /// Playing time the call consumed (video and audio inputs).
    pub media_seconds: u32,
    pub ms: u32,
    /// The provider's usage record, as it was sent.
    pub usage_json: Option<&'a str>,
    pub ok: bool,
    pub error: Option<&'a str>,
    pub started_at: i64,
}

impl Default for AiCallNew<'_> {
    /// A successful text generation call at the epoch that counted
    /// nothing: the shape a test fills in.
    fn default() -> Self {
        AiCallNew {
            job_id: None,
            step: AiCallStep::Generate,
            model: "",
            model_version: None,
            response_id: None,
            input_kind: AiSourceKind::Text,
            input_tokens: 0,
            cached_tokens: 0,
            tool_tokens: 0,
            output_tokens: 0,
            thoughts_tokens: 0,
            media_seconds: 0,
            ms: 0,
            usage_json: None,
            ok: true,
            error: None,
            started_at: 0,
        }
    }
}

/// A ledger row as the audit reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCallRow {
    pub id: i64,
    pub job_id: Option<AiJobId>,
    pub step: AiCallStep,
    pub model: String,
    pub model_version: Option<String>,
    pub response_id: Option<String>,
    pub input_kind: AiSourceKind,
    pub input_tokens: u32,
    pub cached_tokens: u32,
    pub tool_tokens: u32,
    pub output_tokens: u32,
    pub thoughts_tokens: u32,
    pub media_seconds: u32,
    pub ms: u32,
    pub usage_json: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub started_at: i64,
}

/// What a user has consumed since a moment: the numbers a quota
/// compares against its caps. Only successful calls count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct AiUsageSince {
    /// Successful generate, revise and more steps.
    pub generations: u32,
    /// Playing time of video and video-link inputs, in seconds.
    pub video_seconds: u32,
    /// Playing time of audio inputs, in seconds.
    pub audio_seconds: u32,
}

impl Store {
    // ---- sources ----

    pub fn create_ai_source(&self, user: UserId, new: &AiSourceNew<'_>) -> Result<AiSourceId> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO ai_sources (user_id, kind, sha256, mime, size, filename, url, text,
               pages, duration_s, est_words, image_count, created_at, last_used_at,
               origin_mime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14)",
            params![
                user.raw(),
                new.kind.as_str(),
                new.sha256,
                new.mime,
                new.size as i64,
                new.filename,
                new.url,
                new.text,
                new.pages.map(i64::from),
                new.duration_s.map(i64::from),
                new.est_words.map(i64::from),
                new.image_count.map(i64::from),
                new.now_ms,
                new.origin_mime,
            ],
        )?;
        Ok(AiSourceId(conn.last_insert_rowid()))
    }

    /// None when the id is not the user's, or when housekeeping has
    /// removed the source since the job that names it was made.
    pub fn get_ai_source(&self, user: UserId, id: AiSourceId) -> Result<Option<AiSourceRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {SOURCE_COLS} FROM ai_sources WHERE id = ?1 AND user_id = ?2"),
                params![id.0, user.raw()],
                source_row_from,
            )
            .optional()?)
    }

    /// Marks the source used now, which is what keeps housekeeping off it.
    pub fn touch_ai_source(&self, user: UserId, id: AiSourceId, now_ms: i64) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "ai source",
            "UPDATE ai_sources SET last_used_at = ?1 WHERE id = ?2 AND user_id = ?3",
            params![now_ms, id.0, user.raw()],
        )
    }

    /// How many of the user's sources no job has read yet: what a
    /// composer holds before Generate is pressed.
    pub fn count_unattached_ai_sources(&self, user: UserId) -> Result<u32> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM ai_sources s WHERE s.user_id = ?1
               AND NOT EXISTS (SELECT 1 FROM ai_job_sources js
                               WHERE js.source_id = s.id AND js.user_id = ?1)",
            [user.raw()],
            |r| r.get::<_, i64>(0).map(|n| n.max(0) as u32),
        )?)
    }

    /// The user's sources no job has read yet, newest first.
    pub fn list_unattached_ai_sources(&self, user: UserId, limit: u32) -> Result<Vec<AiSourceRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {SOURCE_COLS} FROM ai_sources s WHERE s.user_id = ?1
               AND NOT EXISTS (SELECT 1 FROM ai_job_sources js
                               WHERE js.source_id = s.id AND js.user_id = ?1)
             ORDER BY created_at DESC, id DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![user.raw(), i64::from(limit)], source_row_from)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Removes a source the user took back before generating from it.
    /// A source a job has read is refused as `Invalid`: the job's
    /// review still names it. Returns the blob hash when nothing else
    /// holds it any more, for the caller to remove from storage.
    pub fn delete_ai_source(&self, user: UserId, id: AiSourceId) -> Result<Option<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let sha: String = tx
            .query_row(
                "SELECT sha256 FROM ai_sources WHERE id = ?1 AND user_id = ?2",
                params![id.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StoreError::NotFound("ai source"))?;
        let in_use: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_job_sources WHERE source_id = ?1 AND user_id = ?2)",
            params![id.0, user.raw()],
            |r| r.get(0),
        )?;
        if in_use {
            return Err(StoreError::Invalid(
                "That source is part of a generation and stays with it.".into(),
            ));
        }
        tx.execute(
            "DELETE FROM ai_figures WHERE user_id = ?2 AND extraction_id IN
               (SELECT id FROM ai_extractions WHERE source_id = ?1 AND user_id = ?2)",
            params![id.0, user.raw()],
        )?;
        tx.execute(
            "DELETE FROM ai_extractions WHERE source_id = ?1 AND user_id = ?2",
            params![id.0, user.raw()],
        )?;
        tx.execute(
            "DELETE FROM ai_sources WHERE id = ?1 AND user_id = ?2",
            params![id.0, user.raw()],
        )?;
        let orphan = (self.blob_refs(&tx, &sha)? == 0).then_some(sha);
        tx.commit()?;
        Ok(orphan)
    }

    // ---- extractions ----

    /// The cache lookup: the user's own reading of this content by this
    /// model and prompt, if one exists.
    pub fn find_ai_extraction(
        &self,
        user: UserId,
        sha256: &str,
        model: &str,
        prompt_version: u32,
    ) -> Result<Option<AiExtractionRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {EXTRACTION_COLS} FROM ai_extractions
                     WHERE user_id = ?1 AND sha256 = ?2 AND model = ?3 AND prompt_version = ?4"
                ),
                params![user.raw(), sha256, model, i64::from(prompt_version)],
                extraction_row_from,
            )
            .optional()?)
    }

    /// Records a reading; the source must be the user's. A second
    /// reading of the same content by the same model and prompt is a
    /// constraint error. Look it up first.
    pub fn insert_ai_extraction(&self, user: UserId, new: &AiExtractionNew<'_>) -> Result<i64> {
        let conn = self.conn();
        require_source(&conn, user, new.source_id)?;
        conn.execute(
            "INSERT INTO ai_extractions (user_id, source_id, sha256, model, prompt_version,
               text, figures, est_words, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                user.raw(),
                new.source_id.0,
                new.sha256,
                new.model,
                i64::from(new.prompt_version),
                new.text,
                new.figures,
                i64::from(new.est_words),
                new.now_ms,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_ai_extraction(&self, user: UserId, id: i64) -> Result<Option<AiExtractionRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {EXTRACTION_COLS} FROM ai_extractions WHERE id = ?1 AND user_id = ?2"
                ),
                params![id, user.raw()],
                extraction_row_from,
            )
            .optional()?)
    }

    pub fn touch_ai_extraction(&self, user: UserId, id: i64, now_ms: i64) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "ai extraction",
            "UPDATE ai_extractions SET last_used_at = ?1 WHERE id = ?2 AND user_id = ?3",
            params![now_ms, id, user.raw()],
        )
    }

    /// Pins a figure crop (an ordinary media row of the user's) to the
    /// extraction that produced it, so the unlinked-media sweep leaves
    /// it alone. Both rows must be the user's.
    pub fn add_ai_figure(&self, user: UserId, extraction_id: i64, media: MediaId) -> Result<()> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO ai_figures (user_id, extraction_id, media_id)
             SELECT ?3, e.id, m.id FROM ai_extractions e, media m
             WHERE e.id = ?1 AND e.user_id = ?3 AND m.id = ?2 AND m.user_id = ?3",
            params![extraction_id, media.0, user.raw()],
        )?;
        // Zero rows is either "already pinned" or "not yours"; tell them
        // apart with the guard released: the lookups take it again.
        if n == 0 {
            if self.get_ai_extraction(user, extraction_id)?.is_none() {
                return Err(StoreError::NotFound("ai extraction"));
            }
            if self.get_media(user, media)?.is_none() {
                return Err(StoreError::NotFound("media"));
            }
        }
        Ok(())
    }

    /// The crops pinned to an extraction, ascending by media id.
    pub fn ai_figure_media_ids(&self, user: UserId, extraction_id: i64) -> Result<Vec<MediaId>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT media_id FROM ai_figures
             WHERE extraction_id = ?1 AND user_id = ?2 ORDER BY media_id",
        )?;
        let rows = stmt.query_map(params![extraction_id, user.raw()], |r| {
            Ok(MediaId(r.get(0)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- jobs ----

    /// Queues a job over a set of sources. Every source, the deck (when
    /// given), the extraction (when given) and the parent job (when
    /// given) must all be the user's; an empty set is `Invalid`.
    pub fn create_ai_job(&self, user: UserId, new: &AiJobNew<'_>) -> Result<AiJobId> {
        let Some(first) = new.source_ids.first().copied() else {
            return Err(StoreError::Invalid(
                "A generation reads at least one source.".into(),
            ));
        };
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for source in new.source_ids {
            require_source(&tx, user, *source)?;
        }
        if let Some(deck) = new.deck_id {
            require_deck(&tx, user, deck)?;
        }
        if let Some(extraction) = new.extraction_id {
            require_extraction(&tx, user, extraction)?;
        }
        if let Some(parent) = new.parent_job_id {
            require_job(&tx, user, parent)?;
        }
        tx.execute(
            "INSERT INTO ai_jobs (user_id, source_id, deck_id, extraction_id, op, parent_job_id,
               instruction, state, progress, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', 0, 0, ?8, ?8)",
            params![
                user.raw(),
                first.0,
                new.deck_id.map(|d| d.0),
                new.extraction_id,
                new.op.as_str(),
                new.parent_job_id.map(|j| j.0),
                new.instruction,
                new.now_ms,
            ],
        )?;
        let id = tx.last_insert_rowid();
        for (ord, source) in new.source_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO ai_job_sources (user_id, job_id, source_id, ord)
                 VALUES (?1, ?2, ?3, ?4)",
                params![user.raw(), id, source.0, ord as i64],
            )?;
        }
        tx.commit()?;
        Ok(AiJobId(id))
    }

    /// The ids of the set a job reads, in order, whether or not the
    /// rows still exist.
    pub fn list_ai_job_source_ids(&self, user: UserId, job: AiJobId) -> Result<Vec<AiSourceId>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT source_id FROM ai_job_sources WHERE job_id = ?1 AND user_id = ?2 ORDER BY ord",
        )?;
        let rows = stmt.query_map(params![job.0, user.raw()], |r| Ok(AiSourceId(r.get(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The set a job reads, in order; a member housekeeping has taken
    /// is `None` in its place, so the caller can say how many expired.
    pub fn list_ai_job_sources(
        &self,
        user: UserId,
        job: AiJobId,
    ) -> Result<Vec<(AiSourceId, Option<AiSourceRow>)>> {
        let conn = self.conn();
        let cols = SOURCE_COLS
            .split(", ")
            .map(|c| format!("s.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn.prepare(&format!(
            "SELECT js.source_id, {cols}
             FROM ai_job_sources js
             LEFT JOIN ai_sources s ON s.id = js.source_id AND s.user_id = js.user_id
             WHERE js.job_id = ?1 AND js.user_id = ?2 ORDER BY js.ord"
        ))?;
        let rows = stmt.query_map(params![job.0, user.raw()], |r| {
            let id = AiSourceId(r.get(0)?);
            let present: Option<i64> = r.get(1)?;
            let row = match present {
                Some(_) => Some(source_row_at(r, 1)?),
                None => None,
            };
            Ok((id, row))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_ai_job(&self, user: UserId, id: AiJobId) -> Result<Option<AiJobRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {JOB_COLS} FROM ai_jobs WHERE id = ?1 AND user_id = ?2"),
                params![id.0, user.raw()],
                job_row_from,
            )
            .optional()?)
    }

    /// The user's jobs, newest first.
    pub fn list_ai_jobs(&self, user: UserId, limit: u32) -> Result<Vec<AiJobRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {JOB_COLS} FROM ai_jobs WHERE user_id = ?1
             ORDER BY created_at DESC, id DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![user.raw(), i64::from(limit)], job_row_from)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The job the runner is on or about to take for this user, if any
    /// (the newest, should there be several queued).
    pub fn active_ai_job(&self, user: UserId) -> Result<Option<AiJobRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {JOB_COLS} FROM ai_jobs
                     WHERE user_id = ?1 AND state IN ('queued', 'extracting', 'generating')
                     ORDER BY created_at DESC, id DESC LIMIT 1"
                ),
                [user.raw()],
                job_row_from,
            )
            .optional()?)
    }

    /// Moves a job along. `error` is the user-safe sentence a failed job
    /// shows; a final state also stamps `finished_at`.
    pub fn set_ai_job_state(
        &self,
        user: UserId,
        id: AiJobId,
        state: AiJobState,
        progress: u8,
        error: Option<&str>,
        now_ms: i64,
    ) -> Result<()> {
        let finished = (!state.is_active()).then_some(now_ms);
        exec_expect_row(
            &self.conn(),
            "ai job",
            "UPDATE ai_jobs SET state = ?1, progress = ?2, error = ?3, updated_at = ?4,
               finished_at = COALESCE(?5, finished_at)
             WHERE id = ?6 AND user_id = ?7",
            params![
                state.as_str(),
                i64::from(progress.min(100)),
                error,
                now_ms,
                finished,
                id.0,
                user.raw()
            ],
        )
    }

    /// Records which reading the job worked from (the extraction must be
    /// the user's).
    pub fn set_ai_job_extraction(
        &self,
        user: UserId,
        id: AiJobId,
        extraction_id: i64,
    ) -> Result<()> {
        let conn = self.conn();
        require_extraction(&conn, user, extraction_id)?;
        exec_expect_row(
            &conn,
            "ai job",
            "UPDATE ai_jobs SET extraction_id = ?1 WHERE id = ?2 AND user_id = ?3",
            params![extraction_id, id.0, user.raw()],
        )
    }

    // ---- drafts ----

    /// Replaces a job's open drafts with `drafts`. With `keep_kept`, the
    /// rows the user already decided on (kept, discarded) stay and the
    /// new rows are numbered after them; without it every row of the job
    /// goes first. Returns the new rows' ids in order.
    pub fn replace_ai_drafts(
        &self,
        user: UserId,
        job: AiJobId,
        drafts: &[AiDraftNew],
        keep_kept: bool,
        now_ms: i64,
    ) -> Result<Vec<AiDraftId>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        require_job(&tx, user, job)?;
        if keep_kept {
            tx.execute(
                "DELETE FROM ai_drafts WHERE job_id = ?1 AND user_id = ?2
                   AND status IN ('pending', 'edited')",
                params![job.0, user.raw()],
            )?;
        } else {
            tx.execute(
                "DELETE FROM ai_drafts WHERE job_id = ?1 AND user_id = ?2",
                params![job.0, user.raw()],
            )?;
        }
        let ids = insert_drafts(&tx, user, job, drafts, now_ms)?;
        tx.commit()?;
        Ok(ids)
    }

    /// Adds drafts after a job's existing ones ("ask for more"). Returns
    /// the new rows' ids in order.
    pub fn append_ai_drafts(
        &self,
        user: UserId,
        job: AiJobId,
        drafts: &[AiDraftNew],
        now_ms: i64,
    ) -> Result<Vec<AiDraftId>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        require_job(&tx, user, job)?;
        let ids = insert_drafts(&tx, user, job, drafts, now_ms)?;
        tx.commit()?;
        Ok(ids)
    }

    /// A job's drafts, every status, ascending by ord.
    pub fn list_ai_drafts(&self, user: UserId, job: AiJobId) -> Result<Vec<AiDraftRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {DRAFT_COLS} FROM ai_drafts WHERE job_id = ?1 AND user_id = ?2 ORDER BY ord"
        ))?;
        let rows = stmt.query_map(params![job.0, user.raw()], draft_row_from)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_ai_draft(&self, user: UserId, id: AiDraftId) -> Result<Option<AiDraftRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {DRAFT_COLS} FROM ai_drafts WHERE id = ?1 AND user_id = ?2"),
                params![id.0, user.raw()],
                draft_row_from,
            )
            .optional()?)
    }

    /// The user's edit of a draft's content. A pending draft becomes
    /// `edited`; any other status is left as it is.
    #[allow(clippy::too_many_arguments)]
    pub fn update_ai_draft_fields(
        &self,
        user: UserId,
        id: AiDraftId,
        note_type: NoteType,
        front_html: &SanitizedHtml,
        back_html: &SanitizedHtml,
        tags: &[String],
        now_ms: i64,
    ) -> Result<()> {
        super::cards::check_tags(tags)?;
        exec_expect_row(
            &self.conn(),
            "ai draft",
            "UPDATE ai_drafts SET note_type = ?1, front_html = ?2, back_html = ?3, tags = ?4,
               status = CASE status WHEN 'pending' THEN 'edited' ELSE status END,
               updated_at = ?5
             WHERE id = ?6 AND user_id = ?7",
            params![
                note_type.as_str(),
                front_html,
                back_html,
                json_list(tags),
                now_ms,
                id.0,
                user.raw()
            ],
        )
    }

    /// The user's decision on a draft; a keep records the note it made
    /// (which must be the user's).
    pub fn set_ai_draft_status(
        &self,
        user: UserId,
        id: AiDraftId,
        status: AiDraftStatus,
        note_id: Option<NoteId>,
        now_ms: i64,
    ) -> Result<()> {
        let conn = self.conn();
        if let Some(note) = note_id {
            let owned: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM notes WHERE id = ?1 AND user_id = ?2)",
                params![note.0, user.raw()],
                |r| r.get(0),
            )?;
            if !owned {
                return Err(StoreError::NotFound("note"));
            }
        }
        exec_expect_row(
            &conn,
            "ai draft",
            "UPDATE ai_drafts SET status = ?1, note_id = ?2, updated_at = ?3
             WHERE id = ?4 AND user_id = ?5",
            params![
                status.as_str(),
                note_id.map(|n| n.0),
                now_ms,
                id.0,
                user.raw()
            ],
        )
    }

    pub fn count_ai_drafts(&self, user: UserId, job: AiJobId) -> Result<AiDraftCounts> {
        Ok(self.conn().query_row(
            "SELECT
               COALESCE(SUM(status IN ('pending', 'edited')), 0),
               COALESCE(SUM(status = 'kept'), 0),
               COALESCE(SUM(status = 'discarded'), 0)
             FROM ai_drafts WHERE job_id = ?1 AND user_id = ?2",
            params![job.0, user.raw()],
            |r| {
                Ok(AiDraftCounts {
                    pending: r.get::<_, i64>(0)? as u32,
                    kept: r.get::<_, i64>(1)? as u32,
                    discarded: r.get::<_, i64>(2)? as u32,
                })
            },
        )?)
    }

    // ---- calls ----

    /// One ledger row per model call, successful or not. The job, when
    /// given, must be the user's.
    pub fn insert_ai_call(&self, user: UserId, call: &AiCallNew<'_>) -> Result<i64> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction()?;
        if let Some(job) = call.job_id {
            require_job(&tx, user, job)?;
        }
        tx.execute(
            "INSERT INTO ai_calls (user_id, job_id, step, model, model_version, response_id,
               input_kind, input_tokens, cached_tokens, tool_tokens, output_tokens,
               thoughts_tokens, media_seconds, ms, usage_json, ok, error, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
               ?18)",
            params![
                user.raw(),
                call.job_id.map(|j| j.0),
                call.step.as_str(),
                call.model,
                call.model_version,
                call.response_id,
                call.input_kind.as_str(),
                i64::from(call.input_tokens),
                i64::from(call.cached_tokens),
                i64::from(call.tool_tokens),
                i64::from(call.output_tokens),
                i64::from(call.thoughts_tokens),
                i64::from(call.media_seconds),
                i64::from(call.ms),
                call.usage_json,
                call.ok as i64,
                call.error,
                call.started_at,
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// What the user has consumed since `since_ms`, from the ledger:
    /// successful generation steps, and the playing time of successful
    /// calls over video (a video link counts as video) and audio.
    pub fn ai_usage_since(&self, user: UserId, since_ms: i64) -> Result<AiUsageSince> {
        Ok(self.conn().query_row(
            "SELECT
               COALESCE(SUM(ok = 1 AND step IN ('generate', 'revise', 'more')), 0),
               COALESCE(SUM(CASE WHEN ok = 1 AND input_kind IN ('video', 'youtube')
                                 THEN media_seconds ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN ok = 1 AND input_kind = 'audio'
                                 THEN media_seconds ELSE 0 END), 0)
             FROM ai_calls WHERE user_id = ?1 AND started_at >= ?2",
            params![user.raw(), since_ms],
            |r| {
                Ok(AiUsageSince {
                    generations: r.get::<_, i64>(0)?.max(0) as u32,
                    video_seconds: r.get::<_, i64>(1)?.max(0) as u32,
                    audio_seconds: r.get::<_, i64>(2)?.max(0) as u32,
                })
            },
        )?)
    }

    /// The user's calls that started in `[from_ms, to_ms)`, newest
    /// first, `limit` at a time from `offset`.
    pub fn ai_calls_for_user(
        &self,
        user: UserId,
        from_ms: i64,
        to_ms: i64,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<AiCallRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, job_id, step, model, model_version, response_id, input_kind,
               input_tokens, cached_tokens, tool_tokens, output_tokens, thoughts_tokens,
               media_seconds, ms, usage_json, ok, error, started_at
             FROM ai_calls
             WHERE user_id = ?1 AND started_at >= ?2 AND started_at < ?3
             ORDER BY started_at DESC, id DESC LIMIT ?4 OFFSET ?5",
        )?;
        let rows = stmt.query_map(
            params![
                user.raw(),
                from_ms,
                to_ms,
                i64::from(limit),
                i64::from(offset)
            ],
            ai_call_row,
        )?;
        rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
    }

    /// How many of the user's calls started in `[from_ms, to_ms)`.
    pub fn ai_call_count_for_user(&self, user: UserId, from_ms: i64, to_ms: i64) -> Result<u32> {
        Ok(self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM ai_calls
                 WHERE user_id = ?1 AND started_at >= ?2 AND started_at < ?3",
                params![user.raw(), from_ms, to_ms],
                |r| r.get::<_, i64>(0),
            )?
            .max(0) as u32)
    }

    // ---- turns ----

    /// Appends a turn to a review's thread. The root and, when given,
    /// the child must be the user's.
    pub fn add_ai_turn(&self, user: UserId, new: &AiTurnNew<'_>) -> Result<i64> {
        let conn = self.conn();
        require_job(&conn, user, new.job_id)?;
        if let Some(child) = new.child_job_id {
            require_job(&conn, user, child)?;
        }
        conn.execute(
            "INSERT INTO ai_turns (user_id, job_id, role, text, child_job_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                user.raw(),
                new.job_id.0,
                new.role.as_str(),
                new.text,
                new.child_job_id.map(|j| j.0),
                new.now_ms,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// The thread of a review, oldest first, each turn with the state
    /// of the pass it ran.
    pub fn list_ai_turns(&self, user: UserId, job: AiJobId) -> Result<Vec<AiTurnRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.job_id, t.role, t.text, t.child_job_id, t.created_at, j.state
             FROM ai_turns t
             LEFT JOIN ai_jobs j ON j.id = t.child_job_id AND j.user_id = t.user_id
             WHERE t.job_id = ?1 AND t.user_id = ?2
             ORDER BY t.id",
        )?;
        let rows = stmt.query_map(params![job.0, user.raw()], |r| {
            let child_state = match r.get::<_, Option<String>>(6)? {
                Some(s) => Some(parse_col("state", &s, AiJobState::parse)?),
                None => None,
            };
            Ok(AiTurnRow {
                id: r.get(0)?,
                job_id: AiJobId(r.get(1)?),
                role: parse_col("role", &r.get::<_, String>(2)?, AiTurnRole::parse)?,
                text: r.get(3)?,
                child_job_id: r.get::<_, Option<i64>>(4)?.map(AiJobId),
                child_state,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- across every account: the runner and the scheduler ----

    /// Unscoped by design: the runner's next piece of work, whoever
    /// queued it (the oldest queued job).
    pub fn next_queued_ai_job(&self) -> Result<Option<AiJobRow>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {JOB_COLS} FROM ai_jobs WHERE state = 'queued'
                     ORDER BY created_at, id LIMIT 1"
                ),
                [],
                job_row_from,
            )
            .optional()?)
    }

    /// Unscoped by design: the queue's head, oldest first, up to `limit`
    /// jobs, each with the account that queued it, since every step the
    /// runner takes from here on is scoped to that account. The runner
    /// walks the list so one account whose job cannot start yet (its
    /// heavy permit is held by an import) does not hold up the accounts
    /// behind it.
    pub fn queued_ai_jobs(&self, limit: u32) -> Result<Vec<(UserId, AiJobRow)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {JOB_COLS}, user_id FROM ai_jobs WHERE state = 'queued'
             ORDER BY created_at, id LIMIT ?1"
        ))?;
        let rows = stmt.query_map([i64::from(limit)], |r| {
            Ok((UserId::from_db(r.get(14)?), job_row_from(r)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Unscoped by design: at boot, every job the previous process was
    /// working on goes back to the queue with one more attempt on it;
    /// a job that has now been interrupted twice fails instead, with
    /// that as its error. Returns how many jobs were touched.
    pub fn requeue_interrupted_ai_jobs(&self, now_ms: i64) -> Result<u32> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let failed = tx.execute(
            "UPDATE ai_jobs SET state = 'failed', attempts = attempts + 1, progress = 0,
               error = 'interrupted twice', updated_at = ?1, finished_at = ?1
             WHERE state IN ('extracting', 'generating') AND attempts + 1 >= 2",
            [now_ms],
        )?;
        let requeued = tx.execute(
            "UPDATE ai_jobs SET state = 'queued', attempts = attempts + 1, progress = 0,
               updated_at = ?1
             WHERE state IN ('extracting', 'generating')",
            [now_ms],
        )?;
        tx.commit()?;
        Ok((failed + requeued) as u32)
    }

    /// Unscoped by design: housekeeping over every account. Sources last
    /// used before `before_ms` whose readings were not used since either
    /// go, with their readings and the pins on their figure crops
    /// (the crops themselves are media rows the unlinked-media sweep
    /// takes next); so do sources no job ever read that were last used
    /// before `unattached_before_ms`, the shorter leash a composer's
    /// leftovers are on. A member of a job the runner still has to do
    /// is never taken, and neither is a member of a set whose reading
    /// was used since the cutoff. Returns the blob hashes nothing names
    /// any more, for the caller to remove from storage. Jobs keep their
    /// source ids and render as expired.
    pub fn sweep_stale_ai_sources(
        &self,
        before_ms: i64,
        unattached_before_ms: i64,
    ) -> Result<Vec<String>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let stale: Vec<(i64, i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT s.id, s.user_id, s.sha256 FROM ai_sources s
                 WHERE NOT EXISTS (SELECT 1 FROM ai_job_sources js
                                   JOIN ai_jobs j ON j.id = js.job_id
                                   WHERE js.source_id = s.id
                                     AND j.state IN ('queued', 'extracting', 'generating'))
                   AND NOT EXISTS (SELECT 1 FROM ai_extractions e
                                   WHERE e.source_id = s.id AND e.last_used_at >= ?1)
                   AND NOT EXISTS (SELECT 1 FROM ai_job_sources js
                                   JOIN ai_jobs j ON j.id = js.job_id
                                   JOIN ai_extractions e ON e.id = j.extraction_id
                                   WHERE js.source_id = s.id AND e.last_used_at >= ?1)
                   AND (s.last_used_at < ?1
                        OR (s.last_used_at < ?2
                            AND NOT EXISTS (SELECT 1 FROM ai_job_sources js
                                            WHERE js.source_id = s.id)))",
            )?;
            let rows = stmt.query_map(params![before_ms, unattached_before_ms], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for (id, user_id, _) in &stale {
            tx.execute(
                "DELETE FROM ai_figures WHERE user_id = ?2 AND extraction_id IN
                   (SELECT id FROM ai_extractions WHERE source_id = ?1 AND user_id = ?2)",
                params![id, user_id],
            )?;
            tx.execute(
                "DELETE FROM ai_extractions WHERE source_id = ?1 AND user_id = ?2",
                params![id, user_id],
            )?;
            tx.execute(
                "DELETE FROM ai_sources WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
            )?;
        }
        let mut orphans = Vec::new();
        for (_, _, sha) in stale {
            if self.blob_refs(&tx, &sha)? == 0 && !orphans.contains(&sha) {
                orphans.push(sha);
            }
        }
        tx.commit()?;
        Ok(orphans)
    }

    // ---- lineage ----

    /// The reading a note's cards were generated from: note → job →
    /// extraction, all the user's, in one query. None for a note that
    /// was not generated, or whose reading has since been housekept.
    pub fn ai_extraction_for_note(
        &self,
        user: UserId,
        note: NoteId,
    ) -> Result<Option<AiExtractionRow>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT e.id, e.source_id, e.sha256, e.model, e.prompt_version, e.text,
                        e.figures, e.est_words, e.created_at, e.last_used_at
                 FROM notes n
                 JOIN ai_jobs j ON j.id = n.ai_job_id AND j.user_id = ?2
                 JOIN ai_extractions e ON e.id = j.extraction_id AND e.user_id = ?2
                 WHERE n.id = ?1 AND n.user_id = ?2 AND n.deleted_at IS NULL",
                params![note.0, user.raw()],
                extraction_row_from,
            )
            .optional()?)
    }
}

/// Writes `drafts` for `job` after its highest existing ord.
fn insert_drafts(
    tx: &Connection,
    user: UserId,
    job: AiJobId,
    drafts: &[AiDraftNew],
    now: i64,
) -> Result<Vec<AiDraftId>> {
    let next: i64 = tx.query_row(
        "SELECT COALESCE(MAX(ord), -1) + 1 FROM ai_drafts WHERE job_id = ?1 AND user_id = ?2",
        params![job.0, user.raw()],
        |r| r.get(0),
    )?;
    let mut ids = Vec::with_capacity(drafts.len());
    for (i, d) in drafts.iter().enumerate() {
        super::cards::check_tags(&d.tags)?;
        let figures: Vec<i64> = d.figure_media_ids.iter().map(|m| m.0).collect();
        tx.execute(
            "INSERT INTO ai_drafts (user_id, job_id, ord, note_type, front_html, back_html,
               tags, figure_media_ids, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', ?9, ?9)",
            params![
                user.raw(),
                job.0,
                next + i as i64,
                d.note_type.as_str(),
                d.front_html,
                d.back_html,
                json_list(&d.tags),
                json_list(&figures),
                now,
            ],
        )?;
        ids.push(AiDraftId(tx.last_insert_rowid()));
    }
    Ok(ids)
}

fn require_source(conn: &Connection, user: UserId, source: AiSourceId) -> Result<()> {
    let owned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ai_sources WHERE id = ?1 AND user_id = ?2)",
        params![source.0, user.raw()],
        |r| r.get(0),
    )?;
    if !owned {
        return Err(StoreError::NotFound("ai source"));
    }
    Ok(())
}

fn require_extraction(conn: &Connection, user: UserId, extraction: i64) -> Result<()> {
    let owned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ai_extractions WHERE id = ?1 AND user_id = ?2)",
        params![extraction, user.raw()],
        |r| r.get(0),
    )?;
    if !owned {
        return Err(StoreError::NotFound("ai extraction"));
    }
    Ok(())
}

fn require_job(conn: &Connection, user: UserId, job: AiJobId) -> Result<()> {
    let owned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ai_jobs WHERE id = ?1 AND user_id = ?2)",
        params![job.0, user.raw()],
        |r| r.get(0),
    )?;
    if !owned {
        return Err(StoreError::NotFound("ai job"));
    }
    Ok(())
}
