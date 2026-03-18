use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::{run_postgres_migrations, run_sqlite_migrations};
use tauri_app_lib::repository::{
    AppendCommitInput, ArchiveSeriesInput, CreateSeriesInput, MarkSilentSeriesInput,
    MemoRepository, RepositoryError,
};

const TEST_FAILURE_INJECTION_ENV: &str = "REMEMBER_TEST_REPOSITORY_INJECT_FAILURE";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeriesState {
    status: String,
    latest_excerpt: String,
    last_updated_at: String,
    archived_at: Option<String>,
}

#[tokio::test]
async fn p3_t5_startup_self_heal_repairs_unresolved_dual_sync_alerts() {
    let _test_guard = p3_t5_test_lock().lock().await;
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t5_startup_self_heal_repairs_unresolved_dual_sync_alerts: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t5");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t5");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for p3-t5");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t5");

    let prefix = format!("p3t5-{}", nonce());
    cleanup_all_p3_t5_postgres_artifacts(&postgres_pool).await;
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());

    let create_series_id = format!("{prefix}-create-series");
    let create_alert_reason = format!("series_id={create_series_id}");
    set_failure_injection("sqlite", "rollback_create_series", &create_series_id);
    let mut create_lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to begin postgres lock tx for create_series");
    sqlx::query("LOCK TABLE series IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *create_lock_tx)
        .await
        .expect("failed to lock postgres series table for create_series");
    let create_error = repository
        .create_series(CreateSeriesInput {
            id: create_series_id.clone(),
            name: "Create-Series".to_string(),
            created_at: "2099-05-01T09:00:00Z".to_string(),
        })
        .await
        .expect_err("create_series should fail when rollback compensation is injected");
    clear_failure_injection();
    create_lock_tx
        .rollback()
        .await
        .expect("failed to rollback create_series lock tx");
    assert!(matches!(create_error, RepositoryError::DualWriteFailed(_)));
    assert_series_present_only_in_sqlite(&sqlite_pool, &postgres_pool, &create_series_id).await;
    assert_unresolved_alert(
        &sqlite_pool,
        &postgres_pool,
        "create_series",
        &create_series_id,
        &create_alert_reason,
    )
    .await;

    let shared_series_id = format!("{prefix}-shared-series");
    repository
        .create_series(CreateSeriesInput {
            id: shared_series_id.clone(),
            name: "Shared-Series".to_string(),
            created_at: "2099-05-01T10:00:00Z".to_string(),
        })
        .await
        .expect("shared series create should succeed");
    let shared_before = load_series_state_sqlite(&sqlite_pool, &shared_series_id).await;

    let append_commit_id = format!("{prefix}-append-commit");
    let append_alert_reason = format!("series_id={shared_series_id}, commit_id={append_commit_id}");
    set_failure_injection("sqlite", "rollback_append_commit", &append_commit_id);
    let mut append_lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to begin postgres lock tx for append_commit");
    sqlx::query("LOCK TABLE commits IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *append_lock_tx)
        .await
        .expect("failed to lock postgres commits table for append_commit");
    let append_error = repository
        .append_commit(AppendCommitInput {
            commit_id: append_commit_id.clone(),
            series_id: shared_series_id.clone(),
            content: "append-after-failure".to_string(),
            created_at: "2099-05-01T10:00:01Z".to_string(),
        })
        .await
        .expect_err("append_commit should fail when rollback compensation is injected");
    clear_failure_injection();
    append_lock_tx
        .rollback()
        .await
        .expect("failed to rollback append_commit lock tx");
    assert!(matches!(append_error, RepositoryError::DualWriteFailed(_)));
    assert_commit_present_only_in_sqlite(&sqlite_pool, &postgres_pool, &append_commit_id).await;
    assert_unresolved_alert(
        &sqlite_pool,
        &postgres_pool,
        "append_commit",
        &append_commit_id,
        &append_alert_reason,
    )
    .await;

    let archive_series_id = format!("{prefix}-archive-series");
    repository
        .create_series(CreateSeriesInput {
            id: archive_series_id.clone(),
            name: "Archive-Series".to_string(),
            created_at: "2099-05-01T11:00:00Z".to_string(),
        })
        .await
        .expect("archive series create should succeed");
    let archive_before = load_series_state_sqlite(&sqlite_pool, &archive_series_id).await;
    let archive_alert_reason = format!("series_id={archive_series_id}");
    set_failure_injections(&[
        ("sqlite", "restore_series_snapshot", &archive_series_id),
        ("postgres", "archive_series", &archive_series_id),
    ]);
    let archive_error = repository
        .archive_series(ArchiveSeriesInput {
            series_id: archive_series_id.clone(),
            archived_at: "2099-05-01T11:00:01Z".to_string(),
        })
        .await
        .expect_err("archive_series should fail when rollback compensation is injected");
    clear_failure_injection();
    assert!(matches!(archive_error, RepositoryError::DualWriteFailed(_)));
    assert_series_statuses(
        &sqlite_pool,
        &postgres_pool,
        &archive_series_id,
        "archived",
        "active",
    )
    .await;
    assert_unresolved_alert(
        &sqlite_pool,
        &postgres_pool,
        "archive_series",
        &archive_series_id,
        &archive_alert_reason,
    )
    .await;

    let silent_series_id = format!("{prefix}-silent-series");
    repository
        .create_series(CreateSeriesInput {
            id: silent_series_id.clone(),
            name: "Silent-Series".to_string(),
            created_at: "2020-01-01T00:00:00Z".to_string(),
        })
        .await
        .expect("silent series create should succeed");
    let silent_before = load_series_state_sqlite(&sqlite_pool, &silent_series_id).await;
    let threshold_before = "2099-05-01T00:00:00Z".to_string();
    let mark_silent_batch_id = format!("mark_silent:{threshold_before}:{silent_series_id}");
    let mark_silent_alert_reason =
        format!("threshold_before={threshold_before}, affected_series_ids={silent_series_id}");
    set_failure_injection("sqlite", "rollback_mark_silent_series", &silent_series_id);
    let mut mark_silent_lock_tx = postgres_pool
        .begin()
        .await
        .expect("failed to begin postgres lock tx for mark_silent_series");
    sqlx::query("LOCK TABLE series IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *mark_silent_lock_tx)
        .await
        .expect("failed to lock postgres series table for mark_silent_series");
    let mark_silent_error = repository
        .mark_silent_series(MarkSilentSeriesInput {
            threshold_before: threshold_before.clone(),
        })
        .await
        .expect_err("mark_silent_series should fail when rollback compensation is injected");
    clear_failure_injection();
    mark_silent_lock_tx
        .rollback()
        .await
        .expect("failed to rollback mark_silent_series lock tx");
    assert!(matches!(
        mark_silent_error,
        RepositoryError::DualWriteFailed(_)
    ));
    assert_series_statuses(
        &sqlite_pool,
        &postgres_pool,
        &silent_series_id,
        "silent",
        "active",
    )
    .await;
    assert_unresolved_alert(
        &sqlite_pool,
        &postgres_pool,
        "mark_silent_series",
        &mark_silent_batch_id,
        &mark_silent_alert_reason,
    )
    .await;

    let restarted_repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());
    let summary = restarted_repository.run_startup_self_heal().await;

    assert_eq!(summary.scanned_alerts, 4);
    assert_eq!(summary.repaired_alerts, 4);
    assert_eq!(summary.unresolved_alerts, 0);
    assert_eq!(summary.failed_alerts, 0);
    assert!(summary.messages.is_empty());
    assert!(
        !summary.completed_at.trim().is_empty(),
        "completed_at should be populated"
    );

    assert_series_absent_in_both(&sqlite_pool, &postgres_pool, &create_series_id).await;
    assert_commit_absent_in_both(&sqlite_pool, &postgres_pool, &append_commit_id).await;
    assert_series_state_matches(
        &sqlite_pool,
        &postgres_pool,
        &shared_series_id,
        &shared_before,
    )
    .await;
    assert_series_state_matches(
        &sqlite_pool,
        &postgres_pool,
        &archive_series_id,
        &archive_before,
    )
    .await;
    assert_series_state_matches(
        &sqlite_pool,
        &postgres_pool,
        &silent_series_id,
        &silent_before,
    )
    .await;
    assert_no_unresolved_alerts(&sqlite_pool, &postgres_pool, &prefix).await;

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

#[tokio::test]
async fn p3_t5_startup_self_heal_closes_alerts_when_target_state_is_already_consistent() {
    let _test_guard = p3_t5_test_lock().lock().await;
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t5_startup_self_heal_closes_alerts_when_target_state_is_already_consistent: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for idempotent p3-t5");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for idempotent p3-t5");

    let postgres_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect postgres for idempotent p3-t5");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for idempotent p3-t5");

    let prefix = format!("p3t5-idempotent-{}", nonce());
    cleanup_all_p3_t5_postgres_artifacts(&postgres_pool).await;
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let alert_id = format!("{prefix}-alert");
    let series_id = format!("{prefix}-series");
    let reason = format!(
        "dual_sync create_series single-side success; succeeded_side=sqlite; failed_side=postgres; operation_error=simulated; detail=series_id={series_id}"
    );
    insert_unresolved_alert(
        &sqlite_pool,
        &postgres_pool,
        &alert_id,
        "create_series",
        &series_id,
        &reason,
    )
    .await;

    let repository =
        repository::DualSyncRepository::new(sqlite_pool.clone(), postgres_pool.clone());
    let summary = repository.run_startup_self_heal().await;

    assert_eq!(summary.scanned_alerts, 1);
    assert_eq!(summary.repaired_alerts, 1);
    assert_eq!(summary.unresolved_alerts, 0);
    assert_eq!(summary.failed_alerts, 0);
    assert_no_unresolved_alerts(&sqlite_pool, &postgres_pool, &prefix).await;

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

fn set_failure_injection(backend: &str, operation: &str, key: &str) {
    std::env::set_var(
        TEST_FAILURE_INJECTION_ENV,
        format!("{backend}|{operation}|{key}"),
    );
}

fn set_failure_injections(rules: &[(&str, &str, &str)]) {
    let raw = rules
        .iter()
        .map(|(backend, operation, key)| format!("{backend}|{operation}|{key}"))
        .collect::<Vec<_>>()
        .join(";");
    std::env::set_var(TEST_FAILURE_INJECTION_ENV, raw);
}

fn clear_failure_injection() {
    std::env::remove_var(TEST_FAILURE_INJECTION_ENV);
}

async fn insert_unresolved_alert(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    alert_id: &str,
    op_type: &str,
    commit_id: &str,
    reason: &str,
) {
    let created_at = "2099-05-01T00:00:00Z";
    sqlx::query(
        "INSERT INTO consistency_alerts (id, op_type, commit_id, reason, created_at, resolved_at)
         VALUES (?, ?, ?, ?, ?, NULL)",
    )
    .bind(alert_id)
    .bind(op_type)
    .bind(commit_id)
    .bind(reason)
    .bind(created_at)
    .execute(sqlite_pool)
    .await
    .expect("failed to insert sqlite unresolved alert");

    sqlx::query(
        "INSERT INTO consistency_alerts (id, op_type, commit_id, reason, created_at, resolved_at)
         VALUES ($1, $2, $3, $4, $5::timestamptz, NULL)",
    )
    .bind(alert_id)
    .bind(op_type)
    .bind(commit_id)
    .bind(reason)
    .bind(created_at)
    .execute(postgres_pool)
    .await
    .expect("failed to insert postgres unresolved alert");
}

async fn assert_unresolved_alert(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    op_type: &str,
    commit_id: &str,
    reason_snippet: &str,
) {
    let sqlite_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM consistency_alerts
         WHERE op_type = ?
           AND commit_id = ?
           AND resolved_at IS NULL
           AND reason LIKE ?",
    )
    .bind(op_type)
    .bind(commit_id)
    .bind(format!("%{reason_snippet}%"))
    .fetch_one(sqlite_pool)
    .await
    .expect("failed to query sqlite unresolved alert");
    let postgres_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM consistency_alerts
         WHERE op_type = $1
           AND commit_id = $2
           AND resolved_at IS NULL
           AND reason LIKE $3",
    )
    .bind(op_type)
    .bind(commit_id)
    .bind(format!("%{reason_snippet}%"))
    .fetch_one(postgres_pool)
    .await
    .expect("failed to query postgres unresolved alert");

    assert_eq!(sqlite_count, 1, "sqlite unresolved alert should exist");
    assert_eq!(postgres_count, 1, "postgres unresolved alert should exist");
}

