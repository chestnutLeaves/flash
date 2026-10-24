use flash_core::{CardId, DeckId, MediaId, NoteId, RevisionId, UserId};
use flash_store::notes::{generate_cards, GeneratedCard, NoteType, MAX_FIELD_HTML};
use flash_store::{
    richtext, CardRow, NoteRevision, NoteSave, RestoredRevision, RevisionSource, StoreError,
};

use crate::policy::AddSource;
use crate::service::{bounded_search, note_refused, text_to_html, Result, ServiceError, Services};

impl Services {
    pub fn create_mcp_notes(
        &self,
        user: UserId,
        deck: &str,
        inputs: &[NoteInput],
        now: i64,
    ) -> Result<flash_store::McpMutationReceipt> {
        let prepared = inputs
            .iter()
            .map(|input| self.prepare_note(user, input))
            .collect::<Result<Vec<_>>>()?;
        let inserts = prepared.iter().map(|p| p.generated.len() as u32).sum();
        let cap = self.note_cap(user, inserts, now)?;
        let saves = prepared
            .iter()
            .zip(inputs)
            .map(|(p, input)| p.save(input.note_type, cap, None, now))
            .collect::<Vec<_>>();
        self.store
            .create_notes_batch(user, deck, &saves, now)
            .map_err(|e| self.cap_error(e))
    }

    pub fn update_mcp_sources(
        &self,
        user: UserId,
        patches: &[McpSourcePatch],
        now: i64,
    ) -> Result<flash_store::McpMutationReceipt> {
        let mut inputs = Vec::new();
        let mut states = Vec::new();
        let mut seeds = Vec::new();
        for patch in patches {
            let state = self.store.mcp_source_state(user, patch.card_id)?;
            let seed = self.editor_seed(user, patch.card_id)?;
            let mut sent = patch.sibling_card_ids.clone();
            sent.sort_by_key(|id| id.0);
            if (state.1.len() > 1 || !sent.is_empty()) && sent != state.1 {
                // The exact ids, so the assistant can send them at once:
                // told only to read list_cards again, one sent the same
                // wrong list over and over, then deleted and remade the
                // cards, losing their reviews.
                let card = patch.card_id.0;
                let ids = state.1.iter().map(|id| id.0).collect::<Vec<_>>();
                return Err(ServiceError::Malformed(if ids.len() > 1 {
                    format!(
                        "sibling_mismatch: card {card} shares its source with cards {ids:?}, \
                         which change together: send sibling_card_ids {ids:?} (this card \
                         included) in one edit for them"
                    )
                } else {
                    format!(
                        "sibling_mismatch: card {card} has no other cards: omit \
                         sibling_card_ids or send [{card}]"
                    )
                }));
            }
            if let Some(reason) = self.mcp_source_warning(user, &seed, &state.1)? {
                return Err(ServiceError::Malformed(reason));
            }
            if patch.front_html.is_none()
                && patch.back_html.is_none()
                && patch.tags.is_none()
                && patch.speech == flash_store::NoteSpeech::default()
            {
                return Err(ServiceError::Malformed(
                    "an update must supply at least one field".into(),
                ));
            }
            let (seed_front, seed_back) = sanitized_seed(&seed);
            seeds.push((seed.note_type, seed_front, seed_back));
            inputs.push(NoteInput {
                note_type: seed.note_type,
                front_html: patch.front_html.clone().unwrap_or(seed.front_html),
                back_html: patch.back_html.clone().unwrap_or(seed.back_html),
                tags: patch.tags.clone().unwrap_or(seed.tags),
                speech: patch.speech.clone(),
                // An assistant's edit leaves the languages a person picked.
                langs: None,
            });
            states.push(state);
        }
        let prepared = inputs
            .iter()
            .map(|input| self.prepare_note(user, input))
            .collect::<Result<Vec<_>>>()?;
        let inserts = prepared
            .iter()
            .zip(patches)
            .map(|(p, patch)| self.store.new_cards_for(user, patch.card_id, &p.generated))
            .sum::<flash_store::Result<u32>>()?;
        let cap = self.note_cap(user, inserts, now)?;
        let edits = prepared
            .iter()
            .zip(&inputs)
            .zip(patches)
            .zip(states)
            .zip(&seeds)
            .map(
                |((((p, input), patch), (snapshot, siblings)), (seed_type, front, back))| {
                    flash_store::SourceEdit {
                        card: patch.card_id,
                        snapshot,
                        siblings,
                        seed: (*seed_type, front, back),
                        save: p.save(input.note_type, cap, None, now),
                    }
                },
            )
            .collect::<Vec<_>>();
        self.store
            .update_sources_batch(user, &edits)
            .map_err(|e| self.cap_error(e))
    }

