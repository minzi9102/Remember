use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager, Runtime};
use uuid::Uuid;

use super::dto::{
    CommitAppendData, CommitItem, SeriesArchiveData, SeriesCreateData, SeriesListData,
    SeriesScanSilentData, SeriesStatus, SeriesSummary, TimelineListData,
};
use crate::repository::{
    self, AppendCommitInput, ArchiveSeriesInput, CommitRecord, CreateSeriesInput,
    DynMemoRepository, ListSeriesQuery, MarkSilentSeriesInput, RepositoryError, SearchSeriesQuery,
    SeriesRecord, TimelineQuery,
};

const SQLITE_DB_FILE_NAME: &str = "remember.sqlite3";

#[derive(Clone)]
pub struct ApplicationService {
    repository: DynMemoRepository,
    silent_days_threshold: u32,
}

#[derive(Clone)]
pub struct ApplicationServiceState {
    service: Arc<ApplicationService>,
}

#[derive(Clone)]
pub struct ServiceBootstrapReport {
    pub service_state: ApplicationServiceState,
    pub database_path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl ApplicationError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "validation error: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for ApplicationError {}

impl From<RepositoryError> for ApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::Validation(message) => Self::Validation(message),
            RepositoryError::NotFound(message) => Self::NotFound(message),
            RepositoryError::Conflict(message) => Self::Conflict(message),
            RepositoryError::Storage(message) => Self::Internal(message),
        }
    }
}

impl ApplicationServiceState {
    pub fn new(service: ApplicationService) -> Self {
        Self {
            service: Arc::new(service),
        }
    }

    pub fn service(&self) -> &ApplicationService {
        self.service.as_ref()
    }
}

impl ApplicationService {
    pub fn new(repository: DynMemoRepository, silent_days_threshold: u32) -> Self {
        Self {
            repository,
            silent_days_threshold,
        }
    }

    pub async fn create_series(&self, name: String) -> Result<SeriesCreateData, ApplicationError> {
        let name = validate_non_empty(&name, "name")?;
        let created_at = now_utc_rfc3339_seconds();
        let record = self
            .repository
            .create_series(CreateSeriesInput {
                id: Uuid::now_v7().to_string(),
                name,
                created_at,
            })
            .await?;

        Ok(SeriesCreateData {
            series: map_series_record(record),
        })
    }

    pub async fn list_series(
        &self,
        query: String,
        include_archived: bool,
        cursor: Option<String>,
        limit: u64,
    ) -> Result<SeriesListData, ApplicationError> {
        validate_positive_limit(limit, "limit")?;
        let query = query.trim().to_string();
        let cursor = cursor
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if !query.is_empty() {
            let items = self
                .repository
                .search_series_by_name(SearchSeriesQuery {
                    query,
                    include_archived,
                    limit,
                })
                .await?
                .into_iter()
                .map(map_series_record)
                .collect();
            return Ok(SeriesListData {
                items,
                next_cursor: None,
                limit_echo: limit,
            });
        }

        let paged = self
            .repository
            .list_series(ListSeriesQuery {
                include_archived,
                cursor,
                limit,
            })
            .await?;

        Ok(SeriesListData {
            items: paged.items.into_iter().map(map_series_record).collect(),
            next_cursor: paged.next_cursor,
            limit_echo: paged.limit_echo,
        })
    }

    pub async fn append_commit(
        &self,
        series_id: String,
        content: String,
        client_ts: String,
    ) -> Result<CommitAppendData, ApplicationError> {
        let series_id = validate_non_empty(&series_id, "seriesId")?;
        let content = validate_non_empty(&content, "content")?;
        let created_at = normalize_rfc3339(&client_ts, "clientTs")?;
        let result = self
            .repository
            .append_commit(AppendCommitInput {
                commit_id: Uuid::now_v7().to_string(),
                series_id,
                content,
                created_at,
            })
            .await?;

        Ok(CommitAppendData {
            commit: map_commit_record(result.commit),
            series: map_series_record(result.series),
        })
    }

