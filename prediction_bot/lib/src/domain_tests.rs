use super::*;
use googletest::{
    assert_that,
    matchers::{anything, contains_substring, eq, err, is_empty, ne},
};

const DEFAULTS: Policy = Policy {
    amount: Points(100),
    interval: 86_400,
};

fn member(user_id: u64) -> Actor {
    Actor {
        user_id: user_id.into(),
        moderator: false,
        bot: false,
    }
}

fn moderator(user_id: u64) -> Actor {
    Actor {
        moderator: true,
        ..member(user_id)
    }
}

fn execute(state: &mut State, actor: Actor, command: &Command, now: i64) -> Decision {
    let decision = decide(state, actor, command, now, DEFAULTS).unwrap();
    for event in &decision.events {
        apply(state, event).unwrap();
    }
    decision
}

fn market(state: &mut State, closes_at: i64) {
    execute(state, member(1), &Command::Join, 1_000);
    execute(
        state,
        member(1),
        &Command::Create {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            question: "Who wins?".to_owned(),
            options: vec!["Red".to_owned(), "Blue".to_owned()],
            closes_at,
        },
        1_000,
    );
}

#[googletest::test]
fn enrollment_grants_once_and_preserves_schedule() {
    let mut state = State::default();
    let first = execute(&mut state, member(7), &Command::Join, 1_000);
    assert_that!(first.events.len(), eq(3));
    assert_that!(state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(state.accounts[&UserId(7)].next_grant, eq(87_400));
    let repeat = execute(&mut state, member(7), &Command::Join, 2_000);
    assert_that!(repeat.events, is_empty());
    assert_that!(state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(state.accounts[&UserId(7)].next_grant, eq(87_400));
}

#[googletest::test]
fn grants_cover_exact_due_boundaries_after_offline_intervals() {
    let mut state = State::default();
    execute(&mut state, member(7), &Command::Join, 1_000);
    let early = execute(
        &mut state,
        member(0),
        &Command::Grant { user_id: UserId(7) },
        87_399,
    );
    assert_that!(early.events, is_empty());
    let due = execute(
        &mut state,
        member(0),
        &Command::Grant { user_id: UserId(7) },
        173_800,
    );
    assert_that!(due.events.len(), eq(1));
    assert_that!(state.accounts[&UserId(7)].balance.0, eq(300));
    assert_that!(state.accounts[&UserId(7)].next_grant, eq(260_200));
}

#[googletest::test]
fn settlement_distributes_pool_and_ties_by_numeric_user_id() {
    let mut state = State::default();
    market(&mut state, 2_000);
    for user in [2, 3, 4] {
        execute(&mut state, member(user), &Command::Join, 1_000);
    }
    for (user, outcome, amount) in [(2, 0, 1), (3, 0, 2), (4, 1, 2)] {
        execute(
            &mut state,
            member(user),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(outcome),
                amount: Points(amount),
            },
            1_500,
        );
    }
    execute(
        &mut state,
        moderator(1),
        &Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
        },
        2_000,
    );
    assert_that!(
        (
            state.accounts[&UserId(2)].balance.0,
            state.accounts[&UserId(3)].balance.0
        ),
        eq((101, 101))
    );
    assert_that!(state.accounts[&UserId(4)].balance.0, eq(98));
    assert_that!(
        state.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        eq(&Status::Resolved {
            outcome: OutcomeIndex(0),
            refunded: false
        })
    );

    let mut tied = State::default();
    market(&mut tied, 2_000);
    for user in [2, 3, 4] {
        execute(&mut tied, member(user), &Command::Join, 1_000);
    }
    for (user, outcome, amount) in [(3, 0, 1), (2, 0, 1), (4, 1, 1)] {
        execute(
            &mut tied,
            member(user),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(outcome),
                amount: Points(amount),
            },
            1_500,
        );
    }
    execute(
        &mut tied,
        moderator(1),
        &Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
        },
        2_000,
    );
    assert_that!(
        (
            tied.accounts[&UserId(2)].balance.0,
            tied.accounts[&UserId(3)].balance.0
        ),
        eq((101, 100))
    );
}

