use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

const DEFAULT_PAGE_LIMIT: u64 = 50;
const MAX_PAGE_LIMIT: u64 = 200;
const TEST_FAILURE_INJECTION_ENV: &str = "REMEMBER_TEST_REPOSITORY_INJECT_FAILURE";

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
            completed_at: "1970-01-01T00:00:00Z".to_string(),
            messages: Vec::new(),
        }
    }
}

pub type DynMemoRepository = Arc<dyn MemoRepository + Send + Sync>;

#[async_trait]
pub trait MemoRepository: Send + Sync {
    async fn create_series(
        &self,
        input: CreateSeriesInput,
    ) -> Result<SeriesRecord, RepositoryError>;
    async fn list_series(
        &self,
        query: ListSeriesQuery,
    ) -> Result<PagedResult<SeriesRecord>, RepositoryError>;
    async fn append_commit(
        &self,
        input: AppendCommitInput,
    ) -> Result<AppendCommitResult, RepositoryError>;
    async fn list_timeline(
        &self,
        query: TimelineQuery,
    ) -> Result<PagedResult<CommitRecord>, RepositoryError>;
    async fn archive_series(
        &self,
        input: ArchiveSeriesInput,
    ) -> Result<ArchiveSeriesResult, RepositoryError>;
    async fn mark_silent_series(
        &self,
        input: MarkSilentSeriesInput,
    ) -> Result<MarkSilentSeriesResult, RepositoryError>;
    async fn search_series_by_name(
        &self,
        query: SearchSeriesQuery,
    ) -> Result<Vec<SeriesRecord>, RepositoryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesStatus {
    Active,
    Silent,
    Archived,
}

impl SeriesStatus {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Silent => "silent",
            Self::Archived => "archived",
        }
    }

    pub fn from_db_value(value: &str) -> Result<Self, RepositoryError> {
        match value {
            "active" => Ok(Self::Active),
            "silent" => Ok(Self::Silent),
            "archived" => Ok(Self::Archived),
            _ => Err(RepositoryError::storage(format!(
                "unknown series status `{value}`"
            ))),
        }
    }
}

impl fmt::Display for SeriesStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_db_value())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesRecord {
    pub id: String,
    pub name: String,
    pub status: SeriesStatus,
    pub latest_excerpt: String,
    pub last_updated_at: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub id: String,
    pub series_id: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub limit_echo: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendCommitResult {
    pub commit: CommitRecord,
    pub series: SeriesRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSeriesResult {
    pub series_id: String,
    pub archived_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkSilentSeriesResult {
    pub affected_series_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSeriesInput {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSeriesQuery {
    pub include_archived: bool,
    pub cursor: Option<String>,
    pub limit: u64,
}

impl Default for ListSeriesQuery {
    fn default() -> Self {
        Self {
            include_archived: false,
            cursor: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendCommitInput {
    pub commit_id: String,
    pub series_id: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineQuery {
    pub series_id: String,
    pub cursor: Option<String>,
    pub limit: u64,
}

impl Default for TimelineQuery {
    fn default() -> Self {
        Self {
            series_id: String::new(),
            cursor: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSeriesInput {
    pub series_id: String,
    pub archived_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkSilentSeriesInput {
    pub threshold_before: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSeriesQuery {
    pub query: String,
    pub include_archived: bool,
    pub limit: u64,
}

impl Default for SearchSeriesQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            include_archived: false,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    NotImplemented(String),
    Storage(String),
}

impl RepositoryError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::NotImplemented(message.into())
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage(message.into())
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => write!(f, "validation error: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::NotImplemented(message) => write!(f, "not implemented: {message}"),
            Self::Storage(message) => write!(f, "storage error: {message}"),
        }
    }
}

impl std::error::Error for RepositoryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorToken {
    pub timestamp: String,
    pub id: String,
}

pub fn validate_non_empty(value: &str, field: &str) -> Result<String, RepositoryError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RepositoryError::validation(format!(
            "field `{field}` must be a non-empty string"
        )));
    }

    Ok(trimmed.to_string())
}

pub fn validate_limit(limit: u64, field: &str) -> Result<u64, RepositoryError> {
    if limit == 0 || limit > MAX_PAGE_LIMIT {
        return Err(RepositoryError::validation(format!(
            "field `{field}` must be within 1..={MAX_PAGE_LIMIT}"
        )));
    }

    Ok(limit)
}

pub fn parse_cursor(cursor: Option<&str>, field: &str) -> Result<Option<CursorToken>, RepositoryError> {
    let Some(raw) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let Some((timestamp, id)) = raw.split_once('|') else {
        return Err(RepositoryError::validation(format!(
            "field `{field}` must match `timestamp|id`"
        )));
    };
    let timestamp = validate_non_empty(timestamp, field)?;
    let id = validate_non_empty(id, field)?;

    Ok(Some(CursorToken { timestamp, id }))
}

pub fn encode_cursor(timestamp: &str, id: &str) -> String {
    format!("{timestamp}|{id}")
}

pub fn excerpt(content: &str) -> String {
    let mut preview: String = content.chars().take(48).collect();
    if content.chars().count() > 48 {
        preview.push_str("...");
    }
    preview
}

pub fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::not_found("record not found"),
        sqlx::Error::Database(db_error) => map_database_error(db_error),
        other => RepositoryError::storage(format!("database operation failed: {other}")),
    }
}

pub fn maybe_inject_test_failure(
    backend: &str,
    operation: &str,
    key: &str,
) -> Result<(), RepositoryError> {
    let Ok(raw) = std::env::var(TEST_FAILURE_INJECTION_ENV) else {
        return Ok(());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    for rule in trimmed.split(';').map(str::trim).filter(|rule| !rule.is_empty()) {
        let mut parts = rule.splitn(3, '|');
        let Some(expected_backend) = parts.next() else {
            continue;
        };
        let Some(expected_operation) = parts.next() else {
            continue;
        };
        let Some(expected_key) = parts.next() else {
            continue;
        };

        if expected_backend == backend && expected_operation == operation && expected_key == key {
            return Err(RepositoryError::storage(format!(
                "injected {backend} failure for {operation} on key `{key}`"
            )));
        }
    }

    Ok(())
}

fn map_database_error(error: Box<dyn sqlx::error::DatabaseError>) -> RepositoryError {
    let message = error.message().to_string();
    let code = error.code().map(|value| value.to_string());

    if matches!(code.as_deref(), Some("23505")) || message.contains("UNIQUE constraint failed") {
        return RepositoryError::conflict(message);
    }

    if matches!(code.as_deref(), Some("23503")) || message.contains("FOREIGN KEY constraint failed")
    {
        return RepositoryError::not_found(message);
    }

    RepositoryError::storage(message)
}
