use std::sync::Arc;

use remember_core::repository::{DynMemoRepository, StartupSelfHealSummary};
use remember_core::rpc::{handle_rpc, RpcInvocation};
use remember_core::service::{ApplicationService, ApplicationServiceState};
use remember_sqlite::{migrations::run_sqlite_migrations, SqliteRepository};
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;

#[tokio::test]
async fn rpc_contract_keeps_six_paths_semantics() {
    let service_state = build_service_state().await;

    let create = invoke(
        &service_state,
        "series.create",
        serde_json::json!({ "name": "Inbox" }),
    )
    .await;
    assert!(create.ok);
    let series_id = create
        .data
        .as_ref()
        .and_then(|value| value.get("series"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .expect("series id should exist")
        .to_string();

    let list = invoke(
        &service_state,
        "series.list",
        serde_json::json!({
            "query": "",
            "includeArchived": false,
            "cursor": null,
            "limit": 20
        }),
    )
    .await;
    assert!(list.ok);

    let append = invoke(
        &service_state,
        "commit.append",
        serde_json::json!({
            "seriesId": series_id,
            "content": "first note",
            "clientTs": "2026-03-16T10:00:00+08:00"
        }),
    )
    .await;
    assert!(append.ok);

    let series_id = append
        .data
        .as_ref()
        .and_then(|value| value.get("series"))
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .expect("series id should exist")
        .to_string();

    let timeline = invoke(
        &service_state,
        "timeline.list",
        serde_json::json!({
            "seriesId": series_id,
            "cursor": null,
            "limit": 20
        }),
    )
    .await;
    assert!(timeline.ok);

    let scan = invoke(
        &service_state,
        "series.scan_silent",
        serde_json::json!({
            "now": "2026-03-20T10:00:00+08:00",
            "thresholdDays": 7
        }),
    )
    .await;
    assert!(scan.ok);

    let archive = invoke(
        &service_state,
        "series.archive",
        serde_json::json!({
            "seriesId": series_id
        }),
    )
    .await;
    assert!(archive.ok);
}

#[tokio::test]
async fn rpc_contract_reports_unknown_command() {
    let service_state = build_service_state().await;
    let envelope = invoke(&service_state, "series.unknown", serde_json::json!({})).await;
    assert!(!envelope.ok);
    assert_eq!(
        envelope.error.as_ref().map(|error| error.code),
        Some("UNKNOWN_COMMAND")
    );
}

async fn invoke(
    service_state: &ApplicationServiceState,
    path: &str,
    payload: Value,
) -> remember_core::rpc::RpcEnvelope {
    handle_rpc(
        RpcInvocation {
            request_id: Uuid::now_v7().to_string(),
            path: path.to_string(),
            payload,
            transport: "named_pipe".to_string(),
        },
        service_state,
    )
    .await
}

async fn build_service_state() -> ApplicationServiceState {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("failed to connect sqlite memory db");
    run_sqlite_migrations(&pool)
        .await
        .expect("failed to run sqlite migrations");
    let repository: DynMemoRepository = Arc::new(SqliteRepository::new(pool));
    let service = ApplicationService::new(repository, 7);
    ApplicationServiceState::new(service, StartupSelfHealSummary::clean())
}
