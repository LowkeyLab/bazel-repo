use sqlx::PgPool;

/// Runs all migrations for the nicknamer2 database.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::postgres;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    async fn setup_pool() -> (sqlx::PgPool, impl std::any::Any) {
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

        (pool, container)
    }

    #[tokio::test]
    async fn test_migration_creates_names_table() {
        let (pool, _container) = setup_pool().await;

        run_migrations(&pool)
            .await
            .expect("Migrations should run successfully");

        let names_exists: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_name = 'names'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check names table");

        assert!(names_exists.0, "Names table should exist after migration");
    }

    #[tokio::test]
    async fn test_migration_creates_servers_table() {
        let (pool, _container) = setup_pool().await;

        run_migrations(&pool)
            .await
            .expect("Migrations should run successfully");

        let servers_exists: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_name = 'servers'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check servers table");

        assert!(
            servers_exists.0,
            "Servers table should exist after migration"
        );
    }

    #[tokio::test]
    async fn test_migration_tracking_table_exists() {
        let (pool, _container) = setup_pool().await;

        run_migrations(&pool)
            .await
            .expect("Migrations should run successfully");

        let migration_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .expect("Failed to query _sqlx_migrations");

        assert_eq!(
            migration_count.0, 2,
            "_sqlx_migrations should contain exactly two applied migrations"
        );
    }
}
