#![allow(dead_code)]

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use sqlx::{postgres::PgPool, sqlite::SqlitePool};
use uuid::Uuid;

use super::{
    map_sqlx_error, AppendCommitInput, AppendCommitResult, ArchiveSeriesInput, ArchiveSeriesResult,
    CreateSeriesInput, ListSeriesQuery, MarkSilentSeriesInput, MarkSilentSeriesResult,
    MemoRepository, PagedResult, PostgresRepository, RepositoryError, SearchSeriesQuery,
    SeriesRecord, SqliteRepository, TimelineQuery,
};

const POSTGRES_SNAPSHOT_TIMEOUT_SQL: &str = "SET LOCAL statement_timeout = '3s'";

#[derive(Debug, Clone)]
pub struct DualSyncRepository {
    sqlite: SqliteRepository,
    postgres: PostgresRepository,
}

#[derive(Debug, Clone)]
struct SeriesSnapshot {
    status: String,
    latest_excerpt: String,
    last_updated_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendSide {
    Sqlite,
    Postgres,
}

impl BackendSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
        }
    }
}

impl DualSyncRepository {
    pub fn new(sqlite_pool: SqlitePool, postgres_pool: PgPool) -> Self {
        Self {
            sqlite: SqliteRepository::new(sqlite_pool),
            postgres: PostgresRepository::new(postgres_pool),
        }
    }

    async fn load_sqlite_series_snapshot(
        &self,
        series_id: &str,
    ) -> Result<SeriesSnapshot, RepositoryError> {
        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT status, latest_excerpt, last_updated_at, archived_at
             FROM series
             WHERE id = ?",
        )
        .bind(series_id)
        .fetch_optional(self.sqlite.pool())
        .await
        .map_err(map_sqlx_error)?;

