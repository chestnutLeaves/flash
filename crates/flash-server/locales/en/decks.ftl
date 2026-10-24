### The open core's words in English: the home page (Today), the list of
### decks, a deck's page and its settings. A deck is a named set of
### flashcards; a subdeck is a deck inside another. Every message has a
### note above it: where it shows, who reads it, and what each variable
### holds.

## Deck tables (Today, the decks page, a deck's subdecks)

# Column heading: the deck's name.
decks-col-deck = Deck
# Column heading: how many of its cards are due for review now.
decks-col-due = Due
# Column heading: how many new cards it offers today.
decks-col-new = New
# Spoken name of the arrow that folds a deck's subdecks. $name is the
# deck's name.
decks-fold-label = Subdecks of { $name }
# Tooltip of that arrow while the subdecks are hidden.
decks-fold-show = Show subdecks
# Tooltip of that arrow while the subdecks show.
decks-fold-hide = Hide subdecks
# Note after an archived deck's name.
decks-archived-note = (archived)
# Small link on an archived deck's row that brings it back (lowercase,
# quiet).
decks-unarchive = unarchive
# Small link on a deck's row that starts studying it (lowercase, quiet).
decks-study = study
# Column heading, shown when a deck has a goal date: a ring that fills as
# the deck is learned, and the time left (goal-left-*).
decks-col-goal = Goal
# Spoken name of the round "…" button at the end of a deck's row, which
# opens that deck's menu. $name is the deck's name.
decks-more-label = More for { $name }
# Tooltip of that button.
decks-more-title = More
# Menu item on a deck's row: opens the page of its settings.
decks-more-settings = Deck settings
# Menu item on a deck's row: adds new cards to today's limit for it.
decks-more-new = More new cards today

## Today (the home page)

# Heading of the home page: what is due today.
today-heading = Today
# Under a big number: cards due for review now.
today-due = due now
# Under a big number: new cards available today.
today-new = new today
# Under a big number: cards reviewed today.
today-reviewed = reviewed today
# Button that starts studying.
today-start = Start studying
# Shown instead of the button when nothing is due.
today-caught-up = All caught up. Nothing due right now.
# Before the buttons that add 5, 10 or 20 new cards to today's limit.
today-more-new = More new cards today:
# The same when some were already added today. $count is how many.
today-more-new-boosted = More new cards today (+{ $count } already):
# Heading over the deck table.
today-decks = Decks
# Shown when there are no decks, with links to make one or import one.
today-no-decks-html = No decks yet. <a href="/decks">Create one</a> or <a href="/import">import</a>.
# Title of the box inviting the person to connect an AI assistant.
today-connect = Connect your AI
# Text of that box.
today-connect-sub = Make cards and study with Claude, ChatGPT, Grok, Mistral, or Gemini. One-minute setup.
# Spoken name and tooltip of the button that hides that box.
today-connect-dismiss = Dismiss

## The decks page

# Browser tab title of the decks page.
decks-title = Decks - Flash
# Heading of the decks page.
decks-heading = Decks
# Link that shows archived decks in the list.
decks-show-archived = Show archived
# Link that hides them again.
decks-hide-archived = Hide archived
# Link to the import page.
decks-import = Import
# Placeholder of the field that names a new deck.
decks-new-name = New deck name
# Button that makes the deck.
decks-create = Create
# Shown when there are no decks.
decks-none = No decks yet.
# Shown when every deck is archived and archived decks are hidden.
decks-all-archived = Every deck is archived.

## A deck's page