async fn assert_no_unresolved_alerts(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    prefix: &str,
) {
    let sqlite_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM consistency_alerts
         WHERE resolved_at IS NULL
           AND (commit_id LIKE ? OR reason LIKE ?)",
    )
    .bind(format!("{prefix}%"))
    .bind(format!("%{prefix}%"))
    .fetch_one(sqlite_pool)
    .await
    .expect("failed to query sqlite unresolved alert count");
    let postgres_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM consistency_alerts
         WHERE resolved_at IS NULL
           AND (commit_id LIKE $1 OR reason LIKE $2)",
    )
    .bind(format!("{prefix}%"))
    .bind(format!("%{prefix}%"))
    .fetch_one(postgres_pool)
    .await
    .expect("failed to query postgres unresolved alert count");

    assert_eq!(sqlite_count, 0, "sqlite unresolved alerts should be closed");
    assert_eq!(
        postgres_count, 0,
        "postgres unresolved alerts should be closed"
    );
}

async fn assert_series_present_only_in_sqlite(
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

    assert_eq!(sqlite_count, 1, "sqlite series should exist");
    assert_eq!(postgres_count, 0, "postgres series should be absent");
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

async fn assert_commit_present_only_in_sqlite(
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

    assert_eq!(sqlite_count, 1, "sqlite commit should exist");
    assert_eq!(postgres_count, 0, "postgres commit should be absent");
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

async fn assert_series_statuses(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    series_id: &str,
    sqlite_status: &str,
    postgres_status: &str,
) {
    let sqlite_value: String = sqlx::query_scalar("SELECT status FROM series WHERE id = ?")
        .bind(series_id)
        .fetch_one(sqlite_pool)
        .await
        .expect("failed to query sqlite series status");
    let postgres_value: String = sqlx::query_scalar("SELECT status FROM series WHERE id = $1")
        .bind(series_id)
        .fetch_one(postgres_pool)
        .await
        .expect("failed to query postgres series status");

    assert_eq!(sqlite_value, sqlite_status, "sqlite status mismatch");
    assert_eq!(postgres_value, postgres_status, "postgres status mismatch");
}

async fn load_series_state_sqlite(sqlite_pool: &sqlx::SqlitePool, series_id: &str) -> SeriesState {
    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT status, latest_excerpt, last_updated_at, archived_at
         FROM series
         WHERE id = ?",
    )
    .bind(series_id)
    .fetch_one(sqlite_pool)
    .await
    .expect("failed to query sqlite series state");

    SeriesState {
        status: row.0,
        latest_excerpt: row.1,
        last_updated_at: row.2,
        archived_at: row.3,
    }
}

async fn load_series_state_postgres(postgres_pool: &sqlx::PgPool, series_id: &str) -> SeriesState {
    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT
            status,
            latest_excerpt,
            to_char(last_updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_updated_at,
            CASE
                WHEN archived_at IS NULL THEN NULL
                ELSE to_char(archived_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
            END AS archived_at
         FROM series
         WHERE id = $1",
    )
    .bind(series_id)
    .fetch_one(postgres_pool)
    .await
    .expect("failed to query postgres series state");

    SeriesState {
        status: row.0,
        latest_excerpt: row.1,
        last_updated_at: row.2,
        archived_at: row.3,
    }
}

async fn assert_series_state_matches(
    sqlite_pool: &sqlx::SqlitePool,
    postgres_pool: &sqlx::PgPool,
    series_id: &str,
    expected: &SeriesState,
) {
    let sqlite_state = load_series_state_sqlite(sqlite_pool, series_id).await;
    let postgres_state = load_series_state_postgres(postgres_pool, series_id).await;

    assert_eq!(
        &sqlite_state, expected,
        "sqlite state should match expected"
    );
    assert_eq!(
        &postgres_state, expected,
        "postgres state should match expected"
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
            OR reason LIKE $2
            OR id LIKE $1",
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

fn p3_t5_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn cleanup_all_p3_t5_postgres_artifacts(pool: &sqlx::PgPool) {
    cleanup_postgres_prefix(pool, "p3t5-").await;
    cleanup_postgres_prefix(pool, "p3t5-idempotent-").await;
}
