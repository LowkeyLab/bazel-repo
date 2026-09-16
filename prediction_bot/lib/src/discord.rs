//! Guild slash-command transport. Economic decisions remain in the domain and store.
use std::{collections::BTreeSet, fmt::Write as _, sync::Arc, time::Duration};

use crate::{
    domain::{Actor, Command, DomainError, Market, Status},
    store::{Store, StoreError, View},
};
use chrono::DateTime;
use serenity::{
    Client,
    all::{
        CommandDataOptionValue, CommandInteraction, CommandOptionType, Context, GatewayIntents,
        Guild, GuildId, Interaction, Permissions, Ready,
    },
    builder::{CreateAllowedMentions, CreateCommand, CreateCommandOption, EditInteractionResponse},
    client::EventHandler,
};
use thiserror::Error;
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub guild_id: Option<u64>,
    pub user_id: u64,
    pub bot: bool,
    pub moderator: bool,
    pub subcommand: String,
    pub options: Vec<InputOption>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InputOption {
    pub name: String,
    pub value: InputValue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputValue {
    String(String),
    Integer(i64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Write(Command),
    Balance,
    Leaderboard,
    List,
    Show { id: String },
}

fn exact(input: &Input, fields: &[&str]) -> Result<(), &'static str> {
    if input.options.len() != fields.len()
        || fields
            .iter()
            .any(|field| input.options.iter().filter(|o| o.name == *field).count() != 1)
    {
        return Err("Invalid command options.");
    }
    Ok(())
}
fn text<'a>(input: &'a Input, field: &str) -> Result<&'a str, &'static str> {
    input
        .options
        .iter()
        .find(|o| o.name == field)
        .and_then(|o| match &o.value {
            InputValue::String(s) => Some(s.as_str()),
            InputValue::Integer(_) => None,
        })
        .ok_or("Enter a text value.")
}
fn integer(input: &Input, field: &str) -> Result<i64, &'static str> {
    input
        .options
        .iter()
        .find(|o| o.name == field)
        .and_then(|o| match o.value {
            InputValue::Integer(i) => Some(i),
            InputValue::String(_) => None,
        })
        .ok_or("Enter a whole number.")
}
fn market_id(input: &Input) -> Result<String, &'static str> {
    let value = text(input, "id")?.trim();
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err("Enter a valid market ID.");
    }
    Ok(value.to_owned())
}
fn outcome(input: &Input) -> Result<usize, &'static str> {
    let value = integer(input, "outcome")?;
    if !(1..=10).contains(&value) {
        return Err("Choose an outcome number from 1 to 10.");
    }
    usize::try_from(value - 1).map_err(|_| "Invalid outcome number.")
}
pub(crate) fn parse(input: &Input) -> Result<(u64, Actor, Action), &'static str> {
    let guild = input
        .guild_id
        .filter(|id| *id != 0)
        .ok_or("This command is available only in a server.")?;
    if input.bot || input.user_id == 0 {
        return Err("Bots cannot use the prediction economy.");
    }
    let actor = Actor {
        user_id: input.user_id,
        moderator: input.moderator,
        bot: input.bot,
    };
    let action = match input.subcommand.as_str() {
        "join" => {
            exact(input, &[])?;
            Action::Write(Command::Join)
        }
        "balance" => {
            exact(input, &[])?;
            Action::Balance
        }
        "leaderboard" => {
            exact(input, &[])?;
            Action::Leaderboard
        }
        "list" => {
            exact(input, &[])?;
            Action::List
        }
        "show" => {
            exact(input, &["id"])?;
            Action::Show {
                id: market_id(input)?,
            }
        }
        "create" => {
            exact(input, &["question", "options", "closes_at"])?;
            let question = text(input, "question")?.trim();
            if question.is_empty() || question.chars().count() > 200 {
                return Err("Question must contain 1 to 200 characters.");
            }
            let options: Vec<String> = text(input, "options")?
                .split('|')
                .map(str::trim)
                .map(str::to_owned)
                .collect();
            if !(2..=10).contains(&options.len())
                || options
                    .iter()
                    .any(|s| s.is_empty() || s.chars().count() > 80)
            {
                return Err("Provide 2 to 10 outcomes of up to 80 characters, separated by |.");
            }
            let mut distinct = BTreeSet::new();
            if options.iter().any(|s| !distinct.insert(s.to_lowercase())) {
                return Err("Outcome labels must be distinct.");
            }
            let closes_at = DateTime::parse_from_rfc3339(text(input, "closes_at")?.trim())
                .map_err(|_| "Enter a valid RFC 3339 close time.")?
                .timestamp();
            Action::Write(Command::Create {
                id: Uuid::now_v7().to_string(),
                question: question.to_owned(),
                options,
                closes_at,
            })
        }
        "bet" => {
            exact(input, &["id", "outcome", "amount"])?;
            let amount = integer(input, "amount")?;
            if amount <= 0 {
                return Err("Stake must be a positive whole number.");
            }
            Action::Write(Command::Bet {
                id: market_id(input)?,
                outcome: outcome(input)?,
                amount,
            })
        }
        "resolve" => {
            exact(input, &["id", "outcome"])?;
            Action::Write(Command::Resolve {
                id: market_id(input)?,
                outcome: outcome(input)?,
            })
        }
        "cancel" => {
            exact(input, &["id"])?;
            Action::Write(Command::Cancel {
                id: market_id(input)?,
            })
        }
        _ => return Err("Unknown market command."),
    };
    Ok((guild, actor, action))
}
fn from_discord(command: &CommandInteraction) -> Result<Input, &'static str> {
    if command.data.name != "market" || command.data.options.len() != 1 {
        return Err("Unknown market command.");
    }
    let sub = &command.data.options[0];
    let CommandDataOptionValue::SubCommand(fields) = &sub.value else {
        return Err("Invalid market command.");
    };
    let options = fields
        .iter()
        .map(|field| {
            let value = match &field.value {
                CommandDataOptionValue::String(s) => InputValue::String(s.clone()),
                CommandDataOptionValue::Integer(i) => InputValue::Integer(*i),
                _ => return Err("Invalid command options."),
            };
            Ok(InputOption {
                name: field.name.clone(),
                value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let moderator = command
        .member
        .as_ref()
        .and_then(|member| member.permissions)
        .is_some_and(|p| p.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD));
    Ok(Input {
        guild_id: command.guild_id.map(GuildId::get),
        user_id: command.user.id.get(),
        bot: command.user.bot,
        moderator,
        subcommand: sub.name.clone(),
        options,
    })
}
fn no_mentions() -> CreateAllowedMentions {
    CreateAllowedMentions::new()
        .everyone(false)
        .all_users(false)
        .all_roles(false)
        .replied_user(false)
}
fn truncate_to(content: &str, limit: usize) -> String {
    let mut result = String::new();
    let mut units = 0;
    for c in content.chars() {
        if units + c.len_utf16() > limit.saturating_sub(1) {
            result.push('…');
            break;
        }
        result.push(c);
        units += c.len_utf16();
    }
    result
}
fn truncate(content: &str) -> String {
    truncate_to(content, 2_000)
}
fn recoverable_defer_code(code: isize) -> bool {
    code == 40_060
}

async fn content_after_defer<F, Fut>(can_continue: bool, make_content: F) -> Option<String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = String>,
{
    if can_continue {
        Some(make_content().await)
    } else {
        None
    }
}

/// Verify a configured expected ID against the authenticated Discord application.
///
/// # Errors
/// Returns an error if the authenticated ID is zero or differs from the configured expectation.
pub fn verify_application_id(observed: u64, expected: Option<u64>) -> Result<u64, &'static str> {
    if observed == 0 || expected.is_some_and(|id| id == 0 || id != observed) {
        return Err("configured application ID does not match Discord token");
    }
    Ok(observed)
}

fn reply(content: &str) -> EditInteractionResponse {
    EditInteractionResponse::new()
        .content(truncate(content))
        .allowed_mentions(no_mentions())
}
fn safe_error(error: &StoreError) -> String {
    match error {
        StoreError::Domain(DomainError::Invalid(reason)) => {
            format!("Cannot complete command: {reason}.")
        }
        StoreError::Domain(DomainError::Overflow) => {
            "Cannot complete command: point total is too large.".to_owned()
        }
        _ => "The prediction economy is temporarily unavailable. Please try again.".to_owned(),
    }
}
fn render_query(view: &View, action: &Action, actor: Actor, now: i64) -> String {
    let state = &view.state;
    match action {
        Action::Balance => match state.accounts.get(&actor.user_id) {
            Some(account) => format!(
                "Available: {} points. Next grant: <t:{}:f>.",
                account.balance, account.next_grant
            ),
            None => "You are not enrolled. Use /market join first.".to_owned(),
        },
        Action::Leaderboard => {
            let mut accounts: Vec<_> = state.accounts.iter().collect();
            accounts.sort_by(|(ua, a), (ub, b)| b.balance.cmp(&a.balance).then_with(|| ua.cmp(ub)));
            if accounts.is_empty() {
                return "No members are enrolled yet.".to_owned();
            }
            let mut out = "Top balances:\n".to_owned();
            for (n, (user, account)) in accounts.into_iter().take(10).enumerate() {
                let _ = writeln!(
                    out,
                    "{}. User ID {} — {} points",
                    n + 1,
                    user,
                    account.balance
                );
            }
            out
        }
        Action::List => {
            let mut markets: Vec<_> = state
                .markets
                .iter()
                .filter(|(_, m)| m.status == Status::Open && now < m.closes_at)
                .collect();
            markets.sort_by(|(ida, a), (idb, b)| {
                b.created_at.cmp(&a.created_at).then_with(|| ida.cmp(idb))
            });
            if markets.is_empty() {
                return "No markets are open for betting.".to_owned();
            }
            let mut out = "Newest open markets:\n".to_owned();
            for (id, m) in markets.into_iter().take(10) {
                let _ = writeln!(
                    out,
                    "{} — {} (closes <t:{}:f>)",
                    id,
                    truncate_to(&m.question, 60),
                    m.closes_at
                );
            }
            out
        }
        Action::Show { id } => state.markets.get(id).map_or_else(
            || "No market with that ID exists in this server.".to_owned(),
            |m| render_market(id, m, now),
        ),
        Action::Write(_) => "Invalid query.".to_owned(),
    }
}
fn render_market(id: &str, market: &Market, now: i64) -> String {
    let status = match &market.status {
        Status::Open if now < market.closes_at => "Open",
        Status::Open => "Closed; awaiting outcome",
        Status::Resolved { refunded: true, .. } => "Resolved; no-winner refund",
        Status::Resolved { .. } => "Resolved",
        Status::Cancelled => "Cancelled",
    };
    let mut out = format!(
        "{}\nID: {}\nStatus: {}\nCloses: <t:{}:f>\n",
        truncate_to(&market.question, 200),
        id,
        status,
        market.closes_at
    );
    for (n, option) in market.options.iter().enumerate() {
        let total: i128 = market
            .bets
            .iter()
            .filter(|bet| bet.outcome == n)
            .map(|bet| i128::from(bet.amount))
            .sum();
        let _ = writeln!(
            out,
            "{}. {} — {} points pooled",
            n + 1,
            truncate_to(option, 80),
            total
        );
    }
    if let Status::Resolved { outcome, .. } = market.status {
        let _ = writeln!(out, "Winning outcome: {}", outcome + 1);
    }
    out
}
fn market_command() -> CreateCommand {
    use CommandOptionType::{Integer, String as Text, SubCommand};
    let required =
        |kind, name, description| CreateCommandOption::new(kind, name, description).required(true);
    CreateCommand::new("market")
        .description("Play-point prediction markets in this server")
        .add_option(CreateCommandOption::new(
            SubCommand,
            "join",
            "Enroll and receive your first grant",
        ))
        .add_option(CreateCommandOption::new(
            SubCommand,
            "balance",
            "Show your available points and next grant",
        ))
        .add_option(CreateCommandOption::new(
            SubCommand,
            "leaderboard",
            "Show the ten highest available balances",
        ))
        .add_option(
            CreateCommandOption::new(SubCommand, "create", "Create a prediction market")
                .add_sub_option(
                    required(Text, "question", "Question, up to 200 characters").max_length(200),
                )
                .add_sub_option(required(
                    Text,
                    "options",
                    "Outcomes separated by |, for example Yes | No",
                ))
                .add_sub_option(required(
                    Text,
                    "closes_at",
                    "RFC 3339 close time, for example 2030-01-02T03:04:05Z",
                )),
        )
        .add_option(CreateCommandOption::new(
            SubCommand,
            "list",
            "List the ten newest open markets",
        ))
        .add_option(
            CreateCommandOption::new(SubCommand, "show", "Inspect a market")
                .add_sub_option(required(Text, "id", "Market ID")),
        )
        .add_option(
            CreateCommandOption::new(SubCommand, "bet", "Stake points on an outcome")
                .add_sub_option(required(Text, "id", "Market ID"))
                .add_sub_option(
                    required(Integer, "outcome", "Outcome number shown by /market show")
                        .min_int_value(1),
                )
                .add_sub_option(
                    required(Integer, "amount", "Positive whole-point stake").min_int_value(1),
                ),
        )
        .add_option(
            CreateCommandOption::new(
                SubCommand,
                "resolve",
                "Settle a closed market; Manage Guild required",
            )
            .add_sub_option(required(Text, "id", "Market ID"))
            .add_sub_option(
                required(Integer, "outcome", "Winning outcome number").min_int_value(1),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                SubCommand,
                "cancel",
                "Cancel an unresolved market; Manage Guild required",
            )
            .add_sub_option(required(Text, "id", "Market ID")),
        )
}
struct Handler {
    store: Arc<Store>,
}
impl Handler {
    async fn register(&self, ctx: &Context, guild: GuildId) {
        if guild
            .set_commands(&ctx.http, vec![market_command()])
            .await
            .is_err()
        {
            tracing::error!(guild = guild.get(), "market command registration failed");
        }
    }
    async fn handle(&self, ctx: &Context, command: CommandInteraction) {
        let can_continue = match command.defer_ephemeral(&ctx.http).await {
            Ok(()) => true,
            Err(serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(
                response,
            ))) => recoverable_defer_code(response.error.code),
            Err(_) => false,
        };
        let content = content_after_defer(can_continue, || async {
            match from_discord(&command).and_then(|input| parse(&input)) {
                Ok((guild, actor, Action::Write(request))) => {
                    let key = format!("discord:{}", command.id.get());
                    match self.store.execute(guild, &key, actor, &request).await {
                        Ok(message) => message,
                        Err(error) => {
                            tracing::error!(guild, "market command failed");
                            safe_error(&error)
                        }
                    }
                }
                Ok((guild, actor, query)) => {
                    if let Ok(view) = self.store.view(guild).await {
                        render_query(&view, &query, actor, chrono::Utc::now().timestamp())
                    } else {
                        tracing::error!(guild, "market query failed");
                        "The prediction economy is temporarily unavailable. Please try again."
                            .to_owned()
                    }
                }
                Err(message) => message.to_owned(),
            }
        })
        .await;
        let Some(content) = content else {
            tracing::warn!("could not defer market interaction");
            return;
        };
        if command
            .edit_response(&ctx.http, reply(&content))
            .await
            .is_err()
        {
            tracing::warn!("could not deliver market interaction response");
        }
    }
}
#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        tracing::info!(guilds = ready.guilds.len(), "Discord gateway ready");
        for guild in ready.guilds {
            self.register(&ctx, guild.id).await;
        }
    }
    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        self.register(&ctx, guild.id).await;
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            self.handle(&ctx, command).await;
        }
    }
}
#[derive(Debug, Error)]
pub enum DiscordError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("Discord gateway failed")]
    Gateway,
    #[error("cannot initialize shutdown signal")]
    Signal(#[from] std::io::Error),
}
async fn grant_worker(store: Arc<Store>, mut shutdown: watch::Receiver<bool>) {
    let mut timer = tokio::time::interval(Duration::from_mins(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = shutdown.changed() => { if changed.is_err() || *shutdown.borrow() { break; } }
            _ = timer.tick() => { if store.grant_due().await.is_err() { tracing::error!("grant discovery failed"); } }
        }
    }
}
#[cfg(unix)]
async fn shutdown_signal() -> Result<(), std::io::Error> {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! { result = tokio::signal::ctrl_c() => result, _ = term.recv() => Ok(()) }
}
#[cfg(not(unix))]
async fn shutdown_signal() -> Result<(), std::io::Error> {
    tokio::signal::ctrl_c().await
}
/// Run the gateway and worker while holding one `PostgreSQL` advisory lock.
///
/// # Errors
/// Returns an error if the gateway lock, Discord connection, or shutdown signal cannot initialize,
/// or if the gateway fails while running.
pub async fn run(store: Arc<Store>, token: String) -> Result<(), DiscordError> {
    let mut guard = store.gateway_guard().await?;
    guard.close_on_drop();
    let result = run_gateway(store, token).await;
    if guard.close().await.is_err() {
        tracing::warn!("could not close gateway lock connection");
    }
    result
}
async fn run_gateway(store: Arc<Store>, token: String) -> Result<(), DiscordError> {
    let mut client = Client::builder(&token, GatewayIntents::GUILDS)
        .event_handler(Handler {
            store: Arc::clone(&store),
        })
        .await
        .map_err(|_| DiscordError::Gateway)?;
    let shards = Arc::clone(&client.shard_manager);
    let (sender, receiver) = watch::channel(false);
    let mut worker = tokio::spawn(grant_worker(store, receiver));
    let mut gateway = Box::pin(client.start_autosharded());
    let result = tokio::select! {
        result = &mut gateway => result.map_err(|_| DiscordError::Gateway),
        result = shutdown_signal() => {
            match result {
                Ok(()) => {
                    tracing::info!("shutdown requested");
                    shards.shutdown_all().await;
                    if tokio::time::timeout(Duration::from_secs(15), &mut gateway).await.is_err() { tracing::warn!("gateway shutdown timed out"); }
                    Ok(())
                }
                Err(error) => Err(DiscordError::Signal(error)),
            }
        }
    };
    let _ = sender.send(true);
    shards.shutdown_all().await;
    if tokio::time::timeout(Duration::from_secs(15), &mut worker)
        .await
        .is_err()
    {
        tracing::warn!("grant worker shutdown timed out");
        worker.abort();
        let _ = worker.await;
    }
    tracing::info!("Discord bot stopped");
    result
}
#[cfg(test)]
#[path = "discord_tests.rs"]
mod tests;