# Browser tab title of a deck's page. $name is the deck's full name.
deck-title = { $name } - Flash
# Spoken name of the trail of decks this one sits in.
deck-crumbs = Parent decks
# Button that brings an archived deck back.
deck-unarchive = Unarchive
# Note on an archived deck's page.
deck-archived-hint = This deck is archived. Its cards stay, but they are not in the study queue.
# Label of the field for the card's question side.
deck-front = Front
# Label of the field for the card's answer side.
deck-back = Back
# Label of the tags field.
deck-tags = Tags
# Example tags in the empty tags field; keep them short study tags.
deck-tags-placeholder = exam-2, cardiac
# Hint under the tags field.
deck-tags-hint = Comma-separated, optional.
# Button that adds the card.
deck-add = Add card
# Button that opens the full editor (card types, languages, images).
deck-advanced = Advanced
# Placeholder of the field that searches the deck's cards.
deck-search = Search this deck…
# Heading of the panel listing the decks inside this one.
deck-subdecks-heading = Subdecks
# Placeholder and spoken name of the field that names a new subdeck.
deck-new-subdeck = New subdeck name
# Button that makes the subdeck.
deck-add-subdeck = Add subdeck
# Row under a deck's subdecks (and a menu item) that opens the field for
# a new subdeck.
deck-subnew-open = New subdeck
# Button that closes the new-subdeck field without making one, and the
# selection bar's button that steps back from deleting.
deck-subnew-cancel = Cancel

## A deck's page: its head, its subdecks, the search and filter, Add, and
## the bar of actions on the selected cards

# The word after the number of cards due for review, in the quiet line
# under the deck's name ("8 due"). $count is that number, shown before
# it in bold.
deck-meta-due = { $count ->
        [one] due
       *[other] due
    }
# The word after the number of new cards today ("12 new"). $count is
# that number, shown before it.
deck-meta-new = { $count ->
        [one] new
       *[other] new
    }
# The word after the deck's number of cards, its subdecks' included ("62
# cards"). $count is that number, shown before it.
deck-meta-cards = { $count ->
        [one] card
       *[other] cards
    }
# Button that studies this deck; a number of due cards may follow it.
deck-study-short = Study
# Spoken name of the button that opens the deck's menu (goal, export,
# settings, archive).
deck-more-label = More for this deck
# Tooltip of that button.
deck-more-title = More
# Menu item that downloads the deck and its subdecks as an Anki package.
deck-export-apkg = Export as Anki deck (.apkg)
# Menu item that downloads the deck's cards as a spreadsheet file.
deck-export-csv = Export as spreadsheet (.csv)
# Menu item that archives the deck.
deck-archive-deck = Archive deck
# A subdeck row: how many cards it holds, its own subdecks' included.
# $count is that number.
deck-sub-cards = { $count ->
        [one] { $count } card
       *[other] { $count } cards
    }
# Spoken name of a subdeck row's Study link. $name is the subdeck's name.
deck-sub-study-label = Study { $name }
# Row that shows the rest of the subdecks when there are many. $count is
# how many subdecks there are in all.
deck-subs-all = { $count ->
        [one] Show all { $count }
       *[other] Show all { $count }
    }
# Row that opens the list of archived subdecks. $count is how many.
deck-subs-archived = { $count ->
        [one] { $count } archived subdeck
       *[other] { $count } archived subdecks
    }
# Placeholder of the field that searches the deck's cards. $count is how
# many cards the deck has, its subdecks' included.
deck-search-n = { $count ->
        [one] Search { $count } card…
       *[other] Search { $count } cards…
    }
# Button that opens the filter: archived or not, which decks, which tags.
deck-filter = Filter
# Spoken name of the filter's choice between active and archived cards.
deck-filter-show = Show
# That choice: the cards in study.
deck-filter-active = Active
# That choice: the archived cards, out of study.
deck-filter-archived = Archived
# Heading in the filter over the deck and its subdecks.
deck-filter-decks = Decks
# Heading in the filter over the deck's tags.
deck-filter-tags = Tags
# Placeholder of the field that narrows the filter's list of tags.
deck-filter-find-tag = Find a tag…
# Button that opens the form that adds a card.
deck-add-open = Add
# Spoken name of the arrow beside Add that opens the other ways to add
# cards.
deck-add-more = More ways to add cards
# Menu item: write one card by hand.
deck-add-write = Write a card
# Menu item: bring in a deck made in Anki.
deck-add-import = Import an Anki deck
# Button that closes the form that adds a card.
deck-add-close = Close
# Spoken name of the bar of actions on the selected cards.
deck-bulk-label = Selected cards
# The bar's button that adds or removes tags on the selected cards.
deck-bulk-tags = Tags
# Label of the field in that menu.
deck-bulk-tags-label = Add or remove tags
# Button that adds the tags written in the field.
deck-bulk-tag-add = Add
# Button that removes the tags written in the field.
deck-bulk-tag-remove = Remove
# The bar's button that moves the selected cards to another deck, and
# the button in its menu that does it.
deck-bulk-move = Move
# Label of the list of decks to move them to.
deck-bulk-move-label = Move to deck
# The bar's button that archives the selected cards.
deck-bulk-archive = Archive
# The bar's button that brings archived cards back into study.
deck-bulk-unarchive = Unarchive
# The bar's button that deletes the selected cards (it asks first).
deck-bulk-delete = Delete
# Spoken name and tooltip of the bar's button that clears the selection.
deck-bulk-clear = Clear selection (Esc)
# Shown in the bar before deleting the selected cards.
deck-bulk-delete-warn = Delete them for good? This can't be undone.

