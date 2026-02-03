use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;
use thiserror;
use user::User;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    DbError(String),
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::DbError(e.to_string())
    }
}

/// Data Access Object for User table mapping
#[derive(Debug, sqlx::FromRow)]
struct UserDAO {
    id: Uuid,
    discord_id: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<UserDAO> for User {
    fn from(dao: UserDAO) -> Self {
        User {
            id: dao.id,
            discord_id: dao.discord_id as u64,
            created_at: dao.created_at,
            updated_at: dao.updated_at,
        }
    }
}

impl From<User> for UserDAO {
    fn from(user: User) -> Self {
        UserDAO {
            id: user.id,
            discord_id: user.discord_id as i64,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
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

/// Gets a user by their UUID.
pub trait ByIdGetter {
    fn get_by_id(&self, id: Uuid) -> impl Future<Output = Result<Option<User>, Error>> + Send;
}

/// Updates an existing user in the database.
pub trait Updater {
    fn update(&self, user: User) -> impl Future<Output = Result<(), Error>> + Send;
}

/// Deletes a user from the database by their UUID.
pub trait Deleter {
    fn delete(&self, id: Uuid) -> impl Future<Output = Result<bool, Error>> + Send;
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
        let dao: UserDAO = user.into();
        sqlx::query(
            r#"
            INSERT INTO users (id, discord_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(dao.id)
        .bind(dao.discord_id)
        .bind(dao.created_at)
        .bind(dao.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl ByDiscordIdGetter for Repo {
    async fn get_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, Error> {
        let dao = sqlx::query_as::<_, UserDAO>(
            r#"
            SELECT id, discord_id, created_at, updated_at
            FROM users
            WHERE discord_id = $1
            "#,
        )
        .bind(discord_id as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }
}

impl ByIdGetter for Repo {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, Error> {
        let dao = sqlx::query_as::<_, UserDAO>(
            r#"
            SELECT id, discord_id, created_at, updated_at
            FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }
}

impl Updater for Repo {
    async fn update(&self, user: User) -> Result<(), Error> {
        let dao: UserDAO = user.into();
        sqlx::query(
            r#"
            UPDATE users
            SET discord_id = $2, updated_at = $3
            WHERE id = $1
            "#,
        )
        .bind(dao.id)
        .bind(dao.discord_id)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl Deleter for Repo {
    async fn delete(&self, id: Uuid) -> Result<bool, Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM users
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}
