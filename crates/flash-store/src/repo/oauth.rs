//! OAuth 2.1 storage: dynamically-registered clients, single-use codes,
//! and opaque tokens. Callers pass SHA-256 hex of codes/tokens only.

use flash_core::UserId;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{Result, Store, StoreError};

/// `Store::revoke_all_tokens_for_user` on a connection or a transaction
/// the caller holds (a password reset revokes every grant with the rest
/// of its changes).
pub fn revoke_all_tokens_for_user_tx(
    conn: &Connection,
    user: UserId,
    keep: Option<i64>,
    now_ms: i64,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE oauth_tokens SET revoked_at = ?1
         WHERE user_id = ?2 AND revoked_at IS NULL
           AND (?3 IS NULL OR id != ?3)",
        params![now_ms, user.raw(), keep],
    )?)
}

/// `Store::revoke_tokens_for_user` on a connection or a transaction the
/// caller holds ("sign out other devices" ends sessions with it).
pub fn revoke_tokens_for_user_tx(
    conn: &Connection,
    user: UserId,
    client_id: &str,
    keep: Option<i64>,
    now_ms: i64,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE oauth_tokens SET revoked_at = ?1
         WHERE user_id = ?2 AND client_id = ?3 AND revoked_at IS NULL
           AND (?4 IS NULL OR id != ?4)",
        params![now_ms, user.raw(), client_id, keep],
    )?)
}

#[derive(Debug, Clone)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CodeGrant {
    pub client_id: String,
    pub user_id: UserId,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scope: String,
}

/// Result of trying to consume an authorization code.
pub enum CodeConsume {
    Fresh(CodeGrant),
    /// The code was already used. OAuth 2.1 requires revoking the grant.
    Replayed {
        client_id: String,
        user_id: UserId,
    },
    /// The code exists but its minute ran out before the client came back
    /// for it. Unlike an unknown code, it names whose connection stalled.
    Expired {
        client_id: String,
        user_id: UserId,
    },
    Invalid,
}

#[derive(Debug, Clone)]
pub struct RefreshGrant {
    pub client_id: String,
    pub user_id: UserId,
    pub scope: String,
}

/// How long the refresh token a grant was just rotated from still brings
/// back the pair that rotation made. A client whose reply was lost (a
/// phone dropping off a network mid-request, a connector retrying) or
/// that sent two refreshes at once comes back within seconds with the
/// token it still holds; a minute covers that, and is all the time a
/// copied token has to be cashed in without the grant being revoked.
pub const REFRESH_GRACE_MS: i64 = 60 * 1000;

/// The pair a rotation installs, and the salt it was derived with.
#[derive(Debug, Clone, Copy)]
pub struct NextPair<'a> {
    pub access_hash: &'a str,
    pub refresh_hash: &'a str,
    pub salt: &'a str,
    pub access_expires_ms: i64,
    pub refresh_expires_ms: i64,
}

/// What presenting a refresh token came to.
#[derive(Debug)]
pub enum RefreshOutcome {
    /// The grant's current refresh token: rotated to the new pair.
    Rotated(RefreshGrant),
    /// The token the grant was last rotated from, again within
    /// `REFRESH_GRACE_MS` and before the new refresh token was used: that
    /// rotation's pair is made again from the token and this `salt`.
    /// Nothing is minted or changed.
    Again {
        grant: RefreshGrant,
        salt: String,
        access_expires_at: i64,
    },
    /// A token the grant already rotated away, outside that grace: two
    /// parties hold the grant, so it is revoked, access and refresh.
    Reused(RefreshGrant),
    /// Unknown, expired, revoked, or another client's.
    Invalid,
}

/// What a valid access token says about its bearer: enough for an API
/// extractor to decide both "who" and "which client minted this".
#[derive(Debug, Clone)]
pub struct ApiTokenInfo {
    pub id: i64,
    pub user: UserId,
    pub role: String,
    pub client_id: String,
    /// The client's registered display name ("Claude", "ChatGPT", …).
    pub client_name: String,
    pub scope: String,
    /// The account's language as learned or chosen (`account_locale`),
    /// and whether it was chosen; read with the token so a reply asks no
    /// more of the store to speak it.
    pub locale: Option<String>,
    pub locale_chosen: bool,
    /// The time zone the account studies in, only when it was learned or
    /// chosen (a default says nothing about where someone is).
    pub zone: Option<String>,
}

