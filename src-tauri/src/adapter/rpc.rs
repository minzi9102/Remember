use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{to_value, Value};
use tauri::State;

use crate::application::{
    config::RuntimeConfigState,
    service::{ApplicationError, ApplicationService, ApplicationServiceState},
};
use crate::repository::StartupSelfHealSummary;

const VALIDATION_ERROR_CODE: &str = "VALIDATION_ERROR";
const UNKNOWN_COMMAND_CODE: &str = "UNKNOWN_COMMAND";
const NOT_FOUND_CODE: &str = "NOT_FOUND";
const CONFLICT_CODE: &str = "CONFLICT";
const NOT_IMPLEMENTED_CODE: &str = "NOT_IMPLEMENTED";
const INTERNAL_ERROR_CODE: &str = "INTERNAL_ERROR";
const PG_TIMEOUT_CODE: &str = "PG_TIMEOUT";
const DUAL_WRITE_FAILED_CODE: &str = "DUAL_WRITE_FAILED";
const FORCE_ERROR_CODE_FIELD: &str = "__forceErrorCode";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcEnvelope {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub meta: RpcMeta,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMeta {
    pub path: String,
    pub runtime_mode: String,
    pub used_fallback: bool,
    pub responded_at_unix_ms: u128,
    pub startup_self_heal: StartupSelfHealSummary,
}

#[derive(Debug, Clone, Copy)]
enum RpcErrorKind {
    Validation,
    UnknownCommand,
    NotFound,
    Conflict,
    NotImplemented,
    Internal,
    PgTimeout,
    DualWriteFailed,
}

#[tauri::command]
pub(crate) fn rpc_invoke(
    path: String,
    payload: Option<Value>,
    config_state: State<'_, RuntimeConfigState>,
    service_state: State<'_, ApplicationServiceState>,
) -> RpcEnvelope {
    tauri::async_runtime::block_on(handle_rpc(
        &path,
        payload.unwrap_or(Value::Null),
        config_state.inner(),
        service_state.inner(),
    ))
}

pub(crate) async fn handle_rpc(
    path: &str,
    payload: Value,
    config_state: &RuntimeConfigState,
    service_state: &ApplicationServiceState,
) -> RpcEnvelope {
    tracing::debug!(
        component = "rpc",
        path,
        runtime_mode = %config_state.config.runtime_mode,
        used_fallback = config_state.used_fallback,
        "rpc invoke received"
    );

    let dispatch_result = match resolve_forced_error(&payload) {
        Ok(Some(error)) => {
            tracing::warn!(
                component = "rpc",
                path,
                code = error.code,
                message = %error.message,
                "forced rpc error injected"
            );
            Err(error)
        }
        Ok(None) => dispatch(path, &payload, service_state.service()).await,
        Err(error) => Err(error),
    };

    match dispatch_result {
        Ok(data) => {
            tracing::info!(component = "rpc", path, "rpc invoke succeeded");
            RpcEnvelope::success(path, config_state, service_state, data)
        }
        Err(error) => {
            tracing::warn!(
                component = "rpc",
                path,
                code = error.code,
                message = %error.message,
                "rpc invoke failed"
            );
            RpcEnvelope::failure(path, config_state, service_state, error)
        }
    }
}

impl RpcEnvelope {
    fn success(
        path: &str,
        config_state: &RuntimeConfigState,
        service_state: &ApplicationServiceState,
        data: Value,
    ) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            meta: build_meta(path, config_state, service_state),
        }
    }

    fn failure(
        path: &str,
        config_state: &RuntimeConfigState,
        service_state: &ApplicationServiceState,
        error: RpcError,
    ) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
            meta: build_meta(path, config_state, service_state),
        }
    }
}

