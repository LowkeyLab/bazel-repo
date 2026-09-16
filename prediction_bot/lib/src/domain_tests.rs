use super::*;

const DEFAULTS: Policy = Policy {
    amount: 100,
    interval: 86_400,
};

fn member(user_id: u64) -> Actor {
    Actor {
        user_id,
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

fn execute(state: &mut State, actor: Actor, command: Command, now: i64) -> Decision {
    let decision = decide(state, actor, &command, now, DEFAULTS).unwrap();
    for event in &decision.events {
        apply(state, event).unwrap();
    }
    decision
}

fn market(state: &mut State, closes_at: i64) {
    execute(state, member(1), Command::Join, 1_000);
    execute(
        state,
        member(1),
        Command::Create {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            question: "Who wins?".to_owned(),
            options: vec!["Red".to_owned(), "Blue".to_owned()],
            closes_at,
        },
        1_000,
    );
}

#[test]
fn enrollment_grants_once_and_preserves_schedule() {
    let mut state = State::default();
    let first = execute(&mut state, member(7), Command::Join, 1_000);
    assert_eq!(first.events.len(), 3);
    assert_eq!(state.accounts[&7].balance, 100);
    assert_eq!(state.accounts[&7].next_grant, 87_400);
    let repeat = execute(&mut state, member(7), Command::Join, 2_000);
    assert!(repeat.events.is_empty());
    assert_eq!(state.accounts[&7].balance, 100);
    assert_eq!(state.accounts[&7].next_grant, 87_400);
}

#[test]
fn grants_cover_exact_due_boundaries_after_offline_intervals() {
    let mut state = State::default();
    execute(&mut state, member(7), Command::Join, 1_000);
    let early = execute(&mut state, member(0), Command::Grant { user_id: 7 }, 87_399);
    assert!(early.events.is_empty());
    let due = execute(
        &mut state,
        member(0),
        Command::Grant { user_id: 7 },
        173_800,
    );
    assert_eq!(due.events.len(), 1);
    assert_eq!(state.accounts[&7].balance, 300);
    assert_eq!(state.accounts[&7].next_grant, 260_200);
}

#[test]
fn settlement_distributes_pool_and_ties_by_numeric_user_id() {
    let mut state = State::default();
    market(&mut state, 2_000);
    for user in [2, 3, 4] {
        execute(&mut state, member(user), Command::Join, 1_000);
    }
    for (user, outcome, amount) in [(2, 0, 1), (3, 0, 2), (4, 1, 2)] {
        execute(
            &mut state,
            member(user),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome,
                amount,
            },
            1_500,
        );
    }
    execute(
        &mut state,
        moderator(1),
        Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
        },
        2_000,
    );
    assert_eq!(
        (state.accounts[&2].balance, state.accounts[&3].balance),
        (101, 101)
    );
    assert_eq!(state.accounts[&4].balance, 98);
    assert_eq!(
        state.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        Status::Resolved {
            outcome: 0,
            refunded: false
        }
    );

    let mut tied = State::default();
    market(&mut tied, 2_000);
    for user in [2, 3, 4] {
        execute(&mut tied, member(user), Command::Join, 1_000);
    }
    for (user, outcome, amount) in [(3, 0, 1), (2, 0, 1), (4, 1, 1)] {
        execute(
            &mut tied,
            member(user),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome,
                amount,
            },
            1_500,
        );
    }
    execute(
        &mut tied,
        moderator(1),
        Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
        },
        2_000,
    );
    assert_eq!(
        (tied.accounts[&2].balance, tied.accounts[&3].balance),
        (101, 100)
    );
}

