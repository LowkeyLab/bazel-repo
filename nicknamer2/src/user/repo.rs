use sqlx::{PgPool, Row};
use std::future::Future;
use thiserror;
use user::User;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    DbError(String),
}

/// Saves a user to the database.
pub trait Saver {
    fn save(&self, user: User) -> impl Future<Output = Result<(), Error>> + Send;
}

pub trait ByDiscordIdGetter {
    fn get_by_discord_id(
        &self,
        discord_id: u64,
    ) -> impl Future<Output = Result<Option<User>, Error>> + Send;
}

#[derive(Debug)]
pub struct Repo {
    pool: PgPool,
}

impl Repo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Saver for Repo {
    async fn save(&self, user: User) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO users (id, discord_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(user.id)
        .bind(user.discord_id as i64)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DbError(e.to_string()))?;

        Ok(())
    }
}

impl ByDiscordIdGetter for Repo {
    async fn get_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, discord_id, created_at, updated_at
            FROM users
            WHERE discord_id = $1
            "#,
        )
        .bind(discord_id as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::DbError(e.to_string()))?;

        let user = row.map(|r| User {
            id: r.get("id"),
            discord_id: r.get::<i64, _>("discord_id") as u64,
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        });

        Ok(user)
    }
}