## A deck's settings

# Browser tab title. $name is the deck's full name.
deck-settings-title = { $name } settings - Flash
# Line under the deck's name, naming the page.
deck-settings-label = Deck settings
# Link back to the deck's page.
deck-settings-back = Back to deck
# Heading of the card that renames the deck.
deck-name-heading = Name
# Spoken name of the deck name field.
deck-name-label = Deck name
# Button that renames the deck.
deck-rename = Rename
# Brief note after a change was saved.
deck-saved = Saved
# Heading of the card that moves the deck inside another.
deck-parent-heading = Parent deck
# Explains the parent deck. $count is how many subdecks this deck has
# (they move with it).
deck-parent-hint =
    { $count ->
        [0] The deck this one sits in. Studying a deck includes its subdecks.
        [one] The deck this one sits in. Studying a deck includes its subdecks, and this deck's subdeck moves with it.
       *[other] The deck this one sits in. Studying a deck includes its subdecks, and this deck's { $count } subdecks move with it.
    }
# Spoken name of the parent deck picker.
deck-parent-label = Parent deck
# The picker's choice for no parent at all.
deck-top-level = (top level)
# Button that moves the deck.
deck-move = Move
# Brief note after the deck moved.
deck-moved = Moved
# Heading of the card with the daily limits.
deck-limits-heading = Daily limits
# Label: the most new cards to show per day.
deck-limit-new = New cards / day
# Label: the most reviews per day.
deck-limit-reviews = Max reviews / day
# Placeholder of a limit field left to the account's setting. $count is
# that setting.
deck-limit-default = account default ({ $count })
# Button that saves the limits.
deck-save-limits = Save limits
# Badge: new cards added to today's limit. $count is how many.
deck-boosted = +{ $count } today
# Heading of the card for an archived deck.
deck-archived-heading = Archived deck
# Explains archiving, on an archived deck. $count is how many subdecks
# come back with it.
deck-archived-explain =
    { $count ->
        [0] This deck is out of the study queue and the due counts. Unarchive it and its cards come back with the review history they had.
        [one] This deck is out of the study queue and the due counts. Unarchive it and its cards come back with the review history they had, and so do its { $count } subdeck.
       *[other] This deck is out of the study queue and the due counts. Unarchive it and its cards come back with the review history they had, and so do its { $count } subdecks.
    }
# Added after that when the deck sits inside others.
deck-archived-parents = The decks it sits in are unarchived with it.
# Brief note after the deck was archived.
deck-archived-toast = Archived
# Heading of the card that archives the deck.
deck-archive-heading = Archive deck
# Explains archiving. $count is how many subdecks go with it.
deck-archive-explain =
    { $count ->
        [0] Keeps every card, its media and review history, and takes the deck out of study and out of due counts. You can unarchive it any time.
        [one] Keeps every card, its media and review history, and takes the deck out of study and out of due counts, along with its { $count } subdeck. You can unarchive it any time.
       *[other] Keeps every card, its media and review history, and takes the deck out of study and out of due counts, along with its { $count } subdecks. You can unarchive it any time.
    }
# Button that archives the deck.
deck-archive = Archive
# Brief note after the deck was brought back.
deck-restored = Restored
# Heading of the card that deletes the deck.
deck-delete-heading = Delete deck
# Explains deleting. $count is how many subdecks go with it.
deck-delete-explain =
    { $count ->
        [0] Removes this deck and every card in it. Review history is kept. This can't be undone.
        [one] Removes this deck, its { $count } subdeck and every card in them. Review history is kept. This can't be undone.
       *[other] Removes this deck, its { $count } subdecks and every card in them. Review history is kept. This can't be undone.
    }
