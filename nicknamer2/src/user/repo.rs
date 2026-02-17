use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;
use user::User;

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

/// Creates a user in the database.
pub trait UserCreator {
    fn save(&self, user: User) -> impl Future<Output = anyhow::Result<Uuid>> + Send;
}

/// Reads users from the database.
pub trait UserReader {
    fn get_by_discord_id(
        &self,
        discord_id: u64,
    ) -> impl Future<Output = anyhow::Result<Option<User>>> + Send;

    fn get_by_id(&self, id: Uuid) -> impl Future<Output = anyhow::Result<Option<User>>> + Send;
}

/// Updates an existing user in the database.
pub trait UserUpdater {
    fn update(&self, user: User) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Deletes a user from the database by their UUID.
pub trait UserDeleter {
    fn delete(&self, id: Uuid) -> impl Future<Output = anyhow::Result<bool>> + Send;
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

impl UserCreator for Repo {
    async fn save(&self, user: User) -> anyhow::Result<Uuid> {
        let id = user.id;
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

        Ok(id)
    }
}

impl UserReader for Repo {
    async fn get_by_discord_id(&self, discord_id: u64) -> anyhow::Result<Option<User>> {
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

    async fn get_by_id(&self, id: Uuid) -> anyhow::Result<Option<User>> {
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

impl UserUpdater for Repo {
    async fn update(&self, user: User) -> anyhow::Result<()> {
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

impl UserDeleter for Repo {
    async fn delete(&self, id: Uuid) -> anyhow::Result<bool> {
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
