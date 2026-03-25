use sqlx::{
    migrate::{MigrateError, Migrator},
    sqlite::SqlitePool,
};

#[allow(dead_code)]
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");

#[allow(dead_code)]
pub async fn run_sqlite_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    SQLITE_MIGRATOR.run(pool).await
}

#[cfg(test)]
mod tests {
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    use super::run_sqlite_migrations;

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
}
