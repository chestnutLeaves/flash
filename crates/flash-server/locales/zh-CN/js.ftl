### The open core's words in English that its pages' scripts say
### (static/*.js, through static/i18n.js). Every message has a note above
### it: where it shows, who reads it, and what each variable holds. A
### script's message keeps to text, variables and at most one choice.

## Shared behaviours (ui.js)

# Spoken name of the play button of a sound on a card.
js-ui-play-audio = 播放音频
# Spoken name of that button while the sound plays.
js-ui-pause-audio = 暂停音频
# A copy button's words just after it copied.
js-ui-copied = 已复制
# A copy button's words when copying failed.
js-ui-copy-failed = 无法复制
# Tooltip of the arrow that folds a deck's subdecks, while they are
# hidden (decks-fold-show).
js-ui-show-subdecks = 显示子牌组
# Tooltip of that arrow while they show (decks-fold-hide).
js-ui-hide-subdecks = 隐藏子牌组
# The decks page's link that shows archived decks (decks-show-archived).
js-ui-show-archived = 显示已存档
# That link while they show (decks-hide-archived).
js-ui-hide-archived = 隐藏已存档

## Signing in with a passkey (webauthn.js)

# The passkey prompt was dismissed or timed out.
js-passkey-cancelled = 已取消或超时。
# Something went wrong with no more to say.
js-passkey-failed = 那没有用。再试一次。
# The server's answer was not what the page expected.
js-passkey-bad-list = 服务器的凭据列表不是列表。
# The same, for an entry of it.
js-passkey-bad-entry = 服务器的凭证列表包含其他内容。
# The same, for an entry that is not a passkey.
js-passkey-not-passkey = 服务器返回的凭据不是通行密钥。
# The browser made no passkey.
js-passkey-none-made = 浏览器没有创建通行密钥。
# The browser used no passkey.
js-passkey-none-used = 浏览器没有使用通行密钥。
# Signing in: the passkey prompt ended with none (cancelled, or none on
# this device for this site). "Another way" is the page's other ways in.
js-passkey-not-used = 没有使用通行密钥。请使用其他方式登录，然后在“设置”中重新添加通行密钥。
# The link to make an account is cut short.
js-passkey-link-incomplete = 此注册链接不完整。

## A deck's page (deck.js)

# The selection bar: how many cards are selected. $count is how many.
js-deck-selected = { $count ->
       *[other] 已选择 { $count } 张
    }
# The selection bar's link that selects every card the list holds, past
# this page. $count is how many that is.
js-deck-select-all = { $count ->
       *[other] 全选 { $count } 张
    }
# The button that deletes the selected cards for good. $count is how
# many.
js-deck-delete-n = { $count ->
       *[other] 删除 { $count } 张卡片
    }
# Spoken name and tooltip of a card's delete button after one click:
# a second click deletes it.
js-deck-row-confirm = 再次点击即可删除
# The button that opens the form that adds a card.
js-deck-add = 添加
# The same button while the form is open: it closes it.
js-deck-close = 关闭
# Shown when an action on the cards could not reach the server or was
# refused without words of its own.
js-deck-failed = 那没有通过。再试一次。

## The card editor (editor.js)

# Label of the first field of a plain note (editor-front).
js-editor-front = 正面
# Label of the second field of a plain note (editor-back).
js-editor-back = 背面
# Grey text in the empty first field (editor-front-placeholder).
js-editor-question = 问题
# Grey text in the empty second field (editor-back-placeholder).
js-editor-answer = 回答
# Grey text in the second field of a typed-answer note.
js-editor-answer-to-type = 回答输入
# Label of a cloze note's first field: the text with blanks.
js-editor-text = 文本
# Label of a cloze note's second field: extra shown with the answer.
js-editor-extra = 额外内容
# Grey text in a cloze note's first field; "{{c1::blanks}}" is the cloze
# syntax, kept as it is.
js-editor-cloze-hint = 带有 {"{{"}c1::blanks{"}}"} 的句子
# Grey text in a cloze note's second field.
js-editor-extra-hint = 与答案一起显示（可选）
# Shown in the field while a file uploads. $name is the file's name.
js-editor-uploading = 正在上传{ $name }...
# An upload failed.
js-editor-upload-failed = 上传失败。
# A formatting button pressed with nothing selected.
js-editor-select-first = 首先选择一些文本。
# Saving while a file still uploads.
js-editor-wait-upload = 等待上传完成。
# The button that closes the full editor, back to the two fields.
js-editor-simple = 简单
# The button that opens the full editor (deck-advanced).
js-editor-advanced = 高级
