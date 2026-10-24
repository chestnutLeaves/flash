//! Reference outputs of the .apkg importer, for another implementation of
//! it to check itself against. Each package in `tests/import_vectors/`
//! sits beside the JSON of what `parse_apkg` and `review_history` make of
//! it. `FLASH_WRITE_VECTORS=1 cargo test -p flash-store --test it
//! import_vectors_tests` builds the packages and writes both; otherwise
//! the stored packages are parsed again and must still give the stored
//! JSON, so a change to the importer shows up here and the reference is
//! regenerated on purpose rather than drifting.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flash_store::export::{build_apkg, ExportCard, ExportMedia};
use flash_store::import::{parse_apkg, review_history};

const NOW: i64 = 1_700_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/import_vectors")
}

/// The smallest PNG header the media checks accept, and an MP3's.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\x01\x02\x03";
const MP3: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00\xff\xfb";

/// What the exporter writes: rich sides, subdecks, tags, a cloze note of
/// two cards with a hint, a typed card, a highlight and an image.
fn exported() -> Vec<u8> {
    let cloze = "{{c1::Paris}} is in {{c2::France::country}}\u{1f}Geography".to_string();
    let cards = vec![
        ExportCard {
            deck: "Pharm::Anticoagulants".into(),
            front: "Warfarin antidote?".into(),
            back: "Vitamin K\nFFP".into(),
            front_html: Some("<b>Warfarin</b> antidote?".into()),
            back_html: Some("<ul><li>Vitamin K</li><li>FFP</li></ul>".into()),
            tags: vec!["exam".into(), "hematology".into()],
            ..Default::default()
        },
        ExportCard {
            deck: "Geo".into(),
            front: "[...] is in France".into(),
            back: "Paris\nGeography".into(),
            cloze: Some((cloze.clone(), 1)),
            ..Default::default()
        },
        ExportCard {
            deck: "Geo".into(),
            front: "Paris is in [country]".into(),
            back: "France\nGeography".into(),
            cloze: Some((cloze, 2)),
            ..Default::default()
        },
        ExportCard {
            deck: "Geo".into(),
            front: "Capital of Italy?".into(),
            back: "Rome".into(),
            type_answer: Some("Rome".into()),
            ..Default::default()
        },
        ExportCard {
            deck: "Art".into(),
            front: "[image]".into(),
            back: "A red cat".into(),
            front_html: Some(r#"<img src="/media/7" alt="cat">"#.into()),
            back_html: Some(r#"A <span class="hl-red">red</span> cat"#.into()),
            ..Default::default()
        },
    ];
    let media = ExportMedia {
        files: vec![("cat.png".into(), PNG.to_vec())],
        names: [(7, "cat.png".to_string())].into(),
    };
    build_apkg(&cards, &media, NOW).unwrap()
}

/// An exporter package with its collection rewritten by `sql` and extra
/// zip entries in place of its media.
fn rebuild(apkg: Vec<u8>, sql: &str, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(apkg)).unwrap();
    let mut db_bytes = Vec::new();
    zip.by_name("collection.anki2")
        .unwrap()
        .read_to_end(&mut db_bytes)
        .unwrap();
    let dir = std::env::temp_dir().join(format!("flash-import-vectors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("c.anki2");
    std::fs::write(&db_path, &db_bytes).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(sql).unwrap();
    drop(conn);
    let db = std::fs::read(&db_path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    out.start_file("collection.anki2", options).unwrap();
    out.write_all(&db).unwrap();
    for (name, bytes) in entries {
        out.start_file(*name, options).unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// Notes the exporter does not write: a three-field notetype with two
/// templates (a typed reverse, an answer that repeats its question, a
/// hint), CSS colors, a sound, LaTeX, a script, a suspended card, a card
/// whose layout is missing, and a review history.
fn templated() -> Vec<u8> {
    let base = build_apkg(
        &[ExportCard {
            deck: "Vocab::Japanese".into(),
            front: "placeholder".into(),
            back: "placeholder".into(),
            ..Default::default()
        }],
        &ExportMedia::default(),
        NOW,
    )
    .unwrap();
    let note = NOW;
    let card = NOW + 1;
    let model = serde_json::json!({
        "1425279151000": {
            "id": 1425279151000i64, "name": "Vocab", "type": 0,
            "css": ".card { font-family: serif } .kana { color: #e53935 } .gloss { color: blue }",
            "flds": [
                {"name": "Word", "ord": 0}, {"name": "Reading", "ord": 1},
                {"name": "Meaning", "ord": 2},
            ],
            "tmpls": [
                {"ord": 0, "qfmt": "{{Word}} <span class=\"kana\">{{Reading}}</span>",
                 "afmt": "{{FrontSide}}<hr id=answer>{{Meaning}}<br>{{hint:Reading}}"},
                {"ord": 1, "qfmt": "{{Meaning}} {{type:Word}}",
                 "afmt": "{{Meaning}}<hr>{{type:Word}}<br>{{Reading}}"},
            ],
        }
    });
    let fields = [
        "食べる [sound:taberu.mp3]",
        "たべる",
        "to <i>eat</i> [$]x^2[/$]<script>alert(1)</script>",
    ]
    .join("\u{1f}");
    let sql = format!(
        "UPDATE col SET models = '{models}';
         UPDATE notes SET flds = '{fields}', tags = ' verbs n5 ';
         INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due,
             ivl, factor, reps, lapses, left, odue, odid, flags, data)
           VALUES ({rev}, {note}, (SELECT did FROM cards WHERE id = {card}), 1, 0, -1, 0, -1, 9,
             0, 0, 0, 0, 0, 0, 0, 0, ''),
                  ({gone}, {note}, (SELECT did FROM cards WHERE id = {card}), 5, 0, -1, 0, 0, 9,
             0, 0, 0, 0, 0, 0, 0, 0, '');
         INSERT INTO revlog (id, cid, usn, ease, ivl, lastIvl, factor, time, type) VALUES
           ({r1}, {card}, -1, 3, 1, 0, 2500, 1000, 0),
           ({r2}, {card}, -1, 1, 1, 1, 2500, 1000, 1),
           ({r3}, {card}, -1, 4, 4, 1, 2500, 1000, 2),
           ({r4}, {rev}, -1, 2, 1, 0, 2500, 1000, 0),
           ({r5}, {card}, -1, 3, 1, 1, 2500, 1000, 3);",
        models = model.to_string().replace('\'', "''"),
        fields = fields.replace('\'', "''"),
        rev = NOW + 2,
        gone = NOW + 3,
        r1 = NOW - 9 * DAY,
        r2 = NOW - 6 * DAY,
        r3 = NOW - 5 * DAY,
        r4 = NOW - 4 * DAY,
        r5 = NOW - 3 * DAY,
    );
    rebuild(
        base,
        &sql,
        &[("media", br#"{"0": "taberu.mp3"}"#), ("0", MP3)],
    )
}

/// The fuzz corpus's modern package: zstd collection, protobuf manifest.
fn modern() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/apkg/modern.apkg"))
        .unwrap()
}

type Vector = (&'static str, fn() -> Vec<u8>);
const VECTORS: [Vector; 3] = [
    ("exported", exported),
    ("templated", templated),
    ("modern", modern),
];

fn reference(bytes: &[u8]) -> serde_json::Value {
    let parsed = parse_apkg(bytes).unwrap_or_else(|e| panic!("{e} / {}", e.detail()));
    let history: Vec<[i64; 3]> = review_history(bytes)
        .unwrap()
        .into_iter()
        .map(|h| [h.at_ms, h.anki_card_id, h.rating.as_i64()])
        .collect();
    serde_json::json!({ "parsed": parsed, "history": history })
}

#[test]
fn the_importer_still_gives_its_reference_outputs() {
    let dir = dir();
    if std::env::var("FLASH_WRITE_VECTORS").is_ok() {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, build) in VECTORS {
            let bytes = build();
            std::fs::write(dir.join(format!("{name}.apkg")), &bytes).unwrap();
            let json = serde_json::to_string_pretty(&reference(&bytes)).unwrap();
            std::fs::write(dir.join(format!("{name}.json")), json + "\n").unwrap();
        }
    }
    for (name, _) in VECTORS {
        let read = |ext: &str| {
            let path = dir.join(format!("{name}.{ext}"));
            std::fs::read(&path).unwrap_or_else(|e| {
                panic!(
                    "{}: {e} (regenerate with FLASH_WRITE_VECTORS=1)",
                    path.display()
                )
            })
        };
        let stored: serde_json::Value = serde_json::from_slice(&read("json")).unwrap();
        assert_eq!(
            reference(&read("apkg")),
            stored,
            "{name}: the importer's output changed; regenerate with FLASH_WRITE_VECTORS=1"
        );
    }
}
