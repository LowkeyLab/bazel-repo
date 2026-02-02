use user::User;
use user_repo::{ByDiscordIdGetter, ByIdGetter, Deleter, Error, Saver, Updater};
use uuid::Uuid;

struct Service<T>
where
    T: Saver + ByDiscordIdGetter + ByIdGetter + Updater + Deleter,
{
    repo: T,
}

impl<T> Service<T>
where
    T: Saver + ByDiscordIdGetter + ByIdGetter + Updater + Deleter,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_user(&self, discord_id: u64) -> Result<(), Error> {
        let user = User::new(discord_id);
        self.repo.save(user).await
    }

    pub async fn get_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, Error> {
        self.repo.get_by_discord_id(discord_id).await
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<User>, Error> {
        self.repo.get_by_id(id).await
    }

    pub async fn update_user(&self, user: User) -> Result<(), Error> {
        self.repo.update(user).await
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<bool, Error> {
        self.repo.delete(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};
    use user_repo::Repo;
    use uuid::Uuid;

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_by_id() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let discord_id = 555555555;

        // Create a user first
        service
            .create_user(discord_id)
            .await
            .expect("Failed to create user");

        // Get the user by discord_id to obtain the UUID
        let created_user = service
            .get_by_discord_id(discord_id)
            .await
            .expect("Failed to get user")
            .expect("User should exist");

        // Test retrieval by UUID
        let result = service
            .get_by_id(created_user.id)
            .await
            .expect("Failed to get user by id");
        assert!(result.is_some());
        let user = result.unwrap();
        assert_eq!(user.id, created_user.id);
        assert_eq!(user.discord_id, discord_id);

        // Test retrieval of non-existent user
        let fake_id = Uuid::new_v4();
        let result = service
            .get_by_id(fake_id)
            .await
            .expect("Failed to query for non-existent user");
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_update_user() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let discord_id = 111222333;

        // Create a user first
        service
            .create_user(discord_id)
            .await
            .expect("Failed to create user");

        // Get the user
        let mut user = service
            .get_by_discord_id(discord_id)
            .await
            .expect("Failed to get user")
            .expect("User should exist");

        // Update the discord_id
        let new_discord_id = 999888777;
        user.discord_id = new_discord_id;

        let result = service.update_user(user.clone()).await;
        assert!(result.is_ok());

        // Verify the update
        let updated_user = service
            .get_by_id(user.id)
            .await
            .expect("Failed to get updated user")
            .expect("User should exist");

        assert_eq!(updated_user.discord_id, new_discord_id);
        assert_eq!(updated_user.id, user.id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete_user() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let discord_id = 444555666;

        // Create a user first
        service
            .create_user(discord_id)
            .await
            .expect("Failed to create user");

        // Get the user to obtain the UUID
        let user = service
            .get_by_discord_id(discord_id)
            .await
            .expect("Failed to get user")
            .expect("User should exist");

        // Delete the user
        let result = service.delete_user(user.id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true); // Should return true for successful deletion

        // Verify the user no longer exists
        let result = service
            .get_by_id(user.id)
            .await
            .expect("Failed to query for deleted user");
        assert!(result.is_none());

        // Try to delete a non-existent user
        let fake_id = Uuid::new_v4();
        let result = service.delete_user(fake_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false); // Should return false for non-existent user
    }
}
