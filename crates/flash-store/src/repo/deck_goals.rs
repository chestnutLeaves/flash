//! Deck goals: the date a deck should be learned by, and the counts that
//! say how it is going.

use std::collections::HashMap;

use flash_core::goal::{self, Date};
use flash_core::{DeckId, UserId};
use rusqlite::{params, Connection};

use crate::{exec_expect_row, Result, Store};

/// A deck's goal date and the plan it was set with (`flash_core::goal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckGoal {
    pub date: Date,
    /// New cards a day when the plan was made: what being on track means.
    pub pace: u32,
    /// How many studyable cards the deck held then; a change re-plans.
    pub cards: u32,
}

/// One deck's studyable cards as a goal counts them: not deleted, not
/// suspended, in a deck that is not archived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GoalCounts {
    /// Never studied.
    pub unseen: u32,
    /// Past their learning steps (review or relearning).
    pub learned: u32,
    pub total: u32,
}

impl Store {
    pub fn deck_goal_paths(&self, user: UserId) -> Result<Vec<(DeckId, String, Option<Date>)>> {
        deck_goal_paths(&self.conn(), user)
    }

    pub fn all_deck_goals(&self, user: UserId) -> Result<Vec<(DeckId, DeckGoal)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, goal_date, goal_pace, goal_cards
             FROM decks WHERE user_id = ?1 AND goal_date IS NOT NULL",
        )?;
        let rows = stmt.query_map([user.raw()], |r| {
            Ok((
                DeckId(r.get(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (deck, date, pace, cards) = row?;
            if let Some(date) = goal::parse_date(&date) {
                out.push((
                    deck,
                    DeckGoal {
                        date,
                        pace: pace.clamp(0, u32::MAX as i64) as u32,
                        cards: cards.clamp(0, u32::MAX as i64) as u32,
                    },
                ));
            }
        }
        Ok(out)
    }

    pub fn set_deck_goal(&self, user: UserId, deck: DeckId, goal: Option<DeckGoal>) -> Result<()> {
        exec_expect_row(
            &self.conn(),
            "deck",
            "UPDATE decks SET goal_date = ?1, goal_pace = ?2, goal_cards = ?3
             WHERE id = ?4 AND user_id = ?5",
            params![
                goal.map(|g| g.date.to_string()),
                goal.map(|g| g.pace),
                goal.map(|g| g.cards),
                deck.0,
                user.raw()
            ],
        )
    }

    /// Each of `decks`' studyable cards, as its goal counts them. A deck
    /// with none is missing. Only the decks asked about are read, so an
    /// account's other cards cost nothing.
    pub fn goal_counts(
        &self,
        user: UserId,
        decks: &[DeckId],
    ) -> Result<HashMap<DeckId, GoalCounts>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(GOAL_COUNTS)?;
        let ids = serde_json::json!(decks.iter().map(|d| d.0).collect::<Vec<_>>()).to_string();
        let rows = stmt.query_map(params![user.raw(), ids], |r| {
            Ok((
                DeckId(r.get(0)?),
                GoalCounts {
                    unseen: r.get::<_, i64>(1)? as u32,
                    learned: r.get::<_, i64>(2)? as u32,
                    total: r.get::<_, i64>(3)? as u32,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// `goal_counts` for the user ?1 over the decks listed in ?2 (JSON):
/// idx_cards_study seeks each deck and holds all it asks of a card.
pub(crate) const GOAL_COUNTS: &str = "SELECT c.deck_id,
       SUM(CASE WHEN s.phase = 0 THEN 1 ELSE 0 END),
       SUM(CASE WHEN s.phase IN (2, 3) THEN 1 ELSE 0 END),
       COUNT(*)
     FROM cards c
     JOIN card_state s ON s.card_id = c.id
     JOIN decks d ON d.id = c.deck_id
     WHERE c.user_id = ?1 AND d.user_id = ?1
       AND c.deck_id IN (SELECT value FROM json_each(?2))
       AND c.deleted_at IS NULL AND c.suspended = 0
       AND d.archived_at IS NULL
     GROUP BY c.deck_id";

/// Every deck's id, whole path and goal date (None without one): what
/// finding the goal that applies to a deck needs, on any connection (a
/// sync replay's transaction too).
pub fn deck_goal_paths(
    conn: &Connection,
    user: UserId,
) -> Result<Vec<(DeckId, String, Option<Date>)>> {
    let mut stmt = conn.prepare("SELECT id, name, goal_date FROM decks WHERE user_id = ?1")?;
    let rows = stmt.query_map([user.raw()], |r| {
        Ok((
            DeckId(r.get(0)?),
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, date) = row?;
        out.push((id, name, date.as_deref().and_then(goal::parse_date)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flash_core::queue::Due;
    use flash_core::{validate_card_text, CardId, CardState, Phase};

    const NOW: i64 = 1_700_000_000_000;
    const MINUTE: i64 = 60_000;

    /// `goal_counts` as it read before it took a list of decks: every
    /// deck of the account, found through the deck alone.
    fn every_deck(store: &Store, user: UserId) -> HashMap<DeckId, GoalCounts> {
        let conn = store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT c.deck_id,
                   SUM(CASE WHEN s.phase = 0 THEN 1 ELSE 0 END),
                   SUM(CASE WHEN s.phase IN (2, 3) THEN 1 ELSE 0 END),
                   COUNT(*)
                 FROM cards c
                 JOIN card_state s ON s.card_id = c.id
                 JOIN decks d ON d.id = c.deck_id
                 WHERE d.user_id = ?1 AND c.deleted_at IS NULL AND c.suspended = 0
                   AND d.archived_at IS NULL
                 GROUP BY c.deck_id",
            )
            .unwrap();
        let rows = stmt
            .query_map([user.raw()], |r| {
                Ok((
                    DeckId(r.get(0)?),
                    GoalCounts {
                        unseen: r.get::<_, i64>(1)? as u32,
                        learned: r.get::<_, i64>(2)? as u32,
                        total: r.get::<_, i64>(3)? as u32,
                    },
                ))
            })
            .unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    /// `list_decks`' counts as they read before each named the account
    /// as well as the deck.
    fn deck_counts_by_deck_alone(store: &Store, user: UserId, due: Due) -> Vec<[i64; 4]> {
        let sql = crate::repo::decks::LIST_DECKS.replace("c.user_id = ?1 AND ", "");
        assert_eq!(sql.matches("c.deck_id = d.id").count(), 3);
        assert!(!sql.contains("c.user_id"));
        let conn = store.conn();
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows = stmt
            .query_map(
                rusqlite::params![user.raw(), due.now_ms, due.reviews_before_ms],
                |r| Ok([r.get(0)?, r.get(3)?, r.get(4)?, r.get(6)?]),
            )
            .unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    /// Asking for some decks counts each exactly as counting every deck
    /// did, over subdecks, an archived deck, suspended and deleted cards,
    /// every phase and two accounts; another account's deck counts
    /// nothing. A deck's row on the deck list counts the same too.
    #[test]
    fn counting_only_the_decks_asked_changes_no_count() {
        let store = Store::open_in_memory().unwrap();
        let mut accounts = Vec::new();
        for name in ["FlashTester", "Other"] {
            let user = store.create_user(name, None, "member", NOW).unwrap();
            let decks: Vec<DeckId> = ["Boards", "Boards::Cells", "Old", "Other", "Empty"]
                .iter()
                .map(|d| store.create_deck(user, d, "", NOW).unwrap())
                .collect();
            let mut cards: Vec<CardId> = Vec::new();
            for i in 0..40 {
                let deck = decks[i % 4];
                let text = validate_card_text(&format!("{name} q{i}"), "a").unwrap();
                cards.extend(
                    store
                        .create_cards(user, deck, &[(text, vec![])], None, NOW)
                        .unwrap(),
                );
            }
            let phases = [
                Phase::New,
                Phase::Learning,
                Phase::Review,
                Phase::Relearning,
            ];
            let states: Vec<(CardId, CardState)> = cards
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 5 != 0)
                .map(|(i, card)| {
                    let state = CardState {
                        phase: phases[i % 4],
                        stability: Some(3.0),
                        difficulty: Some(5.0),
                        due_ms: NOW + (i as i64 - 20) * 60 * MINUTE,
                        last_review_ms: Some(NOW - MINUTE),
                        reps: 1,
                        lapses: 0,
                    };
                    (*card, state)
                })
                .collect();
            store.set_card_states(user, &states).unwrap();
            for card in cards.iter().skip(3).step_by(7) {
                store.set_card_suspended(user, *card, true, NOW).unwrap();
            }
            for card in cards.iter().skip(5).step_by(9) {
                store.delete_card(user, *card, NOW).unwrap();
            }
            store.set_deck_archived(user, decks[2], Some(NOW)).unwrap();
            accounts.push((user, decks));
        }

        for (user, decks) in &accounts {
            let all = every_deck(&store, *user);
            assert!(all.len() >= 3, "{all:?}");
            let mut asked: Vec<Vec<DeckId>> = vec![vec![], decks.clone()];
            asked.extend(decks.iter().map(|d| vec![*d]));
            asked.push(vec![decks[0], decks[1]]);
            asked.push(vec![decks[1], decks[3], decks[4]]);
            for list in asked {
                let expected: HashMap<DeckId, GoalCounts> = all
                    .iter()
                    .filter(|(deck, _)| list.contains(deck))
                    .map(|(deck, counts)| (*deck, *counts))
                    .collect();
                assert_eq!(
                    store.goal_counts(*user, &list).unwrap(),
                    expected,
                    "{list:?}"
                );
            }
            for due in [Due::exact(NOW), Due::by_day(NOW, NOW + 6 * 60 * MINUTE)] {
                let rows: Vec<[i64; 4]> = store
                    .list_decks(*user, due)
                    .unwrap()
                    .iter()
                    .map(|d| {
                        [
                            d.id.0,
                            d.due_count.into(),
                            d.new_count.into(),
                            d.card_count.into(),
                        ]
                    })
                    .collect();
                assert_eq!(rows, deck_counts_by_deck_alone(&store, *user, due));
            }
        }
        let (first, _) = accounts[0];
        let (_, other_decks) = &accounts[1];
        assert!(store.goal_counts(first, other_decks).unwrap().is_empty());
    }
}
