use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::SqlitePoolOptions;
use tauri_app_lib::repository;
use tauri_app_lib::repository::migrations::run_sqlite_migrations;
use tauri_app_lib::repository::{
    AppendCommitInput, ArchiveSeriesInput, CreateSeriesInput, ListSeriesQuery,
    MarkSilentSeriesInput, MemoRepository, RepositoryError, SearchSeriesQuery, TimelineQuery,
};

#[tokio::test]
async fn p2_t5_sqlite_basic_read_write_query() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p2-t5");
    run_sqlite_migrations(&pool)
        .await
        .expect("failed to run sqlite migrations for p2-t5");

    let repo: Arc<dyn MemoRepository + Send + Sync> =
        Arc::new(repository::SqliteRepository::new(pool));
    run_p2_t5_suite(repo, format!("p2t5-sqlite-{}", nonce())).await;
}

#[tokio::test]
async fn p2_t5_sqlite_exercises_read_write_query_flow() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db for p2-t5 flow");
    run_sqlite_migrations(&pool)
        .await
        .expect("failed to run sqlite migrations for p2-t5 flow");

    let repo: Arc<dyn MemoRepository + Send + Sync> =
        Arc::new(repository::SqliteRepository::new(pool));
    run_p2_t5_suite(repo, format!("p2t5-flow-{}", nonce())).await;
}

