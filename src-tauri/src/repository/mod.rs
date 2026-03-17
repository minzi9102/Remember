#![allow(dead_code)]

pub mod migrations;
mod postgres;
mod sqlite;

use std::fmt;
use std::sync::Arc;

use crate::application::config::RuntimeMode;
use async_trait::async_trait;

#[allow(unused_imports)]
pub use postgres::PostgresRepository;
#[allow(unused_imports)]
pub use sqlite::SqliteRepository;

const DEFAULT_PAGE_LIMIT: u64 = 50;
const MAX_PAGE_LIMIT: u64 = 200;

#[derive(Debug, Clone)]
pub struct RepositoryLayer {
    runtime_mode: RuntimeMode,
}

impl RepositoryLayer {
    pub fn new(runtime_mode: RuntimeMode) -> Self {
        Self { runtime_mode }
    }

    pub fn runtime_mode(&self) -> &RuntimeMode {
        &self.runtime_mode
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
pub(crate) struct CursorToken {
    pub timestamp: String,
    pub id: String,
}

pub(crate) fn validate_non_empty(value: &str, field: &str) -> Result<String, RepositoryError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RepositoryError::validation(format!(
            "field `{field}` must be a non-empty string"
        )));
    }

    Ok(trimmed.to_string())
}

pub(crate) fn validate_limit(limit: u64, field: &str) -> Result<u64, RepositoryError> {
    if limit == 0 || limit > MAX_PAGE_LIMIT {
        return Err(RepositoryError::validation(format!(
            "field `{field}` must be within 1..={MAX_PAGE_LIMIT}"
        )));
    }

    Ok(limit)
}

