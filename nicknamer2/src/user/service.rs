use repo::{Repo, Saver};
use user::User;

struct Service<T>
where
    T: Saver,
{
    repo: T,
}

impl<T> Service<T>
where
    T: Saver,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_user(&self, discord_id: u64) -> Result<(), repo::Error> {
        let user = User::new(discord_id);
        self.repo.save(user).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    async fn setup_db() -> (
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

        // Configure connection options with retries and longer timeout
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to database");

        // Run migrations using the shared migrations library
        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, container)
    }

    #[tokio::test]
    async fn test_create_user() {
        let (pool, _container) = setup_db().await;
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
}
