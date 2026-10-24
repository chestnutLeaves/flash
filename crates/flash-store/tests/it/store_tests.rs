//! Integration tests over an in-memory database: full migration run plus
//! CRUD, review recording, immutability triggers, and queue scoping.

use flash_core::queue::{Due, StudyScope};
use flash_core::{validate_card_text, CardState, Phase, Rating, Scheduler};
use flash_store::{NextPair, RefreshOutcome, Store, StoreError, REFRESH_GRACE_MS};

const NOW: i64 = 1_700_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

fn store_with_user() -> (Store, flash_core::UserId) {
    let store = Store::open_in_memory().expect("open + migrate");
    let user = store
        .create_user("Test User", Some("t@example.com"), "member", NOW)
        .unwrap();
    (store, user)
}

fn card(front: &str, back: &str, tags: &[&str]) -> (flash_core::CardText, Vec<String>) {
    (
        validate_card_text(front, back).unwrap(),
        tags.iter().map(|t| t.to_string()).collect(),
    )
}

/// Every review the log holds for the user, summed over the buckets the
/// stats page reads.
fn total_reviews(store: &Store, user: flash_core::UserId) -> u32 {
    store
        .review_buckets(user)
        .unwrap()
        .iter()
        .map(|(_, n)| n)
        .sum()
}

#[test]
fn migrations_and_default_settings() {
    let (store, user) = store_with_user();
    let settings = store.get_settings(user).unwrap();
    assert_eq!(settings.grading_mode, flash_core::GradingMode::Silent);
    assert_eq!(settings.new_per_day, 20);
    assert_eq!(settings.reviews_per_day, 200);
    assert_eq!((settings.boost_new, settings.boost_day), (0, 0));
    assert!(settings.fsrs_params.is_none());
}

/// The per-account deck cap is the store's, so no caller can walk
/// around it, an import calling the store directly included.
#[test]
fn the_deck_cap_is_enforced_where_the_row_is_created() {
    let (store, user) = store_with_user();
    for i in 0..flash_store::MAX_DECKS_PER_USER {
        store
            .create_deck(user, &format!("deck {i}"), "", NOW)
            .unwrap();
    }
    let err = store.create_deck(user, "one more", "", NOW).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("at most 500 decks")),
        "{err:?}"
    );
    assert_eq!(
        store.deck_count(user).unwrap(),
        flash_store::MAX_DECKS_PER_USER
    );
}

#[test]
fn daily_limits_round_trip() {
    let (store, user) = store_with_user();
    store.set_daily_limits(user, 35, 400).unwrap();
    store.set_account_boost(user, 10, 123).unwrap();
    let s = store.get_settings(user).unwrap();
    assert_eq!(
        (s.new_per_day, s.reviews_per_day, s.boost_new, s.boost_day),
        (35, 400, 10, 123)
    );

    let deck = store.create_deck(user, "Limited", "", NOW).unwrap();
    let limits = store.get_deck_limits(user, deck).unwrap();
    assert_eq!(
        limits,
        flash_store::DeckLimits::default(),
        "new decks inherit everything"
    );
    store.set_deck_limits(user, deck, Some(5), None).unwrap();
    store.set_deck_boost(user, deck, 3, 456).unwrap();
    let limits = store.get_deck_limits(user, deck).unwrap();
    assert_eq!(limits.new_per_day, Some(5));
    assert_eq!(limits.reviews_per_day, None);
    assert_eq!((limits.boost_new, limits.boost_day), (3, 456));
    assert_eq!(store.all_deck_limits(user).unwrap(), vec![(deck, limits)]);
    // Someone else's deck is not found, not silently updated.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(store.set_deck_limits(other, deck, Some(1), None).is_err());
}

#[test]
fn a_deck_goal_round_trips_and_clears() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "Boards", "", NOW).unwrap();
    store.create_deck(user, "Other", "", NOW).unwrap();
    assert_eq!(store.all_deck_goals(user).unwrap(), vec![]);
    let goal = flash_store::DeckGoal {
        date: flash_core::goal::parse_date("2026-11-16").unwrap(),
        pace: 5,
        cards: 120,
    };
    store.set_deck_goal(user, deck, Some(goal)).unwrap();
    assert_eq!(store.all_deck_goals(user).unwrap(), vec![(deck, goal)]);
    store.set_deck_goal(user, deck, None).unwrap();
    assert_eq!(store.all_deck_goals(user).unwrap(), vec![]);
    // Someone else's deck is not found, not silently updated.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(store.set_deck_goal(other, deck, Some(goal)).is_err());
}

#[test]
fn goal_counts_take_only_studyable_cards() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "Boards", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[
                card("a", "1", &[]),
                card("b", "2", &[]),
                card("c", "3", &[]),
                card("d", "4", &[]),
            ],
            None,
            NOW,
        )
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(user, ids[0]).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, NOW).unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    store.suspend_cards(user, &[ids[3]], NOW).unwrap();
    let counts = store.goal_counts(user, &[deck]).unwrap();
    assert_eq!(
        counts.get(&deck),
        Some(&flash_store::GoalCounts {
            unseen: 2,
            learned: 1,
            total: 3
        })
    );
}

#[test]
fn deck_and_card_round_trip() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "Pharmacology", "", NOW).unwrap();
    assert_eq!(
        store.find_deck_by_name(user, "Pharmacology").unwrap(),
        Some(deck)
    );

    let ids = store
        .create_cards(
            user,
            deck,
            &[
                card(
                    "What is the antidote for warfarin?",
                    "Vitamin K",
                    &["exam-2"],
                ),
                card(
                    "Digoxin therapeutic range?",
                    "0.5-2.0 ng/mL",
                    &["cardiac", "exam-2"],
                ),
            ],
            None,
            NOW,
        )
        .unwrap();
    assert_eq!(ids.len(), 2);

    let row = store.get_card(user, ids[1]).unwrap().unwrap();
    assert_eq!(row.back, "0.5-2.0 ng/mL");
    assert_eq!(row.tags, vec!["cardiac", "exam-2"]);

    let state = store.get_card_state(user, ids[0]).unwrap();
    assert_eq!(state, CardState::new_card(NOW));

    let decks = store.list_decks(user, Due::exact(NOW)).unwrap();
    assert_eq!(decks[0].new_count, 2);
    assert_eq!(decks[0].due_count, 0);

    let found = store
        .list_cards(user, Some(deck), Some("warfarin"), 10)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, ids[0]);

    // Tags must round-trip through list_cards, not just get_card.
    let all = store.list_cards(user, Some(deck), None, 10).unwrap();
    let digoxin = all.iter().find(|c| c.id == ids[1]).unwrap();
    assert_eq!(digoxin.tags, vec!["cardiac", "exam-2"]);
    assert_eq!(
        all.iter().find(|c| c.id == ids[0]).unwrap().tags,
        vec!["exam-2"]
    );
}

#[test]
fn review_recording_updates_state_and_log() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(user, deck, &[card("f", "b", &[])], None, NOW)
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();

    let state = store.get_card_state(user, ids[0]).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, NOW).unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();

    let after = store.get_card_state(user, ids[0]).unwrap();
    assert_eq!(after.phase, Phase::Review);
    assert_eq!(after.reps, 1);
    assert!(after.due_ms > NOW);
    // f32 round-trips through a REAL column exactly.
    assert_eq!(after.stability, outcome.state.stability);

    assert_eq!(
        store.daily_consumption_by_deck(user, NOW - 1).unwrap()[&deck],
        (1, 0)
    );
}

#[test]
fn review_log_is_immutable() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(user, deck, &[card("f", "b", &[])], None, NOW)
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(user, ids[0]).unwrap(),
            Rating::Good,
            NOW,
        )
        .unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    // Every interface records its own provenance; the V012 rebuild kept
    // the CHECK and added the app.
    store
        .record_review(user, ids[0], &outcome, NOW + 1, "mobile", Some("ios"), None)
        .unwrap();
    assert!(store
        .record_review(user, ids[0], &outcome, NOW + 2, "bogus", None, None)
        .is_err());
    assert_eq!(total_reviews(&store, user), 2);

    // Triggers must reject tampering even through raw SQL.
    let err = store.raw_execute_for_tests("UPDATE review_log SET rating = 4");
    assert!(err.unwrap_err().to_string().contains("immutable"));
    let err = store.raw_execute_for_tests("DELETE FROM review_log");
    assert!(err.unwrap_err().to_string().contains("immutable"));
}

