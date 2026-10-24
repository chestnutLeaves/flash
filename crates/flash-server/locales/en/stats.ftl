### The open core's words in English: the statistics page, and the
### server's list of user accounts (admins only). Every message has a
### note above it: where it shows, who reads it, and what each variable
### holds.

## Statistics

# Browser tab title of the statistics page.
stats-title = Stats - Flash
# Heading of the statistics page.
stats-heading = Stats
# Under a big number: reviews in the last 30 days.
stats-reviews-30 = reviews, last 30 days
# Under a big number: the share of reviews remembered, last 30 days.
stats-retention = retention, last 30 days
# Under a big number: every review ever.
stats-reviews-all = reviews, all time
# Under a big number: days in a row with reviews, up to today.
stats-streak = day streak
# Under a big number: the most days in a row with reviews.
stats-longest = longest streak
# Under a big number: cards well learned, out of those in study.
stats-mature = mature cards (of active)
# A percentage. $n is the number.
stats-percent = { $n }%
# Heading of the calendar of reviews: the last twelve months.
stats-past-year = Past year
# Spoken name of the buttons that pick the calendar's year.
stats-years = Heatmap year
# Tooltip of a day in the calendar: cards due that day. $day is the
# date, $count how many.
stats-cell-due = { $day }: { $count } due
# Tooltip of a day in the calendar: reviews done that day. $day is the
# date, $count how many reviews.
stats-cell-reviews =
    { $count ->
        [one] { $day }: { $count } review
       *[other] { $day }: { $count } reviews
    }
# Legend under the calendar: reviews per day on average. The number is
# shown apart. $count is that average, a whole number.
stats-legend-avg-html =
    { $count ->
        [one] Daily average: <span>{ $count } review</span>
       *[other] Daily average: <span>{ $count } reviews</span>
    }
# Legend: the share of days with reviews. $pct is that share as a
# percentage.
stats-legend-learned-html = Days learned: <span>{ $pct }%</span>
# Legend: the longest run of days with reviews. $count is how many
# days.
stats-legend-longest-html =
    { $count ->
        [one] Longest streak: <span>{ $count } day</span>
       *[other] Longest streak: <span>{ $count } days</span>
    }
# Legend: the current run of days with reviews. $count is how many
# days.
stats-legend-current-html =
    { $count ->
        [one] Current streak: <span>{ $count } day</span>
       *[other] Current streak: <span>{ $count } days</span>
    }
# Heading of the bar chart of the last 30 days.
stats-per-day = Reviews per day
# Heading of the table of the coming week.
stats-coming-up = Coming up
# Column heading: the day.
stats-col-day = Day
# Column heading: cards due that day.
stats-col-due = Cards due
# The first row of that table: today.
stats-today = Today

## The server's accounts (admins only)

# Browser tab title of the accounts page.
users-title = Users - Flash
# Heading of the accounts page.
users-heading = Users
# Column heading: the person's name.
users-col-user = User
# Column heading: their address.
users-col-email = Email
# Column heading: when they made the account.
users-col-joined = Joined
# Column heading: how many decks they have.
users-col-decks = Decks
# Column heading: how many cards they have.
users-col-cards = Cards
# Column heading: how many reviews they have done.
users-col-reviews = Reviews
# Column heading: when they last studied.
users-col-studied = Last studied
# Column heading: when they last signed in.
users-col-login = Last login
# Column heading: which AI assistant (MCP client) they connected.
users-col-mcp = MCP
# Badge beside an admin's name.
users-admin = admin
