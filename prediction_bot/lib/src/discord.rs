//! Guild slash-command transport. Economic decisions remain in the domain and store.
use std::{
    collections::BTreeSet,
    fmt::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::types::{ApplicationId, ChannelId, GuildId, MarketId, OutcomeIndex, Points, UserId};
use crate::{
    announcements::ConfigurationChange,
    domain::{Actor, Command, DomainError, Market, Status},
    store::{Store, StoreError, View},
};
use chrono::DateTime;
use serenity::{
    Client,
    all::{
        ActionRowComponent, ChannelType, CommandDataOptionValue, CommandInteraction,
        CommandOptionType, ComponentInteraction, ComponentInteractionDataKind, Context,
        GatewayIntents, Guild, GuildId as SerenityGuildId, Interaction, Message, ModalInteraction,
        Permissions, Ready,
    },
    builder::{
        CreateAllowedMentions, CreateCommand, CreateCommandOption, CreateInteractionResponse,
        CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse,
    },
    client::{ClientBuilder, EventHandler},
    http::Http,
};
use thiserror::Error;
use tokio::sync::watch;
use uuid::Uuid;

mod announcements;
pub mod transport;
#[path = "discord_ui.rs"]
mod ui;
use crate::audit::{
    AuditEvent, AuditListener, Failure, FailureCategory, LifecycleKind, Outcome, QueryKind,
    Rejection, Stage, discord_failure, store_outcome,
};
use transport::{InteractionTransport, SerenityTransport};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub guild_id: Option<GuildId>,
    pub user_id: UserId,
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
    Channel(ChannelId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Write(Command),
    CreateForm,
    Help,
    Balance,
    Leaderboard,
    List,
    Show { id: MarketId },
    AnnouncementsSet { channel_id: ChannelId },
    AnnouncementsStatus,
    AnnouncementsDisable,
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
            InputValue::Integer(_) | InputValue::Channel(_) => None,
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
            InputValue::String(_) | InputValue::Channel(_) => None,
        })
        .ok_or("Enter a whole number.")
}
fn channel_id(input: &Input, field: &str) -> Result<ChannelId, &'static str> {
    input
        .options
        .iter()
        .find(|option| option.name == field)
        .and_then(|option| match option.value {
            InputValue::Channel(id) if id.0 != 0 => Some(id),
            InputValue::Channel(_) | InputValue::String(_) | InputValue::Integer(_) => None,
        })
        .ok_or("Choose a server text channel.")
}

fn market_id(input: &Input) -> Result<MarketId, &'static str> {
    let value = text(input, "id")?.trim();
    if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
        return Err("Enter a valid market ID.");
    }
    Ok(value.into())
}
fn outcome(input: &Input) -> Result<OutcomeIndex, &'static str> {
    let value = integer(input, "outcome")?;
    if !(1..=10).contains(&value) {
        return Err("Choose an outcome number from 1 to 10.");
    }
    usize::try_from(value - 1)
        .map(OutcomeIndex)
        .map_err(|_| "Invalid outcome number.")
}
fn bet_request(input: &Input) -> Result<Command, &'static str> {
    exact(input, &["id", "outcome", "amount"])?;
    let amount = integer(input, "amount")?;
    if amount <= 0 {
        return Err("Stake must be a positive whole number.");
    }
    Ok(Command::Bet {
        id: market_id(input)?,
        outcome: outcome(input)?,
        amount: Points(amount),
    })
}

