use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    sqlite::SqlitePoolOptions,
};
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::{run_postgres_migrations, run_sqlite_migrations};
use tauri_app_lib::repository::{
    AppendCommitInput, CreateSeriesInput, MemoRepository, RepositoryError,
    POSTGRES_APPLICATION_NAME, POSTGRES_LOCK_TIMEOUT, POSTGRES_STATEMENT_TIMEOUT,
};

#[tokio::test]
async fn p3_t2_dual_sync_enforces_postgres_statement_timeout_and_recovers() {
    let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => {
            eprintln!(
                "skip p3_t2_dual_sync_enforces_postgres_statement_timeout_and_recovers: REMEMBER_TEST_POSTGRES_DSN is not configured"
            );
            return;
        }
    };

    let sqlite_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p3-t2");
    run_sqlite_migrations(&sqlite_pool)
        .await
        .expect("failed to run sqlite migrations for p3-t2");

    let postgres_options: PgConnectOptions = postgres_dsn
        .parse()
        .expect("failed to parse postgres dsn for p3-t2");
    let postgres_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(
            postgres_options
                .application_name(POSTGRES_APPLICATION_NAME)
                .options([
                    ("statement_timeout", POSTGRES_STATEMENT_TIMEOUT),
                    ("lock_timeout", POSTGRES_LOCK_TIMEOUT),
                ]),
        )
        .await
        .expect("failed to connect postgres for p3-t2");
    run_postgres_migrations(&postgres_pool)
        .await
        .expect("failed to run postgres migrations for p3-t2");
    let lock_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&postgres_dsn)
        .await
        .expect("failed to connect lock postgres pool for p3-t2");

    let prefix = format!("p3t2-timeout-{}", nonce());
    cleanup_postgres_prefix(&postgres_pool, &prefix).await;

    let repository = repository::DualSyncRepository::new(sqlite_pool, postgres_pool.clone());
    let series_id = format!("{prefix}-series");
    repository
        .create_series(CreateSeriesInput {
            id: series_id.clone(),
            name: "Timeout-Probe".to_string(),
            created_at: "2099-03-18T09:00:00Z".to_string(),
        })
        .await
        .expect("dual create_series should succeed before timeout probe");
    assert_postgres_session_timeouts(&postgres_pool).await;

    let mut lock_tx = lock_pool
        .begin()
        .await
        .expect("failed to start postgres lock transaction");
    sqlx::query("LOCK TABLE commits IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock_tx)
        .await
        .expect("failed to lock commits table for timeout probe");

    let started = Instant::now();
    let error = repository
        .append_commit(AppendCommitInput {
            commit_id: format!("{prefix}-blocked-commit"),
            series_id: series_id.clone(),
            content: "blocked-write".to_string(),
            created_at: "2099-03-18T09:00:01Z".to_string(),
        })
        .await
        .expect_err("append_commit should fail when postgres write is locked past timeout");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(2500),
        "expected timeout close to 3s and not too early, got {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(4500),
        "expected timeout close to 3s (<=4.5s), got {elapsed:?}"
    );
    assert_timeout_error(error);

    lock_tx
        .rollback()
        .await
        .expect("failed to rollback lock transaction");

    let recovered = repository
        .append_commit(AppendCommitInput {
            commit_id: format!("{prefix}-recovered-commit"),
            series_id: series_id.clone(),
            content: "recovered-write".to_string(),
            created_at: "2099-03-18T09:00:02Z".to_string(),
        })
        .await
        .expect("append_commit should recover after lock is released");

    assert_eq!(recovered.commit.series_id, series_id);
    assert_eq!(recovered.commit.content, "recovered-write");

    cleanup_postgres_prefix(&postgres_pool, &prefix).await;
}

async fn assert_postgres_session_timeouts(pool: &sqlx::PgPool) {
    let statement_timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(pool)
        .await
        .expect("failed to read postgres statement_timeout for p3-t2");
    assert_eq!(statement_timeout, POSTGRES_STATEMENT_TIMEOUT);

    let lock_timeout: String = sqlx::query_scalar("SHOW lock_timeout")
        .fetch_one(pool)
        .await
        .expect("failed to read postgres lock_timeout for p3-t2");
    assert_eq!(lock_timeout, POSTGRES_LOCK_TIMEOUT);
}

fn assert_timeout_error(error: RepositoryError) {
    let message = match error {
        RepositoryError::PgTimeout(message) | RepositoryError::Storage(message) => message,
        other => panic!("expected pg timeout/storage error for timeout, got {other:?}"),
    };
    let normalized = message.to_ascii_lowercase();
    assert!(
        normalized.contains("statement timeout")
            || normalized.contains("lock timeout")
            || normalized.contains("query_canceled")
            || normalized.contains("57014")
            || normalized.contains("55p03"),
        "expected postgres timeout signal in error message, got `{message}`"
    );
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
