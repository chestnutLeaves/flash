# MCP card content and editing

Flash 0.3.0 changed the MCP contract. Refresh cached tool definitions
after upgrading from an earlier version. There is no compatibility alias for
`update_card`, `delete_card`, plain `front`/`back` creation arguments, or
`spoken_front`/`spoken_back` responses. Flash's own voice features are unchanged.

## Decks and subdecks

A deck is addressed by its name, and a name is a path: `Pharmacology::Cardio`
is the deck `Cardio` inside `Pharmacology`, as in Anki. Every tool that takes
a deck name takes a path. Spaces around `::` and empty segments are ignored.

`create_deck` and `create_cards` create the decks a path sits in when they do
not exist, in the same transaction as the deck or the cards. One
`create_cards` call fills one deck, so a tree with cards in several subdecks
is one call per subdeck.

A deck includes the decks under it. `start_study_session` and `list_cards` on
`Pharmacology` cover `Pharmacology::Cardio` too, and each card still spends
the daily limits of the deck it lives in. `archive_deck` and `delete_deck`
take the subdecks with the deck. `unarchive_deck` brings them back, and also
the decks the named deck sits in, so an active deck is never inside an
archived one. For the same reason no new deck can be made inside an archived
deck, though cards can still be added to an archived deck that exists.

`list_decks` returns each deck followed by its subdecks. A deck's `due` and
`new` already include its subdecks, so rows are not to be added up.

`rename_deck` takes the deck's current full name as `deck` and its new full
name as `new_name`. Because a name is a path, that renames a deck, moves it
into another deck (`Cardio` to `Pharmacology::Cardio`, creating
`Pharmacology` if needed), or moves it out to the top level. Subdecks, cards
and review history follow. It is refused when a deck already has the new
name (two decks are never merged), when the new name is inside the deck
itself, when a subdeck's full name would pass 200 bytes, and when an active
deck would land inside an archived one.

## Read and study

`list_cards` returns rendered `front_html` and `back_html`, an editable
`source` (note ID/type, front/back HTML, tags, sibling card IDs), and media
metadata. Plain text is HTML-escaped. Formatting is the sanitized content
Flash stores, not unsanitized input. Source fields retain cloze notation.

Use the study tools for quizzes: they return only the rendered question
side until the answer is requested. Clients decide how to render or speak
the content; do not speak raw markup or URLs, or reveal cloze answers.

Pagination uses `before_card_id`: pass the returned `next_cursor` while
`has_more` is true. Default limit is 50, maximum 500, newest IDs first.
A page is also bounded by size (about 64 KB of card JSON), so it may hold
fewer cards than `limit`: a couple of hundred plain cards, a few dozen rich
ones.
Rely on `has_more`, never on the page length, to know when to stop.
`total_count` describes the current matching set, not a snapshot across calls.

Media entries contain `media_id`, `url`, `mime`, `filename`, and `size`.
Fetch the absolute URL with `Authorization: Bearer <connector token>`.
The download route supports the existing range behavior and checks ownership.
It neither redirects credentials to external storage nor grants account API
access. Keep stable `/media/{id}` references in editable HTML. No image,
audio, or video bytes are embedded in MCP results. A client unable to fetch
authenticated references cannot inspect those media through links alone.

## Create and update

`create_cards` accepts 1-500 source objects and creates the named deck if
needed. Fields are `front_html`, `back_html`, optional `tags`, and optional
`note_type`: `basic` (default), `basic_reversed`, `basic_typed`, or `cloze`.
The number of generated cards can exceed the number of source objects.

```json
{"deck":"Pharmacology","cards":[{"front_html":"<b>Drug A</b>: mechanism?","back_html":"Answer"}]}
```

`update_cards` takes a `cards` array. Each entry identifies a `card_id` and
supplies only the source fields to change: `front_html`, `back_html`, or
`tags`. An omitted field is preserved; an empty tags array clears tags.
Explicit empty HTML is subject to the usual card-type validation.

```json
{"cards":[{"card_id":42,"back_html":"<b>Updated answer</b>"}]}
```

For a shared source, copy its exact `sibling_card_ids` into the update.
Submit one edit per source, not one per sibling. All sibling cards update
in place, keeping their IDs and history. Note type stays unchanged. New
cloze slots can create cards; removing existing slots is rejected and
requires an explicit confirmed deletion first. Read again after structural
changes. Legacy imports whose rich source was discarded or cannot be
reconstructed losslessly remain readable. The source's `read_only_reason`
explains the limitation, and a rewrite that would lose content is refused
with `source_unavailable`.

Batches validate ownership, source membership, media, and size limits,
and commit atomically. An error leaves the whole batch unchanged. Duplicate
targets are rejected. The maximum request body remains 1 MiB, so large HTML
batches may need fewer than 500 entries. Writes return source/card IDs,
actual created/updated counts, and resulting deck totals.

## Delete

Call `delete_cards` with explicit `card_ids`. The first response contains
the exact IDs/count, `expires_ms`, and a `confirm_token`; nothing is deleted.
Describe the selection and ask the user once. After approval, repeat the
same IDs with that token. A single-card operation is a batch of one.

`delete_deck` uses the same protocol with the deck name, and deletes the
decks under the deck with it. Its preview and its result carry
`decks_affected`, the deck and then its subdecks by name, and list the cards
those decks show. Name every affected deck when asking. Cards deleted earlier
go with their deck without being counted. If a card joins one of the decks
after the preview, or a subdeck is added, moved in or moved out, request a
fresh confirmation.
Hard-deleting an original target permanently invalidates pending approvals,
even if a replacement later receives the same database ID.

Confirmations last ten minutes, are bound to the account and OAuth client,
and survive reconnects, token refreshes, and restarts. Errors distinguish
`confirmation_invalid`, `confirmation_expired`,
`confirmation_target_mismatch`, and `confirmation_target_changed`. Each
message starts with its code (also in the error's `data.code`) and goes on
to say that the call deleted nothing and what to do next.
Failed transactions do not spend the token. Completed retries return the
original result while its receipt is retained (at least 24 hours past expiry).