    pub async fn list_timeline(
        &self,
        series_id: String,
        cursor: Option<String>,
        limit: u64,
    ) -> Result<TimelineListData, ApplicationError> {
        let series_id = validate_non_empty(&series_id, "seriesId")?;
        validate_positive_limit(limit, "limit")?;
        let cursor = cursor
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let paged = self
            .repository
            .list_timeline(TimelineQuery {
                series_id: series_id.clone(),
                cursor,
                limit,
            })
            .await?;

        Ok(TimelineListData {
            series_id,
            items: paged.items.into_iter().map(map_commit_record).collect(),
            next_cursor: paged.next_cursor,
        })
    }

    pub async fn archive_series(
        &self,
        series_id: String,
    ) -> Result<SeriesArchiveData, ApplicationError> {
        let series_id = validate_non_empty(&series_id, "seriesId")?;
        let archived_at = now_utc_rfc3339_seconds();
        let result = self
            .repository
            .archive_series(ArchiveSeriesInput {
                series_id,
                archived_at,
            })
            .await?;

        Ok(SeriesArchiveData {
            series_id: result.series_id,
            archived_at: result.archived_at,
        })
    }

    pub async fn scan_silent(
        &self,
        now: String,
        threshold_days: u64,
    ) -> Result<SeriesScanSilentData, ApplicationError> {
        let normalized_now = normalize_rfc3339(&now, "now")?;
        let threshold_days = if threshold_days == 0 {
            u64::from(self.silent_days_threshold)
        } else {
            threshold_days
        };
        let threshold_days_i64 = i64::try_from(threshold_days).map_err(|_| {
            ApplicationError::validation("field `thresholdDays` is too large to process")
        })?;

        let now_dt = DateTime::parse_from_rfc3339(&normalized_now)
            .map_err(|error| {
                ApplicationError::validation(format!(
                    "field `now` must be a valid RFC3339 timestamp: {error}"
                ))
            })?
            .with_timezone(&Utc);
        let threshold_before = now_dt
            .checked_sub_signed(Duration::days(threshold_days_i64))
            .ok_or_else(|| {
                ApplicationError::validation("failed to compute threshold datetime for `now`")
            })?
            .to_rfc3339_opts(SecondsFormat::Secs, true);

        let result = self
            .repository
            .mark_silent_series(MarkSilentSeriesInput { threshold_before })
            .await?;

        Ok(SeriesScanSilentData {
            affected_series_ids: result.affected_series_ids,
            threshold_days,
        })
    }
}

pub async fn bootstrap_sqlite_service<R: Runtime>(
    app: &AppHandle<R>,
    silent_days_threshold: u32,
) -> Result<ServiceBootstrapReport, ApplicationError> {
    let (database_path, warnings) = resolve_sqlite_database_path(app);
    let pool = connect_sqlite_pool(&database_path).await?;
    repository::migrations::run_sqlite_migrations(&pool)
        .await
        .map_err(|error| {
            ApplicationError::internal(format!(
                "failed to run sqlite migrations on {}: {error}",
                database_path.display()
            ))
        })?;

    let repository: DynMemoRepository = Arc::new(repository::SqliteRepository::new(pool));
    let service = ApplicationService::new(repository, silent_days_threshold);

    Ok(ServiceBootstrapReport {
        service_state: ApplicationServiceState::new(service),
        database_path,
        warnings,
    })
}

async fn connect_sqlite_pool(database_path: &PathBuf) -> Result<SqlitePool, ApplicationError> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| {
            ApplicationError::internal(format!(
                "failed to connect sqlite database {}: {error}",
                database_path.display()
            ))
        })
}