#[googletest::test]
fn no_winner_resolution_and_cancellation_refund_stakes() {
    let mut no_winner = State::default();
    market(&mut no_winner, 2_000);
    execute(&mut no_winner, member(2), &Command::Join, 1_000);
    execute(
        &mut no_winner,
        member(2),
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(7),
        },
        1_500,
    );
    execute(
        &mut no_winner,
        moderator(1),
        &Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(1),
        },
        2_000,
    );
    assert_that!(no_winner.accounts[&UserId(2)].balance.0, eq(100));
    assert_that!(
        no_winner.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        eq(&Status::Resolved {
            outcome: OutcomeIndex(1),
            refunded: true
        })
    );

    let mut cancelled = State::default();
    market(&mut cancelled, 2_000);
    execute(&mut cancelled, member(2), &Command::Join, 1_000);
    execute(
        &mut cancelled,
        member(2),
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(7),
        },
        1_500,
    );
    execute(
        &mut cancelled,
        moderator(1),
        &Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        },
        1_700,
    );
    assert_that!(cancelled.accounts[&UserId(2)].balance.0, eq(100));
    assert_that!(
        cancelled.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        eq(&Status::Cancelled)
    );
}

#[googletest::test]
fn invalid_commands_and_replay_do_not_publish_partial_changes() {
    let mut state = State::default();
    market(&mut state, 2_000);
    execute(&mut state, member(2), &Command::Join, 1_000);
    let before = state.clone();
    for command in [
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(2),
            amount: Points(1),
        },
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(-1),
        },
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(101),
        },
    ] {
        assert_that!(
            decide(&state, member(2), command, 1_500, DEFAULTS),
            err(anything())
        );
    }
    assert_that!(
        decide(
            &state,
            member(2),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(1)
            },
            2_000,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &state,
            member(2),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0)
            },
            2_000,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(2)
            },
            2_000,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0)
            },
            1_999,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &State::default(),
            Actor {
                bot: true,
                ..member(7)
            },
            &Command::Join,
            1_000,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(state, eq(&before));

    let invalid = Event::BetPlaced {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        user_id: UserId(2),
        outcome: OutcomeIndex(99),
        amount: Points(1),
        accepted_at: 1_500,
    };
    assert_that!(apply(&mut state, &invalid), err(anything()));
    assert_that!(state, eq(&before));
}

#[googletest::test]
fn history_replays_identically_and_uses_recorded_allocations() {
    let mut state = State::default();
    let mut history = Vec::new();
    for (actor, command, now) in [
        (member(1), &Command::Join, 1_000),
        (member(2), &Command::Join, 1_000),
        (member(3), &Command::Join, 1_000),
        (member(4), &Command::Join, 1_000),
        (
            member(1),
            &Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(1),
            },
            1_500,
        ),
        (
            member(3),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(2),
            },
            1_500,
        ),
        (
            member(4),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(1),
                amount: Points(2),
            },
            1_500,
        ),
        (
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
            },
            2_000,
        ),
    ] {
        history.extend(execute(&mut state, actor, command, now).events);
    }
    let mut replayed = State::default();
    for event in &history {
        apply(&mut replayed, event).unwrap();
    }
    assert_that!(replayed, eq(&state));
    if let Event::MarketResolved { payouts, .. } = history.last_mut().unwrap() {
        *payouts = vec![
            Allocation {
                user_id: UserId(2),
                amount: Points(3),
            },
            Allocation {
                user_id: UserId(3),
                amount: Points(2),
            },
        ];
    } else {
        panic!("expected resolution");
    }
    let mut altered = State::default();
    for event in &history {
        apply(&mut altered, event).unwrap();
    }
    assert_that!(
        (
            altered.accounts[&UserId(2)].balance.0,
            altered.accounts[&UserId(3)].balance.0
        ),
        eq((102, 100))
    );
}

