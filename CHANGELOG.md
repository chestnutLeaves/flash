# Changelog

Every user-visible change to the open core, newest first. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
versions follow [Semantic Versioning](https://semver.org/): before 1.0, a
minor bump may change behaviour and a patch bump only fixes. Each entry
names the issue it answers and the person who raised it.

## [Unreleased]

## [0.2.0] - 2026-09-17

Upgrading from 0.1.0 needs nothing beyond the new image or binary: the
database moves from schema 15 to 22 on the first boot, with every deck,
card, review and media file kept. See "Upgrading" in the README.

### Added

- Decks can be archived. An archived deck keeps its cards, media and
  review history but leaves the study queue until it is unarchived. On
  the web, in the JSON API (`POST /api/v1/decks/{id}/archive` and
  `/unarchive`) and as the MCP tools `archive_deck` and `unarchive_deck`.
  (#1, suggested by @xblr)
- Card generation with your own model. PDFs, images, video, audio,
  YouTube links and pasted text become drafts you review, edit by chat
  and keep. `FLASH_AI_PROVIDER` picks the API: `openai` for any endpoint
  speaking the OpenAI Chat Completions protocol, hosted or on your own
  machine, or `gemini` for the native adapter, which also reads PDFs,
  video and YouTube links. See "Using your own model" in the README.
- Voice mode: study hands-free, natively in the app and on the web. The
  cards are read to you in a natural voice from the `tools/flash-tts`
  sidecar (`FLASH_TTS_URL`, with English, Japanese and Mandarin voices),
  your spoken answer is graded by the model, and you can ask about the
  card, grade, repeat or skip by speaking.
- An Anki import brings the whole review history, and the stats page can
  show a single year.

## [0.1.0] - 2026-09-07

The open core as first published: the FSRS scheduler, the SQLite store
with Anki import and export, and the server with its web UI, JSON API and
MCP endpoint (Streamable HTTP with OAuth 2.1). Accounts are created by an
admin; the first admin enrolls from a one-time link in the boot log. Ships
as a static Linux binary and a Docker image.

[Unreleased]: https://github.com/flash-cards/flash/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/flash-cards/flash/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/flash-cards/flash/releases/tag/v0.1.0
