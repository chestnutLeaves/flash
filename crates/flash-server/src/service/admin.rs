use flash_core::UserId;

use crate::service::{Result, ServiceError, Services};

impl Services {
    pub fn admin_users(&self, now_ms: i64) -> Result<Vec<flash_store::AdminUserRow>> {
        Ok(self.store.list_users_admin(now_ms)?)
    }

    /// A one-time member enrollment link, valid for seven days; returns
    /// the raw token (only its hash is stored) and the expiry.
    pub fn create_member_invite(
        &self,
        by: UserId,
        display_name: &str,
        now_ms: i64,
    ) -> Result<(String, i64)> {
        const INVITE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
        let name = display_name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ServiceError::Invalid(crate::i18n::t(
                "invite-name-required",
            )));
        }
        let token = crate::auth::new_token();
        let expires = now_ms + INVITE_TTL_MS;
        self.store.create_invite(
            &crate::auth::hash_token(&token),
            name,
            "member",
            Some(by),
            expires,
            now_ms,
        )?;
        Ok((token, expires))
    }
}