/// Imported history is identified by its timestamps: the same reviews
/// again add nothing, another user's do, and a card that is not the
/// user's takes no rows at all.
#[test]
fn imported_reviews_dedupe_by_timestamp() {
    let (store, user) = store_with_user();
    let other = store.create_user("Other", None, "member", NOW).unwrap();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(user, deck, &[card("f", "b", &[])], None, NOW)
        .unwrap();
    let other_deck = store.create_deck(other, "D", "", NOW).unwrap();
    let other_ids = store
        .create_cards(other, other_deck, &[card("f", "b", &[])], None, NOW)
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let first = scheduler
        .review(
            &CardState::new_card(NOW - 2 * DAY),
            Rating::Good,
            NOW - 2 * DAY,
        )
        .unwrap();
    let second = scheduler
        .review(&first.state, Rating::Good, NOW - DAY)
        .unwrap();
    let rows = |card: flash_core::CardId| {
        vec![
            flash_store::ImportedReview {
                card,
                reviewed_at_ms: NOW - 2 * DAY,
                outcome: first.clone(),
                answer_ms: None,
            },
            flash_store::ImportedReview {
                card,
                reviewed_at_ms: NOW - DAY,
                outcome: second.clone(),
                answer_ms: None,
            },
        ]
    };

    assert_eq!(store.import_reviews(user, &rows(ids[0])).unwrap(), 2);
    assert_eq!(store.import_reviews(user, &rows(ids[0])).unwrap(), 0);
    assert_eq!(total_reviews(&store, user), 2);
    // Marked as imported, outside any session or client.
    let provenance: Vec<(String, Option<i64>, Option<String>)> = store
        .with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT source, session_id, client FROM review_log WHERE user_id = ?1")?;
            let rows = stmt.query_map([user.raw()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    assert!(provenance
        .iter()
        .all(|(s, session, client)| s == "import" && session.is_none() && client.is_none()));

    // The same timestamps are another user's own reviews.
    assert_eq!(store.import_reviews(other, &rows(other_ids[0])).unwrap(), 2);
    // A card that is not the user's is not written, not even once.
    assert_eq!(store.import_reviews(user, &rows(other_ids[0])).unwrap(), 0);
    assert_eq!(total_reviews(&store, user), 2);
    // Flash's own review at an imported timestamp is a different review.
    store
        .record_review(user, ids[0], &second, NOW - DAY, "web", None, None)
        .unwrap();
    assert_eq!(total_reviews(&store, user), 3);

    // States land only on the user's cards.
    store
        .set_card_states(user, &[(ids[0], second.state.clone())])
        .unwrap();
    assert_eq!(store.get_card_state(user, ids[0]).unwrap().reps, 2);
    let err = store
        .set_card_states(user, &[(other_ids[0], second.state.clone())])
        .unwrap_err();
    assert!(matches!(err, StoreError::NotFound(_)), "{err}");
}

#[test]
fn api_tokens_list_revoke_and_purge() {
    let (store, user) = store_with_user();
    store
        .create_oauth_client("mcp", "Claude", &[], "{}", NOW)
        .unwrap();
    for (i, (client, label)) in [
        ("flash-mobile", "iPhone"),
        ("flash-mobile", "iPad"),
        ("mcp", ""),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .insert_oauth_token(
                &format!("a{i}"),
                &format!("r{i}"),
                client,
                user,
                "mobile",
                label,
                None,
                NOW + 3_600_000,
                NOW + 60 * DAY,
                NOW + i as i64,
            )
            .unwrap();
    }

    let info = store.lookup_api_token("a0", NOW + 1).unwrap().unwrap();
    assert_eq!(info.user, user);
    assert_eq!(
        (info.role.as_str(), info.client_id.as_str()),
        ("member", "flash-mobile")
    );
    assert!(store.lookup_api_token("nope", NOW).unwrap().is_none());

    // Newest first, one client only.
    let mine = store
        .list_tokens_for_user(user, "flash-mobile", NOW + 5)
        .unwrap();
    let labels: Vec<&str> = mine.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, ["iPad", "iPhone"]);
    assert!(mine.iter().all(|t| t.last_used_at.is_none()));

    // A refresh stamps last_used_at.
    let rotated = store
        .rotate_refresh_token(
            "r1",
            None,
            &NextPair {
                access_hash: "a1n",
                refresh_hash: "r1n",
                salt: "s1",
                access_expires_ms: NOW + 7_200_000,
                refresh_expires_ms: NOW + 61 * DAY,
            },
            NOW + 20,
        )
        .unwrap();
    assert!(matches!(rotated, RefreshOutcome::Rotated(_)), "{rotated:?}");
    let mine = store
        .list_tokens_for_user(user, "flash-mobile", NOW + 25)
        .unwrap();
    assert_eq!(mine[0].last_used_at, Some(NOW + 20));

    // "Sign out other devices" keeps the caller's grant and never touches
    // MCP grants.
    let keep = mine[1].id;
    assert_eq!(
        store
            .revoke_tokens_for_user(user, "flash-mobile", Some(keep), NOW + 30)
            .unwrap(),
        1
    );
    assert!(store.lookup_api_token("a1n", NOW + 31).unwrap().is_none());
    assert!(store.lookup_api_token("a0", NOW + 31).unwrap().is_some());
    assert!(store.lookup_api_token("a2", NOW + 31).unwrap().is_some());

    // Signing out this device revokes exactly once.
    store
        .revoke_token(user, "flash-mobile", keep, NOW + 40)
        .unwrap();
    assert!(matches!(
        store.revoke_token(user, "flash-mobile", keep, NOW + 41),
        Err(StoreError::NotFound(_))
    ));
    assert!(store
        .list_tokens_for_user(user, "flash-mobile", NOW + 42)
        .unwrap()
        .is_empty());

    // Revoked rows survive the grace window, then go; the live MCP grant
    // stays.
    assert_eq!(store.purge_expired_tokens(NOW + 50).unwrap(), 0);
    assert_eq!(store.purge_expired_tokens(NOW + 8 * DAY).unwrap(), 2);
    assert_eq!(
        store
            .list_tokens_for_user(user, "mcp", NOW + 8 * DAY)
            .unwrap()
            .len(),
        1
    );
}

/// A grant of `client` whose tokens are `a0` and `r0`.
fn grant(store: &Store, user: flash_core::UserId, client: &str) {
    store
        .insert_oauth_token(
            "a0",
            "r0",
            client,
            user,
            "mobile",
            "",
            None,
            NOW + 3_600_000,
            NOW + 60 * DAY,
            NOW,
        )
        .unwrap();
}

/// Presents refresh token `old` at `now`, naming no client; a rotation
/// makes `a-{new}` and `new` from salt `s-{new}`.
fn present(store: &Store, old: &str, new: &str, now: i64) -> RefreshOutcome {
    store
        .rotate_refresh_token(
            old,
            None,
            &NextPair {
                access_hash: &format!("a-{new}"),
                refresh_hash: new,
                salt: &format!("s-{new}"),
                access_expires_ms: now + 3_600_000,
                refresh_expires_ms: now + 60 * DAY,
            },
            now,
        )
        .unwrap()
}

#[test]
fn a_rotated_away_refresh_token_gets_the_same_pair_only_within_the_grace() {
    let (store, user) = store_with_user();
    grant(&store, user, "flash-mobile");
    let rotated = present(&store, "r0", "r1", NOW);
    assert!(matches!(rotated, RefreshOutcome::Rotated(_)), "{rotated:?}");

    // The reply was lost: the same token, within the grace, is told how
    // the current pair was made, and nothing is minted.
    match present(&store, "r0", "rx", NOW + REFRESH_GRACE_MS) {
        RefreshOutcome::Again {
            grant,
            salt,
            access_expires_at,
        } => {
            assert_eq!(salt, "s-r1");
            assert_eq!(access_expires_at, NOW + 3_600_000);
            assert_eq!(
                (grant.client_id.as_str(), grant.user_id),
                ("flash-mobile", user)
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(store.lookup_api_token("a-rx", NOW + 1).unwrap().is_none());
    assert!(store.lookup_api_token("a-r1", NOW + 1).unwrap().is_some());

    // Past the grace it is reuse: the whole grant is revoked.
    let reused = present(&store, "r0", "ry", NOW + REFRESH_GRACE_MS + 1);
    assert!(matches!(reused, RefreshOutcome::Reused(_)), "{reused:?}");
    assert!(store.lookup_api_token("a-r1", NOW + 2).unwrap().is_none());
    let current = present(&store, "r1", "rz", NOW + REFRESH_GRACE_MS + 2);
    assert!(matches!(current, RefreshOutcome::Invalid), "{current:?}");
}

#[test]
fn a_rotated_away_refresh_token_after_the_new_one_was_used_revokes_the_grant() {
    let (store, user) = store_with_user();
    grant(&store, user, "flash-mobile");
    present(&store, "r0", "r1", NOW);
    let used = present(&store, "r1", "r2", NOW + 1);
    assert!(matches!(used, RefreshOutcome::Rotated(_)), "{used:?}");

    // Well within the grace, but the pair it made was taken up already.
    let reused = present(&store, "r0", "rx", NOW + 2);
    assert!(matches!(reused, RefreshOutcome::Reused(_)), "{reused:?}");
    assert!(store.lookup_api_token("a-r2", NOW + 3).unwrap().is_none());
    let current = present(&store, "r2", "ry", NOW + 3);
    assert!(matches!(current, RefreshOutcome::Invalid), "{current:?}");
}

#[test]
fn two_rotations_in_one_millisecond_still_know_which_came_last() {
    let (store, user) = store_with_user();
    grant(&store, user, "flash-mobile");
    present(&store, "r0", "r1", NOW);
    present(&store, "r1", "r2", NOW);
    let reused = present(&store, "r0", "rx", NOW);
    assert!(matches!(reused, RefreshOutcome::Reused(_)), "{reused:?}");
}

#[test]
fn another_clients_refresh_token_is_refused_and_left_alone() {
    let (store, user) = store_with_user();
    grant(&store, user, "mcp");
    let ask = |old: &str, client: &str, new: &str, now: i64| {
        store
            .rotate_refresh_token(
                old,
                Some(client),
                &NextPair {
                    access_hash: &format!("a-{new}"),
                    refresh_hash: new,
                    salt: "s",
                    access_expires_ms: now + 3_600_000,
                    refresh_expires_ms: now + 60 * DAY,
                },
                now,
            )
            .unwrap()
    };
    let refused = ask("r0", "flash-mobile", "rx", NOW);
    assert!(matches!(refused, RefreshOutcome::Invalid), "{refused:?}");
    let rotated = ask("r0", "mcp", "r1", NOW + 1);
    assert!(matches!(rotated, RefreshOutcome::Rotated(_)), "{rotated:?}");
    // Its spent token is no reuse to the wrong client, either.
    let refused = ask("r0", "flash-mobile", "ry", NOW + 10 * REFRESH_GRACE_MS);
    assert!(matches!(refused, RefreshOutcome::Invalid), "{refused:?}");
    assert!(store.lookup_api_token("a-r1", NOW + 2).unwrap().is_some());
}

#[test]
fn a_spent_refresh_token_is_forgotten_once_it_would_have_expired() {
    let (store, user) = store_with_user();
    grant(&store, user, "flash-mobile");
    present(&store, "r0", "r1", NOW);
    let spent = |at: i64| {
        store.purge_expired_tokens(at).unwrap();
        store
            .with_conn(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM oauth_spent_refresh", [], |r| {
                        r.get::<_, i64>(0)
                    })?,
                )
            })
            .unwrap()
    };
    assert_eq!(spent(NOW + 60 * DAY - 1), 1);
    // r0 was good until NOW + 60 days: past that it is only unknown.
    let late = present(&store, "r0", "rx", NOW + 60 * DAY);
    assert!(matches!(late, RefreshOutcome::Invalid), "{late:?}");
    assert_eq!(spent(NOW + 60 * DAY + 1), 0);
    assert!(store.lookup_api_token("a-r1", NOW + 1).unwrap().is_some());
}

