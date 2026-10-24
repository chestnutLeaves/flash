### The open core's words in English: signing in, making an account from
### an invite, resetting a password, and approving an AI assistant's
### connection. Every message has a note above it: where it shows, who
### reads it, and what each variable holds.

## Shared by the sign-in pages

# Label over an email address field.
auth-email = Email
# Example text inside an empty email field. Keep it an address shape in
# the reader's language.
auth-email-placeholder = you@example.com

## The sign-in page

# Browser tab title of the sign-in page when its own methods sit folded
# under "Other ways to continue" (an extension's buttons lead).
login-title-fold = Continue - Flash
# Browser tab title of the sign-in page.
login-title = Sign in - Flash
# Line under the logo on the folded sign-in page: what Flash is.
login-tagline-fold = Spaced-repetition flashcards
# Line under the logo on the sign-in page: what Flash is.
login-tagline = Flashcards that schedule themselves.
# Disclosure that opens the passkey and password sign-in, under an
# extension's sign-in buttons.
login-other-ways = Other ways to continue
# Button: sign in with a passkey (Face ID, fingerprint, device PIN).
login-passkey = Sign in with passkey
# Small line between the passkey button and the email and password form
# (folded page).
login-or-email-password = or with your email and password
# Small line between the passkey button and the password form.
login-or-password = or with your password
# Example text inside the empty password field.
login-password-placeholder = Password
# Button that submits the email and password.
login-submit = Sign in
# Link to the page that resets a forgotten password.
login-forgot = Forgot password?
# Link to make an account by email (folded page).
login-create-email = Create an account with email
# Question before the "Create an account" link, for a first-time visitor.
login-new-here = New here?
# Link to make an account.
login-create = Create an account
# Error under the password form. Deliberately does not say which of the
# two was wrong.
login-wrong = Incorrect email or password.
# Note on the sign-in page when a session from an emailed link asked for
# something that needs a real sign-in (deleting the account, say).
login-again = For your security, sign in again to do this.
# Note on the sign-in page right after the person deleted their account.
login-deleted = Your account and all its data have been deleted.

## Making an account from an invite or a signup link

# Browser tab title of the page that finishes making an account.
enroll-title = Welcome - Flash
# Heading greeting the new person. $name is the name the account will
# carry.
enroll-heading = Welcome, { $name }
# Line under the heading when the address was just verified by the
# signup email.
enroll-intro-verified = Email verified. Now set up a passkey to start studying. Face ID, fingerprint, or your device PIN. No password.
# Line under the heading for an invite from an admin.
enroll-intro = Set up a passkey to start studying. Face ID, fingerprint, or your device PIN. No password.
# Button that makes the account with a passkey.
enroll-create-passkey = Create my passkey
# Disclosure that opens the password form instead.
enroll-use-password = Use a password instead
# Example text inside the new password field.
enroll-password-placeholder = Password (8+ characters)
# Example text inside the field that repeats the new password.
enroll-confirm-placeholder = Confirm password
# Button that makes the account with the password typed.
enroll-create-password = Create account with password

## Resetting a password

# Browser tab title of the page that asks for the reset email.
reset-title = Reset password - Flash
# Heading once the reset email is on its way.
reset-sent-heading = Check your email
# Text once the reset email is on its way. Never says whether the
# address has an account.
reset-sent-body =
    If that address has an account, a reset link is on its way.
    It works once and expires in 1 hour.
# Heading of the page that asks for the reset email.
reset-heading = Reset your password
# Line under that heading.
reset-intro = We'll email you a link to set a new one.
# Button that sends the reset email.
reset-send = Send reset link
# Link back to the sign-in page.
reset-back = Back to sign in
# Browser tab title of the page reached from the reset email.
reset-new-title = New password - Flash
# Heading of the page reached from the reset email.
reset-new-heading = Set a new password
# Line under that heading: other devices get signed out.
reset-new-intro = This signs you out everywhere else.
# Label over the new password field.
reset-new-password = New password
# Label over the field that repeats the new password (a verb: confirm it).
reset-new-confirm = Confirm
# Button that saves the new password.
reset-new-save = Save password

