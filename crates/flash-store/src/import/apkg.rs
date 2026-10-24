//! Anki .apkg import: a zip holding a SQLite collection. Both collection
//! generations are handled: legacy (models/decks as JSON in `col`) and the
//! newer table-based schema (`notetypes`/`decks` tables).
//!
//! Fidelity model: one Flash card per real Anki card. Cloze notes expand
//! to one card per cloze index; standard notes render through their
//! actual card templates (so multi-field and reversed layouts come out
//! exactly as Anki shows them); notes without readable templates fall
//! back to first-two-fields. Every card carries a dual representation:
//! plain text (MCP/CSV) and sanitized semantic HTML (web UI),
//! plus its Anki card id (so the package's review history can follow it
//! at commit), suspension state, and (for cloze) the original markup for
//! round-trip export.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use rusqlite::{Connection, OpenFlags};

use super::apkg_media::read_media_manifest;
use super::cloze;
use super::colorize;
use super::deck_options::load_deck_options;
use super::notetype::load_notetypes;
use super::patterns;
use super::template;
use super::{ImportError, ImportNote, ImportRow, MediaSummary, ParsedImport};
use crate::media::{scan_refs, MediaKind, MediaRef};
use crate::richtext;
use crate::tmp::tempdir;

/// How much SQLite work one collection may cost before it is refused.
/// Reading a real collection takes well under a second; a crafted one
/// (views over recursive CTEs, corrupt pages) could otherwise hold the
/// import permit for as long as it likes.
const QUERY_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// The sentences an uploader sees when a package cannot be read. What
/// the zip, zstd or SQLite layer actually said goes to the log through
/// `ImportError::detail`, never to the response. The zip sentence lives
/// with the one opener in `apkg_media`.
const DAMAGED: &str = "import-collection-damaged";
const UNEXPECTED_LAYOUT: &str = "import-collection-layout";
const STAGING_FAILED: &str = "import-staging-failed";

/// The settings SQLite's own guidance prescribes for a database file
/// received from a stranger: no schema-driven code paths, strict cell
/// checks, no writes, and a deadline. The tables the importer reads must
/// be real tables, not views that run arbitrary SQL on SELECT.
fn harden_untrusted(conn: &Connection) -> Result<(), ImportError> {
    use rusqlite::config::DbConfig;
    let damaged = |e: rusqlite::Error| ImportError::new(DAMAGED, format!("harden: {e}"));
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(damaged)?;
    for pragma in [
        "PRAGMA trusted_schema = OFF",
        "PRAGMA cell_size_check = ON",
        "PRAGMA query_only = ON",
    ] {
        conn.execute_batch(pragma).map_err(damaged)?;
    }
    let started = std::time::Instant::now();
    conn.progress_handler(10_000, Some(move || started.elapsed() > QUERY_BUDGET))
        .map_err(damaged)?;
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE name IN ('notes','cards','revlog','col','notetypes','decks','deck_config')
               AND type != 'table'",
        )
        .map_err(damaged)?;
    let odd: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .map_err(damaged)?
        .collect::<rusqlite::Result<_>>()
        .map_err(damaged)?;
    if !odd.is_empty() {
        // The names are the attacker's to choose; they go to the log only.
        return Err(ImportError::new(
            UNEXPECTED_LAYOUT,
            format!("not tables: {}", odd.join(", ")),
        ));
    }
    Ok(())
}

/// Largest collection database an .apkg may unpack to. Real collections
/// are tens of megabytes; the cap is what a small host can hold in memory
/// twice over (the buffer and the temp file) without being pushed out.
const MAX_COLLECTION_BYTES: u64 = 256 * 1024 * 1024;

/// The package's collection staged as a hardened read-only connection.
/// The scratch directory lives exactly as long as the connection: field
/// order drops `conn` first, then the directory it was opened in.
struct Collection {
    conn: Connection,
    _dir: crate::tmp::TempDir,
}