#[test]
fn delete_user_account_purges_everything_and_only_theirs() {
    use flash_store::media::MediaKind;
    let (store, user) = store_with_user();
    let other = store
        .create_user("Other", Some("o@example.com"), "member", NOW)
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();

    // The doomed account: deck, tagged cards, a review, media (one hash
    // shared with `other`, one unique), sessions, tokens, a passkey.
    let deck = store.create_deck(user, "Pharm", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[card("f", "b", &["t1"]), card("g", "c", &[])],
            None,
            NOW,
        )
        .unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(user, ids[0]).unwrap(),
            Rating::Good,
            NOW,
        )
        .unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    let shared = "a".repeat(64);
    let unique = "b".repeat(64);
    let m1 = store
        .create_media(
            user,
            &shared,
            "x.png",
            "image/png",
            MediaKind::Image,
            10,
            NOW,
        )
        .unwrap();
    store
        .create_media(
            user,
            &unique,
            "y.png",
            "image/png",
            MediaKind::Image,
            10,
            NOW,
        )
        .unwrap();
    store.link_card_media(user, ids[0], m1).unwrap();
    store
        .create_web_session("sess-hash", user, "", NOW, NOW + 1000)
        .unwrap();
    store
        .create_password_reset("reset-hash", user, NOW, NOW + 1000)
        .unwrap();
    store
        .add_passkey(user, b"cred", "{}", "phone", NOW)
        .unwrap();
    store
        .create_oauth_client(
            "client-1",
            "Claude",
            &["https://x/cb".to_string()],
            "{}",
            NOW,
        )
        .unwrap();
    store
        .insert_oauth_token(
            "acc",
            "ref",
            "client-1",
            user,
            "study",
            "",
            None,
            NOW + 1000,
            NOW + 2000,
            NOW,
        )
        .unwrap();

    // The survivor references the shared blob and has a review of its own.
    let odeck = store.create_deck(other, "Keep", "", NOW).unwrap();
    let oids = store
        .create_cards(other, odeck, &[card("k", "v", &[])], None, NOW)
        .unwrap();
    store
        .create_media(
            other,
            &shared,
            "x.png",
            "image/png",
            MediaKind::Image,
            10,
            NOW,
        )
        .unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(other, oids[0]).unwrap(),
            Rating::Good,
            NOW,
        )
        .unwrap();
    store
        .record_review(other, oids[0], &outcome, NOW, "web", None, None)
        .unwrap();

    for who in [user, other] {
        store
            .observe_locale(who, flash_store::LocaleSource::Browser, "es-MX", None, NOW)
            .unwrap();
    }

    let deleted = store.delete_user_account(user).unwrap();
    assert!(store.locale_signals(user).unwrap().is_empty());
    assert_eq!(store.locale_signals(other).unwrap().len(), 1);
    assert_eq!(
        deleted.orphan_blobs,
        vec![unique.clone()],
        "only the unshared hash is orphaned"
    );

    assert!(matches!(
        store.get_user_info(user),
        Err(StoreError::NotFound(_))
    ));
    assert!(store.get_settings(user).is_err());
    assert!(store.list_decks(user, Due::exact(NOW)).unwrap().is_empty());
    assert!(store.passkeys_for_user(user).unwrap().is_empty());
    assert!(store
        .lookup_web_session("sess-hash", NOW)
        .unwrap()
        .is_none());
    assert!(
        !store.email_taken("t@example.com").unwrap(),
        "the address is free again"
    );
    let admin_rows = store.list_users_admin(NOW).unwrap();
    assert_eq!(admin_rows.len(), 1);
    assert_eq!(admin_rows[0].id, other);
    assert_eq!(
        admin_rows[0].review_count, 1,
        "the survivor's review log is intact"
    );
    assert_eq!(admin_rows[0].card_count, 1);

    // Deleting again is a clean not-found, and the survivor's log is still
    // protected by the trigger.
    assert!(matches!(
        store.delete_user_account(user),
        Err(StoreError::NotFound(_))
    ));
    let err = store.raw_execute_for_tests("DELETE FROM review_log");
    assert!(err.unwrap_err().to_string().contains("immutable"));

    assert_eq!(store.admin_count().unwrap(), 0);
    store.create_user("Boss", None, "admin", NOW).unwrap();
    assert_eq!(store.admin_count().unwrap(), 1);
}

/// An emailed link and a phone's folder name an account by its id alone,
/// so the newest account's id is not given to the next one after it goes.
#[test]
fn a_deleted_accounts_id_is_never_given_again() {
    let (store, _) = store_with_user();
    let newest = store
        .create_user("Newest", Some("n@example.com"), "member", NOW)
        .unwrap();
    store.delete_user_account(newest).unwrap();
    let next = store
        .create_user("Next", Some("x@example.com"), "member", NOW)
        .unwrap();
    assert!(next.raw() > newest.raw(), "{next:?} after {newest:?}");
}

#[test]
fn queue_scoping_by_deck_and_tag() {
    let (store, user) = store_with_user();
    let pharm = store.create_deck(user, "Pharm", "", NOW).unwrap();
    let anatomy = store.create_deck(user, "Anatomy", "", NOW).unwrap();
    store
        .create_cards(user, pharm, &[card("p1", "b", &["exam-2"])], None, NOW)
        .unwrap();
    store
        .create_cards(user, anatomy, &[card("a1", "b", &[])], None, NOW)
        .unwrap();

    assert_eq!(
        store
            .queue_entries(user, &StudyScope::all(), Due::exact(NOW), u32::MAX)
            .unwrap()
            .0
            .len(),
        2
    );
    assert_eq!(
        store
            .queue_entries(user, &StudyScope::deck(anatomy), Due::exact(NOW), u32::MAX)
            .unwrap()
            .0
            .len(),
        1
    );
    assert_eq!(
        store
            .queue_entries(
                user,
                &StudyScope::tagged(["EXAM-2"]),
                Due::exact(NOW),
                u32::MAX
            )
            .unwrap()
            .0
            .len(),
        1,
        "tag matching is case-insensitive"
    );
}

