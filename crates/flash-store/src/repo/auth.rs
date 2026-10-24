//! Web sessions, enrollment invites, and passkey credentials.
//! Secrets never touch the database in the clear: callers pass SHA-256
//! hex of the cookie/invite token, produced in the server crate.

use flash_core::UserId;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{exec_expect_row, Result, Store, StoreError};

#[derive(Debug, Clone)]
pub struct StoredPasskey {
    pub id: i64,
    pub user_id: UserId,
    /// serde-serialized webauthn_rs::prelude::Passkey (JSON).
    pub passkey_json: String,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// A valid session cookie: whose it is, and which session it names.
#[derive(Debug, Clone)]
pub struct WebSessionHit {
    pub id: i64,
    pub user: UserId,
    pub role: String,
    pub last_used_at: Option<i64>,
    /// Opened by an emailed sign-in link, not a password or passkey
    /// (`create_link_session`).
    pub via_link: bool,
    /// The account's language as learned or chosen (`account_locale`),
    /// and whether it was chosen; read with the session so a page asks
    /// no more of the store to speak it.
    pub locale: Option<String>,
    pub locale_chosen: bool,
    /// The time zone the account studies in, only when it was learned or
    /// chosen (a default says nothing about where someone is).
    pub zone: Option<String>,
}

/// One open browser session, as the devices list shows it.
#[derive(Debug, Clone)]
pub struct WebSessionRow {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    /// The session of the browser that asked.
    pub current: bool,
    /// Opened by an emailed sign-in link.
    pub via_link: bool,
}

#[derive(Debug, Clone)]
pub struct InviteInfo {
    pub display_name: String,
    pub role: String,
}

// The writes an enrollment or a reset makes, on a connection or a
// transaction the caller holds (`Store::with_tx`); each `Store` method of
// the same name is its `_tx` form on the store's own connection.

/// `Store::lookup_password_reset` on `conn`.
pub fn lookup_password_reset_tx(
    conn: &Connection,
    token_hash: &str,
    now_ms: i64,
) -> Result<Option<UserId>> {
    Ok(conn
        .query_row(
            "SELECT user_id FROM password_resets
             WHERE token_hash = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![token_hash, now_ms],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .map(UserId::from_db))
}

/// `Store::consume_password_reset` on `conn`.
pub fn consume_password_reset_tx(
    conn: &Connection,
    token_hash: &str,
    now_ms: i64,
) -> Result<UserId> {
    let user = lookup_password_reset_tx(conn, token_hash, now_ms)?
        .ok_or(StoreError::NotFound("password reset"))?;
    exec_expect_row(
        conn,
        "password reset",
        "UPDATE password_resets SET used_at = ?1
         WHERE token_hash = ?2 AND used_at IS NULL AND expires_at > ?1",
        params![now_ms, token_hash],
    )?;
    Ok(user)
}

/// `Store::delete_unused_password_resets` on `conn`.
pub fn delete_unused_password_resets_tx(conn: &Connection, user: UserId) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM password_resets WHERE user_id = ?1 AND used_at IS NULL",
        [user.raw()],
    )?)
}

/// `Store::lookup_invite` on `conn`.
pub fn lookup_invite_tx(
    conn: &Connection,
    token_hash: &str,
    now_ms: i64,
) -> Result<Option<InviteInfo>> {
    Ok(conn
        .query_row(
            "SELECT display_name, role FROM invites
             WHERE token_hash = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![token_hash, now_ms],
            |r| {
                Ok(InviteInfo {
                    display_name: r.get(0)?,
                    role: r.get(1)?,
                })
            },
        )
        .optional()?)
}

/// `Store::consume_invite` on `conn`.
pub fn consume_invite_tx(conn: &Connection, token_hash: &str, now_ms: i64) -> Result<()> {
    exec_expect_row(
        conn,
        "invite",
        "UPDATE invites SET used_at = ?1
         WHERE token_hash = ?2 AND used_at IS NULL AND expires_at > ?1",
        params![now_ms, token_hash],
    )
}