pub(crate) fn parse_cursor(
    cursor: Option<&str>,
    field: &str,
) -> Result<Option<CursorToken>, RepositoryError> {
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

pub(crate) fn encode_cursor(timestamp: &str, id: &str) -> String {
    format!("{timestamp}|{id}")
}

pub(crate) fn excerpt(content: &str) -> String {
    let mut preview: String = content.chars().take(48).collect();
    if content.chars().count() > 48 {
        preview.push_str("...");
    }
    preview
}

pub(crate) fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::not_found("record not found"),
        sqlx::Error::Database(db_error) => map_database_error(db_error),
        other => RepositoryError::storage(format!("database operation failed: {other}")),
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions};

    use super::migrations::{run_postgres_migrations, run_sqlite_migrations};
    use super::{
        AppendCommitInput, ArchiveSeriesInput, CreateSeriesInput, MarkSilentSeriesInput,
        MemoRepository, RepositoryError, SearchSeriesQuery, SqliteRepository, TimelineQuery,
    };
    use crate::repository::{ListSeriesQuery, PostgresRepository, SeriesStatus};

    #[tokio::test]
    async fn sqlite_repository_contract_suite() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to connect sqlite memory db");
        run_sqlite_migrations(&pool)
            .await
            .expect("failed to run sqlite migrations");

        let repo: Arc<dyn MemoRepository + Send + Sync> = Arc::new(SqliteRepository::new(pool));
        run_repository_contract_suite(repo, format!("sqlite-{}", nonce())).await;
    }

    #[tokio::test]
    async fn postgres_repository_contract_suite_is_optional() {
        let postgres_dsn = match std::env::var("REMEMBER_TEST_POSTGRES_DSN") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                eprintln!(
                    "skip postgres repository contract test: REMEMBER_TEST_POSTGRES_DSN is not configured"
                );
                return;
            }
        };

        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&postgres_dsn)
            .await
            .expect("failed to connect postgres with REMEMBER_TEST_POSTGRES_DSN");
        run_postgres_migrations(&pool)
            .await
            .expect("failed to run postgres migrations");

        let prefix = format!("pg-{}", nonce());
        let repo: Arc<dyn MemoRepository + Send + Sync> =
            Arc::new(PostgresRepository::new(pool.clone()));
        run_repository_contract_suite(repo, prefix.clone()).await;
        cleanup_postgres_prefix(&pool, &prefix).await;
    }

    async fn run_repository_contract_suite(
        repo: Arc<dyn MemoRepository + Send + Sync>,
        prefix: String,
    ) {
        let series_a = format!("{prefix}-series-a");
        let series_b = format!("{prefix}-series-b");
        let series_c = format!("{prefix}-series-c");
        let missing_series = format!("{prefix}-missing-series");
        let commit_1 = format!("{prefix}-commit-1");
        let commit_2 = format!("{prefix}-commit-2");
        let missing_commit = format!("{prefix}-commit-missing");

        repo.create_series(CreateSeriesInput {
            id: series_a.clone(),
            name: "Inbox".to_string(),
            created_at: "2026-03-10T08:00:00Z".to_string(),
        })
        .await
        .expect("create series A should succeed");
        repo.create_series(CreateSeriesInput {
            id: series_b.clone(),
            name: "Project-A".to_string(),
            created_at: "2026-03-11T08:00:00Z".to_string(),
        })
        .await
        .expect("create series B should succeed");
        repo.create_series(CreateSeriesInput {
            id: series_c.clone(),
            name: "Dormant".to_string(),
            created_at: "2025-12-01T08:00:00Z".to_string(),
        })
        .await
        .expect("create series C should succeed");

        let first_page = repo
            .list_series(ListSeriesQuery {
                include_archived: false,
                cursor: None,
                limit: 1,
            })
            .await
            .expect("first list page should succeed");
        assert_eq!(first_page.items.len(), 1);
        assert!(first_page.next_cursor.is_some());
        assert_eq!(first_page.items[0].id, series_b);

        let second_page = repo
            .list_series(ListSeriesQuery {
                include_archived: false,
                cursor: first_page.next_cursor.clone(),
                limit: 10,
            })
            .await
            .expect("second list page should succeed");
        assert!(
            second_page.items.iter().any(|item| item.id == series_a),
            "second page should contain series A"
        );

        repo.append_commit(AppendCommitInput {
            commit_id: commit_1.clone(),
            series_id: series_a.clone(),
            content: "first note".to_string(),
            created_at: "2026-03-12T09:00:00Z".to_string(),
        })
        .await
        .expect("first commit append should succeed");
        let append_result = repo
            .append_commit(AppendCommitInput {
                commit_id: commit_2.clone(),
                series_id: series_a.clone(),
                content: "second note from repository contract".to_string(),
                created_at: "2026-03-13T10:00:00Z".to_string(),
            })
            .await
            .expect("second commit append should succeed");
        assert_eq!(append_result.commit.id, commit_2);
        assert_eq!(append_result.series.id, series_a);
        assert_eq!(append_result.series.status, SeriesStatus::Active);

        let reordered = repo
            .list_series(ListSeriesQuery {
                include_archived: false,
                cursor: None,
                limit: 10,
            })
            .await
            .expect("list series after append should succeed");
        assert_eq!(
            reordered.items.first().map(|item| item.id.as_str()),
            Some(series_a.as_str()),
            "series A should be promoted to top after append"
        );

        let timeline_page_1 = repo
            .list_timeline(TimelineQuery {
                series_id: series_a.clone(),
                cursor: None,
                limit: 1,
            })
            .await
            .expect("timeline page 1 should succeed");
        assert_eq!(timeline_page_1.items.len(), 1);
        assert_eq!(timeline_page_1.items[0].id, commit_2);
        assert!(timeline_page_1.next_cursor.is_some());

        let timeline_page_2 = repo
            .list_timeline(TimelineQuery {
                series_id: series_a.clone(),
                cursor: timeline_page_1.next_cursor.clone(),
                limit: 1,
            })
            .await
            .expect("timeline page 2 should succeed");
        assert_eq!(timeline_page_2.items.len(), 1);
        assert_eq!(timeline_page_2.items[0].id, commit_1);

        let archive_result = repo
            .archive_series(ArchiveSeriesInput {
                series_id: series_b.clone(),
                archived_at: "2026-03-14T10:00:00Z".to_string(),
            })
            .await
            .expect("archive should succeed");
        assert_eq!(archive_result.series_id, series_b);

        let list_without_archived = repo
            .list_series(ListSeriesQuery {
                include_archived: false,
                cursor: None,
                limit: 10,
            })
            .await
            .expect("list without archived should succeed");
        assert!(
            list_without_archived
                .items
                .iter()
                .all(|item| item.id != series_b),
            "archived series should be hidden in non-archived list"
        );

        let list_with_archived = repo
            .list_series(ListSeriesQuery {
                include_archived: true,
                cursor: None,
                limit: 10,
            })
            .await
            .expect("list with archived should succeed");
        let archived_item = list_with_archived
            .items
            .iter()
            .find(|item| item.id == series_b)
            .expect("archived series should appear when include_archived=true");
        assert_eq!(archived_item.status, SeriesStatus::Archived);

        let silent_result = repo
            .mark_silent_series(MarkSilentSeriesInput {
                threshold_before: "2026-01-01T00:00:00Z".to_string(),
            })
            .await
            .expect("mark silent should succeed");
        assert!(
            silent_result.affected_series_ids.contains(&series_c),
            "series C should be marked as silent"
        );

        let search_result = repo
            .search_series_by_name(SearchSeriesQuery {
                query: "inBoX".to_string(),
                include_archived: false,
                limit: 10,
            })
            .await
            .expect("search should succeed");
        assert!(
            search_result.iter().any(|item| item.id == series_a),
            "case-insensitive search should include series A"
        );

        let append_missing_error = repo
            .append_commit(AppendCommitInput {
                commit_id: missing_commit,
                series_id: missing_series.clone(),
                content: "orphan commit".to_string(),
                created_at: "2026-03-15T10:00:00Z".to_string(),
            })
            .await
            .expect_err("append on missing series should fail");
        assert!(matches!(append_missing_error, RepositoryError::NotFound(_)));

        let archive_missing_error = repo
            .archive_series(ArchiveSeriesInput {
                series_id: missing_series,
                archived_at: "2026-03-15T10:00:00Z".to_string(),
            })
            .await
            .expect_err("archive on missing series should fail");
        assert!(matches!(
            archive_missing_error,
            RepositoryError::NotFound(_)
        ));

        let invalid_limit_error = repo
            .list_series(ListSeriesQuery {
                include_archived: false,
                cursor: None,
                limit: 0,
            })
            .await
            .expect_err("limit=0 should fail validation");
        assert!(matches!(
            invalid_limit_error,
            RepositoryError::Validation(_)
        ));

        let invalid_cursor_error = repo
            .list_timeline(TimelineQuery {
                series_id: series_a.clone(),
                cursor: Some("bad-cursor".to_string()),
                limit: 10,
            })
            .await
            .expect_err("invalid cursor should fail validation");
        assert!(matches!(
            invalid_cursor_error,
            RepositoryError::Validation(_)
        ));

        let invalid_search_error = repo
            .search_series_by_name(SearchSeriesQuery {
                query: "   ".to_string(),
                include_archived: false,
                limit: 10,
            })
            .await
            .expect_err("empty search query should fail validation");
        assert!(matches!(
            invalid_search_error,
            RepositoryError::Validation(_)
        ));
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
}
