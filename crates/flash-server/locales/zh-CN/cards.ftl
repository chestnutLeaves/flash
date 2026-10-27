### The open core's words in English: a deck's list of cards, and the
### card editor (adding and editing a card). A note is what a person
### writes; it makes one card, or several (a reversed note makes two, a
### cloze note one per blank). Every message has a note above it: where
### it shows, who reads it, and what each variable holds.

## A deck's list of cards

# Shown when the deck has no cards.
cards-none = 没有卡。
# Shown when the search or the filter leaves no card.
cards-none-filtered = 没有匹配的卡片。
# Chip above the list while it shows the archived cards; clicking it
# goes back to the cards in study.
cards-chip-archived = 已存档
# The filter's choice, and its chip, for the deck's own cards without
# its subdecks'.
cards-own = 仅此牌组
# Spoken name of a chip's remove button. $name is what the chip says (a
# tag, a deck, "Archived").
cards-chip-remove = 从过滤器中删除 { $name }
# Spoken name of the choice between cards with any of the chosen tags
# and cards with all of them.
cards-match-label = 匹配标签
# That choice: a card with any one of the tags.
cards-match-any = 任何
# That choice: only a card with every one of the tags.
cards-match-all = 全部
# Link that clears the whole filter.
cards-chip-clear = 清除
# Spoken name of the box that selects every card on the page.
cards-select-page = 选择此页面上的每张卡
# Spoken name of a card's box.
cards-select-card = 选择这张卡
# Tooltip of the label under a card that lives in a subdeck. $name is
# that subdeck's name.
cards-from-title = 在{ $name }中。只显示它的牌。
# Column heading: the card's question side.
cards-col-front = 正面
# Column heading: the card's answer side.
cards-col-back = 背面
# Column heading: the card's tags.
cards-col-tags = 标签
# Spoken name of the pager under the list.
cards-pages = 卡片页
# Pager button to the page before.
cards-previous = 上一页
# Pager button to the page after.
cards-next = 下一页
# Where the pager is. $page is this page, $pages how many, $count how
# many cards.
cards-page-of = 第 { $page } / { $pages } 页，共 { $count } 张卡片
# Spoken name of a card's edit button.
cards-edit-label = 编辑卡
# Tooltip of that button.
cards-edit = 编辑
# Spoken name of a card's unarchive button.
cards-unarchive-label = 取消归档卡片
# Tooltip of that button.
cards-unarchive = 取消归档
# Spoken name of a card's archive button.
cards-archive-label = 归档卡片
# Tooltip of that button.
cards-archive = 归档
# Spoken name of a card's delete button.
cards-delete-label = 删除卡
# Tooltip of that button.
cards-delete = 删除
# What an action on the selected cards did, in the note at the foot of
# the screen. $count is how many cards it changed.
cards-bulk-archived = { $count ->
       *[other] 已归档 { $count } 张卡片
    }
# The same for bringing archived cards back into study. $count is how
# many cards came back.
cards-bulk-unarchived = { $count ->
       *[other] 已将 { $count } 张卡片恢复到学习中
    }
# The same for deleting. $count is how many cards were deleted.
cards-bulk-deleted = { $count ->
       *[other] 已删除 { $count } 张卡片
    }
# The same for moving. $count is how many cards moved (a note's other
# cards go with it), $deck the deck they went to.
cards-bulk-moved = { $count ->
       *[other] 已将 { $count } 张卡片移至 { $deck }
    }
# The same for adding tags. $count is how many cards got a new tag.
cards-bulk-tagged = { $count ->
       *[other] 已为 { $count } 张卡片添加标签
    }
# The same for removing tags. $count is how many cards lost a tag.
cards-bulk-untagged = { $count ->
       *[other] 已从 { $count } 张卡片移除标签
    }
