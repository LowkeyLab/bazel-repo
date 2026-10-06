use serenity::builder::{CreateAllowedMentions, CreateMessage};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::{SnapshotV1, StakeSummary};

const CONTENT_LIMIT: usize = 2_000;
const TRUNCATION_MARKER: &str = "…";
const CHOICE_LABEL_WIDTH: usize = 32;

pub fn render(snapshot: &SnapshotV1) -> CreateMessage {
    let mut content = render_content(snapshot, usize::MAX);
    if utf16_len(&content) > CONTENT_LIMIT {
        let fixed_units = utf16_len(&render_content(snapshot, 0));
        let field_count = match snapshot {
            SnapshotV1::Created { options, .. } => options.len() + 1,
            SnapshotV1::Resolved { .. } => 2,
            SnapshotV1::Enabled { .. }
            | SnapshotV1::Cancelled { .. }
            | SnapshotV1::BetPlaced { .. }
            | SnapshotV1::MemberEnrolled { .. } => 1,
        };
        let field_count = field_count + snapshot.odds().len();
        let mut field_limit = CONTENT_LIMIT.saturating_sub(fixed_units) / field_count;
        content = render_content(snapshot, field_limit);
        // Display-width padding is repeated across the table and may consume
        // more UTF-16 units than the labels themselves. Check the complete
        // message while reducing only user text, never rows or saved amounts.
        while utf16_len(&content) > CONTENT_LIMIT && field_limit > 0 {
            field_limit -= 1;
            content = render_content(snapshot, field_limit);
        }
    }
    CreateMessage::new()
        .content(content)
        .allowed_mentions(no_mentions())
}

// Rendering empty user fields measures the exact space reserved for labels,
// every outcome row, identifiers, and timestamps before sharing the remainder.
fn render_content(snapshot: &SnapshotV1, field_limit: usize) -> String {
    let stakes = snapshot.stakes();
    let odds = render_odds(snapshot.odds(), stakes, field_limit);
    let (heading, id, details) = match snapshot {
        SnapshotV1::Enabled { .. } => return "Prediction market announcements are enabled! New markets, bets, and results will appear here.".to_owned(),
        SnapshotV1::MemberEnrolled {
            user_id,
            occurred_at,
        } => (
            "👋 New participant",
            None,
            format!(
                "<@{user_id}> joined this server’s prediction market!\nEvent time: <t:{occurred_at}:F>"
            ),
        ),
        SnapshotV1::BetPlaced {
            id,
            question,
            bet_count,
            occurred_at,
            ..
        } => {
            let noun = if *bet_count == 1 { "bet" } else { "bets" };
            (
                "🎲 Another bet",
                Some(id),
                format!(
                    "Question: {}\n{bet_count} {noun} placed{odds}\nEvent time: <t:{occurred_at}:F>",
                    escape_field(question, field_limit)
                ),
            )
        }
        SnapshotV1::Created {
            id,
            question,
            creator,
            options,
            closes_at,
            occurred_at,
            ..
        } => {
            let outcomes = stakes.map_or_else(
                || render_creation_outcomes(options, field_limit),
                |stakes| render_stake_table(
                    options.iter().map(|label| (label.as_str(), "N/A (no bets)".to_owned(), String::new())),
                    stakes,
                    field_limit,
                ),
            );
            (
                "📈 Market created",
                Some(id),
                format!(
                    "Question: {}\nCreator: <@{creator}>\nOutcomes:\n{outcomes}\nCloses: <t:{closes_at}:F>\nEvent time: <t:{occurred_at}:F>",
                    escape_field(question, field_limit),
                ),
            )
        }
        SnapshotV1::Resolved {
            id,
            question,
            winner,
            refunded,
            occurred_at,
            ..
        } => {
            let refund = if *refunded {
                "yes (no winning bets)"
            } else {
                "no"
            };
            (
                "✅ Market resolved",
                Some(id),
                format!(
                    "Question: {}\nWinning outcome: {}\nStakes refunded: {refund}{odds}\nEvent time: <t:{occurred_at}:F>",
                    escape_field(question, field_limit),
                    escape_field(winner, field_limit),
                ),
            )
        }
        SnapshotV1::Cancelled {
            id,
            question,
            occurred_at,
            ..
        } => (
            "🚫 Market cancelled",
            Some(id),
            format!(
                "Question: {}\nStakes refunded: yes{odds}\nEvent time: <t:{occurred_at}:F>",
                escape_field(question, field_limit),
            ),
        ),
    };

    let prefix = id.map_or_else(
        || heading.to_owned(),
        |id| format!("{heading}\nMarket ID: `{}`", escape_markdown(&id.0)),
    );
    format!("{prefix}\n{details}")
}

