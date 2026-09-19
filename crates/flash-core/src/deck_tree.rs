//! The deck list as a tree: a flat set of deck paths in, rows in display
//! order out, each with its depth, the name to show and counts that
//! include its subdecks.
//!
//! The order is decided here and not by sorting names: `A B` and `A-x`
//! both sort between `A` and `A::B`, so sorted names would put a stranger
//! between a deck and its subdecks.

use std::collections::HashMap;

use crate::deck_path;

/// One deck going in. The counts are its own cards only.
#[derive(Debug, Clone, Copy)]
pub struct Node<'a> {
    pub name: &'a str,
    pub due: u32,
    pub new: u32,
}

/// One deck coming out, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Which of the nodes passed in this is.
    pub index: usize,
    /// The row's parent, as an index into the nodes passed in.
    pub parent: Option<usize>,
    pub depth: u32,
    /// The name to show under the parent: the last segment, or, for a
    /// deck whose parent does not exist as a deck, the rest of its path.
    pub leaf: String,
    /// Its own cards and its subdecks', held to the caps.
    pub due: u32,
    pub new: u32,
}

/// Orders `nodes` as a tree, siblings by name without regard to case.
///
/// A deck hangs under its nearest ancestor that exists; when none does it
/// is a root and keeps its whole path as its name, so a path stored before
/// paths meant anything lists as it always did.
///
/// A parent's counts are its own plus its subdecks', held to `new_cap` and
/// `review_cap`. With each deck's own count already held to its own
/// budget, that is what a session over the parent serves: every card
/// spends its own deck's budget, and the account's budget caps the sum.
pub fn build(nodes: &[Node<'_>], new_cap: u32, review_cap: u32) -> Vec<Row> {
    let by_name: HashMap<&str, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.name, i)).collect();
    let parents: Vec<Option<usize>> = nodes
        .iter()
        .map(|n| {
            deck_path::ancestors(n.name)
                .into_iter()
                .rev()
                .find_map(|a| by_name.get(a).copied())
        })
        .collect();
    let leaves: Vec<String> = nodes
        .iter()
        .zip(&parents)
        .map(|(n, parent)| match parent {
            Some(p) => n.name[nodes[*p].name.len() + deck_path::SEP.len()..]
                .trim()
                .to_string(),
            None => n.name.to_string(),
        })
        .collect();

    let mut roots = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, parent) in parents.iter().enumerate() {
        match parent {
            Some(p) => children[*p].push(i),
            None => roots.push(i),
        }
    }
    let by_leaf = |a: &usize, b: &usize| {
        (leaves[*a].to_lowercase(), &leaves[*a]).cmp(&(leaves[*b].to_lowercase(), &leaves[*b]))
    };
    roots.sort_by(by_leaf);
    for siblings in &mut children {
        siblings.sort_by(by_leaf);
    }

    let mut rows = Vec::with_capacity(nodes.len());
    let mut walk = Walk {
        nodes,
        parents: &parents,
        leaves: &leaves,
        children: &children,
        caps: (review_cap, new_cap),
        rows: &mut rows,
    };
    for root in roots {
        walk.visit(root, 0);
    }
    rows
}

struct Walk<'a> {
    nodes: &'a [Node<'a>],
    parents: &'a [Option<usize>],
    leaves: &'a [String],
    children: &'a [Vec<usize>],
    /// (due, new).
    caps: (u32, u32),
    rows: &'a mut Vec<Row>,
}

impl Walk<'_> {
    /// Emits `i` and its subtree; returns the subtree's (due, new).
    fn visit(&mut self, i: usize, depth: u32) -> (u32, u32) {
        let at = self.rows.len();
        self.rows.push(Row {
            index: i,
            parent: self.parents[i],
            depth,
            leaf: self.leaves[i].clone(),
            due: 0,
            new: 0,
        });
        let (mut due, mut new) = (self.nodes[i].due, self.nodes[i].new);
        for &child in &self.children[i] {
            let (d, n) = self.visit(child, depth + 1);
            due = due.saturating_add(d);
            new = new.saturating_add(n);
        }
        let held = (due.min(self.caps.0), new.min(self.caps.1));
        (self.rows[at].due, self.rows[at].new) = held;
        held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, due: u32, new: u32) -> Node<'_> {
        Node { name, due, new }
    }

    fn shape(rows: &[Row]) -> Vec<(u32, &str)> {
        rows.iter().map(|r| (r.depth, r.leaf.as_str())).collect()
    }

    #[test]
    fn subdecks_follow_their_parent_whatever_sorts_between() {
        // By name, "A B" and "A-x" both sort between "A" and "A::B".
        let nodes = [
            node("A B", 0, 0),
            node("A", 0, 0),
            node("A::B", 0, 0),
            node("A-x", 0, 0),
        ];
        let rows = build(&nodes, 99, 99);
        assert_eq!(
            shape(&rows),
            vec![(0, "A"), (1, "B"), (0, "A B"), (0, "A-x")]
        );
        assert_eq!(rows[1].parent, Some(1), "B hangs under the node named A");
        assert_eq!(rows[0].parent, None);
    }

    #[test]
    fn siblings_sort_without_regard_to_case() {
        let nodes = [node("zeta", 0, 0), node("Alpha", 0, 0), node("beta", 0, 0)];
        assert_eq!(
            shape(&build(&nodes, 9, 9)),
            vec![(0, "Alpha"), (0, "beta"), (0, "zeta")]
        );
    }

    #[test]
    fn a_deck_without_its_parent_keeps_its_path() {
        // No "Bio" deck: "Bio::Cells" lists as it did before paths nested.
        // No "A::B" either: "A::B::C" hangs under "A" as "B::C".
        let nodes = [
            node("Bio::Cells", 0, 0),
            node("A", 0, 0),
            node("A::B::C", 0, 0),
        ];
        assert_eq!(
            shape(&build(&nodes, 9, 9)),
            vec![(0, "A"), (1, "B::C"), (0, "Bio::Cells")]
        );
    }

    #[test]
    fn a_parent_counts_its_subdecks_up_to_the_caps() {
        let nodes = [
            node("A", 1, 2),
            node("A::B", 3, 4),
            node("A::B::C", 5, 6),
            node("Other", 7, 8),
        ];
        let rows = build(&nodes, 100, 100);
        let counts: Vec<(u32, u32)> = rows.iter().map(|r| (r.due, r.new)).collect();
        assert_eq!(counts, vec![(9, 12), (8, 10), (5, 6), (7, 8)]);

        // The account's new-card budget caps the sum, not each part.
        let rows = build(&nodes, 10, 100);
        let new: Vec<u32> = rows.iter().map(|r| r.new).collect();
        assert_eq!(new, vec![10, 10, 6, 8]);
    }

    #[test]
    fn every_node_comes_out_once() {
        let nodes = [node("B::x", 0, 0), node("A", 0, 0), node("B", 0, 0)];
        let mut seen: Vec<usize> = build(&nodes, 0, 0).iter().map(|r| r.index).collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2]);
    }
}