/// Several tags narrow each other: the session is the cards carrying
/// all of them, and a deck narrows that again.
#[test]
fn a_tag_scope_takes_the_cards_carrying_every_tag() {
    let (store, user) = store_with_user();
    let pharm = store.create_deck(user, "Pharm", "", NOW).unwrap();
    let anatomy = store.create_deck(user, "Anatomy", "", NOW).unwrap();
    store
        .create_cards(
            user,
            pharm,
            &[
                card("both", "b", &["nclex", "cardio"]),
                card("one", "b", &["nclex"]),
            ],
            None,
            NOW,
        )
        .unwrap();
    store
        .create_cards(user, anatomy, &[card("a1", "b", &["cardio"])], None, NOW)
        .unwrap();
    let n = |scope: StudyScope| {
        store
            .queue_entries(user, &scope, Due::exact(NOW), u32::MAX)
            .unwrap()
            .0
            .len()
    };

    assert_eq!(n(StudyScope::tagged(["nclex"])), 2);
    assert_eq!(n(StudyScope::tagged(["cardio"])), 2);
    assert_eq!(
        n(StudyScope::tagged(["nclex", "cardio"])),
        1,
        "every tag must be on the card, not just one of them"
    );
    assert_eq!(
        n(StudyScope::tagged(["NCLEX", " Cardio "])),
        1,
        "the names are normalized before they are matched"
    );
    assert_eq!(
        n(StudyScope::tagged(["cardio", "cardio"])),
        2,
        "a repeated tag asks for no more than the tag"
    );
    assert_eq!(
        n(StudyScope::new(Some(anatomy), ["cardio"])),
        1,
        "a deck and tags narrow each other"
    );
    assert_eq!(n(StudyScope::new(Some(anatomy), ["nclex"])), 0);
    assert_eq!(
        n(StudyScope::tagged(["nclex", "nope"])),
        0,
        "a tag nothing carries empties the session"
    );
}

/// The picker's rows: the tags on cards the user could actually study,
/// with counts, and only the ones inside the deck when it names one.
#[test]
fn list_tags_counts_the_live_cards_in_scope() {
    let (store, user) = store_with_user();
    let pharm = store.create_deck(user, "Pharm", "", NOW).unwrap();
    let sub = store.create_deck(user, "Pharm::Beta", "", NOW).unwrap();
    let anatomy = store.create_deck(user, "Anatomy", "", NOW).unwrap();
    store
        .create_cards(
            user,
            pharm,
            &[
                card("p1", "b", &["nclex", "cardio"]),
                card("p2", "b", &["nclex"]),
            ],
            None,
            NOW,
        )
        .unwrap();
    store
        .create_cards(user, sub, &[card("s1", "b", &["cardio"])], None, NOW)
        .unwrap();
    store
        .create_cards(user, anatomy, &[card("a1", "b", &["bones"])], None, NOW)
        .unwrap();
    let named = |deck| {
        store
            .list_tags(user, deck)
            .unwrap()
            .into_iter()
            .map(|t| (t.name, t.cards))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        named(None),
        vec![
            ("bones".to_string(), 1),
            ("cardio".to_string(), 2),
            ("nclex".to_string(), 2)
        ],
        "by name, counting every deck"
    );
    assert_eq!(
        named(Some(pharm)),
        vec![("cardio".to_string(), 2), ("nclex".to_string(), 2)],
        "a deck's tags hold its subdecks'"
    );
    assert_eq!(named(Some(anatomy)), vec![("bones".to_string(), 1)]);

    // What the user cannot study is not offered as a filter.
    let deleted = store.list_cards(user, Some(anatomy), None, 10).unwrap()[0].id;
    store.delete_card(user, deleted, NOW).unwrap();
    assert_eq!(named(Some(anatomy)), vec![], "a deleted card takes its tag");
    store.set_deck_archived(user, sub, Some(NOW)).unwrap();
    assert_eq!(
        named(Some(pharm)),
        vec![("cardio".to_string(), 1), ("nclex".to_string(), 2)],
        "an archived subdeck's cards stop counting"
    );

    // Another account's tags are not this one's, and neither is its deck.
    let other = store
        .create_user("Other", Some("o@example.com"), "member", NOW)
        .unwrap();
    assert_eq!(store.list_tags(other, None).unwrap(), vec![]);
    assert!(store.list_tags(other, Some(pharm)).is_err());
}

/// A deck's name is a path. Creating `Bio::Cells::Organelles` creates the
/// decks it sits in; a deck's scope and its card list hold its subdecks,
/// and every queue entry still names the deck the card lives in, which is
/// the deck whose budget it spends.
#[test]
fn a_deck_scope_holds_its_subdecks() {
    let (store, user) = store_with_user();
    let organelles = store
        .create_deck(user, " Bio :: Cells::Organelles ", "deep", NOW)
        .unwrap();
    let decks = store.list_decks(user, Due::exact(NOW)).unwrap();
    let names: Vec<(&str, &str)> = decks
        .iter()
        .map(|d| (d.name.as_str(), d.description.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Bio", ""),
            ("Bio::Cells", ""),
            ("Bio::Cells::Organelles", "deep")
        ],
        "the path is cleaned, and the decks on the way are made with it"
    );
    let id_of = |name: &str| store.find_deck_by_name(user, name).unwrap().unwrap();
    let (bio, cells) = (id_of("Bio"), id_of("Bio::Cells"));
    assert_eq!(id_of(" Bio::Cells :: Organelles"), organelles);
    // Decks that only look related: a longer first segment, and another
    // case. Neither is under "Bio".
    let bio2 = store.create_deck(user, "Bio2::Cells", "", NOW).unwrap();
    let lower = store.create_deck(user, "bio::cells", "", NOW).unwrap();

    for (deck, front) in [
        (bio, "b"),
        (cells, "c"),
        (organelles, "o"),
        (bio2, "x"),
        (lower, "y"),
    ] {
        store
            .create_cards(user, deck, &[card(front, "back", &[])], None, NOW)
            .unwrap();
    }
    let in_scope = |deck| {
        let mut decks: Vec<_> = store
            .queue_entries(user, &StudyScope::deck(deck), Due::exact(NOW), u32::MAX)
            .unwrap()
            .0
            .into_iter()
            .map(|e| e.deck_id)
            .collect();
        decks.sort();
        decks
    };
    let mut whole = vec![bio, cells, organelles];
    whole.sort();
    assert_eq!(in_scope(bio), whole);
    let mut inner = vec![cells, organelles];
    inner.sort();
    assert_eq!(in_scope(cells), inner);
    assert_eq!(in_scope(organelles), vec![organelles]);
    assert_eq!(in_scope(bio2), vec![bio2]);

    assert_eq!(store.count_cards(user, Some(bio), None).unwrap(), 3);
    assert_eq!(store.count_cards(user, Some(cells), None).unwrap(), 2);
    let fronts: Vec<String> = store
        .list_cards(user, Some(cells), None, 50)
        .unwrap()
        .into_iter()
        .map(|c| c.front)
        .collect();
    assert_eq!(
        fronts,
        vec!["o", "c"],
        "newest first, the subdeck's included"
    );

    // An archived subdeck is out of its parent's scope too, and takes
    // the deck under it along.
    store.set_deck_archived(user, cells, Some(NOW)).unwrap();
    assert_eq!(in_scope(bio), vec![bio]);

    // Someone else's deck scopes nothing and lists nothing.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(store
        .queue_entries(other, &StudyScope::deck(bio), Due::exact(NOW), u32::MAX)
        .unwrap()
        .0
        .is_empty());
    assert_eq!(store.count_cards(other, Some(bio), None).unwrap(), 0);
}

/// A subtree is matched on whole segments and exactly: `%` and `_` in a
/// name are characters, not wildcards.
#[test]
fn a_wildcard_in_a_deck_name_matches_only_itself() {
    let (store, user) = store_with_user();
    let percent = store.create_deck(user, "A%", "", NOW).unwrap();
    let underscore = store.create_deck(user, "A_", "", NOW).unwrap();
    let a1x = store.create_deck(user, "A1::x", "", NOW).unwrap();
    store
        .create_cards(user, a1x, &[card("f", "b", &[])], None, NOW)
        .unwrap();
    for deck in [percent, underscore] {
        assert!(store
            .queue_entries(user, &StudyScope::deck(deck), Due::exact(NOW), u32::MAX)
            .unwrap()
            .0
            .is_empty());
        assert_eq!(store.count_cards(user, Some(deck), None).unwrap(), 0);
    }
}