impl RpcErrorKind {
    fn as_code(self) -> &'static str {
        match self {
            Self::Validation => VALIDATION_ERROR_CODE,
            Self::UnknownCommand => UNKNOWN_COMMAND_CODE,
            Self::NotFound => NOT_FOUND_CODE,
            Self::Conflict => CONFLICT_CODE,
            Self::NotImplemented => NOT_IMPLEMENTED_CODE,
            Self::Internal => INTERNAL_ERROR_CODE,
            Self::PgTimeout => PG_TIMEOUT_CODE,
            Self::DualWriteFailed => DUAL_WRITE_FAILED_CODE,
        }
    }
}

impl RpcError {
    fn from_kind(kind: RpcErrorKind, message: impl Into<String>) -> Self {
        Self {
            code: kind.as_code(),
            message: message.into(),
        }
    }

    fn validation(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::Validation, message)
    }

    fn unknown_path(path: &str) -> Self {
        Self::from_kind(
            RpcErrorKind::UnknownCommand,
            format!("unknown rpc path `{path}`"),
        )
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::NotFound, message)
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::Conflict, message)
    }

    fn not_implemented(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::NotImplemented, message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::Internal, message)
    }

    fn pg_timeout(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::PgTimeout, message)
    }

    fn dual_write_failed(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::DualWriteFailed, message)
    }
}

fn build_meta(
    path: &str,
    config_state: &RuntimeConfigState,
    service_state: &ApplicationServiceState,
) -> RpcMeta {
    RpcMeta {
        path: path.to_string(),
        runtime_mode: config_state
            .config
            .runtime_mode
            .as_config_value()
            .to_string(),
        used_fallback: config_state.used_fallback,
        responded_at_unix_ms: current_unix_ms(),
        startup_self_heal: service_state.startup_self_heal().clone(),
    }
}

fn current_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn resolve_forced_error(payload: &Value) -> Result<Option<RpcError>, RpcError> {
    let Some(raw_code) = payload
        .as_object()
        .and_then(|object| object.get(FORCE_ERROR_CODE_FIELD))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let normalized = raw_code.to_ascii_uppercase();
    let forced_error = match normalized.as_str() {
        PG_TIMEOUT_CODE => RpcError::pg_timeout("simulated postgres timeout for diagnostics"),
        DUAL_WRITE_FAILED_CODE => {
            RpcError::dual_write_failed("simulated dual write failure for diagnostics")
        }
        VALIDATION_ERROR_CODE => RpcError::validation("simulated validation error for diagnostics"),
        _ => {
            return Err(RpcError::validation(format!(
                "field `{FORCE_ERROR_CODE_FIELD}` must be one of {PG_TIMEOUT_CODE}, {DUAL_WRITE_FAILED_CODE}, {VALIDATION_ERROR_CODE}"
            )))
        }
    };

    Ok(Some(forced_error))
}

async fn dispatch(
    path: &str,
    payload: &Value,
    service: &ApplicationService,
) -> Result<Value, RpcError> {
    match path {
        "series.create" => series_create(payload, service).await,
        "series.list" => series_list(payload, service).await,
        "commit.append" => commit_append(payload, service).await,
        "timeline.list" => timeline_list(payload, service).await,
        "series.archive" => series_archive(payload, service).await,
        "series.scan_silent" => series_scan_silent(payload, service).await,
        _ => Err(RpcError::unknown_path(path)),
    }
}

