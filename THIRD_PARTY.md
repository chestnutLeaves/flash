# Third-party notices

Flash is licensed under the GNU Affero General Public License v3.0
(`LICENSE`). It ships the following third-party work.

## Assets embedded in the `flash-server` binary

| Asset | Version | License | Text |
|---|---|---|---|
| [htmx](https://htmx.org) | 2.0.7 | Zero-Clause BSD | `crates/flash-server/static/LICENSES/htmx-2.0.7-0BSD.txt` |
| [KaTeX](https://katex.org) (JS, CSS, fonts) | 0.16.22 | MIT | `crates/flash-server/static/LICENSES/katex-0.16.22-MIT.txt` |
| [Geist](https://vercel.com/font) and Geist Mono (woff2, subset in two parts per face by `tools/fonts/geist_web.py`; Geist SemiBold is also inside the README's `screenshots/logo.svg`) | 1.800 | SIL Open Font License 1.1 | `crates/flash-server/static/LICENSES/geist-OFL-1.1.txt` |

## Rust dependencies

Every crate in `Cargo.lock` carries its own license, one of those listed
in `deny.toml` (Apache-2.0, MIT, MIT-0, BSD-2-Clause, BSD-3-Clause, 0BSD,
ISC, MPL-2.0, Unicode-3.0, Zlib, BSL-1.0, CC0-1.0, CDLA-Permissive-2.0,
Unlicense, bzip2-1.0.6); `cargo deny` refuses any other. List them with
[cargo-license](https://github.com/onur/cargo-license):

```
cargo install cargo-license
cargo license --workspace
```

The FSRS scheduler is the [`fsrs`](https://crates.io/crates/fsrs) crate
by the Open Spaced Repetition project (BSD-3-Clause); the reference
vectors under `crates/flash-core/tests/` were generated with
[py-fsrs](https://github.com/open-spaced-repetition/py-fsrs).