#[test]
fn no_winner_resolution_and_cancellation_refund_stakes() {
    let mut no_winner = State::default();
    market(&mut no_winner, 2_000);
    execute(&mut no_winner, member(2), Command::Join, 1_000);
    execute(
        &mut no_winner,
        member(2),
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
            amount: 7,
        },
        1_500,
    );
    execute(
        &mut no_winner,
        moderator(1),
        Command::Resolve {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 1,
        },
        2_000,
    );
    assert_eq!(no_winner.accounts[&2].balance, 100);
    assert_eq!(
        no_winner.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        Status::Resolved {
            outcome: 1,
            refunded: true
        }
    );

    let mut cancelled = State::default();
    market(&mut cancelled, 2_000);
    execute(&mut cancelled, member(2), Command::Join, 1_000);
    execute(
        &mut cancelled,
        member(2),
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
            amount: 7,
        },
        1_500,
    );
    execute(
        &mut cancelled,
        moderator(1),
        Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        },
        1_700,
    );
    assert_eq!(cancelled.accounts[&2].balance, 100);
    assert_eq!(
        cancelled.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"].status,
        Status::Cancelled
    );
}

#[test]
fn invalid_commands_and_replay_do_not_publish_partial_changes() {
    let mut state = State::default();
    market(&mut state, 2_000);
    execute(&mut state, member(2), Command::Join, 1_000);
    let before = state.clone();
    for command in [
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 2,
            amount: 1,
        },
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
            amount: -1,
        },
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
            amount: 101,
        },
    ] {
        assert!(decide(&state, member(2), &command, 1_500, DEFAULTS).is_err());
    }
    assert!(
        decide(
            &state,
            member(2),
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 1
            },
            2_000,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            member(2),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0
            },
            2_000,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 2
            },
            2_000,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0
            },
            1_999,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &State::default(),
            Actor {
                bot: true,
                ..member(7)
            },
            &Command::Join,
            1_000,
            DEFAULTS
        )
        .is_err()
    );
    assert_eq!(state, before);

    let invalid = Event::BetPlaced {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        user_id: 2,
        outcome: 99,
        amount: 1,
        accepted_at: 1_500,
    };
    assert!(apply(&mut state, &invalid).is_err());
    assert_eq!(state, before);
}

#[test]
fn history_replays_identically_and_uses_recorded_allocations() {
    let mut state = State::default();
    let mut history = Vec::new();
    for (actor, command, now) in [
        (member(1), Command::Join, 1_000),
        (member(2), Command::Join, 1_000),
        (member(3), Command::Join, 1_000),
        (member(4), Command::Join, 1_000),
        (
            member(1),
            Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 1,
            },
            1_500,
        ),
        (
            member(3),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 2,
            },
            1_500,
        ),
        (
            member(4),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 1,
                amount: 2,
            },
            1_500,
        ),
        (
            moderator(1),
            Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
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
    assert_eq!(replayed, state);
    if let Event::MarketResolved { payouts, .. } = history.last_mut().unwrap() {
        *payouts = vec![
            Allocation {
                user_id: 2,
                amount: 3,
            },
            Allocation {
                user_id: 3,
                amount: 2,
            },
        ];
    } else {
        panic!("expected resolution");
    }
    let mut altered = State::default();
    for event in &history {
        apply(&mut altered, event).unwrap();
    }
    assert_eq!(
        (altered.accounts[&2].balance, altered.accounts[&3].balance),
        (102, 100)
    );
}

#[test]
fn overflow_rejects_entire_decision_and_event() {
    let huge = Policy {
        amount: i64::MAX,
        interval: 1,
    };
    let empty = State::default();
    assert!(decide(&empty, member(1), &Command::Join, i64::MAX, huge).is_err());
    assert_eq!(empty, State::default());

    let mut state = State::default();
    let join = decide(&state, member(1), &Command::Join, 0, huge).unwrap();
    for event in &join.events {
        apply(&mut state, event).unwrap();
    }
    let before = state.clone();
    assert_eq!(state.accounts[&1].balance, i64::MAX);
    assert!(decide(&state, member(0), &Command::Grant { user_id: 1 }, 1, huge).is_err());
    assert_eq!(state, before);
    let overflow = Event::PointsGranted {
        user_id: 1,
        reason: GrantReason::Periodic,
        amount: i64::MAX,
        from_due: 1,
        through_due: 1,
        next_grant: 2,
    };
    assert!(apply(&mut state, &overflow).is_err());
    assert_eq!(state, before);
}

