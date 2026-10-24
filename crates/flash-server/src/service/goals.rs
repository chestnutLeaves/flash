//! A deck's goal date as the service applies it. The rules are
//! `flash_core::goal`'s; this reads what they need from the store: which
//! deck's goal applies to which deck (its own, else the nearest deck it
//! sits in with one, a goal already passed skipped), each deck's unseen
//! cards as the study day began, and the plan a goal was set with.

use std::collections::HashMap;

use flash_core::goal::{self, Date, GoalWindow, Plan};
use flash_core::{deck_path, DeckId, UserId, UserSettings};
use flash_store::{DeckGoal, GoalCounts};
use jiff::ToSpan;

use crate::service::{Result, ServiceError, Services};

/// A deck's goal as its row shows it, over the deck and the decks under
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalView {
    pub date: Date,
    /// The account's study date today.
    pub today: Date,
    /// Study days left: 0 on the day itself, negative once it passed.
    pub days_left: i64,
    /// Never studied, as today began.
    pub unseen: u32,
    pub learned: u32,
    pub total: u32,
    /// How the deck gets there from today; None from the goal day on.
    pub plan: Option<Plan>,
    /// New cards behind the plan the goal was set with.
    pub behind_by: u32,
}

impl GoalView {
    pub fn passed(&self) -> bool {
        self.days_left < 0
    }

    /// How much of the deck is learned, out of 100.
    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        ((self.learned as u64 * 100) / self.total as u64) as u32
    }

    /// The last day new cards come in; the final window follows it.
    pub fn cutoff(&self) -> Option<Date> {
        let plan = self.plan?;
        self.today.checked_add(plan.learn_days.days()).ok()
    }
}

/// How a goal on a date would go, for the dialog that sets one.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalPreview {
    pub date: Date,
    pub today: Date,
    pub unseen: u32,
    pub plan: Plan,
}

impl GoalPreview {
    /// The last day new cards come in.
    pub fn cutoff(&self) -> Date {
        self.today
            .checked_add(self.plan.learn_days.days())
            .unwrap_or(self.date)
    }
}

/// The goal a finished session counts toward: the session's deck's own,
/// else the nearest deck's it sits in; for the whole account, the goal
/// nearest in time. Only goals still ahead (or today).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionGoal {
    pub deck: DeckId,
    /// The deck's path as a person reads it.
    pub path: String,
    pub view: GoalView,
}

/// New cards a day above which the dialog warns the date is tight.
pub const TIGHT_PACE: u32 = 50;

#[derive(Debug, Default)]
pub(crate) struct Goals {
    pub paced: HashMap<DeckId, (i64, u32)>,
    pub views: HashMap<DeckId, GoalView>,
}

impl Goals {
    pub fn pace(&self, deck: DeckId) -> Option<u32> {
        let (days_left, unseen) = self.paced.get(&deck)?;
        goal::plan(*days_left, *unseen).map(|p| p.pace)
    }
}

fn nearest_first(name: &str) -> impl Iterator<Item = &str> {
    std::iter::once(name).chain(deck_path::ancestors(name).into_iter().rev())
}

/// The goal dates on `name` and the decks it sits in, nearest first,
/// passed or not. The one that applies on a day is the first not before it.
fn goal_chain<'a>(
    name: &'a str,
    date_of: impl Fn(&str) -> Option<Date> + 'a,
) -> impl Iterator<Item = Date> + 'a {
    nearest_first(name).filter_map(date_of)
}

pub(crate) fn in_subtree(name: &str, root: &str) -> bool {
    name == root || deck_path::is_descendant(name, root)
}

fn goal_ahead(row: &crate::service::DeckRow) -> Option<SessionGoal> {
    row.goal
        .as_ref()
        .filter(|g| !g.passed() && row.archived_at.is_none())
        .map(|g| SessionGoal {
            deck: row.id,
            path: row.path.clone(),
            view: g.clone(),
        })
}

/// The goal dates on `deck` and the decks it sits in, nearest first,
/// passed or not, given every deck's path and own goal date
/// (`flash_store::ext::deck_goal_paths`). Empty for a deck not listed.
pub fn goal_dates_for(deck: DeckId, decks: &[(DeckId, String, Option<Date>)]) -> Vec<Date> {
    let Some((_, name, _)) = decks.iter().find(|(id, _, _)| *id == deck) else {
        return Vec::new();
    };
    let dates: HashMap<&str, Date> = decks
        .iter()
        .filter_map(|(_, n, d)| d.map(|d| (n.as_str(), d)))
        .collect();
    goal_chain(name, |path| dates.get(path).copied()).collect()
}

