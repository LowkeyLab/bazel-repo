use serenity::builder::{CreateAllowedMentions, CreateMessage};

use super::SnapshotV1;

const CONTENT_LIMIT: usize = 2_000;
const TRUNCATION_MARKER: &str = "\n…";

pub(crate) fn render(snapshot: &SnapshotV1) -> CreateMessage {
    let (heading, id, details) = match snapshot {
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
                .map(|option| format!("• {}", escape_markdown(option)))
                .collect::<Vec<_>>()
                .join("\n");
            (
                "📈 Market created",
                id,
                format!(
                    "Question: {}\nCreator: `{creator}`\nOutcomes:\n{outcomes}\nCloses: <t:{closes_at}:F>\nEvent time: <t:{occurred_at}:F>",
                    escape_markdown(question),
                ),
            )
        }
        SnapshotV1::Resolved {
            id,
            question,
            winner,
            refunded,
            occurred_at,
        } => {
            let refund = if *refunded {
                "yes (no winning bets)"
            } else {
                "no"
            };
            (
                "✅ Market resolved",
                id,
                format!(
                    "Question: {}\nWinning outcome: {}\nStakes refunded: {refund}\nEvent time: <t:{occurred_at}:F>",
                    escape_markdown(question),
                    escape_markdown(winner),
                ),
            )
        }
        SnapshotV1::Cancelled {
            id,
            question,
            occurred_at,
        } => (
            "🚫 Market cancelled",
            id,
            format!(
                "Question: {}\nStakes refunded: yes\nEvent time: <t:{occurred_at}:F>",
                escape_markdown(question),
            ),
        ),
    };

    let prefix = format!("{heading}\nMarket ID: `{}`", escape_markdown(id));
    let content = fit_content(&prefix, &details);
    CreateMessage::new()
        .content(content)
        .allowed_mentions(no_mentions())
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

fn fit_content(prefix: &str, details: &str) -> String {
    let full = format!("{prefix}\n{details}");
    if utf16_len(&full) <= CONTENT_LIMIT {
        return full;
    }

    let marker_units = utf16_len(TRUNCATION_MARKER);
    let mut content = take_utf16(&full, CONTENT_LIMIT.saturating_sub(marker_units));
    content.push_str(TRUNCATION_MARKER);
    content
}

fn take_utf16(text: &str, limit: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|character| {
            let next = units + character.len_utf16();
            if next > limit {
                return false;
            }
            units = next;
            true
        })
        .collect()
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}
