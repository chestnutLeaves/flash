//! Atomic connector edits and durable, user/client-scoped deletion approvals.
use std::collections::{HashMap, HashSet};

use flash_core::{CardId, DeckId, NoteId, RevisionId, UserId};
use rusqlite::{params, Connection, OptionalExtension};

use crate::notes::NoteType;
use crate::richtext::SanitizedHtml;
use crate::{NoteSave, Result, RevisionSource, Store, StoreError};
use crate::richtext::external_image_urls;

pub struct SourceEdit<'a> {
    pub card: CardId,
    pub snapshot: String,
    pub siblings: Vec<CardId>,
    /// The text the card has now, as the editor reads it: what a
    /// note-less card's new note is made with, so its old text becomes
    /// the note's first revision (`adopt_card_into_note_tx`).
    pub seed: (NoteType, &'a SanitizedHtml, &'a SanitizedHtml),
    pub save: NoteSave<'a>,
}

/// Computed inside the write transaction, so reporting cannot fail after commit.
pub struct McpMutationReceipt {
    pub sources: Vec<(NoteId, Vec<CardId>)>,
    /// For an update, one per source: the revision keeping the text the
    /// edit replaced, or None when the edit changed nothing. Empty for a
    /// creation.
    pub revisions: Vec<Option<RevisionId>>,
    pub created: u32,
    pub updated: u32,
    pub deck_totals: Vec<(i64, u32)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpDeletionReceipt {
    pub card_ids: Vec<i64>,
    /// The decks that went, by name: the deck and the decks under it.
    /// Empty for a deletion of cards, and in a receipt stored before a
    /// deck deletion covered its subdecks.
    #[serde(default)]
    pub decks: Vec<String>,
    pub deck_totals: Vec<(i64, u32)>,
    pub orphan_blobs: Vec<String>,
}

/// What a staged deletion would remove: what the person is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDeletionPreview {
    /// The cards the person can see, ascending.
    pub card_ids: Vec<i64>,
    /// The decks that would go, by name, outermost first; empty for a
    /// deletion of cards.
    pub decks: Vec<String>,
}

/// The deck and the decks under it as the approval pins them: ids
/// ascending, and names in the order the deck list shows them, each deck
/// followed by the decks under it. A deck that is not the user's is not
/// found.
fn pinned_decks(conn: &Connection, user: UserId, deck: DeckId) -> Result<(Vec<i64>, Vec<String>)> {
    let decks = super::decks::subtree(conn, user, deck)?;
    if decks.is_empty() {
        return Err(StoreError::NotFound("deck"));
    }
    let mut ids: Vec<i64> = decks.iter().map(|(id, _)| id.0).collect();
    ids.sort_unstable();
    let nodes: Vec<flash_core::deck_tree::Node<'_>> = decks
        .iter()
        .map(|(_, name)| flash_core::deck_tree::Node {
            name,
            due: 0,
            new: 0,
            paced: false,
        })
        .collect();
    let names = flash_core::deck_tree::build(&nodes, 0, 0)
        .into_iter()
        .map(|row| decks[row.index].1.clone())
        .collect();
    Ok((ids, names))
}

fn invalid(code: &str) -> StoreError {
    StoreError::Invalid(code.into())
}

