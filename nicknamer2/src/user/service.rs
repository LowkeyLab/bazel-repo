use repo::{ByDiscordIdGetter, Saver};
use user::User;

struct Service<T>
where
    T: Saver + ByDiscordIdGetter,
{
    repo: T,
}

impl<T> Service<T>
where
    T: Saver + ByDiscordIdGetter,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_user(&self, discord_id: u64) -> Result<(), repo::Error> {
        let user = User::new(discord_id);
        self.repo.save(user).await
    }

    pub async fn get_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, repo::Error> {
        self.repo.get_by_discord_id(discord_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use repo::Repo;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    /// Spins up a fresh PostgreSQL container for a single test
    /// Returns the pool and container. Container is dropped when test completes.
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
    async fn test_create_user() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        let discord_id = 123456789;
        let result = service.create_user(discord_id).await;

        assert!(result.is_ok());

        // Verify in DB
        let row: (i64,) = sqlx::query_as("SELECT discord_id FROM users WHERE discord_id = $1")
            .bind(discord_id as i64)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(row.0, discord_id as i64);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_by_discord_id() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let discord_id = 987654321;

        // Create a user first
        service
            .create_user(discord_id)
            .await
            .expect("Failed to create user");

        // Test retrieval of existing user
        let result = service
            .get_by_discord_id(discord_id)
            .await
            .expect("Failed to get user");
        assert!(result.is_some());
        let user = result.unwrap();
        assert_eq!(user.discord_id, discord_id);

        // Test retrieval of non-existent user
        let result = service
            .get_by_discord_id(111111111)
            .await
            .expect("Failed to query for non-existent user");
        assert!(result.is_none());
    }
}
