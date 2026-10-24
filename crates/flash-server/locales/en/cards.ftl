### The open core's words in English: a deck's list of cards, and the
### card editor (adding and editing a card). A note is what a person
### writes; it makes one card, or several (a reversed note makes two, a
### cloze note one per blank). Every message has a note above it: where
### it shows, who reads it, and what each variable holds.

## A deck's list of cards

# Shown when the deck has no cards.
cards-none = No cards.
# Shown when the search or the filter leaves no card.
cards-none-filtered = No cards match.
# Chip above the list while it shows the archived cards; clicking it
# goes back to the cards in study.
cards-chip-archived = Archived
# The filter's choice, and its chip, for the deck's own cards without
# its subdecks'.
cards-own = This deck only
# Spoken name of a chip's remove button. $name is what the chip says (a
# tag, a deck, "Archived").
cards-chip-remove = Remove { $name } from the filter
# Spoken name of the choice between cards with any of the chosen tags
# and cards with all of them.
cards-match-label = Match tags
# That choice: a card with any one of the tags.
cards-match-any = Any
# That choice: only a card with every one of the tags.
cards-match-all = All
# Link that clears the whole filter.
cards-chip-clear = Clear
# Spoken name of the box that selects every card on the page.
cards-select-page = Select every card on this page
# Spoken name of a card's box.
cards-select-card = Select this card
# Tooltip of the label under a card that lives in a subdeck. $name is
# that subdeck's name.
cards-from-title = In { $name }. Show only its cards.
# Column heading: the card's question side.
cards-col-front = Front
# Column heading: the card's answer side.
cards-col-back = Back
# Column heading: the card's tags.
cards-col-tags = Tags
# Spoken name of the pager under the list.
cards-pages = Card pages
# Pager button to the page before.
cards-previous = Previous
# Pager button to the page after.
cards-next = Next
# Where the pager is. $page is this page, $pages how many, $count how
# many cards.
cards-page-of = Page { $page } of { $pages } · { $count } cards
# Spoken name of a card's edit button.
cards-edit-label = Edit card
# Tooltip of that button.
cards-edit = Edit
# Spoken name of a card's unarchive button.
cards-unarchive-label = Unarchive card
# Tooltip of that button.
cards-unarchive = Unarchive
# Spoken name of a card's archive button.
cards-archive-label = Archive card
# Tooltip of that button.
cards-archive = Archive
# Spoken name of a card's delete button.
cards-delete-label = Delete card
# Tooltip of that button.
cards-delete = Delete
# What an action on the selected cards did, in the note at the foot of
# the screen. $count is how many cards it changed.
cards-bulk-archived = { $count ->
        [one] { $count } card archived
       *[other] { $count } cards archived
    }
# The same for bringing archived cards back into study. $count is how
# many cards came back.
cards-bulk-unarchived = { $count ->
        [one] { $count } card back in study
       *[other] { $count } cards back in study
    }
# The same for deleting. $count is how many cards were deleted.
cards-bulk-deleted = { $count ->
        [one] { $count } card deleted
       *[other] { $count } cards deleted
    }
# The same for moving. $count is how many cards moved (a note's other
# cards go with it), $deck the deck they went to.
cards-bulk-moved = { $count ->
        [one] { $count } card moved to { $deck }
       *[other] { $count } cards moved to { $deck }
    }
# The same for adding tags. $count is how many cards got a new tag.
cards-bulk-tagged = { $count ->
        [one] Tags added to { $count } card
       *[other] Tags added to { $count } cards
    }
# The same for removing tags. $count is how many cards lost a tag.
cards-bulk-untagged = { $count ->
        [one] Tags removed from { $count } card
       *[other] Tags removed from { $count } cards
    }