fn ids(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<CardId>> {
    Ok(conn
        .prepare(sql)?
        .query_map(args, |r| Ok(CardId(r.get(0)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Imported cloze siblings use the same grouping as editor adoption.
fn siblings(conn: &Connection, user: UserId, card: CardId) -> Result<Vec<CardId>> {
    let (deck, note, source): (i64, Option<i64>, Option<String>) = conn.query_row(
        "SELECT deck_id, note_id, cloze_text FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
        params![card.0, user.raw()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).optional()?.ok_or(StoreError::NotFound("card"))?;
    if let Some(note) = note {
        ids(conn, "SELECT id FROM cards WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL ORDER BY id", params![note, user.raw()])
    } else if let Some(source) = source {
        // Include duplicate imports on reads too. The service marks ambiguous
        // sources read-only rather than making a whole listing fail.
        ids(conn, "SELECT id FROM cards WHERE deck_id = ?1 AND user_id = ?2 AND note_id IS NULL AND deleted_at IS NULL AND cloze_text = ?3 AND cloze_index IS NOT NULL ORDER BY id", params![deck, user.raw(), source])
    } else {
        Ok(vec![card])
    }
}

fn snapshot(conn: &Connection, user: UserId, card: CardId) -> Result<String> {
    let siblings = siblings(conn, user, card)?;
    let mut values = Vec::new();
    for id in siblings {
        let value = conn.query_row(
            "SELECT c.front, c.back, c.front_html, c.back_html, c.note_id, c.ord,
              c.cloze_text, c.cloze_index, c.type_answer, n.front_html, n.back_html,
              n.note_type FROM cards c LEFT JOIN notes n ON n.id=c.note_id
              WHERE c.id = ?1 AND c.user_id = ?2 AND c.deleted_at IS NULL",
            params![id.0, user.raw()],
            |r| {
                let mut fields = Vec::<Option<String>>::new();
                for i in [0, 1, 2, 3, 6, 8, 9, 10, 11] {
                    fields.push(r.get(i)?);
                }
                Ok(serde_json::json!([
                    id.0,
                    fields,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<i64>>(7)?
                ]))
            },
        )?;
        let tags = super::cards::card_tags(conn, user, id)?;
        values.push(serde_json::json!([value, tags]));
    }
    Ok(serde_json::Value::Array(values).to_string())
}

/// The cards of the decks the person can see, ascending.
fn live_deck_cards(conn: &Connection, user: UserId, decks: &[i64]) -> Result<Vec<i64>> {
    Ok(ids(
        conn,
        "SELECT id FROM cards WHERE user_id = ?1 AND deleted_at IS NULL
           AND deck_id IN (SELECT value FROM json_each(?2)) ORDER BY id",
        params![user.raw(), serde_json::json!(decks).to_string()],
    )?
    .iter()
    .map(|c| c.0)
    .collect())
}

/// Every card row of the decks, soft-deleted ones included, ascending:
/// what deleting them erases, and so what an approval pins.
fn all_deck_cards(conn: &Connection, user: UserId, decks: &[i64]) -> Result<Vec<i64>> {
    Ok(ids(
        conn,
        "SELECT id FROM cards WHERE user_id = ?1
           AND deck_id IN (SELECT value FROM json_each(?2)) ORDER BY id",
        params![user.raw(), serde_json::json!(decks).to_string()],
    )?
    .iter()
    .map(|c| c.0)
    .collect())
}

pub fn deck_totals(conn: &Connection, user: UserId, decks: &[DeckId]) -> Result<Vec<(i64, u32)>> {
    let mut seen = HashSet::new();
    let mut totals = Vec::new();
    for deck in decks {
        if seen.insert(deck.0) {
            let count = conn.query_row(
                "SELECT COUNT(*) FROM cards WHERE deck_id = ?1 AND user_id = ?2 AND deleted_at IS NULL",
                params![deck.0, user.raw()],
                |r| r.get::<_, u32>(0),
            )?;
            totals.push((deck.0, count));
        }
    }
    totals.sort_unstable();
    Ok(totals)
}

fn check_media(conn: &Connection, user: UserId, save: &NoteSave<'_>) -> Result<()> {
    for html in [save.front_html, save.back_html] {
        for id in crate::richtext::media_ids(html) {
            let owned: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM media WHERE id = ?1 AND user_id = ?2)",
                params![id, user.raw()],
                |r| r.get(0),
            )?;
            if !owned {
                return Err(invalid("media is not one of your uploads"));
            }
        }
    }
    Ok(())
}

impl Store {
    pub fn mcp_source_state(&self, user: UserId, card: CardId) -> Result<(String, Vec<CardId>)> {
        let conn = self.conn();
        Ok((snapshot(&conn, user, card)?, siblings(&conn, user, card)?))
    }

    pub fn create_notes_batch(
        &self,
        user: UserId,
        name: &str,
        saves: &[NoteSave<'_>],
        now: i64,
    ) -> Result<McpMutationReceipt> {
        if saves.is_empty() || saves.len() > 500 || name.trim().is_empty() {
            return Err(invalid("invalid batch or deck name"));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // A name is a path here as everywhere: `Bio::Cells` lands in the
        // subdeck, made with the decks it sits in when it is new.
        let deck = match super::decks::find_deck(&tx, user, name)? {
            Some(deck) => deck,
            None => super::decks::insert_deck_path_tx(&tx, user, name, "", now)?,
        };
        let mut result = Vec::new();
        for save in saves {
            check_media(&tx, user, save)?;
            let created = self.create_note_tx(&tx, user, deck, save)?;
            for (side, html) in [("front", save.front_html.as_str()), ("back", save.back_html.as_str())]
            {
                for url in external_image_urls(html) {
                    let now = save.now_ms;
                    tx.execute(
                        "INSERT INTO external_media_refs
                           (user_id, note_id, side, source_url, status, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)
                         ON CONFLICT(user_id, note_id, side, source_url) DO NOTHING",
                        params![user.raw(), created.0.0, side, url, now],
                    )?;
                    let payload = serde_json::json!({
                        "note_id": created.0.0,
                        "side": side,
                        "url": url,
                    })
                    .to_string();
                    let dedupe = format!("external_media:{}:{}:{}:{}", user.raw(), created.0.0, side, url);
                    crate::repo::jobs::enqueue_job(
                        &tx,
                        &crate::repo::jobs::NewJob {
                            kind: crate::repo::jobs::JOB_EXTERNAL_MEDIA,
                            dedupe_key: Some(&dedupe),
                            user: Some(user),
                            payload: &payload,
                            run_at: now,
                        },
                        now,
                    )?;
                }
            }
            result.push(created);
        }
        let receipt = McpMutationReceipt {
            created: result.iter().map(|(_, ids)| ids.len() as u32).sum(),
            updated: 0,
            sources: result,
            revisions: Vec::new(),
            deck_totals: deck_totals(&tx, user, &[deck])?,
        };
        tx.commit()?;
        Ok(receipt)
    }

    pub fn update_sources_batch(
        &self,
        user: UserId,
        edits: &[SourceEdit<'_>],
    ) -> Result<McpMutationReceipt> {
        if edits.is_empty() || edits.len() > 500 {
            return Err(invalid("invalid batch size"));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        // Each card the batch reaches, with the edit that named it.
        let mut touched = HashMap::new();
        for edit in edits {
            if snapshot(&tx, user, edit.card)? != edit.snapshot {
                return Err(invalid("source_changed: read the cards again"));
            }
            if siblings(&tx, user, edit.card)? != edit.siblings {
                return Err(invalid("siblings_changed: read the cards again"));
            }
            for id in &edit.siblings {
                if let Some(first) = touched.insert(id.0, edit.card.0) {
                    return Err(invalid(&format!(
                        "duplicate_source: cards {first} and {} share one source: send one \
                         edit for it",
                        edit.card.0
                    )));
                }
            }
        }
        let mut result = Vec::new();
        let mut revisions = Vec::new();
        let mut decks = Vec::new();
        for edit in edits {
            check_media(&tx, user, &edit.save)?;
            let (seed_type, seed_front, seed_back) = edit.seed;
            let note = self.adopt_card_into_note_tx(
                &tx,
                user,
                edit.card,
                seed_type,
                seed_front,
                seed_back,
                edit.save.now_ms,
            )?;
            let slots:Vec<(u32,i64)>=tx.prepare("SELECT ord,id FROM cards WHERE note_id = ?1 AND user_id = ?2 AND deleted_at IS NULL")?
                .query_map(params![note.0,user.raw()], |r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
            let removed: Vec<i64> = slots
                .iter()
                .filter(|(ord, _)| !edit.save.generated.iter().any(|g| g.ord == *ord))
                .map(|(_, id)| *id)
                .collect();
            if !removed.is_empty() {
                return Err(invalid(&format!(
                    "implicit_card_removal: delete these cards explicitly first: {removed:?}"
                )));
            }
            let update = self.update_note_tx(&tx, user, note, &edit.save, RevisionSource::Mcp)?;
            result.push((note, update.cards));
            revisions.push(update.revision);
            let deck = tx.query_row(
                "SELECT deck_id FROM notes WHERE id = ?1 AND user_id = ?2",
                params![note.0, user.raw()],
                |r| r.get::<_, i64>(0),
            )?;
            decks.push(DeckId(deck));
        }
        let total: u32 = result.iter().map(|(_, ids)| ids.len() as u32).sum();
        let receipt = McpMutationReceipt {
            sources: result,
            revisions,
            updated: touched.len() as u32,
            created: total - touched.len() as u32,
            deck_totals: deck_totals(&tx, user, &decks)?,
        };
        tx.commit()?;
        Ok(receipt)
    }

    /// Stage an exact deletion set. A deck's stored snapshot includes its
    /// soft-deleted rows, because deleting the deck erases them and any
    /// change to them must void the approval. The returned IDs are the
    /// cards the person can see: those are what they are asked about.
    ///
    /// A deck goes with the decks under it, so the snapshot covers their
    /// cards too, and the set of decks is pinned beside it: a subdeck made
    /// or moved in afterwards may hold no card at all, and would otherwise
    /// ride along on an approval that never mentioned it.
    #[allow(clippy::too_many_arguments)]
    pub fn stage_mcp_delete(
        &self,
        user: UserId,
        client: &str,
        target: &str,
        deck: Option<DeckId>,
        cards: &[CardId],
        hash: &str,
        now: i64,
    ) -> Result<McpDeletionPreview> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM mcp_confirmations WHERE user_id = ?2 AND token_hash IN (SELECT token_hash FROM mcp_confirmations WHERE expires_ms<?1 AND user_id = ?2 LIMIT 1000)",params![now-86_400_000,user.raw()])?;
        let count:u32=tx.query_row("SELECT COUNT(*) FROM mcp_confirmations WHERE user_id = ?1 AND expires_ms>?2 AND result IS NULL AND invalidated = 0",params![user.raw(),now],|r|r.get(0))?;
        if count >= 32 {
            return Err(invalid("too_many_pending_confirmations"));
        }
        let pinned = deck.map(|deck| pinned_decks(&tx, user, deck)).transpose()?;
        let selected = if let Some((decks, _)) = &pinned {
            all_deck_cards(&tx, user, decks)?
        } else {
            if cards.is_empty() || cards.len() > 500 {
                return Err(invalid("invalid batch size"));
            }
            let mut seen = HashSet::new();
            for card in cards {
                if !seen.insert(card.0) {
                    return Err(invalid("duplicate card id"));
                }
                let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL)",params![card.0,user.raw()],|r|r.get(0))?;
                if !owned {
                    return Err(StoreError::NotFound("card"));
                }
            }
            let mut selected: Vec<i64> = cards.iter().map(|c| c.0).collect();
            selected.sort_unstable();
            selected
        };
        tx.execute(
            "INSERT INTO mcp_confirmations(token_hash,user_id,client_id,target,deck_id,deck_ids,card_ids,expires_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                hash,
                user.raw(),
                client,
                target,
                deck.map(|d| d.0),
                pinned.as_ref().map(|(decks, _)| serde_json::json!(decks).to_string()),
                serde_json::json!(selected).to_string(),
                now + 600_000
            ],
        )?;
        let preview = match pinned {
            Some((decks, names)) => McpDeletionPreview {
                card_ids: live_deck_cards(&tx, user, &decks)?,
                decks: names,
            },
            None => McpDeletionPreview {
                card_ids: selected,
                decks: Vec::new(),
            },
        };
        tx.commit()?;
        Ok(preview)
    }

    pub fn confirm_mcp_delete(
        &self,
        user: UserId,
        client: &str,
        target: &str,
        hash: &str,
        now: i64,
    ) -> Result<McpDeletionReceipt> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        type Pending = (
            String,
            Option<i64>,
            Option<String>,
            String,
            i64,
            Option<String>,
            bool,
        );
        let pending:Option<Pending>=tx.query_row("SELECT target,deck_id,deck_ids,card_ids,expires_ms,result,invalidated FROM mcp_confirmations WHERE token_hash = ?1 AND user_id = ?2 AND client_id = ?3",params![hash,user.raw(),client],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
        let Some((expected, deck, deck_ids, raw, expires, result, invalidated)) = pending else {
            return Err(invalid("confirmation_invalid"));
        };
        if expected != target {
            return Err(invalid("confirmation_target_mismatch"));
        }
        if let Some(result) = result {
            return serde_json::from_str(&result)
                .map_err(|_| invalid("invalid stored confirmation result"));
        }
        // Hard-delete triggers invalidate the original resource identity,
        // even if SQLite later reuses all of its IDs. A completed receipt
        // takes precedence: our own deletion fires those same triggers.
        if invalidated {
            return Err(invalid("confirmation_target_changed"));
        }
        if expires <= now {
            return Err(invalid("confirmation_expired"));
        }
        let selected: Vec<i64> =
            serde_json::from_str(&raw).map_err(|_| invalid("invalid stored confirmation"))?;
        let mut decks = Vec::new();
        let mut deck_names = Vec::new();
        let mut orphan_blobs = Vec::new();
        let mut deleted = selected.clone();
        if let Some(deck) = deck {
            // What was approved: the decks pinned at staging (a row staged
            // before decks nested pinned the named deck alone) and every
            // card row in them. Either one different now is a different
            // deletion, and needs asking again.
            let approved: Vec<i64> = match deck_ids {
                Some(raw) => serde_json::from_str(&raw)
                    .map_err(|_| invalid("invalid stored confirmation"))?,
                None => vec![deck],
            };
            let Ok((current, names)) = pinned_decks(&tx, user, DeckId(deck)) else {
                return Err(invalid("confirmation_target_changed"));
            };
            if current != approved || all_deck_cards(&tx, user, &current)? != selected {
                return Err(invalid("confirmation_target_changed"));
            }
            // The receipt names what the preview named: the live cards.
            deleted = live_deck_cards(&tx, user, &current)?;
            orphan_blobs = self.delete_deck_tx(&tx, user, DeckId(deck))?.orphan_blobs;
            decks.extend(current.into_iter().map(DeckId));
            deck_names = names;
        } else {
            for id in &selected {
                let deck:Option<i64>=tx.query_row("SELECT deck_id FROM cards WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL",params![id,user.raw()],|r|r.get(0)).optional()?;
                let Some(deck) = deck else {
                    return Err(invalid("confirmation_target_changed"));
                };
                decks.push(DeckId(deck));
                self.delete_card_tx(&tx, user, CardId(*id), now)?;
            }
        }
        let receipt = McpDeletionReceipt {
            card_ids: deleted,
            decks: deck_names,
            deck_totals: deck_totals(&tx, user, &decks)?,
            orphan_blobs,
        };
        let result = serde_json::to_string(&receipt)
            .map_err(|_| invalid("cannot serialize deletion result"))?;
        tx.execute(
            "UPDATE mcp_confirmations SET result = ?1 WHERE token_hash = ?2 AND user_id = ?3 AND client_id = ?4",
            params![result, hash, user.raw(), client],
        )?;
        tx.commit()?;
        Ok(receipt)
    }
}
