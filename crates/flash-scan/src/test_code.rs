//! The workspace's test code, which the shipped-source model leaves out:
//! every file under a crate's `tests/`, and each item under `src/` that is
//! compiled only for tests (`#[cfg(test)]`, inline or out of line). What a
//! rule about tests reads (how they wait, what they open) it reads here,
//! by line, with the file's text.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;

use crate::{cfg, module_dir, out_of_line, relative, Crate};

/// One file's test code: the whole of a file under `tests/`, or the
/// `#[cfg(test)]` items of one under `src/`.
#[derive(Debug, Clone)]
pub struct TestCode {
    pub path: PathBuf,
    /// `crate/rel/path.rs`, forward slashes.
    pub rel: String,
    pub text: String,
    /// The test code's lines, 1-based and inclusive.
    pub ranges: Vec<(usize, usize)>,
}

impl TestCode {
    /// Whether 1-based `line` is test code.
    pub fn covers(&self, line: usize) -> bool {
        self.ranges.iter().any(|&(a, b)| a <= line && line <= b)
    }

    /// The test code's lines with their 1-based numbers.
    pub fn lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l))
            .filter(|(n, _)| self.covers(*n))
    }
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>()
                .is_ok_and(|m| m.path().is_ident("test"))
    })
}

/// The `#[cfg(test)]` items among `items`, and inside the modules that
/// ship: their line ranges, and the files of those declared out of line
/// (`#[cfg(test)] mod tests;`), by name beside `base`.
fn walk(
    items: &[syn::Item],
    base: &Path,
    ranges: &mut Vec<(usize, usize)>,
    files: &mut Vec<PathBuf>,
) {
    for item in items {
        if test_only(cfg::item_attrs(item)) {
            let span = item.span();
            ranges.push((span.start().line, span.end().line));
            if let syn::Item::Mod(m) = item {
                if m.content.is_none() {
                    files.extend(out_of_line(base, &m.ident));
                }
            }
        } else if let syn::Item::Mod(m) = item {
            if let Some((_, inner)) = &m.content {
                walk(inner, &base.join(m.ident.to_string()), ranges, files);
            }
        }
    }
}

fn whole(krate: &Crate, path: PathBuf, text: String) -> TestCode {
    let lines = text.lines().count().max(1);
    TestCode {
        rel: format!("{}/{}", krate.name, relative(&krate.dir, &path)),
        path,
        text,
        ranges: vec![(1, lines)],
    }
}

impl Crate {
    /// This crate's test code, sorted by path.
    pub fn test_code(&self) -> Vec<TestCode> {
        let read = |path: &Path| {
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        };
        let mut out = Vec::new();
        let mut tests = Vec::new();
        crate::walk(&self.dir.join("tests"), &mut tests);
        for path in tests {
            let text = read(&path);
            out.push(whole(self, path, text));
        }
        let mut sources = Vec::new();
        crate::walk(&self.dir.join("src"), &mut sources);
        let mut test_files = Vec::new();
        let mut read_sources = Vec::new();
        for path in sources {
            let text = read(&path);
            let ast = crate::parse(&path.display().to_string(), &text);
            let base = module_dir(&path).unwrap_or_default();
            let mut ranges = Vec::new();
            walk(&ast.items, &base, &mut ranges, &mut test_files);
            read_sources.push((path, text, ranges));
        }
        for (path, text, ranges) in read_sources {
            if test_files.iter().any(|f| path == *f || path.starts_with(f)) {
                out.push(whole(self, path, text));
            } else if !ranges.is_empty() {
                out.push(TestCode {
                    rel: format!("{}/{}", self.name, relative(&self.dir, &path)),
                    path,
                    text,
                    ranges,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_items_are_found_inline_and_nested() {
        let text = "fn shipped() {}\n\
                    #[cfg(test)]\n\
                    mod tests {\n\
                    fn helper() {}\n\
                    }\n\
                    mod inner {\n\
                    #[cfg(test)]\n\
                    fn probe() {}\n\
                    #[cfg(not(test))]\n\
                    fn real() {}\n\
                    }\n\
                    #[cfg(test)]\n\
                    mod far;\n";
        let ast = syn::parse_file(text).unwrap();
        let (mut ranges, mut files) = (Vec::new(), Vec::new());
        walk(&ast.items, Path::new("src"), &mut ranges, &mut files);
        assert_eq!(ranges, vec![(2, 5), (7, 8), (12, 13)]);
        assert_eq!(files, vec![Path::new("src/far.rs"), Path::new("src/far")]);
    }
}