#[test]
fn malformed_markets_deadlines_bots_and_terminal_operations_are_rejected() {
    let mut state = State::default();
    execute(&mut state, member(1), Command::Join, 1_000);
    let create = |question: &str, options: Vec<&str>, closes_at: i64| Command::Create {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
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
        assert!(decide(&state, member(1), &command, 1_000, DEFAULTS).is_err());
    }
    assert!(
        decide(
            &state,
            Actor {
                bot: true,
                ..member(1)
            },
            &create("Who wins?", vec!["Red", "Blue"], 2_000),
            1_000,
            DEFAULTS
        )
        .is_err()
    );
    execute(
        &mut state,
        member(1),
        create("Who wins?", vec!["Red", "Blue"], 2_000),
        1_000,
    );
    let before = state.clone();
    assert!(
        decide(
            &state,
            Actor {
                bot: true,
                ..member(1)
            },
            &Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 1
            },
            1_500,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            member(1),
            &Command::Cancel {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned()
            },
            1_500,
            DEFAULTS
        )
        .is_err()
    );
    execute(
        &mut state,
        moderator(1),
        Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        },
        2_100,
    );
    assert!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0
            },
            2_100,
            DEFAULTS
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            moderator(1),
            &Command::Cancel {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned()
            },
            2_100,
            DEFAULTS
        )
        .is_err()
    );
    assert_ne!(state, before);
}

#[test]
fn replay_rejects_unbalanced_or_unknown_allocations_atomically() {
    let mut state = State::default();
    market(&mut state, 2_000);
    execute(&mut state, member(2), Command::Join, 1_000);
    execute(&mut state, member(3), Command::Join, 1_000);
    execute(
        &mut state,
        member(2),
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 0,
            amount: 1,
        },
        1_500,
    );
    execute(
        &mut state,
        member(3),
        Command::Bet {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
            outcome: 1,
            amount: 1,
        },
        1_500,
    );
    let before = state.clone();
    let invalid = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        outcome: 0,
        resolver: 1,
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: 2,
            amount: 1,
        }],
        refunded: false,
    };
    assert!(apply(&mut state, &invalid).is_err());
    assert_eq!(state, before);
    let unknown = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        outcome: 0,
        resolver: 1,
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: 99,
            amount: 2,
        }],
        refunded: false,
    };
    assert!(apply(&mut state, &unknown).is_err());
    assert_eq!(state, before);
}

#[test]
fn event_payload_round_trip_encodes_discord_ids_as_decimal_strings() {
    let event = Event::MemberEnrolled {
        user_id: u64::MAX,
        enrolled_at: 1_000,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["kind"], "member_enrolled");
    assert_eq!(json["user_id"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<Event>(json.clone()).unwrap(),
        event
    );
    let mut unknown = json;
    unknown["extra"] = serde_json::json!("unsupported");
    assert!(serde_json::from_value::<Event>(unknown).is_err());
    assert_eq!(event.name(), "member.enrolled");
    assert_eq!(event.subject(), format!("members/{}", u64::MAX));
}

#[test]
fn settlement_balance_overflow_keeps_market_open_and_balances_unchanged() {
    let huge = Policy {
        amount: i64::MAX,
        interval: 86_400,
    };
    let mut state = State::default();
    let commands = [
        (member(1), Command::Join, 1_000),
        (member(2), Command::Join, 1_000),
        (member(3), Command::Join, 1_000),
        (
            member(1),
            Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 1,
            },
            1_500,
        ),
        (
            member(3),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 1,
                amount: 1,
            },
            1_500,
        ),
    ];
    for (actor, command, now) in commands {
        let decision = decide(&state, actor, &command, now, huge).unwrap();
        for event in &decision.events {
            apply(&mut state, event).unwrap();
        }
    }
    let before = state.clone();
    let resolution = Event::MarketResolved {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        outcome: 0,
        resolver: 1,
        settled_at: 2_000,
        payouts: vec![Allocation {
            user_id: 2,
            amount: 2,
        }],
        refunded: false,
    };
    assert!(
        decide(
            &state,
            moderator(1),
            &Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0
            },
            2_000,
            huge
        )
        .is_err()
    );
    assert!(apply(&mut state, &resolution).is_err());
    assert_eq!(state, before);
}

