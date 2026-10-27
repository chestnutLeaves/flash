### The open core's words in English: the settings page. Every message
### has a note above it: where it shows, who reads it, and what each
### variable holds.

# Browser tab title of the settings page.
settings-title = 设置 - Flash
# Heading of the settings page.
settings-heading = 设置
# Button that saves one card of settings.
settings-save = 保存
# Brief note after a change was saved.
settings-saved = 已保存

## Name

# Heading of the card with the person's name.
settings-name = 名称
# Error badge when the name is empty or too long.
settings-name-error = 需要输入名称（最多 80 个字符）
# What the name is for.
settings-name-hint = Flash 和您的 AI 如何称呼您。

## The person's cards

# Heading of the card about the person's cards.
settings-cards = 您的卡片
# How many cards the account holds. $count is that number.
settings-cards-count =
    { $count ->
       *[other] { $count } 张卡片。
    }
# Export is always possible; followed by the two export links.
settings-cards-export = 卡片始终属于您，导出功能也不会被阻止：
# Link that downloads the cards as an Anki package.
settings-export-apkg = Anki .apkg
# Link that downloads the cards as a spreadsheet file.
settings-export-csv = CSV

## Daily limits

# Heading of the card with the daily limits.
settings-limits = 每日限额
# What these limits are.
settings-limits-hint = 帐户默认值。任何牌组都可以在自己的页面上覆盖它们。
# Label: the most new cards per day.
settings-limit-new = 新卡/天
# Label: the most reviews per day.
settings-limit-reviews = 最大复习/天
# Label of the choice of when cards become due.
settings-due = 卡到期
# Choice: all of a day's cards are due from the day's start.
settings-due-day = 一整天
# Choice: each card is due at the hour it was scheduled for.
settings-due-exact = 在其确切时间
# Explains the two choices.
settings-due-hint = 一整天：您开始学习时，今天应复习的所有卡片都已准备好，一次学习即可完成当天内容。在准确时间：卡片会等到预定的时间才进入待复习状态。

## Grading

# Heading of the card for how an AI assistant gives grades when the
# person studies with it.
settings-grading = 您的助理如何评分
# What that card sets.
settings-grading-hint = 当您使用 AI 助手学习时，它如何报告成绩。
# Choice: the assistant grades without saying so.
settings-grading-silent = 静默：安静评分并继续对话
# Choice: the assistant says the grade, and the person may change it.
settings-grading-announce = 宣布：说出评分；您可以覆盖它
# Choice: the person grades every card.
settings-grading-self = 自行评分：要求您为每张卡片评分

## Password

# Heading of the password card.
settings-password = 密码
# Error badge when a password change failed.
settings-password-error = 修改失败。请检查当前密码，并确认两次输入的新密码一致（至少 8 个字符）
# Offer to add a password, to an account without one.
settings-password-add = 添加密码，以便无需通行密钥也能登录。
# The same when the account also signs in with a connected account.
settings-password-add-external = 添加密码，以便无需通行密钥或已连接账户也能登录。
# Placeholder of the new password field.
settings-password-new = 密码（8 个以上字符）
# Placeholder of the field that repeats it.
settings-password-confirm = 确认密码
# Button that sets the password.
settings-password-set = 设置密码
# Placeholder of the field for the password in use now.
settings-password-current = 当前密码
# Placeholder of the new password field when changing it.
settings-password-new-change = 新密码（8 个以上字符）
# Placeholder of the field that repeats it.
settings-password-confirm-change = 确认新密码
# Button that changes the password.
settings-password-change = 更改密码
# What changing it does to other devices.
settings-password-change-hint = 更改密码 退出您的其他设备。
# Button that removes the password.
settings-password-remove = 删除密码
# Advice to an account whose only way in is the password.
settings-password-tip =
    提示：通行密钥更快，也能抵御钓鱼攻击。下次受邀注册设备时，可以添加通行密钥。

## Passkeys

# Heading of the passkeys card.
settings-passkeys = 通行密钥
# Shown when there are none.
settings-passkeys-none = 未注册。
# When a passkey was added. $date is the date.
settings-passkey-added = 添加了 { $date }

## Devices