fn open_collection(bytes: &[u8]) -> Result<Collection, ImportError> {
    // Prefer the newest collection present. The archive lives only in
    // this block: the manifest read below opens its own, and two central
    // directories side by side would double what a crafted package
    // costs before its first byte of content is read.
    let db_bytes = {
        let mut zip = super::apkg_media::open_package(bytes)?;
        let mut db_bytes: Option<Vec<u8>> = None;
        for name in [
            "collection.anki21b",
            "collection.anki21",
            "collection.anki2",
        ] {
            if let Ok(file) = zip.by_name(name) {
                // The collection is attacker-controlled bytes behind two
                // layers of compression; cap what either layer may expand
                // to, or a small upload becomes gigabytes in memory and
                // on disk.
                let mut buf = Vec::new();
                file.take(MAX_COLLECTION_BYTES + 1)
                    .read_to_end(&mut buf)
                    .map_err(|e| ImportError::new(DAMAGED, format!("read {name}: {e}")))?;
                if buf.len() as u64 > MAX_COLLECTION_BYTES {
                    return Err(ImportError::user(
                        ImportNote::new("import-collection-large")
                            .num("mb", MAX_COLLECTION_BYTES >> 20),
                    ));
                }
                if name.ends_with('b') {
                    buf = super::apkg_media::zstd_decode_capped(&buf, MAX_COLLECTION_BYTES)
                        .map_err(|e| ImportError::new(DAMAGED, format!("zstd {name}: {e}")))?;
                }
                db_bytes = Some(buf);
                break;
            }
        }
        db_bytes.ok_or_else(|| ImportError::user(ImportNote::new("import-no-collection")))?
    };

    // SQLite needs a file; use a temp path cleaned up on drop.
    let dir = tempdir("flash-apkg").map_err(|e| ImportError::new(STAGING_FAILED, e))?;
    let db_path = dir.path.join("collection.sqlite");
    std::fs::write(&db_path, &db_bytes)
        .map_err(|e| ImportError::new(STAGING_FAILED, format!("temp write: {e}")))?;
    // The buffer's job is done; the rows below should not sit beside a
    // second copy of the collection.
    drop(db_bytes);
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| ImportError::new(DAMAGED, format!("open: {e}")))?;
    harden_untrusted(&conn)?;
    Ok(Collection { conn, _dir: dir })
}

