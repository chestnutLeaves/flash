# MCP card content and editing

Flash 0.3.0 changed the MCP contract. Refresh cached tool definitions
after upgrading from an earlier version. There is no compatibility alias for
`update_card`, `delete_card`, or plain `front`/`back` creation arguments.

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

## Goal dates

A deck can have a goal date: the day the user needs to know it by, as
`YYYY-MM-DD` on the account's calendar. `create_deck` takes it as
`goal_date`, and `set_limits` with a deck sets it, moves it, or clears it
with an empty string. It must be after today and within five years.

While the date is ahead, the deck's new cards come at a pace that brings
every one in before its last days (about an eighth of the time left, between
three and fourteen days), which are kept for review. That pace replaces the
deck's new-card limit and comes on top of the account's. A review that would
be set after the date, for a card that would by then be less than 95% likely
to be remembered (5 points above the account's desired retention, at most
99%), comes back on one of those last days instead. A deck inside one with a
goal follows it unless it has its own.

`list_decks` and `get_limits` show a deck's `goal`: its `date`, `days_left`,
`new_per_day`, `learned` and `total` over the deck and its subdecks,
`on_track`, and `behind_by`, the new cards behind the plan made when the date
was set (adding cards makes a new plan, so only skipped days count). From the
day on the deck is studied as any other, and once the date has passed `goal`
says `passed: true` until the date is cleared or the deck archived.

## Read and study

`list_cards` returns rendered `front_html` and `back_html`, an editable
`source` (note ID/type, front/back HTML, tags, sibling card IDs), and media
metadata. Plain text is HTML-escaped. Formatting is the sanitized content
Flash stores, not unsanitized input. Source fields retain cloze notation.

Use the study tools for quizzes: they return only the rendered question
side until the answer is requested. Clients decide how to render or speak
the content; do not speak raw markup or URLs, or reveal cloze answers.

`list_tags` names the user's tags with the number of cards carrying each,
across every deck or, with `deck` (a path), inside one deck and its
subdecks. `start_study_session` takes those names as `tags`: a card is in
the session only if it carries every one of them, at most ten, and a
`deck` given alongside narrows that deck rather than being ignored as it
was before. `tag` is still accepted and is simply one more tag.

When a session's queue is empty, its `note` says why: today's limits
hold cards back, everything is caught up, or the scope has no cards at
all. An account with no cards is told so, with an offer to make some,
instead of being called caught up; `list_decks` carries the same `note`
while the account has no cards.

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

## Panels

Hosts that render MCP Apps views (the `io.modelcontextprotocol/ui`
extension) show a Flash panel in the conversation. What the model reads
is the same with or without a panel.

- When `start_study_session` runs, the panel shows the card's front, a
  Show answer button, then Again, Hard, Good and Easy, under a progress
  bar ("2 / 10": reviews done against reviews done plus the queue). Every
  tap records a review, as `submit_review` does.
- When `create_cards` or `update_cards` runs, the panel shows the cards
  written, one at a time with both sides, paged with arrows or a swipe.
  An edited card shows each side it changed beside what that side said
  before. After cards are made, a button starts studying their deck in
  the same panel. `restore_cards` opens the same view on its one card:
  when it lists versions, beside what the newest earlier version said;
  after a restore, beside the text the restore replaced.
