//! Atomic application of asynchronous external-image localization results.

use flash_core::{MediaId, NoteId, UserId};
use rusqlite::{params, OptionalExtension};

use crate::{Result, Store, StoreError};

impl Store {
    /// Applies one localization result only while the note still contains the
    /// exact source URL. This prevents a late worker from overwriting a user
    /// edit made after the MCP request returned.
    pub fn apply_external_media(
        &self,
        user: UserId,
        note: NoteId,
        side: &str,
        source_url: &str,
        media: Option<MediaId>,
        error: Option<&str>,
        now: i64,
    ) -> Result<bool> {
        if !matches!(side, "front" | "back") {
            return Err(StoreError::Invalid("invalid external media side".into()));
        }
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let current: Option<String> = tx
            .query_row(
                &format!("SELECT {side}_html FROM notes WHERE id = ?1 AND user_id = ?2 AND deleted_at IS NULL"),
                params![note.0, user.raw()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            tx.execute(
                "UPDATE external_media_refs SET status = 'failed', last_error = 'note no longer exists', updated_at = ?1
                 WHERE user_id = ?2 AND note_id = ?3 AND side = ?4 AND source_url = ?5",
                params![now, user.raw(), note.0, side, source_url],
            )?;
            tx.commit()?;
            return Ok(false);
        };
        if !crate::richtext::external_image_urls(&current)
            .iter()
            .any(|url| url == source_url)
        {
            tx.execute(
                "UPDATE external_media_refs SET status = 'failed', last_error = 'reference was edited or removed', updated_at = ?1
                 WHERE user_id = ?2 AND note_id = ?3 AND side = ?4 AND source_url = ?5",
                params![now, user.raw(), note.0, side, source_url],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        let html = crate::richtext::replace_external_image(
            &current,
            source_url,
            media.map(|id| id.0),
        );
        tx.execute(
            &format!("UPDATE notes SET {side}_html = ?1, updated_at = ?2 WHERE id = ?3 AND user_id = ?4"),
            params![html.as_str(), now, note.0, user.raw()],
        )?;
        tx.execute(
            &format!("UPDATE cards SET {side}_html = ?1, updated_at = ?2 WHERE note_id = ?3 AND user_id = ?4 AND deleted_at IS NULL"),
            params![html.as_str(), now, note.0, user.raw()],
        )?;
        if let Some(media) = media {
            tx.execute(
                "INSERT OR IGNORE INTO card_media (card_id, media_id)
                 SELECT c.id, m.id FROM cards c JOIN media m ON m.id = ?1
                 WHERE c.note_id = ?2 AND c.user_id = ?3 AND m.user_id = ?3 AND c.deleted_at IS NULL",
                params![media.0, note.0, user.raw()],
            )?;
        }
        tx.execute(
            "UPDATE external_media_refs SET status = ?1, media_id = ?2, last_error = ?3, updated_at = ?4
             WHERE user_id = ?5 AND note_id = ?6 AND side = ?7 AND source_url = ?8",
            params![if media.is_some() { "imported" } else { "failed" }, media.map(|id| id.0), error, now, user.raw(), note.0, side, source_url],
        )?;
        tx.commit()?;
        Ok(true)
    }
}
