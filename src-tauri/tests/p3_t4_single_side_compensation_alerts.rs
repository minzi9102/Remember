use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::{run_postgres_migrations, run_sqlite_migrations};
use tauri_app_lib::repository::{
    AppendCommitInput, ArchiveSeriesInput, CreateSeriesInput, MarkSilentSeriesInput,
    MemoRepository, RepositoryError,
};

const TEST_FAILURE_INJECTION_ENV: &str = "REMEMBER_TEST_REPOSITORY_INJECT_FAILURE";

#[tokio::test]
async fn p3_t4_single_side_compensation_writes_and_resolves_consistency_alerts() {
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t4_single_side_compensation_writes_and_resolves_consistency_alerts: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t4");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t4");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for p3-t4");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t4");

    let prefix = format!("p3t4-{}", nonce());
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());

    // Case 1: create_series fails on postgres, sqlite succeeds then compensates.
    let create_pg_failure_id = format!("{prefix}-create-pg-failure");
    set_failure_injection("postgres", "create_series", &create_pg_failure_id);
    let create_pg_error = repository
        .create_series(CreateSeriesInput {
            id: create_pg_failure_id.clone(),
            name: "Create-PG-Failure".to_string(),
            created_at: "2099-04-01T10:00:01Z".to_string(),
        })
        .await
        .expect_err("create_series should fail when postgres injection is enabled");
    clear_failure_injection();
    assert_dual_write_or_timeout(create_pg_error);
    assert_series_absent_in_both(&sqlite_pool, &postgres_pool, &create_pg_failure_id).await;
    assert_alert_resolved_in_both(
        &sqlite_pool,
        &postgres_pool,
        "create_series",
        &create_pg_failure_id,
        &prefix,
    )
    .await;
    eprintln!("p3_t4: verified postgres-side create failure alert");

    // Case 2: create_series fails on sqlite, postgres succeeds then compensates.
    let create_sqlite_failure_id = format!("{prefix}-create-sqlite-failure");
    set_failure_injection("sqlite", "create_series", &create_sqlite_failure_id);
    let create_sqlite_error = repository
        .create_series(CreateSeriesInput {
            id: create_sqlite_failure_id.clone(),
            name: "Create-SQLite-Failure".to_string(),
            created_at: "2099-04-01T10:10:01Z".to_string(),
        })
        .await
        .expect_err("create_series should fail when sqlite injection is enabled");
    clear_failure_injection();
    assert_dual_write_or_timeout(create_sqlite_error);
    assert_series_absent_in_both(&sqlite_pool, &postgres_pool, &create_sqlite_failure_id).await;
    assert_alert_resolved_in_both(
        &sqlite_pool,
        &postgres_pool,
        "create_series",
        &create_sqlite_failure_id,
        &prefix,
    )
    .await;
    eprintln!("p3_t4: verified sqlite-side create failure alert");

    let shared_series_id = format!("{prefix}-shared-series");
    repository
        .create_series(CreateSeriesInput {
            id: shared_series_id.clone(),
            name: "Shared-Series".to_string(),
            created_at: "2099-04-01T11:00:00Z".to_string(),
        })
        .await
        .expect("shared series create should succeed");

    // Case 3: append_commit fails on postgres, sqlite succeeds then compensates.
    let append_failure_commit_id = format!("{prefix}-append-pg-failure");
    set_failure_injection("postgres", "append_commit", &append_failure_commit_id);
    let append_error = repository
        .append_commit(AppendCommitInput {
            commit_id: append_failure_commit_id.clone(),
            series_id: shared_series_id.clone(),
            content: "append-pg-failure".to_string(),
            created_at: "2099-04-01T11:00:02Z".to_string(),
        })
        .await
        .expect_err("append_commit should fail when postgres injection is enabled");
    clear_failure_injection();
    assert_dual_write_or_timeout(append_error);
    assert_commit_absent_in_both(&sqlite_pool, &postgres_pool, &append_failure_commit_id).await;
    assert_alert_resolved_in_both(
        &sqlite_pool,
        &postgres_pool,
        "append_commit",
        &append_failure_commit_id,
        &prefix,
    )
    .await;
    eprintln!("p3_t4: verified append single-side alert");

    // Case 4: archive_series fails on postgres after preflight, sqlite succeeds then compensates.
    let archive_target_id = format!("{prefix}-archive-target");
    repository
        .create_series(CreateSeriesInput {
            id: archive_target_id.clone(),
            name: "Archive-Target".to_string(),
            created_at: "2099-04-01T12:00:00Z".to_string(),
        })
        .await
        .expect("archive target create should succeed");
    set_failure_injection("postgres", "archive_series", &archive_target_id);
    let archive_error = repository
        .archive_series(ArchiveSeriesInput {
            series_id: archive_target_id.clone(),
            archived_at: "2099-04-01T12:00:01Z".to_string(),
        })
        .await
        .expect_err("archive_series should fail when postgres injection is enabled");
    clear_failure_injection();
    assert_dual_write_or_timeout(archive_error);
    assert_series_status(&sqlite_pool, &archive_target_id, "active").await;
    assert_alert_resolved_in_both(
        &sqlite_pool,
        &postgres_pool,
        "archive_series",
        &archive_target_id,
        &prefix,
    )
    .await;
    eprintln!("p3_t4: verified archive single-side alert");

    // Case 5: mark_silent_series fails on postgres after id discovery, sqlite succeeds then compensates.
    let silent_target_id = format!("{prefix}-silent-target");
    repository
        .create_series(CreateSeriesInput {
            id: silent_target_id.clone(),
            name: "Silent-Target".to_string(),
            created_at: "2000-01-01T00:00:00Z".to_string(),
        })
        .await
        .expect("silent target create should succeed");
    let threshold_before = "2001-01-01T00:00:00Z".to_string();
    set_failure_injection("postgres", "mark_silent_series", &threshold_before);
    let silent_error = repository
        .mark_silent_series(MarkSilentSeriesInput {
            threshold_before: threshold_before.clone(),
        })
        .await
        .expect_err("mark_silent_series should fail when postgres injection is enabled");
    clear_failure_injection();
    assert_dual_write_or_timeout(silent_error);
    assert_series_status(&sqlite_pool, &silent_target_id, "active").await;
    let mark_silent_batch_id = format!("mark_silent:{threshold_before}:{silent_target_id}");
    assert_alert_resolved_in_both(
        &sqlite_pool,
        &postgres_pool,
        "mark_silent_series",
        &mark_silent_batch_id,
        &prefix,
    )
    .await;
    eprintln!("p3_t4: verified mark_silent single-side alert");

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