/// The goal date that applies to `deck` on the study date `today`: its
/// own, else the nearest deck's it sits in, a goal already passed
/// skipped (`goal_dates_for`).
pub fn goal_date_for(
    deck: DeckId,
    decks: &[(DeckId, String, Option<Date>)],
    today: Date,
) -> Option<Date> {
    goal_dates_for(deck, decks)
        .into_iter()
        .find(|date| *date >= today)
}

fn subtree_counts(
    root: &str,
    names: &[(DeckId, String)],
    counts: &HashMap<DeckId, GoalCounts>,
) -> GoalCounts {
    names
        .iter()
        .filter(|(_, name)| in_subtree(name, root))
        .filter_map(|(id, _)| counts.get(id))
        .fold(GoalCounts::default(), |acc, c| GoalCounts {
            unseen: acc.unseen + c.unseen,
            learned: acc.learned + c.learned,
            total: acc.total + c.total,
        })
}

impl Services {
    pub(crate) fn goals(
        &self,
        user: UserId,
        settings: &UserSettings,
        now_ms: i64,
        introduced: &HashMap<DeckId, u32>,
    ) -> Result<Goals> {
        let stored: HashMap<DeckId, DeckGoal> =
            self.store().all_deck_goals(user)?.into_iter().collect();
        if stored.is_empty() {
            return Ok(Goals::default());
        }
        let names = self.store().deck_names(user)?;
        let ids: HashMap<&str, DeckId> = names.iter().map(|(id, n)| (n.as_str(), *id)).collect();
        let date_of = |path: &str| ids.get(path).and_then(|d| stored.get(d)).map(|g| g.date);
        // Only the decks read below are counted: those with a goal of their
        // own or on a deck they sit in, passed or not.
        let under_goals: Vec<DeckId> = names
            .iter()
            .filter(|(_, name)| goal_chain(name, date_of).next().is_some())
            .map(|(id, _)| *id)
            .collect();
        let mut counts = self.store().goal_counts(user, &under_goals)?;
        for (deck, n) in introduced {
            counts.entry(*deck).or_default().unseen += n;
        }
        let today = settings.study_date(now_ms);
        let mut out = Goals::default();
        for (deck, name) in &names {
            if let Some(stored_goal) = stored.get(deck) {
                let c = subtree_counts(name, &names, &counts);
                let days_left = goal::days_between(today, stored_goal.date);
                let plan = goal::plan(days_left, c.unseen).filter(|_| days_left >= 1);
                let mut baseline = stored_goal.pace;
                if stored_goal.cards != c.total && days_left >= 0 {
                    baseline = plan.map(|p| p.pace).unwrap_or(0);
                    self.store().set_deck_goal(
                        user,
                        *deck,
                        Some(DeckGoal {
                            pace: baseline,
                            cards: c.total,
                            ..*stored_goal
                        }),
                    )?;
                }
                out.views.insert(
                    *deck,
                    GoalView {
                        date: stored_goal.date,
                        today,
                        days_left,
                        unseen: c.unseen,
                        learned: c.learned,
                        total: c.total,
                        behind_by: plan
                            .map(|p| goal::behind_by(&p, c.unseen, baseline))
                            .unwrap_or(0),
                        plan,
                    },
                );
            }
            let Some(date) = goal_chain(name, date_of).find(|d| *d >= today) else {
                continue;
            };
            let days_left = goal::days_between(today, date);
            if days_left >= 1 {
                let unseen = counts.get(deck).map(|c| c.unseen).unwrap_or(0);
                out.paced.insert(*deck, (days_left, unseen));
            }
        }
        Ok(out)
    }

    /// The final window that applies to a card in `deck` at `now_ms`, if
    /// a goal at least a day away applies to it.
    pub fn goal_window(
        &self,
        user: UserId,
        settings: &UserSettings,
        deck: DeckId,
        now_ms: i64,
    ) -> Result<Option<GoalWindow>> {
        let decks = self.store().deck_goal_paths(user)?;
        let today = settings.study_date(now_ms);
        Ok(goal_date_for(deck, &decks, today).and_then(|date| {
            GoalWindow::new(&settings.timezone, settings.day_cutoff_hour, date, now_ms)
        }))
    }

