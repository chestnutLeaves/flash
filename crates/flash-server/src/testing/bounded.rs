//! The bounded-input scan: every string and list a request can carry is
//! bounded before a handler sees it. A request struct's field names its
//! bound with `deserialize_with` (a `bounds::` deserializer), and this
//! scan walks every `Deserialize` type in the crates it is given and
//! fails for a field of an unbounded type that names none. A type that
//! is not a request is listed by its crate with the reason, and the list
//! is checked against the scan so it cannot rot. Each crate's tests run
//! it over their own crates and their own list.

use flash_scan::{Crate, Field, TypeDecl};

/// The open core's crates, by directory name: this crate's test scans
/// them, and a downstream extension's test scans every other crate.
pub const OPEN_CORE: &[&str] = &["flash-core", "flash-store", "flash-server"];

const UNBOUNDED: &[&str] = &[
    "String", "str", "Vec", "HashMap", "BTreeMap", "HashSet", "Value", "Cow", "Box", "Bytes",
];

pub fn derives_deserialize(t: &TypeDecl) -> bool {
    t.derives.iter().any(|d| d == "Deserialize")
}

fn unbounded_fields(t: &TypeDecl) -> Vec<&Field> {
    t.fields
        .iter()
        .filter(|f| f.ty.idents.iter().any(|i| UNBOUNDED.contains(&i.as_str())))
        .filter(|f| {
            !f.deserialize_with
                .as_deref()
                .is_some_and(|d| d.contains("bounds::"))
        })
        .collect()
}

fn field_label(f: &Field) -> String {
    match (&f.variant, &f.name) {
        (Some(v), Some(n)) => format!("{v}.{n}: {}", f.ty.compact),
        (Some(v), None) => format!("{v}({})", f.ty.compact),
        (None, Some(n)) => format!("{n}: {}", f.ty.compact),
        (None, None) => f.ty.compact.clone(),
    }
}

/// Panics on a request field with no bound in `crates`, and on an entry
/// of `not_a_request` (type name, reason) the scan no longer finds
/// unbounded there.
pub fn assert_requests_bounded(crates: &[Crate], not_a_request: &[(&str, &str)]) {
    let mut offenders = Vec::new();
    let mut listed_seen: Vec<String> = Vec::new();
    for krate in crates {
        for file in krate.source_files() {
            for u in &file.uses {
                if u.path.last().is_some_and(|l| l == "Deserialize") && u.rename.is_some() {
                    panic!(
                        "{}/{}:{}: a renamed Deserialize import hides derives from the scan",
                        krate.name, file.rel, u.line
                    );
                }
            }
            for declared in file.types.iter().filter(|t| derives_deserialize(t)) {
                let fields = unbounded_fields(declared);
                if fields.is_empty() {
                    continue;
                }
                if not_a_request.iter().any(|(t, _)| *t == declared.name) {
                    listed_seen.push(declared.name.clone());
                    continue;
                }
                for field in fields {
                    offenders.push(format!(
                        "{}/{}:{}: {} `{}`",
                        krate.name,
                        file.rel,
                        field.line,
                        declared.name,
                        field_label(field)
                    ));
                }
            }
        }
    }
    let stale: Vec<&str> = not_a_request
        .iter()
        .map(|(t, _)| *t)
        .filter(|t| !listed_seen.iter().any(|s| s == t))
        .collect();
    assert!(
        offenders.is_empty(),
        "request fields with no bound (add `#[serde(deserialize_with = \"…bounds::…\")]`, \
         or list the type as not a request, with a reason):\n{}",
        offenders.join("\n")
    );
    assert!(
        stale.is_empty(),
        "listed as not a request, but the scan finds no such type with an unbounded field: {}",
        stale.join(", ")
    );
}