pub(crate) fn parse(input: &Input) -> Result<(GuildId, Actor, Action), &'static str> {
    let guild = input
        .guild_id
        .filter(|id| id.0 != 0)
        .ok_or("This command is available only in a server.")?;
    if input.bot || input.user_id.0 == 0 {
        return Err("Bots cannot use the prediction economy.");
    }
    let actor = Actor {
        user_id: input.user_id,
        moderator: input.moderator,
        bot: input.bot,
    };
    let action = match input.subcommand.as_str() {
        "help" => {
            exact(input, &[])?;
            Action::Help
        }
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
        "create" if input.options.is_empty() => Action::CreateForm,
        "create" => {
            exact(input, &["question", "options", "closes_at"])?;
            let options = text(input, "options")?
                .split('|')
                .map(|s| s.trim().to_owned())
                .collect();
            let closes_at = DateTime::parse_from_rfc3339(text(input, "closes_at")?.trim())
                .map_err(|_| "Enter a valid RFC 3339 close time.")?
                .timestamp();
            Action::Write(create_request(
                text(input, "question")?,
                options,
                closes_at,
            )?)
        }

        "bet" => Action::Write(bet_request(input)?),
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
        "announcements.set" => {
            if !input.moderator {
                return Err("Administrator or Manage Guild permission is required.");
            }
            exact(input, &["channel"])?;
            Action::AnnouncementsSet {
                channel_id: channel_id(input, "channel")?,
            }
        }
        "announcements.status" => {
            if !input.moderator {
                return Err("Administrator or Manage Guild permission is required.");
            }
            exact(input, &[])?;
            Action::AnnouncementsStatus
        }
        "announcements.disable" => {
            if !input.moderator {
                return Err("Administrator or Manage Guild permission is required.");
            }
            exact(input, &[])?;
            Action::AnnouncementsDisable
        }
        _ => return Err("Unknown market command."),
    };
    Ok((guild, actor, action))
}
fn create_request(
    question: &str,
    options: Vec<String>,
    closes_at: i64,
) -> Result<Command, &'static str> {
    let question = question.trim();
    if question.is_empty() || question.chars().count() > 200 {
        return Err("Question must contain 1 to 200 characters.");
    }
    if !(2..=10).contains(&options.len())
        || options
            .iter()
            .any(|s| s.is_empty() || s.chars().count() > 80)
    {
        return Err("Provide 2 to 10 outcomes of up to 80 characters each.");
    }
    let mut distinct = BTreeSet::new();
    if options.iter().any(|s| !distinct.insert(s.to_lowercase())) {
        return Err("Outcome labels must be distinct.");
    }
    Ok(Command::Create {
        id: Uuid::now_v7().to_string().into(),
        question: question.to_owned(),
        options,
        closes_at,
    })
}
fn from_discord(command: &CommandInteraction) -> Result<Input, &'static str> {
    if command.data.name != "market" || command.data.options.len() != 1 {
        return Err("Unknown market command.");
    }
    let root = &command.data.options[0];
    let (subcommand, fields) = match &root.value {
        CommandDataOptionValue::SubCommand(fields) => (root.name.clone(), fields),
        CommandDataOptionValue::SubCommandGroup(commands)
            if root.name == "announcements" && commands.len() == 1 =>
        {
            let command = &commands[0];
            if !matches!(command.name.as_str(), "set" | "status" | "disable") {
                return Err("Invalid market command.");
            }
            let CommandDataOptionValue::SubCommand(fields) = &command.value else {
                return Err("Invalid market command.");
            };
            (format!("announcements.{}", command.name), fields)
        }
        _ => return Err("Invalid market command."),
    };
    let options = fields
        .iter()
        .map(|field| {
            let value = match &field.value {
                CommandDataOptionValue::String(s) => InputValue::String(s.clone()),
                CommandDataOptionValue::Integer(i) => InputValue::Integer(*i),
                CommandDataOptionValue::Channel(id) => InputValue::Channel(ChannelId(id.get())),
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
        guild_id: command.guild_id.map(|id| GuildId(id.get())),
        user_id: UserId(command.user.id.get()),
        bot: command.user.bot,
        moderator,
        subcommand,
        options,
    })
}
const HELP: &str = "I run prediction markets for this server using play points—no real money.

• `/market join` — get your first points and receive regular grants.
• `/market create` — ask a question and choose possible outcomes.
• `/market list` — browse markets, pick an outcome, and bet points.
• `/market balance` and `/market leaderboard` — check your points and rankings.

