//! Build-time half of the i18n machinery, shared by every crate that
//! words a page (this one's `build.rs` and a downstream extension's).
//!
//! `catalogs` lists the crate's message files from the filesystem, so
//! the folders under `locales/` are the only list of them: a file added
//! is read, and a folder no language of the table reads fails the build.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::registry::UiLocale;

/// The `.ftl` files under `locales/`, as `(folder, path)`, folders in
/// the table's order and files by name.
fn message_files(locales: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let folders = UiLocale::folders();
    let entries =
        std::fs::read_dir(locales).unwrap_or_else(|e| panic!("{}: {e}", locales.display()));
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if !path.is_dir() {
            problems.push(format!("{}: not a folder of messages", path.display()));
            continue;
        }
        let Some(&folder) = folders.iter().find(|f| **f == name) else {
            problems.push(format!(
                "locales/{name}: no language of the table reads this folder \
                 (flash_core::locale::LANGUAGES chains: {})",
                folders.join(", ")
            ));
            continue;
        };
        let files = std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for file in files {
            let file = file.expect("a readable directory entry").path();
            if file.extension().is_some_and(|e| e == "ftl") {
                found.push((folder, file));
            } else {
                problems.push(format!("{}: not a .ftl file", file.display()));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
    let rank = |folder: &str| folders.iter().position(|f| *f == folder);
    found.sort_by(|a, b| rank(a.0).cmp(&rank(b.0)).then_with(|| a.1.cmp(&b.1)));
    found
}

/// Writes `OUT_DIR/<out>`: the crate's message files as an expression of
/// type `&[(&str, &str)]` (`flash_server::i18n::Resource`), each file
/// `include_str!`ed, for the crate to `include!`.
pub fn catalogs(out: &str) {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let locales = manifest.join("locales");
    println!("cargo::rerun-if-changed={}", locales.display());
    let mut text = String::from("&[\n");
    let mut watched: Vec<PathBuf> = Vec::new();
    for (folder, path) in message_files(&locales) {
        // A folder's own change (a file added) reruns this; a file's
        // text is include_str!'s to track.
        let parent = path.parent().expect("in a folder").to_path_buf();
        if !watched.contains(&parent) {
            println!("cargo::rerun-if-changed={}", parent.display());
            watched.push(parent);
        }
        writeln!(
            text,
            "    ({folder:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .expect("writing to a String");
    }
    text.push_str("]\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets it")).join(out);
    std::fs::write(&dest, text).unwrap_or_else(|e| panic!("{}: {e}", dest.display()));
}
