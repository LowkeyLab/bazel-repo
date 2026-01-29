use chrono::prelude::*;
use uuid::Uuid;

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub discord_id: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub valid_at: DateTime<Utc>,
}

impl Default for User {
    fn default() -> Self {
        let now = Utc::now();
        User {
            id: Uuid::new_v4(),
            discord_id: 0,
            created_at: now,
            updated_at: now,
            valid_at: now,
        }
    }
}

impl User {
    /// Creates a new User with the given discord_id.
    pub fn new(discord_id: u64) -> Self {
        User {
            id: Uuid::new_v4(),
            discord_id,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_user_creation() {
        let discord_id = 123456789;
        let user = User::new(discord_id);
        assert_eq!(user.discord_id, discord_id);
    }
}
