### The open core's words in English: refusals the phone app shows. The
### JSON API answers a refusal with a code and a sentence; the app words
### the codes it knows itself and shows this sentence for the rest. Some
### also reach a page. Every message has a note above it: where it
### shows, who reads it, and what each variable holds.

## Any request

# The app's sign-in is missing or has run out; the app signs in again.
api-unauthorized = 需要登录
# Signing in from the app: the email or password was wrong, or a
# sign-in with a passkey or a provider failed. Deliberately says nothing
# about which part was wrong.
api-invalid-credentials = 这些细节不匹配
# Something only an admin may do.
api-forbidden = 不允许
# Something the app asked for is gone (deleted on another device, say).
# $what is the kind of thing as the server names it, in English (deck,
# card, note, media, session, passkey, device, route, ...); a
# translation picks its own words by it.
api-not-found = 未找到：{ $what }
# A feature this server has not set up. $what is the feature as the
# server names it, in English (passkeys, reset, ...); a translation
# picks its own words by it.
api-not-available = { $what } 在此服务器上不可用
# A file or a request too big for the server to take.
api-too-large = 上传太大
# Too many requests from one place in a short time.
api-rate-limited = 操作过于频繁

## Signing in and account settings

# Adding or using a passkey took too long; the person starts over.
api-passkey-expired = 通行密钥验证已过期；请重新开始
# Removing the last way into the account (password, passkey, or a
# connected account).
api-last-login-method = 这是唯一的登录方式，因此请先添加另一种方法
# Changing the password without typing the current one.
api-current-password = 需要当前密码

## Decks and files

# Importing a file into a deck whose name is too long.
api-deck-name-long = 牌组名称太长
# Uploading a file whose name is too long.
api-file-name-long = 文件名太长
# A card's tags are too many. $max is how many a card may carry.
store-error-tags-many = 一张卡最多可以携带{ $max }标签
# A tag is too long. $max is the longest a tag may be, in bytes.
store-error-tag-long = 标签最多可为{ $max }字节
# Adding cards past the account's card limit. $current is how many
# cards the account has, $cap how many it may have.
card-limit-reached = 已达到卡片上限：{ $current } / { $cap } 张
# An import, an export or the Stats page could not start: the same
# person already has one running.
heavy-busy-yours = 您的另一个导入、导出或统计页面仍在运行；稍后再试
# An import, an export or the Stats page could not start: the server is
# running as many as it will.
heavy-busy-everyone = 服务器正忙于其他导入和导出；稍后再试
