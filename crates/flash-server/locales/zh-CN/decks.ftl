### The open core's words in English: the home page (Today), the list of
### decks, a deck's page and its settings. A deck is a named set of
### flashcards; a subdeck is a deck inside another. Every message has a
### note above it: where it shows, who reads it, and what each variable
### holds.

## Deck tables (Today, the decks page, a deck's subdecks)

# Column heading: the deck's name.
decks-col-deck = 牌组
# Column heading: how many of its cards are due for review now.
decks-col-due = 待复习
# Column heading: how many new cards it offers today.
decks-col-new = 新卡
# Spoken name of the arrow that folds a deck's subdecks. $name is the
# deck's name.
decks-fold-label = { $name } 的子牌组
# Tooltip of that arrow while the subdecks are hidden.
decks-fold-show = 显示子牌组
# Tooltip of that arrow while the subdecks show.
decks-fold-hide = 隐藏子牌组
# Note after an archived deck's name.
decks-archived-note = （已存档）
# Small link on an archived deck's row that brings it back (lowercase,
# quiet).
decks-unarchive = 取消存档
# Small link on a deck's row that starts studying it (lowercase, quiet).
decks-study = 学习
# Column heading, shown when a deck has a goal date: a ring that fills as
# the deck is learned, and the time left (goal-left-*).
decks-col-goal = 目标
# Spoken name of the round "…" button at the end of a deck's row, which
# opens that deck's menu. $name is the deck's name.
decks-more-label = { $name } 的更多内容
# Tooltip of that button.
decks-more-title = 更多
# Menu item on a deck's row: opens the page of its settings.
decks-more-settings = 牌组设置
# Menu item on a deck's row: adds new cards to today's limit for it.
decks-more-new = 今天有更多新卡

## Today (the home page)

# Heading of the home page: what is due today.
today-heading = 今天
# Under a big number: cards due for review now.
today-due = 现在到期
# Under a big number: new cards available today.
today-new = 今天新
# Under a big number: cards reviewed today.
today-reviewed = 今日已复习
# Button that starts studying.
today-start = 开始学习
# Shown instead of the button when nothing is due.
today-caught-up = 已完成今天的学习，目前没有待复习内容。
# Before the buttons that add 5, 10 or 20 new cards to today's limit.
today-more-new = 今天更多新卡：
# The same when some were already added today. $count is how many.
today-more-new-boosted = 今天有更多新卡（+{ $count }已经）：
# Heading over the deck table.
today-decks = 牌组
# Shown when there are no decks, with links to make one or import one.
today-no-decks-html = 还没有牌组。 <a href="/decks">创建一个</a>或<a href="/import">导入</a>。
# Title of the box inviting the person to connect an AI assistant.
today-connect = 连接您的 AI
# Text of that box.
today-connect-sub = 制作卡片并与 Claude、ChatGPT、Grok、Mistral 或 Gemini 一起学习。一分钟设置。
# Spoken name and tooltip of the button that hides that box.
today-connect-dismiss = 不再显示

## The decks page

# Browser tab title of the decks page.
decks-title = 牌组 - Flash
# Heading of the decks page.
decks-heading = 牌组
# Link that shows archived decks in the list.
decks-show-archived = 显示已存档
# Link that hides them again.
decks-hide-archived = 隐藏已存档
# Link to the import page.
decks-import = 导入
# Placeholder of the field that names a new deck.
decks-new-name = 新牌组名称
# Button that makes the deck.
decks-create = 创建
# Shown when there are no decks.
decks-none = 还没有牌组。
# Shown when every deck is archived and archived decks are hidden.
decks-all-archived = 所有牌组均已存档。

## A deck's page

