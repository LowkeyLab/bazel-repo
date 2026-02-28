use name::{DiscordId, DiscordServerId, Name};
use name_repo::{NameCounter, NameCreator, NameDeleter, NameReader, NameUpdater};
use uuid::Uuid;

pub struct Service<T>
where
    T: NameCreator + NameReader + NameUpdater + NameDeleter + NameCounter,
{
    repo: T,
}

impl<T> Service<T>
where
    T: NameCreator + NameReader + NameUpdater + NameDeleter + NameCounter,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_name(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
        name: String,
    ) -> anyhow::Result<Uuid> {
        let name_entity = Name::new(discord_id, discord_server, name);
        self.repo.save(name_entity).await
    }

    pub async fn update_name(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
        new_name: String,
    ) -> anyhow::Result<()> {
        self.repo
            .update(discord_id, discord_server, new_name)
            .await?;
        Ok(())
    }

    pub async fn get_name(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<Option<Name>> {
        self.repo.get(discord_id, discord_server).await
    }

    pub async fn delete_name(
        &self,
        discord_id: DiscordId,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<()> {
        self.repo.delete(discord_id, discord_server).await?;
        Ok(())
    }

    pub async fn list_names(
        &self,
        discord_server: DiscordServerId,
        limit: i64,
        cursor: Option<DiscordId>,
    ) -> anyhow::Result<Vec<Name>> {
        self.repo
            .list_by_server(discord_server, limit, cursor)
            .await
    }

    pub async fn list_servers(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<DiscordServerId>> {
        self.repo.list_servers(limit, cursor).await
    }

    pub async fn count_names_by_server(
        &self,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<i64> {
        self.repo.count_names_by_server(discord_server).await
    }

    pub async fn count_servers(&self) -> anyhow::Result<i64> {
        self.repo.count_servers().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use name_repo::Repo;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    /// Spins up a fresh PostgreSQL container for a single test Returns the pool and container. Container is dropped when test completes.
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

    #[test]
    fn dummy() {
        // Dummy test for gazelle
        assert_eq!(true, true);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_name() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let discord_id = DiscordId(123456789);
        let discord_server = DiscordServerId(987654321);
        let name_str = "TestName".to_string();
        let result = service
            .create_name(discord_id, discord_server, name_str.clone())
            .await;

        assert!(result.is_ok());

        // Verify in DB
        let row: (String,) =
            sqlx::query_as("SELECT name FROM names WHERE discord_id = $1 AND discord_server = $2")
                .bind(discord_id.0 as i64)
                .bind(discord_server.0 as i64)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.0, name_str);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_update_name() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let discord_id = DiscordId(123456789);
        let discord_server = DiscordServerId(987654321);

        // Create initial name
        service
            .create_name(discord_id, discord_server, "InitialName".to_string())
            .await
            .expect("Failed to create name");

        // Update the name
        let new_name = "UpdatedName".to_string();
        let result = service
            .update_name(discord_id, discord_server, new_name.clone())
            .await;

        assert!(result.is_ok());

        // Verify in DB
        let row: (String,) =
            sqlx::query_as("SELECT name FROM names WHERE discord_id = $1 AND discord_server = $2")
                .bind(discord_id.0 as i64)
                .bind(discord_server.0 as i64)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(row.0, new_name);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_name() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let discord_id = DiscordId(123456789);
        let discord_server = DiscordServerId(987654321);
        let name_str = "TestName".to_string();

        // Create a name first
        service
            .create_name(discord_id, discord_server, name_str.clone())
            .await
            .expect("Failed to create name");

        // Test retrieval of existing name
        let result = service
            .get_name(discord_id, discord_server)
            .await
            .expect("Failed to get name");
        assert!(result.is_some());
        let name = result.unwrap();
        assert_eq!(name.id.discord_id, discord_id);
        assert_eq!(name.id.discord_server, discord_server);
        assert_eq!(name.name, name_str);

        // Test retrieval of non-existent name
        let result = service
            .get_name(discord_id, DiscordServerId(111111111))
            .await
            .expect("Failed to query for non-existent name");
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete_name() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let discord_id = DiscordId(123456789);
        let discord_server = DiscordServerId(987654321);

        // Create a name first
        service
            .create_name(discord_id, discord_server, "TestName".to_string())
            .await
            .expect("Failed to create name");

        // Delete the name
        let result = service.delete_name(discord_id, discord_server).await;
        assert!(result.is_ok());

        // Verify it's gone from DB
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM names WHERE discord_id = $1 AND discord_server = $2",
        )
        .bind(discord_id.0 as i64)
        .bind(discord_server.0 as i64)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(count.0, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        // Create names on different servers
        service
            .create_name(DiscordId(111), DiscordServerId(11111), "Alice".to_string())
            .await
            .unwrap();
        service
            .create_name(DiscordId(222), DiscordServerId(22222), "Bob".to_string())
            .await
            .unwrap();

        let result = service.list_servers(10, None).await.unwrap();
        assert_eq!(result, vec![DiscordServerId(11111), DiscordServerId(22222)]);

        // With cursor
        let result = service
            .list_servers(10, Some(DiscordServerId(11111)))
            .await
            .unwrap();
        assert_eq!(result, vec![DiscordServerId(22222)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_names_by_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let server = DiscordServerId(11111);
        service
            .create_name(DiscordId(1), server, "Alice".to_string())
            .await
            .unwrap();
        service
            .create_name(DiscordId(2), server, "Bob".to_string())
            .await
            .unwrap();

        let count = service.count_names_by_server(server).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        service
            .create_name(DiscordId(1), DiscordServerId(100), "Alice".to_string())
            .await
            .unwrap();
        service
            .create_name(DiscordId(2), DiscordServerId(200), "Bob".to_string())
            .await
            .unwrap();

        let count = service.count_servers().await.unwrap();
        assert_eq!(count, 2);
    }
}
