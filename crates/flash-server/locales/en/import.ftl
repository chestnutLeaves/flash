### The open core's words in English: importing cards from a CSV file or
### an Anki package (.apkg). Every message has a note above it: where it
### shows, who reads it, and what each variable holds. Messages named by
### the store's parsers (`ImportNote`) are here too.

## The import page

# Browser tab title of the import page.
import-title = Import - Flash
# Heading of the import page.
import-heading = Import
# What files the page takes. The code parts are a CSV's header row and
# Anki's file extension, shown as code; keep them as they are.
import-intro-html =
    CSV with header <code>front,back,tags,deck</code>, or an Anki <code>.apkg</code> export.
    Cards import as new, unless you bring your Anki study progress with them.
# Label over the file picker.
import-file = File
# Button that opens the file picker.
import-choose = Choose file
# Beside that button before a file is picked; a file may also be dropped
# on the form.
import-no-file-chosen = No file chosen, or drop one here
# Label over the field naming the deck the cards go into.
import-into = Into deck
# Example in that empty field: a deck name.
import-deck-placeholder = e.g. Pharmacology
# Shown on the button while the file uploads and is read.
import-reading = Uploading and reading the deck…
# Button that uploads the file for a preview.
import-preview-button = Preview
# Heading of the preview.
import-preview-heading = Preview
# How many cards will import, and from which file. $count is the cards,
# $name the file's name (shown as code), $size its size (import-size-*).
import-ready-html =
    { $count ->
        [one] <strong>{ $count } card</strong> ready to import from <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
       *[other] <strong>{ $count } cards</strong> ready to import from <span class="mono">{ $name }</span> <span class="muted">({ $size })</span>.
    }
# Badge: rows of the file that were skipped. $count is how many.
import-skipped =
    { $count ->
        [one] { $count } row skipped
       *[other] { $count } rows skipped
    }
# Label over the list of the decks the file holds.
import-decks-in-file = Decks in this file
# How many cards a deck in the file holds. $count is the number of
# cards.
import-deck-cards =
    { $count ->
        [one] { $count } card
       *[other] { $count } cards
    }
# After the list when it was cut short. $count is how many more decks.
import-more = +{ $count } more
# What happens to the file's decks. "Parent::Child" is how a deck path
# is written (shown as code; keep it).
import-keep-hint-html = Each deck is kept, and <code>Parent::Child</code> becomes a subdeck. A deck you already have by that name gets the cards.
# Label over the field naming a deck to hold all the file's decks.
import-put-under = Put under (optional)
# Placeholder of that field when it is empty: the decks stay at the top.
import-top-level = Top level
# Hint under that field.
import-put-under-hint = Name a deck to hold them all as subdecks, or leave it blank.
# Placeholder of the deck name field for a file that names no deck.
import-name-deck = Name this deck
# Disclosure with the import's extra choices.
import-advanced = Advanced
# Choice: keep the cards' colours. $count is how many cards have them.
import-keep-colors =
    { $count ->
        [one] Keep deck colors, matched to Flash's palette ({ $count } colored card).
       *[other] Keep deck colors, matched to Flash's palette ({ $count } colored cards).
    }
# Choice: drop the colours.
import-clean-look = Clean look, import without colors.
# Choice: keep the Anki review history. $count is how many reviews.
import-bring-progress =
    { $count ->
        [one]
            Bring my study progress ({ $count } review found):
            cards keep their schedule instead of starting as new, and every review
            lands in your stats, streaks included.
       *[other]
            Bring my study progress ({ $count } reviews found):
            cards keep their schedule instead of starting as new, and every review
            lands in your stats, streaks included.
    }
# Choice: use the package's Anki scheduling settings. $settings is a
# short list of them (import-settings-*).
import-adopt-settings = Adopt this deck's Anki FSRS settings ({ $settings }).
# What adopting them does.
import-adopt-hint = Replaces your current scheduling settings.
# Shown on the button while the cards and media import.
import-importing = Importing cards and media…
# Button that imports. $count is how many cards.
import-submit =
    { $count ->
        [one] Import { $count } card
       *[other] Import { $count } cards
    }
# Badge after an import.
import-imported = imported
# Line after an import. $count is how many cards were added.
import-added =
    { $count ->
        [one] { $count } card added.
       *[other] { $count } cards added.
    }

## A file's size: $n is the number, already written as the reader writes it

# Megabytes. $n is the size in megabytes.
import-size-mb = { $n } MB
# Kilobytes. $n is the size in kilobytes.
import-size-kb = { $n } KB
# Bytes. $n is the size in bytes.
import-size-b = { $n } B

## The package's Anki settings, listed in brief (comma-separated)

# The retention it aims for. $pct is a percentage.
import-settings-retention = { $pct }% retention
# Scheduling parameters tuned to the person. $count is how many values.
import-settings-params = tuned FSRS parameters ({ $count } values)
# New cards per day. $count is the number.
import-settings-new =
    { $count ->
        [one] { $count } new card/day
       *[other] { $count } new cards/day
    }

## What the preview notes about the file

# The file's decks go into one. $count is how many decks, $decks their
# names, comma-separated.
import-merge = The file holds { $count } decks ({ $decks }); all of their cards go into the deck named above.
# The package's media. $what lists them (import-media-*), $mb their size.
import-media = this deck references { $what } ({ $mb } MB)
# Images in the package. $count is how many.
import-media-images =
    { $count ->
        [one] { $count } image
       *[other] { $count } images
    }
# Sound clips in the package. $count is how many.
import-media-audio =
    { $count ->
        [one] { $count } audio clip
       *[other] { $count } audio clips
    }
