//! Notes: the editable unit behind sibling cards. A note stores the
//! editor's source fields; its cards are (re)generated from them by
//! `crate::notes::generate_cards` and written here in one transaction,
//! keyed on `ord` so an edit updates existing cards in place (scheduling
//! state survives), inserts newly appearing slots, and soft-deletes slots
//! that vanished.
//!
//! An update that changes the note's text first keeps the text it
//! replaces as a revision (`note_revisions`), in the same transaction,
//! so any edit can be undone with `restore_note_revision`.

use std::collections::{BTreeMap, BTreeSet};

use flash_core::speech::{self, SpeechBlock};
use flash_core::{CardId, DeckId, NoteId, RevisionId, UserId};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::notes::{card_speech, generate_cards, GeneratedCard, NoteType};
use crate::repo::ids::{next_id, IdTable};
use crate::richtext::{self, SanitizedHtml};
use crate::{Result, Store, StoreError};

use super::cards::{attach_tag, check_tags};
use super::decks::require_deck;

/// `speech_by` for blocks the assistant that wrote the card gave.
pub const SPEECH_BY_ASSISTANT: &str = "assistant";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardSide {
    Front,
    Back,
}

impl CardSide {
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

/// How many revisions a note keeps, newest first; older ones are dropped
/// as new ones are written.
pub const REVISIONS_KEPT: usize = 20;

/// How long a revision is kept at most, in ms: ninety days. Pruned when
/// the note is next edited.
pub const REVISION_MAX_AGE_MS: i64 = 90 * 24 * 60 * 60 * 1000;

/// Who made the edit that replaced a revision's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionSource {
    /// The web editor.
    Web,
    /// An assistant, through the MCP tools.
    Mcp,
    /// The JSON API (the app's editor).
    Api,
    /// An edit a downstream extension applied on the user's behalf.
    Sync,
    /// A restore of an earlier revision.
    Restore,
}

impl RevisionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionSource::Web => "web",
            RevisionSource::Mcp => "mcp",
            RevisionSource::Api => "api",
            RevisionSource::Sync => "sync",
            RevisionSource::Restore => "restore",
        }
    }

    pub fn parse(s: &str) -> Option<RevisionSource> {
        Some(match s {
            "web" => RevisionSource::Web,
            "mcp" => RevisionSource::Mcp,
            "api" => RevisionSource::Api,
            "sync" => RevisionSource::Sync,
            "restore" => RevisionSource::Restore,
            _ => return None,
        })
    }
}

/// A note's text as it was before an edit.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteRevision {
    pub id: RevisionId,
    pub note_id: NoteId,
    /// When the edit that replaced this text was made, epoch ms.
    pub at: i64,
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// Who made that edit.
    pub source: RevisionSource,
}

/// What an update wrote: the note's cards, and the revision that keeps
/// the text it replaced (none when the save changed nothing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteUpdate {
    pub cards: Vec<CardId>,
    pub revision: Option<RevisionId>,
    /// How many of `cards` the save made.
    pub created: u32,
}

/// A restore's outcome: the note it rewrote, what the update wrote (of
/// its cards, `created` are new; the rest were rewritten in place), and
/// the note's deck with its live card count after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredRevision {
    pub note: NoteId,
    pub update: NoteUpdate,
    pub created: u32,
    pub deck_totals: Vec<(i64, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteRow {
    pub id: NoteId,
    pub deck_id: DeckId,
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    /// Tags of the note's cards (every card carries the same set).
    pub tags: Vec<String>,
    /// Live cards, ascending by ord.
    pub cards: Vec<(u32, CardId)>,
    /// Slots whose card a person deleted: a save does not make them again.
    pub dropped: BTreeSet<u32>,
    /// The language a person picked for each field.
    pub langs: NoteLangs,
}

/// The language a person picked for each field of a card they wrote, as
/// a tag ("ja", "es-MX"); None where nobody said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteLangs {
    pub front: Option<String>,
    pub back: Option<String>,
}

impl NoteLangs {
    /// Each tag as the language table spells it, or the sentence saying why
    /// it is not one.
    fn validated(&self) -> Result<NoteLangs> {
        let check = |tag: &Option<String>| match tag.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(t) => flash_core::languages::normalize_tag(t)
                .map(Some)
                .ok_or_else(|| StoreError::Invalid(format!("unknown language {t:?}"))),
        };
        Ok(NoteLangs {
            front: check(&self.front)?,
            back: check(&self.back)?,
        })
    }
}

