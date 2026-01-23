use chrono::prelude::*;
use uuid::Uuid;

/// A name associated with a user in a Discord server.
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Name {
    pub user_id: Uuid,
    pub server_id: u64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub valid_at: DateTime<Utc>,
}

impl Name {
    /// Creates a new Name instance.
    pub fn new(user_id: Uuid, server_id: u64, name: String) -> Self {
        let now = Utc::now();
        Name {
            user_id,
            server_id,
            name,
            created_at: now,
            updated_at: now,
            valid_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_creation() {
        let user_id = Uuid::new_v4();
        let server_id = 987654321;
        let name_str = "TestUser".to_string();
        let name = Name::new(user_id, server_id, name_str.clone());

        assert_eq!(name.user_id, user_id);
        assert_eq!(name.server_id, server_id);
        assert_eq!(name.name, name_str);
    }
}
