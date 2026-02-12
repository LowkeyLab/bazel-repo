use name::Name;
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;

/// Data Access Object for Name table mapping
#[derive(Debug, sqlx::FromRow)]
struct NameDAO {
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

/// Lists names from the database with pagination support
pub trait Lister {
    fn list_by_server(
        &self,
        server_id: u64,
        limit: i64,
        cursor: Option<Uuid>,
    ) -> impl Future<Output = anyhow::Result<Vec<Name>>> + Send;
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
            SELECT user_id, server_id, name, created_at, updated_at
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

impl Lister for Repo {
    async fn list_by_server(
        &self,
        server_id: u64,
        limit: i64,
        cursor: Option<Uuid>,
    ) -> anyhow::Result<Vec<Name>> {
        let query = if let Some(last_user_id) = cursor {
            sqlx::query_as::<_, NameDAO>(
                r#"
                SELECT user_id, server_id, name, created_at, updated_at
                FROM names
                WHERE server_id = $1 AND user_id > $2
                ORDER BY user_id ASC
                LIMIT $3
                "#,
            )
            .bind(server_id as i64)
            .bind(last_user_id)
            .bind(limit)
        } else {
            sqlx::query_as::<_, NameDAO>(
                r#"
                SELECT user_id, server_id, name, created_at, updated_at
                FROM names
                WHERE server_id = $1
                ORDER BY user_id ASC
                LIMIT $2
                "#,
            )
            .bind(server_id as i64)
            .bind(limit)
        };

        let daos = query.fetch_all(&self.pool).await?;
        Ok(daos.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};
    use user::User;

    #[test]
    fn dummy() {
        // Dummy test to help Gazelle discover this test module
    }

    /// Spins up a fresh PostgreSQL container for a single test
    async fn setup_test_db() -> (
        sqlx::PgPool,
        testcontainers::ContainerAsync<postgres::Postgres>,
    ) {
        let container = postgres::Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();

        let db_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to database");

        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, container)
    }

    async fn insert_user(pool: &sqlx::PgPool, discord_id: u64) -> Uuid {
        let user = User::new(discord_id);
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
        .execute(pool)
        .await
        .unwrap();
        user.id
    }

    async fn insert_name(pool: &sqlx::PgPool, user_id: Uuid, server_id: u64, name: &str) {
        let name_entity = Name::new(user_id, server_id, name.to_string());
        sqlx::query(
            r#"
            INSERT INTO names (id, user_id, server_id, name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(name_entity.user_id)
        .bind(name_entity.server_id as i64)
        .bind(&name_entity.name)
        .bind(name_entity.created_at)
        .bind(name_entity.updated_at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_empty() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let result = repo.list_by_server(12345, 10, None).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_first_page() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server_id = 99999;
        let user1 = insert_user(&pool, 111).await;
        let user2 = insert_user(&pool, 222).await;
        let user3 = insert_user(&pool, 333).await;

        // Insert names in any order
        insert_name(&pool, user2, server_id, "Charlie").await;
        insert_name(&pool, user1, server_id, "Alice").await;
        insert_name(&pool, user3, server_id, "Bob").await;

        let result = repo.list_by_server(server_id, 10, None).await.unwrap();

        assert_eq!(result.len(), 3);
        // Should be sorted by user_id (UUID ordering)
        let mut user_ids = vec![user1, user2, user3];
        user_ids.sort();
        assert_eq!(result[0].user_id, user_ids[0]);
        assert_eq!(result[1].user_id, user_ids[1]);
        assert_eq!(result[2].user_id, user_ids[2]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_with_limit() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server_id = 88888;
        let user1 = insert_user(&pool, 111).await;
        let user2 = insert_user(&pool, 222).await;
        let user3 = insert_user(&pool, 333).await;

        insert_name(&pool, user1, server_id, "Alice").await;
        insert_name(&pool, user2, server_id, "Bob").await;
        insert_name(&pool, user3, server_id, "Charlie").await;

        let result = repo.list_by_server(server_id, 2, None).await.unwrap();

        assert_eq!(result.len(), 2);
        // Should get first 2 by user_id ordering
        let mut user_ids = vec![user1, user2, user3];
        user_ids.sort();
        assert_eq!(result[0].user_id, user_ids[0]);
        assert_eq!(result[1].user_id, user_ids[1]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_with_cursor() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server_id = 77777;
        let user1 = insert_user(&pool, 111).await;
        let user2 = insert_user(&pool, 222).await;
        let user3 = insert_user(&pool, 333).await;

        insert_name(&pool, user1, server_id, "Alice").await;
        insert_name(&pool, user2, server_id, "Bob").await;
        insert_name(&pool, user3, server_id, "Charlie").await;

        // Get all names to determine the actual ordering by user_id
        let all_names = repo.list_by_server(server_id, 10, None).await.unwrap();
        assert_eq!(all_names.len(), 3);

        // Use the first user_id as cursor to get names after it
        let first_user_id = all_names[0].user_id;
        let result = repo
            .list_by_server(server_id, 10, Some(first_user_id))
            .await
            .unwrap();

        // Should get 2 names (the ones after the first in UUID ordering)
        assert_eq!(result.len(), 2);
        // Verify they match the expected ordering
        assert_eq!(result[0].user_id, all_names[1].user_id);
        assert_eq!(result[1].user_id, all_names[2].user_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_cursor_no_results() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server_id = 66666;
        let user1 = insert_user(&pool, 111).await;

        insert_name(&pool, user1, server_id, "Alice").await;

        // Use a UUID larger than user1 - no names after this
        let large_uuid = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
        let result = repo
            .list_by_server(server_id, 10, Some(large_uuid))
            .await
            .unwrap();

        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_different_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server1 = 11111;
        let server2 = 22222;
        let user1 = insert_user(&pool, 111).await;
        let user2 = insert_user(&pool, 222).await;

        insert_name(&pool, user1, server1, "ServerOne").await;
        insert_name(&pool, user2, server2, "ServerTwo").await;

        let result = repo.list_by_server(server1, 10, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ServerOne");

        let result = repo.list_by_server(server2, 10, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ServerTwo");
    }
}
