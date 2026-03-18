#![allow(dead_code)]

use async_trait::async_trait;
use sqlx::{postgres::PgPool, FromRow, Postgres, QueryBuilder};

use super::{
    encode_cursor, excerpt, map_sqlx_error, maybe_inject_test_failure, parse_cursor,
    validate_limit, validate_non_empty, AppendCommitInput, AppendCommitResult, ArchiveSeriesInput,
    ArchiveSeriesResult, CommitRecord, CreateSeriesInput, ListSeriesQuery, MarkSilentSeriesInput,
    MarkSilentSeriesResult, MemoRepository, PagedResult, RepositoryError, SearchSeriesQuery,
    SeriesRecord, SeriesStatus, TimelineQuery, POSTGRES_LOCK_TIMEOUT, POSTGRES_STATEMENT_TIMEOUT,
};

const SERIES_SELECT: &str = r#"
SELECT
    id,
    name,
    status,
    latest_excerpt,
    to_char(last_updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_updated_at,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
    CASE
        WHEN archived_at IS NULL THEN NULL
        ELSE to_char(archived_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
    END AS archived_at
FROM series
"#;

const COMMIT_SELECT: &str = r#"
SELECT
    id,
    series_id,
    content,
    to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
FROM commits
"#;

#[derive(Debug, Clone)]
pub struct PostgresRepository {
    pool: PgPool,
}

impl PostgresRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[allow(dead_code)]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, FromRow)]
struct SeriesRow {
    id: String,
    name: String,
    status: String,
    latest_excerpt: String,
    last_updated_at: String,
    created_at: String,
    archived_at: Option<String>,
}

#[derive(Debug, FromRow)]
struct CommitRow {
    id: String,
    series_id: String,
    content: String,
    created_at: String,
}

#[async_trait]
impl MemoRepository for PostgresRepository {
    async fn create_series(
        &self,
        input: CreateSeriesInput,
    ) -> Result<SeriesRecord, RepositoryError> {
        let id = validate_non_empty(&input.id, "id")?;
        let name = validate_non_empty(&input.name, "name")?;
        let created_at = validate_non_empty(&input.created_at, "createdAt")?;
        maybe_inject_test_failure("postgres", "create_series", &id)?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

        sqlx::query(
            "INSERT INTO series (
                id,
                name,
                status,
                latest_excerpt,
                last_updated_at,
                created_at,
                archived_at
            ) VALUES ($1, $2, 'active', '', $3::timestamptz, $3::timestamptz, NULL)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&created_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let select_sql = format!("{SERIES_SELECT} WHERE id = $1");
        let row: SeriesRow = sqlx::query_as(&select_sql)
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        series_from_row(row)
    }

