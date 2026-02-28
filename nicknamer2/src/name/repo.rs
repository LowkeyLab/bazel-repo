use name::{DiscordId, DiscordServerId, Name, NameId};
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;

/// Data Access Object for Name table mapping
#[derive(Debug, sqlx::FromRow)]
struct NameDAO {
    #[allow(dead_code)]
    id: Uuid,
    discord_id: i64,
    discord_server: i64,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<NameDAO> for Name {
    fn from(dao: NameDAO) -> Self {
        Name {
            id: NameId {
                discord_id: DiscordId(dao.discord_id as u64),
                discord_server: DiscordServerId(dao.discord_server as u64),
            },
            name: dao.name,
            created_at: dao.created_at,
            updated_at: dao.updated_at,
        }
    }
}

/// Creates a name in the database.
pub trait NameCreator {
    fn save(&self, name: Name) -> impl Future<Output = anyhow::Result<Uuid>> + Send;
}

/// Reads names from the database.
pub trait NameReader {
    fn get(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> impl Future<Output = anyhow::Result<Option<Name>>> + Send;

    fn list_by_server(
        &self,
        discord_server: DiscordServerId,
        limit: i64,
        cursor: Option<DiscordId>,
    ) -> impl Future<Output = anyhow::Result<Vec<Name>>> + Send;

    fn list_servers(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> impl Future<Output = anyhow::Result<Vec<DiscordServerId>>> + Send;
}

/// Updates a name in the database.
pub trait NameUpdater {
    fn update(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
        new_name: String,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Deletes a name from the database.
pub trait NameDeleter {
    fn delete(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// Counts names and servers in the database.
pub trait NameCounter {
    fn count_names_by_server(
        &self,
        discord_server: DiscordServerId,
    ) -> impl Future<Output = anyhow::Result<i64>> + Send;

    fn count_servers(&self) -> impl Future<Output = anyhow::Result<i64>> + Send;
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

impl NameCreator for Repo {
    async fn save(&self, name: Name) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO names (id, discord_id, discord_server, name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(name.id.discord_id.0 as i64)
        .bind(name.id.discord_server.0 as i64)
        .bind(name.name)
        .bind(name.created_at)
        .bind(name.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }
}

impl NameUpdater for Repo {
    async fn update(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
        new_name: String,
    ) -> anyhow::Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE names
            SET name = $1, updated_at = $2
            WHERE discord_id = $3 AND discord_server = $4
            "#,
        )
        .bind(new_name)
        .bind(now)
        .bind(discord_id.0 as i64)
        .bind(discord_server.0 as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl NameReader for Repo {
    async fn get(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<Option<Name>> {
        let dao = sqlx::query_as::<_, NameDAO>(
            r#"
            SELECT id, discord_id, discord_server, name, created_at, updated_at
            FROM names
            WHERE discord_id = $1 AND discord_server = $2
            "#,
        )
        .bind(discord_id.0 as i64)
        .bind(discord_server.0 as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }

    async fn list_by_server(
        &self,
        discord_server: DiscordServerId,
        limit: i64,
        cursor: Option<DiscordId>,
    ) -> anyhow::Result<Vec<Name>> {
        let query = if let Some(last_discord_id) = cursor {
            sqlx::query_as::<_, NameDAO>(
                r#"
                SELECT id, discord_id, discord_server, name, created_at, updated_at
                FROM names
                WHERE discord_server = $1 AND discord_id > $2
                ORDER BY discord_id ASC
                LIMIT $3
                "#,
            )
            .bind(discord_server.0 as i64)
            .bind(last_discord_id.0 as i64)
            .bind(limit)
        } else {
            sqlx::query_as::<_, NameDAO>(
                r#"
                SELECT id, discord_id, discord_server, name, created_at, updated_at
                FROM names
                WHERE discord_server = $1
                ORDER BY discord_id ASC
                LIMIT $2
                "#,
            )
            .bind(discord_server.0 as i64)
            .bind(limit)
        };

        let daos = query.fetch_all(&self.pool).await?;
        Ok(daos.into_iter().map(Into::into).collect())
    }

    async fn list_servers(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<DiscordServerId>> {
        let rows: Vec<(i64,)> = if let Some(last_server_id) = cursor {
            sqlx::query_as(
                r#"
                SELECT DISTINCT discord_server
                FROM names
                WHERE discord_server > $1
                ORDER BY discord_server ASC
                LIMIT $2
                "#,
            )
            .bind(last_server_id.0 as i64)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT DISTINCT discord_server
                FROM names
                ORDER BY discord_server ASC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(|(id,)| DiscordServerId(id as u64))
            .collect())
    }
}

impl NameDeleter for Repo {
    async fn delete(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            DELETE FROM names
            WHERE discord_id = $1 AND discord_server = $2
            "#,
        )
        .bind(discord_id.0 as i64)
        .bind(discord_server.0 as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

impl NameCounter for Repo {
    async fn count_names_by_server(&self, discord_server: DiscordServerId) -> anyhow::Result<i64> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM names WHERE discord_server = $1")
                .bind(discord_server.0 as i64)
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }

    async fn count_servers(&self) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT discord_server) FROM names")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

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

    async fn insert_name(
        pool: &sqlx::PgPool,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
        name: &str,
    ) {
        let name_entity = Name::new(discord_id, discord_server, name.to_string());
        sqlx::query(
            r#"
            INSERT INTO names (id, discord_id, discord_server, name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(name_entity.id.discord_id.0 as i64)
        .bind(name_entity.id.discord_server.0 as i64)
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

        let result = repo
            .list_by_server(DiscordServerId(12345), 10, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_first_page() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_server = DiscordServerId(99999);
        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        // Insert names in any order
        insert_name(&pool, discord_id2, discord_server, "Charlie").await;
        insert_name(&pool, discord_id1, discord_server, "Alice").await;
        insert_name(&pool, discord_id3, discord_server, "Bob").await;

        let result = repo.list_by_server(discord_server, 10, None).await.unwrap();

        assert_eq!(result.len(), 3);
        // Should be sorted by discord_id (numeric ordering)
        assert_eq!(result[0].id.discord_id, discord_id1);
        assert_eq!(result[1].id.discord_id, discord_id2);
        assert_eq!(result[2].id.discord_id, discord_id3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_with_limit() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_server = DiscordServerId(88888);
        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        insert_name(&pool, discord_id1, discord_server, "Alice").await;
        insert_name(&pool, discord_id2, discord_server, "Bob").await;
        insert_name(&pool, discord_id3, discord_server, "Charlie").await;

        let result = repo.list_by_server(discord_server, 2, None).await.unwrap();

        assert_eq!(result.len(), 2);
        // Should get first 2 by discord_id ordering
        assert_eq!(result[0].id.discord_id, discord_id1);
        assert_eq!(result[1].id.discord_id, discord_id2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_with_cursor() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_server = DiscordServerId(77777);
        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        insert_name(&pool, discord_id1, discord_server, "Alice").await;
        insert_name(&pool, discord_id2, discord_server, "Bob").await;
        insert_name(&pool, discord_id3, discord_server, "Charlie").await;

        // Get all names to determine the actual ordering by discord_id
        let all_names = repo.list_by_server(discord_server, 10, None).await.unwrap();
        assert_eq!(all_names.len(), 3);

        // Use the first discord_id as cursor to get names after it
        let first_discord_id = all_names[0].id.discord_id;
        let result = repo
            .list_by_server(discord_server, 10, Some(first_discord_id))
            .await
            .unwrap();

        // Should get 2 names (the ones after the first in ordering)
        assert_eq!(result.len(), 2);
        // Verify they match the expected ordering
        assert_eq!(result[0].id.discord_id, all_names[1].id.discord_id);
        assert_eq!(result[1].id.discord_id, all_names[2].id.discord_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_cursor_no_results() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_server = DiscordServerId(66666);
        let discord_id1 = DiscordId(111);

        insert_name(&pool, discord_id1, discord_server, "Alice").await;

        // Use a discord_id larger than all - no names after this
        let large_id = DiscordId(999999999999999);
        let result = repo
            .list_by_server(discord_server, 10, Some(large_id))
            .await
            .unwrap();

        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_by_server_different_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server1 = DiscordServerId(11111);
        let server2 = DiscordServerId(22222);
        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);

        insert_name(&pool, discord_id1, server1, "ServerOne").await;
        insert_name(&pool, discord_id2, server2, "ServerTwo").await;

        let result = repo.list_by_server(server1, 10, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ServerOne");

        let result = repo.list_by_server(server2, 10, None).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ServerTwo");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_empty() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let result = repo.list_servers(10, None).await.unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_returns_distinct() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server1 = DiscordServerId(11111);
        let server2 = DiscordServerId(22222);
        let server3 = DiscordServerId(33333);

        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        // Two names on server1, one each on server2 and server3
        insert_name(&pool, discord_id1, server1, "Alice").await;
        insert_name(&pool, discord_id2, server1, "Bob").await;
        insert_name(&pool, discord_id3, server2, "Charlie").await;
        insert_name(&pool, discord_id1, server3, "AliceOnThree").await;

        let result = repo.list_servers(10, None).await.unwrap();
        assert_eq!(result.len(), 3);
        // Should be sorted ascending
        assert_eq!(result, vec![server1, server2, server3]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_limit() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        insert_name(&pool, discord_id1, DiscordServerId(11111), "Alice").await;
        insert_name(&pool, discord_id2, DiscordServerId(22222), "Bob").await;
        insert_name(&pool, discord_id3, DiscordServerId(33333), "Charlie").await;

        let result = repo.list_servers(2, None).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result, vec![DiscordServerId(11111), DiscordServerId(22222)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_cursor() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_id1 = DiscordId(111);
        let discord_id2 = DiscordId(222);
        let discord_id3 = DiscordId(333);

        insert_name(&pool, discord_id1, DiscordServerId(11111), "Alice").await;
        insert_name(&pool, discord_id2, DiscordServerId(22222), "Bob").await;
        insert_name(&pool, discord_id3, DiscordServerId(33333), "Charlie").await;

        // Cursor after server 11111 should return 22222 and 33333
        let result = repo
            .list_servers(10, Some(DiscordServerId(11111)))
            .await
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result, vec![DiscordServerId(22222), DiscordServerId(33333)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_cursor_past_end() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let discord_id1 = DiscordId(111);
        insert_name(&pool, discord_id1, DiscordServerId(11111), "Alice").await;

        let result = repo
            .list_servers(10, Some(DiscordServerId(99999)))
            .await
            .unwrap();
        assert_eq!(result.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_names_by_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        let server = DiscordServerId(11111);
        insert_name(&pool, DiscordId(1), server, "Alice").await;
        insert_name(&pool, DiscordId(2), server, "Bob").await;

        let count = repo.count_names_by_server(server).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());

        insert_name(&pool, DiscordId(1), DiscordServerId(100), "Alice").await;
        insert_name(&pool, DiscordId(2), DiscordServerId(200), "Bob").await;
        insert_name(&pool, DiscordId(3), DiscordServerId(200), "Carol").await;

        let count = repo.count_servers().await.unwrap();
        assert_eq!(count, 2);
    }
}
