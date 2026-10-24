use flash_core::{validate_card_text, CardId, GradingMode, UserId, UserSettings};
use flash_store::GrantRow;

use crate::service::{
    card_refused, AccountOverview, PasskeyInfo, Result, ServiceError, Services, SettingsOverview,
    StudySettingsPatch,
};

impl Services {
    pub fn account_overview(&self, user: UserId) -> Result<AccountOverview> {
        let row = self.store.account_row(user)?;
        let settings = self.store.get_settings(user)?;
        Ok(AccountOverview {
            id: user,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            theme: row.theme,
            grading_mode: settings.grading_mode,
            has_password: self.store.get_password_hash(user)?.is_some(),
            passkeys: self.store.passkeys_for_user(user)?.len() as u32,
        })
    }

    pub fn settings_overview(&self, user: UserId, _now_ms: i64) -> Result<SettingsOverview> {
        let row = self.store.account_row(user)?;
        let settings = self.store.get_settings(user)?;
        Ok(SettingsOverview {
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            theme: row.theme,
            grading_mode: settings.grading_mode,
            new_per_day: settings.new_per_day,
            reviews_per_day: settings.reviews_per_day,
            timezone: settings.timezone,
            day_cutoff_hour: settings.day_cutoff_hour,
            desired_retention: settings.desired_retention,
            due_by_day: settings.due_by_day,
            has_password: self.store.get_password_hash(user)?.is_some(),
            passkeys: self
                .store
                .passkeys_for_user(user)?
                .into_iter()
                .map(|p| PasskeyInfo {
                    id: p.id,
                    label: p.label,
                    created_at: p.created_at,
                    last_used_at: p.last_used_at,
                })
                .collect(),
            card_count: self.store.count_cards(user, None, None)?,
        })
    }

    /// The apps and connectors holding live grants for the user, most
    /// recently used first.
    pub fn connected_apps(&self, user: UserId, now_ms: i64) -> Result<Vec<GrantRow>> {
        Ok(self.store.list_grants_for_user(user, now_ms)?)
    }

    /// Revokes every live grant one client holds for the user. Idempotent:
    /// a client with nothing left to revoke is already disconnected.
    pub fn disconnect_app(&self, user: UserId, client_id: &str, now_ms: i64) -> Result<()> {
        self.store.revoke_grant(client_id, user, now_ms)?;
        Ok(())
    }

    /// Same bounds as signup's name field.
    pub fn set_display_name(&self, user: UserId, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(ServiceError::Invalid(crate::i18n::t("settings-name-error")));
        }
        Ok(self.store.set_display_name(user, name)?)
    }

    pub fn set_grading_mode(&self, user: UserId, mode: &str) -> Result<()> {
        let Some(mode) = GradingMode::from_str(mode) else {
            return Err(ServiceError::Malformed(format!("unknown mode {mode:?}")));
        };
        Ok(self.store.set_grading_mode(user, mode)?)
    }

    /// Validated here so no surface can store a zone jiff can't resolve
    /// (the day cutoff depends on it).
    pub fn set_study_settings(&self, user: UserId, patch: &StudySettingsPatch) -> Result<()> {
        if let Some(tz) = &patch.timezone {
            if flash_core::day::time_zone(tz).is_none() {
                return Err(ServiceError::Malformed(format!("unknown timezone {tz:?}")));
            }
        }
        if patch.day_cutoff_hour.is_some_and(|h| h > 23) {
            return Err(ServiceError::Malformed(
                "day cutoff must be an hour from 0 to 23".into(),
            ));
        }
        if patch
            .desired_retention
            .is_some_and(|r| !(0.70..=0.99).contains(&r))
        {
            return Err(ServiceError::Malformed(
                "desired retention must be between 0.70 and 0.99".into(),
            ));
        }
        Ok(self.store.set_study_settings(
            user,
            patch.timezone.as_deref(),
            patch.day_cutoff_hour,
            patch.desired_retention,
            patch.due_by_day,
        )?)
    }

    /// The zone a browser reports, taken only while the account still
    /// has the default one. The app asks; the web doesn't, so this is how
    /// someone who only ever uses the web gets study days on their own
    /// clock. True when the zone changed.
    pub fn adopt_browser_timezone(&self, user: UserId, tz: &str) -> Result<bool> {
        if flash_core::day::time_zone(tz).is_none() {
            return Err(ServiceError::Malformed(format!("unknown timezone {tz:?}")));
        }
        Ok(self.store.adopt_timezone(user, tz)?)
    }

    /// Whether the account's zone is still the default, which is when a
    /// page asks the browser for its own.
    pub fn timezone_is_default(&self, user: UserId) -> Result<bool> {
        Ok(self.store.get_settings(user)?.timezone == UserSettings::default().timezone)
    }

    /// One of the five UI themes. The web also keeps a cookie copy so a
    /// page can render before any request reaches the store.
    pub fn set_theme(&self, user: UserId, theme: &str) -> Result<()> {
        if crate::auth::Theme::from_str(theme).is_none() {
            return Err(ServiceError::Malformed(format!("unknown theme {theme:?}")));
        }
        Ok(self.store.set_theme(user, theme)?)
    }

    /// Every live card with deck and tags: the export surface. Never
    /// gated by any policy.
    pub fn export_cards(&self, user: UserId) -> Result<Vec<flash_store::export::ExportCard>> {
        Ok(self.store.export_cards(user)?)
    }

    pub fn update_card(
        &self,
        user: UserId,
        card: CardId,
        front: &str,
        back: &str,
        now_ms: i64,
    ) -> Result<()> {
        let text = validate_card_text(front, back).map_err(card_refused)?;
        Ok(self.store.update_card(user, card, &text, now_ms)?)
    }

    pub fn delete_card(&self, user: UserId, card: CardId, now_ms: i64) -> Result<()> {
        Ok(self.store.delete_card(user, card, now_ms)?)
    }

    /// Archives a card (Anki's "suspend"): it leaves every study queue
    /// and its counts, and keeps its schedule and history for when it is
    /// unarchived.
    pub fn set_card_archived(
        &self,
        user: UserId,
        card: CardId,
        archived: bool,
        now_ms: i64,
    ) -> Result<()> {
        Ok(self
            .store
            .set_card_suspended(user, card, archived, now_ms)?)
    }
}
