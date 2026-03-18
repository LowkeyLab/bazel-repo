use chrono::prelude::*;
use name::DiscordServerId;

/// A Discord server registered in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub id: DiscordServerId,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Server {
    /// Creates a new Server instance.
    pub fn new(id: DiscordServerId, display_name: String) -> Self {
        let now = Utc::now();
        Server {
            id,
            display_name,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let id = DiscordServerId(123456789);
        let display_name = "My Server".to_string();
        let server = Server::new(id, display_name.clone());

        assert_eq!(server.id, id);
        assert_eq!(server.display_name, display_name);
    }
}