async fn series_create(payload: &Value, service: &ApplicationService) -> Result<Value, RpcError> {
    let name = required_non_empty_string(payload, "name")?;
    let data = service
        .create_series(name)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

async fn series_list(payload: &Value, service: &ApplicationService) -> Result<Value, RpcError> {
    let query = required_string(payload, "query")?;
    let include_archived = required_bool(payload, "includeArchived")?;
    let cursor = required_optional_string(payload, "cursor")?;
    let limit = required_positive_u64(payload, "limit")?;
    let data = service
        .list_series(query, include_archived, cursor, limit)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

async fn commit_append(payload: &Value, service: &ApplicationService) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    let content = required_non_empty_string(payload, "content")?;
    let client_ts = required_non_empty_string(payload, "clientTs")?;
    let data = service
        .append_commit(series_id, content, client_ts)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

async fn timeline_list(payload: &Value, service: &ApplicationService) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    let cursor = required_optional_string(payload, "cursor")?;
    let limit = required_positive_u64(payload, "limit")?;
    let data = service
        .list_timeline(series_id, cursor, limit)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

async fn series_archive(payload: &Value, service: &ApplicationService) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    let data = service
        .archive_series(series_id)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

async fn series_scan_silent(
    payload: &Value,
    service: &ApplicationService,
) -> Result<Value, RpcError> {
    let now = required_non_empty_string(payload, "now")?;
    let threshold_days = required_non_negative_u64(payload, "thresholdDays")?;
    let data = service
        .scan_silent(now, threshold_days)
        .await
        .map_err(map_application_error)?;
    serialize_data(data)
}

fn map_application_error(error: ApplicationError) -> RpcError {
    match error {
        ApplicationError::Validation(message) => RpcError::validation(message),
        ApplicationError::NotFound(message) => RpcError::not_found(message),
        ApplicationError::Conflict(message) => RpcError::conflict(message),
        ApplicationError::NotImplemented(message) => RpcError::not_implemented(message),
        ApplicationError::PgTimeout(message) => RpcError::pg_timeout(message),
        ApplicationError::DualWriteFailed(message) => RpcError::dual_write_failed(message),
        ApplicationError::Internal(message) => RpcError::internal(message),
    }
}

fn serialize_data<T: Serialize>(data: T) -> Result<Value, RpcError> {
    to_value(data).map_err(|error| {
        RpcError::validation(format!("failed to serialize rpc response payload: {error}"))
    })
}

fn required_string(payload: &Value, key: &str) -> Result<String, RpcError> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            RpcError::validation(format!("field `{key}` is required and must be a string"))
        })
}

fn required_non_empty_string(payload: &Value, key: &str) -> Result<String, RpcError> {
    let value = required_string(payload, key)?;
    if value.trim().is_empty() {
        return Err(RpcError::validation(format!(
            "field `{key}` is required and must be a non-empty string"
        )));
    }
    Ok(value)
}

fn required_optional_string(payload: &Value, key: &str) -> Result<Option<String>, RpcError> {
    let Some(raw_value) = payload.as_object().and_then(|object| object.get(key)) else {
        return Err(RpcError::validation(format!(
            "field `{key}` is required and must be a string or null"
        )));
    };
    if raw_value.is_null() {
        return Ok(None);
    }

    raw_value
        .as_str()
        .map(str::trim)
        .map(ToOwned::to_owned)
        .map(Some)
        .ok_or_else(|| {
            RpcError::validation(format!(
                "field `{key}` is required and must be a string or null"
            ))
        })
}

fn required_bool(payload: &Value, key: &str) -> Result<bool, RpcError> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            RpcError::validation(format!("field `{key}` is required and must be a boolean"))
        })
}

fn required_positive_u64(payload: &Value, key: &str) -> Result<u64, RpcError> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| RpcError::validation(format!("field `{key}` must be a positive integer")))
}

