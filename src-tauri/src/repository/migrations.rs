use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::PgPool,
    sqlite::SqlitePool,
};

#[allow(dead_code)]
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");
#[allow(dead_code)]
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[allow(dead_code)]
pub async fn run_sqlite_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    SQLITE_MIGRATOR.run(pool).await
}

#[allow(dead_code)]
pub async fn run_postgres_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    POSTGRES_MIGRATOR.run(pool).await
}

#[cfg(test)]
mod tests {
    use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions, PgPool, SqlitePool};

    use super::{run_postgres_migrations, run_sqlite_migrations};

    #[tokio::test]
    async fn sqlite_migration_is_idempotent_and_has_expected_schema() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to connect sqlite memory db");

        run_sqlite_migrations(&pool)
            .await
            .expect("first sqlite migration run should succeed");
        run_sqlite_migrations(&pool)
            .await
            .expect("second sqlite migration run should also succeed");

        assert!(
            sqlite_table_exists(&pool, "series").await,
            "table `series` should exist"
        );
        assert!(
            sqlite_table_exists(&pool, "commits").await,
            "table `commits` should exist"
        );
        assert!(
            sqlite_table_exists(&pool, "consistency_alerts").await,
            "table `consistency_alerts` should exist"
        );
        assert!(
            sqlite_table_exists(&pool, "app_settings").await,
            "table `app_settings` should exist"
        );

        assert!(
            sqlite_index_exists(&pool, "idx_series_last_updated_at").await,
            "index `idx_series_last_updated_at` should exist"
        );
        assert!(
            sqlite_index_exists(&pool, "idx_commits_series_created_at").await,
            "index `idx_commits_series_created_at` should exist"
        );

        let series_columns = sqlite_columns(&pool, "series").await;
        assert_eq!(
            series_columns,
            vec![
                "id",
                "name",
                "status",
                "latest_excerpt",
                "last_updated_at",
                "created_at",
                "archived_at",
            ]
        );

        let commits_columns = sqlite_columns(&pool, "commits").await;
        assert_eq!(
            commits_columns,
            vec!["id", "series_id", "content", "created_at"]
        );

        let alerts_columns = sqlite_columns(&pool, "consistency_alerts").await;
        assert_eq!(
            alerts_columns,
            vec![
                "id",
                "op_type",
                "commit_id",
                "reason",
                "created_at",
                "resolved_at",
            ]
        );

        let app_settings_columns = sqlite_columns(&pool, "app_settings").await;
        assert_eq!(app_settings_columns, vec!["key", "value"]);

        let series_ddl: String = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'series'",
        )
        .fetch_one(&pool)
        .await
        .expect("failed to read sqlite series ddl");
        assert!(
            series_ddl.contains("status IN ('active', 'silent', 'archived')"),
            "series status check constraint should include active/silent/archived"
        );
    }

    #[tokio::test]
    async fn postgres_migration_is_optional_and_idempotent() {
        let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                eprintln!(
                    "skip postgres migration test: REMEMBER_TEST_POSTGRES_DSN is not configured"
                );
                return;
            }
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&postgres_dsn)
            .await
            .expect("failed to connect postgres with REMEMBER_TEST_POSTGRES_DSN");

        run_postgres_migrations(&pool)
            .await
            .expect("first postgres migration run should succeed");
        run_postgres_migrations(&pool)
            .await
            .expect("second postgres migration run should also succeed");

        assert!(
            postgres_table_exists(&pool, "series").await,
            "table `series` should exist"
        );
        assert!(
            postgres_table_exists(&pool, "commits").await,
            "table `commits` should exist"
        );
        assert!(
            postgres_table_exists(&pool, "consistency_alerts").await,
            "table `consistency_alerts` should exist"
        );
        assert!(
            postgres_table_exists(&pool, "app_settings").await,
            "table `app_settings` should exist"
        );

        assert!(
            postgres_index_exists(&pool, "idx_series_last_updated_at").await,
            "index `idx_series_last_updated_at` should exist"
        );
        assert!(
            postgres_index_exists(&pool, "idx_commits_series_created_at").await,
            "index `idx_commits_series_created_at` should exist"
        );

        let series_columns = postgres_columns(&pool, "series").await;
        assert_eq!(
            series_columns,
            vec![
                "id",
                "name",
                "status",
                "latest_excerpt",
                "last_updated_at",
                "created_at",
                "archived_at",
            ]
        );

        let commits_columns = postgres_columns(&pool, "commits").await;
        assert_eq!(
            commits_columns,
            vec!["id", "series_id", "content", "created_at"]
        );

        let alerts_columns = postgres_columns(&pool, "consistency_alerts").await;
        assert_eq!(
            alerts_columns,
            vec![
                "id",
                "op_type",
                "commit_id",
                "reason",
                "created_at",
                "resolved_at",
            ]
        );

        let app_settings_columns = postgres_columns(&pool, "app_settings").await;
        assert_eq!(app_settings_columns, vec!["key", "value"]);

        let status_constraint_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_constraint c
                JOIN pg_class t ON t.oid = c.conrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                WHERE n.nspname = current_schema()
                  AND t.relname = 'series'
                  AND c.conname = 'series_status_check'
            )",
        )
        .fetch_one(&pool)
        .await
        .expect("failed to verify postgres series status constraint");
        assert!(
            status_constraint_exists,
            "constraint `series_status_check` should exist on postgres `series`"
        );
    }

    async fn sqlite_table_exists(pool: &SqlitePool, table_name: &str) -> bool {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM sqlite_master
                WHERE type = 'table'
                  AND name = ?
            )",
        )
        .bind(table_name)
        .fetch_one(pool)
        .await
        .expect("failed to query sqlite table existence");
        exists == 1
    }

    async fn sqlite_index_exists(pool: &SqlitePool, index_name: &str) -> bool {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM sqlite_master
                WHERE type = 'index'
                  AND name = ?
            )",
        )
        .bind(index_name)
        .fetch_one(pool)
        .await
        .expect("failed to query sqlite index existence");
        exists == 1
    }

    async fn sqlite_columns(pool: &SqlitePool, table_name: &str) -> Vec<String> {
        let query = format!("PRAGMA table_info({table_name})");
        let rows: Vec<(i64, String, String, i64, Option<String>, i64)> = sqlx::query_as(&query)
            .fetch_all(pool)
            .await
            .expect("failed to query sqlite table columns");
        rows.into_iter().map(|row| row.1).collect()
    }

    async fn postgres_table_exists(pool: &PgPool, table_name: &str) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = current_schema()
                  AND table_name = $1
            )",
        )
        .bind(table_name)
        .fetch_one(pool)
        .await
        .expect("failed to query postgres table existence")
    }

    async fn postgres_index_exists(pool: &PgPool, index_name: &str) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_indexes
                WHERE schemaname = current_schema()
                  AND indexname = $1
            )",
        )
        .bind(index_name)
        .fetch_one(pool)
        .await
        .expect("failed to query postgres index existence")
    }

    async fn postgres_columns(pool: &PgPool, table_name: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT column_name
             FROM information_schema.columns
             WHERE table_schema = current_schema()
               AND table_name = $1
             ORDER BY ordinal_position",
        )
        .bind(table_name)
        .fetch_all(pool)
        .await
        .expect("failed to query postgres table columns")
    }
}