/// The cap counts the decks a path makes on the way, all or nothing, and
/// nothing is made inside an archived deck.
#[test]
fn a_path_meets_the_cap_and_the_archive_as_a_whole() {
    let (store, user) = store_with_user();
    for i in 0..flash_store::MAX_DECKS_PER_USER - 2 {
        store
            .create_deck(user, &format!("deck {i}"), "", NOW)
            .unwrap();
    }
    let err = store.create_deck(user, "A::B::C", "", NOW).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("at most 500 decks")),
        "{err:?}"
    );
    assert_eq!(
        store.deck_count(user).unwrap(),
        flash_store::MAX_DECKS_PER_USER - 2,
        "three decks do not fit in two places, so none was made"
    );
    let b = store.create_deck(user, "A::B", "", NOW).unwrap();
    assert_eq!(
        store.deck_count(user).unwrap(),
        flash_store::MAX_DECKS_PER_USER
    );

    // The same path twice is a duplicate name, and says so.
    store.delete_deck(user, b).unwrap();
    store.create_deck(user, "A::B", "", NOW).unwrap();
    let err = store.create_deck(user, "A :: B", "", NOW).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("already exists")),
        "{err:?}"
    );

    let (store, user) = store_with_user();
    let old = store.create_deck(user, "Old", "", NOW).unwrap();
    store.set_deck_archived(user, old, Some(NOW)).unwrap();
    let err = store.create_deck(user, "Old::Notes", "", NOW).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("\"Old\" is archived")),
        "{err:?}"
    );
    assert_eq!(store.deck_count(user).unwrap(), 1);

    let long = format!("A::{}", "x".repeat(flash_core::deck_path::MAX_LEN));
    let err = store.create_deck(user, &long, "", NOW).unwrap_err();
    assert!(
        matches!(&err, StoreError::Invalid(m) if m.contains("longer than 200 bytes")),
        "{err:?}"
    );
}

/// Archiving a deck takes its cards out of every scope and its counts to
/// zero without touching the cards; unarchiving hands everything back.
#[test]
fn an_archived_deck_is_out_of_the_queue_and_the_counts() {
    let (store, user) = store_with_user();
    // "Anatomy" sorts before "Pharm": archiving it must not move it.
    let anatomy = store.create_deck(user, "Anatomy", "", NOW).unwrap();
    let pharm = store.create_deck(user, "Pharm", "", NOW).unwrap();
    let old = store
        .create_cards(user, anatomy, &[card("a1", "b", &["exam-2"])], None, NOW)
        .unwrap();
    store
        .create_cards(user, pharm, &[card("p1", "b", &[])], None, NOW)
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(user, old[0]).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, NOW).unwrap();
    store
        .record_review(user, old[0], &outcome, NOW, "web", None, None)
        .unwrap();
    // A month on, the reviewed card is due and the other is still new.
    let later = NOW + 30 * DAY;
    let rows = |decks: Vec<flash_store::DeckSummary>| -> Vec<(String, u32, u32, Option<i64>)> {
        decks
            .into_iter()
            .map(|d| (d.name, d.due_count, d.new_count, d.archived_at))
            .collect()
    };
    let active = vec![
        ("Anatomy".to_string(), 1, 0, None),
        ("Pharm".to_string(), 0, 1, None),
    ];
    assert_eq!(
        rows(store.list_decks(user, Due::exact(later)).unwrap()),
        active
    );
    assert_eq!(
        store
            .queue_entries(user, &StudyScope::all(), Due::exact(later), u32::MAX)
            .unwrap()
            .0
            .len(),
        2
    );
    assert_eq!(store.due_between(user, NOW, later + DAY).unwrap().len(), 1);

    store.set_deck_archived(user, anatomy, Some(later)).unwrap();
    let in_scope = |scope: StudyScope| {
        store
            .queue_entries(user, &scope, Due::exact(later), u32::MAX)
            .unwrap()
            .0
            .len()
    };
    assert_eq!(in_scope(StudyScope::all()), 1, "only Pharm's new card");
    assert_eq!(in_scope(StudyScope::deck(anatomy)), 0, "not even by name");
    assert_eq!(in_scope(StudyScope::tagged(["exam-2"])), 0, "nor by tag");
    assert_eq!(
        rows(store.list_decks(user, Due::exact(later)).unwrap()),
        vec![
            ("Anatomy".to_string(), 0, 0, Some(later)),
            ("Pharm".to_string(), 0, 1, None),
        ],
        "archived in place, counts zero"
    );
    assert!(store
        .due_between(user, NOW, later + DAY)
        .unwrap()
        .is_empty());
    // The cards are untouched: still owned, still counted, history kept.
    assert_eq!(store.card_totals(user).unwrap().0, 2);
    assert_eq!(store.get_card_state(user, old[0]).unwrap().reps, 1);
    assert_eq!(total_reviews(&store, user), 1);

    // Archiving twice is nothing; unarchiving restores every number.
    store.set_deck_archived(user, anatomy, Some(later)).unwrap();
    store.set_deck_archived(user, anatomy, None).unwrap();
    assert_eq!(
        rows(store.list_decks(user, Due::exact(later)).unwrap()),
        active
    );
    assert_eq!(in_scope(StudyScope::all()), 2);
    assert_eq!(store.due_between(user, NOW, later + DAY).unwrap().len(), 1);

    // Someone else's deck is not found, not silently archived.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(matches!(
        store.set_deck_archived(other, anatomy, Some(later)),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(
        store.list_decks(user, Due::exact(later)).unwrap()[0].archived_at,
        None
    );
}