/// One live grant as listed in Settings ("this iPhone", "iPad", …).
#[derive(Debug, Clone)]
pub struct TokenRow {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// One client the user has live grants with, as the settings page lists
/// it: an MCP connector registered dynamically (named by its
/// registration) or the first-party app (no registration row, one
/// token per device).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    pub client_id: String,
    /// The registered `client_name`; empty for a client with no
    /// registration row.
    pub client_name: String,
    /// Live tokens under this client.
    pub tokens: u32,
    pub first_granted_at: i64,
    pub last_used_at: Option<i64>,
}

/// Grace after a token is revoked or its refresh half expires before the
/// row is purged: long enough to investigate a report, short enough that
/// the table stays bounded.
const TOKEN_PURGE_GRACE_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// How long a registered client that never completed an authorization is
/// kept. A real connector setup finishes in minutes; a day is generous.
const CLIENT_PURGE_AGE_MS: i64 = 24 * 60 * 60 * 1000;

impl Store {
    pub fn create_oauth_client(
        &self,
        client_id: &str,
        client_name: &str,
        redirect_uris: &[String],
        metadata_json: &str,
        now_ms: i64,
    ) -> Result<()> {
        // A list of strings always serializes; the sentence is fixed so
        // no library text can reach a client through it.
        let uris = serde_json::to_string(redirect_uris)
            .map_err(|_| StoreError::Invalid("redirect uris could not be stored".into()))?;
        self.conn().execute(
            "INSERT INTO oauth_clients (client_id, client_name, redirect_uris, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![client_id, client_name, uris, metadata_json, now_ms],
        )?;
        Ok(())
    }

    /// Revokes every live grant the user holds, for any client, except
    /// `keep`. What a credential change calls: a password change or reset
    /// must end every session that credential (or its absence) allowed,
    /// and connector grants are sessions too.
    pub fn revoke_all_tokens_for_user(
        &self,
        user: UserId,
        keep: Option<i64>,
        now_ms: i64,
    ) -> Result<usize> {
        revoke_all_tokens_for_user_tx(&self.conn(), user, keep, now_ms)
    }

    /// Authorization codes past their expiry, spent or not: each one is
    /// a row a signed-in member could mint at the limiter's pace, and
    /// nothing reads an expired one.
    pub fn purge_expired_oauth_codes(&self, now_ms: i64) -> Result<usize> {
        Ok(self
            .conn()
            .execute("DELETE FROM oauth_codes WHERE expires_at < ?1", [now_ms])?)
    }

    /// Removes registered clients that never obtained a grant and are
    /// older than `CLIENT_PURGE_AGE_MS`. Registration is unauthenticated,
    /// so without this the table grows with every bot that finds it; a
    /// client mid-authorization is younger than the cutoff by definition.
    pub fn purge_unused_oauth_clients(&self, now_ms: i64) -> Result<usize> {
        let cutoff = now_ms - CLIENT_PURGE_AGE_MS;
        Ok(self.conn().execute(
            "DELETE FROM oauth_clients
             WHERE created_at < ?1
               AND NOT EXISTS (SELECT 1 FROM oauth_tokens t WHERE t.client_id = oauth_clients.client_id)
               AND NOT EXISTS (SELECT 1 FROM oauth_codes c WHERE c.client_id = oauth_clients.client_id)",
            [cutoff],
        )?)
    }