# Browser tab title of a deck's page. $name is the deck's full name.
deck-title = { $name } - Flash
# Spoken name of the trail of decks this one sits in.
deck-crumbs = 父级牌组
# Button that brings an archived deck back.
deck-unarchive = 取消归档
# Note on an archived deck's page.
deck-archived-hint = 此牌组已归档。卡片会保留，但不会出现在学习队列中。
# Label of the field for the card's question side.
deck-front = 正面
# Label of the field for the card's answer side.
deck-back = 背面
# Label of the tags field.
deck-tags = 标签
# Example tags in the empty tags field; keep them short study tags.
deck-tags-placeholder = 检查 2，心脏
# Hint under the tags field.
deck-tags-hint = 以逗号分隔，可选。
# Button that adds the card.
deck-add = 添加卡
# Button that opens the full editor (card types, languages, images).
deck-advanced = 高级
# Placeholder of the field that searches the deck's cards.
deck-search = 搜索此牌组...
# Heading of the panel listing the decks inside this one.
deck-subdecks-heading = 下层牌组
# Placeholder and spoken name of the field that names a new subdeck.
deck-new-subdeck = 新子牌组名称
# Button that makes the subdeck.
deck-add-subdeck = 添加子牌组
# Row under a deck's subdecks (and a menu item) that opens the field for
# a new subdeck.
deck-subnew-open = 新子牌组
# Button that closes the new-subdeck field without making one, and the
# selection bar's button that steps back from deleting.
deck-subnew-cancel = 取消

## A deck's page: its head, its subdecks, the search and filter, Add, and
## the bar of actions on the selected cards

# The word after the number of cards due for review, in the quiet line
# under the deck's name ("8 due"). $count is that number, shown before
# it in bold.
deck-meta-due = { $count ->
       *[other] 待复习
    }
# The word after the number of new cards today ("12 new"). $count is
# that number, shown before it.
deck-meta-new = { $count ->
       *[other] 新卡
    }
# The word after the deck's number of cards, its subdecks' included ("62
# cards"). $count is that number, shown before it.
deck-meta-cards = { $count ->
       *[other] 张卡片
    }
# Button that studies this deck; a number of due cards may follow it.
deck-study-short = 学习
# Spoken name of the button that opens the deck's menu (goal, export,
# settings, archive).
deck-more-label = 此牌组的更多内容
# Tooltip of that button.
deck-more-title = 更多
# Menu item that downloads the deck and its subdecks as an Anki package.
deck-export-apkg = 导出为 Anki 牌组 (.apkg)
# Menu item that downloads the deck's cards as a spreadsheet file.
deck-export-csv = 导出为电子表格 (.csv)
# Menu item that archives the deck.
deck-archive-deck = 归档牌组
# A subdeck row: how many cards it holds, its own subdecks' included.
# $count is that number.
deck-sub-cards = { $count ->
       *[other] { $count } 张卡片
    }
# Spoken name of a subdeck row's Study link. $name is the subdeck's name.
deck-sub-study-label = 学习 { $name }
# Row that shows the rest of the subdecks when there are many. $count is
# how many subdecks there are in all.
deck-subs-all = { $count ->
       *[other] 显示全部 { $count } 个
    }
# Row that opens the list of archived subdecks. $count is how many.
deck-subs-archived = { $count ->
       *[other] { $count } 个已归档子牌组
    }
# Placeholder of the field that searches the deck's cards. $count is how
# many cards the deck has, its subdecks' included.
deck-search-n = { $count ->
       *[other] 搜索 { $count } 张卡片…
    }
