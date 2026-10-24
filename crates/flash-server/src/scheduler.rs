//! The background housekeeping no request path was ever a good home
//! for: daily purges of what has expired. Always on, in every binary.

use std::time::Duration;

use crate::service::{now_ms, Services};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// How old an unlinked media row must be before the sweep takes it: an
/// editor upload is linked when its note is saved, which is minutes
/// away at most.
const UNLINKED_MEDIA_GRACE_MS: i64 = 60 * 60 * 1000;

/// How long a finished job is kept, and a failed one.
const DONE_JOB_TTL_MS: i64 = 30 * DAY_MS;
const FAILED_JOB_TTL_MS: i64 = 90 * DAY_MS;
/// How long an import's commit marker is kept.
const IMPORT_COMMIT_TTL_MS: i64 = 7 * DAY_MS;
/// How long an unused MCP session is kept for its client to come back to.
const MCP_SESSION_TTL_MS: i64 = 30 * DAY_MS;
/// How long the browser errors the pages reported are kept after they
/// were last seen (`client_errors`); the privacy policy says so.
pub const CLIENT_ERROR_TTL_MS: i64 = 30 * DAY_MS;

/// Daily purges: tokens revoked or expired long ago, spent password
/// resets, export packages a crash left on disk. Cheap, idempotent, safe
/// to run any time.
pub fn housekeeping(services: &Services, data_dir: &std::path::Path, now_ms: i64) {
    let exports = crate::flows::export_flow::exports_dir_in(data_dir);
    let swept = crate::flows::export_flow::sweep_stale_exports(&exports);
    if swept > 0 {
        tracing::info!("removed {swept} abandoned export files");
    }
    let store = services.store();
    // Media no card links any more (a deleted or plain-edited card, an
    // editor upload never saved into a note): the rows go once they are
    // an hour old, and the blobs nobody else holds are owed to the job
    // that re-checks each before removing it.
    match store.sweep_unlinked_media(now_ms - UNLINKED_MEDIA_GRACE_MS) {
        Ok(orphans) if !orphans.is_empty() => {
            tracing::info!("owed {} orphaned media blobs to removal", orphans.len());
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("sweep media: {e}"),
    }
    match store.purge_expired_tokens(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} expired tokens"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge tokens: {e}"),
    }
    match store.purge_expired_web_sessions(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} expired browser sessions"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge browser sessions: {e}"),
    }
    if let Err(e) = store.purge_expired_password_resets(now_ms) {
        tracing::warn!("purge password resets: {e}");
    }
    // Kept a while past expiry, so the check for a code its client never
    // came back for still finds it (`oauth::JOB_OAUTH_CODE_CHECK`).
    match store.purge_expired_oauth_codes(now_ms - crate::oauth::CODE_KEPT_MS) {
        Ok(n) if n > 0 => tracing::info!("purged {n} expired OAuth codes"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge OAuth codes: {e}"),
    }
    match store.purge_unused_oauth_clients(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} unused OAuth clients"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge OAuth clients: {e}"),
    }
    // Finished jobs, once their dedupe keys have outlived any repeat of
    // what made them; failed ones stay a season for someone to look at.
    match store.purge_done_jobs(now_ms - DONE_JOB_TTL_MS) {
        Ok(n) if n > 0 => tracing::info!("purged {n} finished jobs"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge jobs: {e}"),
    }
    match store.purge_failed_jobs(now_ms - FAILED_JOB_TTL_MS) {
        Ok(n) if n > 0 => tracing::info!("purged {n} failed jobs"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge failed jobs: {e}"),
    }
    // Import commit markers, long after their previews expired.
    if let Err(e) = store.purge_import_commits(now_ms - IMPORT_COMMIT_TTL_MS) {
        tracing::warn!("purge import commits: {e}");
    }
    // Sign-in ceremonies nobody finished, and assistants' sessions nobody
    // has used in a month (their clients start new ones).
    if let Err(e) = store.purge_expired_ceremonies(now_ms) {
        tracing::warn!("purge ceremonies: {e}");
    }
    if let Err(e) = store.purge_idle_mcp_sessions(now_ms - MCP_SESSION_TTL_MS) {
        tracing::warn!("purge mcp sessions: {e}");
    }
    // What broke in browsers, counted: a month of it.
    if let Err(e) = store.purge_client_errors(now_ms - CLIENT_ERROR_TTL_MS) {
        tracing::warn!("purge browser errors: {e}");
    }
}

/// Housekeeping once at boot and then daily. Every binary spawns this.
pub async fn run_housekeeping(services: Services, data_dir: std::path::PathBuf) {
    let mut interval = tokio::time::interval(Duration::from_millis(DAY_MS as u64));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let services = services.clone();
        let data_dir = data_dir.clone();
        let _ =
            tokio::task::spawn_blocking(move || housekeeping(&services, &data_dir, now_ms())).await;
    }
}