fn render_odds(
    odds: &[crate::odds::OutcomeOdds],
    stakes: Option<&StakeSummary>,
    field_limit: usize,
) -> String {
    if odds.is_empty() {
        return String::new();
    }
    if let Some(stakes) = stakes {
        let table = render_stake_table(
            odds.iter().map(|outcome| {
                (
                    outcome.label.as_str(),
                    outcome.chance(),
                    outcome.movement_indicator().trim().to_owned(),
                )
            }),
            stakes,
            field_limit,
        );
        return format!("\nOutcomes:\n{table}");
    }
    let outcomes = odds
        .iter()
        .map(|outcome| {
            format!(
                "• {} — {} implied chance{}",
                escape_field(&outcome.label, field_limit),
                outcome.chance(),
                outcome.movement_indicator()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("\nOutcomes:\n{outcomes}")
}

fn no_mentions() -> CreateAllowedMentions {
    CreateAllowedMentions::new()
        .everyone(false)
        .all_users(false)
        .all_roles(false)
        .replied_user(false)
}

fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if matches!(
            character,
            '\\' | '*' | '_' | '~' | '`' | '|' | '<' | '>' | '#' | '[' | ']' | '(' | ')'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn escape_field(text: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    let escaped = escape_markdown(text);
    if utf16_len(&escaped) <= limit {
        return escaped;
    }

    let budget = limit.saturating_sub(utf16_len(TRUNCATION_MARKER));
    let mut content = String::new();
    let mut units = 0;
    // Keep each character and its Markdown escape together; never leave a
    // dangling backslash or split a Unicode scalar at the truncation boundary.
    for character in text.chars() {
        let token = escape_markdown(&character.to_string());
        let next = units + utf16_len(&token);
        if next > budget {
            break;
        }
        content.push_str(&token);
        units = next;
    }
    content.push_str(TRUNCATION_MARKER);
    content
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn format_points(amount: crate::types::Points) -> String {
    let digits = amount.0.to_string();
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    formatted
}

fn render_creation_outcomes(options: &[String], field_limit: usize) -> String {
    options
        .iter()
        .map(|option| {
            format!(
                "• {} — N/A (no bets) implied chance",
                escape_field(option, field_limit),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_stake_table<'a>(
    outcomes: impl Iterator<Item = (&'a str, String, String)>,
    stakes: &StakeSummary,
    field_limit: usize,
) -> String {
    let mut rows = vec![[
        "Choice".to_owned(),
        "Points staked".to_owned(),
        "Implied chance".to_owned(),
        "Movement".to_owned(),
    ]];
    rows.extend(
        outcomes
            .enumerate()
            .map(|(index, (label, chance, movement))| {
                [
                    format!("{}. {}", index + 1, table_label(label, field_limit)),
                    stakes
                        .outcomes
                        .get(index)
                        .map_or_else(String::new, |amount| format_points(*amount)),
                    chance,
                    movement,
                ]
            }),
    );
    rows.push([
        "Total".to_owned(),
        format_points(stakes.total),
        "-".to_owned(),
        "-".to_owned(),
    ]);
    let widths: [usize; 4] = std::array::from_fn(|column| {
        rows.iter()
            .map(|row| row[column].width())
            .max()
            .unwrap_or(0)
    });
    let lines = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            row.iter()
                .enumerate()
                .map(|(column, value)| {
                    let padding = " ".repeat(widths[column] - value.width());
                    if column == 1 && index != 0 {
                        format!("{padding}{value}")
                    } else if column == 3 {
                        value.clone()
                    } else {
                        format!("{value}{padding}")
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ")
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("```\n{lines}\n```")
}

fn table_label(label: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    let safe: String = label
        .chars()
        .filter_map(|character| {
            if character.is_whitespace() {
                Some(' ')
            } else if character.is_control()
                || matches!(character,
                    '\u{061c}' | '\u{200b}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' |
                    '\u{2066}'..='\u{2069}' | '\u{feff}' | '\u{00ad}'
                )
            {
                None
            } else {
                Some(match character {
                    '`' => 'ˋ',
                    '|' => '¦',
                    other => other,
                })
            }
        })
        .collect();
    if utf16_len(&safe) <= limit && safe.width() <= CHOICE_LABEL_WIDTH {
        return safe;
    }
    let mut shortened = String::new();
    let mut units = 0;
    let mut width = 0;
    for grapheme in safe.graphemes(true) {
        let next_units = units + utf16_len(grapheme);
        let next_width = width + grapheme.width();
        if next_units > limit.saturating_sub(1) || next_width >= CHOICE_LABEL_WIDTH {
            break;
        }
        shortened.push_str(grapheme);
        units = next_units;
        width = next_width;
    }
    shortened.push_str(TRUNCATION_MARKER);
    shortened
}
