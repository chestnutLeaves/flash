### The open core's words in English: a deck's preview, where a person
### goes through every card of a deck once and marks each one missed or
### got, without the marks reaching spaced repetition: nothing is
### scheduled and nothing is recorded. Every message has a note above it:
### where it shows, who reads it, and what each variable holds.

## The page

# Browser tab title of the preview page.
preview-title = 预览 - Flash
# Heading of the preview page (the activity, not an order).
preview-heading = 预览
# A tag beside the heading: going through the cards here leaves their
# spaced-repetition schedule exactly as it was.
preview-unaffected = 间隔重复不受影响
# Link beside the heading that stops the preview early and shows the
# score so far.
preview-end = 结束
# Shown instead of cards when the deck has none to go through.
preview-empty = 该牌组没有可预览的卡片。
# Button under that, back to the deck's page. $name is the deck's name.
preview-go-deck = 返回{ $name }
# Button after the answer shows: the person did not remember the card.
preview-missed = 未记住
# Button after the answer shows: the person remembered the card.
preview-got = 记住

## The end of a preview

# First line under the score.
preview-unaffected-line = 预览不会影响间隔重复。
# Added after that line when the deck has cards to study now. $count is
# how many.
preview-ready =
    { $count ->
       *[other] 现在有 { $count } 张卡片可以学习。
    }
# Added after that line otherwise, when the next cards come later today
# (an account that keeps exact times). $count is how many; 0 when the
# count is not known.
preview-next-later-today =
    { $count ->
        [0] 下一批复习在今天稍后。
       *[other] 下一批复习在今天稍后：{ $count } 张卡片。
    }
# The same, when they come tomorrow. $count is how many; 0 when the
# count is not known.
preview-next-tomorrow =
    { $count ->
        [0] 下一批复习在明天。
       *[other] 下一批复习在明天：{ $count } 张卡片。
    }
# The same, within the week. $count is how many (0 when not known); $day
# is the weekday's whole name ("Thursday").
preview-next-weekday =
    { $count ->
        [0] 下一批复习在 { $day }。
       *[other] 下一批复习在 { $day }：{ $count } 张卡片。
    }
# The same, further off. $count is how many (0 when not known); $days is
# how many days away (7 or more).
preview-next-in-days =
    { $count ->
        [0] 下一批复习在 { $days } 天后。
       *[other] 下一批复习在 { $days } 天后：{ $count } 张卡片。
    }
# Button that goes through the whole deck again.
preview-start-over = 重新开始

## The deck's menu (the ⋯ beside a deck on Today and Decks)

# Menu item that opens the deck's preview. $count is how many cards it
# goes through (at least 1).
preview-menu =
    { $count ->
       *[other] 预览全部 { $count } 张卡片
    }
# Second line of that menu item, smaller.
preview-menu-sub = 间隔重复不受影响

## Words the page's script says

# Heading of the end. $got is how many the person marked "Got it", $total
# how many they marked at all.
js-preview-score = 您获得了{ $got }或 { $total }
# The share of "Got it", beside the score's bar. $percent is a whole
# number from 0 to 100.
js-preview-percent = { $percent } %
# Button that goes through only the cards marked "Missed" again. $count is
# how many.
js-preview-review-missed =
    { $count ->
       *[other] 重新复习错过的 { $count } 张卡片
    }
