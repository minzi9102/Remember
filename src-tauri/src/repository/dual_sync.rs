#![allow(dead_code)]

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sqlx::{postgres::PgPool, sqlite::SqlitePool};
use uuid::Uuid;

use super::{
    map_sqlx_error, maybe_inject_test_failure, postgres::apply_postgres_tx_deadlines,
    AppendCommitInput, AppendCommitResult, ArchiveSeriesInput, ArchiveSeriesResult,
    CreateSeriesInput, ListSeriesQuery, MarkSilentSeriesInput, MarkSilentSeriesResult,
    MemoRepository, PagedResult, PostgresRepository, RepositoryError, SearchSeriesQuery,
    SeriesRecord, SqliteRepository, TimelineQuery,
};

#[derive(Debug, Clone)]
pub struct DualSyncRepository {
    sqlite: SqliteRepository,
    postgres: PostgresRepository,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeriesSnapshot {
    status: String,
    latest_excerpt: String,
    last_updated_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupSelfHealSummary {
    pub scanned_alerts: u64,
    pub repaired_alerts: u64,
    pub unresolved_alerts: u64,
    pub failed_alerts: u64,
    pub completed_at: String,
    pub messages: Vec<String>,
}

impl StartupSelfHealSummary {
    pub fn clean() -> Self {
        Self {
            scanned_alerts: 0,
            repaired_alerts: 0,
            unresolved_alerts: 0,
            failed_alerts: 0,
            completed_at: now_utc_rfc3339_seconds(),
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsistencyAlertRecord {
    id: String,
    op_type: String,
    commit_id: String,
    reason: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedConsistencyAlert {
    id: String,
    op_type: StartupSelfHealOperation,
    commit_id: String,
    succeeded_side: BackendSide,
    failed_side: BackendSide,
    detail: StartupSelfHealDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupSelfHealOperation {
    CreateSeries,
    AppendCommit,
    ArchiveSeries,
    MarkSilentSeries,
}

impl StartupSelfHealOperation {
    fn from_op_type(value: &str) -> Option<Self> {
        match value {
            "create_series" => Some(Self::CreateSeries),
            "append_commit" => Some(Self::AppendCommit),
            "archive_series" => Some(Self::ArchiveSeries),
            "mark_silent_series" => Some(Self::MarkSilentSeries),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CreateSeries => "create_series",
            Self::AppendCommit => "append_commit",
            Self::ArchiveSeries => "archive_series",
            Self::MarkSilentSeries => "mark_silent_series",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupSelfHealDetail {
    CreateSeries {
        series_id: String,
    },
    AppendCommit {
        series_id: String,
    },
    ArchiveSeries {
        series_id: String,
    },
    MarkSilentSeries {
        threshold_before: String,
        affected_series_ids: Vec<String>,
    },
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

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "sqlite" => Some(Self::Sqlite),
            "postgres" => Some(Self::Postgres),
            _ => None,
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

    pub async fn run_startup_self_heal(&self) -> StartupSelfHealSummary {
        let alerts = match self.load_unresolved_consistency_alerts().await {
            Ok(alerts) => alerts,
            Err(error) => {
                return StartupSelfHealSummary {
                    scanned_alerts: 0,
                    repaired_alerts: 0,
                    unresolved_alerts: 1,
                    failed_alerts: 1,
                    completed_at: now_utc_rfc3339_seconds(),
                    messages: vec![format!(
                        "failed to load unresolved consistency alerts: {error}"
                    )],
                };
            }
        };

        let mut summary = StartupSelfHealSummary {
            scanned_alerts: alerts.len() as u64,
            repaired_alerts: 0,
            unresolved_alerts: 0,
            failed_alerts: 0,
            completed_at: String::new(),
            messages: Vec::new(),
        };

        for alert in alerts {
            let parsed = match parse_consistency_alert(&alert) {
                Ok(parsed) => parsed,
                Err(error) => {
                    summary.failed_alerts += 1;
                    summary.messages.push(format!(
                        "alert `{}` ({}) could not be parsed: {error}",
                        alert.id, alert.op_type
                    ));
                    continue;
                }
            };

            match self.apply_startup_self_heal(&parsed).await {
                Ok(()) => {
                    if let Err(error) = self
                        .resolve_consistency_alert_for_startup(&parsed.id, parsed.op_type.as_str())
                        .await
                    {
                        summary.failed_alerts += 1;
                        summary.messages.push(format!(
                            "alert `{}` ({}) repaired but could not be resolved: {error}",
                            parsed.id,
                            parsed.op_type.as_str()
                        ));
                    } else {
                        summary.repaired_alerts += 1;
                    }
                }
                Err(error) => {
                    summary.failed_alerts += 1;
                    summary.messages.push(format!(
                        "alert `{}` ({}) remains unresolved: {error}",
                        parsed.id,
                        parsed.op_type.as_str()
                    ));
                }
            }
        }

        summary.unresolved_alerts = summary.failed_alerts;
        summary.completed_at = now_utc_rfc3339_seconds();
        summary
    }

    async fn load_sqlite_series_snapshot(
        &self,
        series_id: &str,
    ) -> Result<SeriesSnapshot, RepositoryError> {
        let Some(snapshot) = self.load_sqlite_series_snapshot_optional(series_id).await? else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist in sqlite"
            )));
        };

        Ok(snapshot)
    }

    async fn load_sqlite_series_snapshot_optional(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesSnapshot>, RepositoryError> {
        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT status, latest_excerpt, last_updated_at, archived_at
             FROM series
             WHERE id = ?",
        )
        .bind(series_id)
        .fetch_optional(self.sqlite.pool())
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(
            |(status, latest_excerpt, last_updated_at, archived_at)| SeriesSnapshot {
                status,
                latest_excerpt,
                last_updated_at,
                archived_at,
            },
        ))
    }

    async fn load_postgres_series_snapshot(
        &self,
        series_id: &str,
    ) -> Result<SeriesSnapshot, RepositoryError> {
        let Some(snapshot) = self
            .load_postgres_series_snapshot_optional(series_id)
            .await?
        else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist in postgres"
            )));
        };

        Ok(snapshot)
    }

    async fn load_postgres_series_snapshot_optional(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesSnapshot>, RepositoryError> {
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

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
        tx.rollback().await.map_err(map_sqlx_error)?;

        Ok(row.map(
            |(status, latest_excerpt, last_updated_at, archived_at)| SeriesSnapshot {
                status,
                latest_excerpt,
                last_updated_at,
                archived_at,
            },
        ))
    }

    async fn rollback_sqlite_create_series(&self, series_id: &str) -> Result<(), RepositoryError> {
        maybe_inject_test_failure("sqlite", "rollback_create_series", series_id)?;
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
        maybe_inject_test_failure("postgres", "rollback_create_series", series_id)?;
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;
        sqlx::query("DELETE FROM series WHERE id = $1")
            .bind(series_id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn restore_sqlite_series_snapshot(
        &self,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        maybe_inject_test_failure("sqlite", "restore_series_snapshot", series_id)?;
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
        maybe_inject_test_failure("postgres", "restore_series_snapshot", series_id)?;
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;
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

    async fn rollback_sqlite_append_commit(
        &self,
        commit_id: &str,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        maybe_inject_test_failure("sqlite", "rollback_append_commit", commit_id)?;
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
        maybe_inject_test_failure("postgres", "rollback_append_commit", commit_id)?;
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

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

        maybe_inject_test_failure(
            "sqlite",
            "rollback_mark_silent_series",
            &format_affected_series_ids(affected_series_ids),
        )?;
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

        maybe_inject_test_failure(
            "postgres",
            "rollback_mark_silent_series",
            &format_affected_series_ids(affected_series_ids),
        )?;
        let mut tx = self.postgres.pool().begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;
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

    async fn load_unresolved_consistency_alerts(
        &self,
    ) -> Result<Vec<ConsistencyAlertRecord>, RepositoryError> {
        let sqlite_future = async {
            let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
                "SELECT id, op_type, commit_id, reason, created_at
                 FROM consistency_alerts
                 WHERE resolved_at IS NULL",
            )
            .fetch_all(self.sqlite.pool())
            .await
            .map_err(map_sqlx_error)?;

            Ok::<Vec<ConsistencyAlertRecord>, RepositoryError>(
                rows.into_iter()
                    .map(
                        |(id, op_type, commit_id, reason, created_at)| ConsistencyAlertRecord {
                            id,
                            op_type,
                            commit_id,
                            reason,
                            created_at,
                        },
                    )
                    .collect(),
            )
        };
        let postgres_future = async {
            let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
                "SELECT
                    id,
                    op_type,
                    commit_id,
                    reason,
                    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
                 FROM consistency_alerts
                 WHERE resolved_at IS NULL",
            )
            .fetch_all(self.postgres.pool())
            .await
            .map_err(map_sqlx_error)?;

            Ok::<Vec<ConsistencyAlertRecord>, RepositoryError>(
                rows.into_iter()
                    .map(
                        |(id, op_type, commit_id, reason, created_at)| ConsistencyAlertRecord {
                            id,
                            op_type,
                            commit_id,
                            reason,
                            created_at,
                        },
                    )
                    .collect(),
            )
        };

        let (sqlite_result, postgres_result) = tokio::join!(sqlite_future, postgres_future);
        let sqlite_alerts = sqlite_result?;
        let postgres_alerts = postgres_result?;
        Ok(merge_unresolved_alerts(sqlite_alerts, postgres_alerts))
    }

    async fn apply_startup_self_heal(
        &self,
        alert: &ParsedConsistencyAlert,
    ) -> Result<(), RepositoryError> {
        match &alert.detail {
            StartupSelfHealDetail::CreateSeries { series_id } => {
                self.rollback_create_series_on_side(alert.succeeded_side, series_id)
                    .await
            }
            StartupSelfHealDetail::AppendCommit { series_id } => {
                let success_snapshot = self
                    .load_series_snapshot_on_side(alert.succeeded_side, series_id)
                    .await?;
                let failed_snapshot = self
                    .load_series_snapshot_on_side(alert.failed_side, series_id)
                    .await?;
                let Some(_) = success_snapshot else {
                    return Err(RepositoryError::storage(format!(
                        "series `{series_id}` is missing on succeeded side `{}` during startup append_commit repair",
                        alert.succeeded_side.as_str()
                    )));
                };
                let Some(failed_snapshot) = failed_snapshot else {
                    return Err(RepositoryError::storage(format!(
                        "series `{series_id}` is missing on failed side `{}` during startup append_commit repair",
                        alert.failed_side.as_str()
                    )));
                };

                self.rollback_append_commit_on_side(
                    alert.succeeded_side,
                    &alert.commit_id,
                    series_id,
                    &failed_snapshot,
                )
                .await
            }
            StartupSelfHealDetail::ArchiveSeries { series_id } => {
                let success_snapshot = self
                    .load_series_snapshot_on_side(alert.succeeded_side, series_id)
                    .await?;
                let failed_snapshot = self
                    .load_series_snapshot_on_side(alert.failed_side, series_id)
                    .await?;
                let Some(_) = success_snapshot else {
                    return Err(RepositoryError::storage(format!(
                        "series `{series_id}` is missing on succeeded side `{}` during startup archive repair",
                        alert.succeeded_side.as_str()
                    )));
                };
                let Some(failed_snapshot) = failed_snapshot else {
                    return Err(RepositoryError::storage(format!(
                        "series `{series_id}` is missing on failed side `{}` during startup archive repair",
                        alert.failed_side.as_str()
                    )));
                };

                self.restore_series_snapshot_on_side(
                    alert.succeeded_side,
                    series_id,
                    &failed_snapshot,
                )
                .await
            }
            StartupSelfHealDetail::MarkSilentSeries {
                threshold_before: _,
                affected_series_ids,
            } => {
                for series_id in affected_series_ids {
                    let success_snapshot = self
                        .load_series_snapshot_on_side(alert.succeeded_side, series_id)
                        .await?;
                    let failed_snapshot = self
                        .load_series_snapshot_on_side(alert.failed_side, series_id)
                        .await?;
                    let Some(_) = success_snapshot else {
                        return Err(RepositoryError::storage(format!(
                            "series `{series_id}` is missing on succeeded side `{}` during startup mark_silent repair",
                            alert.succeeded_side.as_str()
                        )));
                    };
                    let Some(failed_snapshot) = failed_snapshot else {
                        return Err(RepositoryError::storage(format!(
                            "series `{series_id}` is missing on failed side `{}` during startup mark_silent repair",
                            alert.failed_side.as_str()
                        )));
                    };

                    self.restore_series_snapshot_on_side(
                        alert.succeeded_side,
                        series_id,
                        &failed_snapshot,
                    )
                    .await?;
                }

                Ok(())
            }
        }
    }

    async fn load_series_snapshot_on_side(
        &self,
        side: BackendSide,
        series_id: &str,
    ) -> Result<Option<SeriesSnapshot>, RepositoryError> {
        match side {
            BackendSide::Sqlite => self.load_sqlite_series_snapshot_optional(series_id).await,
            BackendSide::Postgres => self.load_postgres_series_snapshot_optional(series_id).await,
        }
    }

    async fn rollback_create_series_on_side(
        &self,
        side: BackendSide,
        series_id: &str,
    ) -> Result<(), RepositoryError> {
        match side {
            BackendSide::Sqlite => self.rollback_sqlite_create_series(series_id).await,
            BackendSide::Postgres => self.rollback_postgres_create_series(series_id).await,
        }
    }

    async fn rollback_append_commit_on_side(
        &self,
        side: BackendSide,
        commit_id: &str,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        match side {
            BackendSide::Sqlite => {
                self.rollback_sqlite_append_commit(commit_id, series_id, snapshot)
                    .await
            }
            BackendSide::Postgres => {
                self.rollback_postgres_append_commit(commit_id, series_id, snapshot)
                    .await
            }
        }
    }

    async fn restore_series_snapshot_on_side(
        &self,
        side: BackendSide,
        series_id: &str,
        snapshot: &SeriesSnapshot,
    ) -> Result<(), RepositoryError> {
        match side {
            BackendSide::Sqlite => {
                self.restore_sqlite_series_snapshot(series_id, snapshot)
                    .await
            }
            BackendSide::Postgres => {
                self.restore_postgres_series_snapshot(series_id, snapshot)
                    .await
            }
        }
    }

    async fn resolve_consistency_alert(&self, alert_id: &str, op_type: &str) {
        let _ = self
            .resolve_consistency_alert_for_startup(alert_id, op_type)
            .await;
    }

    async fn resolve_consistency_alert_for_startup(
        &self,
        alert_id: &str,
        op_type: &str,
    ) -> Result<(), RepositoryError> {
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

        if let Err(error) = &sqlite_result {
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

        if let Err(error) = &postgres_result {
            tracing::error!(
                component = "repository",
                operation = op_type,
                backend = "postgres",
                alert_id,
                error = %error,
                "failed to resolve consistency alert"
            );
        }

        match (sqlite_result, postgres_result) {
            (Ok(_), Ok(_)) => Ok(()),
            (Err(sqlite_error), Ok(_)) => Err(RepositoryError::storage(format!(
                "failed to resolve consistency alert `{alert_id}` on sqlite: {sqlite_error}"
            ))),
            (Ok(_), Err(postgres_error)) => Err(RepositoryError::storage(format!(
                "failed to resolve consistency alert `{alert_id}` on postgres: {postgres_error}"
            ))),
            (Err(sqlite_error), Err(postgres_error)) => Err(RepositoryError::storage(format!(
                "failed to resolve consistency alert `{alert_id}` on both backends; sqlite_error={sqlite_error}; postgres_error={postgres_error}"
            ))),
        }
    }
}

fn merge_unresolved_alerts(
    sqlite_alerts: Vec<ConsistencyAlertRecord>,
    postgres_alerts: Vec<ConsistencyAlertRecord>,
) -> Vec<ConsistencyAlertRecord> {
    let mut alerts_by_id = BTreeMap::new();
    for alert in sqlite_alerts.into_iter().chain(postgres_alerts) {
        alerts_by_id.entry(alert.id.clone()).or_insert(alert);
    }

    let mut alerts: Vec<_> = alerts_by_id.into_values().collect();
    alerts.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    alerts
}

fn parse_consistency_alert(
    alert: &ConsistencyAlertRecord,
) -> Result<ParsedConsistencyAlert, String> {
    let op_type = StartupSelfHealOperation::from_op_type(&alert.op_type)
        .ok_or_else(|| format!("unsupported consistency alert op_type `{}`", alert.op_type))?;
    let succeeded_side = parse_backend_side_field(&alert.reason, "succeeded_side=")?;
    let failed_side = parse_backend_side_field(&alert.reason, "failed_side=")?;
    let detail = extract_detail_segment(&alert.reason)?;
    let parsed_detail = match op_type {
        StartupSelfHealOperation::CreateSeries => {
            parse_series_detail(detail, "create_series", &alert.commit_id)?
        }
        StartupSelfHealOperation::AppendCommit => {
            parse_append_commit_detail(detail, &alert.commit_id)?
        }
        StartupSelfHealOperation::ArchiveSeries => {
            parse_series_detail(detail, "archive_series", &alert.commit_id)?
        }
        StartupSelfHealOperation::MarkSilentSeries => parse_mark_silent_detail(detail)?,
    };

    Ok(ParsedConsistencyAlert {
        id: alert.id.clone(),
        op_type,
        commit_id: alert.commit_id.clone(),
        succeeded_side,
        failed_side,
        detail: parsed_detail,
    })
}

fn parse_backend_side_field(reason: &str, marker: &str) -> Result<BackendSide, String> {
    let value = extract_marker_value(reason, marker)
        .ok_or_else(|| format!("missing `{marker}` in consistency alert reason"))?;
    BackendSide::from_str(&value)
        .ok_or_else(|| format!("unsupported backend side `{value}` in consistency alert reason"))
}

fn extract_detail_segment(reason: &str) -> Result<&str, String> {
    let marker = "; detail=";
    let Some(index) = reason.rfind(marker) else {
        return Err("missing `detail=` segment in consistency alert reason".to_string());
    };

    Ok(reason[index + marker.len()..].trim())
}

fn extract_marker_value(reason: &str, marker: &str) -> Option<String> {
    let index = reason.find(marker)?;
    let start = index + marker.len();
    let remainder = &reason[start..];
    let end = remainder.find(';').unwrap_or(remainder.len());
    Some(remainder[..end].trim().to_string())
}

fn parse_series_detail(
    detail: &str,
    operation: &str,
    commit_id: &str,
) -> Result<StartupSelfHealDetail, String> {
    let Some(series_id) = detail.strip_prefix("series_id=") else {
        return Err(format!(
            "invalid `{operation}` detail `{detail}`, expected `series_id=...`"
        ));
    };
    let series_id = series_id.trim().to_string();
    if series_id.is_empty() {
        return Err(format!("`{operation}` detail is missing series_id"));
    }
    if series_id != commit_id {
        return Err(format!(
            "`{operation}` detail series_id `{series_id}` does not match commit_id column `{commit_id}`"
        ));
    }

    match operation {
        "create_series" => Ok(StartupSelfHealDetail::CreateSeries { series_id }),
        "archive_series" => Ok(StartupSelfHealDetail::ArchiveSeries { series_id }),
        _ => Err(format!("unsupported series detail operation `{operation}`")),
    }
}

fn parse_append_commit_detail(
    detail: &str,
    commit_id: &str,
) -> Result<StartupSelfHealDetail, String> {
    let Some(series_segment) = detail.strip_prefix("series_id=") else {
        return Err(format!(
            "invalid `append_commit` detail `{detail}`, expected `series_id=..., commit_id=...`"
        ));
    };
    let Some((series_id, detail_commit_id)) = series_segment.split_once(", commit_id=") else {
        return Err(format!(
            "invalid `append_commit` detail `{detail}`, expected `series_id=..., commit_id=...`"
        ));
    };
    let series_id = series_id.trim().to_string();
    let detail_commit_id = detail_commit_id.trim();
    if series_id.is_empty() {
        return Err("`append_commit` detail is missing series_id".to_string());
    }
    if detail_commit_id != commit_id {
        return Err(format!(
            "`append_commit` detail commit_id `{detail_commit_id}` does not match commit_id column `{commit_id}`"
        ));
    }

    Ok(StartupSelfHealDetail::AppendCommit { series_id })
}

fn parse_mark_silent_detail(detail: &str) -> Result<StartupSelfHealDetail, String> {
    let Some(threshold_segment) = detail.strip_prefix("threshold_before=") else {
        return Err(format!(
            "invalid `mark_silent_series` detail `{detail}`, expected `threshold_before=..., affected_series_ids=...`"
        ));
    };
    let Some((threshold_before, affected_series_ids)) =
        threshold_segment.split_once(", affected_series_ids=")
    else {
        return Err(format!(
            "invalid `mark_silent_series` detail `{detail}`, expected `threshold_before=..., affected_series_ids=...`"
        ));
    };

    let threshold_before = threshold_before.trim().to_string();
    if threshold_before.is_empty() {
        return Err("`mark_silent_series` detail is missing threshold_before".to_string());
    }
    let affected_series_ids = parse_affected_series_ids(affected_series_ids.trim());

    Ok(StartupSelfHealDetail::MarkSilentSeries {
        threshold_before,
        affected_series_ids,
    })
}

fn parse_affected_series_ids(raw: &str) -> Vec<String> {
    if raw == "<none>" || raw.is_empty() {
        return Vec::new();
    }

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn is_pg_timeout_error(error: &RepositoryError) -> bool {
    match error {
        RepositoryError::PgTimeout(_) => true,
        RepositoryError::Storage(message) | RepositoryError::DualWriteFailed(message) => {
            let normalized = message.to_ascii_lowercase();
            normalized.contains("statement timeout")
                || normalized.contains("lock timeout")
                || normalized.contains("query_canceled")
                || normalized.contains("57014")
                || normalized.contains("55p03")
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
        let sqlite_snapshot = self.load_sqlite_series_snapshot(&series_id).await?;

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
                    // The dual_sync invariant requires both backends to share the same
                    // pre-write series snapshot before we fan out the append.
                    .rollback_postgres_append_commit(&commit_id, &series_id, &sqlite_snapshot)
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

#[cfg(test)]
mod tests {
    use super::{
        merge_unresolved_alerts, parse_consistency_alert, BackendSide, ConsistencyAlertRecord,
        StartupSelfHealDetail, StartupSelfHealOperation,
    };

    #[test]
    fn parses_append_commit_consistency_alert_reason() {
        let alert = ConsistencyAlertRecord {
            id: "alert-1".to_string(),
            op_type: "append_commit".to_string(),
            commit_id: "commit-1".to_string(),
            reason: "dual_sync append_commit single-side success; succeeded_side=sqlite; failed_side=postgres; operation_error=simulated; detail=series_id=series-1, commit_id=commit-1".to_string(),
            created_at: "2026-03-17T00:00:00Z".to_string(),
        };

        let parsed = parse_consistency_alert(&alert).expect("alert should parse");

        assert_eq!(parsed.id, "alert-1");
        assert_eq!(parsed.op_type, StartupSelfHealOperation::AppendCommit);
        assert_eq!(parsed.succeeded_side, BackendSide::Sqlite);
        assert_eq!(parsed.failed_side, BackendSide::Postgres);
        assert_eq!(
            parsed.detail,
            StartupSelfHealDetail::AppendCommit {
                series_id: "series-1".to_string(),
            }
        );
    }

    #[test]
    fn merges_and_sorts_unresolved_alerts_by_id_and_created_at() {
        let sqlite_alerts = vec![
            ConsistencyAlertRecord {
                id: "alert-b".to_string(),
                op_type: "archive_series".to_string(),
                commit_id: "series-b".to_string(),
                reason: "reason-b".to_string(),
                created_at: "2026-03-17T00:02:00Z".to_string(),
            },
            ConsistencyAlertRecord {
                id: "alert-a".to_string(),
                op_type: "create_series".to_string(),
                commit_id: "series-a".to_string(),
                reason: "reason-a".to_string(),
                created_at: "2026-03-17T00:01:00Z".to_string(),
            },
        ];
        let postgres_alerts = vec![ConsistencyAlertRecord {
            id: "alert-a".to_string(),
            op_type: "create_series".to_string(),
            commit_id: "series-a".to_string(),
            reason: "reason-a".to_string(),
            created_at: "2026-03-17T00:01:00Z".to_string(),
        }];

        let merged = merge_unresolved_alerts(sqlite_alerts, postgres_alerts);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "alert-a");
        assert_eq!(merged[1].id, "alert-b");
    }
}
