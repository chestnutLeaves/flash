//! What a person hands card generation, before any model sees it: which
//! kind of thing an upload is, the facts a preflight can read off the
//! bytes (a recording's length), a plain-text file decoded to the text
//! it holds, the one hash a set of sources is cached under, a set of
//! photos packed as one blob (how photo sets were stored before every
//! photo became its own source; kept to read those), and a video link
//! reduced to the one form the provider is given. Nothing here touches
//! the database; the service records what these functions decide.

use flash_store::media::{classify, content_matches, sanitize_filename, MediaKind};
use flash_store::{richtext, AiSourceKind, AiSourceRow};
use sha2::{Digest, Sha256};

/// What an upload is on the way in: the stored kinds, plus two that
/// become a stored kind once read or converted. A text file is stored
/// as `text`; a document is converted to a PDF and stored as `pdf`
/// with its original type in `origin_mime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadKind {
    Pdf,
    Image,
    Video,
    Audio,
    TextFile,
    Document,
}

impl UploadKind {
    /// The kind the row will carry.
    pub fn stored(self) -> AiSourceKind {
        match self {
            UploadKind::Pdf | UploadKind::Document => AiSourceKind::Pdf,
            UploadKind::Image => AiSourceKind::Image,
            UploadKind::Video => AiSourceKind::Video,
            UploadKind::Audio => AiSourceKind::Audio,
            UploadKind::TextFile => AiSourceKind::Text,
        }
    }
}

/// What an upload turned out to be, from its bytes and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub upload: UploadKind,
    /// The type of the bytes as uploaded (a document's own, not the
    /// PDF it becomes).
    pub mime: &'static str,
    /// The name as the store keeps it: path stripped, junk removed.
    pub filename: String,
    /// The extension the name carried, lowercased, for a converter
    /// that needs the input named by its type.
    pub extension: String,
}

/// Plain-text files the composer takes as pasted text would be, as
/// `(extension, mime)`.
pub const TEXT_FILES: &[(&str, &str)] = &[
    ("txt", "text/plain"),
    ("text", "text/plain"),
    ("md", "text/markdown"),
    ("markdown", "text/markdown"),
    ("csv", "text/csv"),
    ("tsv", "text/tab-separated-values"),
    ("html", "text/html"),
    ("htm", "text/html"),
];

/// Office documents the operator's converter turns into PDFs, as
/// `(extension, mime)`.
pub const DOCUMENTS: &[(&str, &str)] = &[
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ("doc", "application/msword"),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("ppt", "application/vnd.ms-powerpoint"),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    ("xls", "application/vnd.ms-excel"),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("odp", "application/vnd.oasis.opendocument.presentation"),
    ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
];

/// The sentence for a file of a kind generation does not read.
pub const UNSUPPORTED_UPLOAD: &str = "That file type isn't supported. Upload a PDF, a document, \
                                      photos, a text file, a video or an audio recording.";

/// The sentence for a document when no converter is configured.
pub const NO_CONVERTER: &str = "This server does not convert Word, PowerPoint or spreadsheet \
                                files. Save it as a PDF and upload that.";

fn extension_of(filename: &str) -> String {
    match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Whether the bytes could be text: nothing that would never appear in
/// text within the first few kilobytes.
fn looks_like_text(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8 * 1024)];
    !head.contains(&0)
}

/// Whether the bytes are a zip (every OOXML and OpenDocument file is
/// one) or an OLE compound file (the older Office formats).
fn looks_like_document(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0])
}

