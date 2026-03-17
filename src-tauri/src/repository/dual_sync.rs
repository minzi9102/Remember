#![allow(dead_code)]

use async_trait::async_trait;
use sqlx::{postgres::PgPool, sqlite::SqlitePool};

use super::{
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

impl DualSyncRepository {
    pub fn new(sqlite_pool: SqlitePool, postgres_pool: PgPool) -> Self {
        Self {
            sqlite: SqliteRepository::new(sqlite_pool),
            postgres: PostgresRepository::new(postgres_pool),
        }
    }
}

#[async_trait]
impl MemoRepository for DualSyncRepository {
    async fn create_series(
        &self,
        input: CreateSeriesInput,
    ) -> Result<SeriesRecord, RepositoryError> {
        let sqlite_record = self.sqlite.create_series(input.clone()).await?;
        let postgres_record = self.postgres.create_series(input).await?;

        if sqlite_record.id != postgres_record.id || sqlite_record.created_at != postgres_record.created_at {
            return Err(RepositoryError::storage(
                "dual_sync create_series produced inconsistent id/created_at between sqlite and postgres",
            ));
        }

        Ok(sqlite_record)
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
        let sqlite_result = self.sqlite.append_commit(input.clone()).await?;
        let postgres_result = self.postgres.append_commit(input).await?;

        if sqlite_result.commit.id != postgres_result.commit.id
            || sqlite_result.commit.created_at != postgres_result.commit.created_at
        {
            return Err(RepositoryError::storage(
                "dual_sync append_commit produced inconsistent commit_id/created_at between sqlite and postgres",
            ));
        }

        Ok(sqlite_result)
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
        let sqlite_result = self.sqlite.archive_series(input.clone()).await?;
        let postgres_result = self.postgres.archive_series(input).await?;

        if sqlite_result.series_id != postgres_result.series_id
            || sqlite_result.archived_at != postgres_result.archived_at
        {
            return Err(RepositoryError::storage(
                "dual_sync archive_series produced inconsistent series_id/archived_at between sqlite and postgres",
            ));
        }

        Ok(sqlite_result)
    }

    async fn mark_silent_series(
        &self,
        input: MarkSilentSeriesInput,
    ) -> Result<MarkSilentSeriesResult, RepositoryError> {
        let sqlite_result = self.sqlite.mark_silent_series(input.clone()).await?;
        let postgres_result = self.postgres.mark_silent_series(input).await?;

        if sqlite_result.affected_series_ids != postgres_result.affected_series_ids {
            return Err(RepositoryError::storage(
                "dual_sync mark_silent_series produced inconsistent affected ids between sqlite and postgres",
            ));
        }

        Ok(sqlite_result)
    }

    async fn search_series_by_name(
        &self,
        query: SearchSeriesQuery,
    ) -> Result<Vec<SeriesRecord>, RepositoryError> {
        self.sqlite.search_series_by_name(query).await
    }
}
