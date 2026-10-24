### The open core's words in English: the settings page. Every message
### has a note above it: where it shows, who reads it, and what each
### variable holds.

# Browser tab title of the settings page.
settings-title = Settings - Flash
# Heading of the settings page.
settings-heading = Settings
# Button that saves one card of settings.
settings-save = Save
# Brief note after a change was saved.
settings-saved = Saved

## Name

# Heading of the card with the person's name.
settings-name = Name
# Error badge when the name is empty or too long.
settings-name-error = a name is required (up to 80 characters)
# What the name is for.
settings-name-hint = What Flash and your AI call you.

## The person's cards

# Heading of the card about the person's cards.
settings-cards = Your cards
# How many cards the account holds. $count is that number.
settings-cards-count =
    { $count ->
        [one] { $count } card.
       *[other] { $count } cards.
    }
# Export is always possible; followed by the two export links.
settings-cards-export = Your cards are always yours, and export is never blocked:
# Link that downloads the cards as an Anki package.
settings-export-apkg = Anki .apkg
# Link that downloads the cards as a spreadsheet file.
settings-export-csv = CSV

## Daily limits

# Heading of the card with the daily limits.
settings-limits = Daily limits
# What these limits are.
settings-limits-hint = Account defaults. Any deck can override them on its own page.
# Label: the most new cards per day.
settings-limit-new = New cards / day
# Label: the most reviews per day.
settings-limit-reviews = Max reviews / day
# Label of the choice of when cards become due.
settings-due = Cards come due
# Choice: all of a day's cards are due from the day's start.
settings-due-day = For the whole day
# Choice: each card is due at the hour it was scheduled for.
settings-due-exact = At their exact time
# Explains the two choices.
settings-due-hint = For the whole day: everything due today is ready when you sit down, so one session clears the day. At their exact time: a card waits until the hour it was scheduled for.

## Grading

# Heading of the card for how an AI assistant gives grades when the
# person studies with it.
settings-grading = How your assistant grades
# What that card sets.
settings-grading-hint = How your AI assistant reports grades when you study with it.
# Choice: the assistant grades without saying so.
settings-grading-silent = Silent: grades quietly and keeps the conversation moving
# Choice: the assistant says the grade, and the person may change it.
settings-grading-announce = Announce: says the grade; you can override it
# Choice: the person grades every card.
settings-grading-self = Self-grade: asks you to rate every card

## Password

# Heading of the password card.
settings-password = Password
# Error badge when a password change failed.
settings-password-error = that didn't work. Check the current password and that the new ones match (8+ characters)
# Offer to add a password, to an account without one.
settings-password-add = Add a password to sign in without a passkey.
# The same when the account also signs in with a connected account.
settings-password-add-external = Add a password to sign in without a passkey or a connected account.
# Placeholder of the new password field.
settings-password-new = Password (8+ characters)
# Placeholder of the field that repeats it.
settings-password-confirm = Confirm password
# Button that sets the password.
settings-password-set = Set password
# Placeholder of the field for the password in use now.
settings-password-current = Current password
# Placeholder of the new password field when changing it.
settings-password-new-change = New password (8+ characters)
# Placeholder of the field that repeats it.
settings-password-confirm-change = Confirm new password
# Button that changes the password.
settings-password-change = Change password
# What changing it does to other devices.
settings-password-change-hint = Changing your password signs out your other devices.
# Button that removes the password.
settings-password-remove = Remove password
# Advice to an account whose only way in is the password.
settings-password-tip =
    Tip: passkeys are faster and phishing-resistant. You can add one the next
    time you're invited to enroll a device.

## Passkeys

# Heading of the passkeys card.
settings-passkeys = Passkeys
# Shown when there are none.
settings-passkeys-none = None enrolled.
# When a passkey was added. $date is the date.
settings-passkey-added = added { $date }

## Devices