Server admins and members with Manage Guild permission can resolve or cancel markets. They can use /market announcements set to choose a server text channel where I have View Channel and Send Messages, /market announcements status to inspect delivery, or /market announcements disable to stop delivery and discard pending announcements.

Announcements cover new participant registration, each accepted bet, market creation, resolution, and cancellation. Bet announcements show the market and total bet count, without bettor, outcome, or stake details. They retry with increasing delays, do not backfill older events, and may be delivered twice after an uncertain Discord response. Changing or disabling the channel cannot stop an announcement already in flight.";

fn mention_reply(message: &Message, bot_user_id: UserId) -> Option<CreateMessage> {
    if message.guild_id.is_none()
        || message.author.bot
        || bot_user_id.0 == 0
        || !message
            .mentions
            .iter()
            .any(|user| user.id.get() == bot_user_id.0)
    {
        return None;
    }
    Some(CreateMessage::new()
        .content("Need a hand? Run `/market help` to learn how to join, create predictions, and bet with play points.")
        .allowed_mentions(no_mentions()))
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

fn defer_succeeded(result: serenity::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(response))) => {
            recoverable_defer_code(response.error.code)
        }
        Err(_) => false,
    }
}
fn interaction_error(message: &str) -> CreateInteractionResponse {
    CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content(truncate(message))
            .ephemeral(true)
            .allowed_mentions(no_mentions()),
    )
}