fn resolve_sqlite_database_path<R: Runtime>(app: &AppHandle<R>) -> (PathBuf, Vec<String>) {
    let mut warnings = Vec::new();

    match app.path().app_data_dir() {
        Ok(mut app_data_dir) => {
            if let Err(error) = fs::create_dir_all(&app_data_dir) {
                warnings.push(format!(
                    "failed to create app data directory {}, fallback to current working directory: {error}",
                    app_data_dir.display()
                ));
            } else {
                app_data_dir.push(SQLITE_DB_FILE_NAME);
                return (app_data_dir, warnings);
            }
        }
        Err(error) => {
            warnings.push(format!(
                "failed to resolve app data directory, fallback to current working directory: {error}"
            ));
        }
    }

    (PathBuf::from(SQLITE_DB_FILE_NAME), warnings)
}

fn map_series_record(record: SeriesRecord) -> SeriesSummary {
    SeriesSummary {
        id: record.id,
        name: record.name,
        status: map_series_status(record.status),
        last_updated_at: record.last_updated_at,
        latest_excerpt: record.latest_excerpt,
        created_at: record.created_at,
        archived_at: record.archived_at,
    }
}

fn map_commit_record(record: CommitRecord) -> CommitItem {
    CommitItem {
        id: record.id,
        series_id: record.series_id,
        content: record.content,
        created_at: record.created_at,
    }
}

fn map_series_status(status: crate::repository::SeriesStatus) -> SeriesStatus {
    match status {
        crate::repository::SeriesStatus::Active => SeriesStatus::Active,
        crate::repository::SeriesStatus::Silent => SeriesStatus::Silent,
        crate::repository::SeriesStatus::Archived => SeriesStatus::Archived,
    }
}

fn validate_non_empty(value: &str, field: &str) -> Result<String, ApplicationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ApplicationError::validation(format!(
            "field `{field}` is required and must be a non-empty string"
        )));
    }
    Ok(trimmed.to_string())
}

fn validate_positive_limit(value: u64, field: &str) -> Result<(), ApplicationError> {
    if value == 0 {
        return Err(ApplicationError::validation(format!(
            "field `{field}` must be a positive integer"
        )));
    }
    Ok(())
}

