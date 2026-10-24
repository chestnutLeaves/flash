use flash_core::queue::{order_queue, StudyScope};
use flash_core::{CardId, DeckId, SessionId, UserId, UserSettings};
use flash_store::SessionStats;
use jiff::{tz::TimeZone, Timestamp, Zoned};

use crate::i18n::Words;
use crate::service::goals::in_subtree;
use crate::service::{
    check_scope, Dashboard, DayAhead, DayCount, FinishedSession, HeatCell, MonthSpan, NextDue,
    QueueCounts, Result, Services, StatsData,
};
use crate::state::HeavyPermit;

impl Services {
    /// Today's numbers and the decks behind them. Archived decks are not
    /// part of today: the web dashboard and the app's Today screen both
    /// read this, and neither lists them.
    pub fn dashboard(&self, user: UserId, now_ms: i64) -> Result<Dashboard> {
        Ok(Dashboard {
            counts: self.queue_counts(user, &StudyScope::all(), now_ms)?,
            boost_today: self.boost_today(user, None, now_ms)?,
            decks: self
                .list_decks(user, now_ms)?
                .into_iter()
                .filter(|d| d.archived_at.is_none())
                .collect(),
            show_connect_cta: !self.store.connect_cta_hidden(user)?,
            timezone_is_default: self.timezone_is_default(user)?,
        })
    }

    pub fn dismiss_connect_cta(&self, user: UserId) -> Result<()> {
        Ok(self.store.set_connect_cta_hidden(user, true)?)
    }

    pub fn end_session(
        &self,
        user: UserId,
        session: SessionId,
        now_ms: i64,
    ) -> Result<SessionStats> {
        self.store.end_session(user, session, now_ms)?;
        Ok(self.store.session_stats(user, session)?)
    }

    /// What a session whose queue ran out shows: its grades, how long it
    /// took, when the next cards come and the week ahead. `held` is what
    /// today's limits held back (`SessionInfo::held_by_limit`).
    pub fn finished_session(
        &self,
        user: UserId,
        session: SessionId,
        held: u32,
        now_ms: i64,
    ) -> Result<FinishedSession> {
        let scope = self.session_scope(user, session)?;
        check_scope(&scope)?;
        let settings = self.store.get_settings(user)?;
        let daily_new = self.daily_new(user, &settings, &scope, now_ms)?;
        Ok(FinishedSession {
            stats: self.store.session_stats(user, session)?,
            took_ms: self
                .store
                .session_span(user, session)?
                .map(|(start, last)| (last - start).max(0)),
            next_due: self
                .next_due_given(user, &settings, &scope, held, now_ms, || Ok(daily_new))?,
            week: self.week_ahead_given(user, &settings, &scope, held, now_ms, daily_new)?,
            goal: self.session_goal(user, &scope, now_ms)?,
        })
    }