        let Some((status, latest_excerpt, last_updated_at, archived_at)) = row else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist in sqlite"
            )));
        };

        Ok(SeriesSnapshot {
            status,
            latest_excerpt,
            last_updated_at,
            archived_at,
        })
    }

    async fn load_postgres_series_snapshot(
        &self,
        series_id: &str,
    ) -> Result<SeriesSnapshot, RepositoryError> {
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        sqlx::query(POSTGRES_SNAPSHOT_TIMEOUT_SQL)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
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
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let Some((status, latest_excerpt, last_updated_at, archived_at)) = row else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist in postgres"
            )));
        };

        Ok(SeriesSnapshot {
            status,
            latest_excerpt,
            last_updated_at,
            archived_at,
        })
    }

    async fn rollback_sqlite_create_series(&self, series_id: &str) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(series_id)
            .execute(self.sqlite.pool())
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn rollback_postgres_create_series(
        &self,
        series_id: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM series WHERE id = $1")
            .bind(series_id)
            .execute(self.postgres.pool())
            .await
            .map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn restore_sqlite_series_snapshot(
        &self,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE series
             SET status = ?,
                 latest_excerpt = ?,
                 last_updated_at = ?,
                 archived_at = ?
             WHERE id = ?",
        )
        .bind(&snapshot.status)
        .bind(&snapshot.latest_excerpt)
        .bind(&snapshot.last_updated_at)
        .bind(snapshot.archived_at.as_deref())
        .bind(series_id)
        .execute(self.sqlite.pool())
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn restore_postgres_series_snapshot(
        &self,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE series
             SET status = $1,
                 latest_excerpt = $2,
                 last_updated_at = $3::timestamptz,
                 archived_at = $4::timestamptz
             WHERE id = $5",
        )
        .bind(&snapshot.status)
        .bind(&snapshot.latest_excerpt)
        .bind(&snapshot.last_updated_at)
        .bind(snapshot.archived_at.as_deref())
        .bind(series_id)
        .execute(self.postgres.pool())
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn rollback_sqlite_append_commit(
        &self,
        commit_id: &str,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.sqlite.pool().begin().await.map_err(map_sqlx_error)?;

        sqlx::query("DELETE FROM commits WHERE id = ?")
            .bind(commit_id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        sqlx::query(
            "UPDATE series
             SET status = ?,
                 latest_excerpt = ?,
                 last_updated_at = ?,
                 archived_at = ?
             WHERE id = ?",
        )
        .bind(&snapshot.status)
        .bind(&snapshot.latest_excerpt)
        .bind(&snapshot.last_updated_at)
        .bind(snapshot.archived_at.as_deref())
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn rollback_postgres_append_commit(
        &self,
        commit_id: &str,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;

        sqlx::query("DELETE FROM commits WHERE id = $1")
            .bind(commit_id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        sqlx::query(
            "UPDATE series
             SET status = $1,
                 latest_excerpt = $2,
                 last_updated_at = $3::timestamptz,
                 archived_at = $4::timestamptz
             WHERE id = $5",
        )
        .bind(&snapshot.status)
        .bind(&snapshot.latest_excerpt)
        .bind(&snapshot.last_updated_at)
        .bind(snapshot.archived_at.as_deref())
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn rollback_sqlite_mark_silent_series(
        &self,
        affected_series_ids: &[String],
    ) -> Result<(), RepositoryError> {
        if affected_series_ids.is_empty() {
            return Ok(());
        }

        let mut tx = self.sqlite.pool().begin().await.map_err(map_sqlx_error)?;
        for series_id in affected_series_ids {
            sqlx::query("UPDATE series SET status = 'active' WHERE id = ?")
                .bind(series_id)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_error)?;
        }
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn rollback_postgres_mark_silent_series(
        &self,
        affected_series_ids: &[String],
    ) -> Result<(), RepositoryError> {
        if affected_series_ids.is_empty() {
            return Ok(());
        }

        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        for series_id in affected_series_ids {
            sqlx::query("UPDATE series SET status = 'active' WHERE id = $1")
                .bind(series_id)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_error)?;
        }
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn write_consistency_alert(
        &self,
        op_type: &str,
        commit_id: &str,
        reason: &str,
    ) -> Option<String> {
        let alert_id = Uuid::now_v7().to_string();
        let created_at = now_utc_rfc3339_seconds();

        let sqlite_result = sqlx::query(
            "INSERT INTO consistency_alerts (id, op_type, commit_id, reason, created_at, resolved_at)
             VALUES (?, ?, ?, ?, ?, NULL)",
        )
        .bind(&alert_id)
        .bind(op_type)
        .bind(commit_id)
        .bind(reason)
        .bind(&created_at)
        .execute(self.sqlite.pool())
        .await
        .map_err(map_sqlx_error);

        if let Err(error) = &sqlite_result {
            tracing::error!(
                component = "repository",
                operation = op_type,
                backend = "sqlite",
                error = %error,
                "failed to persist consistency alert"
            );
        }

        let postgres_result = sqlx::query(
            "INSERT INTO consistency_alerts (id, op_type, commit_id, reason, created_at, resolved_at)
             VALUES ($1, $2, $3, $4, $5::timestamptz, NULL)",
        )
        .bind(&alert_id)
        .bind(op_type)
        .bind(commit_id)
        .bind(reason)
        .bind(&created_at)
        .execute(self.postgres.pool())
        .await
        .map_err(map_sqlx_error);

        if let Err(error) = &postgres_result {
            tracing::error!(
                component = "repository",
                operation = op_type,
                backend = "postgres",
                error = %error,
                "failed to persist consistency alert"
            );
        }

        if sqlite_result.is_err() && postgres_result.is_err() {
            tracing::warn!(
                component = "repository",
                operation = op_type,
                commit_id,
                "consistency alert was not persisted on either backend"
            );
            None
        } else {
            Some(alert_id)
        }
    }

    async fn resolve_consistency_alert(&self, alert_id: &str, op_type: &str) {
        let resolved_at = now_utc_rfc3339_seconds();

        let sqlite_result = sqlx::query(
            "UPDATE consistency_alerts
             SET resolved_at = ?
             WHERE id = ?
               AND resolved_at IS NULL",
        )
        .bind(&resolved_at)
        .bind(alert_id)
        .execute(self.sqlite.pool())
        .await
        .map_err(map_sqlx_error);

        if let Err(error) = sqlite_result {
            tracing::error!(
                component = "repository",
                operation = op_type,
                backend = "sqlite",
                alert_id,
                error = %error,
                "failed to resolve consistency alert"
            );
        }

        let postgres_result = sqlx::query(
            "UPDATE consistency_alerts
             SET resolved_at = $1::timestamptz
             WHERE id = $2
               AND resolved_at IS NULL",
        )
        .bind(&resolved_at)
        .bind(alert_id)
        .execute(self.postgres.pool())
        .await
        .map_err(map_sqlx_error);

        if let Err(error) = postgres_result {
            tracing::error!(
                component = "repository",
                operation = op_type,
                backend = "postgres",
                alert_id,
                error = %error,
                "failed to resolve consistency alert"
            );
        }
    }
}

fn is_pg_timeout_error(error: &RepositoryError) -> bool {
    match error {
        RepositoryError::PgTimeout(_) => true,
        RepositoryError::Storage(message) | RepositoryError::DualWriteFailed(message) => {
            let normalized = message.to_ascii_lowercase();
            normalized.contains("statement timeout")
                || normalized.contains("query_canceled")
                || normalized.contains("57014")
                || normalized.contains("canceling statement")
        }
        _ => false,
    }
}

fn build_dual_failure(
    operation: &str,
    sqlite_error: Option<&RepositoryError>,
    postgres_error: Option<&RepositoryError>,
) -> RepositoryError {
    let sqlite_summary = sqlite_error
        .map(ToString::to_string)
        .unwrap_or_else(|| "none".to_string());
    let postgres_summary = postgres_error
        .map(ToString::to_string)
        .unwrap_or_else(|| "none".to_string());
    let has_pg_timeout = sqlite_error.map(is_pg_timeout_error).unwrap_or(false)
        || postgres_error.map(is_pg_timeout_error).unwrap_or(false);

    if has_pg_timeout {
        RepositoryError::pg_timeout(format!(
            "dual_sync {operation} failed because postgres timed out; sqlite_error={sqlite_summary}; postgres_error={postgres_summary}"
        ))
    } else {
        RepositoryError::dual_write_failed(format!(
            "dual_sync {operation} failed; sqlite_error={sqlite_summary}; postgres_error={postgres_summary}"
        ))
    }
}

fn build_single_side_failure(
    operation: &str,
    failed_side: BackendSide,
    operation_error: &RepositoryError,
    compensation_error: Option<&RepositoryError>,
) -> RepositoryError {
    if let Some(compensation_error) = compensation_error {
        return RepositoryError::dual_write_failed(format!(
            "dual_sync {operation} failed on {} and compensation failed; operation_error={operation_error}; compensation_error={compensation_error}",
            failed_side.as_str(),
        ));
    }

    let message = format!(
        "dual_sync {operation} failed on {} and compensation succeeded; operation_error={operation_error}",
        failed_side.as_str(),
    );
    if failed_side == BackendSide::Postgres && is_pg_timeout_error(operation_error) {
        RepositoryError::pg_timeout(message)
    } else if is_pg_timeout_error(operation_error) {
        RepositoryError::pg_timeout(message)
    } else {
        RepositoryError::dual_write_failed(message)
    }
}

fn now_utc_rfc3339_seconds() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn build_mark_silent_batch_id(threshold_before: &str, affected_series_ids: &[String]) -> String {
    let mut ids = affected_series_ids.to_vec();
    ids.sort();
    format!("mark_silent:{threshold_before}:{}", ids.join(","))
}

fn format_affected_series_ids(affected_series_ids: &[String]) -> String {
    if affected_series_ids.is_empty() {
        "<none>".to_string()
    } else {
        affected_series_ids.join(",")
    }
}

fn build_consistency_alert_reason(
    operation: &str,
    succeeded_side: BackendSide,
    failed_side: BackendSide,
    operation_error: &RepositoryError,
    detail: &str,
) -> String {
    format!(
        "dual_sync {operation} single-side success; succeeded_side={}; failed_side={}; operation_error={operation_error}; detail={detail}",
        succeeded_side.as_str(),
        failed_side.as_str(),
    )
}

#[async_trait]
impl MemoRepository for DualSyncRepository {
    async fn create_series(
        &self,
        input: CreateSeriesInput,
    ) -> Result<SeriesRecord, RepositoryError> {
        let series_id = input.id.clone();
        let sqlite_future = self.sqlite.create_series(input.clone());
        let postgres_future = self.postgres.create_series(input);
        let (sqlite_result, postgres_result) = tokio::join!(sqlite_future, postgres_future);

        match (sqlite_result, postgres_result) {
            (Ok(sqlite_record), Ok(postgres_record)) => {
                if sqlite_record.id != postgres_record.id
                    || sqlite_record.created_at != postgres_record.created_at
                {
                    return Err(RepositoryError::storage(
                        "dual_sync create_series produced inconsistent id/created_at between sqlite and postgres",
                    ));
                }

                Ok(sqlite_record)
            }
            (Err(sqlite_error), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "create_series",
                    sqlite_error = %sqlite_error,
                    postgres_error = %postgres_error,
                    "dual_sync write failed on both backends"
                );
                Err(build_dual_failure(
                    "create_series",
                    Some(&sqlite_error),
                    Some(&postgres_error),
                ))
            }
            (Ok(_), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "create_series",
                    succeeded_side = "sqlite",
                    failed_side = "postgres",
                    error = %postgres_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "create_series",
                    BackendSide::Sqlite,
                    BackendSide::Postgres,
                    &postgres_error,
                    &format!("series_id={series_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("create_series", &series_id, &alert_reason)
                    .await;
                let compensation_result = self.rollback_sqlite_create_series(&series_id).await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "create_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "create_series",
                            rollback_side = "sqlite",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "create_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "create_series",
                            rollback_side = "sqlite",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "create_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
            (Err(sqlite_error), Ok(_)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "create_series",
                    succeeded_side = "postgres",
                    failed_side = "sqlite",
                    error = %sqlite_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "create_series",
                    BackendSide::Postgres,
                    BackendSide::Sqlite,
                    &sqlite_error,
                    &format!("series_id={series_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("create_series", &series_id, &alert_reason)
                    .await;
                let compensation_result = self.rollback_postgres_create_series(&series_id).await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "create_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "create_series",
                            rollback_side = "postgres",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "create_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "create_series",
                            rollback_side = "postgres",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "create_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
        }
    }

    async fn list_series(
        &self,
        query: ListSeriesQuery,
    ) -> Result<PagedResult<SeriesRecord>, RepositoryError> {
        self.sqlite.list_series(query).await
    }

    async fn append_commit(
        &self,
        input: AppendCommitInput,
    ) -> Result<AppendCommitResult, RepositoryError> {
        let series_id = input.series_id.clone();
        let commit_id = input.commit_id.clone();
        let sqlite_snapshot_future = self.load_sqlite_series_snapshot(&series_id);
        let postgres_snapshot_future = self.load_postgres_series_snapshot(&series_id);
        let (sqlite_snapshot_result, postgres_snapshot_result) =
            tokio::join!(sqlite_snapshot_future, postgres_snapshot_future);
        let sqlite_snapshot = sqlite_snapshot_result?;
        let postgres_snapshot = postgres_snapshot_result?;

        let sqlite_future = self.sqlite.append_commit(input.clone());
        let postgres_future = self.postgres.append_commit(input);
        let (sqlite_result, postgres_result) = tokio::join!(sqlite_future, postgres_future);

        match (sqlite_result, postgres_result) {
            (Ok(sqlite_result), Ok(postgres_result)) => {
                if sqlite_result.commit.id != postgres_result.commit.id
                    || sqlite_result.commit.created_at != postgres_result.commit.created_at
                {
                    return Err(RepositoryError::storage(
                        "dual_sync append_commit produced inconsistent commit_id/created_at between sqlite and postgres",
                    ));
                }

                Ok(sqlite_result)
            }
            (Err(sqlite_error), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "append_commit",
                    sqlite_error = %sqlite_error,
                    postgres_error = %postgres_error,
                    "dual_sync write failed on both backends"
                );
                Err(build_dual_failure(
                    "append_commit",
                    Some(&sqlite_error),
                    Some(&postgres_error),
                ))
            }
            (Ok(_), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "append_commit",
                    succeeded_side = "sqlite",
                    failed_side = "postgres",
                    error = %postgres_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "append_commit",
                    BackendSide::Sqlite,
                    BackendSide::Postgres,
                    &postgres_error,
                    &format!("series_id={series_id}, commit_id={commit_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("append_commit", &commit_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .rollback_sqlite_append_commit(&commit_id, &series_id, &sqlite_snapshot)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "append_commit")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "append_commit",
                            rollback_side = "sqlite",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "append_commit",
                            BackendSide::Postgres,
                            &postgres_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "append_commit",
                            rollback_side = "sqlite",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "append_commit",
                            BackendSide::Postgres,
                            &postgres_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
            (Err(sqlite_error), Ok(_)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "append_commit",
                    succeeded_side = "postgres",
                    failed_side = "sqlite",
                    error = %sqlite_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "append_commit",
                    BackendSide::Postgres,
                    BackendSide::Sqlite,
                    &sqlite_error,
                    &format!("series_id={series_id}, commit_id={commit_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("append_commit", &commit_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .rollback_postgres_append_commit(&commit_id, &series_id, &postgres_snapshot)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "append_commit")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "append_commit",
                            rollback_side = "postgres",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "append_commit",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "append_commit",
                            rollback_side = "postgres",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "append_commit",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
        }
    }

    async fn list_timeline(
        &self,
        query: TimelineQuery,
    ) -> Result<PagedResult<super::CommitRecord>, RepositoryError> {
        self.sqlite.list_timeline(query).await
    }

    async fn archive_series(
        &self,
        input: ArchiveSeriesInput,
    ) -> Result<ArchiveSeriesResult, RepositoryError> {
        let series_id = input.series_id.clone();
        let sqlite_snapshot_future = self.load_sqlite_series_snapshot(&series_id);
        let postgres_snapshot_future = self.load_postgres_series_snapshot(&series_id);
        let (sqlite_snapshot_result, postgres_snapshot_result) =
            tokio::join!(sqlite_snapshot_future, postgres_snapshot_future);
        let sqlite_snapshot = sqlite_snapshot_result?;
        let postgres_snapshot = postgres_snapshot_result?;

        let sqlite_future = self.sqlite.archive_series(input.clone());
        let postgres_future = self.postgres.archive_series(input);
        let (sqlite_result, postgres_result) = tokio::join!(sqlite_future, postgres_future);

        match (sqlite_result, postgres_result) {
            (Ok(sqlite_result), Ok(postgres_result)) => {
                if sqlite_result.series_id != postgres_result.series_id
                    || sqlite_result.archived_at != postgres_result.archived_at
                {
                    return Err(RepositoryError::storage(
                        "dual_sync archive_series produced inconsistent series_id/archived_at between sqlite and postgres",
                    ));
                }

                Ok(sqlite_result)
            }
            (Err(sqlite_error), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "archive_series",
                    sqlite_error = %sqlite_error,
                    postgres_error = %postgres_error,
                    "dual_sync write failed on both backends"
                );
                Err(build_dual_failure(
                    "archive_series",
                    Some(&sqlite_error),
                    Some(&postgres_error),
                ))
            }
            (Ok(_), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "archive_series",
                    succeeded_side = "sqlite",
                    failed_side = "postgres",
                    error = %postgres_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "archive_series",
                    BackendSide::Sqlite,
                    BackendSide::Postgres,
                    &postgres_error,
                    &format!("series_id={series_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("archive_series", &series_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .restore_sqlite_series_snapshot(&series_id, &sqlite_snapshot)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "archive_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "archive_series",
                            rollback_side = "sqlite",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "archive_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "archive_series",
                            rollback_side = "sqlite",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "archive_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
            (Err(sqlite_error), Ok(_)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "archive_series",
                    succeeded_side = "postgres",
                    failed_side = "sqlite",
                    error = %sqlite_error,
                    "dual_sync single-side failure detected"
                );
                let alert_reason = build_consistency_alert_reason(
                    "archive_series",
                    BackendSide::Postgres,
                    BackendSide::Sqlite,
                    &sqlite_error,
                    &format!("series_id={series_id}"),
                );
                let alert_id = self
                    .write_consistency_alert("archive_series", &series_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .restore_postgres_series_snapshot(&series_id, &postgres_snapshot)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "archive_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "archive_series",
                            rollback_side = "postgres",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "archive_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "archive_series",
                            rollback_side = "postgres",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "archive_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
        }
    }

    async fn mark_silent_series(
        &self,
        input: MarkSilentSeriesInput,
    ) -> Result<MarkSilentSeriesResult, RepositoryError> {
        let threshold_before = input.threshold_before.clone();
        let sqlite_future = self.sqlite.mark_silent_series(input.clone());
        let postgres_future = self.postgres.mark_silent_series(input);
        let (sqlite_result, postgres_result) = tokio::join!(sqlite_future, postgres_future);

        match (sqlite_result, postgres_result) {
            (Ok(sqlite_result), Ok(postgres_result)) => {
                if sqlite_result.affected_series_ids != postgres_result.affected_series_ids {
                    return Err(RepositoryError::storage(
                        "dual_sync mark_silent_series produced inconsistent affected ids between sqlite and postgres",
                    ));
                }

                Ok(sqlite_result)
            }
            (Err(sqlite_error), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "mark_silent_series",
                    sqlite_error = %sqlite_error,
                    postgres_error = %postgres_error,
                    "dual_sync write failed on both backends"
                );
                Err(build_dual_failure(
                    "mark_silent_series",
                    Some(&sqlite_error),
                    Some(&postgres_error),
                ))
            }
            (Ok(sqlite_result), Err(postgres_error)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "mark_silent_series",
                    succeeded_side = "sqlite",
                    failed_side = "postgres",
                    error = %postgres_error,
                    "dual_sync single-side failure detected"
                );
                let batch_id = build_mark_silent_batch_id(
                    &threshold_before,
                    &sqlite_result.affected_series_ids,
                );
                let detail = format!(
                    "threshold_before={threshold_before}, affected_series_ids={}",
                    format_affected_series_ids(&sqlite_result.affected_series_ids)
                );
                let alert_reason = build_consistency_alert_reason(
                    "mark_silent_series",
                    BackendSide::Sqlite,
                    BackendSide::Postgres,
                    &postgres_error,
                    &detail,
                );
                let alert_id = self
                    .write_consistency_alert("mark_silent_series", &batch_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .rollback_sqlite_mark_silent_series(&sqlite_result.affected_series_ids)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "mark_silent_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "mark_silent_series",
                            rollback_side = "sqlite",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "mark_silent_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "mark_silent_series",
                            rollback_side = "sqlite",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "mark_silent_series",
                            BackendSide::Postgres,
                            &postgres_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
            (Err(sqlite_error), Ok(postgres_result)) => {
                tracing::warn!(
                    component = "repository",
                    operation = "mark_silent_series",
                    succeeded_side = "postgres",
                    failed_side = "sqlite",
                    error = %sqlite_error,
                    "dual_sync single-side failure detected"
                );
                let batch_id = build_mark_silent_batch_id(
                    &threshold_before,
                    &postgres_result.affected_series_ids,
                );
                let detail = format!(
                    "threshold_before={threshold_before}, affected_series_ids={}",
                    format_affected_series_ids(&postgres_result.affected_series_ids)
                );
                let alert_reason = build_consistency_alert_reason(
                    "mark_silent_series",
                    BackendSide::Postgres,
                    BackendSide::Sqlite,
                    &sqlite_error,
                    &detail,
                );
                let alert_id = self
                    .write_consistency_alert("mark_silent_series", &batch_id, &alert_reason)
                    .await;
                let compensation_result = self
                    .rollback_postgres_mark_silent_series(&postgres_result.affected_series_ids)
                    .await;
                match compensation_result {
                    Ok(()) => {
                        if let Some(alert_id) = alert_id.as_deref() {
                            self.resolve_consistency_alert(alert_id, "mark_silent_series")
                                .await;
                        }
                        tracing::info!(
                            component = "repository",
                            operation = "mark_silent_series",
                            rollback_side = "postgres",
                            "dual_sync compensation completed"
                        );
                        Err(build_single_side_failure(
                            "mark_silent_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            None,
                        ))
                    }
                    Err(compensation_error) => {
                        tracing::error!(
                            component = "repository",
                            operation = "mark_silent_series",
                            rollback_side = "postgres",
                            error = %compensation_error,
                            "dual_sync compensation failed"
                        );
                        Err(build_single_side_failure(
                            "mark_silent_series",
                            BackendSide::Sqlite,
                            &sqlite_error,
                            Some(&compensation_error),
                        ))
                    }
                }
            }
        }
    }

    async fn search_series_by_name(
        &self,
        query: SearchSeriesQuery,
    ) -> Result<Vec<SeriesRecord>, RepositoryError> {
        self.sqlite.search_series_by_name(query).await
    }
}