#[test]
fn private_replay_matches_sequential_public_apply() {
    let mut decided = State::default();
    let mut events = Vec::new();
    for (actor, command, now) in [
        (member(1), Command::Join, 1_000),
        (member(2), Command::Join, 1_000),
        (
            member(1),
            Command::Create {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                question: "Who wins?".to_owned(),
                options: vec!["Red".to_owned(), "Blue".to_owned()],
                closes_at: 2_000,
            },
            1_000,
        ),
        (
            member(2),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount: 7,
            },
            1_500,
        ),
        (
            moderator(1),
            Command::Resolve {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 1,
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
    assert_eq!(replay(&events).unwrap(), sequential);
    assert_eq!(sequential, decided);
    assert_eq!(replay(&[]).unwrap(), State::default());
}

#[test]
fn private_replay_rejects_invalid_history_as_one_result() {
    let events = vec![
        Event::GuildEconomyInitialized {
            amount: 100,
            interval: 86_400,
        },
        Event::MemberEnrolled {
            user_id: 7,
            enrolled_at: 1_000,
        },
        Event::PointsGranted {
            user_id: 7,
            reason: GrantReason::Initial,
            amount: 99,
            from_due: 1_000,
            through_due: 1_000,
            next_grant: 87_400,
        },
    ];
    assert!(replay(&events).is_err());
}

#[test]
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
        assert!(serde_json::from_value::<Event>(payload).is_err());
    }
}

#[test]
fn event_snowflakes_require_canonical_positive_decimal_strings() {
    for user_id in ["+7", "07", "0", "+0", "00", " 7", "7 ", "-7", "7.0"] {
        let payload = serde_json::json!({
            "kind": "member_enrolled", "user_id": user_id, "enrolled_at": 1_000
        });
        assert!(
            serde_json::from_value::<Event>(payload).is_err(),
            "accepted {user_id}"
        );
    }
    let actor = Actor {
        user_id: 0,
        moderator: false,
        bot: false,
    };
    let encoded = serde_json::to_value(actor).unwrap();
    assert_eq!(encoded["user_id"], "0");
    assert_eq!(serde_json::from_value::<Actor>(encoded).unwrap(), actor);
    for user_id in ["+0", "00"] {
        let encoded = serde_json::json!({"user_id": user_id, "moderator": false, "bot": false});
        assert!(serde_json::from_value::<Actor>(encoded).is_err());
    }
}

#[test]
fn derived_market_pool_matches_stakes_and_preserves_available_points() {
    let mut state = State::default();
    market(&mut state, 2_000);
    for user in [2, 3] {
        execute(&mut state, member(user), Command::Join, 1_000);
    }
    for (user, amount) in [(2, 7), (3, 5)] {
        execute(
            &mut state,
            member(user),
            Command::Bet {
                id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
                outcome: 0,
                amount,
            },
            1_500,
        );
    }
    let market = &state.markets["78e82954-4c67-4e0d-8c80-8ab95a527ae5"];
    assert_eq!(market.total_staked, 12);
    assert_eq!(
        market.total_staked,
        market.bets.iter().map(|bet| bet.amount).sum::<i64>()
    );
    let available = state
        .accounts
        .values()
        .map(|account| account.balance)
        .sum::<i64>();
    assert_eq!(available + market.total_staked, 300);
    execute(
        &mut state,
        moderator(1),
        Command::Cancel {
            id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".to_owned(),
        },
        1_700,
    );
    assert_eq!(
        state
            .accounts
            .values()
            .map(|account| account.balance)
            .sum::<i64>(),
        300
    );
}