/// The speech blocks a save brings for the note's two fields, from the
/// assistant that wrote them: None for a field it says nothing about.
/// Such a field keeps its blocks while its text stays the same, and
/// loses them when its text changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteSpeech {
    pub front: Option<Vec<SpeechBlock>>,
    pub back: Option<Vec<SpeechBlock>>,
}

impl NoteSpeech {
    fn validated(&self) -> Result<NoteSpeech> {
        let check = |side: &Option<Vec<SpeechBlock>>| {
            side.as_deref()
                .map(speech::validate)
                .transpose()
                .map_err(StoreError::Invalid)
        };
        Ok(NoteSpeech {
            front: check(&self.front)?,
            back: check(&self.back)?,
        })
    }

    fn any(&self) -> bool {
        self.front.is_some() || self.back.is_some()
    }
}

/// A note's stored speech: each field's blocks as JSON, and who wrote
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StoredSpeech {
    front: Option<String>,
    back: Option<String>,
    by: Option<String>,
}

/// Everything a note save needs, beyond the generated cards.
pub struct NoteSave<'a> {
    pub note_type: NoteType,
    pub front_html: &'a SanitizedHtml,
    pub back_html: &'a SanitizedHtml,
    pub generated: &'a [GeneratedCard],
    pub tags: &'a [String],
    /// Card cap a policy set; the check runs inside the write transaction.
    pub cap: Option<u32>,
    /// The user's `ai_jobs` row a downstream extension made the note from
    /// (`notes.ai_job_id`). Set on creation only.
    pub origin: Option<i64>,
    /// How each field reads aloud, when the writer says.
    pub speech: NoteSpeech,
    /// The language picked for each field: None leaves them as they are
    /// (an assistant's edit, a restore, an app from before them), and a
    /// field whose language changes loses its blocks like one whose
    /// words change.
    pub langs: Option<NoteLangs>,
    pub now_ms: i64,
}