# The note after Undo put an action back.
cards-bulk-undone = 未完成
# Button in that note that puts the action back.
cards-bulk-undo = 撤消
# An action on "all the cards that match" found a different number than
# the page showed: cards were added or changed meanwhile.
cards-error-bulk-changed = 当您选择时，列表会发生变化。再次选择卡片。
# An action named more cards than one action takes. $max is the limit.
cards-error-bulk-many = 一次最多可以更换{ $max }卡。
# Moving cards into a deck that is archived.
cards-error-move-archived = 该牌组已存档。首先将其取消存档。
# Adding or removing tags with none written.
cards-error-tags-none = 至少写一个标签。
# Badge naming how a card comes from its note: the blank it hides.
# $n is the blank's number.
cards-kind-cloze = 完型填空 { $n }
# Badge: the card asks the back and answers with the front.
cards-kind-reversed = 颠倒的
# Badge: the card asks for its answer to be typed.
cards-kind-typed = 打字的

## A card's own page and the edit dialog

# Browser tab title of a card's own page.
editor-page-title = 编辑卡-Flash
# Spoken name of the link back to the card's deck.
editor-crumbs = 牌组
# Heading of the editor (page and dialog).
editor-heading = 编辑卡
# Label of the tags field.
editor-tags = 标签
# Hint under the tags field.
editor-tags-hint = 以逗号分隔，适用于本笔记的每张卡片。
# Disclosure listing the note's earlier versions. $count is how many.
editor-history =
    { $count ->
       *[other] 历史记录（之前有 { $count } 个版本）
    }
# When an earlier version was replaced, and by what. $ago is how long
# ago (when-*-ago), $by what replaced it (editor-by-*).
editor-replaced = { $ago } , 之前 { $by }
# What replaced a version: an edit on this website.
editor-by-web = 此处编辑
# What replaced a version: an AI assistant's edit.
editor-by-mcp = 助理的编辑
# What replaced a version: an edit in the phone app.
editor-by-app = 应用程序中的编辑
# What replaced a version: an edit from another device.
editor-by-sync = 在其他地方进行的编辑
# What replaced a version: an earlier version brought back.
editor-by-restore = 恢复
# Button that brings an earlier version back.
editor-restore = 恢复
# Brief note in the editor once an earlier version is back.
editor-restored = 恢复了早期版本。
# Brief note in the editor once its changes are saved.
editor-saved = 已保存。
# Button that leaves without saving.
editor-cancel = 取消
# Button that saves the card.
editor-save = 保存

## The editor itself

# Label of the note type picker.
editor-type = 类型
# The note types, as Anki names them. A plain question and answer.
note-type-basic = 基本的
# A question and answer, plus a second card the other way round.
note-type-reversed = 基本卡（和反转卡）
# A question whose answer is typed.
note-type-typed = 基本（输入答案）
# A text with blanks to fill in.
note-type-cloze = 完型填空
# Badge when several cards share one note. $count is how many.
editor-siblings = { $count } 卡分享此注释
# Tooltip of that badge.
editor-siblings-hint = 保存会重新生成该笔记的每张卡
# Spoken name of the formatting toolbar.
editor-toolbar = 格式化
# Tooltip of the toolbar's bold button; Ctrl+B is its shortcut.
editor-bold-tip = 粗体 (Ctrl+B)
# Spoken name of the bold button.
editor-bold = 粗体
# Tooltip of the italic button; Ctrl+I is its shortcut.
editor-italic-tip = 斜体 (Ctrl+I)
# Spoken name of the italic button.
editor-italic = 斜体
# Tooltip of the underline button; Ctrl+U is its shortcut.
editor-underline-tip = 下划线 (Ctrl+U)
# Spoken name of the underline button.
editor-underline = 强调
# Tooltip and spoken name of the strikethrough button.
editor-strike = 删除线
# Tooltip and spoken name of the superscript button.
editor-superscript = 上标
# Tooltip and spoken name of the subscript button.
editor-subscript = 下标
# The text colour menu.
editor-text-color = 文字颜色
# A colour's swatch in it. $hue is the colour (hue-*).
editor-text-hue = 文本 { $hue }
# The swatch that removes the colour.
editor-no-color = 无颜色
# The highlight menu.
editor-highlight = 强调
# A colour's swatch in it. $hue is the colour (hue-*).
editor-highlight-hue = 突出显示 { $hue }
# The swatch that removes the highlight.
editor-no-highlight = 无亮点
# Tooltip and spoken name of the bulleted list button.
editor-bulleted = 项目符号列表
# Tooltip and spoken name of the numbered list button.
editor-numbered = 编号列表
# Tooltip and spoken name of the button that removes formatting.
editor-clear = 清除格式
# Tooltip of the button that attaches a file.
editor-attach-tip = 附加图像、音频或视频
# Its spoken name.
editor-attach = 附加媒体
# Tooltip of the button that blanks the selection on a cloze note.
editor-cloze-tip = 完形填空删除（Ctrl+Shift+C；Ctrl+Shift+Alt+C 重复使用最后一个数字）
# Its spoken name.
editor-cloze = 完形填空删除
# Tooltip of the button that shows the field's HTML.
editor-source-tip = HTML 源代码
# Its spoken name.
editor-source = 切换 HTML 源
# Label of the question field.
editor-front = 正面
# Label of the answer field.
editor-back = 背面
# Grey text in the empty question field.
editor-front-placeholder = 问题
# Grey text in the empty answer field.
editor-back-placeholder = 回答
# Spoken name of the question field's HTML box.
editor-front-source = 正面的 HTML 源代码
# Spoken name of the answer field's HTML box.
editor-back-source = 背面 HTML 源代码
# Hint under the answer on a typed-answer note.
editor-typed-hint = 在学习过程中，您将输入这个答案并看到差异。
# Hint under the fields.
editor-drop-hint = 将图像和音频直接拖放到字段中，或粘贴到字段里。
# Hint on a cloze note. The keys are shown as key caps.
editor-cloze-hint-html = 选择一个答案并按<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>将其清空。
# Hint on a reversed note.
editor-reversed-hint = 两张卡片：从前到后、从后到前。
# Hint: the shortcut that saves. The keys are shown as key caps.
editor-save-hint-html = <kbd>Ctrl</kbd>+<kbd>Enter</kbd> 保存。