/// `Store::add_passkey` on `conn`.
pub fn add_passkey_tx(
    conn: &Connection,
    user: UserId,
    cred_id: &[u8],
    passkey_json: &str,
    label: &str,
    now_ms: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO webauthn_credentials (user_id, cred_id, passkey, label, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user.raw(), cred_id, passkey_json, label, now_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

/// A `webauthn_credentials` row selected as `id, user_id, passkey, label,
/// created_at, last_used_at`.
fn passkey_row(r: &rusqlite::Row) -> rusqlite::Result<StoredPasskey> {
    Ok(StoredPasskey {
        id: r.get(0)?,
        user_id: UserId::from_db(r.get(1)?),
        passkey_json: r.get(2)?,
        label: r.get(3)?,
        created_at: r.get(4)?,
        last_used_at: r.get(5)?,
    })
}

impl Store {
    // ---- Web sessions ----

    /// Opens a browser session; `label` names the browser in the
    /// devices list ("Chrome on Windows").
    pub fn create_web_session(
        &self,
        id_hash: &str,
        user: UserId,
        label: &str,
        now_ms: i64,
        expires_ms: i64,
    ) -> Result<()> {
        self.insert_web_session(id_hash, user, label, now_ms, expires_ms, false)
    }

    /// Opens a browser session from an emailed sign-in link: one that may
    /// study but signs in again before changing how the account is
    /// reached.
    pub fn create_link_session(
        &self,
        id_hash: &str,
        user: UserId,
        label: &str,
        now_ms: i64,
        expires_ms: i64,
    ) -> Result<()> {
        self.insert_web_session(id_hash, user, label, now_ms, expires_ms, true)
    }

    fn insert_web_session(
        &self,
        id_hash: &str,
        user: UserId,
        label: &str,
        now_ms: i64,
        expires_ms: i64,
        via_link: bool,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO sessions (id_hash, user_id, label, created_at, expires_at, via_link)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id_hash, user.raw(), label, now_ms, expires_ms, via_link],
        )?;
        Ok(())
    }

    /// Resolves a session cookie hash to its session, if valid and
    /// unexpired.
    pub fn lookup_web_session(&self, id_hash: &str, now_ms: i64) -> Result<Option<WebSessionHit>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT s.id, s.user_id, u.role, s.last_used_at, s.via_link,
                        st.locale, st.locale_source,
                        CASE WHEN st.timezone_source <> 'default' THEN st.timezone END
                 FROM sessions s
                 JOIN users u ON u.id = s.user_id
                 LEFT JOIN user_settings st ON st.user_id = s.user_id
                 WHERE s.id_hash = ?1 AND s.expires_at > ?2",
                params![id_hash, now_ms],
                |r| {
                    Ok(WebSessionHit {
                        id: r.get(0)?,
                        user: UserId::from_db(r.get(1)?),
                        role: r.get(2)?,
                        last_used_at: r.get(3)?,
                        via_link: r.get(4)?,
                        locale: r.get(5)?,
                        locale_chosen: r.get::<_, Option<String>>(6)?.as_deref() == Some("chosen"),
                        zone: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    /// Records that a session was used. Callers write it now and then,
    /// not per request: one SQLite connection serves every request.
    pub fn touch_web_session(&self, id: i64, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE sessions SET last_used_at = ?1 WHERE id = ?2",
            params![now_ms, id],
        )?;
        Ok(())
    }

    /// The user's open browser sessions, newest first; `current_hash`
    /// is the cookie of the browser asking, whose row says so.
    pub fn list_web_sessions(
        &self,
        user: UserId,
        current_hash: Option<&str>,
        now_ms: i64,
    ) -> Result<Vec<WebSessionRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at, last_used_at, id_hash IS ?3, via_link FROM sessions
             WHERE user_id = ?1 AND expires_at > ?2
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user.raw(), now_ms, current_hash], |r| {
            Ok(WebSessionRow {
                id: r.get(0)?,
                label: r.get(1)?,
                created_at: r.get(2)?,
                last_used_at: r.get(3)?,
                current: r.get(4)?,
                via_link: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete_web_session(&self, id_hash: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM sessions WHERE id_hash = ?1", [id_hash])?;
        Ok(())
    }

    /// Signs one of the user's own browsers out (the devices list).
    pub fn delete_web_session_by_id(&self, user: UserId, id: i64) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "session",
            "DELETE FROM sessions WHERE id = ?1 AND user_id = ?2",
            params![id, user.raw()],
        )
    }

    /// Drops sessions past their expiry. Housekeeping for the scheduler:
    /// a lookup already refuses them.
    pub fn purge_expired_web_sessions(&self, now_ms: i64) -> Result<usize> {
        Ok(self
            .conn()
            .execute("DELETE FROM sessions WHERE expires_at <= ?1", [now_ms])?)
    }

    /// Signs the user out everywhere, optionally sparing one session
    /// (password change keeps the session that made it), and ends every
    /// other way back in: unused reset links and every extension's
    /// `before_delete_sessions`.
    pub fn delete_sessions_for_user(&self, user: UserId, keep_hash: Option<&str>) -> Result<usize> {
        self.with_tx(|tx| self.delete_sessions_for_user_tx(tx, user, keep_hash))
    }

    /// `delete_sessions_for_user` inside the caller's transaction.
    pub fn delete_sessions_for_user_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        user: UserId,
        keep_hash: Option<&str>,
    ) -> Result<usize> {
        for ext in &self.extensions {
            ext.before_delete_sessions(tx, user)?;
        }
        // A saved reset link is not a second chance: after a reset, the
        // other links this user may hold (up to the daily cap) die too.
        delete_unused_password_resets_tx(tx, user)?;
        Ok(tx.execute(
            "DELETE FROM sessions WHERE user_id = ?1 AND (?2 IS NULL OR id_hash != ?2)",
            params![user.raw(), keep_hash],
        )?)
    }

    // ---- Password resets ----
    // Same posture as invites: token stored SHA-256-only, TTL, single-use
    // burn via used_at. Resets target an existing user, so they live in
    // their own table rather than overloading invites.

    pub fn create_password_reset(
        &self,
        token_hash: &str,
        user: UserId,
        now_ms: i64,
        expires_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO password_resets (token_hash, user_id, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![token_hash, user.raw(), now_ms, expires_ms],
        )?;
        Ok(())
    }

    /// Valid (unexpired, unused) reset for this token hash, if any.
    pub fn lookup_password_reset(&self, token_hash: &str, now_ms: i64) -> Result<Option<UserId>> {
        lookup_password_reset_tx(&self.conn(), token_hash, now_ms)
    }

    /// Burns the reset; errors if already used or expired (single-use).
    pub fn consume_password_reset(&self, token_hash: &str, now_ms: i64) -> Result<UserId> {
        consume_password_reset_tx(&self.conn(), token_hash, now_ms)
    }

    /// Reset emails sent to this user since `since_ms` (send cooldown).
    pub fn recent_password_resets(&self, user: UserId, since_ms: i64) -> Result<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM password_resets WHERE user_id = ?1 AND created_at > ?2",
            params![user.raw(), since_ms],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Every reset link the user has not used. A credential change and
    /// every sign-out-everywhere reach this: a link mailed before must not
    /// still set the password after.
    pub fn delete_unused_password_resets(&self, user: UserId) -> Result<usize> {
        delete_unused_password_resets_tx(&self.conn(), user)
    }

    /// Rollback for a failed send (mirrors delete_invite).
    pub fn delete_password_reset(&self, token_hash: &str) -> Result<()> {
        self.conn().execute(
            "DELETE FROM password_resets WHERE token_hash = ?1",
            [token_hash],
        )?;
        Ok(())
    }

    pub fn purge_expired_password_resets(&self, now_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM password_resets WHERE expires_at <= ?1",
            [now_ms],
        )?)
    }

    // ---- Invites ----

    #[allow(clippy::too_many_arguments)]
    pub fn create_invite(
        &self,
        token_hash: &str,
        display_name: &str,
        role: &str,
        created_by: Option<UserId>,
        expires_ms: i64,
        now_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO invites (token_hash, display_name, role, created_by, expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                token_hash,
                display_name,
                role,
                created_by.map(UserId::raw),
                expires_ms,
                now_ms
            ],
        )?;
        Ok(())
    }

    /// Valid (unexpired, unused) invite for this token hash, if any.
    pub fn lookup_invite(&self, token_hash: &str, now_ms: i64) -> Result<Option<InviteInfo>> {
        lookup_invite_tx(&self.conn(), token_hash, now_ms)
    }

    /// Removes an invite outright (rollback when the mail carrying it
    /// could not be sent).
    pub fn delete_invite(&self, token_hash: &str) -> Result<()> {
        self.conn()
            .execute("DELETE FROM invites WHERE token_hash = ?1", [token_hash])?;
        Ok(())
    }

    /// Burns the invite; errors if it was already used (single-use).
    pub fn consume_invite(&self, token_hash: &str, now_ms: i64) -> Result<()> {
        consume_invite_tx(&self.conn(), token_hash, now_ms)
    }

    // ---- Passkeys ----

    pub fn add_passkey(
        &self,
        user: UserId,
        cred_id: &[u8],
        passkey_json: &str,
        label: &str,
        now_ms: i64,
    ) -> Result<i64> {
        add_passkey_tx(&self.conn(), user, cred_id, passkey_json, label, now_ms)
    }

    /// Whether anyone has a passkey here, i.e. whether passkey sign-in is
    /// offered at all.
    pub fn any_passkey(&self) -> Result<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM webauthn_credentials)",
            [],
            |r| r.get(0),
        )?)
    }

    /// The passkey an authenticator named while signing in, whoever owns
    /// it: the sign-in is how the owner is found.
    pub fn passkey_by_cred_id(&self, cred_id: &[u8]) -> Result<Option<StoredPasskey>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT id, user_id, passkey, label, created_at, last_used_at
                 FROM webauthn_credentials WHERE cred_id = ?1",
                [cred_id],
                passkey_row,
            )
            .optional()?)
    }

    pub fn passkeys_for_user(&self, user: UserId) -> Result<Vec<StoredPasskey>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, passkey, label, created_at, last_used_at
             FROM webauthn_credentials WHERE user_id = ?1",
        )?;
        let rows = stmt.query_map([user.raw()], passkey_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Persists an updated serialized passkey (e.g. new sign counter) for
    /// one of `user`'s credentials.
    pub fn update_passkey(
        &self,
        user: UserId,
        id: i64,
        passkey_json: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE webauthn_credentials SET passkey = ?1, last_used_at = ?2
             WHERE id = ?3 AND user_id = ?4",
            params![passkey_json, now_ms, id, user.raw()],
        )?;
        Ok(())
    }

    /// Records a successful login with this passkey. Separate from
    /// update_passkey because most authenticators never change their
    /// serialized state (sign counter stuck at 0), yet the login still
    /// happened.
    pub fn touch_passkey(&self, user: UserId, id: i64, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE webauthn_credentials SET last_used_at = ?1 WHERE id = ?2 AND user_id = ?3",
            params![now_ms, id, user.raw()],
        )?;
        Ok(())
    }

    /// Removes one of the user's passkeys; false when it wasn't theirs.
    pub fn delete_passkey(&self, user: UserId, id: i64) -> Result<bool> {
        let n = self.conn().execute(
            "DELETE FROM webauthn_credentials WHERE id = ?1 AND user_id = ?2",
            params![id, user.raw()],
        )?;
        Ok(n > 0)
    }

    pub fn user_count(&self) -> Result<u32> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        Ok(n as u32)
    }
}
