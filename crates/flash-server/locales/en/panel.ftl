### The open core's words in English for the study panel: the view an
### AI assistant shows inside the conversation, to study cards and to
### page through cards just made or edited
### (panel/panel.js, through static/i18n.js). Every message has a note
### above it: where it shows, and what each variable holds. A panel
### message keeps to text, variables and at most one choice. The grade
### buttons' times are the study page's (interval-*).

## The header

# Tooltip of the button that opens a card's menu.
panel-more = More
# Spoken name of that button.
panel-more-label = More for this card
# Menu item: edit the card on screen.
panel-edit-card = Edit card
# Menu item: archive the card on screen (it stays in its deck, never
# studied).
panel-archive-card = Archive card
# The button that shows a long card full screen.
panel-expand = Expand
# Its tooltip.
panel-expand-title = Show this card full screen
# Its spoken name.
panel-expand-label = Expand: show this card full screen
# Before the deck's name, when the session is limited to tags; a chip
# per tag follows.
panel-tags = Tags:

## Studying

# Shown while the panel waits for its first cards.
panel-loading = Loading your cards...
# The button under a card cut short that shows it whole.
panel-show-all = Show all
# The button that turns the card (study-show-answer).
panel-show-answer = Show answer
# Its tooltip: the key that does the same (study-key-space).
panel-key-space = Space
# The grade buttons (study-grade-*).
panel-again = Again
# The second grade button.
panel-hard = Hard
# The third grade button.
panel-good = Good
# The fourth grade button.
panel-easy = Easy
# A grade button's tooltip: the key that does the same. $key is 1 to 4.
panel-key = Key { $key }
# How many cards are left in the session, beside the progress bar.
# $count is that number.
panel-to-go = { $count } to go
# Where the count was, right after a card is archived.
panel-archived = Card archived
# The link beside it that brings the card back.
panel-undo = Undo

## Editing a card in the panel

# Label of the first field (editor-front).
panel-front = Front
# Label of the second field (editor-back).
panel-back = Back
# The link that opens the card in the site's full editor.
panel-advanced-edit = Advanced edit
# The button that saves the edit.
panel-save = Save
# The button that closes the editor without saving.
panel-cancel = Cancel
# A save failed, and the server gave no sentence of its own.
panel-save-failed = Couldn't save the card. Try again.

## A finished session

# Heading when the day's limit of new cards was reached.
panel-held-title = That's today's new cards
# Under it: how many new cards wait. $count is at least 1.
panel-held-text =
    { $count ->
        [one] { $count } more card waits behind today's limit.
       *[other] { $count } more cards wait behind today's limit.
    }
# The same, then what was done in this session: $held is
# panel-held-text, $summary is panel-reviewed-in or
# panel-reviewed-quick.
panel-held-text-summary = { $held } { $summary }.
# The button that lets more new cards through today. $count is how many
# (at most 10).
panel-study-more = Study { $count } more
# Heading when the deck or tags hold no cards at all.
panel-empty-title = No cards here yet
# Under it.
panel-empty-text = Ask for cards from your notes, a document or a topic, and study them here.
# The button under it, which asks the assistant for cards.
panel-make = Make cards
# Heading when the session ended after grading cards.
panel-done-title = Session done
# Heading when nothing was due from the start.
panel-caught-up = All caught up
# Under it.
panel-nothing-due = Nothing is due right now.
# How many cards were graded in the panel ($count, at least 1); the
# start of panel-reviewed-in and panel-reviewed-quick.
panel-cards-reviewed =
    { $count ->
        [one] { $count } card reviewed
       *[other] { $count } cards reviewed
    }
# How long the session took. $reviewed is panel-cards-reviewed, $count
# whole minutes, at least 1.
panel-reviewed-in =
    { $count ->
        [one] { $reviewed } in { $count } minute
       *[other] { $reviewed } in { $count } minutes
    }
# The same for a session shorter than a minute. $reviewed is
# panel-cards-reviewed.
panel-reviewed-quick = { $reviewed } in under a minute
# The count of each grade given, as small pills. $count is how many.
panel-score-again = { $count } again
# The same for Hard. $count is how many cards were graded Hard.
panel-score-hard = { $count } hard
# The same for Good. $count is how many cards were graded Good.
panel-score-good = { $count } good
# The same for Easy. $count is how many cards were graded Easy.
panel-score-easy = { $count } easy
# When the next cards come, later today (an account on exact times).
# $count is how many (0 when unknown), $time the hour ("3:40 PM").
# spare: $hour, the hour $time shows (3 in "3:40 PM"), for a language whose words agree with it.
panel-next-today =
    { $count ->
        [0] More cards later today, around { $time }
        [one] { $count } more card later today, around { $time }
       *[other] { $count } more cards later today, around { $time }
    }
# The same, tomorrow. $count is how many (0 when unknown).
panel-next-tomorrow =
    { $count ->
        [0] Cards due tomorrow
        [one] { $count } card due tomorrow
       *[other] { $count } cards due tomorrow
    }
# The same, later this week. $count is how many (0 when unknown), $day
# the weekday's name ("Thursday").
panel-next-weekday =
    { $count ->
        [0] Cards due { $day }
        [one] { $count } card due { $day }
       *[other] { $count } cards due { $day }
    }
# The same, a week or more away. $count is how many cards (0 when
# unknown), $days how many days.
panel-next-days =
    { $count ->
        [0] Cards due in { $days } days
        [one] { $count } card due in { $days } days
       *[other] { $count } cards due in { $days } days
    }
# When no card is scheduled.
panel-nothing-scheduled = Nothing scheduled yet
# The button that turns on reminder emails.
panel-remind = Remind me
# Its tooltip.
panel-remind-title = Email me when cards are due
# Shown once reminders are on.
panel-reminded = Reminder set
# Shown when the panel could not turn reminders on.
panel-remind-fallback = You can turn on reminders in Flash settings.
# The link to the site's record of every review.
panel-progress = See your progress
# What the Make cards button asks the assistant, as the person's own
# message in the chat. $deck is the deck's name.
panel-ask-deck = Help me make flashcards for { $deck }.
# The same for tags. $tags is the tags, separated by commas.
panel-ask-tags = Help me make flashcards tagged { $tags }.
# The same for an account with no cards.
panel-ask-first = Help me make my first flashcards.

## The cards just made or edited

# Beside the deck's name: how many cards were made. $count is that
# number.
panel-new-cards =
    { $count ->
        [one] { $count } new card
       *[other] { $count } new cards
    }
# The same for an edit: how many cards were changed. $count is that
# number.
panel-edited =
    { $count ->
        [one] { $count } edited
       *[other] { $count } edited
    }
# Above a side's old text in an edit.
panel-before = Before
# Spoken name of the arrow to the previous card.
panel-previous = Previous card
# Spoken name of the arrow to the next card.
panel-next = Next card
# Which card is shown. $at is its place, $count how many.
panel-position = { $at } of { $count }
# The button that starts studying the deck the cards went into.
panel-study-now = Study now
# The button that opens the deck on the site.
panel-see-all = See all

## Trouble

# The server did not answer.
panel-no-answer = Flash didn't answer.
# The server did not answer in time.
panel-no-answer-late = Flash didn't answer in time.
# The button that tries the last action again.
panel-try-again = Try again
# Shown when the chat app cannot show the panel.
panel-needs-host = This panel needs a host that shows MCP Apps.
