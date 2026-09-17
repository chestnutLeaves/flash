# Third-party notices

Flash is licensed under the GNU Affero General Public License v3.0
(`LICENSE`). It ships the following third-party work.

## Assets embedded in the `flash-server` binary

| Asset | Version | License | Text |
|---|---|---|---|
| [htmx](https://htmx.org) | 2.0.7 | Zero-Clause BSD | `crates/flash-server/static/LICENSES/htmx-2.0.7-0BSD.txt` |
| [KaTeX](https://katex.org) (JS, CSS, fonts) | 0.16.22 | MIT | `crates/flash-server/static/LICENSES/katex-0.16.22-MIT.txt` |
| [Geist](https://vercel.com/font) and Geist Mono (woff2) | — | SIL Open Font License 1.1 | `crates/flash-server/static/LICENSES/geist-OFL-1.1.txt` |

## Brand marks

The "Connect your AI" page shows the marks of the clients it connects
to (`crates/flash-server/templates/ai_stack.html`): the Claude mark of
Anthropic, the OpenAI mark of OpenAI, and the Grok mark of xAI. They are
those companies' trademarks, used only to identify their products, and
are not covered by this repository's license. Keep them as they are or
remove them; do not reuse them elsewhere.

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

## The voice sidecar (`tools/flash-tts`)

An optional Python service, not part of the binary. Its Docker image and
a local install pull these in:

| Work | License | Role |
|---|---|---|
| [Kokoro](https://github.com/hexgrad/kokoro) (`kokoro`, `misaki`) by hexgrad | Apache-2.0 | The text-to-speech engine and its grapheme-to-phoneme front ends |
| [Kokoro-82M weights](https://huggingface.co/hexgrad/Kokoro-82M) and voice packs | Apache-2.0 | Downloaded at image build or first start into the Hugging Face cache |
| [unidic-lite](https://github.com/polm/unidic-lite) | BSD-3-Clause (dictionary data under the UniDic licence, BSD/LGPL/GPL triple) | Japanese tokenizer dictionary for `misaki[ja]` |
| [PyTorch](https://pytorch.org), [FastAPI](https://fastapi.tiangolo.com), [uvicorn](https://www.uvicorn.org), [NumPy](https://numpy.org) | BSD-3-Clause, MIT, BSD-3-Clause, BSD-3-Clause | Inference, HTTP, arithmetic |
| [espeak-ng](https://github.com/espeak-ng/espeak-ng) | GPL-3.0-or-later | System package the engine's phonemizer calls for English words outside its dictionary. It runs inside the sidecar, a separate program from the AGPL server; GPL-3.0 and AGPL-3.0 are compatible either way |
| [FFmpeg](https://ffmpeg.org) | LGPL-2.1-or-later (GPL when built with GPL components) | System package, run as a separate process to encode each clip as MP4/AAC; nothing links against it |

The Python packages carry their own licences in the installed
distribution metadata (`pip show --verbose <name>`).