## Problems while signing in or making an account (shown as one line of
## text on the page, or as the whole page for a dead link)

# Shown when the email field was left empty.
auth-email-required = email required
# Shown when the email field holds something that is not an address.
auth-email-invalid = that doesn't look like an email address
# Shown when the address already has an account.
auth-email-taken = email already in use
# Shown when the new password is shorter than 8 characters.
auth-password-short = password must be at least 8 characters
# Shown when the new password is longer than the limit.
auth-password-long = password is too long
# Shown when the two new password fields differ.
auth-passwords-differ = passwords don't match
# Shown when the anti-bot check on a form was not completed.
auth-captcha = Please complete the verification check and try again.
# The line under the heading of the page for a reset link that is dead.
auth-reset-spent = This reset link is invalid, expired, or already used.
# The error line on the new password form when its link died meanwhile.
auth-reset-spent-line = this reset link is invalid, expired, or already used
# The whole page for an invite link that is not a link Flash made.
auth-invite-malformed = malformed invite
# The whole page for an invite link that is dead.
auth-invite-spent = This invite link is invalid, expired, or already used.
# The app's error line when an invite link it opened is dead.
auth-invite-spent-line = this invite link is invalid, expired, or already used
# Shown when making a passkey from an invite that has expired.
auth-invite-expired = invite is invalid or expired
# Shown when signing in with a passkey on an account that has none.
auth-no-passkeys = no passkeys enrolled
# Shown when the device's passkey answer did not verify.
auth-passkey-failed = passkey verification failed
# Shown when the device offered a passkey this server does not know.
auth-passkey-unknown = unknown credential
# Shown when a passkey answer arrives with no sign-in started.
auth-no-ceremony = no ceremony in progress

## Approving an AI assistant's connection (OAuth)

# Browser tab title of the page asking to approve a connection.
consent-title = Authorize - Flash
# Heading of the page asking to approve a connection.
consent-heading = Authorize access
# First paragraph. $client is the app's name, as whoever registered it
# chose it (untrusted).
consent-wants-html =
    <strong>{ $client }</strong> wants to access your flashcards:
    study sessions, card contents, and the ability to create and edit cards.
# Second paragraph. $host is the web address the approval is sent to.
consent-where-html =
    If you approve, the sign-in will be sent to <strong>{ $host }</strong>.
    The name above is chosen by whoever registered the app; the address is where
    your access actually goes.
# Warning under the two paragraphs.
consent-only-if =
    Only approve if you started this yourself from an app you trust (like
    adding a connector in Claude, ChatGPT, Grok, Mistral, or Gemini) and the
    address is that app's.
# Button that approves the connection.
consent-approve = Approve
# Button that refuses the connection.
consent-deny = Deny
# Browser tab title once a connection is approved.
consent-done-title = Connected - Flash
# Heading once a connection is approved.
consent-done-heading = You're connected
# Text once a connection is approved. $client is the app's name.
consent-done-body-html =
    <strong>{ $client }</strong> can reach your flashcards now. Go back
    to it to carry on.
# Button to Flash's home page.
consent-done-open = Open Flash
# Browser tab title when an app's connection request is refused.
oauth-error-title = Authorization request refused - Flash
# Heading when an app's connection request is refused.
oauth-error-heading = That request can't be completed
# Text when an app's connection request is refused. $code is the
# technical error code, shown as code.
oauth-error-body-html =
    The app that sent you here made a request Flash can't accept
    (<code>{ $code }</code>). Nothing was authorized and you have not been
    sent anywhere.
# Advice under it.
oauth-error-hint =
    If you were adding a connector, go back to the app and
    try again; if the problem repeats, the app's connector is misconfigured
    and its maker will want to know the code above.
# Button to Flash's home page.
oauth-error-back = Back to Flash