fn required_non_negative_u64(payload: &Value, key: &str) -> Result<u64, RpcError> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_u64)
        .ok_or_else(|| RpcError::validation(format!("field `{key}` must be a non-negative integer")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use super::{
        handle_rpc, DUAL_WRITE_FAILED_CODE, NOT_FOUND_CODE, PG_TIMEOUT_CODE, UNKNOWN_COMMAND_CODE,
        VALIDATION_ERROR_CODE,
    };
    use crate::application::{
        config::{AppConfig, RuntimeConfigState, RuntimeMode},
        service::{build_test_service_state, ApplicationError},
    };

    #[tokio::test]
    async fn returns_success_envelope_for_known_path() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox" }),
            &state,
            &service_state,
        )
        .await;

        assert!(envelope.ok);
        assert!(envelope.error.is_none());
        assert_eq!(envelope.meta.path, "series.create");
        assert_eq!(envelope.meta.runtime_mode, "sqlite_only");
        assert_eq!(envelope.meta.startup_self_heal.scanned_alerts, 0);
        let data = envelope
            .data
            .as_ref()
            .and_then(Value::as_object)
            .expect("data should be an object");
        let series = data
            .get("series")
            .and_then(Value::as_object)
            .expect("series should exist");

        assert!(series.get("id").and_then(Value::as_str).is_some());
        assert_eq!(series.get("name").and_then(Value::as_str), Some("Inbox"));
        assert_eq!(series.get("status").and_then(Value::as_str), Some("active"));
        assert!(series.contains_key("lastUpdatedAt"));
        assert!(series.contains_key("latestExcerpt"));
        assert!(series.contains_key("createdAt"));
    }

    #[tokio::test]
    async fn returns_commit_item_fields_for_commit_append() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let created = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox" }),
            &state,
            &service_state,
        )
        .await;
        let series_id = created
            .data
            .as_ref()
            .and_then(|value| value.get("series"))
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .expect("series id should exist")
            .to_string();

        let envelope = handle_rpc(
            "commit.append",
            serde_json::json!({
                "seriesId": series_id,
                "content": "first-note",
                "clientTs": "2026-03-16T10:00:00+08:00"
            }),
            &state,
            &service_state,
        )
        .await;

        assert!(envelope.ok);
        let data = envelope
            .data
            .as_ref()
            .and_then(Value::as_object)
            .expect("data should be an object");
        let commit = data
            .get("commit")
            .and_then(Value::as_object)
            .expect("commit should exist");

        assert!(commit.get("id").and_then(Value::as_str).is_some());
        assert_eq!(
            commit.get("content").and_then(Value::as_str),
            Some("first-note")
        );
        assert_eq!(
            commit.get("createdAt").and_then(Value::as_str),
            Some("2026-03-16T02:00:00Z")
        );
    }

    #[tokio::test]
    async fn returns_commit_items_for_timeline_list() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let created = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox" }),
            &state,
            &service_state,
        )
        .await;
        let series_id = created
            .data
            .as_ref()
            .and_then(|value| value.get("series"))
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .expect("series id should exist")
            .to_string();
        let _ = handle_rpc(
            "commit.append",
            serde_json::json!({
                "seriesId": series_id,
                "content": "first-note",
                "clientTs": "2026-03-16T00:00:00Z"
            }),
            &state,
            &service_state,
        )
        .await;

        let envelope = handle_rpc(
            "timeline.list",
            serde_json::json!({
                "seriesId": series_id,
                "cursor": null,
                "limit": 20
            }),
            &state,
            &service_state,
        )
        .await;

        assert!(envelope.ok);
        let data = envelope
            .data
            .as_ref()
            .and_then(Value::as_object)
            .expect("data should be an object");
        let items = data
            .get("items")
            .and_then(Value::as_array)
            .expect("items should exist");
        assert_eq!(items.len(), 1);
        let first_item = items
            .first()
            .and_then(Value::as_object)
            .expect("first timeline item should be object");
        assert!(first_item.contains_key("id"));
        assert!(first_item.contains_key("content"));
        assert!(first_item.contains_key("createdAt"));
    }

    #[tokio::test]
    async fn returns_validation_error_for_invalid_payload() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.list",
            serde_json::json!({
                "query": "",
                "includeArchived": false,
                "cursor": null,
                "limit": 0
            }),
            &state,
            &service_state,
        )
        .await;

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, VALIDATION_ERROR_CODE);
    }

    #[tokio::test]
    async fn series_scan_silent_accepts_zero_threshold_days() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let created = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Old Inbox" }),
            &state,
            &service_state,
        )
        .await;
        let series_id = created
            .data
            .as_ref()
            .and_then(|value| value.get("series"))
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .expect("series id should exist")
            .to_string();
        let _ = handle_rpc(
            "commit.append",
            serde_json::json!({
                "seriesId": series_id.clone(),
                "content": "aging note",
                "clientTs": "2026-03-01T00:00:00Z"
            }),
            &state,
            &service_state,
        )
        .await;

        let envelope = handle_rpc(
            "series.scan_silent",
            serde_json::json!({
                "now": "2026-03-16T00:00:00+08:00",
                "thresholdDays": 0
            }),
            &state,
            &service_state,
        )
        .await;

        assert!(envelope.ok);
        let data = envelope
            .data
            .as_ref()
            .and_then(Value::as_object)
            .expect("data should be an object");
        assert_eq!(data.get("thresholdDays").and_then(Value::as_u64), Some(7));
        let affected_ids = data
            .get("affectedSeriesIds")
            .and_then(Value::as_array)
            .expect("affected ids should exist");
        assert_eq!(affected_ids.len(), 1);
        assert_eq!(affected_ids[0].as_str(), Some(series_id.as_str()));
    }

    #[tokio::test]
    async fn maps_service_not_found_to_rpc_not_found() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "timeline.list",
            serde_json::json!({
                "seriesId": "missing-series",
                "cursor": null,
                "limit": 20
            }),
            &state,
            &service_state,
        )
        .await;

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, NOT_FOUND_CODE);
    }

    #[tokio::test]
    async fn returns_unknown_command_error_for_unknown_path() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.unknown",
            serde_json::json!({}),
            &state,
            &service_state,
        )
        .await;

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, UNKNOWN_COMMAND_CODE);
    }

    #[tokio::test]
    async fn returns_pg_timeout_error_when_forced() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox", "__forceErrorCode": "PG_TIMEOUT" }),
            &state,
            &service_state,
        )
        .await;

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, PG_TIMEOUT_CODE);
    }

    #[tokio::test]
    async fn returns_dual_write_failed_error_when_forced() {
        let state = test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox", "__forceErrorCode": "DUAL_WRITE_FAILED" }),
            &state,
            &service_state,
        )
        .await;

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, DUAL_WRITE_FAILED_CODE);
    }

    #[tokio::test]
    async fn dual_sync_runtime_mode_executes_commands_with_dual_meta() {
        let state = dual_sync_test_state();
        let service_state = build_test_service_state().await;
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox" }),
            &state,
            &service_state,
        )
        .await;

        assert!(envelope.ok, "dual_sync mode should execute command");
        assert_eq!(envelope.meta.runtime_mode, "dual_sync");
        assert_eq!(envelope.meta.startup_self_heal.repaired_alerts, 0);
        assert!(envelope.error.is_none());
    }

    #[test]
    fn maps_application_pg_timeout_to_rpc_pg_timeout() {
        let rpc_error = super::map_application_error(ApplicationError::PgTimeout(
            "simulated timeout".to_string(),
        ));
        assert_eq!(rpc_error.code, PG_TIMEOUT_CODE);
    }

    #[test]
    fn maps_application_dual_write_failed_to_rpc_dual_write_failed() {
        let rpc_error = super::map_application_error(ApplicationError::DualWriteFailed(
            "simulated dual write failure".to_string(),
        ));
        assert_eq!(rpc_error.code, DUAL_WRITE_FAILED_CODE);
    }

    fn test_state() -> RuntimeConfigState {
        RuntimeConfigState {
            config: AppConfig {
                runtime_mode: RuntimeMode::SqliteOnly,
                postgres_dsn: None,
                silent_days_threshold: 7,
                hotkey: "Alt+Space".to_string(),
            },
            config_path: PathBuf::from("config.toml"),
            warnings: Vec::new(),
            used_fallback: false,
        }
    }

    fn dual_sync_test_state() -> RuntimeConfigState {
        RuntimeConfigState {
            config: AppConfig {
                runtime_mode: RuntimeMode::DualSync,
                postgres_dsn: Some("postgres://configured".to_string()),
                silent_days_threshold: 7,
                hotkey: "Alt+Space".to_string(),
            },
            config_path: PathBuf::from("config.toml"),
            warnings: Vec::new(),
            used_fallback: false,
        }
    }
}
