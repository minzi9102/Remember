use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::{run_postgres_migrations, run_sqlite_migrations};
use tauri_app_lib::repository::{
    AppendCommitInput, ArchiveSeriesInput, CreateSeriesInput, MarkSilentSeriesInput,
    MemoRepository, RepositoryError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeriesState {
    status: String,
    latest_excerpt: String,
    last_updated_at: String,
    archived_at: Option<String>,
}

#[tokio::test]
async fn p3_t3_dual_sync_rolls_back_writes_and_maps_error_codes() {
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t3_dual_sync_rolls_back_writes_and_maps_error_codes: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t3");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t3");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for p3-t3");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t3");

    let prefix = format!("p3t3-{}", nonce());
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());

    // Case 1: create_series timeout => both sides rolled back.
    let blocked_create_series_id = format!("{prefix}-blocked-create-series");
    let mut lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to start postgres lock transaction for create_series");
    sqlx::query("LOCK TABLE series IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock_tx)
        .await
        .expect("failed to lock postgres series table for create_series");

    let create_error = repository
        .create_series(CreateSeriesInput {
            id: blocked_create_series_id.clone(),
            name: "Blocked-Create".to_string(),
            created_at: "2099-03-20T09:00:00Z".to_string(),
        })
        .await
        .expect_err("create_series should fail when postgres series table is locked");
    assert_pg_timeout_error(create_error);

    lock_tx
        .rollback()
        .await
        .expect("failed to rollback create_series lock transaction");

    assert_series_absent_in_both(&sqlite_pool, &postgres_pool, &blocked_create_series_id).await;

    // Prepare one shared series for append/archive/silent cases.
    let shared_series_id = format!("{prefix}-shared-series");
    repository
        .create_series(CreateSeriesInput {
            id: shared_series_id.clone(),
            name: "Shared-Series".to_string(),
            created_at: "2099-03-20T10:00:00Z".to_string(),
        })
        .await
        .expect("shared series create should succeed");

    // Case 2: append_commit timeout => commit rolled back and series snapshot restored.
    let before_append = sqlite_series_state(&sqlite_pool, &shared_series_id).await;
    let blocked_commit_id = format!("{prefix}-blocked-commit");
    let mut lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to start postgres lock transaction for append_commit");
    sqlx::query("LOCK TABLE commits IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock_tx)
        .await
        .expect("failed to lock postgres commits table for append_commit");

    let append_error = repository
        .append_commit(AppendCommitInput {
            commit_id: blocked_commit_id.clone(),
            series_id: shared_series_id.clone(),
            content: "blocked-append".to_string(),
            created_at: "2099-03-20T10:00:01Z".to_string(),
        })
        .await
        .expect_err("append_commit should fail when postgres commits table is locked");
    assert_pg_timeout_error(append_error);

    lock_tx
        .rollback()
        .await
        .expect("failed to rollback append_commit lock transaction");

    assert_commit_absent_in_both(&sqlite_pool, &postgres_pool, &blocked_commit_id).await;
    let after_append = sqlite_series_state(&sqlite_pool, &shared_series_id).await;
    assert_eq!(
        after_append, before_append,
        "append timeout should restore sqlite series snapshot"
    );

    // Case 3: archive_series timeout => archive changes rolled back.
    let mut lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to start postgres lock transaction for archive_series");
    sqlx::query("LOCK TABLE series IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock_tx)
        .await
        .expect("failed to lock postgres series table for archive_series");

    let archive_error = repository
        .archive_series(ArchiveSeriesInput {
            series_id: shared_series_id.clone(),
            archived_at: "2099-03-20T10:00:02Z".to_string(),
        })
        .await
        .expect_err("archive_series should fail when postgres series table is locked");
    assert_pg_timeout_error(archive_error);

    lock_tx
        .rollback()
        .await
        .expect("failed to rollback archive_series lock transaction");

    assert_series_status_in_both(&sqlite_pool, &postgres_pool, &shared_series_id, "active").await;

    // Case 4: mark_silent_series timeout => silent mark rolled back.
    let silent_target_series_id = format!("{prefix}-silent-target");
    repository
        .create_series(CreateSeriesInput {
            id: silent_target_series_id.clone(),
            name: "Silent-Target".to_string(),
            created_at: "2020-01-01T00:00:00Z".to_string(),
        })
        .await
        .expect("silent target create should succeed");

    let mut lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to start postgres lock transaction for mark_silent_series");
    sqlx::query("LOCK TABLE series IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock_tx)
        .await
        .expect("failed to lock postgres series table for mark_silent_series");

    let silent_error = repository
        .mark_silent_series(MarkSilentSeriesInput {
            threshold_before: "2099-03-20T00:00:00Z".to_string(),
        })
        .await
        .expect_err("mark_silent_series should fail when postgres series table is locked");
    assert_pg_timeout_error(silent_error);

    lock_tx
        .rollback()
        .await
        .expect("failed to rollback mark_silent_series lock transaction");

    assert_series_status_in_both(
        &sqlite_pool,
        &postgres_pool,
        &silent_target_series_id,
        "active",
    )
    .await;

    // Case 5: non-timeout single-side failure => DUAL_WRITE_FAILED + sqlite compensation.
    let single_side_id = format!("{prefix}-single-side");
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
    .bind(&single_side_id)
    .bind("PG-Only-Seed")
    .bind("2099-03-20T11:00:00Z")
    .execute(&postgres_pool)
    .await
    .expect("failed to seed postgres-only series for single-side failure case");

    let single_side_error = repository
        .create_series(CreateSeriesInput {
            id: single_side_id.clone(),
            name: "Conflicting-Create".to_string(),
            created_at: "2099-03-20T11:00:01Z".to_string(),
        })
        .await
        .expect_err("create_series should fail on postgres conflict while sqlite succeeds first");
    match single_side_error {
        RepositoryError::DualWriteFailed(_) => {}
        other => panic!("expected dual write failed error, got {other:?}"),
    }

    let sqlite_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM series WHERE id = ?")
        .bind(&single_side_id)
        .fetch_one(&sqlite_pool)
        .await
        .expect("failed to query sqlite series count after single-side compensation");
    assert_eq!(
        sqlite_count, 0,
        "sqlite should be compensated when postgres side rejects create"
    );

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

fn assert_pg_timeout_error(error: RepositoryError) {
    match error {
        RepositoryError::PgTimeout(_) => {}
        other => panic!("expected PG timeout error, got {other:?}"),
    }
}

async fn sqlite_series_state(pool: &sqlx::SqlitePool, series_id: &str) -> SeriesState {
    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT status, latest_excerpt, last_updated_at, archived_at
         FROM series
         WHERE id = ?",
    )
    .bind(series_id)
    .fetch_one(pool)
    .await
    .expect("failed to query sqlite series state");

    SeriesState {
        status: row.0,
        latest_excerpt: row.1,
        last_updated_at: row.2,
        archived_at: row.3,
    }
}

async fn assert_series_absent_in_both(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    series_id: &str,
) {
    let sqlite_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM series WHERE id = ?")
        .bind(series_id)
        .fetch_one(sqlite_pool)
        .await
        .expect("failed to query sqlite series count");
    let pg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM series WHERE id = $1")
        .bind(series_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres series count");

    assert_eq!(sqlite_count, 0, "sqlite should not retain failed series");
    assert_eq!(pg_count, 0, "postgres should not retain failed series");
}

async fn assert_commit_absent_in_both(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    commit_id: &str,
) {
    let sqlite_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commits WHERE id = ?")
        .bind(commit_id)
        .fetch_one(sqlite_pool)
        .await
        .expect("failed to query sqlite commit count");
    let pg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres commit count");

    assert_eq!(sqlite_count, 0, "sqlite should not retain failed commit");
    assert_eq!(pg_count, 0, "postgres should not retain failed commit");
}

async fn assert_series_status_in_both(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    series_id: &str,
    expected_status: &str,
) {
    let sqlite_status: String = sqlx::query_scalar("SELECT status FROM series WHERE id = ?")
        .bind(series_id)
        .fetch_one(sqlite_pool)
        .await
        .expect("failed to query sqlite series status");
    let postgres_status: String = sqlx::query_scalar("SELECT status FROM series WHERE id = $1")
        .bind(series_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres series status");

    assert_eq!(sqlite_status, expected_status);
    assert_eq!(postgres_status, expected_status);
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