pub fn parse_apkg(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let collection = open_collection(bytes)?;
    let conn = &collection.conn;

    let deck_names = load_deck_names(conn)?;
    let notetypes = load_notetypes(conn);
    let note_cards = load_anki_cards(conn)?;
    let history = count_history(conn);
    // filename -> (kind, size) for everything actually in the package.
    let packaged: HashMap<String, (MediaKind, u64)> = read_media_manifest(bytes)
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.filename, (e.kind, e.size)))
        .collect();

    let mut out = ParsedImport {
        settings: load_deck_options(conn),
        progress_reviews: history.min(super::MAX_HISTORY_ROWS),
        ..Default::default()
    };
    if history > super::MAX_HISTORY_ROWS {
        out.messages
            .push(ImportNote::new("import-history-capped").num("max", super::MAX_HISTORY_ROWS));
    }

    let mut cloze_notes = 0u32;
    let mut cloze_cards = 0u32;
    let mut extra_cards = 0u32;
    let mut suspended_cards = 0u32;
    let mut extra_layouts_skipped = 0u32;
    let mut seen_media: HashMap<String, MediaKind> = HashMap::new();

    let damaged = |e: rusqlite::Error| ImportError::new(DAMAGED, format!("notes: {e}"));
    // A note's cells are cut in SQLite before they are copied out: no
    // field past the per-note count or the raw-field length can reach
    // a card, so nothing past them is worth an allocation. Every note
    // makes at least one row or is skipped, so more notes than rows is
    // the same sentence as more cards than rows.
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, mid, substr(flds, 1, {}), substr(tags, 1, {}) FROM notes LIMIT {}",
            super::MAX_FIELDS_PER_NOTE * super::MAX_RAW_FIELD_BYTES,
            (crate::repo::cards::MAX_TAG_LEN + 1) * crate::repo::cards::MAX_TAGS_PER_CARD * 4,
            super::MAX_ROWS + 1
        ))
        .map_err(damaged)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(damaged)?;

    for (seen, row) in rows.enumerate() {
        if seen >= super::MAX_ROWS {
            return Err(super::too_many_cards());
        }
        let (note_id, mid, flds, tags) = row.map_err(damaged)?;
        let fields_raw: Vec<&str> = flds
            .split('\u{1f}')
            .take(super::MAX_FIELDS_PER_NOTE)
            .map(super::clip_raw)
            .collect();
        let tags: Vec<String> = super::clip_tags(tags.split_whitespace().map(str::to_string));
        let cards: &[AnkiCard] = note_cards.get(&note_id).map(Vec::as_slice).unwrap_or(&[]);

        // Media refs from every field, deduped per note.
        let mut media: Vec<MediaRef> = Vec::new();
        let mut media_names: HashSet<String> = HashSet::new();
        for field in &fields_raw {
            for r in scan_refs(field) {
                if media_names.insert(r.filename.clone()) {
                    media.push(r);
                }
            }
        }
        for r in &media {
            seen_media.entry(r.filename.clone()).or_insert(r.kind);
        }

        let class_colors = notetypes.get(&mid).map(|n| &n.class_colors);
        let default_did = cards.first().map(|c| c.did);
        let deck_of = |card: Option<&AnkiCard>| -> Option<String> {
            card.map(|c| c.did)
                .or(default_did)
                .and_then(|did| deck_names.get(&did))
                .cloned()
        };
        let suspended_of = |card: Option<&AnkiCard>| card.is_some_and(|c| c.queue == -1);

        // ---- cloze notes: one card per index ----
        let cloze_parsed = fields_raw
            .iter()
            .enumerate()
            .find_map(|(i, f)| cloze::parse(f).map(|c| (i, c)));
        if let Some((cloze_field, cz)) = cloze_parsed {
            cloze_notes += 1;
            let extra_raw = fields_raw
                .iter()
                .enumerate()
                .find(|(j, f)| *j != cloze_field && !f.trim().is_empty())
                .map(|(_, f)| *f)
                .unwrap_or("");
            // Original markup, media/HTML-stripped: the round-trip source.
            let source = format!(
                "{}\u{1f}{}",
                text_pipeline(fields_raw[cloze_field]),
                text_pipeline(extra_raw)
            );
            let extra_text = text_pipeline(extra_raw);
            for idx in cz.indices() {
                let anki_card = cards.iter().find(|c| c.ord == idx as i64 - 1);
                let front = text_pipeline(&cz.front(idx, false));
                let front_html = html_pipeline(&cz.front(idx, true), class_colors);
                let answers = cz.answers(idx).join("; ");
                let mut back = text_pipeline(&answers);
                if !extra_text.is_empty() {
                    back = if back.is_empty() {
                        extra_text.clone()
                    } else {
                        format!("{back}\n{extra_text}")
                    };
                }
                // The reveal shows the *completed* sentence with the
                // hidden answer highlighted (Anki's flip behavior),
                // plus the Extra notes. The plain text keeps the concise
                // answer-only text above.
                let filled = cz.filled(idx);
                let back_html = html_pipeline(
                    &if extra_raw.trim().is_empty() {
                        filled
                    } else {
                        format!("{filled}<br>{extra_raw}")
                    },
                    class_colors,
                );
                if front.is_empty() || back.is_empty() {
                    out.skipped += 1;
                    continue;
                }
                cloze_cards += 1;
                if suspended_of(anki_card) {
                    suspended_cards += 1;
                }
                out.push_row(ImportRow {
                    anki_card_id: anki_card.map(|c| c.id),
                    media: media.clone(),
                    front,
                    back,
                    front_html,
                    back_html,
                    tags: tags.clone(),
                    deck: deck_of(anki_card),
                    suspended: suspended_of(anki_card),
                    cloze_text: Some(source.clone()),
                    cloze_index: Some(idx),
                    type_answer: None,
                })?;
            }
            continue;
        }

        // ---- standard notes through their real templates ----
        let nt = notetypes
            .get(&mid)
            .filter(|n| !n.templates.is_empty() && !n.fields.is_empty() && !cards.is_empty());
        if let Some(nt) = nt {
            let named: HashMap<&str, &str> = nt
                .fields
                .iter()
                .map(String::as_str)
                .zip(fields_raw.iter().copied())
                .collect();
            let mut emitted = 0u32;
            let mut template_skipped = 0u32;
            for card in cards {
                let Some(tmpl) = usize::try_from(card.ord)
                    .ok()
                    .and_then(|ord| nt.templates.get(ord))
                else {
                    extra_layouts_skipped += 1;
                    continue;
                };
                let front_raw = template::render(&tmpl.qfmt, &named, false);
                // {{FrontSide}} renders empty (Flash stacks front above
                // back), which strands decorative leading separators.
                let back_raw = trim_leading_breaks(template::render(&tmpl.afmt, &named, true));
                let front = text_pipeline(&front_raw);
                let mut back = text_pipeline(&back_raw);
                let mut back_html = html_pipeline(&back_raw, class_colors);
                // Many answer templates re-show the question (same fields,
                // different markup, instead of {{FrontSide}}). Flash
                // already shows the front above the back. Strip the echo,
                // text-level so markup differences don't hide it.
                if !front.is_empty() {
                    if let Some(rest) = richtext::strip_leading_text(&back, &front) {
                        if !rest.is_empty() {
                            back = rest;
                            if let Some(h) = back_html.take() {
                                back_html = match richtext::strip_leading_text_html(&h, &front) {
                                    // Echo cut; drop html entirely if what
                                    // remains is plain text anyway.
                                    Some(rest_h) if rest_h.contains('<') => Some(rest_h),
                                    Some(_) => None,
                                    // Couldn't align the cut: keep original.
                                    None => Some(h),
                                };
                            }
                        }
                    }
                }
                if front.is_empty() || back.is_empty() {
                    if card.ord == 0 {
                        template_skipped += 1;
                    }
                    continue;
                }
                let type_answer = template::type_answer_field(&tmpl.qfmt)
                    .and_then(|f| named.get(f))
                    .map(|v| text_pipeline(v))
                    .filter(|v| !v.is_empty());
                if card.ord > 0 {
                    extra_cards += 1;
                }
                if card.queue == -1 {
                    suspended_cards += 1;
                }
                out.push_row(ImportRow {
                    anki_card_id: Some(card.id),
                    media: media.clone(),
                    front,
                    back,
                    front_html: html_pipeline(&front_raw, class_colors),
                    back_html,
                    tags: tags.clone(),
                    deck: deck_of(Some(card)),
                    suspended: card.queue == -1,
                    cloze_text: None,
                    cloze_index: None,
                    type_answer,
                })?;
                emitted += 1;
            }
            if emitted > 0 {
                // Skips alongside emitted cards are final; a fully-empty
                // note falls through to the fallback, which does its own
                // counting (avoids double-counting one skipped note).
                out.skipped += template_skipped;
                continue;
            }
        }

        // ---- fallback: first two fields, ord-1 = reversed heuristic ----
        let front_raw = fields_raw.first().copied().unwrap_or("");
        let back_raw = fields_raw.get(1).copied().unwrap_or("");
        let front = text_pipeline(front_raw);
        let back = text_pipeline(back_raw);
        if front.is_empty() || back.is_empty() {
            out.skipped += 1;
            continue;
        }
        let ord0 = cards.iter().find(|c| c.ord == 0);
        if suspended_of(ord0) {
            suspended_cards += 1;
        }
        let front_html = html_pipeline(front_raw, class_colors);
        let back_html = html_pipeline(back_raw, class_colors);
        out.push_row(ImportRow {
            anki_card_id: ord0.map(|c| c.id),
            media: media.clone(),
            front: front.clone(),
            back: back.clone(),
            front_html: front_html.clone(),
            back_html: back_html.clone(),
            tags: tags.clone(),
            deck: deck_of(ord0),
            suspended: suspended_of(ord0),
            cloze_text: None,
            cloze_index: None,
            type_answer: None,
        })?;
        if let Some(rev) = cards.iter().find(|c| c.ord == 1) {
            extra_cards += 1;
            if rev.queue == -1 {
                suspended_cards += 1;
            }
            out.push_row(ImportRow {
                anki_card_id: Some(rev.id),
                media,
                front: back,
                back: front,
                front_html: back_html,
                back_html: front_html,
                tags,
                deck: deck_of(Some(rev)),
                suspended: rev.queue == -1,
                cloze_text: None,
                cloze_index: None,
                type_answer: None,
            })?;
        }
        extra_layouts_skipped += cards.iter().filter(|c| c.ord >= 2).count() as u32;
    }

    out.media = summarize_media(&seen_media, &packaged);
    if out.skipped > 0 {
        out.messages
            .push(ImportNote::new("import-notes-skipped").num("count", out.skipped));
    }
    if cloze_notes > 0 {
        out.messages.push(
            ImportNote::new("import-cloze-expanded")
                .num("notes", cloze_notes)
                .num("cards", cloze_cards),
        );
    }
    if extra_cards > 0 {
        out.messages
            .push(ImportNote::new("import-extra-layouts").num("count", extra_cards));
    }
    if suspended_cards > 0 {
        out.messages
            .push(ImportNote::new("import-suspended").num("count", suspended_cards));
    }
    if extra_layouts_skipped > 0 {
        out.messages
            .push(ImportNote::new("import-layouts-skipped").num("count", extra_layouts_skipped));
    }
    if let Some(msg) = media_message(&out.media) {
        out.messages.push(msg);
    }
    Ok(out)
}

