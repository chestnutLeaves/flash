//! Search folding against its list of cases (`tests/fold_vectors.json`):
//! every alphabet's case, every accent and mark, the letters with no
//! decomposition, and mixed text. The list is data so that another
//! implementation of the rule can check itself against the same cases.

use flash_store::fold::fold;

const VECTORS: &str = include_str!("../fold_vectors.json");

#[test]
fn every_case_folds_as_listed() {
    let doc: serde_json::Value = serde_json::from_str(VECTORS).unwrap();
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the list has its cases");
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let folded = case["folded"].as_str().unwrap();
        assert_eq!(fold(text), folded, "fold({text:?})");
    }
}
