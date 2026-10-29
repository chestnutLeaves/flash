### The open core's words in English: the study page, where a person
### sees a card, shows its answer and grades how well they remembered it.
### Every message has a note above it: where it shows, who reads it, and
### what each variable holds.

## The page

# Browser tab title of the study page.
study-title = 学习 - Flash
# Heading of the study page (the activity, not an order).
study-heading = 学习

## Studying by tags (a dialog that narrows the session to some tags)

# Link beside the heading that opens the tag picker, with no tag picked.
study-tags = 标签
# The same link with one or two tags picked. $tags is the picked tags,
# comma-separated.
study-tags-some = 标签： { $tags }
# The same link with more than two picked: the first two, then how many
# more. $first and $second are the first two tags' names, $more how
# many other tags are picked.
study-tags-more = 标签：{ $first },{ $second }+ { $more }
# Heading of the tag picker.
study-tags-heading = 按标签学习
# Text in the picker when the deck being studied has no tags.
study-tags-none-deck = 此牌组中还没有标签。添加或编辑卡片时会设置标签。
# Text in the picker when no card has tags.
study-tags-none = 还没有标签。添加或编辑卡片时会设置标签。
# Placeholder and spoken name of the field that filters the tag list.
study-tags-filter = 过滤标签
# Shown when the filter matches no tag.
study-tags-no-match = 没有标签匹配。
# How picking several tags works.
study-tags-every = 仅当卡片带有每个选取的标签时，该卡片才处于会话中。
# Button that unticks every tag.
study-tags-clear = 清除
# Button that starts a session with the picked tags.
study-tags-apply = 应用

## The card

# Spoken name of the "…" button on the card's corner.
study-more-label = 此卡的更多信息
# Tooltip of that button.
study-more-title = 更多
# Menu item: edit the card on screen.
study-edit-card = 编辑卡
# Menu item: take the card out of study (it stays in its deck).
study-archive-card = 归档卡片
# Placeholder of the field where the answer is typed, on cards that ask
# for it.
study-type-answer = 输入答案...
# Button that shows the card's answer.
study-show-answer = 显示答案
# The key that shows the answer on cards with a typed answer, as its key
# cap reads (lowercase).
study-key-enter = Enter
# The key that shows the answer on other cards, as its key cap reads
# (lowercase).
study-key-space = 空格
# How many cards are left in the session. $count is the number.
study-left = { $count } 左

## The four grades, from forgetting to knowing it cold. Anki's own words
## for them; keep them as the person's flashcard app says them.

# Grade: forgot it; the card comes back soon.
study-grade-again = 再次
# Grade: remembered it, with effort.
study-grade-hard = 困难
# Grade: remembered it.
study-grade-good = 良好
# Grade: knew it at once.
study-grade-easy = 容易

## When a card comes back after each grade, under the grade buttons:
## very short, as a button has room for.

# $count minutes.
interval-minutes = { $count } 分钟
# $count hours.
interval-hours = { $count } 小时
# $count days.
interval-days = { $count } 天
# $count months.
interval-months = { $count } 个月
# $count years; may have one decimal (1.5).
interval-years = { $count } 年

## A finished session

# Heading when the day's limit of new cards was reached.
study-done-limit-heading = 今日新卡已用完
# Heading when the session ended with cards reviewed.
study-done-heading = 会话完成
# Heading when nothing was due.
study-done-caught-up = 已完成今日复习
# Line under the limit heading. $count is how many new cards wait for
# tomorrow; the number is shown bold.
study-done-held-html =
    { $count ->
       *[other] 今日限制后还有 <strong>{ $count }</strong> 张卡片等待。
    }
# Added after the line above. $count is how many cards were reviewed.
study-done-held-reviewed = 已复习 { $count } 张卡片。
# The same, with $took how long it took (study-took-*). $count is how
# many cards were reviewed.
study-done-held-reviewed-took = 已复习 { $count } 张卡片，{ $took }。
# Line under the "Session done" heading. $count is how many cards were
# reviewed.
study-done-reviewed =
    { $count ->
       *[other] 已复习 { $count } 张卡片
    }
# The same, with $took how long it took (study-took-*). $count is how
# many cards were reviewed.
study-done-reviewed-took =
    { $count ->
       *[other] 已复习 { $count } 张卡片 { $took }
    }
# How long the session took, when under a minute.
study-took-under-minute = 一分钟内
# How long the session took. $count is the minutes.
study-took-minutes =
    { $count ->
       *[other] 用时 { $count } 分钟
    }