# Heading of the devices card.
settings-devices = Devices
# Brief note after a device was signed out.
settings-devices-signed-out = Signed out
# What the card lists.
settings-devices-hint = Every browser and phone signed in to this account. Signing one out ends its session there.
# Button that signs one device out.
settings-device-sign-out = Sign out
# Button that signs out every device but this one.
settings-devices-sign-out-others = Sign out everywhere else
# A browser's name. $browser is the browser (Chrome, Safari...), $os
# the system it runs on (Windows, Android...).
settings-device-browser = { $browser } on { $os }
# A browser that did not say what it is.
settings-device-browser-unknown = Browser
# A phone that did not say what it is.
settings-device-phone-unknown = Phone
# The browser this page is open in.
settings-device-this = This browser
# A browser's last use. $ago is how long ago (when-*-ago).
settings-device-used = Last used { $ago }
# A browser never seen since it signed in. $ago is how long ago.
settings-device-signed-in = Signed in { $ago }
# A phone with the app. $ago is how long ago it was last used.
settings-device-app-used = Flash app · last used { $ago }
# A phone never seen since it signed in. $ago is how long ago.
settings-device-app-signed-in = Flash app · signed in { $ago }
# Added after a device that signed in through a link in an email.
# $detail is what the line said before it.
settings-device-via-link = { $detail } · signed in from an email link

## Connected apps (AI assistants and the phone app)

# Heading of the connected apps card.
settings-apps = Connected apps
# Brief note after an app was disconnected.
settings-apps-disconnected = Disconnected
# What the card lists.
settings-apps-hint = Everything that can read and change your cards on your behalf. Disconnecting signs it out everywhere; it can ask to connect again.
# Shown when nothing is connected.
settings-apps-none = Nothing connected.
# How many devices the phone app is signed in on. $count is that
# number.
settings-app-devices =
    { $count ->
        [one] { $count } device
       *[other] { $count } devices
    }
# When the app was first connected. $date is the date.
settings-app-since = since { $date }
# When the app last used its access. $ago is how long ago.
settings-app-last-used = last used { $ago }
# Button that disconnects an app.
settings-app-disconnect = Disconnect
# The phone app's own row in the connected apps card.
settings-app-flash = Flash app
# A connected app that gave no name of its own.
settings-app-unnamed = Connector

## Inviting someone (admins only)

# Heading of the invite card.
settings-invite = Invite someone (admin)
# What an invite is.
settings-invite-hint =
    Creates a one-time enrollment link, valid for 7 days.
    Whoever opens it picks a passkey or a password and is in.
# Placeholder of the invited person's name field.
settings-invite-name = Their name
# Button that makes the invite.
settings-invite-create = Create invite
# Above the link, which is shown once.
settings-invite-send = Send them this link (shown once):
# Button that copies the link.
settings-invite-copy = Copy link

## Appearance

# Heading of the appearance card.
settings-appearance = Appearance
# Which theme a new account has.
settings-appearance-hint = Dark is the default.
# The themes (colour schemes).
theme-dark = Dark
# A light theme.
theme-light = Light
# A warm clay-coloured theme.
theme-clay = Clay
# A brown coffee-coloured theme.
theme-mocha = Mocha
# A grey-brown theme.
theme-taupe = Taupe

## Deleting the account

# Heading of the card that deletes the account.
settings-delete = Delete account
# The word the person types to confirm, in capitals; the page asks for
# exactly this word.
settings-delete-word = DELETE
# Error badge when the word was not typed. $word is settings-delete-word.
settings-delete-error-confirm = type { $word } exactly to confirm
# Error badge when the password was wrong.
settings-delete-error-password = that password is incorrect
# Error badge when the person is the only admin.
settings-delete-error-admin = you're the only admin, so make someone else admin first
# What deleting removes.
settings-delete-hint =
    Permanently removes your account and everything in it: decks, cards, review
    history, media, passkeys, sessions, and any connected AI. This can't be undone.
# Offer to export first; followed by the two export links.
settings-delete-copy = Want a copy first?
# Placeholder of the confirm field. $word is settings-delete-word.
settings-delete-type = Type { $word } to confirm
# Tooltip of the confirm field. $word is settings-delete-word.
settings-delete-type-tip = Type { $word } in capitals
# Button that deletes the account.
settings-delete-submit = Delete my account

## Errors

# An invite was made with no name, or one longer than 80 characters.
invite-name-required = name required
