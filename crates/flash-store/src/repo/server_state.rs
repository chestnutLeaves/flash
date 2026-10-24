//! What the server keeps about itself across restarts
//! (`V031_DURABLE_CEREMONIES`): assistants' MCP sessions, and a few named
//! facts (a signing key, whether the last stop was clean).

use rusqlite::{params, OptionalExtension};

use crate::{Result, Store};

impl Store {
    /// The `initialize` an MCP session began with, by the session id's
    /// hash. Reading it counts as use, so a session in use stays.
    pub fn load_mcp_session(&self, id_hash: &str, now_ms: i64) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "UPDATE mcp_sessions SET updated_at = ?2 WHERE id_hash = ?1 RETURNING state",
                params![id_hash, now_ms],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn store_mcp_session(&self, id_hash: &str, state: &str, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO mcp_sessions (id_hash, state, updated_at)
             VALUES (?1, ?2, ?3)",
            params![id_hash, state, now_ms],
        )?;
        Ok(())
    }

    pub fn delete_mcp_session(&self, id_hash: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM mcp_sessions WHERE id_hash = ?1", [id_hash])?;
        Ok(())
    }

    /// Sessions unused since `before_ms` go; their clients start new ones.
    pub fn purge_idle_mcp_sessions(&self, before_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM mcp_sessions WHERE updated_at < ?1",
            [before_ms],
        )?)
    }

    /// The named fact, when it is there.
    pub fn server_state(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT value FROM server_state WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The named fact, written once: the first value stands and is
    /// returned, so two processes starting together agree on it.
    pub fn server_state_or_insert(&self, name: &str, value: &str) -> Result<String> {
        self.with_tx(|tx| {
            tx.execute(
                "INSERT OR IGNORE INTO server_state (name, value) VALUES (?1, ?2)",
                params![name, value],
            )?;
            Ok(tx.query_row(
                "SELECT value FROM server_state WHERE name = ?1",
                [name],
                |r| r.get(0),
            )?)
        })
    }

    pub fn set_server_state(&self, name: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO server_state (name, value) VALUES (?1, ?2)",
            params![name, value],
        )?;
        Ok(())
    }

    /// Removes the named fact, saying whether it was there.
    pub fn take_server_state(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM server_state WHERE name = ?1", [name])?
            > 0)
    }
}