fn set_failure_injection(backend: &str, operation: &str, key: &str) {
    std::env::set_var(
        TEST_FAILURE_INJECTION_ENV,
        format!("{backend}|{operation}|{key}"),
    );
}

fn clear_failure_injection() {
    std::env::remove_var(TEST_FAILURE_INJECTION_ENV);
}

fn assert_dual_write_or_timeout(error: RepositoryError) {
    match error {
        RepositoryError::DualWriteFailed(_) | RepositoryError::PgTimeout(_) => {}
        other => panic!("expected dual_write_failed or pg_timeout, got {other:?}"),
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
    let postgres_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM series WHERE id = $1")
        .bind(series_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres series count");

    assert_eq!(sqlite_count, 0, "sqlite series should be absent");
    assert_eq!(postgres_count, 0, "postgres series should be absent");
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
    let postgres_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres commit count");

    assert_eq!(sqlite_count, 0, "sqlite commit should be absent");
    assert_eq!(postgres_count, 0, "postgres commit should be absent");
}

async fn assert_series_status(pool: &sqlx::SqlitePool, series_id: &str, expected_status: &str) {
    let status: String = sqlx::query_scalar("SELECT status FROM series WHERE id = ?")
        .bind(series_id)
        .fetch_one(pool)
        .await
        .expect("failed to query sqlite series status");
    assert_eq!(status, expected_status);
}

async fn assert_alert_resolved_in_both(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    op_type: &str,
    commit_id: &str,
    prefix: &str,
) {
    let like_pattern = format!("%{prefix}%");
    let sqlite_row: (String, Option<String>) = sqlx::query_as(
        "SELECT reason, resolved_at
         FROM consistency_alerts
         WHERE op_type = ?
           AND commit_id = ?
           AND reason LIKE ?
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(op_type)
    .bind(commit_id)
    .bind(&like_pattern)
    .fetch_one(sqlite_pool)
    .await
    .expect("failed to query sqlite consistency alert");
    assert!(
        sqlite_row.1.is_some(),
        "sqlite consistency alert should be resolved"
    );

    let postgres_row: (String, Option<String>) = sqlx::query_as(
        "SELECT
            reason,
            CASE
                WHEN resolved_at IS NULL THEN NULL
                ELSE to_char(resolved_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
            END AS resolved_at
         FROM consistency_alerts
         WHERE op_type = $1
           AND commit_id = $2
           AND reason LIKE $3
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(op_type)
    .bind(commit_id)
    .bind(&like_pattern)
    .fetch_one(postgres_pool)
    .await
    .expect("failed to query postgres consistency alert");
    assert!(
        postgres_row.1.is_some(),
        "postgres consistency alert should be resolved"
    );
}

async fn cleanup_postgres_prefix(pool: &sqlx::PgPool, prefix: &str) {
    let like_pattern = format!("{prefix}%");
    let reason_like_pattern = format!("%{prefix}%");
    let _ = sqlx::query("DELETE FROM commits WHERE series_id LIKE $1 OR id LIKE $1")
        .bind(&like_pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM series WHERE id LIKE $1")
        .bind(&like_pattern)
        .execute(pool)
        .await;
    let _ = sqlx::query(
        "DELETE FROM consistency_alerts
         WHERE commit_id LIKE $1
            OR reason LIKE $2",
    )
    .bind(&like_pattern)
    .bind(&reason_like_pattern)
    .execute(pool)
    .await;
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos()
}