    async fn list_series(
        &self,
        query: ListSeriesQuery,
    ) -> Result<PagedResult<SeriesRecord>, RepositoryError> {
        let limit = validate_limit(query.limit, "limit")?;
        let cursor = parse_cursor(query.cursor.as_deref(), "cursor")?;

        let mut builder = QueryBuilder::<Postgres>::new(SERIES_SELECT);
        builder.push(" WHERE 1 = 1");
        if !query.include_archived {
            builder.push(" AND status <> 'archived'");
        }
        if let Some(cursor) = &cursor {
            builder.push(" AND (last_updated_at < ");
            builder.push_bind(&cursor.timestamp);
            builder.push("::timestamptz OR (last_updated_at = ");
            builder.push_bind(&cursor.timestamp);
            builder.push("::timestamptz AND id < ");
            builder.push_bind(&cursor.id);
            builder.push("))");
        }
        builder.push(" ORDER BY last_updated_at DESC, id DESC LIMIT ");
        builder.push_bind((limit + 1) as i64);

        let rows: Vec<SeriesRow> = builder
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        let items = rows
            .into_iter()
            .map(series_from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(finalize_page(items, limit, |item| {
            encode_cursor(&item.last_updated_at, &item.id)
        }))
    }

    async fn append_commit(
        &self,
        input: AppendCommitInput,
    ) -> Result<AppendCommitResult, RepositoryError> {
        let commit_id = validate_non_empty(&input.commit_id, "commitId")?;
        let series_id = validate_non_empty(&input.series_id, "seriesId")?;
        let created_at = validate_non_empty(&input.created_at, "createdAt")?;
        if input.content.trim().is_empty() {
            return Err(RepositoryError::validation(
                "field `content` must be a non-empty string",
            ));
        }
        let content = input.content;
        let latest_excerpt = excerpt(&content);

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

        let status: Option<String> = sqlx::query_scalar("SELECT status FROM series WHERE id = $1")
            .bind(&series_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        let Some(status) = status else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist"
            )));
        };
        if status == SeriesStatus::Archived.as_db_value() {
            return Err(RepositoryError::conflict(format!(
                "series `{series_id}` is archived and cannot receive new commits"
            )));
        }
        maybe_inject_test_failure("postgres", "append_commit", &commit_id)?;

        sqlx::query(
            "INSERT INTO commits (id, series_id, content, created_at)
             VALUES ($1, $2, $3, $4::timestamptz)",
        )
        .bind(&commit_id)
        .bind(&series_id)
        .bind(&content)
        .bind(&created_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        sqlx::query(
            "UPDATE series
             SET latest_excerpt = $1, last_updated_at = $2::timestamptz, status = 'active', archived_at = NULL
             WHERE id = $3",
        )
        .bind(&latest_excerpt)
        .bind(&created_at)
        .bind(&series_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let select_sql = format!("{SERIES_SELECT} WHERE id = $1");
        let series_row: SeriesRow = sqlx::query_as(&select_sql)
            .bind(&series_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(AppendCommitResult {
            commit: CommitRecord {
                id: commit_id,
                series_id,
                content,
                created_at,
            },
            series: series_from_row(series_row)?,
        })
    }

    async fn list_timeline(
        &self,
        query: TimelineQuery,
    ) -> Result<PagedResult<CommitRecord>, RepositoryError> {
        let series_id = validate_non_empty(&query.series_id, "seriesId")?;
        let limit = validate_limit(query.limit, "limit")?;
        let cursor = parse_cursor(query.cursor.as_deref(), "cursor")?;

        let series_exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM series WHERE id = $1")
                .bind(&series_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?;
        if series_exists.is_none() {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist"
            )));
        }

        let mut builder = QueryBuilder::<Postgres>::new(COMMIT_SELECT);
        builder.push(" WHERE series_id = ");
        builder.push_bind(&series_id);
        if let Some(cursor) = &cursor {
            builder.push(" AND (created_at < ");
            builder.push_bind(&cursor.timestamp);
            builder.push("::timestamptz OR (created_at = ");
            builder.push_bind(&cursor.timestamp);
            builder.push("::timestamptz AND id < ");
            builder.push_bind(&cursor.id);
            builder.push("))");
        }
        builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
        builder.push_bind((limit + 1) as i64);

        let rows: Vec<CommitRow> = builder
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        let items = rows.into_iter().map(commit_from_row).collect();

        Ok(finalize_page(items, limit, |item| {
            encode_cursor(&item.created_at, &item.id)
        }))
    }

    async fn archive_series(
        &self,
        input: ArchiveSeriesInput,
    ) -> Result<ArchiveSeriesResult, RepositoryError> {
        let series_id = validate_non_empty(&input.series_id, "seriesId")?;
        let archived_at = validate_non_empty(&input.archived_at, "archivedAt")?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

        let existing: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT
                status,
                CASE
                    WHEN archived_at IS NULL THEN NULL
                    ELSE to_char(archived_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
                END AS archived_at
             FROM series
             WHERE id = $1",
        )
        .bind(&series_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        let Some((status, existing_archived_at)) = existing else {
            return Err(RepositoryError::not_found(format!(
                "series `{series_id}` does not exist"
            )));
        };

        let effective_archived_at = if status == SeriesStatus::Archived.as_db_value()
            && existing_archived_at.is_some()
        {
            existing_archived_at.expect("checked is_some")
        } else {
            maybe_inject_test_failure("postgres", "archive_series", &series_id)?;
            sqlx::query(
                    "UPDATE series
                     SET status = 'archived', archived_at = $1::timestamptz, last_updated_at = $1::timestamptz
                     WHERE id = $2",
                )
                .bind(&archived_at)
                .bind(&series_id)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_error)?;
            archived_at
        };

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(ArchiveSeriesResult {
            series_id,
            archived_at: effective_archived_at,
        })
    }

    async fn mark_silent_series(
        &self,
        input: MarkSilentSeriesInput,
    ) -> Result<MarkSilentSeriesResult, RepositoryError> {
        let threshold_before = validate_non_empty(&input.threshold_before, "thresholdBefore")?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;
        apply_postgres_tx_deadlines(&mut tx).await?;

        let affected_series_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id
             FROM series
             WHERE status = 'active'
               AND last_updated_at < $1::timestamptz
             ORDER BY id",
        )
        .bind(&threshold_before)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;
        maybe_inject_test_failure("postgres", "mark_silent_series", &threshold_before)?;

        if !affected_series_ids.is_empty() {
            let mut builder =
                QueryBuilder::<Postgres>::new("UPDATE series SET status = 'silent' WHERE id IN (");
            {
                let mut separated = builder.separated(", ");
                for id in &affected_series_ids {
                    separated.push_bind(id);
                }
            }
            builder.push(")");
            builder
                .build()
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_error)?;
        }

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(MarkSilentSeriesResult {
            affected_series_ids,
        })
    }

    async fn search_series_by_name(
        &self,
        query: SearchSeriesQuery,
    ) -> Result<Vec<SeriesRecord>, RepositoryError> {
        let pattern_source = validate_non_empty(&query.query, "query")?;
        let limit = validate_limit(query.limit, "limit")?;
        let pattern = format!("%{pattern_source}%");

        let mut builder = QueryBuilder::<Postgres>::new(SERIES_SELECT);
        builder.push(" WHERE LOWER(name) LIKE LOWER(");
        builder.push_bind(pattern);
        builder.push(")");
        if !query.include_archived {
            builder.push(" AND status <> 'archived'");
        }
        builder.push(" ORDER BY last_updated_at DESC, id DESC LIMIT ");
        builder.push_bind(limit as i64);

        let rows: Vec<SeriesRow> = builder
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        rows.into_iter().map(series_from_row).collect()
    }
}

