### The open core's words in English: signing in, making an account from
### an invite, resetting a password, and approving an AI assistant's
### connection. Every message has a note above it: where it shows, who
### reads it, and what each variable holds.

## Shared by the sign-in pages

# Label over an email address field.
auth-email = 电子邮件
# Example text inside an empty email field. Keep it an address shape in
# the reader's language.
auth-email-placeholder = 你@example.com

## The sign-in page

# Browser tab title of the sign-in page when its own methods sit folded
# under "Other ways to continue" (an extension's buttons lead).
login-title-fold = 继续 - Flash
# Browser tab title of the sign-in page.
login-title = 登录 - Flash
# Line under the logo on the folded sign-in page: what Flash is.
login-tagline-fold = 间隔重复抽认卡
# Line under the logo on the sign-in page: what Flash is.
login-tagline = 可自行安排学习计划的记忆卡片。
# Disclosure that opens the passkey and password sign-in, under an
# extension's sign-in buttons.
login-other-ways = 其他继续方式
# Button: sign in with a passkey (Face ID, fingerprint, device PIN).
login-passkey = 使用通行密钥登录
# Small line between the passkey button and the email and password form
# (folded page).
login-or-email-password = 或使用您的电子邮件和密码
# Small line between the passkey button and the password form.
login-or-password = 或使用您的密码
# Example text inside the empty password field.
login-password-placeholder = 密码
# Button that submits the email and password.
login-submit = 登录
# Link to the page that resets a forgotten password.
login-forgot = 忘记密码？
# Link to make an account by email (folded page).
login-create-email = 使用电子邮件创建帐户
# Question before the "Create an account" link, for a first-time visitor.
login-new-here = 新来的？
# Link to make an account.
login-create = 创建一个帐户
# Error under the password form. Deliberately does not say which of the
# two was wrong.
login-wrong = 电子邮件或密码不正确。
# Note on the sign-in page when a session from an emailed link asked for
# something that needs a real sign-in (deleting the account, say).
login-again = 为了您的安全，请重新登录才能执行此操作。
# Note on the sign-in page right after the person deleted their account.
login-deleted = 您的帐户及其所有数据已被删除。

## Making an account from an invite or a signup link

# Browser tab title of the page that finishes making an account.
enroll-title = 欢迎 - Flash
# Heading greeting the new person. $name is the name the account will
# carry.
enroll-heading = 欢迎您，{ $name }
# Line under the heading when the address was just verified by the
# signup email.
enroll-intro-verified = 电子邮件已验证。现在设置通行密钥即可开始学习。可使用面容 ID、指纹或设备 PIN，无需密码。
# Line under the heading for an invite from an admin.
enroll-intro = 设置通行密钥即可开始学习。可使用面容 ID、指纹或设备 PIN，无需密码。
# Button that makes the account with a passkey.
enroll-create-passkey = 创建我的通行密钥
# Disclosure that opens the password form instead.
enroll-use-password = 请改用密码
# Example text inside the new password field.
enroll-password-placeholder = 密码（8 个以上字符）
# Example text inside the field that repeats the new password.
enroll-confirm-placeholder = 确认密码
# Button that makes the account with the password typed.
enroll-create-password = 使用密码创建帐户

## Resetting a password

# Browser tab title of the page that asks for the reset email.
reset-title = 重置密码 - Flash
# Heading once the reset email is on its way.
reset-sent-heading = 检查您的电子邮件
# Text once the reset email is on its way. Never says whether the
# address has an account.
reset-sent-body =
    如果该地址对应一个账户，重置链接将发送到你的邮箱。
    链接只能使用一次，并会在 1 小时后过期。
# Heading of the page that asks for the reset email.
reset-heading = 重置您的密码
# Line under that heading.
reset-intro = 我们会通过电子邮件向您发送设置新密码的链接。
# Button that sends the reset email.
reset-send = 发送重置链接
# Link back to the sign-in page.
reset-back = 返回登录
# Browser tab title of the page reached from the reset email.
reset-new-title = 新密码 - Flash
# Heading of the page reached from the reset email.
reset-new-heading = 设置新密码
# Line under that heading: other devices get signed out.
reset-new-intro = 这会让你在其他地方退出。
# Label over the new password field.
reset-new-password = 新密码
# Label over the field that repeats the new password (a verb: confirm it).
reset-new-confirm = 确认
# Button that saves the new password.
reset-new-save = 保存密码

