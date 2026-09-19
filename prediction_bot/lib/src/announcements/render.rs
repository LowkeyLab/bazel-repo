use serenity::builder::{CreateAllowedMentions, CreateMessage};

use super::SnapshotV1;

const CONTENT_LIMIT: usize = 2_000;
const TRUNCATION_MARKER: &str = "…";

pub(crate) fn render(snapshot: &SnapshotV1) -> CreateMessage {
    let mut content = render_content(snapshot, usize::MAX);
    if utf16_len(&content) > CONTENT_LIMIT {
        let fixed_units = utf16_len(&render_content(snapshot, 0));
        let field_count = match snapshot {
            SnapshotV1::Created { options, .. } => options.len() + 1,
            SnapshotV1::Resolved { .. } => 2,
            SnapshotV1::Cancelled { .. }
            | SnapshotV1::BetPlaced { .. }
            | SnapshotV1::MemberEnrolled { .. } => 1,
        };
        let field_count = field_count + snapshot.odds().len();
        let field_limit = CONTENT_LIMIT.saturating_sub(fixed_units) / field_count;
        content = render_content(snapshot, field_limit);
    }
    CreateMessage::new()
        .content(content)
        .allowed_mentions(no_mentions())
}

// Rendering empty user fields measures the exact space reserved for labels,
// every outcome bullet, identifiers, and timestamps before sharing the remainder.
fn render_content(snapshot: &SnapshotV1, field_limit: usize) -> String {
    let odds = render_odds(snapshot.odds(), field_limit);
    let (heading, id, details) = match snapshot {
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
        } => {
            let outcomes = options
                .iter()
                .map(|option| {
                    format!(
                        "• {} — N/A (no bets) implied chance",
                        escape_field(option, field_limit)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
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

    let prefix = match id {
        Some(id) => format!("{heading}\nMarket ID: `{}`", escape_markdown(&id.0)),
        None => heading.to_owned(),
    };
    format!("{prefix}\n{details}")
}

fn render_odds(odds: &[crate::odds::OutcomeOdds], field_limit: usize) -> String {
    if odds.is_empty() {
        return String::new();
    }
    let outcomes = odds
        .iter()
        .map(|outcome| {
            format!(
                "• {} — {} implied chance",
                escape_field(&outcome.label, field_limit),
                outcome.chance()
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