async fn content_after_defer<F, Fut, T>(can_continue: bool, make_content: F) -> Option<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
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
pub fn verify_application_id(
    observed: ApplicationId,
    expected: Option<ApplicationId>,
) -> Result<ApplicationId, &'static str> {
    if observed.0 == 0 || expected.is_some_and(|id| id.0 == 0 || id != observed) {
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
        Action::Help => HELP.to_owned(),
        Action::CreateForm => "Choose an outcome preset to create a market.".to_owned(),
        Action::Write(_)
        | Action::AnnouncementsSet { .. }
        | Action::AnnouncementsStatus
        | Action::AnnouncementsDisable => "Invalid query.".to_owned(),
    }
}
fn render_market(id: &MarketId, market: &Market, now: i64) -> String {
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
            .filter(|bet| bet.outcome.0 == n)
            .map(|bet| i128::from(bet.amount.0))
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
        let _ = writeln!(out, "Winning outcome: {}", outcome.0 + 1);
    }
    out
}
#[expect(
    clippy::too_many_lines,
    reason = "Keep the declarative market command schema together"
)]
fn market_command() -> CreateCommand {
    use CommandOptionType::{Channel, Integer, String as Text, SubCommand, SubCommandGroup};
    let required =
        |kind, name, description| CreateCommandOption::new(kind, name, description).required(true);
    CreateCommand::new("market")
        .description("Play-point prediction markets in this server")
        .add_option(CreateCommandOption::new(
            SubCommand,
            "help",
            "Learn how to use prediction markets",
        ))
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
                    CreateCommandOption::new(Text, "question", "Question, up to 200 characters")
                        .max_length(200),
                )
                .add_sub_option(CreateCommandOption::new(
                    Text,
                    "options",
                    "Outcomes separated by |, for example Yes | No",
                ))
                .add_sub_option(CreateCommandOption::new(
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
        .add_option(
            CreateCommandOption::new(
                SubCommandGroup,
                "announcements",
                "Configure market announcements; Manage Guild required",
            )
            .add_sub_option(
                CreateCommandOption::new(SubCommand, "set", "Set the announcement destination")
                    .add_sub_option(
                        required(Channel, "channel", "Server text channel")
                            .channel_types(vec![ChannelType::Text]),
                    ),
            )
            .add_sub_option(CreateCommandOption::new(
                SubCommand,
                "status",
                "Show announcement delivery status",
            ))
            .add_sub_option(CreateCommandOption::new(
                SubCommand,
                "disable",
                "Disable announcements and discard pending deliveries",
            )),
        )
}
fn interaction_event(
    audit: &dyn AuditListener,
    guild: Option<GuildId>,
    interaction_id: u64,
    stage: Stage,
    outcome: Outcome,
) {
    audit.on_event(&AuditEvent::InteractionCompleted {
        guild,
        interaction_id,
        stage,
        outcome,
    });
}
fn delivery_outcome(result: &serenity::Result<()>) -> Outcome {
    match result {
        Ok(()) => Outcome::Succeeded,
        Err(error) => Outcome::Failed(discord_failure(error)),
    }
}
fn category_failure(category: FailureCategory) -> Outcome {
    Outcome::Failed(Failure {
        category,
        sqlstate: None,
        http_status: None,
        discord_code: None,
    })
}
async fn deferred_response<F, Fut>(
    transport: &dyn InteractionTransport,
    audit: &dyn AuditListener,
    guild: Option<GuildId>,
    interaction_id: u64,
    make_content: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = EditInteractionResponse>,
{
    let result = transport.acknowledge().await;
    let outcome = delivery_outcome(&result);
    let can_continue = defer_succeeded(result);
    // Discord's already-acknowledged response permits receipt recovery.
    interaction_event(
        audit,
        guild,
        interaction_id,
        Stage::Acknowledge,
        if can_continue {
            Outcome::Succeeded
        } else {
            outcome
        },
    );
    if let Some(response) = content_after_defer(can_continue, make_content).await {
        interaction_event(
            audit,
            guild,
            interaction_id,
            Stage::Deliver,
            delivery_outcome(&transport.edit(response).await),
        );
    }
}

/// Execute a deferred Discord write using the store's receipt-based recovery.
pub async fn execute_interaction(
    transport: &dyn InteractionTransport,
    store: &Arc<Store>,
    guild: GuildId,
    actor: Actor,
    request: &Command,
    interaction_id: u64,
) {
    deferred_response(
        transport,
        store.audit().as_ref(),
        Some(guild),
        interaction_id,
        || execute_request(store, guild, actor, request, interaction_id),
    )
    .await;
}

async fn execute_request(
    store: &Store,
    guild: GuildId,
    actor: Actor,
    request: &Command,
    interaction_id: u64,
) -> EditInteractionResponse {
    let key = format!("discord:{interaction_id}");
    match store.execute(guild, &key, actor, request).await {
        Ok(message) => reply(&message),
        Err(error) => reply(&safe_error(&error)),
    }
}

async fn read_query<F>(
    audit: &dyn AuditListener,
    guild: GuildId,
    interaction_id: u64,
    query: QueryKind,
    read: F,
    timeout: Option<Duration>,
) -> Result<Arc<View>, String>
where
    F: std::future::Future<Output = Result<Arc<View>, StoreError>>,
{
    let started = Instant::now();
    let result = match timeout {
        Some(limit) => tokio::time::timeout(limit, read).await,
        None => Ok(read.await),
    };
    let outcome = match &result {
        Ok(Ok(_)) => Outcome::Succeeded,
        Ok(Err(error)) => store_outcome(Stage::Query, error),
        Err(_) => category_failure(FailureCategory::Timeout),
    };
    audit.on_event(&AuditEvent::QueryCompleted {
        guild,
        interaction_id,
        query,
        outcome,
        stage: Stage::Query,
        elapsed: started.elapsed(),
    });
    match result {
        Ok(result) => result.map_err(|error| safe_error(&error)),
        Err(_) => Err("Loading took too long. Please select the option again.".to_owned()),
    }
}

fn rejected(audit: &dyn AuditListener, guild: Option<GuildId>, interaction_id: u64) {
    interaction_event(
        audit,
        guild,
        interaction_id,
        Stage::Validate,
        Outcome::Rejected(Rejection::InvalidInput),
    );
}

struct Handler {
    store: Arc<Store>,
    bot_user_id: UserId,
}
impl Handler {
    async fn handle_message(&self, http: &Http, message: &Message) {
        let (Some(guild), Some(response)) =
            (message.guild_id, mention_reply(message, self.bot_user_id))
        else {
            return;
        };
        let result = message
            .channel_id
            .send_message(http, response)
            .await
            .map(|_| ());
        self.store
            .audit()
            .on_event(&AuditEvent::MentionReplyCompleted {
                guild: GuildId(guild.get()),
                channel_id: ChannelId(message.channel_id.get()),
                message_id: message.id.get(),
                outcome: delivery_outcome(&result),
                stage: Stage::Deliver,
            });
    }

    async fn register(&self, http: &serenity::http::Http, guild: SerenityGuildId) {
        let result = guild
            .set_commands(http, vec![market_command()])
            .await
            .map(|_| ());
        self.store
            .audit()
            .on_event(&AuditEvent::RegistrationCompleted {
                guild: GuildId(guild.get()),
                outcome: delivery_outcome(&result),
                stage: Stage::Register,
            });
    }
    async fn handle(&self, http: &serenity::http::Http, command: CommandInteraction) {
        let transport = SerenityTransport::Command(&command, http);
        deferred_response(
            &transport,
            self.store.audit().as_ref(),
            command.guild_id.map(|id| GuildId(id.get())),
            command.id.get(),
            || async {
                match from_discord(&command).and_then(|input| parse(&input)) {
                    Ok((_, _, Action::Help)) => reply(HELP),
                    Ok((guild, actor, Action::Write(request))) => {
                        execute_request(&self.store, guild, actor, &request, command.id.get()).await
                    }
                    Ok((guild, actor, Action::AnnouncementsSet { channel_id })) => {
                        let key = format!("discord:{}", command.id.get());
                        match self
                            .store
                            .announcement_configuration_receipt(guild, &key, actor)
                            .await
                        {
                            Ok(Some(message)) => {
                                return reply(&announcements::configuration_receipt(
                                    &message, false,
                                ));
                            }
                            Ok(None) => {}
                            Err(error) => return reply(&safe_error(&error)),
                        }
                        match announcements::validate_destination(
                            http,
                            guild,
                            self.bot_user_id,
                            channel_id,
                        )
                        .await
                        {
                            Ok(()) => {
                                match self
                                    .store
                                    .configure_announcements(
                                        guild,
                                        &key,
                                        actor,
                                        ConfigurationChange::Set { channel_id },
                                    )
                                    .await
                                {
                                    Ok(message) => reply(&announcements::configuration_receipt(
                                        &message, false,
                                    )),
                                    Err(error) => reply(&safe_error(&error)),
                                }
                            }
                            Err(message) => reply(message),
                        }
                    }
                    Ok((guild, actor, Action::AnnouncementsDisable)) => {
                        let key = format!("discord:{}", command.id.get());
                        match self
                            .store
                            .configure_announcements(
                                guild,
                                &key,
                                actor,
                                ConfigurationChange::Disable,
                            )
                            .await
                        {
                            Ok(message) => {
                                reply(&announcements::configuration_receipt(&message, true))
                            }
                            Err(error) => reply(&safe_error(&error)),
                        }
                    }
                    Ok((guild, actor, Action::AnnouncementsStatus)) => {
                        match self.store.announcement_status(guild, actor).await {
                            Ok(status) => reply(&announcements::render_status(&status)),
                            Err(error) => reply(&safe_error(&error)),
                        }
                    }
                    Ok((guild, actor, query)) => {
                        let kind = match query {
                            Action::Balance => QueryKind::Balance,
                            Action::Leaderboard => QueryKind::Leaderboard,
                            Action::List => QueryKind::List,
                            Action::Show { .. } => QueryKind::Show,
                            _ => QueryKind::Component,
                        };
                        match read_query(
                            self.store.audit().as_ref(),
                            guild,
                            command.id.get(),
                            kind,
                            self.store.view(guild),
                            None,
                        )
                        .await
                        {
                            Ok(view) => ui::query(
                                &view,
                                &query,
                                actor,
                                guild,
                                chrono::Utc::now().timestamp(),
                            )
                            .edit(),
                            Err(message) => reply(&message),
                        }
                    }
                    Err(message) => {
                        rejected(
                            self.store.audit().as_ref(),
                            command.guild_id.map(|id| GuildId(id.get())),
                            command.id.get(),
                        );
                        reply(message)
                    }
                }
            },
        )
        .await;
    }
    async fn handle_component(&self, http: &serenity::http::Http, component: ComponentInteraction) {
        let actor = Actor {
            user_id: UserId(component.user.id.get()),
            bot: component.user.bot,
            moderator: false,
        };
        let guild = component
            .guild_id
            .map_or(GuildId(0), |id| GuildId(id.get()));
        // A modal must be the initial response: do not defer this interaction.
        // Bound the read so a slow database can still receive an error acknowledgement.
        let response =
            if let ComponentInteractionDataKind::StringSelect { values } = &component.data.kind {
                match read_query(
                    self.store.audit().as_ref(),
                    guild,
                    component.id.get(),
                    QueryKind::Component,
                    self.store.view(guild),
                    Some(Duration::from_secs(2)),
                )
                .await
                {
                    Ok(view) => ui::component(
                        guild,
                        actor,
                        &component.data.custom_id,
                        values,
                        &view,
                        chrono::Utc::now().timestamp(),
                    )
                    .unwrap_or_else(|message| {
                        rejected(
                            self.store.audit().as_ref(),
                            component.guild_id.map(|id| GuildId(id.get())),
                            component.id.get(),
                        );
                        interaction_error(message)
                    }),
                    Err(message) => interaction_error(&message),
                }
            } else {
                rejected(
                    self.store.audit().as_ref(),
                    component.guild_id.map(|id| GuildId(id.get())),
                    component.id.get(),
                );
                interaction_error("Choose an option from the market menu.")
            };
        let result = SerenityTransport::Component(&component, http)
            .respond(response)
            .await;
        interaction_event(
            self.store.audit().as_ref(),
            component.guild_id.map(|id| GuildId(id.get())),
            component.id.get(),
            Stage::Deliver,
            delivery_outcome(&result),
        );
    }
    async fn handle_modal(&self, http: &serenity::http::Http, modal: ModalInteraction) {
        let transport = SerenityTransport::Modal(&modal, http);
        deferred_response(
            &transport,
            self.store.audit().as_ref(),
            modal.guild_id.map(|id| GuildId(id.get())),
            modal.id.get(),
            || async {
                let actor = Actor {
                    user_id: UserId(modal.user.id.get()),
                    bot: modal.user.bot,
                    moderator: false,
                };
                let guild = modal.guild_id.map_or(GuildId(0), |id| GuildId(id.get()));
                let fields = modal
                    .data
                    .components
                    .iter()
                    .flat_map(|row| &row.components)
                    .map(|component| match component {
                        ActionRowComponent::InputText(field) => Ok(InputOption {
                            name: field.custom_id.clone(),
                            value: InputValue::String(
                                field.value.clone().ok_or("Missing form value.")?,
                            ),
                        }),
                        _ => Err("Invalid form field."),
                    })
                    .collect::<Result<Vec<_>, _>>();
                // Anchor durations to the submission time, including on Discord redelivery.
                let request = fields.and_then(|fields| {
                    ui::modal_command(
                        guild,
                        actor,
                        &modal.data.custom_id,
                        fields,
                        modal.id.created_at().unix_timestamp(),
                    )
                });
                match request {
                    Ok(request) => {
                        execute_request(&self.store, guild, actor, &request, modal.id.get()).await
                    }
                    Err(message) => {
                        rejected(
                            self.store.audit().as_ref(),
                            modal.guild_id.map(|id| GuildId(id.get())),
                            modal.id.get(),
                        );
                        reply(message)
                    }
                }
            },
        )
        .await;
    }
}
#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, message: Message) {
        self.handle_message(&ctx.http, &message).await;
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        lifecycle(
            self.store.audit().as_ref(),
            LifecycleKind::Ready,
            Some(ApplicationId(ready.application.id.get())),
            Stage::Ready,
            Outcome::Succeeded,
        );
        for guild in ready.guilds {
            self.register(&ctx.http, guild.id).await;
        }
    }
    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        self.register(&ctx.http, guild.id).await;
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        handle_interaction(
            Arc::clone(&self.store),
            &ctx.http,
            self.bot_user_id,
            interaction,
        )
        .await;
    }
}

