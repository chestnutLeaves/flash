/// The doors to `Words` that skip the catalog, each with the label its
/// reason comment starts with.
const DOORS: &[(&str, &str)] = &[
    ("theirs", "theirs"),
    ("for_machines", "for machines"),
    ("for_admins", "for admins"),
    ("kept", "kept"),
    ("from_store", "from the store"),
];

/// Every door to `Words` that skips the catalog, under `src`, said why:
/// a `// <label>: <reason>` comment on its line or one of the four above
/// (`// theirs: the deck's name`). A door with no reason is a sentence
/// that may have been typed into the code.
pub fn doors(src: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(src, &mut files);
    files.sort();
    let mut problems = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            for (door, label) in DOORS {
                let called = line.contains(&format!("Words::{door}("));
                if !called || line.contains(&format!("fn {door}(")) {
                    continue;
                }
                let said = lines[i.saturating_sub(4)..=i].iter().any(|l| {
                    l.split_once(&format!("// {label}:"))
                        .is_some_and(|(_, why)| !why.trim().is_empty())
                });
                if !said {
                    let shown = file.strip_prefix(src).unwrap_or(&file).display();
                    problems.push(format!(
                        "{shown}:{}: Words::{door} with no `// {label}: <why>` beside it",
                        i + 1
                    ));
                }
            }
        }
    }
    problems
}