# Videos in the package. $count is how many.
import-media-video =
    { $count ->
        [one] { $count } video
       *[other] { $count } videos
    }
# Files of a kind Flash does not take (SVG, say). $count is how many.
import-media-unsupported =
    { $count ->
        [one] { $count } unsupported file
       *[other] { $count } unsupported files
    }
# The review history was cut to the most recent ones. $max is how many.
import-history-capped = review history is capped at the most recent { $max } reviews; older ones stay in Anki
# The same in the phone app, which keeps fewer. $max is how many.
import-history-capped-phone = review history is capped at the most recent { $max } reviews on a phone; older ones stay in Anki
# A CSV row with text that is not UTF-8, read by the phone app. $row is
# its line, $field which column.
import-row-invalid-utf8 = row { $row }: invalid UTF-8 in field { $field }
# Notes left out. $count is how many.
import-notes-skipped =
    { $count ->
        [one] { $count } note skipped (empty or unsupported layout)
       *[other] { $count } notes skipped (empty or unsupported layout)
    }
# Cloze notes turned into cards. $notes notes became $cards cards (fewer
# than $notes when some blanks were empty).
import-cloze-expanded =
    { $notes ->
        [one] { $notes } cloze note became
       *[other] { $notes } cloze notes became
    } { $cards ->
        [one] { $cards } card
       *[other] { $cards } cards
    }
# Extra cards from the same notes (reversed cards and the like). $count
# is how many.
import-extra-layouts =
    { $count ->
        [one] { $count } additional card layout included (reversed etc.)
       *[other] { $count } additional card layouts included (reversed etc.)
    }
# Cards suspended in Anki stay out of study here too. $count is how
# many cards were suspended.
import-suspended =
    { $count ->
        [one] { $count } card arrives suspended, as in Anki
       *[other] { $count } cards arrive suspended, as in Anki
    }
# Extra cards that could not be read. $count is how many.
import-layouts-skipped =
    { $count ->
        [one] { $count } extra card skipped (unreadable note layout)
       *[other] { $count } extra cards skipped (unreadable note layout)
    }
# A CSV row that could not be read. $row is its line, $error what the
# CSV reader said (English, technical).
import-row-unreadable = row { $row }: { $error }
# A CSV row with an empty side. $row is its line.
import-row-empty = row { $row }: empty front or back
# Something about one file inside the package. $file is its name, $what
# what is wrong with it.
import-about-file = { $file }: { $what }

## Why a file could not be imported

# The file has too many cards. $max is the limit.
import-too-many-cards = that file has more than { $max } cards; imports are capped at { $max } per file. Split it in Anki first
# The same, counted after reading. $count is the cards, $max the limit.
import-file-too-many-cards = that file has { $count } cards; imports are capped at { $max } per file, so split it in Anki first
# The file has no card to import. $count is the rows skipped.
import-nothing =
    { $count ->
        [one] no importable cards found ({ $count } row skipped)
       *[other] no importable cards found ({ $count } rows skipped)
    }
# The CSV's first row could not be read.
import-csv-header-unreadable = the CSV's header row could not be read
# The CSV lacks the needed columns; 'front' and 'back' are the column
# names it must have, as written (keep them).
import-csv-columns = CSV needs 'front' and 'back' header columns
# The file is not an Anki package.
import-not-a-zip = that file is not an Anki package (not a zip archive)
# The package's list of media is damaged.
import-media-list-damaged = the package's media list is damaged and could not be read
# Part of the package could not be unpacked.
import-compressed-damaged = compressed content is damaged and could not be unpacked
# The package holds too many files. $max is the limit.
import-too-many-files = that package holds more than { $max } files; imports are capped at that many per file
# The package's list of media is too large.
import-media-list-large = the package's media list is too large
# A media file the package lists is not in it. $file is its name.
import-media-missing = { $file } is missing from the package
# A media file could not be read. $file is its name.
import-media-unreadable = { $file } could not be read from the package
# A media file is over the size limit. $max is the limit in bytes.
import-media-too-large = larger than the { $max } byte limit
# A media file's content does not match its checksum.
import-media-hash = content does not match its declared hash
# Unpacking would pass the size limit. $max is the limit in bytes.
import-decompress-limit = decompresses past the { $max } byte limit
# The Anki collection inside the package is damaged.
import-collection-damaged = the collection inside the package is damaged and could not be read
# The Anki collection inside the package is not laid out as expected.
import-collection-layout = the collection inside the package has an unexpected layout
# The package could not be prepared for reading.
import-staging-failed = the package could not be staged for reading; try again
# The Anki collection is too large. $mb is the limit in megabytes.
import-collection-large = the collection inside the file is larger than { $mb } MB
# The package holds no Anki collection.
import-no-collection = no Anki collection found inside the file
# The preview is too old to import from.
import-expired = preview expired; upload again
# The preview is another account's.
import-not-yours = preview belongs to another user
# Some cards name no deck and none was given.
import-needs-deck = some cards name no deck: name a deck to put the import under
# No deck name was given.
import-deck-required = deck name required
# An earlier import of the file stopped part way.
import-cut-off = this import was cut off part way through, and some of its cards are already in your decks; check them before importing the file again
# Too many uploads from one account at once.
import-busy-uploads = too many uploads at once; try again in a moment
# Another import is running for the account.
import-busy = another import is running; try again in a moment
# The deck name given is too long.
import-deck-too-long = deck name is too long
# The form came with no file.
import-no-file = no file
# The file's name is too long.
import-name-too-long = the file's name is too long
