use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::{run_postgres_migrations, run_sqlite_migrations};
use tauri_app_lib::repository::{
    AppendCommitInput, CreateSeriesInput, ListSeriesQuery, MemoRepository,
};

#[tokio::test]
async fn p3_t1_dual_sync_keeps_commit_id_and_created_at_consistent() {
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t1_dual_sync_keeps_commit_id_and_created_at_consistent: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t1");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t1");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for p3-t1");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t1");

    let prefix = format!("p3t1-dual-{}", nonce());
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());
    let series_id = format!("{prefix}-series");
    let commit_id = format!("{prefix}-commit");
    let created_at = "2099-03-17T10:00:00Z";

    repository
        .create_series(CreateSeriesInput {
            id: series_id.clone(),
            name: "Inbox".to_string(),
            created_at: "2099-03-17T09:00:00Z".to_string(),
        })
        .await
        .expect("create series should succeed");

    repository
        .append_commit(AppendCommitInput {
            commit_id: commit_id.clone(),
            series_id: series_id.clone(),
            content: "dual-sync-commit".to_string(),
            created_at: created_at.to_string(),
        })
        .await
        .expect("append commit should succeed");

    let sqlite_created_at: String =
        sqlx::query_scalar("SELECT created_at FROM commits WHERE id = ?")
            .bind(&commit_id)
            .fetch_one(&sqlite_pool)
            .await
            .expect("sqlite commit should exist");
    let postgres_created_at: String = sqlx::query_scalar(
        "SELECT to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
         FROM commits
         WHERE id = $1",
    )
    .bind(&commit_id)
    .fetch_one(&postgres_pool)
    .await
    .expect("postgres commit should exist");

    assert_eq!(sqlite_created_at, created_at);
    assert_eq!(postgres_created_at, created_at);

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

#[tokio::test]
async fn p3_t1_dual_sync_reads_series_from_sqlite() {
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t1_dual_sync_reads_series_from_sqlite: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t1 read-source");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t1 read-source");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for p3-t1 read-source");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t1 read-source");

    let prefix = format!("p3t1-read-{}", nonce());
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository = repository::DualSyncRepository::new(sqlite_pool, postgres_pool.clone());
    let sqlite_series = format!("{prefix}-sqlite-series");
    let pg_only_series = format!("{prefix}-pg-only-series");

    repository
        .create_series(CreateSeriesInput {
            id: sqlite_series.clone(),
            name: "SQLite-Series".to_string(),
            created_at: "2099-03-17T08:00:00Z".to_string(),
        })
        .await
        .expect("dual create series should succeed");

    sqlx::query(
        "INSERT INTO series (
            id,
            name,
            status,
            latest_excerpt,
            last_updated_at,
            created_at,
            archived_at
        ) VALUES ($1, $2, 'active', '', $3::timestamptz, $3::timestamptz, NULL)",
    )
    .bind(&pg_only_series)
    .bind("PG-Only-Series")
    .bind("2099-03-17T07:00:00Z")
    .execute(&postgres_pool)
    .await
    .expect("pg-only series insert should succeed");

    let listed = repository
        .list_series(ListSeriesQuery {
            include_archived: true,
            cursor: None,
            limit: 50,
        })
        .await
        .expect("dual list series should succeed");

    assert!(
        listed.items.iter().any(|item| item.id == sqlite_series),
        "sqlite-backed series should be visible from dual_sync read"
    );
    assert!(
        listed.items.iter().all(|item| item.id != pg_only_series),
        "postgres-only series should not be visible because dual_sync reads sqlite"
    );

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

async fn cleanup_postgres_prefix(pool: &sqlx::PgPool, prefix: &str) {
    let like_pattern = format!("{prefix}%");
    let _ = sqlx::query("DELETE FROM commits WHERE series_id LIKE $1 OR id LIKE $1")
        .bind(&like_pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM series WHERE id LIKE $1")
        .bind(&like_pattern)
        .execute(pool)
        .await;
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos()
}
