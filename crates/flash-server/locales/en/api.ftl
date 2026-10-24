### The open core's words in English: refusals the phone app shows. The
### JSON API answers a refusal with a code and a sentence; the app words
### the codes it knows itself and shows this sentence for the rest. Some
### also reach a page. Every message has a note above it: where it
### shows, who reads it, and what each variable holds.

## Any request

# The app's sign-in is missing or has run out; the app signs in again.
api-unauthorized = sign in required
# Signing in from the app: the email or password was wrong, or a
# sign-in with a passkey or a provider failed. Deliberately says nothing
# about which part was wrong.
api-invalid-credentials = those details didn't match
# Something only an admin may do.
api-forbidden = not allowed
# Something the app asked for is gone (deleted on another device, say).
# $what is the kind of thing as the server names it, in English (deck,
# card, note, media, session, passkey, device, route, ...); a
# translation picks its own words by it.
api-not-found = not found: { $what }
# A feature this server has not set up. $what is the feature as the
# server names it, in English (passkeys, reset, ...); a translation
# picks its own words by it.
api-not-available = { $what } is not available on this server
# A file or a request too big for the server to take.
api-too-large = that upload is too large
# Too many requests from one place in a short time.
api-rate-limited = slow down

## Signing in and account settings

# Adding or using a passkey took too long; the person starts over.
api-passkey-expired = that passkey ceremony expired; start again
# Removing the last way into the account (password, passkey, or a
# connected account).
api-last-login-method = that's the only way to sign in, so add another method first
# Changing the password without typing the current one.
api-current-password = the current password is required

## Decks and files

# Importing a file into a deck whose name is too long.
api-deck-name-long = deck name is too long
# Uploading a file whose name is too long.
api-file-name-long = the file's name is too long
# A card's tags are too many. $max is how many a card may carry.
store-error-tags-many = a card may carry at most { $max } tags
# A tag is too long. $max is the longest a tag may be, in bytes.
store-error-tag-long = a tag may be at most { $max } bytes
# Adding cards past the account's card limit. $current is how many
# cards the account has, $cap how many it may have.
card-limit-reached = card limit reached: { $current } of { $cap } cards
# An import, an export or the Stats page could not start: the same
# person already has one running.
heavy-busy-yours = another import, export or stats page of yours is still running; try again in a moment
# An import, an export or the Stats page could not start: the server is
# running as many as it will.
heavy-busy-everyone = the server is busy with other imports and exports; try again in a moment