# Button that opens the filter: archived or not, which decks, which tags.
deck-filter = 筛选
# Spoken name of the filter's choice between active and archived cards.
deck-filter-show = 显示
# That choice: the cards in study.
deck-filter-active = 学习中
# That choice: the archived cards, out of study.
deck-filter-archived = 已存档
# Heading in the filter over the deck and its subdecks.
deck-filter-decks = 牌组
# Heading in the filter over the deck's tags.
deck-filter-tags = 标签
# Placeholder of the field that narrows the filter's list of tags.
deck-filter-find-tag = 寻找标签...
# Button that opens the form that adds a card.
deck-add-open = 添加
# Spoken name of the arrow beside Add that opens the other ways to add
# cards.
deck-add-more = 更多添加卡片的方式
# Menu item: write one card by hand.
deck-add-write = 写一张卡片
# Menu item: bring in a deck made in Anki.
deck-add-import = 导入 Anki 牌组
# Button that closes the form that adds a card.
deck-add-close = 关闭
# Spoken name of the bar of actions on the selected cards.
deck-bulk-label = 精选卡牌
# The bar's button that adds or removes tags on the selected cards.
deck-bulk-tags = 标签
# Label of the field in that menu.
deck-bulk-tags-label = 添加或删除标签
# Button that adds the tags written in the field.
deck-bulk-tag-add = 添加
# Button that removes the tags written in the field.
deck-bulk-tag-remove = 移除
# The bar's button that moves the selected cards to another deck, and
# the button in its menu that does it.
deck-bulk-move = 移动
# Label of the list of decks to move them to.
deck-bulk-move-label = 移至牌组
# The bar's button that archives the selected cards.
deck-bulk-archive = 归档
# The bar's button that brings archived cards back into study.
deck-bulk-unarchive = 取消归档
# The bar's button that deletes the selected cards (it asks first).
deck-bulk-delete = 删除
# Spoken name and tooltip of the bar's button that clears the selection.
deck-bulk-clear = 清除选择 (Esc)
# Shown in the bar before deleting the selected cards.
deck-bulk-delete-warn = 永久删除它们吗？这无法撤消。

## A deck's settings

# Browser tab title. $name is the deck's full name.
deck-settings-title = { $name } 设置 - Flash
# Line under the deck's name, naming the page.
deck-settings-label = 牌组设置
# Link back to the deck's page.
deck-settings-back = 回到牌组
# Heading of the card that renames the deck.
deck-name-heading = 名称
# Spoken name of the deck name field.
deck-name-label = 牌组名称
# Button that renames the deck.
deck-rename = 重命名
# Brief note after a change was saved.
deck-saved = 已保存
# Heading of the card that moves the deck inside another.
deck-parent-heading = 父级牌组
# Explains the parent deck. $count is how many subdecks this deck has
# (they move with it).
deck-parent-hint =
    { $count ->
        [0] 所在的上级牌组。学习牌组时会包含其子牌组。
       *[other] 所在的上级牌组。学习牌组时会包含其子牌组，该牌组的 { $count } 个子牌组也会随之移动。
    }
# Spoken name of the parent deck picker.
deck-parent-label = 父级牌组
# The picker's choice for no parent at all.
deck-top-level = （顶层）
# Button that moves the deck.
deck-move = 移动
# Brief note after the deck moved.
deck-moved = 已移动
# Heading of the card with the daily limits.
deck-limits-heading = 每日限额
# Label: the most new cards to show per day.
deck-limit-new = 新卡/天
# Label: the most reviews per day.
deck-limit-reviews = 每天最多复习数
# Placeholder of a limit field left to the account's setting. $count is
# that setting.
deck-limit-default = 账户默认（{ $count }）
# Button that saves the limits.
deck-save-limits = 保存限制
# Badge: new cards added to today's limit. $count is how many.
deck-boosted = + 今天 { $count }
# Heading of the card for an archived deck.
deck-archived-heading = 已归档牌组
# Explains archiving, on an archived deck. $count is how many subdecks
# come back with it.
deck-archived-explain =
    { $count ->
        [0] 此牌组已从学习队列和待复习数量中移除。取消归档后，卡片会带着原有复习历史恢复。
       *[other] 此牌组已从学习队列和待复习数量中移除。取消归档后，卡片及其 { $count } 个子牌组会带着原有复习历史恢复。
    }
# Added after that when the deck sits inside others.
deck-archived-parents = 它所在的牌组并没有随之归档。
# Brief note after the deck was archived.
deck-archived-toast = 已存档
# Heading of the card that archives the deck.
deck-archive-heading = 归档牌组
# Explains archiving. $count is how many subdecks go with it.
deck-archive-explain =
    { $count ->
        [0] 保留所有卡片、媒体和复习历史，并将牌组移出学习队列和待复习统计。你可以随时取消归档。
       *[other] 保留所有卡片、媒体和复习历史，并将牌组及其 { $count } 个子牌组移出学习队列和待复习统计。你可以随时取消归档。
    }
