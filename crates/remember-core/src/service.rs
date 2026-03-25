use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use uuid::Uuid;

use crate::dto::{
    CommitAppendData, CommitItem, SeriesArchiveData, SeriesCreateData, SeriesListData,
    SeriesScanSilentData, SeriesStatus, SeriesSummary, TimelineListData,
};
use crate::repository::{
    AppendCommitInput, ArchiveSeriesInput, CommitRecord, CreateSeriesInput, DynMemoRepository,
    ListSeriesQuery, MarkSilentSeriesInput, MemoRepository, RepositoryError, SearchSeriesQuery,
    SeriesRecord, StartupSelfHealSummary, TimelineQuery,
};

#[derive(Clone)]
pub struct ApplicationService {
    repository: DynMemoRepository,
    silent_days_threshold: u32,
}

#[derive(Clone)]
pub struct ApplicationServiceState {
    service: Arc<ApplicationService>,
    startup_self_heal: StartupSelfHealSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    NotImplemented(String),
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
            Self::NotImplemented(message) => write!(f, "not implemented: {message}"),
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
            RepositoryError::NotImplemented(message) => Self::NotImplemented(message),
            RepositoryError::Storage(message) => Self::Internal(message),
        }
    }
}

impl ApplicationServiceState {
    pub fn new(service: ApplicationService, startup_self_heal: StartupSelfHealSummary) -> Self {
        Self {
            service: Arc::new(service),
            startup_self_heal,
        }
    }

    pub fn service(&self) -> &ApplicationService {
        self.service.as_ref()
    }

    pub fn startup_self_heal(&self) -> &StartupSelfHealSummary {
        &self.startup_self_heal
    }
}

impl ApplicationService {
    pub fn new(repository: DynMemoRepository, silent_days_threshold: u32) -> Self {
        Self {
            repository,
            silent_days_threshold,
        }
    }

    pub fn repository(&self) -> &Arc<dyn MemoRepository + Send + Sync> {
        &self.repository
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
