use chrono::prelude::*;

/// A Discord user ID (snowflake).
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DiscordId(pub u64);

/// A Discord server ID (snowflake).
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DiscordServerId(pub u64);

/// The natural identity of a name: which Discord user on which Discord server.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct NameId {
    pub discord_id: DiscordId,
    pub discord_server: DiscordServerId,
}

/// A name associated with a user in a Discord server.
#[derive(Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Name {
    pub id: NameId,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Name {
    /// Creates a new Name instance.
    pub fn new(discord_id: DiscordId, discord_server: DiscordServerId, name: String) -> Self {
        let now = Utc::now();
        Name {
            id: NameId {
                discord_id,
                discord_server,
            },
            name,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_id_equality() {
        assert_eq!(DiscordId(123), DiscordId(123));
        assert_ne!(DiscordId(123), DiscordId(456));
    }

    #[test]
    fn test_discord_server_id_equality() {
        assert_eq!(DiscordServerId(123), DiscordServerId(123));
        assert_ne!(DiscordServerId(123), DiscordServerId(456));
    }

    #[test]
    fn test_name_id_creation() {
        let id = NameId {
            discord_id: DiscordId(123),
            discord_server: DiscordServerId(456),
        };
        assert_eq!(id.discord_id, DiscordId(123));
        assert_eq!(id.discord_server, DiscordServerId(456));
    }

    #[test]
    fn test_name_creation() {
        let discord_id = DiscordId(123456789);
        let discord_server = DiscordServerId(987654321);
        let name_str = "TestUser".to_string();
        let name = Name::new(discord_id, discord_server, name_str.clone());

        assert_eq!(name.id.discord_id, discord_id);
        assert_eq!(name.id.discord_server, discord_server);
        assert_eq!(name.name, name_str);
    }
}