// ---- per-card data ----

struct AnkiCard {
    id: i64,
    ord: i64,
    did: i64,
    queue: i64,
}

/// Every Anki card, grouped by note, ord order. More cards than
/// `MAX_ROWS` is the row-cap sentence before any of them is built: each
/// card becomes at most one row, so the count is the same bound.
fn load_anki_cards(conn: &Connection) -> Result<HashMap<i64, Vec<AnkiCard>>, ImportError> {
    let mut out: HashMap<i64, Vec<AnkiCard>> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT id, nid, ord, did, queue FROM cards ORDER BY nid, ord LIMIT {}",
        super::MAX_ROWS + 1
    )) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        }) {
            for (seen, (id, nid, ord, did, queue)) in rows.flatten().enumerate() {
                if seen >= super::MAX_ROWS {
                    return Err(super::too_many_cards());
                }
                out.entry(nid).or_default().push(AnkiCard {
                    id,
                    ord,
                    did,
                    queue,
                });
            }
        }
    }
    Ok(out)
}

// ---- review history ----

/// Which revlog rows the replay can use, in SQL so the preview's count
/// and the commit's read are the same set. Learn (0), review (1) and
/// relearn (2) rows carry the button pressed as `ease`, Anki's four
/// buttons in every phase; cram (3), manual (4) and rescheduled rows
/// are not reviews.
const REPLAYABLE: &str = "type IN (0, 1, 2) AND ease BETWEEN 1 AND 4";

