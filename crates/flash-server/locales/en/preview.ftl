### The open core's words in English: a deck's preview, where a person
### goes through every card of a deck once and marks each one missed or
### got, without the marks reaching spaced repetition: nothing is
### scheduled and nothing is recorded. Every message has a note above it:
### where it shows, who reads it, and what each variable holds.

## The page

# Browser tab title of the preview page.
preview-title = Preview - Flash
# Heading of the preview page (the activity, not an order).
preview-heading = Preview
# A tag beside the heading: going through the cards here leaves their
# spaced-repetition schedule exactly as it was.
preview-unaffected = Spaced repetition unaffected
# Link beside the heading that stops the preview early and shows the
# score so far.
preview-end = End
# Shown instead of cards when the deck has none to go through.
preview-empty = This deck has no cards to preview.
# Button under that, back to the deck's page. $name is the deck's name.
preview-go-deck = Back to { $name }
# Button after the answer shows: the person did not remember the card.
preview-missed = Missed
# Button after the answer shows: the person remembered the card.
preview-got = Got it

## The end of a preview

# First line under the score.
preview-unaffected-line = Preview doesn't affect spaced repetition.
# Added after that line when the deck has cards to study now. $count is
# how many.
preview-ready =
    { $count ->
        [one] { $count } card is ready to study now.
       *[other] { $count } cards are ready to study now.
    }
# Added after that line otherwise, when the next cards come later today
# (an account that keeps exact times). $count is how many; 0 when the
# count is not known.
preview-next-later-today =
    { $count ->
        [0] Your next reviews are later today.
        [one] Your next review is later today: { $count } card.
       *[other] Your next reviews are later today: { $count } cards.
    }
# The same, when they come tomorrow. $count is how many; 0 when the
# count is not known.
preview-next-tomorrow =
    { $count ->
        [0] Your next reviews are tomorrow.
        [one] Your next review is tomorrow: { $count } card.
       *[other] Your next reviews are tomorrow: { $count } cards.
    }
# The same, within the week. $count is how many (0 when not known); $day
# is the weekday's whole name ("Thursday").
preview-next-weekday =
    { $count ->
        [0] Your next reviews are on { $day }.
        [one] Your next review is on { $day }: { $count } card.
       *[other] Your next reviews are on { $day }: { $count } cards.
    }
# The same, further off. $count is how many (0 when not known); $days is
# how many days away (7 or more).
preview-next-in-days =
    { $count ->
        [0] Your next reviews are in { $days } days.
        [one] Your next review is in { $days } days: { $count } card.
       *[other] Your next reviews are in { $days } days: { $count } cards.
    }
# Button that goes through the whole deck again.
preview-start-over = Start over

## The deck's menu (the ⋯ beside a deck on Today and Decks)

# Menu item that opens the deck's preview. $count is how many cards it
# goes through (at least 1).
preview-menu =
    { $count ->
        [one] Preview the card
       *[other] Preview all { $count }
    }
# Second line of that menu item, smaller.
preview-menu-sub = Spaced repetition unaffected

## Words the page's script says

# Heading of the end. $got is how many the person marked "Got it", $total
# how many they marked at all.
js-preview-score = You got { $got } of { $total }
# The share of "Got it", beside the score's bar. $percent is a whole
# number from 0 to 100.
js-preview-percent = { $percent }%
# Button that goes through only the cards marked "Missed" again. $count is
# how many.
js-preview-review-missed =
    { $count ->
        [one] Review the one you missed
       *[other] Review the { $count } missed
    }