fn normalize_rfc3339(value: &str, field: &str) -> Result<String, ApplicationError> {
    let value = validate_non_empty(value, field)?;
    let parsed = DateTime::parse_from_rfc3339(&value).map_err(|error| {
        ApplicationError::validation(format!(
            "field `{field}` must be a valid RFC3339 timestamp: {error}"
        ))
    })?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn now_utc_rfc3339_seconds() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
pub(crate) async fn build_test_service_state() -> ApplicationServiceState {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for application service tests");
    repository::migrations::run_sqlite_migrations(&pool)
        .await
        .expect("failed to run sqlite migrations for application service tests");
    let repository: DynMemoRepository = Arc::new(repository::SqliteRepository::new(pool));
    ApplicationServiceState::new(ApplicationService::new(repository, 7))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sqlx::sqlite::SqlitePoolOptions;

    use super::{ApplicationError, ApplicationService};
    use crate::repository::{
        self, CreateSeriesInput, ListSeriesQuery, MarkSilentSeriesInput, MemoRepository,
    };

    #[tokio::test]
    async fn application_service_completes_core_flow() {
        let service = build_test_service().await;
        let created = service
            .create_series("Inbox".to_string())
            .await
            .expect("create_series should succeed");

        let appended = service
            .append_commit(
                created.series.id.clone(),
                "first-note".to_string(),
                "2026-03-16T10:00:00+08:00".to_string(),
            )
            .await
            .expect("append_commit should succeed");
        assert_eq!(appended.commit.series_id, created.series.id);
        assert_eq!(appended.commit.created_at, "2026-03-16T02:00:00Z");

        let listed = service
            .list_series("".to_string(), false, None, 20)
            .await
            .expect("list_series should succeed");
        assert_eq!(
            listed.items.first().map(|item| item.id.as_str()),
            Some(created.series.id.as_str())
        );

        let timeline = service
            .list_timeline(created.series.id.clone(), None, 20)
            .await
            .expect("list_timeline should succeed");
        assert_eq!(timeline.series_id, created.series.id);
        assert_eq!(timeline.items.len(), 1);
        assert_eq!(timeline.items[0].id, appended.commit.id);

        let archived = service
            .archive_series(created.series.id.clone())
            .await
            .expect("archive_series should succeed");
        assert_eq!(archived.series_id, created.series.id);

        let searched = service
            .list_series("Inbox".to_string(), true, None, 20)
            .await
            .expect("search list_series should succeed");
        assert_eq!(searched.items.len(), 1);
        assert!(searched.next_cursor.is_none());
    }

    #[tokio::test]
    async fn rejects_invalid_rfc3339_timestamp() {
        let service = build_test_service().await;
        let created = service
            .create_series("Inbox".to_string())
            .await
            .expect("create_series should succeed");

        let error = service
            .append_commit(
                created.series.id,
                "note".to_string(),
                "not-a-timestamp".to_string(),
            )
            .await
            .expect_err("append_commit should reject invalid clientTs");
        assert!(matches!(error, ApplicationError::Validation(_)));
    }

    #[tokio::test]
    async fn maps_repository_errors_to_application_errors() {
        let service = build_test_service().await;

        let not_found_error = service
            .archive_series("missing-series".to_string())
            .await
            .expect_err("archive on missing series should fail");
        assert!(matches!(not_found_error, ApplicationError::NotFound(_)));

        let created = service
            .create_series("Inbox".to_string())
            .await
            .expect("create_series should succeed");
        service
            .archive_series(created.series.id.clone())
            .await
            .expect("archive should succeed");
        let conflict_error = service
            .append_commit(
                created.series.id,
                "note".to_string(),
                "2026-03-16T00:00:00Z".to_string(),
            )
            .await
            .expect_err("append on archived series should fail");
        assert!(matches!(conflict_error, ApplicationError::Conflict(_)));
    }

    #[tokio::test]
    async fn scan_silent_uses_now_minus_threshold_days() {
        let (service, repository) = build_test_service_with_repository().await;
        repository
            .create_series(CreateSeriesInput {
                id: "series-old".to_string(),
                name: "Old".to_string(),
                created_at: "2026-03-01T00:00:00Z".to_string(),
            })
            .await
            .expect("create old series should succeed");
        repository
            .create_series(CreateSeriesInput {
                id: "series-new".to_string(),
                name: "New".to_string(),
                created_at: "2026-03-15T00:00:00Z".to_string(),
            })
            .await
            .expect("create new series should succeed");
        repository
            .mark_silent_series(MarkSilentSeriesInput {
                threshold_before: "1900-01-01T00:00:00Z".to_string(),
            })
            .await
            .expect("pre-check should succeed");

        let scan = service
            .scan_silent("2026-03-16T00:00:00+08:00".to_string(), 7)
            .await
            .expect("scan_silent should succeed");
        assert_eq!(scan.threshold_days, 7);
        assert!(scan.affected_series_ids.contains(&"series-old".to_string()));
        assert!(!scan.affected_series_ids.contains(&"series-new".to_string()));

        let list = repository
            .list_series(ListSeriesQuery {
                include_archived: true,
                cursor: None,
                limit: 20,
            })
            .await
            .expect("list should succeed");
        let old = list
            .items
            .iter()
            .find(|item| item.id == "series-old")
            .expect("old series should exist");
        assert_eq!(old.status.as_db_value(), "silent");
    }

    async fn build_test_service() -> ApplicationService {
        let (service, _) = build_test_service_with_repository().await;
        service
    }

    async fn build_test_service_with_repository(
    ) -> (ApplicationService, Arc<repository::SqliteRepository>) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to connect sqlite memory db");
        repository::migrations::run_sqlite_migrations(&pool)
            .await
            .expect("failed to run sqlite migrations");
        let repository_impl = Arc::new(repository::SqliteRepository::new(pool));
        let repository_trait: crate::repository::DynMemoRepository = repository_impl.clone();
        (
            ApplicationService::new(repository_trait, 7),
            repository_impl,
        )
    }
}