    /// Some old imports have only rendered HTML and a stripped source.
    /// Expose that limitation on reads, and refuse destructive reconstruction.
    pub(crate) fn mcp_source_warning(
        &self,
        user: UserId,
        seed: &EditorSeed,
        siblings: &[CardId],
    ) -> Result<Option<String>> {
        if seed.note_id.is_some()
            || !matches!(seed.note_type, NoteType::Cloze | NoteType::BasicTyped)
        {
            return Ok(None);
        }
        let original = NoteInput {
            note_type: seed.note_type,
            front_html: seed.front_html.clone(),
            back_html: seed.back_html.clone(),
            tags: seed.tags.clone(),
            speech: Default::default(),
            langs: None,
        };
        let generated = self.prepare_note(user, &original)?;
        let mut seen_slots = std::collections::HashSet::new();
        for id in siblings {
            let row = self.card(user, *id)?;
            let ord = row.cloze_index.map(|n| n.saturating_sub(1)).unwrap_or(0);
            if !seen_slots.insert(ord) || row.tags != seed.tags {
                return Ok(Some("source_unavailable: imported siblings have duplicate slots or different tags; resolve the source grouping before editing".into()));
            }
            let lossless = generated
                .generated
                .iter()
                .find(|g| g.ord == ord)
                .is_some_and(|g| {
                    g.extras.front_html.as_deref() == row.front_html.as_deref()
                        && g.extras.back_html.as_deref() == row.back_html.as_deref()
                        && g.extras.type_answer == row.type_answer
                });
            if !lossless {
                return Ok(Some("source_unavailable: this legacy import lacks a lossless editable source; rendered content is readable, but cannot be safely rewritten".into()));
            }
        }
        Ok(None)
    }

    /// A note's sides and tags as a save would store them, checked as a
    /// save checks them (`prepare_note`): for fields kept somewhere other
    /// than a note.
    pub fn note_fields(&self, user: UserId, input: &NoteInput) -> Result<NoteFields> {
        let p = self.prepare_note(user, input)?;
        Ok(NoteFields {
            front_html: p.front_html,
            back_html: p.back_html,
            tags: p.tags,
        })
    }

    /// Sanitizes the editor's fields, expands them into cards, and checks
    /// every media reference resolves to the user's own upload.
    pub(super) fn prepare_note(&self, user: UserId, input: &NoteInput) -> Result<PreparedNote> {
        if input.front_html.len() > MAX_FIELD_HTML || input.back_html.len() > MAX_FIELD_HTML {
            return Err(note_refused(flash_store::notes::NoteError::FieldTooLong));
        }
        let front_html = richtext::sanitize_with_media(&input.front_html).trimmed();
        let back_html = richtext::sanitize_with_media(&input.back_html).trimmed();
        let generated =
            generate_cards(input.note_type, &front_html, &back_html).map_err(note_refused)?;
        let mut media = richtext::media_ids(&front_html);
        media.extend(richtext::media_ids(&back_html));
        for id in media {
            if self.store.get_media(user, MediaId(id))?.is_none() {
                return Err(ServiceError::refused(
                    "media",
                    "card-error-media",
                    &[("id", id.into())],
                ));
            }
        }
        let tags = flash_core::queue::normalize_tags(&input.tags);
        if let Some(langs) = &input.langs {
            for tag in [&langs.front, &langs.back].into_iter().flatten() {
                if !tag.trim().is_empty() && crate::languages::normalize_tag(tag.trim()).is_none() {
                    return Err(ServiceError::Malformed(format!("unknown language {tag:?}")));
                }
            }
        }
        Ok(PreparedNote {
            front_html,
            back_html,
            generated,
            tags,
            speech: input.speech.clone(),
            langs: input.langs.clone(),
        })
    }

