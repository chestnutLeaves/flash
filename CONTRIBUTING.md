# Contributing to Flash

Thanks for looking. Flash is maintained by one person, so the rules
below exist to keep review cheap for both of us.

## Before you start

- Open an issue first for anything beyond a small fix, so we agree on
  the shape before you write it. Scheduling behaviour (FSRS, queue
  order, daily limits) and anything touching accounts or tokens gets
  extra scrutiny.
- Contributions require the contributor license agreement in
  [CLA.md](CLA.md). A bot asks you to sign it on your first pull
  request; it is a one-time click.

## Building and testing

```
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

All three must be clean; CI runs them on every pull request. A Rust
toolchain is all any platform needs (on Windows, with the MSVC build
tools rustup asks for): the cryptography is ring and RustCrypto, with no
OpenSSL, Perl or NASM to install.

The layout, briefly:

- `crates/flash-core`: pure domain code, no IO. Scheduler reference
  vectors live in `tests/fsrs_vectors.json`; don't regenerate them
  casually.
- `crates/flash-store`: every SQL statement. Migrations are append-only:
  never edit a shipped one, add the next slot.
- `crates/flash-server`: HTTP. Handlers stay thin; every rule lives in
  `service.rs` or a `flows` module so the web UI, the JSON API and the
  MCP tools can't drift apart. Templates are askama, behaviour is htmx.

Tests are integration-style and run against an in-memory database; look
at `crates/flash-server/tests/it/` for the harness (`common::*`). Each
crate's integration tests are modules of one test binary, `tests/it`, so
a new test file is a new `mod` line in `tests/it/main.rs`. Run one file
or one test with a filter:

```
cargo test -p flash-server --test it mcp_tests::
cargo test -p flash-server --test it mcp_tests::a_name
```

## Pull requests

- One change per pull request, with tests that would fail without it.
- Explain the behaviour change in the description; commit messages
  should say why, not just what.
- Keep the diff free of formatting churn and unrelated refactors.

This repository is a read-only mirror of a private repository, so
history here arrives in batches rather than merge commits. A pull
request we take arrives with the next sync as a commit of its own. You
are its author when your code went in largely as written, and a
co-author when a maintainer rewrote it from your work. If later work
has since rewritten your change, you are a co-author of the sync's
main commit instead. We then close the pull request with a link to that commit.

## Releases and the changelog

[CHANGELOG.md](CHANGELOG.md) lists every change a person running Flash
would notice, newest first, under an `Unreleased` heading until it ships.
A pull request that changes behaviour adds its line there in the same
change; a refactor or a test does not. The line says what changed for
the user, not which function moved, and ends with the issue it answers
and who raised it: `(#12, suggested by @handle)` for an idea,
`(#12, reported by @handle)` for a bug. A reporter's credit lives in the
changelog and the release notes.

Versions follow semantic versioning. A release is a tag `vX.Y.Z` on this
repository matching the workspace version in `Cargo.toml`; the release
workflow refuses a tag that does not, takes the changelog section as the
release notes, and attaches the static binary and pushes the image.

## Reporting security issues

Please do not open a public issue for anything that could affect other
people's data. Report it privately through the repository's Security
tab, as [SECURITY.md](SECURITY.md) describes, and allow a few days for
a reply.
