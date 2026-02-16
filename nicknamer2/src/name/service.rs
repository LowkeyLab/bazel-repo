use name::Name;
use name_repo::{Deleter, Getter, Lister, Saver, Updater};
use uuid::Uuid;

pub struct Service<T>
where
    T: Saver + Updater + Getter + Deleter + Lister,
{
    repo: T,
}

impl<T> Service<T>
where
    T: Saver + Updater + Getter + Deleter + Lister,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_name(
        &self,
        user_id: Uuid,
        server_id: u64,
        name: String,
    ) -> anyhow::Result<Uuid> {
        let name_entity = Name::new(user_id, server_id, name);
        self.repo.save(name_entity).await
    }

    pub async fn update_name(
        &self,
        user_id: Uuid,
        server_id: u64,
        new_name: String,
    ) -> anyhow::Result<()> {
        self.repo.update(user_id, server_id, new_name).await?;
        Ok(())
    }

    pub async fn get_name(&self, user_id: Uuid, server_id: u64) -> anyhow::Result<Option<Name>> {
        self.repo.get(user_id, server_id).await
    }

    pub async fn delete_name(&self, user_id: Uuid, server_id: u64) -> anyhow::Result<()> {
        self.repo.delete(user_id, server_id).await?;
        Ok(())
    }

    pub async fn list_names(
        &self,
        server_id: u64,
        limit: i64,
        cursor: Option<Uuid>,
    ) -> anyhow::Result<Vec<Name>> {
        self.repo.list_by_server(server_id, limit, cursor).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use name_repo::Repo;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};
    use user::User;

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

        // First create a user since names reference users
        let user = User::new(123456789);
        let user_id = user.id;
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
        .execute(&pool)
        .await
        .unwrap();

        let server_id = 987654321;
        let name_str = "TestName".to_string();
        let result = service
            .create_name(user_id, server_id, name_str.clone())
            .await;

        assert!(result.is_ok());

        // Verify in DB
        let row: (String,) =
            sqlx::query_as("SELECT name FROM names WHERE user_id = $1 AND server_id = $2")
                .bind(user_id)
                .bind(server_id as i64)
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

        // Create a user
        let user = User::new(123456789);
        let user_id = user.id;
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
        .execute(&pool)
        .await
        .unwrap();

        let server_id = 987654321;

        // Create initial name
        service
            .create_name(user_id, server_id, "InitialName".to_string())
            .await
            .expect("Failed to create name");

        // Update the name
        let new_name = "UpdatedName".to_string();
        let result = service
            .update_name(user_id, server_id, new_name.clone())
            .await;

        assert!(result.is_ok());

        // Verify in DB
        let row: (String,) =
            sqlx::query_as("SELECT name FROM names WHERE user_id = $1 AND server_id = $2")
                .bind(user_id)
                .bind(server_id as i64)
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

        // Create a user
        let user = User::new(123456789);
        let user_id = user.id;
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
        .execute(&pool)
        .await
        .unwrap();

        let server_id = 987654321;
        let name_str = "TestName".to_string();

        // Create a name first
        service
            .create_name(user_id, server_id, name_str.clone())
            .await
            .expect("Failed to create name");

        // Test retrieval of existing name
        let result = service
            .get_name(user_id, server_id)
            .await
            .expect("Failed to get name");
        assert!(result.is_some());
        let name = result.unwrap();
        assert_eq!(name.user_id, user_id);
        assert_eq!(name.server_id, server_id);
        assert_eq!(name.name, name_str);

        // Test retrieval of non-existent name
        let result = service
            .get_name(user_id, 111111111)
            .await
            .expect("Failed to query for non-existent name");
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete_name() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool.clone());
        let service = Service::new(repo);

        // Create a user
        let user = User::new(123456789);
        let user_id = user.id;
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
        .execute(&pool)
        .await
        .unwrap();

        let server_id = 987654321;

        // Create a name first
        service
            .create_name(user_id, server_id, "TestName".to_string())
            .await
            .expect("Failed to create name");

        // Delete the name
        let result = service.delete_name(user_id, server_id).await;
        assert!(result.is_ok());

        // Verify it's gone from DB
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM names WHERE user_id = $1 AND server_id = $2")
                .bind(user_id)
                .bind(server_id as i64)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count.0, 0);
    }
}
