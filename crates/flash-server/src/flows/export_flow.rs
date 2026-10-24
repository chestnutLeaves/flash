//! Export: never gated (your cards are always yours). Both surfaces
//! serve the same bytes.
//!
//! The .apkg is built on disk, one media blob in memory at a time, then
//! streamed to the client from the file; the file lives exactly as long
//! as the download (`ExportFile` removes it on drop). A daily sweep
//! removes anything a crash mid-download left behind.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use flash_core::DeckId;
use parking_lot::Mutex;
use tokio::io::{AsyncRead, ReadBuf};

use crate::auth::new_token;
use crate::config::Config;
use crate::media_store::{MediaStore, MediaStoreError};
use crate::service::{Result, ServiceError, Services};
use crate::state::HeavyPermit;

/// Where finished packages wait to be sent.
pub fn exports_dir(config: &Config) -> PathBuf {
    exports_dir_in(&config.data_dir)
}

pub fn exports_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("exports")
}

/// A leftover older than this was abandoned by a crash, not by a slow
/// download.
const STALE_EXPORT_MS: u128 = 60 * 60 * 1000;

/// Room an export leaves on the disk for the database and everyone else.
const DISK_KEPT_FREE: u64 = 256 * 1024 * 1024;

/// Whether a package of `media_bytes` still leaves `DISK_KEPT_FREE` of
/// the disk's `free` bytes.
fn room_for(media_bytes: u64, free: u64) -> bool {
    media_bytes.saturating_add(DISK_KEPT_FREE) <= free
}

/// Bytes promised to exports still being written. Up to `HEAVY_GLOBAL`
/// run at once and each sees only what the others have not written yet,
/// so they check the disk against one shared count. Pessimistic on
/// purpose: bytes already written count in the promise and again as
/// space gone, so a busy disk refuses early, never late.
struct Promised(Mutex<u64>);

/// The process's one count: the disk is shared by every export.
static PROMISED: Promised = Promised(Mutex::new(0));

impl Promised {
    /// Promises `bytes` when they fit in `free` beside what is promised
    /// already. The promise lasts until the returned guard drops.
    fn reserve(&self, bytes: u64, free: u64) -> Option<Promise<'_>> {
        let mut pending = self.0.lock();
        if !room_for(pending.saturating_add(bytes), free) {
            return None;
        }
        *pending += bytes;
        Some(Promise {
            promised: self,
            bytes,
        })
    }
}

/// Bytes an export holds in `Promised`; given back on drop.
struct Promise<'a> {
    promised: &'a Promised,
    bytes: u64,
}

impl Drop for Promise<'_> {
    fn drop(&mut self) {
        *self.promised.0.lock() -= self.bytes;
    }
}

/// A finished package on disk. Dropping it deletes the file, so whoever
/// holds it decides when the download is over.
pub struct ExportFile {
    pub path: PathBuf,
    pub size: u64,
}

impl Drop for ExportFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The Anki package with every live card and its media, built on disk. A
/// heavy job: the caller holds a permit; the peak in memory is one blob.
pub fn apkg(
    permit: &HeavyPermit,
    services: &Services,
    media: &dyn MediaStore,
    config: &Config,
    now_ms: i64,
) -> Result<ExportFile> {
    apkg_in(permit, services, media, config, None, now_ms)
}

/// As `apkg`, for one deck and its subdecks when `deck` is given.
pub fn apkg_in(
    permit: &HeavyPermit,
    services: &Services,
    media: &dyn MediaStore,
    config: &Config,
    deck: Option<DeckId>,
    now_ms: i64,
) -> Result<ExportFile> {
    let user = permit.user();
    let cards = match deck {
        Some(deck) => services.export_deck_cards(user, deck)?,
        None => services.export_cards(user)?,
    };
    let plan = crate::media::plan_export_media_in(services, user, deck)?;
    let dir = exports_dir(config);
    std::fs::create_dir_all(&dir)
        .map_err(|e| ServiceError::Internal(format!("exports dir: {e}")))?;
    // Media are stored compressed already, so the package is about their
    // size. One that would fill the disk, beside the exports already
    // being written, is refused before it starts: a full disk fails every
    // other write the server makes.
    let media_bytes: u64 = plan.entries.iter().map(|e| e.size).sum();
    let _promise = match fs4::available_space(&dir) {
        Ok(free) => Some(PROMISED.reserve(media_bytes, free).ok_or_else(|| {
            ServiceError::Internal(format!(
                "export of {media_bytes} bytes of media refused: {free} bytes free on disk"
            ))
        })?),
        // A probe that fails leaves the write itself as the bound.
        Err(e) => {
            tracing::warn!("exports dir free space: {e}");
            None
        }
    };
    let path = dir.join(format!("{}-{}.apkg", user.raw(), new_token()));
    let file = std::fs::File::create(&path)
        .map_err(|e| ServiceError::Internal(format!("export file: {e}")))?;
    // From here the file exists; an early return drops the guard and
    // removes it.
    let mut export = ExportFile { path, size: 0 };
    // A file gone from storage costs the package that file, not the
    // whole export: the cards keep its name.
    let mut read = |entry: &flash_store::export::ExportMediaEntry| match media
        .get(&entry.sha256, None)
    {
        Ok(blob) => Ok(Some(blob.bytes)),
        Err(MediaStoreError::NotFound) => {
            tracing::warn!(user = user.raw(), sha = ?entry.sha256, "export media missing from storage");
            Ok(None)
        }
        Err(e) => Err(format!("media {}: {e}", entry.sha256)),
    };
    // A failure to build the package is ours (a temp file, SQLite, zip,
    // the blob store), never the user's: logged in full, shown as
    // "internal error".
    let file = flash_store::export::build_apkg_into(&cards, &plan, &mut read, now_ms, file)
        .map_err(ServiceError::Internal)?;
    export.size = file
        .metadata()
        .map_err(|e| ServiceError::Internal(format!("export size: {e}")))?
        .len();
    Ok(export)
}