/// How many replayable rows the package's log holds, counted to one past
/// `MAX_HISTORY_ROWS` so the caller can tell a capped read from a full
/// one. Best-effort: a package without a revlog has no history.
fn count_history(conn: &Connection) -> usize {
    conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM revlog WHERE {REPLAYABLE} LIMIT {})",
            super::MAX_HISTORY_ROWS + 1
        ),
        [],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n.max(0) as usize)
    .unwrap_or(0)
}

/// The package's replayable reviews, oldest first: the newest
/// `MAX_HISTORY_ROWS` of them. Read at commit from the kept package, so
/// a parked preview never carries the history. A package without a
/// revlog yields none.
pub fn review_history(bytes: &[u8]) -> Result<Vec<super::HistoryEntry>, ImportError> {
    let collection = open_collection(bytes)?;
    let conn = &collection.conn;
    let mut out: Vec<super::HistoryEntry> = Vec::new();
    // revlog id is the review timestamp in ms, so newest first is the
    // order the cap wants: what the LIMIT cuts is the oldest history.
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT id, cid, ease, time FROM revlog WHERE {REPLAYABLE} ORDER BY id DESC LIMIT {}",
        super::MAX_HISTORY_ROWS
    )) {
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3).ok().flatten(),
                ))
            })
            .map_err(|e| ImportError::new(DAMAGED, format!("revlog: {e}")))?;
        for row in rows {
            let (at_ms, anki_card_id, ease, time) =
                row.map_err(|e| ImportError::new(DAMAGED, format!("revlog: {e}")))?;
            // The predicate already keeps ease within the buttons; the
            // conversion just names the one pressed.
            if let Some(rating) = flash_core::Rating::from_i64(ease) {
                out.push(super::HistoryEntry {
                    at_ms,
                    anki_card_id,
                    rating,
                    // Anki's `time`: ms on the card before the answer.
                    answer_ms: time.filter(|ms| *ms > 0),
                });
            }
        }
    }
    out.reverse();
    Ok(out)
}

// ---- text/html pipelines ----

