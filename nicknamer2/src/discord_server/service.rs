use discord_server::Server;
use discord_server_repo::{ServerCreator, ServerReader};
use name::DiscordServerId;

pub struct Service<T>
where
    T: ServerCreator + ServerReader,
{
    repo: T,
}

impl<T> Service<T>
where
    T: ServerCreator + ServerReader,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_server(
        &self,
        discord_server: DiscordServerId,
        display_name: String,
    ) -> anyhow::Result<DiscordServerId> {
        if discord_server.0 == 0 {
            return Err(anyhow::anyhow!("Server ID must be greater than 0"));
        }
        if display_name.is_empty() {
            return Err(anyhow::anyhow!("Display name must not be empty"));
        }
        let server = Server::new(discord_server, display_name);
        self.repo.save(server).await?;
        Ok(discord_server)
    }

    pub async fn get_server(
        &self,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<Option<Server>> {
        self.repo.get(discord_server).await
    }

    pub async fn list_servers(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<Server>> {
        self.repo.list(limit, cursor).await
    }

    pub async fn count_servers(&self) -> anyhow::Result<i64> {
        self.repo.count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_server_repo::Repo;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    #[test]
    fn dummy() {
        assert_eq!(true, true);
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
    async fn test_create_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let id = service
            .create_server(DiscordServerId(12345), "Test Server".to_string())
            .await
            .unwrap();

        assert_eq!(id, DiscordServerId(12345));

        // Verify it was persisted
        let server = service.get_server(DiscordServerId(12345)).await.unwrap();
        assert!(server.is_some());
        assert_eq!(server.unwrap().display_name, "Test Server");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_server_zero_id_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let result = service
            .create_server(DiscordServerId(0), "Test".to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("greater than 0"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_server_empty_name_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let result = service
            .create_server(DiscordServerId(12345), "".to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not be empty"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_duplicate_server_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        service
            .create_server(DiscordServerId(12345), "Server One".to_string())
            .await
            .unwrap();

        let result = service
            .create_server(DiscordServerId(12345), "Server Two".to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_and_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        service
            .create_server(DiscordServerId(111), "First".to_string())
            .await
            .unwrap();
        service
            .create_server(DiscordServerId(222), "Second".to_string())
            .await
            .unwrap();

        let servers = service.list_servers(10, None).await.unwrap();
        assert_eq!(servers.len(), 2);

        let count = service.count_servers().await.unwrap();
        assert_eq!(count, 2);
    }
}
