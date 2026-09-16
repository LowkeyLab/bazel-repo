//! Discord presentation and form parsing. All mutations still use domain commands.
use super::{
    Action, Input, InputOption, create_request, exact, no_mentions, render_market, render_query,
    text, truncate, truncate_to,
};
use crate::{
    domain::{Actor, Command, Status},
    store::View,
};
use chrono::DateTime;
use serenity::{
    all::InputTextStyle,
    builder::{
        CreateActionRow, CreateEmbed, CreateInputText, CreateInteractionResponse,
        CreateInteractionResponseMessage, CreateModal, CreateSelectMenu, CreateSelectMenuKind,
        CreateSelectMenuOption, EditInteractionResponse,
    },
};

pub(super) struct Panel {
    content: String,
    embed: Option<CreateEmbed>,
    components: Vec<CreateActionRow>,
}
impl Panel {
    pub(super) fn message(self) -> CreateInteractionResponseMessage {
        CreateInteractionResponseMessage::new()
            .content(self.content)
            .embeds(self.embed.into_iter().collect())
            .components(self.components)
            .allowed_mentions(no_mentions())
            .ephemeral(true)
    }
    pub(super) fn edit(self) -> EditInteractionResponse {
        EditInteractionResponse::new()
            .content(self.content)
            .embeds(self.embed.into_iter().collect())
            .components(self.components)
            .allowed_mentions(no_mentions())
    }
}
fn menu(id: String, placeholder: &str, options: Vec<CreateSelectMenuOption>) -> CreateActionRow {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(id, CreateSelectMenuKind::String { options })
            .placeholder(placeholder)
            .min_values(1)
            .max_values(1),
    )
}
fn prefix(guild: u64, actor: Actor) -> String {
    format!("pm:{guild}:{}", actor.user_id)
}
fn scope(guild: u64, actor: Actor, custom_id: &str) -> Result<Vec<&str>, &'static str> {
    if guild == 0 || actor.user_id == 0 || actor.bot {
        return Err("Only server members can use these controls.");
    }
    let parts: Vec<_> = custom_id.split(':').collect();
    if custom_id.len() > 100
        || parts.len() < 4
        || parts[0] != "pm"
        || parts[1].parse::<u64>().ok() != Some(guild)
        || parts[2].parse::<u64>().ok() != Some(actor.user_id)
    {
        return Err(
            "This control belongs to another member or server. Run /market list or /market create.",
        );
    }
    Ok(parts[3..].to_vec())
}
fn preset(value: &str) -> Result<Option<Vec<String>>, &'static str> {
    match value {
        "yesno" => Ok(Some(vec!["Yes".into(), "No".into()])),
        "result" => Ok(Some(vec!["Win".into(), "Lose".into(), "Draw".into()])),
        "custom" => Ok(None),
        _ => Err("Choose a valid outcome preset."),
    }
}
fn field(
    id: &str,
    label: &str,
    placeholder: &str,
    max: u16,
    style: InputTextStyle,
) -> CreateActionRow {
    CreateActionRow::InputText(
        CreateInputText::new(style, label, id)
            .placeholder(placeholder)
            .max_length(max)
            .min_length(1)
            .required(true),
    )
}
fn creation_modal(guild: u64, actor: Actor, selected: &str) -> Result<CreateModal, &'static str> {
    let options = preset(selected)?;
    let mut fields = vec![
        field(
            "question",
            "Prediction question",
            "Will it rain tomorrow?",
            200,
            InputTextStyle::Short,
        ),
        field(
            "closes_at",
            "Betting closes in / at",
            "1h, 2d, or 2030-01-02T03:04:05Z",
            40,
            InputTextStyle::Short,
        ),
    ];
    if options.is_none() {
        fields.push(field(
            "options",
            "Outcomes (2–10, one per line)",
            "Red team\nBlue team",
            810,
            InputTextStyle::Paragraph,
        ));
    }
    let title = match selected {
        "yesno" => "Create market: Yes / No",
        "result" => "Create market: Win / Lose / Draw",
        _ => "Create custom market",
    };
    Ok(
        CreateModal::new(format!("{}:new:{selected}", prefix(guild, actor)), title)
            .components(fields),
    )
}

pub(super) fn query(view: &View, action: &Action, actor: Actor, guild: u64, now: i64) -> Panel {
    let mut panel = Panel {
        content: truncate(&render_query(view, action, actor, now)),
        embed: None,
        components: vec![],
    };
    let prefix = prefix(guild, actor);
    match action {
        Action::CreateForm => panel.components.push(menu(
            format!("{prefix}:create"),
            "Choose outcome choices",
            vec![
                CreateSelectMenuOption::new("Yes / No", "yesno"),
                CreateSelectMenuOption::new("Win / Lose / Draw", "result"),
                CreateSelectMenuOption::new("Custom outcomes", "custom"),
            ],
        )),
        Action::List => {
            let mut markets: Vec<_> = view
                .state
                .markets
                .iter()
                .filter(|(_, m)| m.status == Status::Open && now < m.closes_at)
                .collect();
            markets.sort_by(|(ida, a), (idb, b)| {
                b.created_at.cmp(&a.created_at).then_with(|| ida.cmp(idb))
            });
            let options: Vec<_> = markets
                .into_iter()
                .take(10)
                .map(|(id, m)| {
                    CreateSelectMenuOption::new(truncate_to(&m.question, 100), id).description(
                        format!(
                            "{} points pooled · {} outcomes",
                            m.total_staked,
                            m.options.len()
                        ),
                    )
                })
                .collect();
            if !options.is_empty() {
                panel.content =
                    "Choose an open market to view its outcomes and place a bet.".into();
                panel.components.push(menu(
                    format!("{prefix}:list"),
                    "Browse open markets",
                    options,
                ));
            }
        }
        Action::Show { id } => {
            if let Some(market) = view.state.markets.get(id) {
                let open = market.status == Status::Open && now < market.closes_at;
                let details = render_market(id, market, now);
                let description = details
                    .split_once('\n')
                    .map_or(details.as_str(), |(_, rest)| rest);
                panel.content.clear();
                panel.embed = Some(
                    CreateEmbed::new()
                        .title(truncate_to(&market.question, 256))
                        .description(description)
                        .color(if open { 0x0058_65f2 } else { 0x0074_7f8d }),
                );
                if open {
                    panel.components.push(menu(
                        format!("{prefix}:bet:{id}"),
                        "Choose an outcome to place a bet",
                        market
                            .options
                            .iter()
                            .enumerate()
                            .map(|(i, label)| {
                                CreateSelectMenuOption::new(truncate_to(label, 100), i.to_string())
                            })
                            .collect(),
                    ));
                }
            }
        }
        _ => {}
    }
    panel
}