    /// Dashboard numbers without starting a session, derived from the
    /// same ordered queue a session would get, so they always agree.
    pub fn queue_counts(
        &self,
        user: UserId,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<QueueCounts> {
        check_scope(scope)?;
        let settings = self.store.get_settings(user)?;
        let day_start = Self::study_day_start_ms(&settings, now_ms);
        let budgets = self.budgets(user, &settings, now_ms)?;
        let due_at = Self::due_at(&settings, now_ms);
        let (entries, _) =
            self.store
                .queue_entries(user, scope, due_at, budgets.most_new_per_deck())?;
        let ordered = order_queue(&entries, due_at, &budgets);
        let by_id: std::collections::HashMap<CardId, &flash_core::queue::QueueEntry> =
            entries.iter().map(|e| (e.card_id, e)).collect();
        let mut due = 0u32;
        let mut new_available = 0u32;
        for id in &ordered {
            if let Some(e) = by_id.get(id) {
                if e.phase == flash_core::Phase::New {
                    new_available += 1;
                } else if due_at.is_due(e.phase, e.due_ms) {
                    due += 1;
                }
            }
        }
        let reviewed_today = self.store.reviews_since(user, day_start)?.len() as u32;
        Ok(QueueCounts {
            due,
            new_available,
            reviewed_today,
        })
    }

    /// When the next cards come for a scope whose queue ran out. `held`
    /// is what today's limits held back (`SessionInfo::held_by_limit`):
    /// those come at the next study day's start, as many as a day's new
    /// cards allow. None when nothing is scheduled and nothing waits.
    pub fn next_due(
        &self,
        user: UserId,
        scope: &StudyScope,
        held: u32,
        now_ms: i64,
    ) -> Result<Option<NextDue>> {
        check_scope(scope)?;
        let settings = self.store.get_settings(user)?;
        self.next_due_given(user, &settings, scope, held, now_ms, || {
            self.daily_new(user, &settings, scope, now_ms)
        })
    }

    /// `next_due`, with the scope's `daily_new` asked for only when the
    /// held cards come on the day it names.
    fn next_due_given(
        &self,
        user: UserId,
        settings: &UserSettings,
        scope: &StudyScope,
        held: u32,
        now_ms: i64,
        daily_new: impl FnOnce() -> Result<u32>,
    ) -> Result<Option<NextDue>> {
        let tomorrow = Self::next_study_day_ms(settings, now_ms);
        let seen = self
            .store
            .next_due_after(user, scope, Self::due_at(settings, now_ms))?;
        let waiting = (held > 0).then_some(tomorrow);
        let Some(at_ms) = [seen, waiting].into_iter().flatten().min() else {
            return Ok(None);
        };
        let day_start = Self::study_day_start_ms(settings, at_ms);
        let day_end = Self::next_study_day_ms(settings, at_ms);
        // Later today counts from now; a later day from its start.
        let from = if day_start <= now_ms {
            now_ms + 1
        } else {
            day_start
        };
        let mut cards = self.store.count_due_between(user, scope, from, day_end)?;
        if waiting.is_some_and(|t| t >= day_start && t < day_end) {
            cards += held.min(daily_new()?);
        }
        Ok(Some(NextDue {
            at_ms,
            in_days: flash_core::day::study_days_between(
                &settings.timezone,
                settings.day_cutoff_hour,
                now_ms,
                at_ms,
            ),
            cards,
        }))
    }

    /// The week after today, a study day at a time, and how many cards in
    /// scope come due on each: the reviews already scheduled, and the new
    /// cards today's limits held back (`held`), a day's worth on each day
    /// as `next_due` counts them.
    pub fn week_ahead(
        &self,
        user: UserId,
        scope: &StudyScope,
        held: u32,
        now_ms: i64,
    ) -> Result<Vec<DayAhead>> {
        check_scope(scope)?;
        let settings = self.store.get_settings(user)?;
        let daily_new = self.daily_new(user, &settings, scope, now_ms)?;
        self.week_ahead_given(user, &settings, scope, held, now_ms, daily_new)
    }

    fn week_ahead_given(
        &self,
        user: UserId,
        settings: &UserSettings,
        scope: &StudyScope,
        held: u32,
        now_ms: i64,
        daily_new: u32,
    ) -> Result<Vec<DayAhead>> {
        let mut waiting = held;
        let mut start = Self::next_study_day_ms(settings, now_ms);
        let mut days = Vec::with_capacity(7);
        for _ in 0..7 {
            let end = Self::next_study_day_ms(settings, start);
            let fresh = waiting.min(daily_new);
            waiting -= fresh;
            let cards = self.store.count_due_between(user, scope, start, end)? + fresh;
            days.push(DayAhead {
                at_ms: start,
                cards,
            });
            start = end;
        }
        Ok(days)
    }

    /// New cards a day for a scope, as its queue draws them
    /// (`budgets_and_goals`): each deck in it that holds cards on its own
    /// budget, a paced deck's pace on top of the account's new cards and
    /// every other deck's limit within them. A deck holding a goal keeps
    /// its plan's pace.
    fn daily_new(
        &self,
        user: UserId,
        settings: &UserSettings,
        scope: &StudyScope,
        now_ms: i64,
    ) -> Result<u32> {
        let goals = self.goals(user, settings, now_ms, &Default::default())?;
        if let Some(plan) = scope.deck.and_then(|d| goals.views.get(&d)?.plan) {
            return Ok(plan.pace);
        }
        let names = self.store.deck_names(user)?;
        let root = match scope.deck {
            None => None,
            Some(deck) => match names.iter().find(|(id, _)| *id == deck) {
                Some((_, name)) => Some(name.as_str()),
                None => return Ok(0),
            },
        };
        let in_scope: Vec<DeckId> = names
            .iter()
            .filter(|(_, name)| root.is_none_or(|r| in_subtree(name, r)))
            .map(|(id, _)| *id)
            .collect();
        // Decks in scope with studyable cards: a deck that holds none
        // spends nothing.
        let holding = self.store.goal_counts(user, &in_scope)?;
        let limits: std::collections::HashMap<_, _> =
            self.store.all_deck_limits(user)?.into_iter().collect();
        let (mut paced, mut own) = (0u32, 0u32);
        for (deck, _) in &names {
            if !holding.contains_key(deck) {
                continue;
            }
            match goals.pace(*deck) {
                Some(pace) => paced = paced.saturating_add(pace),
                None => {
                    let limit = limits.get(deck).and_then(|l| l.new_per_day);
                    own = own.saturating_add(limit.unwrap_or(settings.new_per_day));
                }
            }
        }
        Ok(paced.saturating_add(own.min(settings.new_per_day)))
    }

    /// The Stats page's numbers: a scan of the whole review history, so a
    /// heavy job that needs its permit. `year` asks for a past calendar
    /// year in the heatmap; None, the current year, or a year before the
    /// first review is the trailing view (the past 46 weeks, today, and
    /// six weeks of upcoming load).
    pub fn stats(&self, permit: &HeavyPermit, now_ms: i64, year: Option<i32>) -> Result<StatsData> {
        let user = permit.user();
        let settings = self.store.get_settings(user)?;
        let tz = flash_core::day::time_zone(&settings.timezone).unwrap_or(TimeZone::UTC);
        const DAY: i64 = flash_core::scheduler::MS_PER_DAY;
        // Shift by the study-day cutoff so a 2am review counts toward the
        // previous day, matching the queue's notion of "today".
        let cutoff_ms = i64::from(settings.day_cutoff_hour) * 3_600_000;
        let local_of = |ts_ms: i64| -> Zoned {
            Timestamp::from_millisecond(ts_ms - cutoff_ms)
                .unwrap_or(Timestamp::UNIX_EPOCH)
                .to_zoned(tz.clone())
        };
        // A study day is the civil epoch day of the cutoff-shifted local
        // time: an index that keeps ordering and, being civil already,
        // converts to and from a date through UTC with no zone work.
        let day_of = |ts_ms: i64| -> i64 {
            let shifted = ts_ms - cutoff_ms;
            let local = local_of(ts_ms);
            (shifted + i64::from(local.offset().seconds()) * 1000).div_euclid(DAY)
        };
        let civil_of = |idx: i64| -> jiff::civil::Date {
            Timestamp::from_millisecond(idx * DAY)
                .unwrap_or(Timestamp::UNIX_EPOCH)
                .to_zoned(TimeZone::UTC)
                .date()
        };
        let idx_of = |date: jiff::civil::Date| -> i64 {
            date.to_zoned(TimeZone::UTC)
                .map(|z| z.timestamp().as_millisecond())
                .unwrap_or(0)
                .div_euclid(DAY)
        };
        let formats = crate::i18n::tr().formats();
        let label_of = |idx: i64| -> Words {
            let date = civil_of(idx);
            Words::formatted(match &formats {
                Some(f) => f.day_month_numeric(date),
                None => format!("{:02}/{:02}", date.month(), date.day()),
            })
        };

        // The whole history, folded into study days.
        let mut per_day: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
        let mut total_reviews = 0u32;
        for (bucket_ms, count) in self.store.review_buckets(user)? {
            *per_day.entry(day_of(bucket_ms)).or_default() += count;
            total_reviews += count;
        }
        // Retention needs each review's rating and phase: the last 30 days.
        let recent = self.store.reviews_since(user, now_ms - 30 * DAY)?;
        let reviews_30d = recent.len() as u32;
        let mut review_reviews = 0u32;
        let mut review_recalled = 0u32;
        for &(_, rating, phase_before) in &recent {
            if phase_before == 2 {
                review_reviews += 1;
                if rating > 1 {
                    review_recalled += 1;
                }
            }
        }

        let today_idx = day_of(now_ms);
        let mut streak = 0u32;
        let mut idx = today_idx;
        // Today counts if studied; otherwise the streak may end yesterday.
        if !per_day.contains_key(&idx) {
            idx -= 1;
        }
        while per_day.contains_key(&idx) {
            streak += 1;
            idx -= 1;
        }

        let mut longest_streak = 0u32;
        {
            let mut keys: Vec<i64> = per_day.keys().copied().collect();
            keys.sort_unstable();
            let mut run = 0u32;
            let mut prev: Option<i64> = None;
            for k in keys {
                run = if prev == Some(k - 1) { run + 1 } else { 1 };
                prev = Some(k);
                longest_streak = longest_streak.max(run);
            }
        }

        let mut days = Vec::with_capacity(30);
        for offset in (0..30).rev() {
            let idx = day_of(now_ms - offset * DAY);
            days.push(DayCount {
                label: label_of(idx),
                count: per_day.get(&idx).copied().unwrap_or(0),
                pct: 0,
            });
        }
        let max = days.iter().map(|d| d.count).max().unwrap_or(0).max(1);
        for day in &mut days {
            day.pct = day.count * 100 / max;
        }

        let mut upcoming_map: std::collections::HashMap<i64, u32> =
            std::collections::HashMap::new();
        for due in self.store.due_between(user, now_ms, now_ms + 7 * DAY)? {
            *upcoming_map.entry(day_of(due)).or_default() += 1;
        }
        let mut upcoming = Vec::with_capacity(7);
        for offset in 0..7 {
            let idx = day_of(now_ms + offset * DAY);
            upcoming.push(DayCount {
                label: if offset == 0 {
                    crate::i18n::t("stats-today")
                } else {
                    label_of(idx)
                },
                count: upcoming_map.get(&idx).copied().unwrap_or(0),
                pct: 0,
            });
        }

        // ---- Heatmap: week columns, Sunday first. The trailing view is
        // 53 of them, today's week at column 46 and six of upcoming due
        // load after it; a calendar year runs from the Sunday on or
        // before Jan 1 to the Saturday on or after Dec 31, 53 or 54. ----
        const WEEKS: i64 = 53;
        const PAST_WEEKS: i64 = 46;
        let first_review_idx = per_day.keys().min().copied();
        let current_year = i32::from(civil_of(today_idx).year());
        let first_year =
            first_review_idx.map_or(current_year, |idx| i32::from(civil_of(idx).year()));
        let heat_years: Vec<i32> = if first_year < current_year {
            (first_year..=current_year).collect()
        } else {
            Vec::new()
        };
        let heat_year = year.filter(|y| (first_year..current_year).contains(y));
        // Epoch day 0 (1970-01-01) was a Thursday; +4 makes Sunday zero.
        let sunday_on_or_before = |idx: i64| idx - (idx + 4).rem_euclid(7);
        let (first_idx, last_idx, stat_lo, stat_hi) = match heat_year {
            None => {
                let first = sunday_on_or_before(today_idx) - PAST_WEEKS * 7;
                (first, first + WEEKS * 7 - 1, today_idx - 364, today_idx)
            }
            Some(y) => {
                let jan1 = idx_of(jiff::civil::date(y as i16, 1, 1));
                let dec31 = idx_of(jiff::civil::date(y as i16, 12, 31));
                (
                    sunday_on_or_before(jan1),
                    sunday_on_or_before(dec31) + 6,
                    jan1,
                    dec31,
                )
            }
        };
        let weeks = (last_idx - first_idx + 1) / 7;

        // Upcoming load fills the columns after today, in the trailing view.
        let mut future_map: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
        if heat_year.is_none() {
            let horizon_ms = now_ms + (last_idx - today_idx + 2) * DAY;
            for due in self.store.due_between(user, now_ms, horizon_ms)? {
                let idx = day_of(due);
                if idx > today_idx {
                    *future_map.entry(idx).or_default() += 1;
                }
            }
        }
        let past_max = per_day
            .iter()
            .filter(|(k, _)| (first_idx..=last_idx.min(today_idx)).contains(k))
            .map(|(_, v)| *v)
            .max()
            .unwrap_or(0)
            .max(1);
        let future_max = future_map.values().copied().max().unwrap_or(0).max(1);
        let level = |count: u32, max: u32| -> u8 {
            if count == 0 {
                0
            } else {
                (count * 4).div_ceil(max).clamp(1, 4) as u8
            }
        };

        let mut heat_weeks = Vec::with_capacity(weeks as usize);
        let mut heat_months: Vec<MonthSpan> = Vec::new();
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        for week in 0..weeks {
            let mut cells = Vec::with_capacity(7);
            let col_start_idx = first_idx + week * 7;
            let col_date = civil_of(col_start_idx);
            let col_month = Words::formatted(match &formats {
                Some(f) => f.month(col_date),
                None => MONTHS[(col_date.month() as usize - 1) % 12].to_string(),
            });
            match heat_months.last_mut() {
                Some(span) if span.label == col_month => span.weeks += 1,
                _ => heat_months.push(MonthSpan {
                    label: col_month,
                    weeks: 1,
                }),
            }
            for dow in 0..7 {
                let cell_idx = col_start_idx + dow;
                let future = heat_year.is_none() && cell_idx > today_idx;
                let count = if future {
                    future_map.get(&cell_idx).copied().unwrap_or(0)
                } else {
                    per_day.get(&cell_idx).copied().unwrap_or(0)
                };
                cells.push(HeatCell {
                    count,
                    level: level(count, if future { future_max } else { past_max }),
                    label: label_of(cell_idx),
                    future,
                    today: heat_year.is_none() && cell_idx == today_idx,
                });
            }
            heat_weeks.push(cells);
        }
        for span in &mut heat_months {
            if span.weeks < 3 {
                span.label = Words::default();
            }
        }

        // The legend's averages cover the window the heading names,
        // counted from the first review when it falls inside it.
        let lo = first_review_idx.map_or(stat_lo, |first| stat_lo.max(first));
        let denom = (stat_hi - lo + 1).max(1) as u32;
        let (window_reviews, window_days) = per_day
            .iter()
            .filter(|(k, _)| (lo..=stat_hi).contains(k))
            .fold((0u32, 0u32), |(r, d), (_, v)| (r + v, d + 1));
        let (active_cards, mature_cards) = self.store.card_totals(user)?;

        Ok(StatsData {
            reviews_30d,
            retention_pct: (review_reviews > 0).then(|| review_recalled * 100 / review_reviews),
            streak,
            longest_streak,
            daily_avg: window_reviews / denom,
            days_learned_pct: (window_days * 100 / denom).min(100),
            total_reviews,
            active_cards,
            mature_cards,
            days,
            upcoming,
            heat_weeks,
            heat_months,
            heat_years,
            heat_year,
        })
    }
}