- When a session runs out, the panel says when the next cards come
  ("12 cards due tomorrow"), from the view's `next_due` (`at_ms`,
  `in_days`, `cards`), and draws the week ahead from its `week` (the
  seven study days after today, each `at_ms` and `cards`).
  `panel_show_answer` carries `comes_back_in_ms`, how long until the
  card comes back after each grade (Again, Hard, Good, Easy), which the
  grade buttons show. A study view also carries `next_card`, the front
  of the card expected after this one: since every panel call goes
  through the host, the panel asks for backs ahead (this card's and the
  next one's) and shows the next card the moment a grade is tapped,
  recording the grade behind it and following `panel_grade`'s answer
  when it names another card. A made-cards view in one deck carries `deck_url`,
  that deck's page on the site. `start_study_session` carries the same `next_due`
  for the model, and its `note`, like `submit_review`'s, says it in words.
  A view with `reminders_offer` shows a Remind me button beside the next
  due day that calls `panel_reminders` with the browser's time zone; the
  open server never sends it. A view held by the day's
  new-card limit offers "Study N more" (N the cards waiting, at most 10),
  which calls `boost_new_today` and then `start_study_session`. A
  finished view links to the site's stats page, from `site_url`.
  A tap on a card's front shows its answer, as Show answer does. On the
  older bridge, the panel names the page its view continues on
  (`setOpenInAppUrl`: the deck for cards just made, the study page for a
  session), for the host's own "Open in" button.
  A session's card has a menu beside Expand with Edit card and Archive
  card. Archive card calls `panel_archive`, which takes the card out of
  study (it stays in its deck, schedule and history kept) and answers
  with the panel's next card, or the finished session, as a grade does;
  the header then reads "Card archived" with Undo, which calls it again
  with `archived: false` and shows the card again, front up. The panel
  tells the model either way. A host that refuses the tool stops being
  offered it.
  Edit card is the quick edit: `panel_edit_source`
  gives the two fields of the note behind the card (a reversed or cloze
  card shows only part of them), as they are, images included, and the
  user may type and delete there but not add an image or formatting.
  `panel_save_edit` saves them (the note's type and tags unchanged; an
  edit that would take away the card being studied is refused) and
  returns the card's front, its back and the next card's front, which
  the panel shows in place; the panel then tells the model the card was
  edited. Advanced edit, under the fields, opens the card's own page on
  the site (`/cards/{id}`, the `card_url` that `panel_edit_source`
  gives with the fields, so a host that hands the panel only what the
  model reads still has it), a plain link: no sign-in
  travels with it. A host that refuses the quick edit's tools (one
  holding an earlier tool list) gets Advanced edit alone.

  The panel offers the host `inline` and `fullscreen` in `ui/initialize`
  and asks for full screen (`ui/request-display-mode`, or
  `requestDisplayMode` on the older bridge) only when the user taps
  Expand, in the header wherever the host lists `fullscreen`, or Show
  all, on a card too long for the inline card even at its smallest text.
  A host that lists no `fullscreen` gets that card grown to fit instead. A host that opens full screen in a second frame
  (ChatGPT does) runs two copies of the page: each keeps the session's
  latest state in its origin's `localStorage` and follows the other's.

- The panel is the resource `ui://flash/panel-v<N>.html`
  (`text/html;profile=mcp-app`), the one `resources/list` lists. Its
  version goes up with every change, because hosts cache views by URI.
  Every earlier URI, back to `ui://flash/study-panel-v2.html`, is still
  answered with the current page, since hosts keep the tool lists they
  were given. Its CSP loads media and the site's font from this server
  only and fetches nothing.
- The tools that open it (`start_study_session`, `create_cards`,
  `update_cards`, `restore_cards`) name the panel in
  `_meta.ui.resourceUri`. Their
  results carry the panel's view in `_meta["flash/panel"]`, which hosts
  keep from the model; its `view` is `study` or `cards`. A host that says
  it renders the extension does not always draw this panel, so the model
  still asks the first card, and where the panel shows, the question
  also appears in chat.
- The panel's own tools, `panel_show_answer`, `panel_grade`,
  `panel_cards` (the next cards to page through, at most 20 at a time),
  `panel_reminders`, `panel_edit_source`, `panel_save_edit` and
  `panel_archive`, are
  visible to the app only
  (`_meta.ui.visibility: ["app"]`).
  `start_study_session`, `end_session` and `boost_new_today` are visible
  to both; every other tool, `create_cards`, `update_cards` and
  `restore_cards` included,
  to the model only, so the panel can never reach it. `panel_grade` does
  not record a card the model already graded in chat; it answers
  `recorded: false` with the current card. A page's first grade carries
  `ui_locale`, the language the host told the panel its person reads
  (MCP Apps' `locale`, or ChatGPT's `window.openai.locale`).
- A tool call's `_meta["openai/locale"]` (ChatGPT; older clients send
  `webplus/i18n`) is kept as the account's language from its assistant,
  as is a panel's `ui_locale`: at most once a day per account, or when it
  changes. While nobody chose a language, the newest one the account's
  browser, assistant or phone said is the account's.
- The panel holds no credentials: its calls go through the host. Card
  media in the panel's copy of a card are signed links,
  `/mcp-media/{id}/s?e=…&s=…`, good for a day and signed with a key the
  server draws at start. What the model reads keeps plain `/media/{id}`
  references.

## Create and update