## Problems while signing in or making an account (shown as one line of
## text on the page, or as the whole page for a dead link)

# Shown when the email field was left empty.
auth-email-required = 需要电子邮件
# Shown when the email field holds something that is not an address.
auth-email-invalid = 这看起来不像电子邮件地址
# Shown when the address already has an account.
auth-email-taken = 电子邮件已被使用
# Shown when the new password is shorter than 8 characters.
auth-password-short = 密码必须至少为 8 个字符
# Shown when the new password is longer than the limit.
auth-password-long = 密码太长
# Shown when the two new password fields differ.
auth-passwords-differ = 密码不匹配
# Shown when the anti-bot check on a form was not completed.
auth-captcha = 请完成验证检查并重试。
# The line under the heading of the page for a reset link that is dead.
auth-reset-spent = 此重置链接无效、已过期或已使用。
# The error line on the new password form when its link died meanwhile.
auth-reset-spent-line = 此重置链接无效、已过期或已使用
# The whole page for an invite link that is not a link Flash made.
auth-invite-malformed = 邀请格式错误
# The whole page for an invite link that is dead.
auth-invite-spent = 此邀请链接无效、已过期或已使用。
# The app's error line when an invite link it opened is dead.
auth-invite-spent-line = 此邀请链接无效、已过期或已使用
# Shown when making a passkey from an invite that has expired.
auth-invite-expired = 邀请无效或已过期
# Shown when signing in with a passkey on an account that has none.
auth-no-passkeys = 没有已注册的通行密钥
# Shown when the device's passkey answer did not verify.
auth-passkey-failed = 通行密钥验证失败
# Shown when the device offered a passkey this server does not know.
auth-passkey-unknown = 未知的凭证
# Shown when a passkey answer arrives with no sign-in started.
auth-no-ceremony = 没有正在进行的仪式

## Approving an AI assistant's connection (OAuth)

# Browser tab title of the page asking to approve a connection.
consent-title = 授权 - Flash
# Heading of the page asking to approve a connection.
consent-heading = 授权访问
# First paragraph. $client is the app's name, as whoever registered it
# chose it (untrusted).
consent-wants-html =
    <strong>{ $client }</strong> 想要访问你的记忆卡片：
    学习会话、卡片内容，以及创建和编辑卡片的权限。
# Second paragraph. $host is the web address the approval is sent to.
consent-where-html =
    如果批准，登录信息会发送到 <strong>{ $host }</strong>。
    上面的名称由注册该应用的人设置；该地址是实际接收访问权限的位置。
# Warning under the two paragraphs.
consent-only-if =
    只有在你从信任的应用主动发起此操作（例如在 Claude、ChatGPT、Grok、Mistral 或 Gemini 中
    添加连接器），并且地址确实属于该应用时，才应批准。
# Button that approves the connection.
consent-approve = 批准
# Button that refuses the connection.
consent-deny = 否定
# Browser tab title once a connection is approved.
consent-done-title = 已连接 - Flash
# Heading once a connection is approved.
consent-done-heading = 您已连接
# Text once a connection is approved. $client is the app's name.
consent-done-body-html =
    <strong>{ $client }</strong> 现在可以访问你的记忆卡片。返回该应用即可继续。
# Button to Flash's home page.
consent-done-open = 打开 Flash
# Browser tab title when an app's connection request is refused.
oauth-error-title = 授权请求被拒绝 - Flash
# Heading when an app's connection request is refused.
oauth-error-heading = 该请求无法完成
# Text when an app's connection request is refused. $code is the
# technical error code, shown as code.
oauth-error-body-html =
    将你带到这里的应用发出了 Flash 无法接受的请求（<code>{ $code }</code>）。没有授予任何权限，
    你也没有被重定向到其他位置。
# Advice under it.
oauth-error-hint =
    如果你正在添加连接器，请返回该应用重试；如果问题重复出现，说明应用的连接器配置有误，
    应将上面的错误代码反馈给应用维护者。
# Button to Flash's home page.
oauth-error-back = 返回 Flash