#[test]
fn sessions_and_stats() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[card("f1", "b", &[]), card("f2", "b", &[])],
            None,
            NOW,
        )
        .unwrap();
    let session = store
        .start_session(user, &StudyScope::deck(deck), NOW)
        .unwrap();
    assert_eq!(
        store.get_session_scope(user, session).unwrap(),
        StudyScope::deck(deck)
    );

    let scheduler = Scheduler::new(None, 0.9).unwrap();
    for (i, &rating) in [Rating::Good, Rating::Again].iter().enumerate() {
        let state = store.get_card_state(user, ids[i]).unwrap();
        let outcome = scheduler.review(&state, rating, NOW).unwrap();
        store
            .record_review(user, ids[i], &outcome, NOW, "mcp", None, Some(session))
            .unwrap();
    }
    let stats = store.session_stats(user, session).unwrap();
    assert_eq!((stats.reviewed, stats.good, stats.again), (2, 1, 1));

    store.end_session(user, session, NOW + 60_000).unwrap();
    // Another user's session id must not resolve.
    let other = store.create_user("Other", None, "member", NOW).unwrap();
    assert!(matches!(
        store.get_session_scope(other, session),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn cross_user_isolation() {
    let (store, alice) = store_with_user();
    let bob = store.create_user("Bob", None, "member", NOW).unwrap();
    let deck = store.create_deck(alice, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(alice, deck, &[card("f", "b", &[])], None, NOW)
        .unwrap();

    assert!(store.get_card(bob, ids[0]).unwrap().is_none());
    assert!(store.get_card_state(bob, ids[0]).is_err());
    assert!(store
        .queue_entries(bob, &StudyScope::all(), Due::exact(NOW), u32::MAX)
        .unwrap()
        .0
        .is_empty());
    assert!(store.delete_card(bob, ids[0], NOW).is_err());

    // A review recorded under Bob's name against Alice's card touches
    // nothing: the state row is guarded by the card's owner.
    let before = store.get_card_state(alice, ids[0]).unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let outcome = scheduler.review(&before, Rating::Good, NOW).unwrap();
    assert!(store
        .record_review(bob, ids[0], &outcome, NOW + 1, "web", None, None)
        .is_err());
    assert_eq!(store.get_card_state(alice, ids[0]).unwrap(), before);

    // Tags: Bob can neither attach to nor read Alice's card.
    store
        .with_conn(|conn| {
            flash_store::ext::attach_tag(conn, bob, ids[0], "stolen")?;
            assert!(flash_store::ext::card_tags(conn, bob, ids[0])?.is_empty());
            assert!(flash_store::ext::card_tags(conn, alice, ids[0])?.is_empty());
            Ok(())
        })
        .unwrap();
}

#[test]
fn web_session_lookup_carries_role() {
    let store = Store::open_in_memory().unwrap();
    let admin = store.create_user("Admin", None, "admin", NOW).unwrap();
    store
        .create_web_session("hash-a", admin, "", NOW, NOW + 1000)
        .unwrap();

    let hit = store.lookup_web_session("hash-a", NOW).unwrap().unwrap();
    assert_eq!((hit.user, hit.role.as_str()), (admin, "admin"));
    // Expired session resolves to nothing.
    assert!(store
        .lookup_web_session("hash-a", NOW + 1000)
        .unwrap()
        .is_none());
}

#[test]
fn email_uniqueness_and_lookup() {
    let store = Store::open_in_memory().unwrap();
    store
        .create_user("A", Some("a@example.com"), "member", NOW)
        .unwrap();
    assert!(store.email_taken("a@example.com").unwrap());
    assert!(!store.email_taken("b@example.com").unwrap());
    // UNIQUE column rejects a duplicate outright.
    assert!(store
        .create_user("B", Some("a@example.com"), "member", NOW)
        .is_err());
    // Multiple email-less users are fine (NULLs are distinct).
    store.create_user("C", None, "member", NOW).unwrap();
    store.create_user("D", None, "member", NOW).unwrap();
}

#[test]
fn admin_user_listing_aggregates_activity() {
    let store = Store::open_in_memory().unwrap();
    let admin = store
        .create_user("Admin", Some("admin@example.com"), "admin", NOW)
        .unwrap();
    let flashtester = store
        .create_user("FlashTester", None, "member", NOW + 1)
        .unwrap();

    // Admin: a deck, two cards, one review, a used passkey, a live grant.
    let deck = store.create_deck(admin, "Pharm", "", NOW).unwrap();
    let ids = store
        .create_cards(
            admin,
            deck,
            &[card("f1", "b1", &[]), card("f2", "b2", &[])],
            None,
            NOW,
        )
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(admin, ids[0]).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, NOW).unwrap();
    store
        .record_review(admin, ids[0], &outcome, NOW + 500, "mcp", None, None)
        .unwrap();
    store
        .add_passkey(admin, b"cred", "{}", "phone", NOW)
        .unwrap();
    store.touch_passkey(admin, 1, NOW + 900).unwrap();
    store
        .create_oauth_client("cid", "Claude", &[], "{}", NOW)
        .unwrap();
    store
        .create_oauth_client("cid2", "ChatGPT", &[], "{}", NOW)
        .unwrap();
    // Two live grants on different clients, plus a second Claude grant
    // that must not produce a duplicate name.
    store
        .insert_oauth_token(
            "ah",
            "rh",
            "cid",
            admin,
            "flash",
            "",
            None,
            NOW + 10_000,
            NOW + 20_000,
            NOW,
        )
        .unwrap();
    store
        .insert_oauth_token(
            "ah3",
            "rh3",
            "cid",
            admin,
            "flash",
            "",
            None,
            NOW + 10_000,
            NOW + 20_000,
            NOW,
        )
        .unwrap();
    store
        .insert_oauth_token(
            "ah4",
            "rh4",
            "cid2",
            admin,
            "flash",
            "",
            None,
            NOW + 10_000,
            NOW + 20_000,
            NOW,
        )
        .unwrap();
    // FlashTester: an expired grant only; must not show as connected. Her last
    // login comes from her session row (no passkey use recorded).
    store
        .insert_oauth_token(
            "ah2",
            "rh2",
            "cid",
            flashtester,
            "flash",
            "",
            None,
            NOW - 2000,
            NOW - 1000,
            NOW - 5000,
        )
        .unwrap();
    store
        .create_web_session("hash-l", flashtester, "", NOW + 700, NOW + 100_000)
        .unwrap();

    let rows = store.list_users_admin(NOW + 1000).unwrap();
    assert_eq!(rows.len(), 2);

    let a = &rows[0];
    assert_eq!(
        (a.display_name.as_str(), a.role.as_str()),
        ("Admin", "admin")
    );
    assert_eq!(a.email.as_deref(), Some("admin@example.com"));
    assert_eq!((a.deck_count, a.card_count, a.review_count), (1, 2, 1));
    assert_eq!(a.last_review_at, Some(NOW + 500));
    assert_eq!(a.last_review_source.as_deref(), Some("mcp"));
    assert_eq!(a.last_login_at, Some(NOW + 900));
    assert_eq!(a.mcp_clients, vec!["ChatGPT", "Claude"]);

    let l = &rows[1];
    assert_eq!(
        (l.display_name.as_str(), l.role.as_str()),
        ("FlashTester", "member")
    );
    assert_eq!(l.email, None);
    assert_eq!((l.deck_count, l.card_count, l.review_count), (0, 0, 0));
    assert_eq!(l.last_review_at, None);
    assert_eq!(l.last_login_at, Some(NOW + 700));
    assert!(l.mcp_clients.is_empty());
}

#[test]
fn review_and_card_totals() {
    let (store, user) = store_with_user();
    assert!(store.review_buckets(user).unwrap().is_empty());
    assert_eq!(store.card_totals(user).unwrap(), (0, 0));

    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[card("f1", "b1", &[]), card("f2", "b2", &[])],
            None,
            NOW,
        )
        .unwrap();
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let state = store.get_card_state(user, ids[0]).unwrap();
    let outcome = scheduler.review(&state, Rating::Good, NOW).unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(user, ids[0]).unwrap(),
            Rating::Good,
            NOW + 1000,
        )
        .unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW + 1000, "web", None, None)
        .unwrap();

    // A second apart is one quarter-hour bucket; the next quarter hour
    // is its own, and buckets come back oldest first.
    let bucket = NOW / 900_000 * 900_000;
    assert_eq!(store.review_buckets(user).unwrap(), vec![(bucket, 2)]);
    store
        .record_review(user, ids[0], &outcome, bucket + 900_000, "web", None, None)
        .unwrap();
    assert_eq!(
        store.review_buckets(user).unwrap(),
        vec![(bucket, 2), (bucket + 900_000, 1)]
    );

    // Force one card mature: review phase with 21d+ stability.
    store
        .raw_execute_for_tests("UPDATE card_state SET phase = 2, stability = 25.0")
        .unwrap();
    assert_eq!(store.card_totals(user).unwrap(), (2, 2));
    store
        .raw_execute_for_tests("UPDATE card_state SET stability = 5.0")
        .unwrap();
    assert_eq!(store.card_totals(user).unwrap(), (2, 0));
}

// ---- the insert-time cap ----

#[test]
fn create_cards_cap_rejects_atomically() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    store
        .create_cards(
            user,
            deck,
            &[card("f1", "b", &[]), card("f2", "b", &[])],
            Some(3),
            NOW,
        )
        .unwrap();
    let err = store
        .create_cards(
            user,
            deck,
            &[card("f3", "b", &["t"]), card("f4", "b", &[])],
            Some(3),
            NOW,
        )
        .unwrap_err();
    match err {
        StoreError::CapExceeded { current, cap } => {
            assert_eq!((current, cap), (2, 3));
        }
        other => panic!("expected CapExceeded, got {other:?}"),
    }
    // Nothing from the rejected batch landed.
    assert_eq!(store.count_cards(user, None, None).unwrap(), 2);
    // A batch that fits exactly still goes through.
    store
        .create_cards(user, deck, &[card("f3", "b", &[])], Some(3), NOW)
        .unwrap();
    assert_eq!(store.count_cards(user, None, None).unwrap(), 3);
}

#[test]
fn card_count_ignores_deleted_cards() {
    let (store, user) = store_with_user();
    let deck = store.create_deck(user, "D", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            deck,
            &[card("f1", "b", &[]), card("f2", "b", &[])],
            None,
            NOW,
        )
        .unwrap();
    store.delete_card(user, ids[0], NOW).unwrap();
    assert_eq!(store.count_cards(user, None, None).unwrap(), 1);
}

// ---- invites ----

#[test]
fn invite_lookup_and_delete() {
    let store = Store::open_in_memory().unwrap();
    store
        .create_invite("hash1", "Ada", "member", None, NOW + 1000, NOW)
        .unwrap();
    let invite = store.lookup_invite("hash1", NOW).unwrap().unwrap();
    assert_eq!(invite.display_name, "Ada");
    assert_eq!(invite.role, "member");
    // Expired invites don't resolve.
    assert!(store.lookup_invite("hash1", NOW + 1000).unwrap().is_none());

    store.delete_invite("hash1").unwrap();
    assert!(store.lookup_invite("hash1", NOW).unwrap().is_none());
}

// ---- password auth ----