    pub fn get_oauth_client(&self, client_id: &str) -> Result<Option<OAuthClient>> {
        let row = self
            .conn()
            .query_row(
                "SELECT client_name, redirect_uris FROM oauth_clients WHERE client_id = ?1",
                [client_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((client_name, uris)) => Ok(Some(OAuthClient {
                client_id: client_id.to_string(),
                client_name,
                // The column was written by `create_oauth_client` above;
                // a row that does not parse is corruption, and the
                // sentence stays fixed either way.
                redirect_uris: serde_json::from_str(&uris)
                    .map_err(|_| StoreError::Invalid("client record is unreadable".into()))?,
            })),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_oauth_code(
        &self,
        code_hash: &str,
        client_id: &str,
        user: UserId,
        redirect_uri: &str,
        code_challenge: &str,
        scope: &str,
        resource: Option<&str>,
        expires_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO oauth_codes
               (code_hash, client_id, user_id, redirect_uri, code_challenge, scope, resource, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![code_hash, client_id, user.raw(), redirect_uri, code_challenge, scope, resource, expires_ms],
        )?;
        Ok(())
    }

    /// Whether the code was redeemed: `None` once it is gone (purged, or
    /// never issued), so a caller cannot mistake a deleted row for one
    /// that was never used.
    pub fn oauth_code_used(&self, user: UserId, code_hash: &str) -> Result<Option<bool>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT used_at IS NOT NULL FROM oauth_codes WHERE code_hash = ?1 AND user_id = ?2",
                params![code_hash, user.raw()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Withdraws a code nobody has redeemed, so a code issued in its
    /// place is the only one that works. False when there was nothing to
    /// withdraw: it was redeemed already, or is gone.
    pub fn retire_oauth_code(&self, user: UserId, code_hash: &str) -> Result<bool> {
        Ok(self.conn().execute(
            "DELETE FROM oauth_codes WHERE code_hash = ?1 AND user_id = ?2 AND used_at IS NULL",
            params![code_hash, user.raw()],
        )? == 1)
    }

    /// Single-use consumption with replay detection.
    pub fn consume_oauth_code(&self, code_hash: &str, now_ms: i64) -> Result<CodeConsume> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let row = tx
            .query_row(
                "SELECT client_id, user_id, redirect_uri, code_challenge, scope, used_at, expires_at
                 FROM oauth_codes WHERE code_hash = ?1",
                [code_hash],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        UserId::from_db(r.get::<_, i64>(1)?),
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<i64>>(5)?,
                        r.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        let Some((client_id, user_id, redirect_uri, code_challenge, scope, used_at, expires_at)) =
            row
        else {
            return Ok(CodeConsume::Invalid);
        };
        if used_at.is_some() {
            tx.commit()?;
            return Ok(CodeConsume::Replayed { client_id, user_id });
        }
        if expires_at <= now_ms {
            return Ok(CodeConsume::Expired { client_id, user_id });
        }
        tx.execute(
            "UPDATE oauth_codes SET used_at = ?1 WHERE code_hash = ?2",
            params![now_ms, code_hash],
        )?;
        tx.commit()?;
        Ok(CodeConsume::Fresh(CodeGrant {
            client_id,
            user_id,
            redirect_uri,
            code_challenge,
            scope,
        }))
    }

    /// `label` names the device for first-party (mobile) grants; MCP
    /// clients pass "".
    #[allow(clippy::too_many_arguments)]
    pub fn insert_oauth_token(
        &self,
        access_hash: &str,
        refresh_hash: &str,
        client_id: &str,
        user: UserId,
        scope: &str,
        label: &str,
        install_id: Option<&str>,
        access_expires_ms: i64,
        refresh_expires_ms: i64,
        now_ms: i64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO oauth_tokens
               (access_hash, refresh_hash, client_id, user_id, scope, label, install_id,
                access_expires_at, refresh_expires_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                access_hash,
                refresh_hash,
                client_id,
                user.raw(),
                scope,
                label,
                install_id,
                access_expires_ms,
                refresh_expires_ms,
                now_ms
            ],
        )?;
        Ok(())
    }

    /// Revokes the user's live grants of one client that one install
    /// asked for: signing in again on a phone replaces its old grant.
    pub fn revoke_install_tokens(
        &self,
        user: UserId,
        client_id: &str,
        install_id: &str,
        now_ms: i64,
    ) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE oauth_tokens SET revoked_at = ?1
             WHERE user_id = ?2 AND client_id = ?3 AND install_id = ?4 AND revoked_at IS NULL",
            params![now_ms, user.raw(), client_id, install_id],
        )?)
    }