    /// Note-editor gate: `inserts` new cards go through `admit_cards`. An
    /// edit that adds none is never blocked (an over-cap account can
    /// still fix its existing cards), which is the policy's contract for
    /// an admission of zero: it only reports the cap the store re-checks.
    fn note_cap(&self, user: UserId, inserts: u32, now_ms: i64) -> Result<Option<u32>> {
        self.admit_cards(user, inserts, AddSource::Interactive, now_ms)
    }

    /// Creates a note (and its cards) in `deck` from the advanced editor.
    pub fn create_note(
        &self,
        user: UserId,
        deck: DeckId,
        input: &NoteInput,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        self.create_note_from(user, deck, input, None, now_ms)
    }

    /// `create_note` that records the downstream extension's row the note
    /// came from (`NoteSave::origin`).
    pub fn create_note_from(
        &self,
        user: UserId,
        deck: DeckId,
        input: &NoteInput,
        origin: Option<i64>,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        let p = self.prepare_note(user, input)?;
        let cap = self.note_cap(user, p.generated.len() as u32, now_ms)?;
        self.store
            .create_note(user, deck, &p.save(input.note_type, cap, origin, now_ms))
            .map_err(|e| self.cap_error(e))
    }

    /// Saves the editor over an existing card: its note is rewritten and
    /// regenerated in place, the text it had kept as a revision; a
    /// note-less card is adopted into a fresh note first (an imported
    /// cloze card brings its siblings). `source` says which editor this
    /// is (the web's or the app's), for the revision.
    pub fn save_card_editor(
        &self,
        user: UserId,
        card: CardId,
        input: &NoteInput,
        source: RevisionSource,
        now_ms: i64,
    ) -> Result<(NoteId, Vec<CardId>)> {
        let p = self.prepare_note(user, input)?;
        let note = match self.store.note_for_card(user, card)? {
            Some(row) => row,
            None => {
                let seed = self.editor_seed(user, card)?;
                let (front, back) = sanitized_seed(&seed);
                let id = self.store.adopt_card_into_note(
                    user,
                    card,
                    seed.note_type,
                    &front,
                    &back,
                    now_ms,
                )?;
                self.store
                    .get_note(user, id)?
                    .ok_or(ServiceError::Store(StoreError::NotFound("note")))?
            }
        };
        let cap = self.note_cap(user, note.new_cards(&p.generated), now_ms)?;
        let saved = self
            .store
            .update_note(
                user,
                note.id,
                &p.save(input.note_type, cap, None, now_ms),
                source,
            )
            .map_err(|e| self.cap_error(e))?;
        Ok((note.id, saved.cards))
    }

    /// The study panel's quick edit: new text for the two fields of the
    /// note behind `card`, the note's type and tags as they are. A person
    /// editing their own card, so the History says "an edit here". An
    /// edit that would take away the card being studied (the cloze gap
    /// it asks about, deleted) is refused, so the panel's card stays.
    pub fn save_panel_edit(
        &self,
        user: UserId,
        card: CardId,
        front_html: String,
        back_html: String,
        now_ms: i64,
    ) -> Result<()> {
        let seed = self.editor_seed(user, card)?;
        let input = NoteInput {
            note_type: seed.note_type,
            front_html,
            back_html,
            tags: seed.tags,
            speech: Default::default(),
            langs: None,
        };
        if let Some(note) = self.store.note_for_card(user, card)? {
            let p = self.prepare_note(user, &input)?;
            let ord = note
                .cards
                .iter()
                .find(|(_, id)| *id == card)
                .map(|(o, _)| *o);
            if ord.is_some_and(|ord| !p.generated.iter().any(|g| g.ord == ord)) {
                return Err(ServiceError::Invalid(crate::i18n::t(
                    "study-edit-removes-card",
                )));
            }
        }
        self.save_card_editor(user, card, &input, RevisionSource::Web, now_ms)?;
        Ok(())
    }

