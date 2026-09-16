use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod snowflake {
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

    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        if *value == 0 {
            return Err(S::Error::custom("event Discord ID must be positive"));
        }
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse(&value, false).map_err(D::Error::custom)
    }

    pub mod actor {
        use serde::{Deserialize, Deserializer, Serializer, de::Error};

        pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&value.to_string())
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
            let value = String::deserialize(deserializer)?;
            super::parse(&value, true).map_err(D::Error::custom)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub amount: i64,
    pub interval: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    #[serde(with = "snowflake::actor")]
    pub user_id: u64,
    pub moderator: bool,
    pub bot: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Join,
    Create {
        id: String,
        question: String,
        options: Vec<String>,
        closes_at: i64,
    },
    Bet {
        id: String,
        outcome: usize,
        amount: i64,
    },
    Resolve {
        id: String,
        outcome: usize,
    },
    Cancel {
        id: String,
    },
    Grant {
        #[serde(with = "snowflake")]
        user_id: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub policy: Option<Policy>,
    pub accounts: BTreeMap<u64, Account>,
    pub markets: BTreeMap<String, Market>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub balance: i64,
    pub next_grant: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Market {
    #[serde(with = "snowflake")]
    pub creator: u64,
    pub question: String,
    pub options: Vec<String>,
    pub closes_at: i64,
    pub created_at: i64,
    pub status: Status,
    pub bets: Vec<Bet>,
    pub total_staked: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Open,
    Resolved { outcome: usize, refunded: bool },
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bet {
    #[serde(with = "snowflake")]
    pub user_id: u64,
    pub outcome: usize,
    pub amount: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allocation {
    #[serde(with = "snowflake")]
    pub user_id: u64,
    pub amount: i64,
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
        amount: i64,
        interval: i64,
    },
    MemberEnrolled {
        #[serde(with = "snowflake")]
        user_id: u64,
        enrolled_at: i64,
    },
    PointsGranted {
        #[serde(with = "snowflake")]
        user_id: u64,
        reason: GrantReason,
        amount: i64,
        from_due: i64,
        through_due: i64,
        next_grant: i64,
    },
    MarketCreated {
        id: String,
        #[serde(with = "snowflake")]
        creator: u64,
        question: String,
        options: Vec<String>,
        created_at: i64,
        closes_at: i64,
    },
    BetPlaced {
        id: String,
        #[serde(with = "snowflake")]
        user_id: u64,
        outcome: usize,
        amount: i64,
        accepted_at: i64,
    },
    MarketResolved {
        id: String,
        outcome: usize,
        #[serde(with = "snowflake")]
        resolver: u64,
        settled_at: i64,
        payouts: Vec<Allocation>,
        refunded: bool,
    },
    MarketCancelled {
        id: String,
        #[serde(with = "snowflake")]
        moderator: u64,
        cancelled_at: i64,
        refunds: Vec<Allocation>,
    },
}

impl Event {
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
    if policy.amount <= 0 || policy.interval <= 0 {
        return Err(DomainError::Invalid("grant policy must be positive"));
    }
    Ok(())
}

fn checked_add(a: i64, b: i64) -> Result<i64, DomainError> {
    a.checked_add(b).ok_or(DomainError::Overflow)
}

fn checked_mul(a: i64, b: i64) -> Result<i64, DomainError> {
    a.checked_mul(b).ok_or(DomainError::Overflow)
}

fn valid_market_id(id: &str) -> bool {
    let bytes = id.as_bytes();
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

fn pool(market: &Market) -> Result<i64, DomainError> {
    if market.total_staked < 0 {
        return Err(DomainError::Invalid("negative market pool"));
    }
    Ok(market.total_staked)
}

fn stakes(market: &Market, outcome: Option<usize>) -> Result<BTreeMap<u64, i64>, DomainError> {
    let mut totals = BTreeMap::new();
    for bet in &market.bets {
        if outcome.is_none_or(|selected| selected == bet.outcome) {
            let old = totals.get(&bet.user_id).copied().unwrap_or(0);
            totals.insert(bet.user_id, checked_add(old, bet.amount)?);
        }
    }
    Ok(totals)
}

fn allocations(totals: BTreeMap<u64, i64>) -> Vec<Allocation> {
    totals
        .into_iter()
        .map(|(user_id, amount)| Allocation { user_id, amount })
        .collect()
}

fn payouts(market: &Market, outcome: usize) -> Result<(Vec<Allocation>, bool), DomainError> {
    let winners = stakes(market, Some(outcome))?;
    if winners.is_empty() {
        return Ok((allocations(stakes(market, None)?), true));
    }
    let total_pool = i128::from(pool(market)?);
    let total_winning_stake: i128 = winners.values().map(|stake| i128::from(*stake)).sum();
    let mut entries: Vec<(u64, i64, i128)> = winners
        .into_iter()
        .map(|(user_id, stake)| {
            let numerator = total_pool * i128::from(stake);
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
            .map(|(user_id, amount, _)| Allocation { user_id, amount })
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

pub fn decide(
    state: &State,
    actor: Actor,
    command: &Command,
    now: i64,
    defaults: Policy,
) -> Result<Decision, DomainError> {
    match command {
        Command::Join => {
            if actor.bot || actor.user_id == 0 {
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
        Command::Grant { user_id } => {
            if actor.user_id != 0 {
                return Err(DomainError::Invalid("grants require the system actor"));
            }
            let policy = state
                .policy
                .ok_or(DomainError::Invalid("economy not initialized"))?;
            positive_policy(policy)?;
            let account = state
                .accounts
                .get(user_id)
                .ok_or(DomainError::Invalid("member not enrolled"))?;
            if now < account.next_grant {
                return finish(state, Vec::new(), "No grant due.".to_owned());
            }
            let count128 = (i128::from(now) - i128::from(account.next_grant))
                / i128::from(policy.interval)
                + 1;
            let count = i64::try_from(count128).map_err(|_| DomainError::Overflow)?;
            let amount = checked_mul(policy.amount, count)?;
            let through_due =
                checked_add(account.next_grant, checked_mul(policy.interval, count - 1)?)?;
            let next_grant = checked_add(through_due, policy.interval)?;
            finish(
                state,
                vec![Event::PointsGranted {
                    user_id: *user_id,
                    reason: GrantReason::Periodic,
                    amount,
                    from_due: account.next_grant,
                    through_due,
                    next_grant,
                }],
                format!("Granted {amount} points."),
            )
        }
        Command::Create {
            id,
            question,
            options,
            closes_at,
        } => {
            if actor.bot || actor.user_id == 0 {
                return Err(DomainError::Invalid("bots cannot create markets"));
            }
            if !state.accounts.contains_key(&actor.user_id) {
                return Err(DomainError::Invalid("member not enrolled"));
            }
            if !valid_market_id(id) || state.markets.contains_key(id) {
                return Err(DomainError::Invalid("invalid or duplicate market ID"));
            }
            valid_market(question, options)?;
            if *closes_at <= now {
                return Err(DomainError::Invalid("market must close in the future"));
            }
            finish(
                state,
                vec![Event::MarketCreated {
                    id: id.clone(),
                    creator: actor.user_id,
                    question: question.clone(),
                    options: options.clone(),
                    created_at: now,
                    closes_at: *closes_at,
                }],
                format!("Market created: {id}"),
            )
        }
        Command::Bet {
            id,
            outcome,
            amount,
        } => {
            if actor.bot || actor.user_id == 0 {
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
            if *outcome >= market.options.len() || *amount <= 0 {
                return Err(DomainError::Invalid("invalid outcome or stake"));
            }
            if *amount > account.balance {
                return Err(DomainError::Invalid("insufficient points"));
            }
            checked_add(pool(market)?, *amount)?;
            finish(
                state,
                vec![Event::BetPlaced {
                    id: id.clone(),
                    user_id: actor.user_id,
                    outcome: *outcome,
                    amount: *amount,
                    accepted_at: now,
                }],
                format!("Staked {amount} points."),
            )
        }
        Command::Resolve { id, outcome } => {
            if !actor.moderator || actor.bot || actor.user_id == 0 {
                return Err(DomainError::Invalid("moderator required"));
            }
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if market.status != Status::Open || now < market.closes_at {
                return Err(DomainError::Invalid("market is not ready to resolve"));
            }
            if *outcome >= market.options.len() {
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
        Command::Cancel { id } => {
            if !actor.moderator || actor.bot || actor.user_id == 0 {
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
                    id: id.clone(),
                    moderator: actor.user_id,
                    cancelled_at: now,
                    refunds: allocations(stakes(market, None)?),
                }],
                "Market cancelled; stakes refunded.".to_owned(),
            )
        }
    }
}

fn recorded_allocations(
    entries: &[Allocation],
    accounts: &BTreeMap<u64, Account>,
) -> Result<BTreeMap<u64, i64>, DomainError> {
    let mut recorded = BTreeMap::new();
    for allocation in entries {
        if allocation.amount <= 0
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
    accounts: &mut BTreeMap<u64, Account>,
    entries: &[Allocation],
) -> Result<(), DomainError> {
    for allocation in entries {
        let account = accounts
            .get_mut(&allocation.user_id)
            .ok_or(DomainError::Invalid("unknown allocation member"))?;
        account.balance = checked_add(account.balance, allocation.amount)?;
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
            if state.policy.is_none() || *user_id == 0 || state.accounts.contains_key(user_id) {
                return Err(DomainError::Invalid("invalid enrollment transition"));
            }
            state.accounts.insert(
                *user_id,
                Account {
                    balance: 0,
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
        } => {
            let policy = state
                .policy
                .ok_or(DomainError::Invalid("economy not initialized"))?;
            positive_policy(policy)?;
            let account = state
                .accounts
                .get_mut(user_id)
                .ok_or(DomainError::Invalid("grant to unknown member"))?;
            if *from_due != account.next_grant || *through_due < *from_due || *amount <= 0 {
                return Err(DomainError::Invalid("invalid grant schedule"));
            }
            let distance = i128::from(*through_due) - i128::from(*from_due);
            if distance % i128::from(policy.interval) != 0 {
                return Err(DomainError::Invalid("grant boundaries are not aligned"));
            }
            let count = i64::try_from(distance / i128::from(policy.interval) + 1)
                .map_err(|_| DomainError::Overflow)?;
            if (*reason == GrantReason::Initial && count != 1)
                || *amount != checked_mul(policy.amount, count)?
                || *next_grant != checked_add(*through_due, policy.interval)?
            {
                return Err(DomainError::Invalid("grant facts disagree with policy"));
            }
            account.balance = checked_add(account.balance, *amount)?;
            account.next_grant = *next_grant;
        }
        Event::MarketCreated {
            id,
            creator,
            question,
            options,
            created_at,
            closes_at,
        } => {
            if state.policy.is_none()
                || !valid_market_id(id)
                || state.markets.contains_key(id)
                || !state.accounts.contains_key(creator)
                || *closes_at <= *created_at
            {
                return Err(DomainError::Invalid("invalid market creation transition"));
            }
            valid_market(question, options)?;
            state.markets.insert(
                id.clone(),
                Market {
                    creator: *creator,
                    question: question.clone(),
                    options: options.clone(),
                    closes_at: *closes_at,
                    created_at: *created_at,
                    status: Status::Open,
                    bets: Vec::new(),
                    total_staked: 0,
                },
            );
        }
        Event::BetPlaced {
            id,
            user_id,
            outcome,
            amount,
            accepted_at,
        } => {
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if market.status != Status::Open
                || *accepted_at < market.created_at
                || *accepted_at >= market.closes_at
                || *outcome >= market.options.len()
                || *amount <= 0
            {
                return Err(DomainError::Invalid("invalid bet transition"));
            }
            let next_pool = checked_add(pool(market)?, *amount)?;
            let account = state
                .accounts
                .get_mut(user_id)
                .ok_or(DomainError::Invalid("bet by unknown member"))?;
            if account.balance < *amount {
                return Err(DomainError::Invalid("insufficient points"));
            }
            account.balance -= *amount;
            let market = state.markets.get_mut(id).unwrap();
            market.total_staked = next_pool;
            market.bets.push(Bet {
                user_id: *user_id,
                outcome: *outcome,
                amount: *amount,
            });
        }
        Event::MarketResolved {
            id,
            outcome,
            resolver,
            settled_at,
            payouts,
            refunded,
        } => {
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if market.status != Status::Open
                || *outcome >= market.options.len()
                || *settled_at < market.closes_at
                || *resolver == 0
            {
                return Err(DomainError::Invalid("invalid resolution transition"));
            }
            let recorded = recorded_allocations(payouts, &state.accounts)?;
            let winning_stakes = stakes(market, Some(*outcome))?;
            if *refunded {
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
                let allocated = recorded
                    .values()
                    .try_fold(0, |total, amount| checked_add(total, *amount))?;
                if allocated != pool(market)? {
                    return Err(DomainError::Invalid("allocations do not conserve the pool"));
                }
            }
            credit_allocations(&mut state.accounts, payouts)?;
            state.markets.get_mut(id).unwrap().status = Status::Resolved {
                outcome: *outcome,
                refunded: *refunded,
            };
        }
        Event::MarketCancelled {
            id,
            moderator,
            refunds,
            ..
        } => {
            let market = state
                .markets
                .get(id)
                .ok_or(DomainError::Invalid("unknown market"))?;
            if market.status != Status::Open
                || *moderator == 0
                || recorded_allocations(refunds, &state.accounts)? != stakes(market, None)?
            {
                return Err(DomainError::Invalid("invalid cancellation transition"));
            }
            credit_allocations(&mut state.accounts, refunds)?;
            state.markets.get_mut(id).unwrap().status = Status::Cancelled;
        }
    }
    Ok(())
}

pub fn apply(state: &mut State, event: &Event) -> Result<(), DomainError> {
    let mut candidate = state.clone();
    apply_inner(&mut candidate, event)?;
    *state = candidate;
    Ok(())
}

pub fn replay(events: &[Event]) -> Result<State, DomainError> {
    let mut candidate = State::default();
    for event in events {
        apply_inner(&mut candidate, event)?;
    }
    Ok(candidate)
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