# Line under the "All caught up" heading.
study-done-nothing-due = 目前没有待复习内容。
# A tinted count of the session's grades: $count cards graded Good
# (study-grade-good, in lowercase).
study-pill-good = { $count } 好
# The same for Easy. $count is how many cards were graded Easy.
study-pill-easy = { $count } 简单
# The same for Hard. $count is how many cards were graded Hard.
study-pill-hard = { $count } 困难
# The same for Again. $count is how many cards were graded Again.
study-pill-again = 再来 { $count }
# Before the buttons that add 5, 10 or 20 new cards to today's limit.
study-more-new = 今天更多新卡：
# Spoken name of the small bar chart of the week ahead.
study-week = 下周到期的卡
# When the next cards come: none are scheduled.
study-nothing-scheduled = 还没有安排
# Label and spoken name of the switch that turns on reminder emails.
study-remind = 卡片到期时给我发送电子邮件
# Beside that switch on a server that sends no reminder emails: where
# they would be turned on.
study-remind-elsewhere = 您可以在“设置”中打开提醒。
# Button back to the home page.
study-back-today = 回到今天
# The goal date a finished session counts toward, when the deck keeps
# to its plan. $date is the goal date ("Oct 28").
study-goal-on-track = 有望实现 { $date }
# The same when the deck has fallen behind its plan. $count is how many
# new cards behind; $date the goal date.
study-goal-behind =
    { $count ->
       *[other] 截至 { $date } 还有 { $count } 张卡片落后于计划
    }
# Under that line: how much of the deck is learned. $learned and $total
# are card counts; $deck is the deck's name.
study-goal-learned =
    { $total ->
       *[other] 在 { $deck } 中已掌握 { $learned } / { $total } 张卡片。
    }
# After study-goal-learned: the days until the goal. $count is the days.
study-goal-days-left =
    { $count ->
       *[other] 还剩 { $count } 天。
    }
# After study-goal-learned, when behind: how to catch up. $count is how
# many new cards.
study-goal-catch-up =
    { $count ->
       *[other] 今天再学习 { $count } 张即可回到计划进度。
    }
# Button that adds that many new cards to today's limit and goes on
# studying. $count is how many.
study-goal-study-more = 再学习 { $count } 张
# How much of the deck is learned, beside the ring. $count is the share,
# out of 100.
study-goal-percent = { $count } %
# Shown when the place being studied has no cards at all.
study-empty = 这里还没有卡。
# Button to the list of decks.
study-go-decks = 前往你的牌组
# Shown after a card was archived from the study page.
study-archived = 卡片已归档。它仍保留在牌组中，但不会进入学习队列。
# Button that puts the archived card back.
study-undo = 撤消

## When the next cards come, after a session ran out. $count is how many
## cards; 0 means an unknown number ("Cards").

# Later the same day. $count is how many cards (0 when unknown), $time
# the time of day.
# spare: $hour, the hour $time shows (3 in "3:40 PM"), for a language whose words agree with it.
study-next-later-today =
    { $count ->
        [0] 今天稍后约 { $time } 会有更多卡片
       *[other] 今天稍后约 { $time } 会有 { $count } 张卡片
    }
# Tomorrow. $count is how many cards (0 when unknown).
study-next-tomorrow =
    { $count ->
        [0] 明天待复习的卡片
       *[other] 明天有 { $count } 张卡片待复习
    }
# Within the week. $count is how many cards (0 when unknown), $day the
# weekday's name.
study-next-weekday =
    { $count ->
        [0] { $day } 待复习的卡片
       *[other] { $day } 有 { $count } 张卡片待复习
    }
# Later. $count is how many cards (0 when unknown), $days the number of
# days, 7 or more.
study-next-in-days =
    { $count ->
        [0] { $days } 天后待复习的卡片
       *[other] { $days } 天后有 { $count } 张卡片待复习
    }

## Refusals while studying

# A quick edit would delete the card on screen (a cloze blank removed).
# "Advanced edit" is the full editor.
study-edit-removes-card = 此编辑会删除你正在学习的卡片。请保留空白，或使用高级编辑。

custom-study-title = 刷题
custom-study-new = 新建刷题
custom-study-active = 进行中
custom-study-completed = 已结束
custom-study-active-empty = 暂无进行中的刷题，可以新建一轮。
custom-study-completed-empty = 还没有已结束的刷题记录。
custom-study-continue = 继续
custom-study-view = 查看
custom-study-default-count = 默认使用当前范围的全部可选卡片
custom-study-mode = 学习方式
custom-study-review = 主动复习
custom-study-review-hint = 根据评分更新复习记录和下次复习时间。
custom-study-practice = 自由练习
custom-study-practice-hint = 不改变 FSRS 状态、正式复习记录或到期时间。
custom-study-deck = 牌组
custom-study-all-decks = 全部牌组
custom-study-tags = 标签
custom-study-tags-hint = 多个标签用逗号分隔，留空表示全部
custom-study-count = 卡片数量
custom-study-all = 全部
custom-study-order = 出题顺序
custom-study-random = 随机
custom-study-ordered = 按创建顺序
custom-study-scope-hint = 包含未到期卡片及子牌组，排除已归档和暂停学习的卡片。
custom-study-cancel = 取消
custom-study-start = 开始刷题
custom-study-next = 下一题
custom-study-exit = 退出
custom-study-back = 返回刷题
custom-study-empty = 所选范围没有可学习的卡片
custom-study-finished = 本轮学习已完成
custom-study-resume = 继续上一次自选学习
custom-study-count-hint = 输入 0 表示全部卡片
