//! Ceremonies that must survive a restart (`V031_DURABLE_CEREMONIES`): an
//! OAuth consent screen or another sign-in round trip in progress. The
//! payload is the ceremony's own JSON; the account it is for, when there
//! is one, is its own column.

use flash_core::UserId;
use rusqlite::{params, OptionalExtension};

use crate::{Result, Store};

/// Live ceremonies past which a new one is refused: several start
/// unauthenticated, so a flood must not grow the table without bound.
pub const DURABLE_CEREMONY_CAP: i64 = 20_000;

impl Store {
    /// Parks a ceremony. False when the table is full of live ones.
    pub fn put_durable_ceremony(
        &self,
        key_hash: &str,
        kind: &str,
        user: Option<UserId>,
        payload: &str,
        expires_ms: i64,
        now_ms: i64,
    ) -> Result<bool> {
        self.with_tx(|tx| {
            tx.execute("DELETE FROM ceremonies WHERE expires_at <= ?1", [now_ms])?;
            let live: i64 = tx.query_row("SELECT COUNT(*) FROM ceremonies", [], |r| r.get(0))?;
            if live >= DURABLE_CEREMONY_CAP {
                return Ok(false);
            }
            tx.execute(
                "INSERT OR REPLACE INTO ceremonies (key_hash, kind, user_id, payload, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![key_hash, kind, user.map(UserId::raw), payload, expires_ms],
            )?;
            Ok(true)
        })
    }

    /// Takes a live ceremony of `kind`: its payload and account. It is
    /// gone after, whatever it held, so it is used once.
    pub fn take_durable_ceremony(
        &self,
        key_hash: &str,
        kind: &str,
        now_ms: i64,
    ) -> Result<Option<(String, Option<UserId>)>> {
        Ok(self
            .conn()
            .query_row(
                "DELETE FROM ceremonies WHERE key_hash = ?1 AND kind = ?2
                 RETURNING payload, user_id, expires_at",
                params![key_hash, kind],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?.map(UserId::from_db),
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .filter(|(_, _, expires)| *expires > now_ms)
            .map(|(payload, user, _)| (payload, user)))
    }

    /// Marks a live ceremony of `kind` as answered: it becomes `as_kind`
    /// and lives no later than `until_ms`, keeping its payload. Its
    /// payload and account when it was live; None otherwise. One
    /// statement, so of two answers to the same screen exactly one
    /// claims it, and the other finds the answered row with
    /// [`Store::answered_ceremony`].
    pub fn claim_durable_ceremony(
        &self,
        key_hash: &str,
        kind: &str,
        as_kind: &str,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<Option<(String, Option<UserId>)>> {
        Ok(self
            .conn()
            .query_row(
                "UPDATE ceremonies SET kind = ?3, expires_at = MIN(expires_at, ?4)
                 WHERE key_hash = ?1 AND kind = ?2 AND expires_at > ?5
                 RETURNING payload, user_id",
                params![key_hash, kind, as_kind, until_ms, now_ms],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?.map(UserId::from_db),
                    ))
                },
            )
            .optional()?)
    }

    /// A live ceremony already claimed as one of `kinds`: which, its
    /// payload and account.
    pub fn answered_ceremony(
        &self,
        key_hash: &str,
        kinds: &[&str],
        now_ms: i64,
    ) -> Result<Option<(String, String, Option<UserId>)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT kind, payload, user_id FROM ceremonies
             WHERE key_hash = ?1 AND expires_at > ?2",
        )?;
        let row = stmt
            .query_row(params![key_hash, now_ms], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?.map(UserId::from_db),
                ))
            })
            .optional()?;
        Ok(row.filter(|(kind, _, _)| kinds.contains(&kind.as_str())))
    }

    /// Rewrites a live ceremony's payload through `f`, which gets the
    /// current one and returns the new one. False when there is none.
    pub fn update_durable_ceremony(
        &self,
        key_hash: &str,
        kind: &str,
        now_ms: i64,
        f: impl FnOnce(&str) -> Option<String>,
    ) -> Result<bool> {
        self.with_tx(|tx| {
            let current: Option<String> = tx
                .query_row(
                    "SELECT payload FROM ceremonies
                     WHERE key_hash = ?1 AND kind = ?2 AND expires_at > ?3",
                    params![key_hash, kind, now_ms],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(next) = current.as_deref().and_then(f) else {
                return Ok(false);
            };
            tx.execute(
                "UPDATE ceremonies SET payload = ?3 WHERE key_hash = ?1 AND kind = ?2",
                params![key_hash, kind, next],
            )?;
            Ok(true)
        })
    }

    /// Ceremonies nobody finished go once they expire.
    pub fn purge_expired_ceremonies(&self, now_ms: i64) -> Result<usize> {
        Ok(self
            .conn()
            .execute("DELETE FROM ceremonies WHERE expires_at <= ?1", [now_ms])?)
    }
}
