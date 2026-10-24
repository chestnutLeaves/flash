//! What the build's scans share (build/literals.rs, build/physical.rs):
//! the crate's files they read and the exemptions they take.

use std::path::{Path, PathBuf};

/// Every file under `dir` with one of `extensions`.
fn files(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            files(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Reads every file with one of `extensions` under the crate's `roots`,
/// in order of path, and fails the build on the problems `check` finds
/// in one (given its path in the crate and its text). `exempt` names
/// files or folders (`src/i18n/packs.rs`) the scan leaves alone, each with why.
pub fn scan(
    roots: &[&str],
    extensions: &[&str],
    exempt: &[(&str, &str)],
    check: impl Fn(&str, &str) -> Vec<String>,
) {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let mut found = Vec::new();
    for root in roots {
        let dir = manifest.join(root);
        println!("cargo::rerun-if-changed={}", dir.display());
        files(&dir, extensions, &mut found);
    }
    found.sort();
    let mut problems = Vec::new();
    for (path, why) in exempt {
        if why.trim().is_empty() {
            problems.push(format!("{path}: exempt with no reason"));
        }
        if !manifest.join(path).exists() {
            problems.push(format!("{path}: exempt, but there is no such file"));
        }
    }
    for file in found {
        let rel = file
            .strip_prefix(&manifest)
            .expect("under the crate")
            .to_string_lossy()
            .replace('\\', "/");
        if exempt
            .iter()
            .any(|(path, _)| rel == *path || (path.ends_with('/') && rel.starts_with(path)))
        {
            continue;
        }
        let source = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{rel}: {e}"));
        problems.extend(check(&rel, &source));
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
