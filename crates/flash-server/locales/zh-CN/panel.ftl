### The open core's words in English for the study panel: the view an
### AI assistant shows inside the conversation, to study cards and to
### page through cards just made or edited
### (panel/panel.js, through static/i18n.js). Every message has a note
### above it: where it shows, and what each variable holds. A panel
### message keeps to text, variables and at most one choice. The grade
### buttons' times are the study page's (interval-*).

## The header

# Tooltip of the button that opens a card's menu.
panel-more = 更多
# Spoken name of that button.
panel-more-label = 此卡的更多信息
# Menu item: edit the card on screen.
panel-edit-card = 编辑卡
# Menu item: archive the card on screen (it stays in its deck, never
# studied).
panel-archive-card = 归档卡片
# The button that shows a long card full screen.
panel-expand = 展开
# Its tooltip.
panel-expand-title = 全屏显示此卡
# Its spoken name.
panel-expand-label = 展开：全屏显示此卡
# Before the deck's name, when the session is limited to tags; a chip
# per tag follows.
panel-tags = 标签：

## Studying

# Shown while the panel waits for its first cards.
panel-loading = 正在加载您的卡片...
# The button under a card cut short that shows it whole.
panel-show-all = 显示全部
# The button that turns the card (study-show-answer).
panel-show-answer = 显示答案
# Its tooltip: the key that does the same (study-key-space).
panel-key-space = 空格
# The grade buttons (study-grade-*).
panel-again = 再次
# The second grade button.
panel-hard = 困难
# The third grade button.
panel-good = 良好
# The fourth grade button.
panel-easy = 容易
# A grade button's tooltip: the key that does the same. $key is 1 to 4.
panel-key = 按键 { $key }
# How many cards are left in the session, beside the progress bar.
# $count is that number.
panel-to-go = 还剩 { $count } 张
# Where the count was, right after a card is archived.
panel-archived = 卡已存档
# The link beside it that brings the card back.
panel-undo = 撤消

## Editing a card in the panel

# Label of the first field (editor-front).
panel-front = 正面
# Label of the second field (editor-back).
panel-back = 背面
# The link that opens the card in the site's full editor.
panel-advanced-edit = 高级编辑
# The button that saves the edit.
panel-save = 保存
# The button that closes the editor without saving.
panel-cancel = 取消
# A save failed, and the server gave no sentence of its own.
panel-save-failed = 无法保存该卡。再试一次。

## A finished session

# Heading when the day's limit of new cards was reached.
panel-held-title = 今日新卡已用完
# Under it: how many new cards wait. $count is at least 1.
panel-held-text =
    { $count ->
       *[other] 今日限制后还有 { $count } 张卡片等待。
    }
# The same, then what was done in this session: $held is
# panel-held-text, $summary is panel-reviewed-in or
# panel-reviewed-quick.
panel-held-text-summary = { $held }{ $summary }。
# The button that lets more new cards through today. $count is how many
# (at most 10).
panel-study-more = 再学习 { $count } 张
# Heading when the deck or tags hold no cards at all.
panel-empty-title = 这里还没有卡
# Under it.
panel-empty-text = 让助手从你的笔记、文档或主题中制作卡片，然后在这里学习。
# The button under it, which asks the assistant for cards.
panel-make = 制作卡片
# Heading when the session ended after grading cards.
panel-done-title = 会话完成
# Heading when nothing was due from the start.
panel-caught-up = 已完成今日复习
# Under it.
panel-nothing-due = 目前没有待复习内容。
# How many cards were graded in the panel ($count, at least 1); the
# start of panel-reviewed-in and panel-reviewed-quick.
panel-cards-reviewed =
    { $count ->
       *[other] 已复习 { $count } 张卡片
    }
# How long the session took. $reviewed is panel-cards-reviewed, $count
# whole minutes, at least 1.
panel-reviewed-in =
    { $count ->
       *[other] 用时 { $count } 分钟完成 { $reviewed }
    }
# The same for a session shorter than a minute. $reviewed is
# panel-cards-reviewed.
panel-reviewed-quick = 一分钟内 { $reviewed }
# The count of each grade given, as small pills. $count is how many.
panel-score-again = 再次{ $count }
# The same for Hard. $count is how many cards were graded Hard.
panel-score-hard = { $count } 困难
# The same for Good. $count is how many cards were graded Good.
panel-score-good = { $count } 好
# The same for Easy. $count is how many cards were graded Easy.
panel-score-easy = { $count } 简单
# When the next cards come, later today (an account on exact times).
# $count is how many (0 when unknown), $time the hour ("3:40 PM").
# spare: $hour, the hour $time shows (3 in "3:40 PM"), for a language whose words agree with it.
panel-next-today =
    { $count ->
        [0] 今天稍后约 { $time } 会有更多卡片
       *[other] 今天稍后约 { $time } 会有 { $count } 张卡片
    }
# The same, tomorrow. $count is how many (0 when unknown).
panel-next-tomorrow =
    { $count ->
        [0] 明天待复习的卡片
       *[other] 明天有 { $count } 张卡片待复习
    }
# The same, later this week. $count is how many (0 when unknown), $day
# the weekday's name ("Thursday").
panel-next-weekday =
    { $count ->
        [0] { $day } 待复习的卡片
       *[other] { $day } 有 { $count } 张卡片待复习
    }
# The same, a week or more away. $count is how many cards (0 when
# unknown), $days how many days.
panel-next-days =
    { $count ->
        [0] { $days } 天后待复习的卡片
       *[other] { $days } 天后有 { $count } 张卡片待复习
    }
# When no card is scheduled.
panel-nothing-scheduled = 还没有安排
# The button that turns on reminder emails.
panel-remind = 提醒我
# Its tooltip.
panel-remind-title = 卡片到期时给我发送电子邮件
# Shown once reminders are on.
panel-reminded = 提醒设置
# Shown when the panel could not turn reminders on.
panel-remind-fallback = 您可以在 Flash 设置中打开提醒。
# The link to the site's record of every review.
panel-progress = 查看您的进步
# What the Make cards button asks the assistant, as the person's own
# message in the chat. $deck is the deck's name.
panel-ask-deck = 帮我制作{ $deck }的抽认卡。
# The same for tags. $tags is the tags, separated by commas.
panel-ask-tags = 帮我制作标记为{ $tags }的抽认卡。
# The same for an account with no cards.
panel-ask-first = 帮我制作我的第一张抽认卡。

## The cards just made or edited

# Beside the deck's name: how many cards were made. $count is that
# number.
panel-new-cards =
    { $count ->
       *[other] { $count } 张新卡
    }
# The same for an edit: how many cards were changed. $count is that
# number.
panel-edited =
    { $count ->
       *[other] 已编辑 { $count } 张
    }
# Above a side's old text in an edit.
panel-before = 前
# Spoken name of the arrow to the previous card.
panel-previous = 上一张卡
# Spoken name of the arrow to the next card.
panel-next = 下一张卡
# Which card is shown. $at is its place, $count how many.
panel-position = { $at } { $count }
# The button that starts studying the deck the cards went into.
panel-study-now = 现在学习
# The button that opens the deck on the site.
panel-see-all = 查看全部

## Trouble

# The server did not answer.
panel-no-answer = Flash 没有返回答案。
# The server did not answer in time.
panel-no-answer-late = Flash 未及时返回答案。
# The button that tries the last action again.
panel-try-again = 再试一次
# Shown when the chat app cannot show the panel.
panel-needs-host = 该面板需要一个显示 MCP 应用程序的主机。