/// Raw Anki field/template HTML -> Flash plain text: TTS wrappers off,
/// legacy LaTeX delimiters normalized, scripts/styles gone *with their
/// contents* (sanitizer pass), media noise removed, structure kept as
/// newlines/bullets, capped to the card-side limit. A side whose only
/// content is media (an image-only front, an audio-only prompt) gets a
/// speakable kind label instead of being empty.
pub(crate) fn text_pipeline(raw: &str) -> String {
    // Clipped on entry: the rewriters below are not linear, and what
    // reaches them may be a field, a rendered template or a cloze
    // expansion, so the bound is applied here rather than trusted.
    let raw = super::clip_raw(raw);
    let s = richtext::strip_tts_wrappers(raw);
    let s = richtext::convert_latex_delims(&s);
    // Reveal patterns translate to <details> here too, so the plain text
    // reads "Label: content" instead of the trigger and hidden div inline.
    let s = patterns::translate_reveal_patterns(&s);
    // The sanitizer drops script/style bodies entirely; plain tag
    // stripping alone would leak them into card text.
    let s = if s.contains('<') {
        richtext::sanitize_card_html(&s).into_string()
    } else {
        s
    };
    let mut t = strip_media_text(&s);
    if t.is_empty() {
        t = media_only_label(raw);
    }
    truncate_side(&mut t);
    t
}

/// "[image]", "[audio]", "[video]" (deduped, in reference order) for a
/// side that contains nothing but media; empty when there is no media
/// either, so genuinely blank sides still skip.
fn media_only_label(raw: &str) -> String {
    let mut kinds: Vec<&str> = Vec::new();
    for r in scan_refs(raw) {
        let label = match r.kind {
            MediaKind::Image => "[image]",
            MediaKind::Audio => "[audio]",
            MediaKind::Video => "[video]",
            MediaKind::Unsupported => continue,
        };
        if !kinds.contains(&label) {
            kinds.push(label);
        }
    }
    kinds.join(" ")
}

/// Raw HTML -> sanitized rich HTML for the UI, or None when the content
/// is plain (no markup worth storing) or absurdly large. Reveal-pattern
/// translation and color remapping run pre-sanitize; `class_colors` is the
/// notetype's CSS color map (None for notes without a readable notetype).
fn html_pipeline(
    raw: &str,
    class_colors: Option<&colorize::ClassColorMap>,
) -> Option<richtext::SanitizedHtml> {
    let raw = super::clip_raw(raw);
    let s = richtext::strip_tts_wrappers(raw);
    let s = richtext::convert_latex_delims(&s);
    let s = patterns::translate_reveal_patterns(&s);
    let empty = colorize::ClassColorMap::new();
    let s = colorize::remap_colors(&s, class_colors.unwrap_or(&empty));
    let s = richtext::media_placeholders(&s);
    let clean = richtext::sanitize_card_html(&s).trimmed();
    if clean.is_empty() || !clean.contains('<') || clean.len() > 40_000 {
        None
    } else {
        Some(clean)
    }
}

/// Strips leading `<hr>`/`<br>` separators (and whitespace) left behind
/// when `{{FrontSide}}` rendered to nothing.
fn trim_leading_breaks(s: String) -> String {
    let mut rest = s.trim_start();
    loop {
        let lower = rest
            .get(..4)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if lower.starts_with("<hr") || lower.starts_with("<br") {
            match rest.find('>') {
                Some(i) => rest = rest[i + 1..].trim_start(),
                None => break,
            }
        } else {
            break;
        }
    }
    rest.to_string()
}

fn truncate_side(s: &mut String) {
    const MAX: usize = flash_core::card::MAX_SIDE_LEN;
    if s.len() > MAX {
        let mut i = MAX;
        while i > 0 && !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
    }
}

// ---- media summary ----

/// Tallies the distinct media files referenced across all cards against
/// what the package actually contains.
fn summarize_media(
    referenced: &HashMap<String, MediaKind>,
    packaged: &HashMap<String, (MediaKind, u64)>,
) -> MediaSummary {
    let mut summary = MediaSummary::default();
    for (name, kind) in referenced {
        match kind {
            MediaKind::Image => summary.images += 1,
            MediaKind::Audio => summary.audio += 1,
            MediaKind::Video => summary.video += 1,
            MediaKind::Unsupported => summary.unsupported += 1,
        }
        match packaged.get(name) {
            Some((_, size)) => summary.total_bytes += size,
            None => summary.missing += 1,
        }
    }
    summary
}