    /// Valid access token -> who holds it and which client minted it.
    pub fn lookup_api_token(&self, access_hash: &str, now_ms: i64) -> Result<Option<ApiTokenInfo>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT t.id, t.user_id, u.role, t.client_id, t.scope,
                        COALESCE(c.client_name, ''),
                        st.locale, st.locale_source,
                        CASE WHEN st.timezone_source <> 'default' THEN st.timezone END
                 FROM oauth_tokens t JOIN users u ON u.id = t.user_id
                 LEFT JOIN oauth_clients c ON c.client_id = t.client_id
                 LEFT JOIN user_settings st ON st.user_id = t.user_id
                 WHERE t.access_hash = ?1 AND t.revoked_at IS NULL AND t.access_expires_at > ?2",
                params![access_hash, now_ms],
                |r| {
                    Ok(ApiTokenInfo {
                        id: r.get(0)?,
                        user: UserId::from_db(r.get(1)?),
                        role: r.get(2)?,
                        client_id: r.get(3)?,
                        scope: r.get(4)?,
                        client_name: r.get(5)?,
                        locale: r.get(6)?,
                        locale_chosen: r.get::<_, Option<String>>(7)?.as_deref() == Some("chosen"),
                        zone: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    /// The user's live grants for one client, newest first.
    pub fn list_tokens_for_user(
        &self,
        user: UserId,
        client_id: &str,
        now_ms: i64,
    ) -> Result<Vec<TokenRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at, last_used_at FROM oauth_tokens
             WHERE user_id = ?1 AND client_id = ?2 AND revoked_at IS NULL
               AND refresh_expires_at > ?3
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user.raw(), client_id, now_ms], |r| {
            Ok(TokenRow {
                id: r.get(0)?,
                label: r.get(1)?,
                created_at: r.get(2)?,
                last_used_at: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The clients holding live grants for the user, one row per client,
    /// most recently used first. A grant is live while its refresh half
    /// has not expired (or, for a token without one, its access half).
    pub fn list_grants_for_user(&self, user: UserId, now_ms: i64) -> Result<Vec<GrantRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT t.client_id, COALESCE(c.client_name, ''), COUNT(*),
                    MIN(t.created_at), MAX(t.last_used_at)
             FROM oauth_tokens t
             LEFT JOIN oauth_clients c ON c.client_id = t.client_id
             WHERE t.user_id = ?1 AND t.revoked_at IS NULL
               AND COALESCE(t.refresh_expires_at, t.access_expires_at) > ?2
             GROUP BY t.client_id
             ORDER BY MAX(COALESCE(t.last_used_at, t.created_at)) DESC",
        )?;
        let rows = stmt.query_map(params![user.raw(), now_ms], |r| {
            Ok(GrantRow {
                client_id: r.get(0)?,
                client_name: r.get(1)?,
                tokens: r.get::<_, i64>(2)?.max(0) as u32,
                first_granted_at: r.get(3)?,
                last_used_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Revokes one of the user's own grants of one client (sign out
    /// this device). The client is named so an id from the devices list
    /// can only ever reach a device, never a connected app.
    pub fn revoke_token(&self, user: UserId, client_id: &str, id: i64, now_ms: i64) -> Result<()> {
        crate::exec_expect_row(
            &self.conn(),
            "token",
            "UPDATE oauth_tokens SET revoked_at = ?1
             WHERE id = ?2 AND user_id = ?3 AND client_id = ?4 AND revoked_at IS NULL",
            params![now_ms, id, user.raw(), client_id],
        )
    }

    /// Revokes every live grant of one client for the user except `keep`
    /// (the caller's own): password changes and "sign out other devices".
    pub fn revoke_tokens_for_user(
        &self,
        user: UserId,
        client_id: &str,
        keep: Option<i64>,
        now_ms: i64,
    ) -> Result<usize> {
        revoke_tokens_for_user_tx(&self.conn(), user, client_id, keep, now_ms)
    }

    /// Drops rows that have been revoked or refresh-expired for longer
    /// than the grace window, and rotated-away refresh tokens past their
    /// own expiry. Housekeeping for the scheduler.
    pub fn purge_expired_tokens(&self, now_ms: i64) -> Result<usize> {
        let cutoff = now_ms - TOKEN_PURGE_GRACE_MS;
        let conn = self.conn();
        conn.execute(
            "DELETE FROM oauth_spent_refresh WHERE expires_at < ?1",
            [now_ms],
        )?;
        Ok(conn.execute(
            "DELETE FROM oauth_tokens
             WHERE (revoked_at IS NOT NULL AND revoked_at < ?1)
                OR refresh_expires_at < ?1",
            [cutoff],
        )?)
    }

    /// Presents a refresh token: rotation with reuse detection (OAuth 2.1
    /// section 4.3.1, RFC 9700 section 4.14.2). The grant's current token
    /// rotates to `next`, whose salt is kept; the token it was last
    /// rotated from, again within `REFRESH_GRACE_MS`, is told that salt
    /// to make the same pair again; any other token the grant rotated
    /// away revokes the grant. A token of another client than
    /// `client_id` (when named) changes nothing.
    pub fn rotate_refresh_token(
        &self,
        old_refresh_hash: &str,
        client_id: Option<&str>,
        next: &NextPair<'_>,
        now_ms: i64,
    ) -> Result<RefreshOutcome> {
        let grant_of = |r: &rusqlite::Row| -> rusqlite::Result<RefreshGrant> {
            Ok(RefreshGrant {
                client_id: r.get(1)?,
                user_id: UserId::from_db(r.get(2)?),
                scope: r.get(3)?,
            })
        };
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let current = tx
            .query_row(
                "SELECT id, client_id, user_id, scope, refresh_expires_at FROM oauth_tokens
                 WHERE refresh_hash = ?1 AND revoked_at IS NULL AND refresh_expires_at > ?2
                   AND (?3 IS NULL OR client_id = ?3)",
                params![old_refresh_hash, now_ms, client_id],
                |r| Ok((r.get::<_, i64>(0)?, grant_of(r)?, r.get::<_, i64>(4)?)),
            )
            .optional()?;
        if let Some((id, grant, expires_at)) = current {
            tx.execute(
                "INSERT INTO oauth_spent_refresh (refresh_hash, token_id, expires_at, spent_at, salt)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![old_refresh_hash, id, expires_at, now_ms, next.salt],
            )?;
            // Refresh is the one per-device write worth recording as "last
            // used". Every request would be too chatty for a single
            // SQLite connection.
            tx.execute(
                "UPDATE oauth_tokens
                 SET access_hash = ?1, refresh_hash = ?2,
                     access_expires_at = ?3, refresh_expires_at = ?4, last_used_at = ?5
                 WHERE id = ?6",
                params![
                    next.access_hash,
                    next.refresh_hash,
                    next.access_expires_ms,
                    next.refresh_expires_ms,
                    now_ms,
                    id
                ],
            )?;
            tx.commit()?;
            return Ok(RefreshOutcome::Rotated(grant));
        }
        // Newest: no later spent row of the grant (rowid orders rotations
        // within one millisecond).
        let spent = tx
            .query_row(
                "SELECT t.id, t.client_id, t.user_id, t.scope, s.spent_at, s.salt,
                        t.access_expires_at,
                        NOT EXISTS (SELECT 1 FROM oauth_spent_refresh l
                                    WHERE l.token_id = s.token_id
                                      AND (l.spent_at, l.rowid) > (s.spent_at, s.rowid))
                 FROM oauth_spent_refresh s JOIN oauth_tokens t ON t.id = s.token_id
                 WHERE s.refresh_hash = ?1 AND s.expires_at > ?2 AND t.revoked_at IS NULL
                   AND (?3 IS NULL OR t.client_id = ?3)",
                params![old_refresh_hash, now_ms, client_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        grant_of(r)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, bool>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((id, grant, spent_at, salt, access_expires_at, newest)) = spent else {
            return Ok(RefreshOutcome::Invalid);
        };
        if newest && now_ms - spent_at <= REFRESH_GRACE_MS {
            return Ok(RefreshOutcome::Again {
                grant,
                salt,
                access_expires_at,
            });
        }
        tx.execute(
            "UPDATE oauth_tokens SET revoked_at = ?1 WHERE id = ?2",
            params![now_ms, id],
        )?;
        tx.commit()?;
        Ok(RefreshOutcome::Reused(grant))
    }

    /// Revokes every token for a (client, user) grant, used on code replay.
    pub fn revoke_grant(&self, client_id: &str, user: UserId, now_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE oauth_tokens SET revoked_at = ?1
             WHERE client_id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
            params![now_ms, client_id, user.raw()],
        )?)
    }
}