pub(super) fn component(
    guild: u64,
    actor: Actor,
    custom_id: &str,
    values: &[String],
    view: &View,
    now: i64,
) -> Result<CreateInteractionResponse, &'static str> {
    let parts = scope(guild, actor, custom_id)?;
    let [value] = values else {
        return Err("Choose exactly one option.");
    };
    match parts.as_slice() {
        ["create"] => Ok(CreateInteractionResponse::Modal(creation_modal(
            guild, actor, value,
        )?)),
        ["list"] => {
            if !view.state.markets.contains_key(value) {
                return Err("No market with that ID exists in this server.");
            }
            Ok(CreateInteractionResponse::Message(
                query(view, &Action::Show { id: value.clone() }, actor, guild, now).message(),
            ))
        }
        ["bet", id] => {
            let market = view
                .state
                .markets
                .get(*id)
                .ok_or("No market with that ID exists in this server.")?;
            if market.status != Status::Open || now >= market.closes_at {
                return Err("This market is closed for betting.");
            }
            let outcome = value
                .parse::<usize>()
                .ok()
                .filter(|i| *i < market.options.len())
                .ok_or("Choose a valid outcome.")?;
            let account = view
                .state
                .accounts
                .get(&actor.user_id)
                .ok_or("You are not enrolled. Use /market join first.")?;
            let label = truncate_to(&format!("Points on {}", market.options[outcome]), 45);
            Ok(CreateInteractionResponse::Modal(
                CreateModal::new(
                    format!("{}:stake:{id}:{outcome}", prefix(guild, actor)),
                    "Place your bet",
                )
                .components(vec![field(
                    "amount",
                    &label,
                    &format!("Available: {} points", account.balance),
                    19,
                    InputTextStyle::Short,
                )]),
            ))
        }
        _ => Err("This control is no longer supported. Run /market list or /market create."),
    }
}
fn close_time(value: &str, now: i64) -> Result<i64, &'static str> {
    let value = value.trim();
    let timestamp = if let Ok(time) = DateTime::parse_from_rfc3339(value) {
        time.timestamp()
    } else {
        let (digits, multiplier) = if let Some(digits) = value.strip_suffix('m') {
            (digits, 60)
        } else if let Some(digits) = value.strip_suffix('h') {
            (digits, 3_600)
        } else if let Some(digits) = value.strip_suffix('d') {
            (digits, 86_400)
        } else {
            return Err("Enter a duration such as 30m, 1h, 2d, or an RFC 3339 time.");
        };
        digits
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .and_then(|n| n.checked_mul(multiplier))
            .and_then(|seconds| now.checked_add(seconds))
            .ok_or("Enter a positive duration within the supported range.")?
    };
    if timestamp <= now || DateTime::from_timestamp(timestamp, 0).is_none() {
        return Err("Closing time must be in the future and within the supported range.");
    }
    Ok(timestamp)
}
pub(super) fn modal_command(
    guild: u64,
    actor: Actor,
    custom_id: &str,
    fields: Vec<InputOption>,
    now: i64,
) -> Result<Command, &'static str> {
    let parts = scope(guild, actor, custom_id)?;
    let input = Input {
        guild_id: Some(guild),
        user_id: actor.user_id,
        bot: actor.bot,
        moderator: actor.moderator,
        subcommand: String::new(),
        options: fields,
    };
    match parts.as_slice() {
        ["new", selected] => {
            let predefined = preset(selected)?;
            exact(
                &input,
                if predefined.is_some() {
                    &["question", "closes_at"]
                } else {
                    &["question", "closes_at", "options"]
                },
            )?;
            let options = match predefined {
                Some(options) => options,
                None => text(&input, "options")?
                    .split('\n')
                    .map(|line| line.trim().to_owned())
                    .collect(),
            };
            create_request(
                text(&input, "question")?,
                options,
                close_time(text(&input, "closes_at")?, now)?,
            )
        }
        ["stake", id, outcome] => {
            exact(&input, &["amount"])?;
            let amount = text(&input, "amount")?
                .trim()
                .parse::<i64>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("Stake must be a positive whole number.")?;
            let outcome = outcome
                .parse::<usize>()
                .ok()
                .filter(|i| *i < 10)
                .ok_or("Choose a valid outcome.")?;
            if uuid::Uuid::parse_str(id).is_err() {
                return Err("Enter a valid market ID.");
            }
            Ok(Command::Bet {
                id: (*id).to_owned(),
                outcome,
                amount,
            })
        }
        _ => Err(
            "This form is no longer supported. Start again with /market create or /market list.",
        ),
    }
}