/// What the package's media come to: how many of each kind and their
/// size in MB (the server lists the kinds there are).
fn media_message(m: &MediaSummary) -> Option<ImportNote> {
    if m.referenced() == 0 {
        return None;
    }
    let mb = (m.total_bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;
    Some(
        ImportNote::new("import-media")
            .num("images", m.images)
            .num("audio", m.audio)
            .num("video", m.video)
            .num("unsupported", m.unsupported)
            .num("mb", mb),
    )
}

// ---- deck names ----

/// An Anki deck name as a deck path: the whole of it, so a subdeck arrives
/// as a subdeck. The decks table separates the levels with \x1f and the
/// older JSON with `::`; both come out joined by `::`, each level trimmed.
/// A name with nothing in it stays as it was, for the caller to refuse.
fn deck_path_of(name: &str) -> String {
    let path = name.replace('\u{1f}', flash_core::deck_path::SEP);
    flash_core::deck_path::normalise(&path).unwrap_or(path)
}

fn load_deck_names(conn: &Connection) -> Result<HashMap<i64, String>, ImportError> {
    // Newer schema: a real decks table.
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT id, name FROM decks LIMIT {}",
        super::MAX_LOOKUP_ROWS
    )) {
        if let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, String>(1)?))) {
            let map: HashMap<i64, String> = rows
                .filter_map(|r| r.ok())
                .map(|(id, name)| (id, deck_path_of(&name)))
                .collect();
            if !map.is_empty() {
                return Ok(map);
            }
        }
    }
    // Legacy: JSON blob in col.decks.
    let json: String = conn
        .query_row("SELECT decks FROM col LIMIT 1", [], |r| r.get(0))
        .map_err(|e| ImportError::new(UNEXPECTED_LAYOUT, format!("col.decks: {e}")))?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| ImportError::new(UNEXPECTED_LAYOUT, format!("col.decks json: {e}")))?;
    let mut map = HashMap::new();
    if let Some(obj) = value.as_object() {
        for (id, deck) in obj {
            if let (Ok(id), Some(name)) = (id.parse(), deck["name"].as_str()) {
                map.insert(id, deck_path_of(name));
            }
        }
    }
    Ok(map)
}

// ---- HTML -> text ----

/// Field HTML -> card text with media noise removed: `[sound:...]` tags
/// are references, not content. Media-only sides come back empty here;
/// `text_pipeline` turns those into a kind label ("[audio]").
fn strip_media_text(html: &str) -> String {
    strip_html(&strip_sound_tags(html))
}

fn strip_sound_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("[sound:") {
        out.push_str(&rest[..start]);
        match rest[start..].find(']') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// HTML-to-text preserving structure: tags removed, entities decoded,
/// block boundaries become newlines and list items become bullets.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in html.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let t = tag.trim();
                let closing = t.starts_with('/');
                let name: String = t
                    .trim_start_matches('/')
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                match (closing, name.as_str()) {
                    (false, "br" | "hr") | (true, "div" | "p" | "tr" | "li") => out.push('\n'),
                    (false, "li") => out.push_str("\n• "),
                    (true, "td" | "th") => out.push(' '),
                    (false, "details") => out.push('\n'),
                    (true, "summary") => out.push_str(": "),
                    (true, "details") => out.push('\n'),
                    _ => {}
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decoded
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::deck_path_of;

    /// Anki's decks table separates the levels of a nested deck with \x1f
    /// and its older JSON with `::`. Both are the whole path here, and one
    /// colon is part of a name: the leaf-only reading used to cut
    /// "Week 1: intro" down to " intro".
    #[test]
    fn an_anki_deck_name_is_its_whole_path() {
        assert_eq!(
            deck_path_of("Anatomy\u{1f}Cardiac\u{1f}Conduction"),
            "Anatomy::Cardiac::Conduction"
        );
        assert_eq!(deck_path_of("Anatomy::Cardiac"), "Anatomy::Cardiac");
        assert_eq!(deck_path_of(" Anatomy :: Cardiac "), "Anatomy::Cardiac");
        assert_eq!(deck_path_of("Week 1: intro"), "Week 1: intro");
        assert_eq!(deck_path_of("Default"), "Default");
    }
}
