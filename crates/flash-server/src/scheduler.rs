//! The background housekeeping no request path was ever a good home
//! for: daily purges of what has expired. Always on, in every binary.

use std::time::Duration;

use crate::service::{now_ms, Services};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// How old an unlinked media row must be before the sweep takes it: an
/// editor upload is linked when its note is saved, which is minutes
/// away at most.
const UNLINKED_MEDIA_GRACE_MS: i64 = 60 * 60 * 1000;

/// How long a generation source and its cached reading stay after they
/// were last used (a preflight, a job, a study turn that read the
/// source). The cards made from them are never touched.
const AI_SOURCE_TTL_MS: i64 = 30 * DAY_MS;

/// The shorter leash on a source nobody ever generated from: a file
/// dropped into the composer and then left there.
const AI_UNATTACHED_SOURCE_TTL_MS: i64 = 2 * DAY_MS;

/// How long a card's voice clip stays reachable for an account after
/// it was last played; the clip itself goes once nobody's card names
/// it, after a day's grace for one made a moment ago.
const VOICE_REF_TTL_MS: i64 = 90 * DAY_MS;
const VOICE_CLIP_GRACE_MS: i64 = DAY_MS;

/// Daily purges: tokens revoked or expired long ago, spent password
/// resets, export packages a crash left on disk. Cheap, idempotent, safe
/// to run any time.
pub fn housekeeping(
    services: &Services,
    media: &dyn crate::media_store::MediaStore,
    data_dir: &std::path::Path,
    now_ms: i64,
) {
    let exports = crate::flows::export_flow::exports_dir_in(data_dir);
    let swept = crate::flows::export_flow::sweep_stale_exports(&exports);
    if swept > 0 {
        tracing::info!("removed {swept} abandoned export files");
    }
    let store = services.store();
    // Media no card links any more (a deleted or plain-edited card, an
    // editor upload never saved into a note): the rows go once they are
    // an hour old, and the blobs nobody else holds go with them.
    match store.sweep_unlinked_media(now_ms - UNLINKED_MEDIA_GRACE_MS) {
        Ok(orphans) => {
            let mut removed = 0u32;
            for sha in &orphans {
                match media.delete(sha) {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!(sha = ?sha, "orphan blob: {e}"),
                }
            }
            if removed > 0 {
                tracing::info!("removed {removed} orphaned media blobs");
            }
        }
        Err(e) => tracing::warn!("sweep media: {e}"),
    }
    // Generation sources nobody has used for a month (two days for one
    // no job ever read), with their cached readings; the blobs go when
    // no other row names the hash.
    match store.sweep_stale_ai_sources(
        now_ms - AI_SOURCE_TTL_MS,
        now_ms - AI_UNATTACHED_SOURCE_TTL_MS,
    ) {
        Ok(orphans) => {
            let mut removed = 0u32;
            for sha in &orphans {
                match media.delete(sha) {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!(sha = ?sha, "stale source blob: {e}"),
                }
            }
            if !orphans.is_empty() {
                tracing::info!(
                    sources = orphans.len(),
                    removed,
                    "removed stale generation sources"
                );
            }
        }
        Err(e) => tracing::warn!("sweep generation sources: {e}"),
    }
    // Voice clips of cards nobody has studied in months, or that are
    // gone: the refs go, then the recordings nobody's card names.
    match store.sweep_voice_clips(now_ms - VOICE_REF_TTL_MS, now_ms - VOICE_CLIP_GRACE_MS) {
        Ok(orphans) => {
            let mut removed = 0u32;
            for key in &orphans {
                match media.delete(key) {
                    Ok(()) => removed += 1,
                    Err(e) => tracing::warn!(key = ?key, "orphan voice clip: {e}"),
                }
            }
            if removed > 0 {
                tracing::info!("removed {removed} orphaned voice clips");
            }
        }
        Err(e) => tracing::warn!("sweep voice clips: {e}"),
    }
    match store.purge_expired_tokens(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} expired tokens"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge tokens: {e}"),
    }
    if let Err(e) = store.purge_expired_password_resets(now_ms) {
        tracing::warn!("purge password resets: {e}");
    }
    match store.purge_expired_oauth_codes(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} expired OAuth codes"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge OAuth codes: {e}"),
    }
    match store.purge_unused_oauth_clients(now_ms) {
        Ok(n) if n > 0 => tracing::info!("purged {n} unused OAuth clients"),
        Ok(_) => {}
        Err(e) => tracing::warn!("purge OAuth clients: {e}"),
    }
}

/// Housekeeping once at boot and then daily. Every binary spawns this.
pub async fn run_housekeeping(
    services: Services,
    media: std::sync::Arc<dyn crate::media_store::MediaStore>,
    data_dir: std::path::PathBuf,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(DAY_MS as u64));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let services = services.clone();
        let media = media.clone();
        let data_dir = data_dir.clone();
        let _ = tokio::task::spawn_blocking(move || {
            housekeeping(&services, media.as_ref(), &data_dir, now_ms())
        })
        .await;
    }
}
