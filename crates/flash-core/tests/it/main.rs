//! Every integration test of this crate, linked into one binary. Each file
//! is a module, so a filter picks a file or a test:
//!
//!   cargo test -p flash-core --test it queue_tests::

mod goal_tests;
mod locale_tests;
mod queue_tests;
mod scheduler_vectors;
mod vector_gen;
