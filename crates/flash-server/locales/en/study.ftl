### The open core's words in English: the study page, where a person
### sees a card, shows its answer and grades how well they remembered it.
### Every message has a note above it: where it shows, who reads it, and
### what each variable holds.

## The page

# Browser tab title of the study page.
study-title = Study - Flash
# Heading of the study page (the activity, not an order).
study-heading = Study

## Studying by tags (a dialog that narrows the session to some tags)

# Link beside the heading that opens the tag picker, with no tag picked.
study-tags = Tags
# The same link with one or two tags picked. $tags is the picked tags,
# comma-separated.
study-tags-some = Tags: { $tags }
# The same link with more than two picked: the first two, then how many
# more. $first and $second are the first two tags' names, $more how
# many other tags are picked.
study-tags-more = Tags: { $first }, { $second } +{ $more }
# Heading of the tag picker.
study-tags-heading = Study by tags
# Text in the picker when the deck being studied has no tags.
study-tags-none-deck = No tags in this deck yet. Tags are set when a card is added or edited.
# Text in the picker when no card has tags.
study-tags-none = No tags yet. Tags are set when a card is added or edited.
# Placeholder and spoken name of the field that filters the tag list.
study-tags-filter = Filter tags
# Shown when the filter matches no tag.
study-tags-no-match = No tag matches.
# How picking several tags works.
study-tags-every = A card is in the session only if it carries every picked tag.
# Button that unticks every tag.
study-tags-clear = Clear
# Button that starts a session with the picked tags.
study-tags-apply = Apply

## The card

# Spoken name of the "…" button on the card's corner.
study-more-label = More for this card
# Tooltip of that button.
study-more-title = More
# Menu item: edit the card on screen.
study-edit-card = Edit card
# Menu item: take the card out of study (it stays in its deck).
study-archive-card = Archive card
# Placeholder of the field where the answer is typed, on cards that ask
# for it.
study-type-answer = Type the answer…
# Button that shows the card's answer.
study-show-answer = Show answer
# The key that shows the answer on cards with a typed answer, as its key
# cap reads (lowercase).
study-key-enter = enter
# The key that shows the answer on other cards, as its key cap reads
# (lowercase).
study-key-space = space
# How many cards are left in the session. $count is the number.
study-left = { $count } left

## The four grades, from forgetting to knowing it cold. Anki's own words
## for them; keep them as the person's flashcard app says them.

# Grade: forgot it; the card comes back soon.
study-grade-again = Again
# Grade: remembered it, with effort.
study-grade-hard = Hard
# Grade: remembered it.
study-grade-good = Good
# Grade: knew it at once.
study-grade-easy = Easy

## When a card comes back after each grade, under the grade buttons:
## very short, as a button has room for.

# $count minutes.
interval-minutes = { $count }m
# $count hours.
interval-hours = { $count }h
# $count days.
interval-days = { $count }d
# $count months.
interval-months = { $count }mo
# $count years; may have one decimal (1.5).
interval-years = { $count }y

## A finished session

# Heading when the day's limit of new cards was reached.
study-done-limit-heading = That's today's new cards
# Heading when the session ended with cards reviewed.
study-done-heading = Session done
# Heading when nothing was due.
study-done-caught-up = All caught up
# Line under the limit heading. $count is how many new cards wait for
# tomorrow; the number is shown bold.
study-done-held-html =
    { $count ->
        [one] <strong>{ $count }</strong> more card waits behind today's limit.
       *[other] <strong>{ $count }</strong> more cards wait behind today's limit.
    }
# Added after the line above. $count is how many cards were reviewed.
study-done-held-reviewed = { $count } reviewed.
# The same, with $took how long it took (study-took-*). $count is how
# many cards were reviewed.
study-done-held-reviewed-took = { $count } reviewed { $took }.
# Line under the "Session done" heading. $count is how many cards were
# reviewed.
study-done-reviewed =
    { $count ->
        [one] { $count } card reviewed
       *[other] { $count } cards reviewed
    }
# The same, with $took how long it took (study-took-*). $count is how
# many cards were reviewed.
study-done-reviewed-took =
    { $count ->
        [one] { $count } card reviewed { $took }
       *[other] { $count } cards reviewed { $took }
    }