async fn run_p2_t5_suite(repo: Arc<dyn MemoRepository + Send + Sync>, prefix: String) {
    let series_inbox = format!("{prefix}-series-inbox");
    let series_project = format!("{prefix}-series-project");
    let series_archive = format!("{prefix}-series-archive");
    let missing_series = format!("{prefix}-series-missing");
    let commit_1 = format!("{prefix}-commit-1");
    let commit_2 = format!("{prefix}-commit-2");
    let missing_commit = format!("{prefix}-commit-missing");

    repo.create_series(CreateSeriesInput {
        id: series_inbox.clone(),
        name: "Inbox".to_string(),
        created_at: "2099-03-10T08:00:00Z".to_string(),
    })
    .await
    .expect("create inbox should succeed");
    repo.create_series(CreateSeriesInput {
        id: series_project.clone(),
        name: "Project-A".to_string(),
        created_at: "2099-03-11T08:00:00Z".to_string(),
    })
    .await
    .expect("create project should succeed");
    repo.create_series(CreateSeriesInput {
        id: series_archive.clone(),
        name: "Archive-Me".to_string(),
        created_at: "2099-03-09T08:00:00Z".to_string(),
    })
    .await
    .expect("create archive-me should succeed");

    let list_page_1 = repo
        .list_series(ListSeriesQuery {
            include_archived: false,
            cursor: None,
            limit: 1,
        })
        .await
        .expect("list page 1 should succeed");
    assert_eq!(list_page_1.items.len(), 1);
    assert_eq!(list_page_1.items[0].id, series_project);
    assert!(list_page_1.next_cursor.is_some());

    let list_page_2 = repo
        .list_series(ListSeriesQuery {
            include_archived: false,
            cursor: list_page_1.next_cursor.clone(),
            limit: 10,
        })
        .await
        .expect("list page 2 should succeed");
    assert!(
        list_page_2.items.iter().any(|item| item.id == series_inbox),
        "list pagination should include inbox in page 2"
    );

    repo.append_commit(AppendCommitInput {
        commit_id: commit_1.clone(),
        series_id: series_inbox.clone(),
        content: "first-note".to_string(),
        created_at: "2099-03-12T09:00:00Z".to_string(),
    })
    .await
    .expect("append first commit should succeed");
    let append_result = repo
        .append_commit(AppendCommitInput {
            commit_id: commit_2.clone(),
            series_id: series_inbox.clone(),
            content: "follow-up-note for p2-t5 coverage".to_string(),
            created_at: "2099-03-13T10:00:00Z".to_string(),
        })
        .await
        .expect("append second commit should succeed");
    assert_eq!(append_result.commit.id, commit_2);
    assert_eq!(append_result.series.id, series_inbox);
    assert_eq!(
        append_result.series.latest_excerpt,
        "follow-up-note for p2-t5 coverage"
    );

    let reordered = repo
        .list_series(ListSeriesQuery {
            include_archived: false,
            cursor: None,
            limit: 10,
        })
        .await
        .expect("reordered list should succeed");
    assert_eq!(
        reordered.items.first().map(|item| item.id.as_str()),
        Some(series_inbox.as_str()),
        "after append, inbox should be promoted to list top"
    );

    let timeline_page_1 = repo
        .list_timeline(TimelineQuery {
            series_id: series_inbox.clone(),
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
            series_id: series_inbox.clone(),
            cursor: timeline_page_1.next_cursor.clone(),
            limit: 1,
        })
        .await
        .expect("timeline page 2 should succeed");
    assert_eq!(timeline_page_2.items.len(), 1);
    assert_eq!(timeline_page_2.items[0].id, commit_1);

    let archived = repo
        .archive_series(ArchiveSeriesInput {
            series_id: series_archive.clone(),
            archived_at: "2099-03-14T10:00:00Z".to_string(),
        })
        .await
        .expect("archive should succeed");
    assert_eq!(archived.series_id, series_archive);

    let without_archived = repo
        .list_series(ListSeriesQuery {
            include_archived: false,
            cursor: None,
            limit: 10,
        })
        .await
        .expect("list without archived should succeed");
    assert!(
        without_archived
            .items
            .iter()
            .all(|item| item.id != series_archive),
        "archived series should be hidden when include_archived=false"
    );

    let with_archived = repo
        .list_series(ListSeriesQuery {
            include_archived: true,
            cursor: None,
            limit: 10,
        })
        .await
        .expect("list with archived should succeed");
    assert!(
        with_archived
            .items
            .iter()
            .any(|item| item.id == series_archive),
        "archived series should be present when include_archived=true"
    );

    let search = repo
        .search_series_by_name(SearchSeriesQuery {
            query: "inBoX".to_string(),
            include_archived: false,
            limit: 10,
        })
        .await
        .expect("search should succeed");
    assert!(
        search.iter().any(|item| item.id == series_inbox),
        "search should be case-insensitive for series name"
    );

    let marked = repo
        .mark_silent_series(MarkSilentSeriesInput {
            threshold_before: "2099-03-01T00:00:00Z".to_string(),
        })
        .await
        .expect("mark silent should succeed");
    assert!(
        marked.affected_series_ids.is_empty(),
        "none should become silent with this threshold in p2-t5 dataset"
    );

    let append_missing_error = repo
        .append_commit(AppendCommitInput {
            commit_id: missing_commit,
            series_id: missing_series.clone(),
            content: "orphan commit".to_string(),
            created_at: "2099-03-15T10:00:00Z".to_string(),
        })
        .await
        .expect_err("append for missing series should fail");
    assert!(matches!(append_missing_error, RepositoryError::NotFound(_)));

    let archive_missing_error = repo
        .archive_series(ArchiveSeriesInput {
            series_id: missing_series,
            archived_at: "2099-03-15T10:00:00Z".to_string(),
        })
        .await
        .expect_err("archive for missing series should fail");
    assert!(matches!(
        archive_missing_error,
        RepositoryError::NotFound(_)
    ));

    let invalid_list_limit_error = repo
        .list_series(ListSeriesQuery {
            include_archived: false,
            cursor: None,
            limit: 0,
        })
        .await
        .expect_err("limit=0 should fail");
    assert!(matches!(
        invalid_list_limit_error,
        RepositoryError::Validation(_)
    ));

    let invalid_timeline_cursor_error = repo
        .list_timeline(TimelineQuery {
            series_id: series_inbox,
            cursor: Some("invalid-cursor-format".to_string()),
            limit: 10,
        })
        .await
        .expect_err("invalid timeline cursor should fail");
    assert!(matches!(
        invalid_timeline_cursor_error,
        RepositoryError::Validation(_)
    ));

    let invalid_search_error = repo
        .search_series_by_name(SearchSeriesQuery {
            query: " ".to_string(),
            include_archived: false,
            limit: 10,
        })
        .await
        .expect_err("empty search query should fail");
    assert!(matches!(
        invalid_search_error,
        RepositoryError::Validation(_)
    ));
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_nanos()
}
