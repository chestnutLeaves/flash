use flash_core::queue::{
    normalize_tags, order_queue, Budgets, DeckBudget, Due, QueueEntry, StudyScope,
};
use flash_core::{CardId, DeckId, Phase};

fn entry(id: i64, phase: Phase, due_ms: i64) -> QueueEntry {
    entry_in(id, 1, phase, due_ms)
}

fn entry_in(id: i64, deck: i64, phase: Phase, due_ms: i64) -> QueueEntry {
    QueueEntry {
        card_id: CardId(id),
        deck_id: DeckId(deck),
        phase,
        due_ms,
    }
}

fn flat(new: u32) -> Budgets {
    Budgets::flat(new, u32::MAX)
}

#[test]
fn learning_before_review_before_new() {
    let now = 1_000_000;
    let entries = [
        entry(1, Phase::New, 0),
        entry(2, Phase::Review, now - 500),
        entry(3, Phase::Learning, now - 100),
        entry(4, Phase::Relearning, now - 200),
    ];
    let order = order_queue(&entries, Due::exact(now), &flat(10));
    assert_eq!(
        order,
        vec![CardId(4), CardId(3), CardId(2), CardId(1)],
        "relearning (older due) then learning, then review, then new"
    );
}

#[test]
fn future_cards_are_excluded_and_new_budget_applies() {
    let now = 1_000_000;
    let entries = [
        entry(1, Phase::Review, now + 1),        // not yet due
        entry(2, Phase::Learning, now + 60_000), // step timer not expired
        entry(3, Phase::New, 0),
        entry(4, Phase::New, 0),
        entry(5, Phase::New, 0),
    ];
    let order = order_queue(&entries, Due::exact(now), &flat(2));
    assert_eq!(order, vec![CardId(3), CardId(4)]);
}

#[test]
fn learn_ahead_only_when_nothing_else_is_due() {
    let now = 1_000_000;
    let soon = now + 5 * 60_000; // learning step expires in 5 min
                                 // Something else is due -> learn-ahead card excluded.
    let entries = [entry(1, Phase::Learning, soon), entry(2, Phase::New, 0)];
    assert_eq!(
        order_queue(&entries, Due::exact(now), &flat(10)),
        vec![CardId(2)]
    );

    // Nothing else due -> the upcoming learning card is pulled forward.
    let entries = [entry(1, Phase::Learning, soon)];
    assert_eq!(
        order_queue(&entries, Due::exact(now), &flat(10)),
        vec![CardId(1)]
    );

    // But not if it's beyond the 20-minute learn-ahead window.
    let entries = [entry(1, Phase::Learning, now + 60 * 60_000)];
    assert!(order_queue(&entries, Due::exact(now), &flat(10)).is_empty());
}

#[test]
fn by_day_a_review_is_due_for_its_whole_study_day() {
    let now = 1_000_000;
    let day_end = now + 10 * 60 * 60_000; // the study day ends in 10 hours
    let entries = [
        entry(1, Phase::Review, now + 3 * 60 * 60_000), // later today
        entry(2, Phase::Review, day_end),               // tomorrow's first moment
        entry(3, Phase::Review, now - 100),             // due already
    ];
    assert_eq!(
        order_queue(&entries, Due::by_day(now, day_end), &flat(0)),
        vec![CardId(3), CardId(1)],
        "later today joins the sitting, tomorrow does not"
    );
    assert_eq!(
        order_queue(&entries, Due::exact(now), &flat(0)),
        vec![CardId(3)],
        "exact timing waits for the due time"
    );
}

#[test]
fn by_day_learning_steps_stay_exact() {
    let now = 1_000_000;
    let day_end = now + 10 * 60 * 60_000;
    let due = Due::by_day(now, day_end);
    assert!(!due.is_due(Phase::Learning, now + 60 * 60_000));
    assert!(!due.is_due(Phase::Relearning, now + 60 * 60_000));
    assert!(due.is_due(Phase::Learning, now));
    // An hour-away step is past the learn-ahead window, so nothing comes.
    let entries = [entry(1, Phase::Learning, now + 60 * 60_000)];
    assert!(order_queue(&entries, due, &flat(10)).is_empty());
}

#[test]
fn overdue_reviews_come_oldest_first() {
    let now = 1_000_000;
    let entries = [
        entry(1, Phase::Review, now - 10),
        entry(2, Phase::Review, now - 999),
        entry(3, Phase::Review, now - 500),
    ];
    let order = order_queue(&entries, Due::exact(now), &flat(0));
    assert_eq!(order, vec![CardId(2), CardId(3), CardId(1)]);
}