`create_cards` accepts 1-500 source objects and creates the named deck if
needed. Fields are `front_html`, `back_html`, optional `tags`, and optional
`note_type`: `basic` (default), `basic_reversed`, `basic_typed`, or `cloze`.
The number of generated cards can exceed the number of source objects.

```json
{"deck":"Pharmacology","cards":[{"front_html":"<b>Drug A</b>: mechanism?","back_html":"Answer"}]}
```

The card fields' schema descriptions tell assistants how to write cards
worth learning from, for any subject:
- The front is answerable on its own and holds one idea. It carries the
  choices of a multiple-choice question and the context a case depends
  on. Lists and multi-part rules become several cards, or a cloze.
- The back is the exact answer, as short as it can be, with anything
  more after it.
- Both sides keep the material's own terms and numbers, write in HTML
  what plain text loses (`10<sup>5</sup>`), and keep each text in one
  script.

Each card also says how each side is read aloud, and in which language:
`front_speech` and `back_speech`, lists of blocks that follow the side as
written. A block is words in one language, by ISO 639-1 code with a region
where the accent matters (`{"lang":"es-MX","text":"¿Dónde está?"}`), or a
cue said in the listener's own language instead (`{"say":"equation"}` for
a formula too long to read, `{"say":"look"}` for anything that cannot be
said). The blocks leave out what is not read aloud (citations, a
romanization next to the script it spells, emoji, images) and write a word
as it is said where its spelling is ambiguous (Japanese kanji in kana).
The schema lists both fields as required so assistants write them with the
sides; a card without them is saved all the same. An optional `languages`
list (`["ja","en"]`), on `create_cards` and `create_deck`, declares the
deck's languages.

```json
{"deck":"Japanese","languages":["ja","en"],"cards":[{"note_type":"basic_reversed",
 "front_html":"How do you say cat in Japanese?","back_html":"猫（ねこ）",
 "front_speech":[{"lang":"en","text":"How do you say cat in Japanese?"}],
 "back_speech":[{"lang":"ja","text":"ねこ"}]}]}
```

`update_cards` takes a `cards` array. Each entry identifies a `card_id` and
supplies only the source fields to change: `front_html`, `back_html`, or
`tags`. An omitted field is preserved; an empty tags array clears tags.
Explicit empty HTML is subject to the usual card-type validation. A side
whose words change loses its speech blocks unless the edit sends new ones
(`front_speech`, `back_speech`); a side it keeps keeps its blocks.

```json
{"cards":[{"card_id":42,"back_html":"<b>Updated answer</b>"}]}
```

For a shared source, copy its exact `sibling_card_ids` into the update:
every card the source makes, the edited card included. Submit one edit
per source, not one per sibling. A mismatch is refused with the ids the
source has, and two edits that reach one source with both card IDs. All sibling cards update
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
actual created/updated counts, and resulting deck totals. `update_cards`
also returns `revision_ids`, one per entry of `source_ids`: the earlier
version of that source the edit kept, or `null` when the edit changed
nothing (the same text, type and tag set).

## Earlier versions and restore

Every edit of a card's source, through `update_cards`, the web editor
or the app, keeps the source as it was (note type, both
fields, tags) as a revision, in the same transaction. The first edit of
a card that has no note yet (an older card, or one from an import) keeps
the card's own text the same way. A source keeps its 20 newest revisions,
none older than 90 days; older ones go when it is next edited. Deleting
the cards' deck or the account deletes them.

`restore_cards` takes a `card_id` from `list_cards`. Alone, it lists the
card's earlier versions, newest first, each with a `revision_id`,
`replaced_at_ms`, `replaced_by` (what made the edit that replaced it:
`web`, `mcp`, `api` or `restore`, or a downstream extension's own word),
the note type, short
plain-text `front_preview` and `back_preview`, and `tags`. Confirm with
the user, then call it again with that `revision_id`:

```json
{"card_id":42,"revision_id":7}
```

The restore goes through the same path as an edit: the source and its
sibling cards are rewritten in place, keeping their IDs, schedules and
review history, and the text it replaces becomes a revision too (by
`restore`), so a restore can itself be undone. It answers with the fields
`update_cards` returns. A revision of another card's source, or of
another account, is not found; a restore that would remove cards is
refused with `implicit_card_removal` (delete them explicitly first), and
one that adds cards meets a card limit, where the server sets one, like
any edit.

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