## Colours, for the swatches' names (lowercase, as inside a sentence)

# The colour red.
hue-red = 红色的
# The colour amber.
hue-amber = 琥珀色
# The colour green.
hue-green = 绿色的
# The colour teal.
hue-teal = 青色
# The colour blue.
hue-blue = 蓝色的
# The colour purple.
hue-purple = 紫色的
# The colour pink.
hue-pink = 粉色的

## Why a card or a note was refused (the editor, the app, an assistant)

# A card's question side is empty.
card-error-front-empty = 卡正面为空
# A card's answer side is empty.
card-error-back-empty = 卡背面是空的
# A side is too long. $max is the limit in bytes.
card-error-side-long = 卡片一面超过 { $max } 字节
# A field of the editor is too long. $max is the limit in bytes of HTML.
card-error-field-long = 字段超过 HTML 的{ $max }字节
# A cloze note has no blank. "{{c1::…}}" is the cloze syntax, kept as it
# is.
card-error-no-cloze = 添加至少一项 {"{{"}c1::…{"}}"} 完形填空删除
# A cloze blank has no answer. $n is the blank's number.
card-error-cloze-empty = 完形填空 c{ $n }有一个空答案
# A cloze blank's card was refused. $n is the blank's number, $error why
# (one of the card-error-* above).
card-error-cloze-card = 完形填空 c{ $n }: { $error }
# The card shows an image or a sound that is not the person's. $id is
# its number.
card-error-media = 媒体{ $id }不是您上传的内容之一

## An image or a sound refused on upload

# The file is of a kind cards cannot show. $filename is its name.
media-error-format = { $filename }：不支持格式
# The file is empty. $filename is its name.
media-error-empty = { $filename } ：空文件
# The file is over the size limit. $filename is its name, $mb the limit
# in megabytes.
media-error-too-large = { $filename } ：超过{ $mb }MB 每个文件限制
# The file's content is not what its name says it is. $filename is its
# name.
media-error-mismatch = { $filename }：内容与其扩展名不匹配
# The account holds as many media files as it may.
media-error-count = 已达到媒体文件限制。先删除一些文件
# The account's media take all the room it has.
media-error-storage = 已达到媒体存储限制。联系支持人员
