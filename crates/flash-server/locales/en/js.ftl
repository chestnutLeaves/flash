### The open core's words in English that its pages' scripts say
### (static/*.js, through static/i18n.js). Every message has a note above
### it: where it shows, who reads it, and what each variable holds. A
### script's message keeps to text, variables and at most one choice.

## Shared behaviours (ui.js)

# Spoken name of the play button of a sound on a card.
js-ui-play-audio = Play audio
# Spoken name of that button while the sound plays.
js-ui-pause-audio = Pause audio
# A copy button's words just after it copied.
js-ui-copied = Copied
# A copy button's words when copying failed.
js-ui-copy-failed = Couldn't copy
# Tooltip of the arrow that folds a deck's subdecks, while they are
# hidden (decks-fold-show).
js-ui-show-subdecks = Show subdecks
# Tooltip of that arrow while they show (decks-fold-hide).
js-ui-hide-subdecks = Hide subdecks
# The decks page's link that shows archived decks (decks-show-archived).
js-ui-show-archived = Show archived
# That link while they show (decks-hide-archived).
js-ui-hide-archived = Hide archived

## Signing in with a passkey (webauthn.js)

# The passkey prompt was dismissed or timed out.
js-passkey-cancelled = Cancelled or timed out.
# Something went wrong with no more to say.
js-passkey-failed = That did not work. Try again.
# The server's answer was not what the page expected.
js-passkey-bad-list = The server's credential list is not a list.
# The same, for an entry of it.
js-passkey-bad-entry = The server's credential list holds something else.
# The same, for an entry that is not a passkey.
js-passkey-not-passkey = The server named a credential that is not a passkey.
# The browser made no passkey.
js-passkey-none-made = The browser did not make a passkey.
# The browser used no passkey.
js-passkey-none-used = The browser did not use a passkey.
# Signing in: the passkey prompt ended with none (cancelled, or none on
# this device for this site). "Another way" is the page's other ways in.
js-passkey-not-used = No passkey was used. Sign in another way, then add your passkey again in Settings.
# The link to make an account is cut short.
js-passkey-link-incomplete = This enrollment link is incomplete.

## A deck's page (deck.js)

# The selection bar: how many cards are selected. $count is how many.
js-deck-selected = { $count ->
        [one] { $count } selected
       *[other] { $count } selected
    }
# The selection bar's link that selects every card the list holds, past
# this page. $count is how many that is.
js-deck-select-all = { $count ->
        [one] Select all { $count }
       *[other] Select all { $count }
    }
# The button that deletes the selected cards for good. $count is how
# many.
js-deck-delete-n = { $count ->
        [one] Delete { $count } card
       *[other] Delete { $count } cards
    }
# Spoken name and tooltip of a card's delete button after one click:
# a second click deletes it.
js-deck-row-confirm = Click again to delete
# The button that opens the form that adds a card.
js-deck-add = Add
# The same button while the form is open: it closes it.
js-deck-close = Close
# Shown when an action on the cards could not reach the server or was
# refused without words of its own.
js-deck-failed = That didn't go through. Try again.

## The card editor (editor.js)

# Label of the first field of a plain note (editor-front).
js-editor-front = Front
# Label of the second field of a plain note (editor-back).
js-editor-back = Back
# Grey text in the empty first field (editor-front-placeholder).
js-editor-question = Question
# Grey text in the empty second field (editor-back-placeholder).
js-editor-answer = Answer
# Grey text in the second field of a typed-answer note.
js-editor-answer-to-type = Answer to type
# Label of a cloze note's first field: the text with blanks.
js-editor-text = Text
# Label of a cloze note's second field: extra shown with the answer.
js-editor-extra = Extra
# Grey text in a cloze note's first field; "{{c1::blanks}}" is the cloze
# syntax, kept as it is.
js-editor-cloze-hint = Sentence with {"{{"}c1::blanks{"}}"}
# Grey text in a cloze note's second field.
js-editor-extra-hint = Shown with the answer (optional)
# Shown in the field while a file uploads. $name is the file's name.
js-editor-uploading = Uploading { $name }…
# An upload failed.
js-editor-upload-failed = Upload failed.
# A formatting button pressed with nothing selected.
js-editor-select-first = Select some text first.
# Saving while a file still uploads.
js-editor-wait-upload = Wait for the upload to finish.
# The button that closes the full editor, back to the two fields.
js-editor-simple = Simple
# The button that opens the full editor (deck-advanced).
js-editor-advanced = Advanced