# Heading of the devices card.
settings-devices = 设备
# Brief note after a device was signed out.
settings-devices-signed-out = 已退出
# What the card lists.
settings-devices-hint = 每个浏览器和手机都登录到此账户。退出其中一个即可结束对应会话。
# Button that signs one device out.
settings-device-sign-out = 退出登录
# Button that signs out every device but this one.
settings-devices-sign-out-others = 在其他地方退出
# A browser's name. $browser is the browser (Chrome, Safari...), $os
# the system it runs on (Windows, Android...).
settings-device-browser = { $browser } 上 { $os }
# A browser that did not say what it is.
settings-device-browser-unknown = 浏览器
# A phone that did not say what it is.
settings-device-phone-unknown = 电话
# The browser this page is open in.
settings-device-this = 这个浏览器
# A browser's last use. $ago is how long ago (when-*-ago).
settings-device-used = 最后使用 { $ago }
# A browser never seen since it signed in. $ago is how long ago.
settings-device-signed-in = { $ago }前登录
# A phone with the app. $ago is how long ago it was last used.
settings-device-app-used = Flash 应用程序·上次使用 { $ago }
# A phone never seen since it signed in. $ago is how long ago.
settings-device-app-signed-in = Flash应用程序·已登录{ $ago }
# Added after a device that signed in through a link in an email.
# $detail is what the line said before it.
settings-device-via-link = { $detail } · 通过电子邮件链接登录

## Connected apps (AI assistants and the phone app)

# Heading of the connected apps card.
settings-apps = 连接的应用程序
# Brief note after an app was disconnected.
settings-apps-disconnected = 已断开连接
# What the card lists.
settings-apps-hint = 一切可以代表您读取和更改您的卡的东西。断开连接在任何地方都将其注销；它可以要求再次连接。
# Shown when nothing is connected.
settings-apps-none = 没有已连接的应用。
# How many devices the phone app is signed in on. $count is that
# number.
settings-app-devices =
    { $count ->
       *[other] { $count } 台设备
    }
# When the app was first connected. $date is the date.
settings-app-since = 自 { $date }
# When the app last used its access. $ago is how long ago.
settings-app-last-used = 最后使用 { $ago }
# Button that disconnects an app.
settings-app-disconnect = 断开
# The phone app's own row in the connected apps card.
settings-app-flash = Flash应用程序
# A connected app that gave no name of its own.
settings-app-unnamed = 连接器

## Inviting someone (admins only)

# Heading of the invite card.
settings-invite = 邀请某人（管理员）
# What an invite is.
settings-invite-hint =
    创建一个有效期为 7 天的一次性注册链接。
    打开链接的人可以选择通行密钥或密码，然后即可登录。
# Placeholder of the invited person's name field.
settings-invite-name = 他们的名字
# Button that makes the invite.
settings-invite-create = 创建邀请
# Above the link, which is shown once.
settings-invite-send = 向他们发送此链接（显示一次）：
# Button that copies the link.
settings-invite-copy = 复制链接

## Appearance

# Heading of the appearance card.
settings-appearance = 外观
# Which theme a new account has.
settings-appearance-hint = 深色是默认值。
# The themes (colour schemes).
theme-dark = 深色
# A light theme.
theme-light = 浅色
# A warm clay-coloured theme.
theme-clay = 黏土
# A brown coffee-coloured theme.
theme-mocha = 摩卡
# A grey-brown theme.
theme-taupe = 灰褐色

## Deleting the account

# Heading of the card that deletes the account.
settings-delete = 删除帐户
# The word the person types to confirm, in capitals; the page asks for
# exactly this word.
settings-delete-word = 删除
# Error badge when the word was not typed. $word is settings-delete-word.
settings-delete-error-confirm = 准确输入{ $word }进行确认
# Error badge when the password was wrong.
settings-delete-error-password = 该密码不正确
# Error badge when the person is the only admin.
settings-delete-error-admin = 您是唯一的管理员，因此请先让其他人成为管理员
# What deleting removes.
settings-delete-hint =
    永久删除您的账户及其中的所有内容：牌组、卡片、复习历史、媒体、通行密钥、会话以及已连接的 AI。此操作无法撤销。
# Offer to export first; followed by the two export links.
settings-delete-copy = 想要先复制一份吗？
# Placeholder of the confirm field. $word is settings-delete-word.
settings-delete-type = 输入{ $word }以确认
# Tooltip of the confirm field. $word is settings-delete-word.
settings-delete-type-tip = 输入{ $word }大写字母
# Button that deletes the account.
settings-delete-submit = 删除我的帐户

## Errors

# An invite was made with no name, or one longer than 80 characters.
invite-name-required = 名称必填