/// Decides what an upload is. A PDF is known by its magic bytes whatever
/// it is called; a document by its extension and a zip or OLE header; a
/// text file by its extension and the absence of bytes text never has;
/// everything else goes by the media module's rules, so a photo, a clip
/// or a recording that the editor would accept is accepted here, and
/// one that lies about its extension is not.
pub fn classify_upload(filename: &str, bytes: &[u8]) -> Result<Classified, String> {
    let filename = sanitize_filename(filename);
    let extension = extension_of(&filename);
    if bytes.is_empty() {
        return Err("The file is empty.".to_string());
    }
    if bytes.starts_with(b"%PDF-") {
        return Ok(Classified {
            upload: UploadKind::Pdf,
            mime: "application/pdf",
            filename,
            extension: "pdf".into(),
        });
    }
    if let Some((_, mime)) = DOCUMENTS.iter().find(|(e, _)| *e == extension) {
        if !looks_like_document(bytes) {
            return Err(format!("{filename} does not look like a document."));
        }
        return Ok(Classified {
            upload: UploadKind::Document,
            mime,
            filename,
            extension,
        });
    }
    if let Some((_, mime)) = TEXT_FILES.iter().find(|(e, _)| *e == extension) {
        if !looks_like_text(bytes) {
            return Err(format!("{filename} does not look like a text file."));
        }
        return Ok(Classified {
            upload: UploadKind::TextFile,
            mime,
            filename,
            extension,
        });
    }
    let (media_kind, mime) = classify(&filename);
    let upload = match media_kind {
        MediaKind::Image => UploadKind::Image,
        MediaKind::Audio => UploadKind::Audio,
        MediaKind::Video => UploadKind::Video,
        MediaKind::Unsupported => return Err(UNSUPPORTED_UPLOAD.to_string()),
    };
    if !content_matches(media_kind, bytes) {
        return Err(format!(
            "{filename} does not look like {} file.",
            match upload {
                UploadKind::Image => "an image",
                UploadKind::Audio => "an audio",
                _ => "a video",
            }
        ));
    }
    Ok(Classified {
        upload,
        mime,
        filename,
        extension,
    })
}

// ---- text files ----

/// The text a plain-text file holds: a byte-order mark stripped, UTF-16
/// read when marked as such, HTML reduced to its text, line endings
/// normalised. Refuses bytes that are not text and a file with nothing
/// in it.
pub fn decode_text(bytes: &[u8], mime: &str) -> Result<String, String> {
    let text = if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let big_endian = bytes[0] == 0xFE;
        // A trailing odd byte is dropped, as chunks_exact dropped it.
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|&pair| {
                if big_endian {
                    u16::from_be_bytes(pair)
                } else {
                    u16::from_le_bytes(pair)
                }
            })
            .collect();
        String::from_utf16(&units).map_err(|_| "That file is not text.".to_string())?
    } else {
        let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        std::str::from_utf8(body)
            .map_err(|_| "That file is not UTF-8 text.".to_string())?
            .to_string()
    };
    let text = if mime == "text/html" {
        richtext::html_to_text(&text)
    } else {
        text
    };
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.is_empty() {
        return Err("The file has no text in it.".to_string());
    }
    Ok(text.to_string())
}

// ---- sets ----

/// The cache key of an ordered set of sources: one member's own hash,
/// so a lone file finds the reading a previous upload of it made; for
/// several members, a hash over their kinds and hashes in order, so
/// the numbering the model cited in its figures is stable.
pub fn set_hash(rows: &[AiSourceRow]) -> String {
    if let [one] = rows {
        return one.sha256.clone();
    }
    let mut hasher = Sha256::new();
    hasher.update(b"flash-source-set-v1\n");
    for row in rows {
        hasher.update(row.kind.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(row.sha256.as_bytes());
        hasher.update(b"\n");
    }
    crate::media_store::hex(&hasher.finalize())
}

// ---- recordings ----

/// The playing time of an MP4-family file (mp4, m4a, m4v, mov), read
/// from the movie header, in whole seconds. None for any other
/// container, a fragmented file, or a header that is not where the
/// specification puts it: the preflight then reports no length and the
/// call is metered from what the model actually consumed.
pub fn mp4_duration_seconds(bytes: &[u8]) -> Option<u32> {
    let moov = find_box(bytes, b"moov")?;
    let mvhd = find_box(moov, b"mvhd")?;
    let version = *mvhd.first()?;
    // After the version and flags: creation and modification times (32
    // or 64 bits each by version), then the timescale and the duration.
    let (timescale, duration) = match version {
        0 => (be_u32(mvhd, 12)?, u64::from(be_u32(mvhd, 16)?)),
        1 => (be_u32(mvhd, 20)?, be_u64(mvhd, 24)?),
        _ => return None,
    };
    if timescale == 0 {
        return None;
    }
    u32::try_from(duration / u64::from(timescale)).ok()
}

/// The payload of the first top-level box named `wanted` in `data`.
fn find_box<'a>(data: &'a [u8], wanted: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 0usize;
    while at + 8 <= data.len() {
        let size32 = u64::from(be_u32(data, at)?);
        let name = &data[at + 4..at + 8];
        let (header, size) = match size32 {
            // Zero: the box runs to the end of the file.
            0 => (8usize, (data.len() - at) as u64),
            // One: a 64-bit size follows the name.
            1 => (16usize, be_u64(data, at + 8)?),
            n => (8usize, n),
        };
        if size < header as u64 {
            return None;
        }
        let end = at.checked_add(usize::try_from(size).ok()?)?;
        if name == wanted {
            return data.get(at + header..end.min(data.len()));
        }
        at = end;
    }
    None
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn be_u64(data: &[u8], at: usize) -> Option<u64> {
    let bytes: [u8; 8] = data.get(at..at + 8)?.try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

// ---- video links ----

/// The hosts a video link may name.
pub const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "youtu.be",
];

const NOT_A_YOUTUBE_LINK: &str = "That doesn't look like a YouTube link.";

/// The one form a video link is stored and sent in:
/// `https://www.youtube.com/watch?v=ID`. Accepts the watch, short-link,
/// shorts, live and embed forms; refuses any other host, so the
/// provider is never asked to fetch an arbitrary address.
pub fn canonical_youtube_url(raw: &str) -> Result<String, String> {
    let url = url::Url::parse(raw.trim()).map_err(|_| NOT_A_YOUTUBE_LINK.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NOT_A_YOUTUBE_LINK.to_string());
    }
    let host = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| NOT_A_YOUTUBE_LINK.to_string())?;
    if !YOUTUBE_HOSTS.contains(&host.as_str()) {
        return Err(NOT_A_YOUTUBE_LINK.to_string());
    }
    let mut segments = url.path_segments().into_iter().flatten();
    let id = if host == "youtu.be" {
        segments.next().map(str::to_string)
    } else {
        match segments.next() {
            Some("watch") => url
                .query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned()),
            Some("shorts") | Some("live") | Some("embed") => segments.next().map(str::to_string),
            _ => None,
        }
    };
    let id = id.filter(|id| is_video_id(id)).ok_or_else(|| {
        "That YouTube link has no video in it. Paste the link of a video page.".to_string()
    })?;
    Ok(format!("https://www.youtube.com/watch?v={id}"))
}