#[test]
fn per_deck_new_limits_apply_under_the_account_total() {
    let now = 1_000_000;
    // Deck 1 has 3 new, deck 2 has 3 new; deck 1 may introduce 1, deck 2
    // is unlimited per-deck, account total is 3.
    let entries = [
        entry_in(1, 1, Phase::New, 0),
        entry_in(2, 1, Phase::New, 0),
        entry_in(3, 1, Phase::New, 0),
        entry_in(4, 2, Phase::New, 0),
        entry_in(5, 2, Phase::New, 0),
        entry_in(6, 2, Phase::New, 0),
    ];
    let mut budgets = Budgets::flat(3, u32::MAX);
    budgets.per_deck.insert(
        DeckId(1),
        DeckBudget {
            new: 1,
            reviews: u32::MAX,
        },
    );
    let order = order_queue(&entries, Due::exact(now), &budgets);
    assert_eq!(order, vec![CardId(1), CardId(4), CardId(5)]);

    // A deck whose remainder is zero introduces nothing even with total left.
    budgets.per_deck.insert(
        DeckId(2),
        DeckBudget {
            new: 0,
            reviews: u32::MAX,
        },
    );
    let order = order_queue(&entries, Due::exact(now), &budgets);
    assert_eq!(order, vec![CardId(1)]);
}

#[test]
fn a_paced_deck_introduces_its_pace_on_top_of_the_account_total() {
    let now = 1_000_000;
    let entries = [
        entry_in(1, 1, Phase::New, 0),
        entry_in(2, 1, Phase::New, 0),
        entry_in(3, 1, Phase::New, 0),
        entry_in(4, 2, Phase::New, 0),
        entry_in(5, 2, Phase::New, 0),
        entry_in(6, 2, Phase::New, 0),
    ];
    let mut budgets = Budgets::flat(2, u32::MAX);
    budgets.per_deck.insert(
        DeckId(2),
        DeckBudget {
            new: 2,
            reviews: u32::MAX,
        },
    );
    budgets.paced.insert(DeckId(2));
    let order = order_queue(&entries, Due::exact(now), &budgets);
    assert_eq!(order, vec![CardId(1), CardId(2), CardId(4), CardId(5)]);

    budgets.new_total = 0;
    let order = order_queue(&entries, Due::exact(now), &budgets);
    assert_eq!(order, vec![CardId(4), CardId(5)]);
}

#[test]
fn review_cap_limits_reviews_but_never_learning() {
    let now = 1_000_000;
    let entries = [
        entry_in(1, 1, Phase::Learning, now - 1),
        entry_in(2, 1, Phase::Relearning, now - 1),
        entry_in(3, 1, Phase::Review, now - 300),
        entry_in(4, 1, Phase::Review, now - 200),
        entry_in(5, 1, Phase::Review, now - 100),
    ];
    // Deck review cap 1: oldest review only; learning cards untouched.
    let mut budgets = Budgets::flat(0, u32::MAX);
    budgets
        .per_deck
        .insert(DeckId(1), DeckBudget { new: 0, reviews: 1 });
    assert_eq!(
        order_queue(&entries, Due::exact(now), &budgets),
        vec![CardId(1), CardId(2), CardId(3)]
    );
    // Account review total 2 caps across decks.
    let budgets = Budgets::flat(0, 2);
    assert_eq!(
        order_queue(&entries, Due::exact(now), &budgets),
        vec![CardId(1), CardId(2), CardId(3), CardId(4)]
    );
}

#[test]
fn a_scope_normalizes_the_tags_it_is_given() {
    let scope = StudyScope::tagged([" NCLEX ", "cardio", "", "CARDIO", "nclex"]);
    assert_eq!(
        scope.tags,
        ["nclex", "cardio"],
        "trimmed, lowercased, deduped"
    );
    assert_eq!(normalize_tags(["  "]), Vec::<String>::new());
    assert_eq!(
        normalize_tags(["Cafe\u{301}", "café"]),
        ["café"],
        "one Unicode form, so a decomposed accent is the same tag"
    );
    assert!(StudyScope::all().is_all());
    assert!(!StudyScope::deck(DeckId(3)).is_all());
    assert!(!StudyScope::tagged(["nclex"]).is_all());
}

#[test]
fn a_scope_round_trips_through_the_stored_form() {
    for scope in [
        StudyScope::all(),
        StudyScope::deck(DeckId(3)),
        StudyScope::tagged(["nclex"]),
        StudyScope::tagged(["nclex", "cardio"]),
        StudyScope::new(Some(DeckId(3)), ["nclex", "cardio"]),
        // A tag may hold any character, a comma included: the stored
        // form is JSON precisely so this survives.
        StudyScope::tagged(["a,b", "c\"d"]),
    ] {
        assert_eq!(StudyScope::parse(&scope.as_string()), scope);
    }
    // The shapes that predate tags keep their spelling, so a reader can
    // still tell what an old row meant.
    assert_eq!(StudyScope::all().as_string(), "all");
    assert_eq!(StudyScope::deck(DeckId(3)).as_string(), "deck:3");
}

#[test]
fn a_row_written_before_tags_still_reads() {
    assert_eq!(StudyScope::parse("all"), StudyScope::all());
    assert_eq!(StudyScope::parse("deck:7"), StudyScope::deck(DeckId(7)));
    assert_eq!(
        StudyScope::parse("tag:EXAM-2"),
        StudyScope::tagged(["exam-2"]),
        "an old tag row normalizes as a new one would"
    );
    // A row nothing can read studies everything rather than nothing: a
    // session that offers too much beats one that offers no cards.
    assert_eq!(StudyScope::parse("what"), StudyScope::all());
    assert_eq!(StudyScope::parse("{not json"), StudyScope::all());
    assert_eq!(StudyScope::parse("deck:x"), StudyScope::all());
}