# Button that archives the deck.
deck-archive = 归档
# Brief note after the deck was brought back.
deck-restored = 已恢复
# Heading of the card that deletes the deck.
deck-delete-heading = 删除牌组
# Explains deleting. $count is how many subdecks go with it.
deck-delete-explain =
    { $count ->
        [0] 删除此牌组及其中的所有卡片。复习历史会保留，且无法撤销。
       *[other] 删除此牌组、其 { $count } 个子牌组及其中的所有卡片。复习历史会保留，且无法撤销。
    }
# The browser's confirm question before deleting. $subdecks is how many
# subdecks go with it, $cards how many cards.
deck-delete-confirm =
    { $subdecks ->
        [0] { $cards ->
           *[other] 删除此牌组及全部 { $cards } 张卡片？此操作无法撤销。
        }
       *[other] { $cards ->
           *[other] 删除此牌组、{ $subdecks } 个子牌组及全部 { $cards } 张卡片？此操作无法撤销。
        }
    }
# Button that deletes the deck.
deck-delete = 删除牌组

## Why a card was not added (a banner over the add form)

# The cloze note had no deletion. "{{c1::…}}" is the cloze syntax, shown
# as is; Ctrl+Shift+C is the keyboard shortcut that makes one.
card-not-added-cloze = 该卡未添加：完形填空注释需要至少删除一个 {"{{"}c1::…{"}}"} 。选择答案并按 Ctrl+Shift+C。
# A side was left empty.
card-not-added-empty = 该卡未添加：双方都需要一些内容。
# A side was over the length limit.
card-not-added-long = 该卡未添加：一侧太长。
# The card pointed at an image or sound of another account.
card-not-added-media = 该卡未添加：它引用了不属于您的媒体。
# Any other refusal.
card-not-added-invalid = 该卡未添加：无法保存内容。
# The account is at its card limit.
card-not-added-limit = 未添加该卡：账户已达到卡片上限。

## Why a deck could not be renamed or moved (a badge beside the heading)

# The name held "::", which writes a deck path; the Parent deck setting
# nests instead (deck-parent-heading).
deck-error-nest = 名称不能包含“::”；使用父牌组嵌套该牌组
# The name was empty.
deck-error-empty = 名称不能为空
# Another deck has the name.
deck-error-taken = 你已经有一个同名的牌组了
# The deck was moved into itself or one of its subdecks.
deck-error-self = 牌组无法自行移动
# The full path ("Parent::Child") would be over the length limit.
deck-error-long = 这会使牌组的全名太长
# The chosen parent deck is archived.
deck-error-archived = 该牌组已存档；首先将其取消存档
# The account is at its deck limit.
deck-error-cap = 账户已达到牌组数量上限
# Any other failure.
deck-error-invalid = 操作无效

## Refusals naming or making a deck (a sentence, shown by the app and
## assistants too)

# A name held "::", which writes a deck path; "Parent deck" is the
# setting that nests a deck (deck-parent-heading).
deck-error-nest-sentence = 名称不能包含“::”。使用父牌组嵌套该牌组。
# A new deck was asked for with no name.
deck-error-name-new = 为新牌组命名。
# The deck named is archived. $name is its name.
deck-error-archived-sentence = 您的牌组“{ $name }”已存档。取消存档，或选择其他名称。

## A deck's goal date: the day the learner wants to know the deck by (an
## exam, a trip, a deadline). Flash paces the deck's new cards to finish
## before it and brings back, in its last days, the cards that need it.

# Refused goal date: today or a day already past.
goal-error-past = 选择今天之后的日期。
# Refused goal date: more than five years ahead.
goal-error-far = 选择未来五年内的日期。

# Menu item on a deck's row without a goal date: opens the dialog that
# sets one.
goal-set = 设定目标日期...
# Menu item on a deck's row with a goal date: opens the dialog to change
# it.
goal-change = 更改目标日期...
# Menu item on a deck's row with a goal date: removes it.
goal-remove = 删除目标日期

# The time left in the Goal column, up to two months away: days, very
# short ("12d"). $count is the number of days.
goal-left-days = 剩余 { $count } 天
# The time left on the goal date itself.
goal-left-today = 今天
# In the Goal column once the goal date has passed.
goal-left-done = 完毕
# The time left, months away in another year: the month and the year
# ("Oct 2027"). $month is the month, short; $year the year in figures.
goal-left-month-year = { $month } { $year }