# The browser's confirm question before deleting. $subdecks is how many
# subdecks go with it, $cards how many cards.
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
            [one] Delete this deck and all { $cards } card? This can't be undone.
           *[other] Delete this deck and all { $cards } cards? This can't be undone.
        }
        [one] { $cards ->
            [one] Delete this deck, its { $subdecks } subdeck and all { $cards } card? This can't be undone.
           *[other] Delete this deck, its { $subdecks } subdeck and all { $cards } cards? This can't be undone.
        }
       *[other] { $cards ->
            [one] Delete this deck, its { $subdecks } subdecks and all { $cards } card? This can't be undone.
           *[other] Delete this deck, its { $subdecks } subdecks and all { $cards } cards? This can't be undone.
        }
    }
# Button that deletes the deck.
deck-delete = Delete deck

## Why a card was not added (a banner over the add form)

# The cloze note had no deletion. "{{c1::…}}" is the cloze syntax, shown
# as is; Ctrl+Shift+C is the keyboard shortcut that makes one.
card-not-added-cloze = That card wasn't added: a cloze note needs at least one {"{{"}c1::…{"}}"} deletion. Select the answer and press Ctrl+Shift+C.
# A side was left empty.
card-not-added-empty = That card wasn't added: both sides need some content.
# A side was over the length limit.
card-not-added-long = That card wasn't added: a side is too long.
# The card pointed at an image or sound of another account.
card-not-added-media = That card wasn't added: it references media that isn't yours.
# Any other refusal.
card-not-added-invalid = That card wasn't added: the content couldn't be saved.
# The account is at its card limit.
card-not-added-limit = That card wasn't added: this account has reached its card limit.

## Why a deck could not be renamed or moved (a badge beside the heading)

# The name held "::", which writes a deck path; the Parent deck setting
# nests instead (deck-parent-heading).
deck-error-nest = a name can't contain "::"; use Parent deck to nest this deck
# The name was empty.
deck-error-empty = the name can't be blank
# Another deck has the name.
deck-error-taken = you already have a deck with that name
# The deck was moved into itself or one of its subdecks.
deck-error-self = a deck can't move into itself
# The full path ("Parent::Child") would be over the length limit.
deck-error-long = that would make a deck's full name too long
# The chosen parent deck is archived.
deck-error-archived = that deck is archived; unarchive it first
# The account is at its deck limit.
deck-error-cap = this account has as many decks as it can hold
# Any other failure.
deck-error-invalid = that didn't work

## Refusals naming or making a deck (a sentence, shown by the app and
## assistants too)

# A name held "::", which writes a deck path; "Parent deck" is the
# setting that nests a deck (deck-parent-heading).
deck-error-nest-sentence = A name cannot contain "::". Use Parent deck to nest this deck.
# A new deck was asked for with no name.
deck-error-name-new = Name the new deck.
# The deck named is archived. $name is its name.
deck-error-archived-sentence = Your deck "{ $name }" is archived. Unarchive it, or pick another name.

## A deck's goal date: the day the learner wants to know the deck by (an
## exam, a trip, a deadline). Flash paces the deck's new cards to finish
## before it and brings back, in its last days, the cards that need it.

# Refused goal date: today or a day already past.
goal-error-past = Pick a date after today.
# Refused goal date: more than five years ahead.
goal-error-far = Pick a date within the next five years.

# Menu item on a deck's row without a goal date: opens the dialog that
# sets one.
goal-set = Set a goal date…
# Menu item on a deck's row with a goal date: opens the dialog to change
# it.
goal-change = Change goal date…
# Menu item on a deck's row with a goal date: removes it.
goal-remove = Remove goal date

# The time left in the Goal column, up to two months away: days, very
# short ("12d"). $count is the number of days.
goal-left-days = { $count }d
# The time left on the goal date itself.
goal-left-today = Today
# In the Goal column once the goal date has passed.
goal-left-done = Done
# The time left, months away in another year: the month and the year
# ("Oct 2027"). $month is the month, short; $year the year in figures.
goal-left-month-year = { $month } { $year }