pub fn csv(permit: &HeavyPermit, services: &Services) -> Result<String> {
    Ok(flash_store::export::build_csv(
        &services.export_cards(permit.user())?,
    ))
}

/// As `csv`, for one deck and its subdecks.
pub fn deck_csv(permit: &HeavyPermit, services: &Services, deck: DeckId) -> Result<String> {
    Ok(flash_store::export::build_csv(
        &services.export_deck_cards(permit.user(), deck)?,
    ))
}

/// `flash-2026-08-31.apkg`, the attachment name both surfaces use.
pub fn file_name(ext: &str, now_ms: i64) -> String {
    format!("flash-{}.{ext}", crate::web::format_day(now_ms))
}

/// `Cardiology 2026-08-31.apkg`: a deck's own export, named after it.
/// Characters a file name cannot carry become spaces.
pub fn deck_file_name(deck: &str, ext: &str, now_ms: i64) -> String {
    let name: String = deck
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = if name.is_empty() {
        "flash".to_string()
    } else {
        name
    };
    format!("{name} {}.{ext}", crate::web::format_day(now_ms))
}

/// A `Content-Disposition` for a download named `name`: the name in
/// UTF-8 for browsers that read it (all current ones), and an ASCII
/// stand-in for the rest, so a deck called "Fisiología" keeps its name.
pub fn disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
        crate::auth::urlencode(name)
    )
}

/// The package as a download: streamed from disk with its length known,
/// the file deleted when the body is dropped (finished or abandoned).
pub async fn attachment_stream(export: ExportFile, content_type: &str, name: String) -> Response {
    let file = match tokio::fs::File::open(&export.path).await {
        Ok(file) => file,
        Err(e) => {
            tracing::error!("export reopen: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };
    let size = export.size;
    let reader = GuardedFile {
        file,
        _guard: export,
    };
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(reader)).into_response();
    let h = response.headers_mut();
    for (key, value) in [
        (header::CONTENT_TYPE, content_type.to_string()),
        (header::CONTENT_LENGTH, size.to_string()),
        (header::CONTENT_DISPOSITION, disposition(&name)),
    ] {
        if let Ok(v) = value.parse() {
            h.insert(key, v);
        }
    }
    response
}

/// A file being sent, holding the guard that deletes it once the reader
/// is dropped.
struct GuardedFile {
    file: tokio::fs::File,
    _guard: ExportFile,
}

impl AsyncRead for GuardedFile {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_read(cx, buf)
    }
}

/// Removes packages older than `STALE_EXPORT_MS`: downloads that a crash
/// or a killed process left behind. Live downloads are younger than that.
pub fn sweep_stale_exports(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_millis() > STALE_EXPORT_MS);
        if stale && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_must_leave_the_kept_free_room() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert!(room_for(GIB, GIB + DISK_KEPT_FREE));
        assert!(!room_for(GIB, GIB + DISK_KEPT_FREE - 1));
        assert!(!room_for(0, DISK_KEPT_FREE - 1));
        assert!(!room_for(1 << 60, 1 << 40));
    }

    #[test]
    fn exports_at_once_share_one_disk() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let promised = Promised(Mutex::new(0));
        let first = promised.reserve(2 * GIB, 3 * GIB).expect("fits alone");
        assert!(promised.reserve(2 * GIB, 3 * GIB).is_none());
        drop(first);
        assert!(promised.reserve(2 * GIB, 3 * GIB).is_some());
    }
}