# Tooltip of the goal marker, first line, when the deck keeps to its
# plan. $date is the goal date ("Oct 28, 2026").
goal-tip-on-track = 有望实现 { $date }
# The same when the deck has fallen behind its plan. $date is the goal
# date ("Oct 28, 2026").
goal-tip-behind = 后面为{ $date }
# Tooltip line: how much of the deck is learned. $learned and $total are
# card counts.
goal-tip-learned =
    { $total ->
       *[other] 已学习 { $learned } / { $total } 张卡片。
    }
# Tooltip line: the plan. $count is new cards a day; $date the last day
# new cards come in; after it, the last reviews before the goal.
goal-tip-pace =
    { $count ->
       *[other] 每天新增 { $count } 张，直到 { $date }，之后进行最后一次复习。
    }
# Tooltip line in the last days before the goal, when no new cards come.
# $date is the goal date.
goal-tip-final = { $date } 之前的最后复习。
# Tooltip line when the deck is behind. $count is how many new cards
# behind it is.
goal-tip-catch-up =
    { $count ->
       *[other] 今天再新增 { $count } 张即可回到计划进度。
    }
# Tooltip on the goal date itself.
goal-tip-today = 你的目标是今天。祝你好运！
# Tooltip once the goal date has passed. $date is the date.
goal-tip-passed = 您的目标日期是{ $date }。
# Under the name of a deck whose goal date has passed, before its two
# buttons (goal-archive, goal-keep).
goal-passed = 你的目标日期已经过去了。
# Button: archives the deck whose goal date passed.
goal-archive = 归档牌组
# Button: removes the passed goal date and keeps studying the deck as
# any other.
goal-keep = 继续学习

# Heading of the dialog (and of the deck settings card) that sets a goal
# date, when the deck has none.
goal-heading-set = 设定目标日期
# The same when the deck has one.
goal-heading-change = 更改目标日期
# Heading of the card on a deck's settings page.
goal-card-heading = 目标日期
# Label of the date field in that dialog.
goal-date-label = 你什么时候需要知道它？
# Spoken name of the row of quick picks under the date field.
goal-picks = 快速选择
# Quick pick: a date some weeks from today. $count is the weeks.
goal-pick-weeks =
    { $count ->
       *[other] { $count } 周
    }
# Quick pick: a date some months from today. $count is the months.
goal-pick-months =
    { $count ->
       *[other] { $count } 个月
    }
# Quick pick: a date a year from today.
goal-pick-year = 1年
# The plan, first line: how many new cards a day and until when. $count
# is new cards a day; $date the last day they come in; $unseen how many
# of the deck's cards were never studied. The bold part is the pace.
goal-plan-pace-html =
    { $count ->
       *[other] 直到 { $date } 每天新增 <b>{ $count } 张卡片</b>（还有 { $unseen } 张未见过）
    }
# The plan, first line, when every card was already studied once.
goal-plan-seen-html = <b>每张卡片都已学习。</b>在此之前通过复习保持记忆。
# The plan, second line: the last days before the goal, kept for review.
# $from is the first of those days, $to the goal date.
goal-plan-final-html = <b>{ $from } 至 { $to }：</b> 复习所有需要复习的卡片
# The same when the goal is too close for days of their own. $to is the
# goal date.
goal-plan-final-close-html = <b>截至 { $to }：</b> 复习所有需要复习的卡片
# The plan, third line: after the goal. $date is the goal date.
goal-plan-after-html = <b>{ $date } 之后，</b>复习恢复正常。你可以随后归档该牌组。
# Shown where the plan goes before a date is picked.
goal-plan-pick = 选择一个日期来查看计划。
# Warning under the plan when the date asks for many new cards a day.
# $count is how many.
goal-tight = 每天大约需要学习 { $count } 张新卡。选择更晚的日期可以减轻每天的负担。
# Button that saves the goal date.
goal-save = 保存目标
# Button that closes the dialog without saving.
goal-cancel = 取消