/// Dispatch an incoming Discord interaction through the gateway's command and form handlers.
pub async fn handle_interaction(
    store: Arc<Store>,
    http: &Http,
    bot_user_id: UserId,
    interaction: Interaction,
) {
    let handler = Handler { store, bot_user_id };
    match interaction {
        Interaction::Command(command) => handler.handle(http, command).await,
        Interaction::Component(component) if component.data.custom_id.starts_with("pm:") => {
            handler.handle_component(http, component).await;
        }
        Interaction::Modal(modal) if modal.data.custom_id.starts_with("pm:") => {
            handler.handle_modal(http, modal).await;
        }
        _ => {}
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
            _ = timer.tick() => { if let Err(error) = store.grant_due().await { store.audit().on_event(&AuditEvent::GrantFailed { guild: None, outcome: store_outcome(Stage::Discover, &error), stage: Stage::Discover }); } }
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
    run_with_http(store, Http::new(&token)).await
}

/// Run with a configured Serenity HTTP client through the same guarded startup path.
/// The caller must authenticate the application identity before opening the store,
/// as the normal binary startup does.
///
/// # Errors
/// Returns the same startup, gateway, and shutdown errors as [`run`].
pub async fn run_with_http(store: Arc<Store>, http: Http) -> Result<(), DiscordError> {
    let application_id = Some(store.application_id());
    let mut guard = match store.gateway_guard().await {
        Ok(guard) => guard,
        Err(error) => {
            lifecycle(
                store.audit().as_ref(),
                LifecycleKind::Startup,
                application_id,
                Stage::Acquire,
                store_outcome(Stage::Acquire, &error),
            );
            return Err(error.into());
        }
    };
    guard.close_on_drop();
    let result = run_gateway(Arc::clone(&store), http).await;
    if let Err(error) = guard.close().await {
        lifecycle(
            store.audit().as_ref(),
            LifecycleKind::Shutdown,
            application_id,
            Stage::GatewayLockRelease,
            store_outcome(Stage::GatewayLockRelease, &StoreError::Database(error)),
        );
    }
    result
}
fn lifecycle(
    audit: &dyn AuditListener,
    kind: LifecycleKind,
    application_id: Option<ApplicationId>,
    stage: Stage,
    outcome: Outcome,
) {
    audit.on_event(&AuditEvent::Lifecycle {
        kind,
        application_id,
        stage,
        outcome,
    });
}
async fn run_gateway(store: Arc<Store>, http: Http) -> Result<(), DiscordError> {
    let application_id = Some(store.application_id());
    let audit = Arc::clone(store.audit());
    let client = async {
        let bot_user_id = UserId(http.get_current_user().await?.id.get());
        ClientBuilder::new_with_http(
            http,
            GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES,
        )
        .event_handler(Handler {
            store: Arc::clone(&store),
            bot_user_id,
        })
        .await
    }
    .await;
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            lifecycle(
                audit.as_ref(),
                LifecycleKind::Startup,
                application_id,
                Stage::Startup,
                Outcome::Failed(discord_failure(&error)),
            );
            return Err(DiscordError::Gateway);
        }
    };
    run_gateway_client(store, client).await
}
async fn run_gateway_client(store: Arc<Store>, client: Client) -> Result<(), DiscordError> {
    run_gateway_client_with_clock(store, client, Arc::new(|| chrono::Utc::now().timestamp())).await
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep worker ownership and the shared shutdown deadline visible in one lifecycle"
)]
async fn run_gateway_client_with_clock(
    store: Arc<Store>,
    mut client: Client,
    clock: crate::announcements::Clock,
) -> Result<(), DiscordError> {
    use crate::announcements::worker::{SHUTDOWN_GRACE, start_announcement_worker_with_deadline};
    let application_id = Some(store.application_id());
    let audit = Arc::clone(store.audit());
    let shards = Arc::clone(&client.shard_manager);
    let (sender, receiver) = watch::channel(false);
    let deadline = Arc::new(std::sync::OnceLock::new());
    let mut grants = tokio::spawn(grant_worker(Arc::clone(&store), receiver.clone()));
    let mut announcements = start_announcement_worker_with_deadline(
        store,
        Arc::clone(&client.http),
        clock,
        receiver,
        Arc::clone(&deadline),
    );
    let mut gateway = Box::pin(client.start_autosharded());
    let mut announcements_finished = false;
    let mut await_gateway = false;
    let (result, outcome) = tokio::select! {
        result = &mut gateway => {
            let outcome = match &result {
                Ok(()) => Outcome::Succeeded,
                Err(error) => Outcome::Failed(discord_failure(error)),
            };
            (result.map_err(|_| DiscordError::Gateway), outcome)
        },
        _ = &mut announcements => {
            announcements_finished = true;
            let outcome = category_failure(FailureCategory::Unknown);
            lifecycle(audit.as_ref(), LifecycleKind::Shutdown, application_id, Stage::AnnouncementWorker, outcome.clone());
            (Err(DiscordError::Gateway), outcome)
        },
        result = shutdown_signal() => {
            match result {
                Ok(()) => {
                    await_gateway = true;
                    lifecycle(audit.as_ref(), LifecycleKind::Shutdown, application_id, Stage::ShutdownRequested, Outcome::Succeeded);
                    (Ok(()), Outcome::Succeeded)
                }
                Err(error) => (Err(DiscordError::Signal(error)), category_failure(FailureCategory::Transport)),
            }
        }
    };
    // Publish one deadline before signaling either worker. Gateway, grant, and
    // announcement drains run concurrently; none receives a second grace period.
    let deadline = *deadline.get_or_init(|| tokio::time::Instant::now() + SHUTDOWN_GRACE);
    let _ = sender.send(true);
    let close_gateway = async {
        if tokio::time::timeout_at(deadline, async {
            shards.shutdown_all().await;
            if await_gateway {
                let _ = (&mut gateway).await;
            }
        })
        .await
        .is_err()
        {
            lifecycle(
                audit.as_ref(),
                LifecycleKind::Shutdown,
                application_id,
                Stage::GatewayShutdown,
                category_failure(FailureCategory::Timeout),
            );
        }
    };
    let close_grants = async {
        if tokio::time::timeout_at(deadline, &mut grants)
            .await
            .is_err()
        {
            lifecycle(
                audit.as_ref(),
                LifecycleKind::Shutdown,
                application_id,
                Stage::GrantWorkerShutdown,
                category_failure(FailureCategory::Timeout),
            );
            grants.abort();
            let _ = grants.await;
        }
    };
    let close_announcements = async {
        // The worker uses the shared deadline, then aborts and awaits its own
        // children. Await it here so their database writes cannot outlive the lock.
        if !announcements_finished && announcements.await.is_err() {
            lifecycle(
                audit.as_ref(),
                LifecycleKind::Shutdown,
                application_id,
                Stage::AnnouncementWorkerShutdown,
                category_failure(FailureCategory::Unknown),
            );
        }
    };
    tokio::join!(close_gateway, close_grants, close_announcements);
    lifecycle(
        audit.as_ref(),
        LifecycleKind::Shutdown,
        application_id,
        Stage::Shutdown,
        outcome,
    );
    result
}
#[cfg(test)]
#[path = "discord_tests.rs"]
mod tests;
