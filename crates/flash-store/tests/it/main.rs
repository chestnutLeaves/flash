//! Every integration test of this crate, linked into one binary. One test
//! binary per file cost a link and a few hundred megabytes each; one binary
//! costs one. Each file is a module, so a filter picks a file or a test:
//!
//!   cargo test -p flash-store --test it store_tests::
//!   cargo test -p flash-store --test it store_tests::a_name

mod card_bulk_tests;
mod client_errors_tests;
mod cloze_tests;
mod export_tests;
mod ext_tests;
mod fold_vectors_tests;
mod fuzz_seeds_tests;
mod hostile_input_tests;
mod import_vectors_tests;
mod jobs_tests;
mod lock_tests;
mod mcp_tests;
mod media_lifecycle_tests;
mod media_tests;
mod migration_tests;
mod notes_tests;
mod parse_caps_tests;
mod revision_tests;
mod search_fold_tests;
mod speech_blocks_tests;
mod speech_tests;
mod store_tests;
mod torture_deck_test;
mod unicode_text_tests;