    /// The earlier versions of the note `card` belongs to, newest first.
    /// A card no edit has touched since it was made (or imported) has
    /// none.
    pub fn card_revisions(&self, user: UserId, card: CardId) -> Result<Vec<NoteRevision>> {
        if self.store.get_card(user, card)?.is_none() {
            return Err(ServiceError::Store(StoreError::NotFound("card")));
        }
        match self.store.note_for_card(user, card)? {
            Some(note) => Ok(self.store.list_note_revisions(user, note.id)?),
            None => Ok(Vec::new()),
        }
    }

    /// Puts an earlier version of `card`'s note back: its cards are
    /// rewritten in place, keeping their ids, schedules and review
    /// history, and the text replaced is kept as a revision in turn. The
    /// revision must be one of this card's note's; a restore that would
    /// drop cards is refused, and one that adds cards meets the card cap
    /// like any edit.
    pub fn restore_card_revision(
        &self,
        user: UserId,
        card: CardId,
        revision: RevisionId,
        now_ms: i64,
    ) -> Result<RestoredRevision> {
        if self.store.get_card(user, card)?.is_none() {
            return Err(ServiceError::Store(StoreError::NotFound("card")));
        }
        // A card with no note was never edited, so no revision is its.
        let note = self
            .store
            .note_for_card(user, card)?
            .ok_or(ServiceError::Store(StoreError::NotFound("revision")))?;
        let kept = self
            .store
            .note_revision(user, revision)?
            .filter(|r| r.note_id == note.id)
            .ok_or(ServiceError::Store(StoreError::NotFound("revision")))?;
        let front = richtext::sanitize_with_media(&kept.front_html).trimmed();
        let back = richtext::sanitize_with_media(&kept.back_html).trimmed();
        let generated = generate_cards(kept.note_type, &front, &back).map_err(note_refused)?;
        let cap = self.note_cap(user, note.new_cards(&generated), now_ms)?;
        self.store
            .restore_note_revision(user, revision, cap, now_ms)
            .map_err(|e| self.cap_error(e))
    }

    /// What the editor opens with for `card`: its note's source fields, or
    /// a synthesized note for a standalone card (imported cloze/typed cards
    /// reconstruct their type from the rich columns).
    pub fn editor_seed(&self, user: UserId, card: CardId) -> Result<EditorSeed> {
        let row = self
            .store
            .get_card(user, card)?
            .ok_or(ServiceError::Store(StoreError::NotFound("card")))?;
        if let Some(note) = self.store.note_for_card(user, card)? {
            return Ok(EditorSeed {
                card_id: card,
                deck_id: note.deck_id,
                note_id: Some(note.id),
                note_type: note.note_type,
                front_html: note.front_html,
                back_html: note.back_html,
                tags: note.tags,
                sibling_count: note.cards.len(),
                langs: note.langs,
            });
        }
        let cloze = self.store.cloze_source(user, card)?;
        let (note_type, front_html, back_html) = match cloze {
            Some(source) => {
                let (text, extra) = source.split_once('\u{1f}').unwrap_or((&source, ""));
                (NoteType::Cloze, text_to_html(text), text_to_html(extra))
            }
            None => {
                let kind = if row.type_answer.is_some() {
                    NoteType::BasicTyped
                } else {
                    NoteType::Basic
                };
                (
                    kind,
                    row.front_html
                        .clone()
                        .unwrap_or_else(|| text_to_html(&row.front)),
                    row.back_html
                        .clone()
                        .unwrap_or_else(|| text_to_html(&row.back)),
                )
            }
        };
        Ok(EditorSeed {
            card_id: card,
            deck_id: row.deck_id,
            note_id: None,
            note_type,
            front_html,
            back_html,
            tags: row.tags,
            sibling_count: 1,
            langs: Default::default(),
        })
    }