fn series_from_row(row: SeriesRow) -> Result<SeriesRecord, RepositoryError> {
    Ok(SeriesRecord {
        id: row.id,
        name: row.name,
        status: SeriesStatus::from_db_value(&row.status)?,
        latest_excerpt: row.latest_excerpt,
        last_updated_at: row.last_updated_at,
        created_at: row.created_at,
        archived_at: row.archived_at,
    })
}

fn commit_from_row(row: CommitRow) -> CommitRecord {
    CommitRecord {
        id: row.id,
        series_id: row.series_id,
        content: row.content,
        created_at: row.created_at,
    }
}

pub(crate) async fn apply_postgres_tx_deadlines(
    tx: &mut sqlx::Transaction<'_, Postgres>,
) -> Result<(), RepositoryError> {
    set_local_timeout(tx, "statement_timeout", POSTGRES_STATEMENT_TIMEOUT).await?;
    set_local_timeout(tx, "lock_timeout", POSTGRES_LOCK_TIMEOUT).await?;
    Ok(())
}

async fn set_local_timeout(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    setting_name: &str,
    setting_value: &str,
) -> Result<(), RepositoryError> {
    let sql = format!("SET LOCAL {setting_name} = '{setting_value}'");
    sqlx::query(&sql)
        .execute(&mut **tx)
        .await
        .map_err(map_sqlx_error)?;
    Ok(())
}

async fn fetch_series_by_id(pool: &PgPool, id: &str) -> Result<SeriesRecord, RepositoryError> {
    let select_sql = format!("{SERIES_SELECT} WHERE id = $1");
    let row: SeriesRow = sqlx::query_as(&select_sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(map_sqlx_error)?;

    series_from_row(row)
}

fn finalize_page<T, F>(mut items: Vec<T>, limit: u64, cursor_of: F) -> PagedResult<T>
where
    F: Fn(&T) -> String,
{
    let has_more = items.len() > limit as usize;
    if has_more {
        items.pop();
    }
    let next_cursor = if has_more {
        items.last().map(cursor_of)
    } else {
        None
    };

    PagedResult {
        items,
        next_cursor,
        limit_echo: limit,
    }
}