# The note after Undo put an action back.
cards-bulk-undone = Undone
# Button in that note that puts the action back.
cards-bulk-undo = Undo
# An action on "all the cards that match" found a different number than
# the page showed: cards were added or changed meanwhile.
cards-error-bulk-changed = The list changed while you were choosing. Select the cards again.
# An action named more cards than one action takes. $max is the limit.
cards-error-bulk-many = At most { $max } cards can be changed at once.
# Moving cards into a deck that is archived.
cards-error-move-archived = That deck is archived. Unarchive it first.
# Adding or removing tags with none written.
cards-error-tags-none = Write at least one tag.
# Badge naming how a card comes from its note: the blank it hides.
# $n is the blank's number.
cards-kind-cloze = cloze { $n }
# Badge: the card asks the back and answers with the front.
cards-kind-reversed = reversed
# Badge: the card asks for its answer to be typed.
cards-kind-typed = typed

## A card's own page and the edit dialog

# Browser tab title of a card's own page.
editor-page-title = Edit card - Flash
# Spoken name of the link back to the card's deck.
editor-crumbs = Deck
# Heading of the editor (page and dialog).
editor-heading = Edit card
# Label of the tags field.
editor-tags = Tags
# Hint under the tags field.
editor-tags-hint = Comma-separated, applied to every card of this note.
# Disclosure listing the note's earlier versions. $count is how many.
editor-history =
    { $count ->
        [one] History ({ $count } earlier version)
       *[other] History ({ $count } earlier versions)
    }
# When an earlier version was replaced, and by what. $ago is how long
# ago (when-*-ago), $by what replaced it (editor-by-*).
editor-replaced = { $ago }, before { $by }
# What replaced a version: an edit on this website.
editor-by-web = an edit here
# What replaced a version: an AI assistant's edit.
editor-by-mcp = an assistant's edit
# What replaced a version: an edit in the phone app.
editor-by-app = an edit in the app
# What replaced a version: an edit from another device.
editor-by-sync = an edit made elsewhere
# What replaced a version: an earlier version brought back.
editor-by-restore = a restore
# Button that brings an earlier version back.
editor-restore = Restore
# Brief note in the editor once an earlier version is back.
editor-restored = Restored the earlier version.
# Brief note in the editor once its changes are saved.
editor-saved = Saved.
# Button that leaves without saving.
editor-cancel = Cancel
# Button that saves the card.
editor-save = Save

## The editor itself

