use sqlx::PgPool;

const MIGRATION_003: &str = include_str!("003_drop_users_recreate_names.sql");
const MIGRATION_004: &str = include_str!("004_create_servers_table.sql");

/// Runs all migrations for the nicknamer2 database.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(MIGRATION_003).execute(pool).await?;
    sqlx::raw_sql(MIGRATION_004).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::postgres;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    #[test]
    fn dummy() {
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

        let result = run_migrations(&pool).await;
        assert!(result.is_ok(), "Migrations should run successfully");

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

        // Verify users table does NOT exist
        let users_table_exists: (bool,) = sqlx::query_as(
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
        .expect("Failed to check if users table exists");

        assert!(
            !users_table_exists.0,
            "Users table should NOT exist after migration"
        );
    }
}