#[googletest::test]
fn overflow_rejects_entire_decision_and_event() {
    let huge = Policy {
        amount: Points(i64::MAX),
        interval: 1,
    };
    let empty = State::default();
    assert_that!(
        decide(&empty, member(1), &Command::Join, i64::MAX, huge),
        err(anything())
    );
    assert_that!(empty, eq(&State::default()));

    let mut state = State::default();
    let join = decide(&state, member(1), &Command::Join, 0, huge).unwrap();
    for event in &join.events {
        apply(&mut state, event).unwrap();
    }
    let before = state.clone();
    assert_that!(state.accounts[&UserId(1)].balance.0, eq(i64::MAX));
    assert_that!(
        decide(
            &state,
            member(0),
            &Command::Grant { user_id: UserId(1) },
            1,
            huge
        ),
        err(anything())
    );
    assert_that!(state, eq(&before));
    let overflow = Event::PointsGranted {
        user_id: UserId(1),
        reason: GrantReason::Periodic,
        amount: Points(i64::MAX),
        from_due: 1,
        through_due: 1,
        next_grant: 2,
    };
    assert_that!(apply(&mut state, &overflow), err(anything()));
    assert_that!(state, eq(&before));
}

#[googletest::test]
fn malformed_markets_deadlines_bots_and_terminal_operations_are_rejected() {
    let mut state = State::default();
    execute(&mut state, member(1), &Command::Join, 1_000);
    let create = |question: &str, options: Vec<&str>, closes_at: i64| Command::Create {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        question: question.to_owned(),
        options: options.into_iter().map(str::to_owned).collect(),
        closes_at,
    };
    for command in [
        create("", vec!["Red", "Blue"], 2_000),
        create("Who wins?", vec!["Red", "red"], 2_000),
        create("Who wins?", vec!["Red"], 2_000),
        create("Who wins?", vec!["Red", "Blue"], 1_000),
    ] {
        assert_that!(
            decide(&state, member(1), &command, 1_000, DEFAULTS),
            err(anything())
        );
    }
    assert_that!(
        decide(
            &state,
            Actor {
                bot: true,
                ..member(1)
            },
            &create("Who wins?", vec!["Red", "Blue"], 2_000),
            1_000,
            DEFAULTS
        ),
        err(anything())
    );
    execute(
        &mut state,
        member(1),
        &create("Who wins?", vec!["Red", "Blue"], 2_000),
        1_000,
    );
    let before = state.clone();
    assert_that!(
        decide(
            &state,
            Actor {
                bot: true,
                ..member(1)
            },
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(1)
            },
            1_500,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &state,
            member(1),
            &Command::Cancel {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into()
            },
            1_500,
            DEFAULTS
        ),
        err(anything())
    );
    execute(
        &mut state,
        moderator(1),
        &Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        },
        2_100,
    );
    assert_that!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0)
            },
            2_100,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(
        decide(
            &state,
            moderator(1),
            &Command::Cancel {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into()
            },
            2_100,
            DEFAULTS
        ),
        err(anything())
    );
    assert_that!(state, ne(&before));
}

#[googletest::test]
fn replay_rejects_unbalanced_or_unknown_allocations_atomically() {
    let mut state = State::default();
    market(&mut state, 2_000);
    execute(&mut state, member(2), &Command::Join, 1_000);
    execute(&mut state, member(3), &Command::Join, 1_000);
    execute(
        &mut state,
        member(2),
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(1),
        },
        1_500,
    );
    execute(
        &mut state,
        member(3),
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
            outcome: OutcomeIndex(1),
            amount: Points(1),
        },
        1_500,
    );
    let before = state.clone();
    let invalid = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        outcome: OutcomeIndex(0),
        resolver: UserId(1),
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: UserId(2),
            amount: Points(1),
        }],
        refunded: false,
    };
    assert_that!(apply(&mut state, &invalid), err(anything()));
    assert_that!(state, eq(&before));
    let unknown = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        outcome: OutcomeIndex(0),
        resolver: UserId(1),
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: UserId(99),
            amount: Points(2),
        }],
        refunded: false,
    };
    assert_that!(apply(&mut state, &unknown), err(anything()));
    assert_that!(state, eq(&before));
}