#[test]
fn password_hash_crud_and_user_by_email() {
    let (store, user) = store_with_user();
    assert_eq!(store.get_password_hash(user).unwrap(), None);
    let by_email = store.user_by_email("t@example.com").unwrap().unwrap();
    assert_eq!(by_email.user, user);
    assert!(by_email.password_hash.is_none());

    store
        .set_password_hash(user, Some("$argon2id$fake"))
        .unwrap();
    assert_eq!(
        store.get_password_hash(user).unwrap().as_deref(),
        Some("$argon2id$fake")
    );
    store.set_password_hash(user, None).unwrap();
    assert_eq!(store.get_password_hash(user).unwrap(), None);
    assert!(store.user_by_email("nobody@example.com").unwrap().is_none());
}

#[test]
fn password_failures_window_and_clear() {
    let (store, user) = store_with_user();
    for _ in 0..3 {
        store.record_password_failure(user, NOW).unwrap();
    }
    let login = store.user_by_email("t@example.com").unwrap().unwrap();
    assert_eq!(login.pw_failed_count, 3);
    assert_eq!(login.pw_failed_at, Some(NOW));

    // A failure outside the window restarts the count.
    let later = NOW + flash_store::PW_LOCKOUT_WINDOW_MS + 1;
    store.record_password_failure(user, later).unwrap();
    let login = store.user_by_email("t@example.com").unwrap().unwrap();
    assert_eq!(login.pw_failed_count, 1);

    store.clear_password_failures(user).unwrap();
    let login = store.user_by_email("t@example.com").unwrap().unwrap();
    assert_eq!(login.pw_failed_count, 0);
    assert_eq!(login.pw_failed_at, None);
}

#[test]
fn password_reset_lifecycle() {
    let (store, user) = store_with_user();
    store
        .create_password_reset("rhash", user, NOW, NOW + 1000)
        .unwrap();
    assert_eq!(
        store.lookup_password_reset("rhash", NOW).unwrap(),
        Some(user)
    );
    assert_eq!(
        store.lookup_password_reset("rhash", NOW + 1001).unwrap(),
        None,
        "expired"
    );
    assert_eq!(store.recent_password_resets(user, NOW - 1).unwrap(), 1);
    assert_eq!(store.recent_password_resets(user, NOW).unwrap(), 0);

    assert_eq!(store.consume_password_reset("rhash", NOW).unwrap(), user);
    assert!(
        store.consume_password_reset("rhash", NOW).is_err(),
        "single use"
    );
    assert_eq!(store.lookup_password_reset("rhash", NOW).unwrap(), None);

    store
        .create_password_reset("rhash2", user, NOW, NOW + 1000)
        .unwrap();
    store.delete_password_reset("rhash2").unwrap();
    assert_eq!(store.lookup_password_reset("rhash2", NOW).unwrap(), None);

    store
        .create_password_reset("rhash3", user, NOW, NOW + 1)
        .unwrap();
    assert_eq!(store.purge_expired_password_resets(NOW + 2).unwrap(), 1);
}

#[test]
fn delete_sessions_for_user_honors_keep() {
    let (store, user) = store_with_user();
    let other = store.create_user("Other", None, "member", NOW).unwrap();
    store
        .create_web_session("s1", user, "", NOW, NOW + 1000)
        .unwrap();
    store
        .create_web_session("s2", user, "", NOW, NOW + 1000)
        .unwrap();
    store
        .create_web_session("s3", other, "", NOW, NOW + 1000)
        .unwrap();

    assert_eq!(store.delete_sessions_for_user(user, Some("s2")).unwrap(), 1);
    assert!(store.lookup_web_session("s1", NOW).unwrap().is_none());
    assert!(store.lookup_web_session("s2", NOW).unwrap().is_some());
    assert!(
        store.lookup_web_session("s3", NOW).unwrap().is_some(),
        "other user untouched"
    );

    assert_eq!(store.delete_sessions_for_user(user, None).unwrap(), 1);
    assert!(store.lookup_web_session("s2", NOW).unwrap().is_none());
}

/// The names of the user's decks, in the store's order.
fn deck_names(store: &Store, user: flash_core::UserId) -> Vec<String> {
    store
        .list_decks(user, Due::exact(NOW))
        .unwrap()
        .into_iter()
        .map(|d| d.name)
        .collect()
}

/// A rename is a move: the decks under the deck follow it, the deck it
/// lands in is made when there is none, and every way it could go wrong
/// leaves every name as it was.
#[test]
fn a_rename_carries_the_subtree() {
    let (store, user) = store_with_user();
    let cells = store.create_deck(user, "Bio::Cells", "", NOW).unwrap();
    store
        .create_deck(user, "Bio::Cells::Organelles", "", NOW)
        .unwrap();
    let bio = store.find_deck_by_name(user, "Bio").unwrap().unwrap();
    let chem = store.create_deck(user, "Chem", "", NOW).unwrap();

    store
        .rename_deck(user, cells, " School :: Cytology ", NOW)
        .unwrap();
    assert_eq!(
        deck_names(&store, user),
        vec![
            "Bio",
            "Chem",
            "School",
            "School::Cytology",
            "School::Cytology::Organelles"
        ]
    );
    // The same path again is nothing.
    store
        .rename_deck(user, cells, "School::Cytology", NOW)
        .unwrap();

    // Moving keeps the deck's own name: under another deck, then back out.
    store.move_deck(user, cells, Some(chem), NOW).unwrap();
    assert!(deck_names(&store, user).contains(&"Chem::Cytology::Organelles".to_string()));
    store.move_deck(user, cells, None, NOW).unwrap();
    assert_eq!(
        deck_names(&store, user),
        vec!["Bio", "Chem", "Cytology", "Cytology::Organelles", "School"]
    );

    let before = deck_names(&store, user);
    let refused = |result: Result<(), StoreError>, needle: &str| {
        let err = result.unwrap_err();
        assert!(
            matches!(&err, StoreError::Invalid(m) if m.contains(needle)),
            "{err:?}"
        );
        assert_eq!(deck_names(&store, user), before, "nothing moved");
    };
    refused(store.rename_deck(user, cells, " :: ", NOW), "empty");
    refused(
        store.rename_deck(user, cells, "Cytology::Inner", NOW),
        "into itself",
    );
    let organelles = store
        .find_deck_by_name(user, "Cytology::Organelles")
        .unwrap();
    refused(store.move_deck(user, cells, organelles, NOW), "into itself");
    // Another deck has the name: two decks are never merged.
    refused(store.rename_deck(user, cells, "Bio", NOW), "already exists");
    // The bound is on the whole path, so a subdeck can be what breaks it.
    let long = "x".repeat(flash_core::deck_path::MAX_LEN - 5);
    refused(
        store.rename_deck(user, cells, &long, NOW),
        "\"Organelles\" longer than 200 bytes",
    );
    // Nothing active moves into an archived deck.
    store.set_deck_archived(user, bio, Some(NOW)).unwrap();
    refused(
        store.move_deck(user, cells, Some(bio), NOW),
        "\"Bio\" is archived",
    );

    // Someone else's deck is not found, as a deck or as a destination.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(matches!(
        store.rename_deck(other, cells, "Mine", NOW),
        Err(StoreError::NotFound(_))
    ));
    let theirs = store.create_deck(other, "Theirs", "", NOW).unwrap();
    assert!(matches!(
        store.move_deck(other, theirs, Some(chem), NOW),
        Err(StoreError::NotFound(_))
    ));
}

/// A deck whose parent is not a deck (a name from before names nested)
/// can take its parent's name: `A::A::A` must not trip over `A::A` while
/// both are on their way.
#[test]
fn a_rename_onto_its_own_old_names_goes_through() {
    let (store, user) = store_with_user();
    store
        .raw_execute_for_tests(&format!(
            "INSERT INTO decks (id, user_id, name, created_at)
             VALUES (901, {0}, 'A::A', 0), (902, {0}, 'A::A::A', 0)",
            user.raw()
        ))
        .unwrap();
    let inner = store.find_deck_by_name(user, "A::A").unwrap().unwrap();
    store.rename_deck(user, inner, "A", NOW).unwrap();
    assert_eq!(deck_names(&store, user), vec!["A", "A::A"]);
}

