use sqlx::PgPool;

// Include the migration SQL at compile time
const MIGRATION_001: &str = include_str!("001_create_users_table.sql");
const MIGRATION_002: &str = include_str!("002_create_names_table.sql");

/// Runs all migrations for the nicknamer2 database.
///
/// This function applies all SQL migrations to the provided PostgreSQL database.
/// It should be called during test setup to ensure the database schema is ready.
///
/// # Errors
///
/// Returns an error if any migration fails to execute.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Run the users table migration
    sqlx::query(MIGRATION_001).execute(pool).await?;
    // Run the names table migration
    sqlx::query(MIGRATION_002).execute(pool).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::postgres;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    #[test]
    fn dummy() {
        // Dummy test for gazelle test discovery
        assert_eq!(true, true);
    }

    #[tokio::test]
    async fn test_migrations_run_successfully() {
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

        // Run migrations
        let result = run_migrations(&pool).await;
        assert!(result.is_ok(), "Migrations should run successfully");

        // Verify the users table exists
        let table_exists: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' 
                AND table_name = 'users'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check if table exists");

        assert!(table_exists.0, "Users table should exist after migration");

        // Verify the names table exists
        let names_table_exists: (bool,) = sqlx::query_as(
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
        .expect("Failed to check if names table exists");

        assert!(
            names_table_exists.0,
            "Names table should exist after migration"
        );
    }
}
