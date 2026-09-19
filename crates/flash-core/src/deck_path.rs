//! Deck paths. A deck's name is its whole path, segments joined by `::`,
//! the way Anki names a subdeck: `Anatomy::Cardiac` is the deck `Cardiac`
//! inside `Anatomy`. There is no parent pointer anywhere; the name is the
//! hierarchy, so everything that addresses a deck by name (import, export,
//! a connector) addresses a subdeck the same way.
//!
//! Two kinds of function live here. The ones that write a path
//! (`normalise`, `join`) clean it: each segment trimmed, empty segments
//! dropped. The ones that read a stored name (`parent`, `ancestors`,
//! `is_descendant`, `reroot`) work on the raw string, because a name
//! stored before paths meant anything may not be clean, and a prefix
//! computed from a cleaned copy would not match the row it came from.

use std::collections::BTreeSet;

pub const SEP: &str = "::";

/// The longest path, in bytes. A subdeck's whole path counts, not its leaf.
pub const MAX_LEN: usize = 200;

/// What separates the levels where a person reads a path. `::` is how a
/// path is stored and what Anki and a connector speak; it is not shown.
pub const SHOWN_SEP: &str = " › ";

/// The path as a person reads it: `Bio › Cells › Organelles`. For a page,
/// a picker, a title; never for storing or looking a deck up.
pub fn display(path: &str) -> String {
    segments(path).join(SHOWN_SEP)
}

/// The non-empty, trimmed segments of a path.
pub fn segments(path: &str) -> Vec<&str> {
    path.split(SEP)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// The path as it should be stored, or `None` when nothing is left of it.
pub fn normalise(path: &str) -> Option<String> {
    let segments = segments(path);
    (!segments.is_empty()).then(|| segments.join(SEP))
}

/// `child` under `parent`, normalised. Either side may be empty: an empty
/// parent is the top level, an empty child is the parent itself.
pub fn join(parent: &str, child: &str) -> Option<String> {
    normalise(&format!("{parent}{SEP}{child}"))
}

/// The last segment: the deck's own name, without the decks it sits in.
pub fn leaf(path: &str) -> &str {
    path.rsplit_once(SEP).map_or(path, |(_, leaf)| leaf).trim()
}

/// The path of the deck this one sits in; `None` at the top level.
pub fn parent(path: &str) -> Option<&str> {
    path.rsplit_once(SEP).map(|(parent, _)| parent)
}

/// Every path this one sits under, outermost first, itself excluded.
pub fn ancestors(path: &str) -> Vec<&str> {
    path.match_indices(SEP).map(|(at, _)| &path[..at]).collect()
}

/// Whether `path` lies somewhere under `root`. A deck is not its own
/// descendant, and `Bio2` is not under `Bio`.
pub fn is_descendant(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.starts_with(SEP))
}

/// `path`, which is `old_root` or lies under it, moved to `new_root`:
/// what a rename or a move does to every deck of the subtree.
pub fn reroot(path: &str, old_root: &str, new_root: &str) -> String {
    debug_assert!(path == old_root || is_descendant(path, old_root));
    format!("{new_root}{}", &path[old_root.len()..])
}

/// The paths and every path they sit under: the decks that must exist for
/// these to be where they say they are.
pub fn closure<'a>(paths: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for path in paths {
        all.extend(ancestors(path).into_iter().map(str::to_string));
        all.insert(path.to_string());
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_path_is_cleaned() {
        assert_eq!(normalise(" Bio :: Cells ").as_deref(), Some("Bio::Cells"));
        assert_eq!(normalise("::Bio::::Cells::").as_deref(), Some("Bio::Cells"));
        assert_eq!(normalise("Plain").as_deref(), Some("Plain"));
        assert_eq!(normalise(" :: "), None);
        // One colon is part of a name, not a separator.
        assert_eq!(normalise("Week 1: intro").as_deref(), Some("Week 1: intro"));
        assert_eq!(segments("A:::B"), vec!["A", ":B"]);
    }

    #[test]
    fn a_path_is_shown_without_its_separator() {
        assert_eq!(
            display("Bio::Cells::Organelles"),
            "Bio › Cells › Organelles"
        );
        assert_eq!(display("Plain"), "Plain");
        // One colon is part of a name and stays.
        assert_eq!(display("Week 1: intro::Day 2"), "Week 1: intro › Day 2");
    }

    #[test]
    fn join_takes_an_empty_side() {
        assert_eq!(
            join("School", "Bio::Cells").as_deref(),
            Some("School::Bio::Cells")
        );
        assert_eq!(join("", "Bio").as_deref(), Some("Bio"));
        assert_eq!(join("School", "").as_deref(), Some("School"));
        assert_eq!(join(" ", ""), None);
    }

    #[test]
    fn a_stored_name_reads_as_a_path() {
        assert_eq!(leaf("A::B::C"), "C");
        assert_eq!(leaf("Plain"), "Plain");
        assert_eq!(parent("A::B::C"), Some("A::B"));
        assert_eq!(parent("Plain"), None);
        assert_eq!(ancestors("A::B::C"), vec!["A", "A::B"]);
        assert!(ancestors("Plain").is_empty());
        // A name stored before paths meant anything: the prefixes are the
        // raw ones, so they match the rows they name.
        assert_eq!(ancestors("A:: B"), vec!["A"]);
        assert_eq!(leaf("A:: B"), "B");
    }

    #[test]
    fn descent_is_by_whole_segments() {
        assert!(is_descendant("Bio::Cells", "Bio"));
        assert!(is_descendant("Bio::Cells::Organelles", "Bio"));
        assert!(!is_descendant("Bio", "Bio"));
        assert!(!is_descendant("Bio2::Cells", "Bio"));
        assert!(!is_descendant("Bio", "Bio::Cells"));
    }

    #[test]
    fn reroot_moves_the_root_and_what_is_under_it() {
        assert_eq!(reroot("Bio", "Bio", "School::Bio"), "School::Bio");
        assert_eq!(
            reroot("Bio::Cells", "Bio", "School::Bio"),
            "School::Bio::Cells"
        );
        assert_eq!(reroot("A::B::C", "A::B", "B"), "B::C");
    }

    #[test]
    fn closure_adds_the_decks_in_between() {
        let all = closure(["A::B::C", "A::D", "E"]);
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        assert_eq!(all, vec!["A", "A::B", "A::B::C", "A::D", "E"]);
    }
}