# Tooltip of the goal marker, first line, when the deck keeps to its
# plan. $date is the goal date ("Oct 28, 2026").
goal-tip-on-track = On track for { $date }
# The same when the deck has fallen behind its plan. $date is the goal
# date ("Oct 28, 2026").
goal-tip-behind = Behind for { $date }
# Tooltip line: how much of the deck is learned. $learned and $total are
# card counts.
goal-tip-learned =
    { $total ->
        [one] { $learned } of { $total } card learned.
       *[other] { $learned } of { $total } cards learned.
    }
# Tooltip line: the plan. $count is new cards a day; $date the last day
# new cards come in; after it, the last reviews before the goal.
goal-tip-pace =
    { $count ->
        [one] { $count } new a day until { $date }, then a last review.
       *[other] { $count } new a day until { $date }, then a last review.
    }
# Tooltip line in the last days before the goal, when no new cards come.
# $date is the goal date.
goal-tip-final = The last reviews before { $date }.
# Tooltip line when the deck is behind. $count is how many new cards
# behind it is.
goal-tip-catch-up =
    { $count ->
        [one] { $count } extra new card today puts you back on track.
       *[other] { $count } extra new cards today put you back on track.
    }
# Tooltip on the goal date itself.
goal-tip-today = Your goal is today. Good luck!
# Tooltip once the goal date has passed. $date is the date.
goal-tip-passed = Your goal date was { $date }.
# Under the name of a deck whose goal date has passed, before its two
# buttons (goal-archive, goal-keep).
goal-passed = Your goal date has passed.
# Button: archives the deck whose goal date passed.
goal-archive = Archive deck
# Button: removes the passed goal date and keeps studying the deck as
# any other.
goal-keep = Keep studying

# Heading of the dialog (and of the deck settings card) that sets a goal
# date, when the deck has none.
goal-heading-set = Set a goal date
# The same when the deck has one.
goal-heading-change = Change goal date
# Heading of the card on a deck's settings page.
goal-card-heading = Goal date
# Label of the date field in that dialog.
goal-date-label = When do you need to know it?
# Spoken name of the row of quick picks under the date field.
goal-picks = Quick picks
# Quick pick: a date some weeks from today. $count is the weeks.
goal-pick-weeks =
    { $count ->
        [one] { $count } week
       *[other] { $count } weeks
    }
# Quick pick: a date some months from today. $count is the months.
goal-pick-months =
    { $count ->
        [one] { $count } month
       *[other] { $count } months
    }
# Quick pick: a date a year from today.
goal-pick-year = 1 year
# The plan, first line: how many new cards a day and until when. $count
# is new cards a day; $date the last day they come in; $unseen how many
# of the deck's cards were never studied. The bold part is the pace.
goal-plan-pace-html =
    { $count ->
        [one] <b>{ $count } new card a day</b> until { $date } ({ $unseen } not seen yet)
       *[other] <b>{ $count } new cards a day</b> until { $date } ({ $unseen } not seen yet)
    }
# The plan, first line, when every card was already studied once.
goal-plan-seen-html = <b>Every card has been seen.</b> Reviews keep them fresh until then.
# The plan, second line: the last days before the goal, kept for review.
# $from is the first of those days, $to the goal date.
goal-plan-final-html = <b>{ $from } to { $to }:</b> a last review of the cards that need it
# The same when the goal is too close for days of their own. $to is the
# goal date.
goal-plan-final-close-html = <b>Until { $to }:</b> a last review of every card that needs it
# The plan, third line: after the goal. $date is the goal date.
goal-plan-after-html = <b>After { $date },</b> reviews go back to normal. You can archive the deck then.
# Shown where the plan goes before a date is picked.
goal-plan-pick = Pick a date to see the plan.
# Warning under the plan when the date asks for many new cards a day.
# $count is how many.
goal-tight = That's about { $count } new cards a day. A later date makes the days lighter.
# Button that saves the goal date.
goal-save = Save goal
# Button that closes the dialog without saving.
goal-cancel = Cancel
