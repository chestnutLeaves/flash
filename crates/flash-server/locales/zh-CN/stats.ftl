### The open core's words in English: the statistics page, and the
### server's list of user accounts (admins only). Every message has a
### note above it: where it shows, who reads it, and what each variable
### holds.

## Statistics

# Browser tab title of the statistics page.
stats-title = 统计 - Flash
# Heading of the statistics page.
stats-heading = 统计
# Under a big number: reviews in the last 30 days.
stats-reviews-30 = 最近 30 天复习次数
# Under a big number: the share of reviews remembered, last 30 days.
stats-retention = 过去 30 天的保留率
# Under a big number: every review ever.
stats-reviews-all = 总复习次数
# Under a big number: days in a row with reviews, up to today.
stats-streak = 连续日
# Under a big number: the most days in a row with reviews.
stats-longest = 最长连胜
# Under a big number: cards well learned, out of those in study.
stats-mature = 成熟卡（活跃）
# A percentage. $n is the number.
stats-percent = { $n } %
# Heading of the calendar of reviews: the last twelve months.
stats-past-year = 去年
# Spoken name of the buttons that pick the calendar's year.
stats-years = 热图年份
# Tooltip of a day in the calendar: cards due that day. $day is the
# date, $count how many.
stats-cell-due = { $day } :{ $count }到期
# Tooltip of a day in the calendar: reviews done that day. $day is the
# date, $count how many reviews.
stats-cell-reviews =
    { $count ->
       *[other] { $day }：{ $count } 次复习
    }
# Legend under the calendar: reviews per day on average. The number is
# shown apart. $count is that average, a whole number.
stats-legend-avg-html =
    { $count ->
       *[other] 每日平均：<span>{ $count } 次复习</span>
    }
# Legend: the share of days with reviews. $pct is that share as a
# percentage.
stats-legend-learned-html = 学习天数：<span>{ $pct }% </span>
# Legend: the longest run of days with reviews. $count is how many
# days.
stats-legend-longest-html =
    { $count ->
       *[other] 最长连续学习：<span>{ $count } 天</span>
    }
# Legend: the current run of days with reviews. $count is how many
# days.
stats-legend-current-html =
    { $count ->
       *[other] 当前连续学习：<span>{ $count } 天</span>
    }
# Heading of the bar chart of the last 30 days.
stats-per-day = 每日复习
# Heading of the table of the coming week.
stats-coming-up = 即将推出
# Column heading: the day.
stats-col-day = 天
# Column heading: cards due that day.
stats-col-due = 到期卡
# The first row of that table: today.
stats-today = 今天

## The server's accounts (admins only)

# Browser tab title of the accounts page.
users-title = 用户 - Flash
# Heading of the accounts page.
users-heading = 用户
# Column heading: the person's name.
users-col-user = 用户
# Column heading: their address.
users-col-email = 电子邮件
# Column heading: when they made the account.
users-col-joined = 已加入
# Column heading: how many decks they have.
users-col-decks = 牌组
# Column heading: how many cards they have.
users-col-cards = 卡
# Column heading: how many reviews they have done.
users-col-reviews = 复习
# Column heading: when they last studied.
users-col-studied = 最后学习
# Column heading: when they last signed in.
users-col-login = 最后登录
# Column heading: which AI assistant (MCP client) they connected.
users-col-mcp = MCP
# Badge beside an admin's name.
users-admin = admin
