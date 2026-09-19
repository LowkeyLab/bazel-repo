use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[path = "types.rs"]
pub mod types;
use types::{MarketId, OutcomeIndex, Points, UserId};

mod snowflake {
    use super::UserId;
    use serde::{Deserialize, Deserializer, Serializer, de::Error, ser::Error as _};

    fn parse(value: &str, allow_system: bool) -> Result<u64, &'static str> {
        if allow_system && value == "0" {
            return Ok(0);
        }
        if value.is_empty()
            || value.as_bytes()[0] == b'0'
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("Discord ID must be a canonical decimal string");
        }
        value.parse().map_err(|_| "Discord ID exceeds u64")
    }

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "Serde serialize_with requires a borrowed field"
    )]
    pub fn serialize<S: Serializer>(value: &UserId, serializer: S) -> Result<S::Ok, S::Error> {
        if value.0 == 0 {
            return Err(S::Error::custom("event Discord ID must be positive"));
        }
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<UserId, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse(&value, false).map(UserId).map_err(D::Error::custom)
    }

    pub mod actor {
        use super::UserId;
        use serde::{Deserialize, Deserializer, Serializer, de::Error};

        #[expect(
            clippy::trivially_copy_pass_by_ref,
            reason = "Serde serialize_with requires a borrowed field"
        )]
        pub fn serialize<S: Serializer>(value: &UserId, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&value.to_string())
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<UserId, D::Error> {
            let value = String::deserialize(deserializer)?;
            super::parse(&value, true)
                .map(UserId)
                .map_err(D::Error::custom)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub amount: Points,
    pub interval: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    #[serde(with = "snowflake::actor")]
    pub user_id: UserId,
    pub moderator: bool,
    pub bot: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Join,
    Create {
        id: MarketId,
        question: String,
        options: Vec<String>,
        closes_at: i64,
    },
    Bet {
        id: MarketId,
        outcome: OutcomeIndex,
        amount: Points,
    },
    Resolve {
        id: MarketId,
        outcome: OutcomeIndex,
    },
    Cancel {
        id: MarketId,
    },
    Grant {
        #[serde(with = "snowflake")]
        user_id: UserId,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub policy: Option<Policy>,
    pub accounts: BTreeMap<UserId, Account>,
    pub markets: BTreeMap<MarketId, Market>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub balance: Points,
    pub next_grant: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Market {
    #[serde(with = "snowflake")]
    pub creator: UserId,
    pub question: String,
    pub options: Vec<String>,
    pub closes_at: i64,
    pub created_at: i64,
    pub status: Status,
    pub bets: Vec<Bet>,
    pub total_staked: Points,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Open,
    Resolved {
        outcome: OutcomeIndex,
        refunded: bool,
    },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bet {
    #[serde(with = "snowflake")]
    pub user_id: UserId,
    pub outcome: OutcomeIndex,
    pub amount: Points,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    #[serde(with = "snowflake")]
    pub user_id: UserId,
    pub amount: Points,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantReason {
    Initial,
    Periodic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    GuildEconomyInitialized {
        amount: Points,
        interval: i64,
    },
    MemberEnrolled {
        #[serde(with = "snowflake")]
        user_id: UserId,
        enrolled_at: i64,
    },
    PointsGranted {
        #[serde(with = "snowflake")]
        user_id: UserId,
        reason: GrantReason,
        amount: Points,
        from_due: i64,
        through_due: i64,
        next_grant: i64,
    },
    MarketCreated {
        id: MarketId,
        #[serde(with = "snowflake")]
        creator: UserId,
        question: String,
        options: Vec<String>,
        created_at: i64,
        closes_at: i64,
    },
    BetPlaced {
        id: MarketId,
        #[serde(with = "snowflake")]
        user_id: UserId,
        outcome: OutcomeIndex,
        amount: Points,
        accepted_at: i64,
    },
    MarketResolved {
        id: MarketId,
        outcome: OutcomeIndex,
        #[serde(with = "snowflake")]
        resolver: UserId,
        settled_at: i64,
        payouts: Vec<Allocation>,
        refunded: bool,
    },
    MarketCancelled {
        id: MarketId,
        #[serde(with = "snowflake")]
        moderator: UserId,
        cancelled_at: i64,
        refunds: Vec<Allocation>,
    },
}

impl Event {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::GuildEconomyInitialized { .. } => "economy.initialized",
            Self::MemberEnrolled { .. } => "member.enrolled",
            Self::PointsGranted { .. } => "points.granted",
            Self::MarketCreated { .. } => "market.created",
            Self::BetPlaced { .. } => "bet.placed",
            Self::MarketResolved { .. } => "market.resolved",
            Self::MarketCancelled { .. } => "market.cancelled",
        }
    }

    #[must_use]
    pub fn subject(&self) -> String {
        match self {
            Self::GuildEconomyInitialized { .. } => "economy".to_owned(),
            Self::MemberEnrolled { user_id, .. } | Self::PointsGranted { user_id, .. } => {
                format!("members/{user_id}")
            }
            Self::MarketCreated { id, .. }
            | Self::BetPlaced { id, .. }
            | Self::MarketResolved { id, .. }
            | Self::MarketCancelled { id, .. } => format!("markets/{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub events: Vec<Event>,
    pub response: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("invalid operation: {0}")]
    Invalid(&'static str),
    #[error("arithmetic overflow")]
    Overflow,
}

fn positive_policy(policy: Policy) -> Result<(), DomainError> {
    if policy.amount <= Points(0) || policy.interval <= 0 {
        return Err(DomainError::Invalid("grant policy must be positive"));
    }
    Ok(())
}

fn checked_add(a: i64, b: i64) -> Result<i64, DomainError> {
    a.checked_add(b).ok_or(DomainError::Overflow)
}

fn checked_points_add(a: Points, b: Points) -> Result<Points, DomainError> {
    checked_add(a.0, b.0).map(Points)
}

fn checked_mul(a: i64, b: i64) -> Result<i64, DomainError> {
    a.checked_mul(b).ok_or(DomainError::Overflow)
}

fn valid_market_id(id: &MarketId) -> bool {
    let bytes = id.0.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn valid_market(question: &str, options: &[String]) -> Result<(), DomainError> {
    if question.trim().is_empty() || question.chars().count() > 200 {
        return Err(DomainError::Invalid("invalid market question"));
    }
    if !(2..=10).contains(&options.len()) {
        return Err(DomainError::Invalid("market needs two to ten outcomes"));
    }
    let mut unique = std::collections::BTreeSet::new();
    for option in options {
        if option.trim().is_empty()
            || option.chars().count() > 80
            || !unique.insert(option.trim().to_lowercase())
        {
            return Err(DomainError::Invalid("invalid or duplicate outcome"));
        }
    }
    Ok(())
}

fn pool(market: &Market) -> Result<Points, DomainError> {
    if market.total_staked < Points(0) {
        return Err(DomainError::Invalid("negative market pool"));
    }
    Ok(market.total_staked)
}

fn stakes(
    market: &Market,
    outcome: Option<OutcomeIndex>,
) -> Result<BTreeMap<UserId, Points>, DomainError> {
    let mut totals = BTreeMap::new();
    for bet in &market.bets {
        if outcome.is_none_or(|selected| selected == bet.outcome) {
            let old = totals.get(&bet.user_id).copied().unwrap_or(Points(0));
            totals.insert(bet.user_id, checked_points_add(old, bet.amount)?);
        }
    }
    Ok(totals)
}

fn allocations(totals: BTreeMap<UserId, Points>) -> Vec<Allocation> {
    totals
        .into_iter()
        .map(|(user_id, amount)| Allocation { user_id, amount })
        .collect()
}

fn payouts(market: &Market, outcome: OutcomeIndex) -> Result<(Vec<Allocation>, bool), DomainError> {
    let winners = stakes(market, Some(outcome))?;
    if winners.is_empty() {
        return Ok((allocations(stakes(market, None)?), true));
    }
    let total_pool = i128::from(pool(market)?.0);
    let total_winning_stake: i128 = winners.values().map(|stake| i128::from(stake.0)).sum();
    let mut entries: Vec<(UserId, i64, i128)> = winners
        .into_iter()
        .map(|(user_id, stake)| {
            let numerator = total_pool * i128::from(stake.0);
            let base = i64::try_from(numerator / total_winning_stake)
                .map_err(|_| DomainError::Overflow)?;
            Ok((user_id, base, numerator % total_winning_stake))
        })
        .collect::<Result<_, DomainError>>()?;
    let base_sum: i128 = entries.iter().map(|(_, base, _)| i128::from(*base)).sum();
    let remainder = usize::try_from(total_pool - base_sum).map_err(|_| DomainError::Overflow)?;
    entries.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    for entry in entries.iter_mut().take(remainder) {
        entry.1 = checked_add(entry.1, 1)?;
    }
    entries.sort_by_key(|entry| entry.0);
    Ok((
        entries
            .into_iter()
            .map(|(user_id, amount, _)| Allocation {
                user_id,
                amount: Points(amount),
            })
            .collect(),
        false,
    ))
}

fn finish(state: &State, events: Vec<Event>, response: String) -> Result<Decision, DomainError> {
    let mut candidate = state.clone();
    for event in &events {
        apply_inner(&mut candidate, event)?;
    }
    Ok(Decision { events, response })
}

/// Validate a command and produce events without changing the supplied state.
///
/// # Errors
/// Returns an error for invalid actors, commands, market transitions, or arithmetic overflow.
pub fn decide(
    state: &State,
    actor: Actor,
    command: &Command,
    now: i64,
    defaults: Policy,
) -> Result<Decision, DomainError> {
    match command {
        Command::Join => decide_join(state, actor, now, defaults),

        Command::Grant { user_id } => decide_grant(state, actor, *user_id, now),

        Command::Create {
            id,
            question,
            options,
            closes_at,
        } => decide_create(state, actor, id, question, options, *closes_at, now),

        Command::Bet {
            id,
            outcome,
            amount,
        } => {
            if actor.bot || actor.user_id == UserId(0) {
                return Err(DomainError::Invalid("bots cannot bet"));
            }
            let account = state
                .accounts
                .get(&actor.user_id)
                .ok_or(DomainError::Invalid("member not enrolled"))?;
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if market.status != Status::Open || now >= market.closes_at || now < market.created_at {
                return Err(DomainError::Invalid("market is not open for betting"));
            }
            if outcome.0 >= market.options.len() || *amount <= Points(0) {
                return Err(DomainError::Invalid("invalid outcome or stake"));
            }
            if *amount > account.balance {
                return Err(DomainError::Invalid("insufficient points"));
            }
            checked_points_add(pool(market)?, *amount)?;
            finish(
                state,
                vec![Event::BetPlaced {
                    id: id.clone(),
                    user_id: actor.user_id,
                    outcome: *outcome,
                    amount: *amount,
                    accepted_at: now,
                }],
                format!(
                    "Staked {amount} points on {}. Remaining balance: {} points.",
                    market.options[outcome.0],
                    account.balance.0 - amount.0
                ),
            )
        }
        Command::Resolve { id, outcome } => {
            if actor.bot || actor.user_id == UserId(0) {
                return Err(DomainError::Invalid("market creator or moderator required"));
            }
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if !actor.moderator && actor.user_id != market.creator {
                return Err(DomainError::Invalid("market creator or moderator required"));
            }
            if market.status != Status::Open || now < market.closes_at {
                return Err(DomainError::Invalid("market is not ready to resolve"));
            }
            if outcome.0 >= market.options.len() {
                return Err(DomainError::Invalid("invalid outcome"));
            }
            let (payouts, refunded) = payouts(market, *outcome)?;
            finish(
                state,
                vec![Event::MarketResolved {
                    id: id.clone(),
                    outcome: *outcome,
                    resolver: actor.user_id,
                    settled_at: now,
                    payouts,
                    refunded,
                }],
                if refunded {
                    "Market resolved; stakes refunded."
                } else {
                    "Market resolved."
                }
                .to_owned(),
            )
        }
        Command::Cancel { id } => decide_cancel(state, actor, id, now),
    }
}

fn recorded_allocations(
    entries: &[Allocation],
    accounts: &BTreeMap<UserId, Account>,
) -> Result<BTreeMap<UserId, Points>, DomainError> {
    let mut recorded = BTreeMap::new();
    for allocation in entries {
        if allocation.amount <= Points(0)
            || !accounts.contains_key(&allocation.user_id)
            || recorded
                .insert(allocation.user_id, allocation.amount)
                .is_some()
        {
            return Err(DomainError::Invalid("invalid allocation"));
        }
    }
    Ok(recorded)
}

fn credit_allocations(
    accounts: &mut BTreeMap<UserId, Account>,
    entries: &[Allocation],
) -> Result<(), DomainError> {
    for allocation in entries {
        let account = accounts
            .get_mut(&allocation.user_id)
            .ok_or(DomainError::Invalid("unknown allocation member"))?;
        account.balance = checked_points_add(account.balance, allocation.amount)?;
    }
    Ok(())
}

fn apply_inner(state: &mut State, event: &Event) -> Result<(), DomainError> {
    match event {
        Event::GuildEconomyInitialized { amount, interval } => {
            let policy = Policy {
                amount: *amount,
                interval: *interval,
            };
            positive_policy(policy)?;
            if state.policy.is_some() || !state.accounts.is_empty() || !state.markets.is_empty() {
                return Err(DomainError::Invalid("economy already initialized"));
            }
            state.policy = Some(policy);
        }
        Event::MemberEnrolled {
            user_id,
            enrolled_at,
        } => {
            if state.policy.is_none()
                || *user_id == UserId(0)
                || state.accounts.contains_key(user_id)
            {
                return Err(DomainError::Invalid("invalid enrollment transition"));
            }
            state.accounts.insert(
                *user_id,
                Account {
                    balance: Points(0),
                    next_grant: *enrolled_at,
                },
            );
        }
        Event::PointsGranted {
            user_id,
            reason,
            amount,
            from_due,
            through_due,
            next_grant,
        } => apply_grant(
            state,
            *user_id,
            *reason,
            *amount,
            *from_due,
            *through_due,
            *next_grant,
        )?,

        Event::MarketCreated {
            id,
            creator,
            question,
            options,
            created_at,
            closes_at,
        } => apply_market_creation(
            state,
            id,
            *creator,
            question,
            options,
            *created_at,
            *closes_at,
        )?,

        Event::BetPlaced {
            id,
            user_id,
            outcome,
            amount,
            accepted_at,
        } => apply_bet(state, id, *user_id, *outcome, *amount, *accepted_at)?,

        Event::MarketResolved {
            id,
            outcome,
            resolver,
            settled_at,
            payouts,
            refunded,
        } => apply_resolution(
            state,
            id,
            *outcome,
            *resolver,
            *settled_at,
            payouts,
            *refunded,
        )?,

        Event::MarketCancelled {
            id,
            moderator,
            refunds,
            ..
        } => apply_cancellation(state, id, *moderator, refunds)?,
    }
    Ok(())
}

/// Apply one recorded event atomically to a projection.
///
/// # Errors
/// Returns an error for an invalid transition or arithmetic overflow, leaving state unchanged.
pub fn apply(state: &mut State, event: &Event) -> Result<(), DomainError> {
    let mut candidate = state.clone();
    apply_inner(&mut candidate, event)?;
    *state = candidate;
    Ok(())
}

/// Reconstruct a projection from recorded events in stream order.
///
/// # Errors
/// Returns an error if any recorded transition is invalid or arithmetic overflows.
pub fn replay(events: &[Event]) -> Result<State, DomainError> {
    let mut candidate = State::default();
    for event in events {
        apply_inner(&mut candidate, event)?;
    }
    Ok(candidate)
}

fn decide_join(
    state: &State,
    actor: Actor,
    now: i64,
    defaults: Policy,
) -> Result<Decision, DomainError> {
    if actor.bot || actor.user_id == UserId(0) {
        return Err(DomainError::Invalid("bots and system users cannot enroll"));
    }
    if state.accounts.contains_key(&actor.user_id) {
        return finish(state, Vec::new(), "Already enrolled.".to_owned());
    }
    let policy = state.policy.unwrap_or(defaults);
    positive_policy(policy)?;
    let next_grant = checked_add(now, policy.interval)?;
    let mut events = Vec::new();
    if state.policy.is_none() {
        events.push(Event::GuildEconomyInitialized {
            amount: policy.amount,
            interval: policy.interval,
        });
    }
    events.push(Event::MemberEnrolled {
        user_id: actor.user_id,
        enrolled_at: now,
    });
    events.push(Event::PointsGranted {
        user_id: actor.user_id,
        reason: GrantReason::Initial,
        amount: policy.amount,
        from_due: now,
        through_due: now,
        next_grant,
    });
    finish(
        state,
        events,
        format!("Enrolled with {} points.", policy.amount),
    )
}

fn decide_grant(
    state: &State,
    actor: Actor,
    user_id: UserId,
    now: i64,
) -> Result<Decision, DomainError> {
    if actor.user_id != UserId(0) {
        return Err(DomainError::Invalid("grants require the system actor"));
    }
    let policy = state
        .policy
        .ok_or(DomainError::Invalid("economy not initialized"))?;
    positive_policy(policy)?;
    let account = state
        .accounts
        .get(&user_id)
        .ok_or(DomainError::Invalid("member not enrolled"))?;
    if now < account.next_grant {
        return finish(state, Vec::new(), "No grant due.".to_owned());
    }
    let count128 =
        (i128::from(now) - i128::from(account.next_grant)) / i128::from(policy.interval) + 1;
    let count = i64::try_from(count128).map_err(|_| DomainError::Overflow)?;
    let amount = Points(checked_mul(policy.amount.0, count)?);
    let through_due = checked_add(account.next_grant, checked_mul(policy.interval, count - 1)?)?;
    let next_grant = checked_add(through_due, policy.interval)?;
    finish(
        state,
        vec![Event::PointsGranted {
            user_id,
            reason: GrantReason::Periodic,
            amount,
            from_due: account.next_grant,
            through_due,
            next_grant,
        }],
        format!("Granted {amount} points."),
    )
}

fn decide_cancel(
    state: &State,
    actor: Actor,
    id: &MarketId,
    now: i64,
) -> Result<Decision, DomainError> {
    if !actor.moderator || actor.bot || actor.user_id == UserId(0) {
        return Err(DomainError::Invalid("moderator required"));
    }
    let market = state
        .markets
        .get(id)
        .ok_or(DomainError::Invalid("unknown market"))?;
    if market.status != Status::Open {
        return Err(DomainError::Invalid("market already terminal"));
    }
    finish(
        state,
        vec![Event::MarketCancelled {
            id: id.to_owned(),
            moderator: actor.user_id,
            cancelled_at: now,
            refunds: allocations(stakes(market, None)?),
        }],
        "Market cancelled; stakes refunded.".to_owned(),
    )
}

fn apply_grant(
    state: &mut State,
    user_id: UserId,
    reason: GrantReason,
    amount: Points,
    from_due: i64,
    through_due: i64,
    next_grant: i64,
) -> Result<(), DomainError> {
    let policy = state
        .policy
        .ok_or(DomainError::Invalid("economy not initialized"))?;
    positive_policy(policy)?;
    let account = state
        .accounts
        .get_mut(&user_id)
        .ok_or(DomainError::Invalid("grant to unknown member"))?;
    if from_due != account.next_grant || through_due < from_due || amount <= Points(0) {
        return Err(DomainError::Invalid("invalid grant schedule"));
    }
    let distance = i128::from(through_due) - i128::from(from_due);
    if distance % i128::from(policy.interval) != 0 {
        return Err(DomainError::Invalid("grant boundaries are not aligned"));
    }
    let count = i64::try_from(distance / i128::from(policy.interval) + 1)
        .map_err(|_| DomainError::Overflow)?;
    if (reason == GrantReason::Initial && count != 1)
        || amount != Points(checked_mul(policy.amount.0, count)?)
        || next_grant != checked_add(through_due, policy.interval)?
    {
        return Err(DomainError::Invalid("grant facts disagree with policy"));
    }
    account.balance = checked_points_add(account.balance, amount)?;
    account.next_grant = next_grant;
    Ok(())
}

fn apply_resolution(
    state: &mut State,
    id: &MarketId,
    outcome: OutcomeIndex,
    resolver: UserId,
    settled_at: i64,
    payouts: &[Allocation],
    refunded: bool,
) -> Result<(), DomainError> {
    let market = state
        .markets
        .get(id)
        .ok_or(DomainError::Invalid("unknown market"))?;
    if market.status != Status::Open
        || outcome.0 >= market.options.len()
        || settled_at < market.closes_at
        || resolver == UserId(0)
    {
        return Err(DomainError::Invalid("invalid resolution transition"));
    }
    let recorded = recorded_allocations(payouts, &state.accounts)?;
    let winning_stakes = stakes(market, Some(outcome))?;
    if refunded {
        if !winning_stakes.is_empty() || recorded != stakes(market, None)? {
            return Err(DomainError::Invalid("invalid no-winner refunds"));
        }
    } else {
        if winning_stakes.is_empty()
            || recorded.len() != winning_stakes.len()
            || winning_stakes
                .iter()
                .any(|(user, stake)| recorded.get(user).is_none_or(|amount| amount < stake))
        {
            return Err(DomainError::Invalid("invalid winning allocations"));
        }
        let allocated = recorded.values().try_fold(Points(0), |total, amount| {
            checked_points_add(total, *amount)
        })?;
        if allocated != pool(market)? {
            return Err(DomainError::Invalid("allocations do not conserve the pool"));
        }
    }
    credit_allocations(&mut state.accounts, payouts)?;
    state.markets.get_mut(id).unwrap().status = Status::Resolved { outcome, refunded };
    Ok(())
}

fn apply_cancellation(
    state: &mut State,
    id: &MarketId,
    moderator: UserId,
    refunds: &[Allocation],
) -> Result<(), DomainError> {
    let market = state
        .markets
        .get(id)
        .ok_or(DomainError::Invalid("unknown market"))?;
    if market.status != Status::Open
        || moderator == UserId(0)
        || recorded_allocations(refunds, &state.accounts)? != stakes(market, None)?
    {
        return Err(DomainError::Invalid("invalid cancellation transition"));
    }
    credit_allocations(&mut state.accounts, refunds)?;
    state.markets.get_mut(id).unwrap().status = Status::Cancelled;
    Ok(())
}

fn decide_create(
    state: &State,
    actor: Actor,
    id: &MarketId,
    question: &str,
    options: &[String],
    closes_at: i64,
    now: i64,
) -> Result<Decision, DomainError> {
    if actor.bot || actor.user_id == UserId(0) {
        return Err(DomainError::Invalid("bots cannot create markets"));
    }
    if !state.accounts.contains_key(&actor.user_id) {
        return Err(DomainError::Invalid("member not enrolled"));
    }
    if !valid_market_id(id) || state.markets.contains_key(id) {
        return Err(DomainError::Invalid("invalid or duplicate market ID"));
    }
    valid_market(question, options)?;
    if closes_at <= now {
        return Err(DomainError::Invalid("market must close in the future"));
    }
    finish(
        state,
        vec![Event::MarketCreated {
            id: id.to_owned(),
            creator: actor.user_id,
            question: question.to_owned(),
            options: options.to_vec(),
            created_at: now,
            closes_at,
        }],
        format!("Market created: {id}"),
    )
}

fn apply_market_creation(
    state: &mut State,
    id: &MarketId,
    creator: UserId,
    question: &str,
    options: &[String],
    created_at: i64,
    closes_at: i64,
) -> Result<(), DomainError> {
    if state.policy.is_none()
        || !valid_market_id(id)
        || state.markets.contains_key(id)
        || !state.accounts.contains_key(&creator)
        || closes_at <= created_at
    {
        return Err(DomainError::Invalid("invalid market creation transition"));
    }
    valid_market(question, options)?;
    state.markets.insert(
        id.to_owned(),
        Market {
            creator,
            question: question.to_owned(),
            options: options.to_vec(),
            closes_at,
            created_at,
            status: Status::Open,
            bets: Vec::new(),
            total_staked: Points(0),
        },
    );
    Ok(())
}

fn apply_bet(
    state: &mut State,
    id: &MarketId,
    user_id: UserId,
    outcome: OutcomeIndex,
    amount: Points,
    accepted_at: i64,
) -> Result<(), DomainError> {
    let market = state
        .markets
        .get(id)
        .ok_or(DomainError::Invalid("unknown market"))?;
    if market.status != Status::Open
        || accepted_at < market.created_at
        || accepted_at >= market.closes_at
        || outcome.0 >= market.options.len()
        || amount <= Points(0)
    {
        return Err(DomainError::Invalid("invalid bet transition"));
    }
    let next_pool = checked_points_add(pool(market)?, amount)?;
    let account = state
        .accounts
        .get_mut(&user_id)
        .ok_or(DomainError::Invalid("bet by unknown member"))?;
    if account.balance < amount {
        return Err(DomainError::Invalid("insufficient points"));
    }
    account.balance = Points(account.balance.0 - amount.0);
    let market = state.markets.get_mut(id).unwrap();
    market.total_staked = next_pool;
    market.bets.push(Bet {
        user_id,
        outcome,
        amount,
    });
    Ok(())
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