# Label of the note type picker.
editor-type = Type
# The note types, as Anki names them. A plain question and answer.
note-type-basic = Basic
# A question and answer, plus a second card the other way round.
note-type-reversed = Basic (and reversed card)
# A question whose answer is typed.
note-type-typed = Basic (type in the answer)
# A text with blanks to fill in.
note-type-cloze = Cloze
# Badge when several cards share one note. $count is how many.
editor-siblings = { $count } cards share this note
# Tooltip of that badge.
editor-siblings-hint = Saving regenerates every card of this note
# Spoken name of the formatting toolbar.
editor-toolbar = Formatting
# Tooltip of the toolbar's bold button; Ctrl+B is its shortcut.
editor-bold-tip = Bold (Ctrl+B)
# Spoken name of the bold button.
editor-bold = Bold
# Tooltip of the italic button; Ctrl+I is its shortcut.
editor-italic-tip = Italic (Ctrl+I)
# Spoken name of the italic button.
editor-italic = Italic
# Tooltip of the underline button; Ctrl+U is its shortcut.
editor-underline-tip = Underline (Ctrl+U)
# Spoken name of the underline button.
editor-underline = Underline
# Tooltip and spoken name of the strikethrough button.
editor-strike = Strikethrough
# Tooltip and spoken name of the superscript button.
editor-superscript = Superscript
# Tooltip and spoken name of the subscript button.
editor-subscript = Subscript
# The text colour menu.
editor-text-color = Text color
# A colour's swatch in it. $hue is the colour (hue-*).
editor-text-hue = Text { $hue }
# The swatch that removes the colour.
editor-no-color = No color
# The highlight menu.
editor-highlight = Highlight
# A colour's swatch in it. $hue is the colour (hue-*).
editor-highlight-hue = Highlight { $hue }
# The swatch that removes the highlight.
editor-no-highlight = No highlight
# Tooltip and spoken name of the bulleted list button.
editor-bulleted = Bulleted list
# Tooltip and spoken name of the numbered list button.
editor-numbered = Numbered list
# Tooltip and spoken name of the button that removes formatting.
editor-clear = Clear formatting
# Tooltip of the button that attaches a file.
editor-attach-tip = Attach image, audio or video
# Its spoken name.
editor-attach = Attach media
# Tooltip of the button that blanks the selection on a cloze note.
editor-cloze-tip = Cloze deletion (Ctrl+Shift+C; Ctrl+Shift+Alt+C reuses the last number)
# Its spoken name.
editor-cloze = Cloze deletion
# Tooltip of the button that shows the field's HTML.
editor-source-tip = HTML source
# Its spoken name.
editor-source = Toggle HTML source
# Label of the question field.
editor-front = Front
# Label of the answer field.
editor-back = Back
# Grey text in the empty question field.
editor-front-placeholder = Question
# Grey text in the empty answer field.
editor-back-placeholder = Answer
# Spoken name of the question field's HTML box.
editor-front-source = Front HTML source
# Spoken name of the answer field's HTML box.
editor-back-source = Back HTML source
# Hint under the answer on a typed-answer note.
editor-typed-hint = During study you'll type this answer and see a diff.
# Hint under the fields.
editor-drop-hint = Drop or paste images and audio straight into a field.
# Hint on a cloze note. The keys are shown as key caps.
editor-cloze-hint-html = Select an answer and press <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> to blank it.
# Hint on a reversed note.
editor-reversed-hint = Two cards: front to back, and back to front.
# Hint: the shortcut that saves. The keys are shown as key caps.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Enter</kbd> saves.

## Colours, for the swatches' names (lowercase, as inside a sentence)

# The colour red.
hue-red = red
# The colour amber.
hue-amber = amber
# The colour green.
hue-green = green
# The colour teal.
hue-teal = teal
# The colour blue.
hue-blue = blue
# The colour purple.
hue-purple = purple
# The colour pink.
hue-pink = pink

## Why a card or a note was refused (the editor, the app, an assistant)

# A card's question side is empty.
card-error-front-empty = card front is empty
# A card's answer side is empty.
card-error-back-empty = card back is empty
# A side is too long. $max is the limit in bytes.
card-error-side-long = card side exceeds { $max } bytes
# A field of the editor is too long. $max is the limit in bytes of HTML.
card-error-field-long = a field exceeds { $max } bytes of HTML
# A cloze note has no blank. "{{c1::…}}" is the cloze syntax, kept as it
# is.
card-error-no-cloze = add at least one {"{{"}c1::…{"}}"} cloze deletion
# A cloze blank has no answer. $n is the blank's number.
card-error-cloze-empty = cloze c{ $n } has an empty answer
# A cloze blank's card was refused. $n is the blank's number, $error why
# (one of the card-error-* above).
card-error-cloze-card = cloze c{ $n }: { $error }
# The card shows an image or a sound that is not the person's. $id is
# its number.
card-error-media = media { $id } is not one of your uploads

## An image or a sound refused on upload

# The file is of a kind cards cannot show. $filename is its name.
media-error-format = { $filename }: format not supported
# The file is empty. $filename is its name.
media-error-empty = { $filename }: empty file
# The file is over the size limit. $filename is its name, $mb the limit
# in megabytes.
media-error-too-large = { $filename }: over the { $mb } MB per-file limit
# The file's content is not what its name says it is. $filename is its
# name.
media-error-mismatch = { $filename }: content does not match its extension
# The account holds as many media files as it may.
media-error-count = media file limit reached. Remove some files first
# The account's media take all the room it has.
media-error-storage = media storage limit reached. Contact support