#[googletest::test]
fn event_payload_round_trip_encodes_discord_ids_as_decimal_strings() {
    let event = Event::MemberEnrolled {
        user_id: UserId(u64::MAX),
        enrolled_at: 1_000,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_that!(json["kind"], eq("member_enrolled"));
    assert_that!(json["user_id"], eq(&u64::MAX.to_string()));
    assert_that!(
        serde_json::from_value::<Event>(json.clone()).unwrap(),
        eq(&event)
    );
    let mut unknown = json;
    unknown["extra"] = serde_json::json!("unsupported");
    assert_that!(serde_json::from_value::<Event>(unknown), err(anything()));
    assert_that!(event.name(), eq("member.enrolled"));
    assert_that!(event.subject(), eq(&format!("members/{}", u64::MAX)));
}

#[googletest::test]
fn settlement_balance_overflow_keeps_market_open_and_balances_unchanged() {
    let huge = Policy {
        amount: Points(i64::MAX),
        interval: 86_400,
    };
    let mut state = State::default();
    let commands = [
        (member(1), &Command::Join, 1_000),
        (member(2), &Command::Join, 1_000),
        (member(3), &Command::Join, 1_000),
        (
            member(1),
            &Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(1),
            },
            1_500,
        ),
        (
            member(3),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(1),
                amount: Points(1),
            },
            1_500,
        ),
    ];
    for (actor, command, now) in commands {
        let decision = decide(&state, actor, command, now, huge).unwrap();
        for event in &decision.events {
            apply(&mut state, event).unwrap();
        }
    }
    let before = state.clone();
    let resolution = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        outcome: OutcomeIndex(0),
        resolver: UserId(1),
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: UserId(2),
            amount: Points(2),
        }],
        refunded: false,
    };
    assert_that!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0)
            },
            2_000,
            huge
        ),
        err(anything())
    );
    assert_that!(apply(&mut state, &resolution), err(anything()));
    assert_that!(state, eq(&before));
}

#[googletest::test]
fn private_replay_matches_sequential_public_apply() {
    let mut decided = State::default();
    let mut events = Vec::new();
    for (actor, command, now) in [
        (member(1), &Command::Join, 1_000),
        (member(2), &Command::Join, 1_000),
        (
            member(1),
            &Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(7),
            },
            1_500,
        ),
        (
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(1),
            },
            2_000,
        ),
    ] {
        events.extend(execute(&mut decided, actor, command, now).events);
    }
    let mut sequential = State::default();
    for event in &events {
        apply(&mut sequential, event).unwrap();
    }
    assert_that!(replay(&events).unwrap(), eq(&sequential));
    assert_that!(sequential, eq(&decided));
    assert_that!(replay(&[]).unwrap(), eq(&State::default()));
}

#[googletest::test]
fn private_replay_rejects_invalid_history_as_one_result() {
    let events = vec![
        Event::GuildEconomyInitialized {
            amount: Points(100),
            interval: 86_400,
        },
        Event::MemberEnrolled {
            user_id: UserId(7),
            enrolled_at: 1_000,
        },
        Event::PointsGranted {
            user_id: UserId(7),
            reason: GrantReason::Initial,
            amount: Points(99),
            from_due: 1_000,
            through_due: 1_000,
            next_grant: 87_400,
        },
    ];
    assert_that!(replay(&events), err(anything()));
}

