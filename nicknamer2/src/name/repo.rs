use name::Name;
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;

/// Data Access Object for Name table mapping
#[derive(Debug, sqlx::FromRow)]
struct NameDAO {
    id: Uuid,
    user_id: Uuid,
    server_id: i64,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<NameDAO> for Name {
    fn from(dao: NameDAO) -> Self {
        Name {
            user_id: dao.user_id,
            server_id: dao.server_id as u64,
            name: dao.name,
            created_at: dao.created_at,
            updated_at: dao.updated_at,
        }
    }
}

/// Saves a name to the database.
pub trait Saver {
    fn save(&self, name: Name) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Updates a name in the database.
pub trait Updater {
    fn update(
        &self,
        user_id: Uuid,
        server_id: u64,
        new_name: String,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Gets a name from the database.
pub trait Getter {
    fn get(
        &self,
        user_id: Uuid,
        server_id: u64,
    ) -> impl Future<Output = anyhow::Result<Option<Name>>> + Send;
}

/// Deletes a name from the database.
pub trait Deleter {
    fn delete(
        &self,
        user_id: Uuid,
        server_id: u64,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
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
    async fn save(&self, name: Name) -> anyhow::Result<()> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO names (id, user_id, server_id, name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(name.user_id)
        .bind(name.server_id as i64)
        .bind(name.name)
        .bind(name.created_at)
        .bind(name.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl Updater for Repo {
    async fn update(&self, user_id: Uuid, server_id: u64, new_name: String) -> anyhow::Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE names
            SET name = $1, updated_at = $2
            WHERE user_id = $3 AND server_id = $4
            "#,
        )
        .bind(new_name)
        .bind(now)
        .bind(user_id)
        .bind(server_id as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl Getter for Repo {
    async fn get(&self, user_id: Uuid, server_id: u64) -> anyhow::Result<Option<Name>> {
        let dao = sqlx::query_as::<_, NameDAO>(
            r#"
            SELECT id, user_id, server_id, name, created_at, updated_at
            FROM names
            WHERE user_id = $1 AND server_id = $2
            "#,
        )
        .bind(user_id)
        .bind(server_id as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }
}

impl Deleter for Repo {
    async fn delete(&self, user_id: Uuid, server_id: u64) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            DELETE FROM names
            WHERE user_id = $1 AND server_id = $2
            "#,
        )
        .bind(user_id)
        .bind(server_id as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