    /// Hard-deletes a deck and all its cards; the caller removes the
    /// returned orphan blobs from the media store.
    pub fn delete_deck(&self, user: UserId, deck: DeckId) -> Result<flash_store::DeletedDeck> {
        Ok(self.store.delete_deck(user, deck)?)
    }

    pub fn list_cards(
        &self,
        user: UserId,
        deck: Option<DeckId>,
        search: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CardRow>> {
        let search = bounded_search(search)?;
        Ok(self.store.list_cards(user, deck, search, limit.min(500))?)
    }
}

/// The advanced editor's submission: a note type, two raw HTML fields
/// (sanitized here, never trusted), and the tag list every generated card
/// receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteInput {
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// Each field's speech blocks, when the writer gives them; a field
    /// it gives none for gets none from it.
    pub speech: flash_store::NoteSpeech,
    /// The language a person picked for each field, from an editor that
    /// offers the choice; None leaves the note's as they are.
    pub langs: Option<flash_store::NoteLangs>,
}

pub struct McpSourcePatch {
    pub card_id: CardId,
    pub front_html: Option<String>,
    pub back_html: Option<String>,
    pub tags: Option<Vec<String>>,
    pub sibling_card_ids: Vec<CardId>,
    pub speech: flash_store::NoteSpeech,
}

/// What the editor opens with for an existing card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSeed {
    pub card_id: CardId,
    pub deck_id: DeckId,
    /// None: a standalone card the save will adopt into a new note.
    pub note_id: Option<NoteId>,
    pub note_type: NoteType,
    pub front_html: String,
    pub back_html: String,
    pub tags: Vec<String>,
    /// Live cards this save will touch (siblings included).
    pub sibling_count: usize,
    /// The language picked for each field (none for a standalone card).
    pub langs: flash_store::NoteLangs,
}

/// A note's sides and tags, checked as a save checks them
/// (`Services::note_fields`).
pub struct NoteFields {
    pub front_html: richtext::SanitizedHtml,
    pub back_html: richtext::SanitizedHtml,
    pub tags: Vec<String>,
}

pub(super) struct PreparedNote {
    pub(super) front_html: richtext::SanitizedHtml,
    pub(super) back_html: richtext::SanitizedHtml,
    pub(super) generated: Vec<GeneratedCard>,
    pub(super) tags: Vec<String>,
    /// Each field's speech blocks, when the writer gave them.
    pub(super) speech: flash_store::NoteSpeech,
    /// The language picked for each field; None leaves them.
    pub(super) langs: Option<flash_store::NoteLangs>,
}

impl PreparedNote {
    pub(super) fn save(
        &self,
        note_type: NoteType,
        cap: Option<u32>,
        origin: Option<i64>,
        now_ms: i64,
    ) -> NoteSave<'_> {
        NoteSave {
            note_type,
            front_html: &self.front_html,
            back_html: &self.back_html,
            generated: &self.generated,
            tags: &self.tags,
            cap,
            origin,
            speech: self.speech.clone(),
            langs: self.langs.clone(),
            now_ms,
        }
    }
}

/// The editor's seed fields as sanitized HTML, for a note made from what
/// a card already says.
fn sanitized_seed(seed: &EditorSeed) -> (richtext::SanitizedHtml, richtext::SanitizedHtml) {
    (
        richtext::sanitize_with_media(&seed.front_html).trimmed(),
        richtext::sanitize_with_media(&seed.back_html).trimmed(),
    )
}