/// Archiving takes the decks under the deck with it; unarchiving brings
/// them back, and brings back the decks a subdeck sits in, so nothing
/// active is ever inside something archived.
#[test]
fn archive_covers_the_subtree_and_unarchive_lifts_the_ancestors() {
    let (store, user) = store_with_user();
    let organelles = store
        .create_deck(user, "Bio::Cells::Organelles", "", NOW)
        .unwrap();
    store.create_deck(user, "Bio::Plants", "", NOW).unwrap();
    store.create_deck(user, "Chem", "", NOW).unwrap();
    let id_of = |name: &str| store.find_deck_by_name(user, name).unwrap().unwrap();
    let archived = || -> Vec<(String, Option<i64>)> {
        store
            .list_decks(user, Due::exact(NOW))
            .unwrap()
            .into_iter()
            .filter(|d| d.archived_at.is_some())
            .map(|d| (d.name, d.archived_at))
            .collect()
    };

    // A subdeck archived earlier keeps its own date.
    store
        .set_deck_archived(user, id_of("Bio::Plants"), Some(NOW))
        .unwrap();
    store
        .set_deck_archived(user, id_of("Bio"), Some(NOW + DAY))
        .unwrap();
    assert_eq!(
        archived(),
        vec![
            ("Bio".to_string(), Some(NOW + DAY)),
            ("Bio::Cells".to_string(), Some(NOW + DAY)),
            ("Bio::Cells::Organelles".to_string(), Some(NOW + DAY)),
            ("Bio::Plants".to_string(), Some(NOW)),
        ]
    );

    // Unarchiving the innermost deck lifts the two it sits in, and leaves
    // the rest of what is under them archived.
    store.set_deck_archived(user, organelles, None).unwrap();
    assert_eq!(archived(), vec![("Bio::Plants".to_string(), Some(NOW))]);

    store
        .set_deck_archived(user, id_of("Bio"), Some(NOW + DAY))
        .unwrap();
    store.set_deck_archived(user, id_of("Bio"), None).unwrap();
    assert!(archived().is_empty(), "the whole subtree comes back");
}

/// Deleting a deck deletes the decks under it, and says how many.
#[test]
fn delete_deck_takes_the_subtree() {
    let (store, user) = store_with_user();
    let organelles = store
        .create_deck(user, "Bio::Cells::Organelles", "", NOW)
        .unwrap();
    let keeper = store.create_deck(user, "Bio2", "", NOW).unwrap();
    let id_of = |name: &str| store.find_deck_by_name(user, name).unwrap().unwrap();
    for deck in [id_of("Bio"), organelles, keeper] {
        store
            .create_cards(
                user,
                deck,
                &[card("f", "b", &[]), card("g", "b", &[])],
                None,
                NOW,
            )
            .unwrap();
    }
    let gone = store.delete_deck(user, id_of("Bio")).unwrap();
    assert_eq!((gone.decks, gone.cards), (3, 4));
    assert_eq!(deck_names(&store, user), vec!["Bio2"]);
    assert_eq!(store.count_cards(user, None, None).unwrap(), 2);
}

#[test]
fn delete_deck_removes_cards_and_frees_the_name() {
    let (store, user) = store_with_user();
    let doomed = store.create_deck(user, "Doomed", "", NOW).unwrap();
    let keeper = store.create_deck(user, "Keeper", "", NOW).unwrap();
    let ids = store
        .create_cards(
            user,
            doomed,
            &[
                card("a", "1", &["t"]),
                card("b", "2", &[]),
                card("c", "3", &[]),
            ],
            None,
            NOW,
        )
        .unwrap();
    store
        .create_cards(user, keeper, &[card("k", "v", &[])], None, NOW)
        .unwrap();
    // One reviewed (review_log row must survive), one soft-deleted (must
    // still be purged so the FK and name free up).
    let scheduler = Scheduler::new(None, 0.9).unwrap();
    let outcome = scheduler
        .review(
            &store.get_card_state(user, ids[0]).unwrap(),
            Rating::Good,
            NOW,
        )
        .unwrap();
    store
        .record_review(user, ids[0], &outcome, NOW, "web", None, None)
        .unwrap();
    store.delete_card(user, ids[1], NOW).unwrap();

    // Someone else can't delete it.
    let other = store.create_user("O", None, "member", NOW).unwrap();
    assert!(matches!(
        store.delete_deck(other, doomed),
        Err(StoreError::NotFound(_))
    ));

    assert_eq!(
        store.delete_deck(user, doomed).unwrap().cards,
        3,
        "soft-deleted card counted"
    );
    let decks = store.list_decks(user, Due::exact(NOW)).unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].id, keeper);
    assert_eq!(store.count_cards(user, None, None).unwrap(), 1);
    // Review history survives the purge.
    let admin_rows = store.list_users_admin(NOW).unwrap();
    assert_eq!(admin_rows[0].review_count, 1);
    // The name is free for reuse immediately.
    store.create_deck(user, "Doomed", "", NOW).unwrap();
    // Deleting again: clean not-found.
    assert!(matches!(
        store.delete_deck(user, doomed),
        Err(StoreError::NotFound(_))
    ));
}

/// An account's language: the newest learned signal while nobody chose,
/// a choice never replaced by a signal, and back to the newest signal
/// when the choice is cleared. Each signal is kept as first and last
/// seen, its last sighting moved at most once a day.
#[test]
fn an_account_language_is_learned_until_it_is_chosen() {
    use flash_store::LocaleSource::{App, Browser, Chosen, Host};
    let store = flash_store::Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    assert_eq!(store.account_locale(user).unwrap(), None);

    store
        .observe_locale(user, Browser, "en-US", Some("en-US,en"), NOW)
        .unwrap();
    assert!(store.adopt_locale(user, Browser, "en-US").unwrap());
    assert!(!store.adopt_locale(user, Browser, "en-US").unwrap());
    assert_eq!(
        store.account_locale(user).unwrap(),
        Some(("en-US".into(), Browser))
    );

    // A newer signal from elsewhere wins over an older learned one.
    store
        .observe_locale(user, Host, "es-ES", None, NOW + 10)
        .unwrap();
    assert!(store.adopt_locale(user, Host, "es-ES").unwrap());
    assert_eq!(
        store.account_locale(user).unwrap(),
        Some(("es-ES".into(), Host))
    );

    // A choice is never replaced by a signal.
    store.set_chosen_locale(user, "es-MX").unwrap();
    assert!(!store.adopt_locale(user, App, "pt-BR").unwrap());
    assert_eq!(
        store.account_locale(user).unwrap(),
        Some(("es-MX".into(), Chosen))
    );

    // Cleared, the account takes the newest signal kept.
    store.clear_chosen_locale(user).unwrap();
    assert_eq!(
        store.account_locale(user).unwrap(),
        Some(("es-ES".into(), Host))
    );

    // The same browser seen again within a day keeps its last sighting;
    // a day later, or a different language, moves it.
    let day = 86_400_000;
    store
        .observe_locale(user, Browser, "en-US", Some("en-US,en"), NOW + 1_000)
        .unwrap();
    let last = |s: &flash_store::Store| {
        s.locale_signals(user)
            .unwrap()
            .into_iter()
            .find(|x| x.source == Browser)
            .unwrap()
    };
    assert_eq!(last(&store).last_at, NOW);
    store
        .observe_locale(user, Browser, "en-US", Some("en-US,en"), NOW + day)
        .unwrap();
    assert_eq!(last(&store).last_at, NOW + day);
    store
        .observe_locale(user, Browser, "es-AR", Some("es-AR,es"), NOW + day + 5)
        .unwrap();
    let browser = last(&store);
    assert_eq!(browser.tag, "es-AR");
    assert_eq!(browser.prefs.as_deref(), Some("es-AR,es"));
    assert_eq!(browser.first_at, NOW);

    // A cleared choice with nothing learned goes back to the default.
    let fresh = store
        .create_user("Fresh", Some("fresh@example.com"), "member", NOW)
        .unwrap();
    store.set_chosen_locale(fresh, "en").unwrap();
    store.clear_chosen_locale(fresh).unwrap();
    assert_eq!(store.account_locale(fresh).unwrap(), None);
}

/// The zone says how it was learned: the default until a browser
/// reports one or someone chooses one, and a chosen zone (UTC
/// included) is never replaced by a browser's guess.
#[test]
fn a_time_zone_records_how_it_was_learned() {
    let store = flash_store::Store::open_in_memory().unwrap();
    let user = store
        .create_user(
            "FlashTester",
            Some("flashtester@example.com"),
            "member",
            NOW,
        )
        .unwrap();
    let source = || -> String {
        store
            .with_conn(|c| {
                Ok(c.query_row(
                    "SELECT timezone_source FROM user_settings WHERE user_id = ?1",
                    [user.raw()],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    };
    assert_eq!(source(), "default");
    assert!(store.adopt_timezone(user, "Europe/Rome").unwrap());
    assert_eq!(source(), "browser");
    store
        .set_study_settings(user, Some("UTC"), None, None, None)
        .unwrap();
    assert_eq!(source(), "chosen");
    assert!(!store.adopt_timezone(user, "Asia/Tokyo").unwrap());
    assert_eq!(store.get_settings(user).unwrap().timezone, "UTC");
}
