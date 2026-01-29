use sqlx::PgPool;
use thiserror;
use user::User;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    DbError(String),
}

pub trait UserSaver {
    async fn save(&self, user: User) -> Result<(), Error>;
}

#[derive(Debug)]
pub struct UserRepo {
    pool: PgPool,
}

impl UserRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserSaver for UserRepo {
    async fn save(&self, user: User) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO users (id, discord_id, created_at, updated_at, valid_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(user.id)
        .bind(user.discord_id as i64)
        .bind(user.created_at)
        .bind(user.updated_at)
        .bind(user.valid_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DbError(e.to_string()))?;

        Ok(())
    }
}
