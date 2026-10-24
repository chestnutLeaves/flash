//! The commit marker of an import (`V030_IMPORT_COMMITS`): claimed before
//! the first card goes in, finished with the count once they all have.

use flash_core::UserId;
use rusqlite::{params, OptionalExtension};

use crate::{Result, Store};

/// What claiming an import's commit found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportClaim {
    /// Nobody committed this preview yet: go ahead.
    New,
    /// It went through before, with this many cards (a retry whose first
    /// reply was lost).
    Finished(u32),
    /// A commit started and never finished: the process died part way,
    /// and some of its cards may already be in.
    Unfinished,
}

impl Store {
    /// Claims the commit of the preview `token_hash` names, or says why
    /// it can't be: done already, or cut off part way.
    pub fn claim_import(&self, user: UserId, token_hash: &str, now_ms: i64) -> Result<ImportClaim> {
        self.with_tx(|tx| {
            let existing: Option<Option<i64>> = tx
                .query_row(
                    "SELECT cards FROM import_commits WHERE user_id = ?1 AND token_hash = ?2",
                    params![user.raw(), token_hash],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(match existing {
                Some(Some(cards)) => ImportClaim::Finished(cards.max(0) as u32),
                Some(None) => ImportClaim::Unfinished,
                None => {
                    tx.execute(
                        "INSERT INTO import_commits (user_id, token_hash, started_at)
                         VALUES (?1, ?2, ?3)",
                        params![user.raw(), token_hash, now_ms],
                    )?;
                    ImportClaim::New
                }
            })
        })
    }

    /// Every card is in.
    pub fn finish_import(
        &self,
        user: UserId,
        token_hash: &str,
        cards: u32,
        now_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE import_commits SET finished_at = ?3, cards = ?4
             WHERE user_id = ?1 AND token_hash = ?2",
            params![user.raw(), token_hash, now_ms, cards],
        )?;
        Ok(())
    }

    /// The commit failed with an error, not a crash: the claim goes, so
    /// the person can try again.
    pub fn release_import(&self, user: UserId, token_hash: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM import_commits
             WHERE user_id = ?1 AND token_hash = ?2 AND finished_at IS NULL",
            params![user.raw(), token_hash],
        )?;
        Ok(())
    }

    /// Markers older than `before_ms` go: their previews expired long ago.
    pub fn purge_import_commits(&self, before_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM import_commits WHERE started_at < ?1",
            [before_ms],
        )?)
    }
}