#[googletest::test]
fn serialized_allocations_reject_unexpected_nested_fields() {
    let market_id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for payload in [
        serde_json::json!({
            "kind": "market_resolved", "id": market_id, "outcome": 0,
            "resolver": "1", "settled_at": 2_000, "refunded": false,
            "payouts": [{"user_id": "7", "amount": 3, "note": "unexpected"}]
        }),
        serde_json::json!({
            "kind": "market_cancelled", "id": market_id,
            "moderator": "1", "cancelled_at": 2_000,
            "refunds": [{"user_id": "7", "amount": 3, "note": "unexpected"}]
        }),
    ] {
        assert_that!(serde_json::from_value::<Event>(payload), err(anything()));
    }
}

#[googletest::test]
fn event_snowflakes_require_canonical_positive_decimal_strings() {
    for user_id in ["+7", "07", "0", "+0", "00", " 7", "7 ", "-7", "7.0"] {
        let payload = serde_json::json!({
            "kind": "member_enrolled", "user_id": user_id, "enrolled_at": 1_000
        });
        assert_that!(
            serde_json::from_value::<Event>(payload),
            err(anything()),
            "accepted {user_id}"
        );
    }
    let actor = Actor {
        user_id: UserId(0),
        moderator: false,
        bot: false,
    };
    let encoded = serde_json::to_value(actor).unwrap();
    assert_that!(encoded["user_id"], eq("0"));
    assert_that!(serde_json::from_value::<Actor>(encoded).unwrap(), eq(actor));
    for user_id in ["+0", "00"] {
        let encoded = serde_json::json!({"user_id": user_id, "moderator": false, "bot": false});
        assert_that!(serde_json::from_value::<Actor>(encoded), err(anything()));
    }
}

#[googletest::test]
fn derived_market_pool_matches_stakes_and_preserves_available_points() {
    let mut state = State::default();
    market(&mut state, 2_000);
    for user in [2, 3] {
        execute(&mut state, member(user), &Command::Join, 1_000);
    }
    for (user, amount) in [(2, 7), (3, 5)] {
        execute(
            &mut state,
            member(user),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
                outcome: OutcomeIndex(0),
                amount: Points(amount),
            },
            1_500,
        );
    }
    let market = &state.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"];
    assert_that!(market.total_staked.0, eq(12));
    assert_that!(
        market.total_staked.0,
        eq(market.bets.iter().map(|bet| bet.amount.0).sum::<i64>())
    );
    let available = state
        .accounts
        .values()
        .map(|account| account.balance.0)
        .sum::<i64>();
    assert_that!(available + market.total_staked.0, eq(300));
    execute(
        &mut state,
        moderator(1),
        &Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned().into(),
        },
        1_700,
    );
    assert_that!(
        state
            .accounts
            .values()
            .map(|account| account.balance.0)
            .sum::<i64>(),
        eq(300)
    );
}

#[googletest::test]
fn bet_receipt_identifies_outcome_stake_and_remaining_balance() {
    let mut state = State::default();
    market(&mut state, 2_000);
    let receipt = execute(
        &mut state,
        member(1),
        &Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".into(),
            outcome: OutcomeIndex(1),
            amount: Points(25),
        },
        1_500,
    )
    .response;
    assert_that!(receipt, contains_substring("Blue"));
    assert_that!(receipt, contains_substring("25 points"));
    assert_that!(receipt, contains_substring("75 points"));
    assert_that!(state.accounts[&UserId(1)].balance.0, eq(75));
}

// A points balance must not be assignable to a grant deadline, and a market
// identity must not be interchangeable with its human-readable question.
#[googletest::test]
fn domain_values_have_distinct_types_for_identity_and_units() {
    fn same_type<A: 'static, B: 'static>(_: &A, _: &B) -> bool {
        std::any::TypeId::of::<A>() == std::any::TypeId::of::<B>()
    }
    let mut state = State::default();
    market(&mut state, 2_000);
    let account = state.accounts.values().next().unwrap();
    let (id, market) = state.markets.iter().next().unwrap();
    assert_that!(same_type(&account.balance, &account.next_grant), eq(false));
    assert_that!(same_type(id, &market.question), eq(false));
}