    /// The account's goals still ahead (or today), nearest first.
    pub fn goals_ahead(&self, user: UserId, now_ms: i64) -> Result<Vec<SessionGoal>> {
        let mut goals: Vec<SessionGoal> = self
            .list_decks(user, now_ms)?
            .iter()
            .filter_map(goal_ahead)
            .collect();
        goals.sort_by_key(|g| g.view.days_left);
        Ok(goals)
    }

    pub fn session_goal(
        &self,
        user: UserId,
        scope: &flash_core::queue::StudyScope,
        now_ms: i64,
    ) -> Result<Option<SessionGoal>> {
        let Some(deck) = scope.deck else {
            return Ok(self.goals_ahead(user, now_ms)?.into_iter().next());
        };
        let rows = self.list_decks(user, now_ms)?;
        let Some(row) = rows.iter().find(|r| r.id == deck) else {
            return Ok(None);
        };
        let goal = nearest_first(&row.name)
            .find_map(|path| rows.iter().find(|r| r.name == path).and_then(goal_ahead));
        Ok(goal)
    }

    fn check_goal_date(settings: &UserSettings, date: Date, now_ms: i64) -> Result<Date> {
        let today = settings.study_date(now_ms);
        let days = goal::days_between(today, date);
        if days < 1 {
            return Err(ServiceError::Invalid(crate::i18n::t("goal-error-past")));
        }
        if days > goal::MAX_DAYS_AHEAD {
            return Err(ServiceError::Invalid(crate::i18n::t("goal-error-far")));
        }
        Ok(date)
    }

    /// Refuses a goal date `set_deck_goal` would refuse, before anything
    /// that should not happen without it (a deck made to hold it).
    pub fn check_deck_goal(&self, user: UserId, date: Date, now_ms: i64) -> Result<()> {
        let settings = self.store().get_settings(user)?;
        Self::check_goal_date(&settings, date, now_ms).map(|_| ())
    }

    fn deck_goal_counts(
        &self,
        user: UserId,
        settings: &UserSettings,
        deck: DeckId,
        now_ms: i64,
    ) -> Result<GoalCounts> {
        let names = self.store().deck_names(user)?;
        let Some((_, name)) = names.iter().find(|(id, _)| *id == deck) else {
            return Err(ServiceError::Store(flash_store::StoreError::NotFound(
                "deck",
            )));
        };
        let subtree: Vec<DeckId> = names
            .iter()
            .filter(|(_, n)| in_subtree(n, name))
            .map(|(id, _)| *id)
            .collect();
        let mut counts = self.store().goal_counts(user, &subtree)?;
        let day_start = settings.study_day_start_ms(now_ms);
        for (id, (introduced, _)) in self.store().daily_consumption_by_deck(user, day_start)? {
            counts.entry(id).or_default().unseen += introduced;
        }
        Ok(subtree_counts(name, &names, &counts))
    }

    /// Gives a deck a goal date (None clears it), planned from today.
    pub fn set_deck_goal(
        &self,
        user: UserId,
        deck: DeckId,
        date: Option<Date>,
        now_ms: i64,
    ) -> Result<()> {
        let Some(date) = date else {
            return Ok(self.store().set_deck_goal(user, deck, None)?);
        };
        let settings = self.store().get_settings(user)?;
        let date = Self::check_goal_date(&settings, date, now_ms)?;
        let counts = self.deck_goal_counts(user, &settings, deck, now_ms)?;
        let today = settings.study_date(now_ms);
        let pace = goal::plan(goal::days_between(today, date), counts.unseen)
            .map(|p| p.pace)
            .unwrap_or(0);
        Ok(self.store().set_deck_goal(
            user,
            deck,
            Some(DeckGoal {
                date,
                pace,
                cards: counts.total,
            }),
        )?)
    }

    /// How a goal on `date` would go for a deck, without setting it.
    pub fn goal_preview(
        &self,
        user: UserId,
        deck: DeckId,
        date: Date,
        now_ms: i64,
    ) -> Result<GoalPreview> {
        let settings = self.store().get_settings(user)?;
        let date = Self::check_goal_date(&settings, date, now_ms)?;
        let counts = self.deck_goal_counts(user, &settings, deck, now_ms)?;
        let today = settings.study_date(now_ms);
        let plan = goal::plan(goal::days_between(today, date), counts.unseen)
            .ok_or_else(|| ServiceError::Invalid(crate::i18n::t("goal-error-past")))?;
        Ok(GoalPreview {
            date,
            today,
            unseen: counts.unseen,
            plan,
        })
    }
}
