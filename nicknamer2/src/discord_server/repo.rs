use discord_server::Server;
use name::DiscordServerId;
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;

/// Data Access Object for servers table mapping
#[derive(Debug, sqlx::FromRow)]
struct ServerDAO {
    #[allow(dead_code)]
    id: Uuid,
    discord_server: i64,
    display_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ServerDAO> for Server {
    fn from(dao: ServerDAO) -> Self {
        Server {
            id: DiscordServerId(dao.discord_server as u64),
            display_name: dao.display_name,
            created_at: dao.created_at,
            updated_at: dao.updated_at,
        }
    }
}

/// Creates servers in the database.
pub trait ServerCreator {
    fn save(&self, server: Server) -> impl Future<Output = anyhow::Result<Uuid>> + Send;
}

/// Reads servers from the database.
pub trait ServerReader {
    fn get(
        &self,
        discord_server: DiscordServerId,
    ) -> impl Future<Output = anyhow::Result<Option<Server>>> + Send;

    fn list(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> impl Future<Output = anyhow::Result<Vec<Server>>> + Send;

    fn count(&self) -> impl Future<Output = anyhow::Result<i64>> + Send;
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

impl ServerCreator for Repo {
    async fn save(&self, server: Server) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO servers (id, discord_server, display_name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(server.id.0 as i64)
        .bind(&server.display_name)
        .bind(server.created_at)
        .bind(server.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }
}

impl ServerReader for Repo {
    async fn get(&self, discord_server: DiscordServerId) -> anyhow::Result<Option<Server>> {
        let dao = sqlx::query_as::<_, ServerDAO>(
            r#"
            SELECT id, discord_server, display_name, created_at, updated_at
            FROM servers
            WHERE discord_server = $1
            "#,
        )
        .bind(discord_server.0 as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }

    async fn list(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<Server>> {
        let daos = if let Some(last_server_id) = cursor {
            sqlx::query_as::<_, ServerDAO>(
                r#"
                SELECT id, discord_server, display_name, created_at, updated_at
                FROM servers
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
            sqlx::query_as::<_, ServerDAO>(
                r#"
                SELECT id, discord_server, display_name, created_at, updated_at
                FROM servers
                ORDER BY discord_server ASC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(daos.into_iter().map(Into::into).collect())
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers")
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_save_and_get_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let server = Server::new(DiscordServerId(12345), "Test Server".to_string());
        let _uuid = repo.save(server).await.unwrap();

        let found = repo.get(DiscordServerId(12345)).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, DiscordServerId(12345));
        assert_eq!(found.display_name, "Test Server");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_nonexistent_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let found = repo.get(DiscordServerId(99999)).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_save_duplicate_server_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let server1 = Server::new(DiscordServerId(12345), "Server One".to_string());
        repo.save(server1).await.unwrap();

        let server2 = Server::new(DiscordServerId(12345), "Server Two".to_string());
        let result = repo.save(server2).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();

        let servers = repo.list(10, None).await.unwrap();
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0].id, DiscordServerId(111));
        assert_eq!(servers[1].id, DiscordServerId(222));
        assert_eq!(servers[2].id, DiscordServerId(333));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_cursor() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();

        let servers = repo.list(10, Some(DiscordServerId(111))).await.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, DiscordServerId(222));
        assert_eq!(servers[1].id, DiscordServerId(333));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_limit() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();

        let servers = repo.list(2, None).await.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, DiscordServerId(111));
        assert_eq!(servers[1].id, DiscordServerId(222));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        assert_eq!(repo.count().await.unwrap(), 0);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();

        assert_eq!(repo.count().await.unwrap(), 2);
    }
}