# How long the session took, when under a minute.
study-took-under-minute = in under a minute
# How long the session took. $count is the minutes.
study-took-minutes =
    { $count ->
        [one] in { $count } minute
       *[other] in { $count } minutes
    }
# Line under the "All caught up" heading.
study-done-nothing-due = Nothing is due right now.
# A tinted count of the session's grades: $count cards graded Good
# (study-grade-good, in lowercase).
study-pill-good = { $count } good
# The same for Easy. $count is how many cards were graded Easy.
study-pill-easy = { $count } easy
# The same for Hard. $count is how many cards were graded Hard.
study-pill-hard = { $count } hard
# The same for Again. $count is how many cards were graded Again.
study-pill-again = { $count } again
# Before the buttons that add 5, 10 or 20 new cards to today's limit.
study-more-new = More new cards today:
# Spoken name of the small bar chart of the week ahead.
study-week = Cards due in the week ahead
# When the next cards come: none are scheduled.
study-nothing-scheduled = Nothing scheduled yet
# Label and spoken name of the switch that turns on reminder emails.
study-remind = Email me when cards are due
# Beside that switch on a server that sends no reminder emails: where
# they would be turned on.
study-remind-elsewhere = You can turn on reminders in Settings.
# Button back to the home page.
study-back-today = Back to today
# The goal date a finished session counts toward, when the deck keeps
# to its plan. $date is the goal date ("Oct 28").
study-goal-on-track = On track for { $date }
# The same when the deck has fallen behind its plan. $count is how many
# new cards behind; $date the goal date.
study-goal-behind =
    { $count ->
        [one] { $count } card behind for { $date }
       *[other] { $count } cards behind for { $date }
    }
# Under that line: how much of the deck is learned. $learned and $total
# are card counts; $deck is the deck's name.
study-goal-learned =
    { $total ->
        [one] { $learned } of { $total } card learned in { $deck }.
       *[other] { $learned } of { $total } cards learned in { $deck }.
    }
# After study-goal-learned: the days until the goal. $count is the days.
study-goal-days-left =
    { $count ->
        [one] { $count } day left.
       *[other] { $count } days left.
    }
# After study-goal-learned, when behind: how to catch up. $count is how
# many new cards.
study-goal-catch-up =
    { $count ->
        [one] { $count } more today puts you back on track.
       *[other] { $count } more today puts you back on track.
    }
# Button that adds that many new cards to today's limit and goes on
# studying. $count is how many.
study-goal-study-more = Study { $count } more
# How much of the deck is learned, beside the ring. $count is the share,
# out of 100.
study-goal-percent = { $count }%
# Shown when the place being studied has no cards at all.
study-empty = No cards here yet.
# Button to the list of decks.
study-go-decks = Go to your decks
# Shown after a card was archived from the study page.
study-archived = Card archived. It stays in its deck, out of study.
# Button that puts the archived card back.
study-undo = Undo

## When the next cards come, after a session ran out. $count is how many
## cards; 0 means an unknown number ("Cards").

# Later the same day. $count is how many cards (0 when unknown), $time
# the time of day.
# spare: $hour, the hour $time shows (3 in "3:40 PM"), for a language whose words agree with it.
study-next-later-today =
    { $count ->
        [0] More cards later today, around { $time }
        [one] { $count } more card later today, around { $time }
       *[other] { $count } more cards later today, around { $time }
    }
# Tomorrow. $count is how many cards (0 when unknown).
study-next-tomorrow =
    { $count ->
        [0] Cards due tomorrow
        [one] { $count } card due tomorrow
       *[other] { $count } cards due tomorrow
    }
# Within the week. $count is how many cards (0 when unknown), $day the
# weekday's name.
study-next-weekday =
    { $count ->
        [0] Cards due { $day }
        [one] { $count } card due { $day }
       *[other] { $count } cards due { $day }
    }
# Later. $count is how many cards (0 when unknown), $days the number of
# days, 7 or more.
study-next-in-days =
    { $count ->
        [0] Cards due in { $days } days
        [one] { $count } card due in { $days } days
       *[other] { $count } cards due in { $days } days
    }

## Refusals while studying

# A quick edit would delete the card on screen (a cloze blank removed).
# "Advanced edit" is the full editor.
study-edit-removes-card = This edit would remove the card you're studying. Keep its blank, or use Advanced edit.