impl Store {
    /// Creates a note in `deck` and its cards. Rejected atomically at the
    /// cap, like `create_cards`.
    pub fn create_note(
        &self,
        user: UserId,
        deck: DeckId,
        save: &NoteSave<'_>,
    ) -> Result<(NoteId, Vec<CardId>)> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let result = self.create_note_tx(&tx, user, deck, save)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn create_note_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        deck: DeckId,
        save: &NoteSave<'_>,
    ) -> Result<(NoteId, Vec<CardId>)> {
        require_deck(tx, user, deck)?;
        if let Some(origin) = save.origin {
            let owned: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM ai_jobs WHERE id = ?1 AND user_id = ?2)",
                params![origin, user.raw()],
                |r| r.get(0),
            )?;
            if !owned {
                return Err(StoreError::NotFound("ai job"));
            }
        }
        let given = save.speech.validated()?;
        let langs = save.langs.clone().unwrap_or_default().validated()?;
        let stored = StoredSpeech {
            front: given.front.as_deref().map(speech::to_json),
            back: given.back.as_deref().map(speech::to_json),
            by: given.any().then(|| SPEECH_BY_ASSISTANT.to_string()),
        };
        let id = next_id(tx, IdTable::Notes)?;
        tx.execute(
            "INSERT INTO notes (id, user_id, deck_id, note_type, front_html, back_html, ai_job_id,
               front_speech, back_speech, speech_by, front_lang, back_lang,
               created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
            params![
                id,
                user.raw(),
                deck.0,
                save.note_type.as_str(),
                save.front_html,
                save.back_html,
                save.origin,
                stored.front,
                stored.back,
                stored.by,
                langs.front,
                langs.back,
                save.now_ms
            ],
        )?;
        let note = NoteId(id);
        let (ids, _) = replace_note_cards(tx, user, note, deck, save, &stored)?;
        Ok((note, ids))
    }

    /// Rewrites a note's source and regenerates its cards in place.
    /// `source` says who is editing; it is recorded on the revision that
    /// keeps the text this save replaces.
    pub fn update_note(
        &self,
        user: UserId,
        note: NoteId,
        save: &NoteSave<'_>,
        source: RevisionSource,
    ) -> Result<NoteUpdate> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let result = self.update_note_tx(&tx, user, note, save, source)?;
        tx.commit()?;
        Ok(result)
    }

    /// `update_note` inside the caller's transaction. When the note's
    /// type, either field or its tag set changes, the text it had is kept
    /// as a revision first; a save that changes none of them keeps none.
    pub fn update_note_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        note: NoteId,
        save: &NoteSave<'_>,
        source: RevisionSource,
    ) -> Result<NoteUpdate> {
        type Current = (i64, String, String, String, StoredSpeech, NoteLangs);
        let current: Option<Current> = tx
            .query_row(
                "SELECT deck_id, note_type, front_html, back_html,
                        front_speech, back_speech, speech_by, front_lang, back_lang FROM notes
                 WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![note.0, user.raw()],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        StoredSpeech {
                            front: r.get(4)?,
                            back: r.get(5)?,
                            by: r.get(6)?,
                        },
                        NoteLangs {
                            front: r.get(7)?,
                            back: r.get(8)?,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((deck, kind, front_html, back_html, old_speech, old_langs)) = current else {
            return Err(StoreError::NotFound("note"));
        };
        let langs = match &save.langs {
            // A picker of languages names "es" for a field that was
            // "es-MX": the same language, so the region stays.
            Some(langs) => {
                let keep = |new: Option<String>, old: &Option<String>| match (new, old) {
                    (Some(n), Some(o)) if !n.contains('-') && o.split('-').next() == Some(&n) => {
                        Some(o.clone())
                    }
                    (n, _) => n,
                };
                let given = langs.validated()?;
                NoteLangs {
                    front: keep(given.front, &old_langs.front),
                    back: keep(given.back, &old_langs.back),
                }
            }
            // A save that names none keeps a field's language while its
            // words stay the same; new words are worked out afresh.
            None => NoteLangs {
                front: if front_html == save.front_html.as_str() {
                    old_langs.front.clone()
                } else {
                    None
                },
                back: if back_html == save.back_html.as_str() {
                    old_langs.back.clone()
                } else {
                    None
                },
            },
        };
        // A field's blocks are the ones the save brings, or the ones it
        // had while its words and its picked language stay the same; a
        // field whose words or language changed has none.
        let given = save.speech.validated()?;
        let side = |given: &Option<Vec<SpeechBlock>>, same: bool, old: &Option<String>| match given
        {
            Some(blocks) => Some(speech::to_json(blocks)),
            None if same => old.clone(),
            None => None,
        };
        let stored = {
            let front = side(
                &given.front,
                front_html == save.front_html.as_str() && old_langs.front == langs.front,
                &old_speech.front,
            );
            let back = side(
                &given.back,
                back_html == save.back_html.as_str() && old_langs.back == langs.back,
                &old_speech.back,
            );
            let by = if given.any() {
                Some(SPEECH_BY_ASSISTANT.to_string())
            } else if front.is_none() && back.is_none() {
                None
            } else {
                old_speech.by.clone()
            };
            StoredSpeech { front, back, by }
        };
        let tags = note_tags(tx, user, note)?;
        // Compared as the store will write them (`attach_tag`), so a save
        // that only reorders or recases the tags changes nothing.
        let wanted: BTreeSet<String> = save
            .tags
            .iter()
            .map(|t| flash_core::queue::normalize_tag(t))
            .filter(|t| !t.is_empty())
            .collect();
        let same_tags = tags.iter().cloned().collect::<BTreeSet<_>>() == wanted;
        let unchanged = kind == save.note_type.as_str()
            && front_html == save.front_html.as_str()
            && back_html == save.back_html.as_str()
            && same_tags;
        let revision = if unchanged {
            None
        } else {
            // Copied from the row in SQL, so the kept fields are exactly
            // the sanitized text the note held and never pass through
            // here as plain strings.
            let tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "INSERT INTO note_revisions
                   (user_id, note_id, at, note_type, front_html, back_html, tags, source)
                 SELECT user_id, id, ?1, note_type, front_html, back_html, ?2, ?3
                 FROM notes WHERE id = ?4 AND user_id = ?5",
                params![save.now_ms, tags, source.as_str(), note.0, user.raw()],
            )?;
            let id = RevisionId(tx.last_insert_rowid());
            prune_revisions(tx, user, note, save.now_ms)?;
            Some(id)
        };
        tx.execute(
            "UPDATE notes SET note_type = ?1, front_html = ?2, back_html = ?3, updated_at = ?4,
                front_speech = ?5, back_speech = ?6, speech_by = ?7,
                front_lang = ?8, back_lang = ?9
             WHERE id = ?10 AND user_id = ?11",
            params![
                save.note_type.as_str(),
                save.front_html,
                save.back_html,
                save.now_ms,
                stored.front,
                stored.back,
                stored.by,
                langs.front,
                langs.back,
                note.0,
                user.raw(),
            ],
        )?;
        let (cards, created) = replace_note_cards(tx, user, note, DeckId(deck), save, &stored)?;
        Ok(NoteUpdate {
            cards,
            revision,
            created,
        })
    }

    /// A note's earlier versions, newest first. Empty for another user's
    /// note.
    pub fn list_note_revisions(&self, user: UserId, note: NoteId) -> Result<Vec<NoteRevision>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {REVISION_COLUMNS} FROM note_revisions
             WHERE note_id = ?1 AND user_id = ?2 ORDER BY at DESC, id DESC"
        ))?;
        let rows = stmt.query_map(params![note.0, user.raw()], revision_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// One revision, if it is the user's.
    pub fn note_revision(&self, user: UserId, id: RevisionId) -> Result<Option<NoteRevision>> {
        Ok(revision_by_id(&self.conn(), user, id)?)
    }

    /// Puts a revision's text back on its note through the ordinary
    /// update, so the note's cards are rewritten in place (ids, schedules
    /// and review history stay) and the text it replaces becomes a
    /// revision itself, made by `RevisionSource::Restore`. Refused when
    /// the revision is not the user's, its note is gone, or the restored
    /// text would drop any of the note's live cards (`implicit_card_removal`,
    /// the rule `update_sources_batch` holds an assistant's edits to:
    /// cards go only by an explicit deletion). `cap` is the card cap any
    /// card it adds is held to.
    pub fn restore_note_revision(
        &self,
        user: UserId,
        id: RevisionId,
        cap: Option<u32>,
        now_ms: i64,
    ) -> Result<RestoredRevision> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let revision = revision_by_id(&tx, user, id)?.ok_or(StoreError::NotFound("revision"))?;
        // Kept text was sanitized when it was saved; sanitizing it again
        // is what lets it back in as `SanitizedHtml`, and changes nothing.
        let front_html = richtext::sanitize_with_media(&revision.front_html).trimmed();
        let back_html = richtext::sanitize_with_media(&revision.back_html).trimmed();
        let generated = generate_cards(revision.note_type, &front_html, &back_html)
            .map_err(|refused| StoreError::Invalid(refused.to_string()))?;
        let slots: Vec<(u32, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT ord, id FROM cards
                 WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
            )?;
            let rows = stmt.query_map(params![revision.note_id.0, user.raw()], |r| {
                Ok((r.get::<_, i64>(0)? as u32, r.get(1)?))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let removed: Vec<i64> = slots
            .iter()
            .filter(|(ord, _)| !generated.iter().any(|g| g.ord == *ord))
            .map(|(_, id)| *id)
            .collect();
        if !removed.is_empty() {
            return Err(StoreError::Invalid(format!(
                "implicit_card_removal: delete these cards explicitly first: {removed:?}"
            )));
        }
        let save = NoteSave {
            note_type: revision.note_type,
            front_html: &front_html,
            back_html: &back_html,
            generated: &generated,
            tags: &revision.tags,
            cap,
            origin: None,
            speech: NoteSpeech::default(),
            // A revision keeps the words, not the languages picked.
            langs: None,
            now_ms,
        };
        let update =
            self.update_note_tx(&tx, user, revision.note_id, &save, RevisionSource::Restore)?;
        let deck: i64 = tx.query_row(
            "SELECT deck_id FROM notes WHERE id = ?1 AND user_id = ?2",
            params![revision.note_id.0, user.raw()],
            |r| r.get(0),
        )?;
        let deck_totals = super::mcp::deck_totals(&tx, user, &[DeckId(deck)])?;
        tx.commit()?;
        Ok(RestoredRevision {
            note: revision.note_id,
            created: update.created,
            update,
            deck_totals,
        })
    }

    pub fn get_note(&self, user: UserId, note: NoteId) -> Result<Option<NoteRow>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT id, deck_id, note_type, front_html, back_html, front_lang, back_lang
                 FROM notes WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![note.0, user.raw()],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        NoteLangs {
                            front: r.get(5)?,
                            back: r.get(6)?,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((id, deck_id, kind, front_html, back_html, langs)) = row else {
            return Ok(None);
        };
        let note_type = NoteType::parse(&kind).ok_or(StoreError::NotFound("note type"))?;
        let mut stmt = conn.prepare(
            "SELECT ord, id FROM cards
             WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL ORDER BY ord",
        )?;
        let cards: Vec<(u32, CardId)> = stmt
            .query_map(params![id, user.raw()], |r| {
                Ok((r.get::<_, i64>(0)? as u32, CardId(r.get(1)?)))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let tags = match cards.first() {
            Some((_, card)) => super::cards::card_tags(&conn, user, *card)?,
            None => Vec::new(),
        };
        Ok(Some(NoteRow {
            id: NoteId(id),
            deck_id: DeckId(deck_id),
            note_type,
            front_html,
            back_html,
            tags,
            cards,
            dropped: dropped_slots(&conn, user, NoteId(id))?,
            langs,
        }))
    }

    /// Records slots of `note` as deleted, as deleting their cards does:
    /// its later saves do not make them (for a downstream extension that
    /// creates a note without some of its cards).
    pub fn drop_note_slots_tx(
        &self,
        tx: &Transaction<'_>,
        user: UserId,
        note: NoteId,
        slots: &[u32],
    ) -> Result<()> {
        drop_slots(tx, user, note, slots)
    }

    /// The note a live card belongs to, if any.
    pub fn note_for_card(&self, user: UserId, card: CardId) -> Result<Option<NoteRow>> {
        let note: Option<i64> = self
            .conn()
            .query_row(
                "SELECT note_id FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        match note {
            Some(id) => self.get_note(user, NoteId(id)),
            None => Ok(None),
        }
    }

    /// How many cards an edit of `card` making `generated` would add: to
    /// its note, or to the one its first edit makes (`adopt_card_into_note`).
    pub fn new_cards_for(
        &self,
        user: UserId,
        card: CardId,
        generated: &[GeneratedCard],
    ) -> Result<u32> {
        if let Some(note) = self.note_for_card(user, card)? {
            return Ok(note.new_cards(generated));
        }
        let conn = self.conn();
        let (deck, source, index): (i64, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT deck_id, cloze_text, cloze_index FROM cards
                 WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or(StoreError::NotFound("card"))?;
        let slots = loose_slots(&conn, user, deck, source.filter(|_| index.is_some()))?;
        Ok(count_new(&slots.held, &slots.dropped, generated))
    }

    /// Promotes a note-less card into a fresh note, so the editor can
    /// regenerate it. An imported cloze card brings its siblings along
    /// (same deck, same cloze source), each at its own index slot, so the
    /// first edit of an imported cloze note doesn't spawn duplicates.
    ///
    /// The note is made with the text the card already has (the editor's
    /// seed: its rich sides, its plain sides as HTML, or an imported
    /// cloze's source), not the text an edit is about to write: the
    /// `update_note` that follows then keeps the card's own text as the
    /// note's first revision, so the first edit of an old card is as
    /// undoable as any other.
    pub fn adopt_card_into_note(
        &self,
        user: UserId,
        card: CardId,
        note_type: NoteType,
        front_html: &SanitizedHtml,
        back_html: &SanitizedHtml,
        now_ms: i64,
    ) -> Result<NoteId> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let result = self
            .adopt_card_into_note_tx(&tx, user, card, note_type, front_html, back_html, now_ms)?;
        tx.commit()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn adopt_card_into_note_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        card: CardId,
        note_type: NoteType,
        front_html: &SanitizedHtml,
        back_html: &SanitizedHtml,
        now_ms: i64,
    ) -> Result<NoteId> {
        type SeedRow = (i64, Option<i64>, Option<String>, Option<i64>);
        let row: Option<SeedRow> = tx
            .query_row(
                "SELECT deck_id, note_id, cloze_text, cloze_index FROM cards
                 WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((deck, note_id, cloze_text, cloze_index)) = row else {
            return Err(StoreError::NotFound("card"));
        };
        if let Some(existing) = note_id {
            return Ok(NoteId(existing));
        }
        let note = next_id(tx, IdTable::Notes)?;
        tx.execute(
            "INSERT INTO notes (id, user_id, deck_id, note_type, front_html, back_html, created_at,
               updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                note,
                user.raw(),
                deck,
                note_type.as_str(),
                front_html,
                back_html,
                now_ms
            ],
        )?;
        let cloze = cloze_text.filter(|_| cloze_index.is_some());
        // A sibling deleted before this first edit stays deleted.
        let gone = Vec::from_iter(loose_slots(tx, user, deck, cloze.clone())?.dropped);
        match cloze {
            Some(source) => {
                // Siblings adopt too, at ord = index - 1. Duplicate indices
                // (a re-import) keep only the lowest card id per slot.
                tx.execute(
                    "UPDATE cards SET note_id = ?1, ord = cloze_index - 1
                     WHERE id IN (
                       SELECT MIN(id) FROM cards
                       WHERE user_id = ?2 AND deck_id = ?3 AND deleted_at IS NULL
                         AND note_id IS NULL AND cloze_text = ?4 AND cloze_index IS NOT NULL
                       GROUP BY cloze_index)",
                    params![note, user.raw(), deck, source],
                )?;
                drop_slots(tx, user, NoteId(note), &gone)?;
            }
            None => {
                tx.execute(
                    "UPDATE cards SET note_id = ?1, ord = 0 WHERE id = ?2 AND user_id = ?3",
                    params![note, card.0, user.raw()],
                )?;
            }
        }
        Ok(NoteId(note))
    }
}

/// The tags a note's cards carry (every card carries the same set),
/// read from its first live card.
fn note_tags(conn: &rusqlite::Connection, user: UserId, note: NoteId) -> Result<Vec<String>> {
    let first: Option<i64> = conn
        .query_row(
            "SELECT id FROM cards WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL
             ORDER BY ord LIMIT 1",
            params![note.0, user.raw()],
            |r| r.get(0),
        )
        .optional()?;
    match first {
        Some(card) => super::cards::card_tags(conn, user, CardId(card)),
        None => Ok(Vec::new()),
    }
}

/// Lets go of a note's revisions past its keep: beyond the newest
/// `REVISIONS_KEPT`, or older than `REVISION_MAX_AGE_MS`.
fn prune_revisions(tx: &Transaction<'_>, user: UserId, note: NoteId, now_ms: i64) -> Result<()> {
    tx.execute(
        "DELETE FROM note_revisions
         WHERE note_id = ?1 AND user_id = ?2
           AND (at < ?3 OR id NOT IN (
             SELECT id FROM note_revisions WHERE note_id = ?1 AND user_id = ?2
             ORDER BY at DESC, id DESC LIMIT ?4))",
        params![
            note.0,
            user.raw(),
            now_ms - REVISION_MAX_AGE_MS,
            REVISIONS_KEPT as i64
        ],
    )?;
    Ok(())
}

const REVISION_COLUMNS: &str = "id, note_id, at, note_type, front_html, back_html, tags, source";

/// Maps a `REVISION_COLUMNS` row. A type, tag list or source the store
/// did not write reads as a conversion failure rather than a guess.
fn revision_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRevision> {
    let bad = |i: usize, what: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            i,
            rusqlite::types::Type::Text,
            format!("unknown revision {what}").into(),
        )
    };
    let kind: String = r.get(3)?;
    let tags: String = r.get(6)?;
    let source: String = r.get(7)?;
    Ok(NoteRevision {
        id: RevisionId(r.get(0)?),
        note_id: NoteId(r.get(1)?),
        at: r.get(2)?,
        note_type: NoteType::parse(&kind).ok_or_else(|| bad(3, "note type"))?,
        front_html: r.get(4)?,
        back_html: r.get(5)?,
        tags: serde_json::from_str(&tags).map_err(|_| bad(6, "tags"))?,
        source: RevisionSource::parse(&source).ok_or_else(|| bad(7, "source"))?,
    })
}

fn revision_by_id(
    conn: &rusqlite::Connection,
    user: UserId,
    id: RevisionId,
) -> rusqlite::Result<Option<NoteRevision>> {
    conn.query_row(
        &format!("SELECT {REVISION_COLUMNS} FROM note_revisions WHERE id = ?1 AND user_id = ?2"),
        params![id.0, user.raw()],
        revision_row,
    )
    .optional()
}

impl Store {
    /// The language a person picked for one side of a card, through its
    /// note: a reversed card's front is the note's back, and a cloze
    /// card's back is its text again. None for a card with no note, or a
    /// side nobody picked a language for.
    pub fn card_side_lang(
        &self,
        user: UserId,
        card: CardId,
        side: CardSide,
    ) -> Result<Option<String>> {
        let row: Option<(String, i64, Option<String>, Option<String>)> = self
            .conn()
            .query_row(
                "SELECT n.note_type, c.ord, n.front_lang, n.back_lang
                 FROM cards c JOIN notes n ON n.id = c.note_id
                 WHERE c.id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL",
                params![card.0, user.raw()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((kind, ord, front, back)) = row else {
            return Ok(None);
        };
        let swapped = kind == NoteType::BasicReversed.as_str() && ord == 1;
        Ok(match (kind == NoteType::Cloze.as_str(), swapped, side) {
            (true, _, _) => front,
            (false, false, CardSide::Front) | (false, true, CardSide::Back) => front,
            (false, false, CardSide::Back) | (false, true, CardSide::Front) => back,
        })
    }
}

/// The slots of `note` whose card a person deleted.
fn dropped_slots(conn: &rusqlite::Connection, user: UserId, note: NoteId) -> Result<BTreeSet<u32>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT dropped_slots FROM notes WHERE id = ?1 AND user_id = ?2",
            params![note.0, user.raw()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

fn set_dropped_slots(
    conn: &rusqlite::Connection,
    user: UserId,
    note: NoteId,
    slots: &BTreeSet<u32>,
) -> Result<()> {
    let json = serde_json::to_string(slots).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE notes SET dropped_slots = ?1 WHERE id = ?2 AND user_id = ?3",
        params![json, note.0, user.raw()],
    )?;
    Ok(())
}

/// Records `slots` of `note` as ones whose card a person deleted.
pub(crate) fn drop_slots(
    conn: &rusqlite::Connection,
    user: UserId,
    note: NoteId,
    slots: &[u32],
) -> Result<()> {
    let mut dropped = dropped_slots(conn, user, note)?;
    let before = dropped.len();
    dropped.extend(slots);
    if dropped.len() != before {
        set_dropped_slots(conn, user, note, &dropped)?;
    }
    Ok(())
}

/// Of the slots whose card a person deleted, those a save making
/// `wanted` leaves out: the ones its text still makes and no card holds,
/// and none when leaving them out would leave the note with no card (an
/// edit that renumbers its gaps onto a deleted one).
fn left_out(dropped: &BTreeSet<u32>, wanted: &[u32], held: impl Fn(u32) -> bool) -> BTreeSet<u32> {
    let out: BTreeSet<u32> = dropped
        .iter()
        .copied()
        .filter(|o| wanted.contains(o) && !held(*o))
        .collect();
    if wanted.iter().all(|o| out.contains(o)) {
        BTreeSet::new()
    } else {
        out
    }
}

impl NoteRow {
    /// How many cards a save making `generated` would add to this note.
    pub fn new_cards(&self, generated: &[GeneratedCard]) -> u32 {
        let held = self.cards.iter().map(|(ord, _)| *ord).collect();
        count_new(&held, &self.dropped, generated)
    }
}

fn count_new(held: &BTreeSet<u32>, dropped: &BTreeSet<u32>, generated: &[GeneratedCard]) -> u32 {
    let wanted: Vec<u32> = generated.iter().map(|g| g.ord).collect();
    let out = left_out(dropped, &wanted, |o| held.contains(&o));
    wanted
        .iter()
        .filter(|o| !held.contains(o) && !out.contains(o))
        .count() as u32
}

/// The slots a note-less card's first note has (`adopt_card_into_note_tx`).
struct LooseSlots {
    /// Slots a live card fills.
    held: BTreeSet<u32>,
    /// Slots only deleted cards filled: the note leaves them out.
    dropped: BTreeSet<u32>,
}

/// An imported cloze's cards (one deck, one `cloze` source) at their
/// indices; any other card alone at slot 0.
fn loose_slots(
    conn: &Connection,
    user: UserId,
    deck: i64,
    cloze: Option<String>,
) -> Result<LooseSlots> {
    let mut slots = LooseSlots {
        held: BTreeSet::new(),
        dropped: BTreeSet::new(),
    };
    let Some(source) = cloze else {
        slots.held.insert(0);
        return Ok(slots);
    };
    let mut stmt = conn.prepare(
        "SELECT cloze_index - 1, MAX(deleted_at IS NULL) FROM cards
         WHERE user_id = ?1 AND deck_id = ?2 AND note_id IS NULL AND cloze_text = ?3
           AND cloze_index IS NOT NULL
         GROUP BY cloze_index",
    )?;
    let rows = stmt.query_map(params![user.raw(), deck, source], |r| {
        Ok((r.get::<_, u32>(0)?, r.get::<_, bool>(1)?))
    })?;
    for row in rows {
        let (ord, live) = row?;
        if live {
            &mut slots.held
        } else {
            &mut slots.dropped
        }
        .insert(ord);
    }
    Ok(slots)
}

/// Writes the generated cards for `note` inside `tx`; see module doc.
/// Each card's speech blocks are derived from the note's (`card_speech`).
/// Answers the cards, in slot order, and how many of them are new.
fn replace_note_cards(
    tx: &Transaction<'_>,
    user: UserId,
    note: NoteId,
    deck: DeckId,
    save: &NoteSave<'_>,
    speech: &StoredSpeech,
) -> Result<(Vec<CardId>, u32)> {
    let note_front = speech::from_json(speech.front.as_deref());
    let note_back = speech::from_json(speech.back.as_deref());
    let existing: BTreeMap<u32, i64> = {
        let mut stmt = tx.prepare(
            "SELECT ord, id FROM cards WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map(params![note.0, user.raw()], |r| {
            Ok((r.get::<_, i64>(0)? as u32, r.get(1)?))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let wanted: Vec<u32> = save.generated.iter().map(|g| g.ord).collect();
    // A slot whose card a person deleted is not made again; one the save
    // does not leave out is forgotten.
    let dropped = dropped_slots(tx, user, note)?;
    let kept = left_out(&dropped, &wanted, |o| existing.contains_key(&o));
    if kept != dropped {
        set_dropped_slots(tx, user, note, &kept)?;
    }

    // Slots that vanished go first, so their rows free the cap and the
    // unique (note_id, ord) index before anything is inserted.
    for (ord, id) in &existing {
        if !wanted.contains(ord) {
            tx.execute(
                "UPDATE cards SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND user_id = ?3",
                params![save.now_ms, id, user.raw()],
            )?;
        }
    }
    let inserts = wanted
        .iter()
        .filter(|o| !existing.contains_key(o) && !kept.contains(o))
        .count() as u32;
    if let Some(cap) = save.cap {
        if inserts > 0 {
            let current: i64 = tx.query_row(
                "SELECT COUNT(*) FROM cards WHERE user_id = ?1 AND deleted_at IS NULL",
                [user.raw()],
                |r| r.get(0),
            )?;
            let current = current as u32;
            if current.saturating_add(inserts) > cap {
                return Err(StoreError::CapExceeded { current, cap });
            }
        }
    }

    let mut ids = Vec::with_capacity(save.generated.len());
    for g in save.generated.iter().filter(|g| !kept.contains(&g.ord)) {
        let x = &g.extras;
        let (front_speech, back_speech) = card_speech(
            save.note_type,
            g.ord,
            note_front.as_deref(),
            note_back.as_deref(),
        );
        let front_speech = front_speech.as_deref().map(speech::to_json);
        let back_speech = back_speech.as_deref().map(speech::to_json);
        let speech_by = (front_speech.is_some() || back_speech.is_some())
            .then(|| speech.by.clone())
            .flatten();
        let card_id = match existing.get(&g.ord) {
            Some(id) => {
                tx.execute(
                    "UPDATE cards SET front = ?1, back = ?2, front_html = ?3, back_html = ?4,
                       cloze_text = ?5, cloze_index = ?6, type_answer = ?7, updated_at = ?8,
                       front_speech = ?9, back_speech = ?10, speech_by = ?11
                     WHERE id = ?12 AND user_id = ?13",
                    params![
                        g.text.front,
                        g.text.back,
                        x.front_html,
                        x.back_html,
                        x.cloze_text,
                        x.cloze_index,
                        x.type_answer,
                        save.now_ms,
                        front_speech,
                        back_speech,
                        speech_by,
                        id,
                        user.raw()
                    ],
                )?;
                *id
            }
            None => {
                let id = next_id(tx, IdTable::Cards)?;
                tx.execute(
                    "INSERT INTO cards (id, user_id, deck_id, note_id, ord, front, back,
                       front_html, back_html, cloze_text, cloze_index, type_answer,
                       front_speech, back_speech, speech_by, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                       ?16, ?16)",
                    params![
                        id,
                        user.raw(),
                        deck.0,
                        note.0,
                        g.ord,
                        g.text.front,
                        g.text.back,
                        x.front_html,
                        x.back_html,
                        x.cloze_text,
                        x.cloze_index,
                        x.type_answer,
                        front_speech,
                        back_speech,
                        speech_by,
                        save.now_ms
                    ],
                )?;
                tx.execute(
                    "INSERT INTO card_state (card_id, due)
                     SELECT ?1, ?2 FROM cards WHERE id = ?1 AND user_id = ?3",
                    params![id, save.now_ms, user.raw()],
                )?;
                id
            }
        };
        let card = CardId(card_id);
        // Tags and media links are replaced wholesale per card.
        tx.execute(
            "DELETE FROM card_tags WHERE card_id = ?1
               AND EXISTS (SELECT 1 FROM cards WHERE id = ?1 AND user_id = ?2)",
            params![card_id, user.raw()],
        )?;
        check_tags(save.tags)?;
        for tag in save.tags {
            attach_tag(tx, user, card, tag)?;
        }
        tx.execute(
            "DELETE FROM card_media WHERE card_id = ?1
               AND EXISTS (SELECT 1 FROM cards WHERE id = ?1 AND user_id = ?2)",
            params![card_id, user.raw()],
        )?;
        let mut media = richtext::media_ids(x.front_html.as_deref().unwrap_or(""));
        for id in richtext::media_ids(x.back_html.as_deref().unwrap_or("")) {
            if !media.contains(&id) {
                media.push(id);
            }
        }
        for media_id in media {
            // Ownership-guarded: a foreign id simply doesn't link (the
            // service already refused it with a proper error).
            tx.execute(
                "INSERT OR IGNORE INTO card_media (card_id, media_id)
                 SELECT ?1, m.id FROM media m WHERE m.id = ?2 AND m.user_id = ?3",
                params![card_id, media_id, user.raw()],
            )?;
        }
        ids.push(card);
    }
    Ok((ids, inserts))
}