/// Eleven characters of the URL-safe alphabet.
fn is_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ---- photo sets ----

/// One photo of a set, as uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageFile {
    pub filename: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// What the source row says about a photo set without opening the blob.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImageEntry {
    pub filename: String,
    pub mime: String,
    pub size: u64,
}

const SET_MAGIC: &[u8; 8] = b"FLASHIMG";
const SET_VERSION: u8 = 1;

/// A set of photos as one blob, so the media store holds it under one
/// hash that the source row names and housekeeping can remove. The
/// layout is the simplest that round-trips: a magic, a version, a
/// count, then for each file its type, its name and its bytes, each
/// length-prefixed.
pub fn pack_images(files: &[ImageFile]) -> Vec<u8> {
    let total: usize = files.iter().map(|f| f.bytes.len() + 64).sum();
    let mut out = Vec::with_capacity(total + 16);
    out.extend_from_slice(SET_MAGIC);
    out.push(SET_VERSION);
    out.push(files.len().min(u8::MAX as usize) as u8);
    for file in files.iter().take(u8::MAX as usize) {
        let mime = file.mime.as_bytes();
        let name = file.filename.as_bytes();
        out.push(mime.len().min(u8::MAX as usize) as u8);
        out.extend_from_slice(&mime[..mime.len().min(u8::MAX as usize)]);
        out.push(name.len().min(u8::MAX as usize) as u8);
        out.extend_from_slice(&name[..name.len().min(u8::MAX as usize)]);
        out.extend_from_slice(&(file.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&file.bytes);
    }
    out
}

/// The photos back out of a packed set. The blob was written by
/// `pack_images`, so a malformed one is a storage fault, not a person's
/// mistake; the message is for the log.
pub fn unpack_images(blob: &[u8]) -> Result<Vec<ImageFile>, String> {
    let malformed = || "photo set blob is malformed".to_string();
    if blob.len() < 10 || &blob[..8] != SET_MAGIC || blob[8] != SET_VERSION {
        return Err(malformed());
    }
    let count = blob[9] as usize;
    let mut at = 10usize;
    let mut files = Vec::with_capacity(count);
    let take = |at: &mut usize, n: usize| -> Result<&[u8], String> {
        let end = at.checked_add(n).ok_or_else(malformed)?;
        let piece = blob.get(*at..end).ok_or_else(malformed)?;
        *at = end;
        Ok(piece)
    };
    for _ in 0..count {
        let mime_len = *blob.get(at).ok_or_else(malformed)? as usize;
        at += 1;
        let mime = String::from_utf8_lossy(take(&mut at, mime_len)?).into_owned();
        let name_len = *blob.get(at).ok_or_else(malformed)? as usize;
        at += 1;
        let filename = String::from_utf8_lossy(take(&mut at, name_len)?).into_owned();
        let len: [u8; 4] = take(&mut at, 4)?.try_into().map_err(|_| malformed())?;
        let bytes = take(&mut at, u32::from_le_bytes(len) as usize)?.to_vec();
        files.push(ImageFile {
            filename,
            mime,
            bytes,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 1,
    ];

    #[test]
    fn uploads_are_known_by_their_bytes_and_their_names() {
        let pdf = classify_upload("notes.txt", b"%PDF-1.7 whatever").unwrap();
        assert_eq!(pdf.upload, UploadKind::Pdf);
        assert_eq!(pdf.mime, "application/pdf");
        assert_eq!(pdf.upload.stored(), AiSourceKind::Pdf);
        let png = classify_upload("../dir/pic.png", PNG).unwrap();
        assert_eq!(
            (png.upload, png.filename.as_str(), png.extension.as_str()),
            (UploadKind::Image, "pic.png", "png")
        );
        let mut mp4 = vec![0, 0, 0, 24];
        mp4.extend_from_slice(b"ftypisom");
        mp4.extend_from_slice(&[0; 16]);
        assert_eq!(
            classify_upload("clip.mp4", &mp4).unwrap().upload,
            UploadKind::Video
        );
        assert_eq!(
            classify_upload("talk.m4a", &mp4).unwrap().upload,
            UploadKind::Audio
        );
        let html = classify_upload("page.html", b"<html>hello</html>").unwrap();
        assert_eq!(
            (html.upload, html.mime),
            (UploadKind::TextFile, "text/html")
        );
        assert_eq!(html.upload.stored(), AiSourceKind::Text);
        assert_eq!(
            classify_upload("notes.md", b"# Heading\n\ntext")
                .unwrap()
                .mime,
            "text/markdown"
        );
        assert!(classify_upload("notes.txt", b"ab\0cd")
            .unwrap_err()
            .contains("does not look like a text file"));
        let docx = classify_upload("slides.PPTX", b"PK\x03\x04rest").unwrap();
        assert_eq!(docx.upload, UploadKind::Document);
        assert_eq!(docx.extension, "pptx");
        assert!(docx.mime.contains("presentationml"));
        assert_eq!(docx.upload.stored(), AiSourceKind::Pdf);
        let doc = classify_upload("old.doc", &[0xD0, 0xCF, 0x11, 0xE0, 1, 2]).unwrap();
        assert_eq!(doc.mime, "application/msword");
        assert!(classify_upload("fake.docx", b"just words")
            .unwrap_err()
            .contains("does not look like a document"));
        assert_eq!(
            classify_upload("archive.zip", b"PK\x03\x04rest").unwrap_err(),
            UNSUPPORTED_UPLOAD
        );
        assert!(classify_upload("fake.png", b"<html>hello</html>")
            .unwrap_err()
            .contains("does not look like an image"));
        assert_eq!(
            classify_upload("x.png", b"").unwrap_err(),
            "The file is empty."
        );
    }

    #[test]
    fn text_files_are_decoded_and_html_is_reduced_to_its_text() {
        assert_eq!(
            decode_text(b"\xEF\xBB\xBFhello\r\nworld\r\n", "text/plain").unwrap(),
            "hello\nworld"
        );
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "hi".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_text(&utf16, "text/plain").unwrap(), "hi");
        let mut utf16be = vec![0xFE, 0xFF];
        for unit in "yo".encode_utf16() {
            utf16be.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(decode_text(&utf16be, "text/plain").unwrap(), "yo");
        assert_eq!(
            decode_text(b"<p>Hello <b>there</b></p><script>x</script>", "text/html").unwrap(),
            "Hello there"
        );
        assert_eq!(
            decode_text(&[0xFF, 0xFF, 0x41], "text/plain").unwrap_err(),
            "That file is not UTF-8 text."
        );
        assert_eq!(
            decode_text(b"  \n ", "text/plain").unwrap_err(),
            "The file has no text in it."
        );
    }

    fn row(kind: AiSourceKind, sha: &str) -> AiSourceRow {
        AiSourceRow {
            id: flash_core::AiSourceId(1),
            kind,
            sha256: sha.into(),
            mime: String::new(),
            size: 0,
            filename: String::new(),
            url: None,
            text: None,
            pages: None,
            duration_s: None,
            est_words: None,
            image_count: None,
            origin_mime: None,
            created_at: 0,
            last_used_at: 0,
        }
    }

    #[test]
    fn a_set_hashes_to_its_only_member_or_to_its_members_in_order() {
        let a = row(AiSourceKind::Pdf, &"a".repeat(64));
        let b = row(AiSourceKind::Image, &"b".repeat(64));
        assert_eq!(set_hash(std::slice::from_ref(&a)), "a".repeat(64));
        let ab = set_hash(&[a.clone(), b.clone()]);
        assert_eq!(ab.len(), 64);
        assert_ne!(ab, set_hash(&[b.clone(), a.clone()]), "order matters");
        assert_eq!(ab, set_hash(&[a.clone(), b.clone()]));
        assert_ne!(
            ab,
            set_hash(&[a, row(AiSourceKind::Pdf, &"b".repeat(64))]),
            "the kind is part of it"
        );
    }

    fn mp4_with_mvhd(version: u8, timescale: u32, duration: u64) -> Vec<u8> {
        let mut mvhd = vec![version, 0, 0, 0];
        match version {
            0 => {
                mvhd.extend_from_slice(&[0; 8]);
                mvhd.extend_from_slice(&timescale.to_be_bytes());
                mvhd.extend_from_slice(&(duration as u32).to_be_bytes());
            }
            _ => {
                mvhd.extend_from_slice(&[0; 16]);
                mvhd.extend_from_slice(&timescale.to_be_bytes());
                mvhd.extend_from_slice(&duration.to_be_bytes());
            }
        }
        let mut mvhd_box = ((mvhd.len() + 8) as u32).to_be_bytes().to_vec();
        mvhd_box.extend_from_slice(b"mvhd");
        mvhd_box.extend_from_slice(&mvhd);
        let mut moov = ((mvhd_box.len() + 8) as u32).to_be_bytes().to_vec();
        moov.extend_from_slice(b"moov");
        moov.extend_from_slice(&mvhd_box);
        let mut file = 16u32.to_be_bytes().to_vec();
        file.extend_from_slice(b"ftypisom");
        file.extend_from_slice(&[0; 4]);
        file.extend_from_slice(&moov);
        file
    }

    #[test]
    fn a_recordings_length_comes_from_the_movie_header() {
        assert_eq!(
            mp4_duration_seconds(&mp4_with_mvhd(0, 1000, 754_000)),
            Some(754)
        );
        assert_eq!(
            mp4_duration_seconds(&mp4_with_mvhd(1, 90_000, 90_000 * 5_400)),
            Some(5_400)
        );
        assert_eq!(mp4_duration_seconds(&mp4_with_mvhd(0, 0, 10)), None);
        assert_eq!(mp4_duration_seconds(b"OggS\0\0\0\0\0\0\0\0"), None);
        assert_eq!(mp4_duration_seconds(&[0, 0, 0, 2, b'm']), None);
    }

    #[test]
    fn video_links_collapse_to_one_form_and_other_hosts_are_refused() {
        let want = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        for link in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s",
            "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/live/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "  http://YouTube.com/watch?v=dQw4w9WgXcQ  ",
        ] {
            assert_eq!(canonical_youtube_url(link).as_deref(), Ok(want), "{link}");
        }
        for link in [
            "https://vimeo.com/12345",
            "https://evil.example/watch?v=dQw4w9WgXcQ",
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "ftp://youtube.com/watch?v=dQw4w9WgXcQ",
            "not a url",
            "https://www.youtube.com/",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXc<",
        ] {
            assert!(canonical_youtube_url(link).is_err(), "{link}");
        }
    }

    #[test]
    fn photo_sets_round_trip() {
        let files = vec![
            ImageFile {
                filename: "a.png".into(),
                mime: "image/png".into(),
                bytes: PNG.to_vec(),
            },
            ImageFile {
                filename: "b.jpg".into(),
                mime: "image/jpeg".into(),
                bytes: vec![0xFF, 0xD8, 0xFF, 1, 2, 3],
            },
        ];
        let blob = pack_images(&files);
        assert_eq!(unpack_images(&blob).unwrap(), files);
        assert!(unpack_images(&blob[..blob.len() - 1]).is_err());
        assert!(unpack_images(b"nope").is_err());
        assert!(unpack_images(&pack_images(&[])).unwrap().is_empty());
    }
}
