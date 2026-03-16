use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::application::config::RuntimeConfigState;

const VALIDATION_ERROR_CODE: &str = "VALIDATION_ERROR";
const UNKNOWN_COMMAND_CODE: &str = "UNKNOWN_COMMAND";
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
}

#[derive(Debug, Clone, Copy)]
enum RpcErrorKind {
    Validation,
    UnknownCommand,
    PgTimeout,
    DualWriteFailed,
}

#[tauri::command]
pub(crate) fn rpc_invoke(
    path: String,
    payload: Option<Value>,
    config_state: State<'_, RuntimeConfigState>,
) -> RpcEnvelope {
    handle_rpc(
        &path,
        payload.unwrap_or(Value::Null),
        config_state.inner(),
    )
}

pub(crate) fn handle_rpc(path: &str, payload: Value, config_state: &RuntimeConfigState) -> RpcEnvelope {
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
        Ok(None) => dispatch(path, &payload),
        Err(error) => Err(error),
    };

    match dispatch_result {
        Ok(data) => {
            tracing::info!(component = "rpc", path, "rpc invoke succeeded");
            RpcEnvelope::success(path, config_state, data)
        }
        Err(error) => {
            tracing::warn!(
                component = "rpc",
                path,
                code = error.code,
                message = %error.message,
                "rpc invoke failed"
            );
            RpcEnvelope::failure(path, config_state, error)
        }
    }
}

impl RpcEnvelope {
    fn success(path: &str, config_state: &RuntimeConfigState, data: Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            meta: build_meta(path, config_state),
        }
    }

    fn failure(path: &str, config_state: &RuntimeConfigState, error: RpcError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
            meta: build_meta(path, config_state),
        }
    }
}

impl RpcErrorKind {
    fn as_code(self) -> &'static str {
        match self {
            Self::Validation => VALIDATION_ERROR_CODE,
            Self::UnknownCommand => UNKNOWN_COMMAND_CODE,
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

    fn pg_timeout(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::PgTimeout, message)
    }

    fn dual_write_failed(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::DualWriteFailed, message)
    }
}

fn build_meta(path: &str, config_state: &RuntimeConfigState) -> RpcMeta {
    RpcMeta {
        path: path.to_string(),
        runtime_mode: config_state.config.runtime_mode.as_config_value().to_string(),
        used_fallback: config_state.used_fallback,
        responded_at_unix_ms: current_unix_ms(),
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

fn dispatch(path: &str, payload: &Value) -> Result<Value, RpcError> {
    match path {
        "series.create" => series_create(payload),
        "series.list" => series_list(payload),
        "commit.append" => commit_append(payload),
        "timeline.list" => timeline_list(payload),
        "series.archive" => series_archive(payload),
        "series.scan_silent" => series_scan_silent(payload),
        _ => Err(RpcError::unknown_path(path)),
    }
}

fn series_create(payload: &Value) -> Result<Value, RpcError> {
    let name = required_non_empty_string(payload, "name")?;
    Ok(json!({
        "series": {
            "id": "stub-series-inbox",
            "name": name,
            "status": "active",
            "lastUpdatedAt": "2026-03-16T00:00:00Z",
            "latestExcerpt": "stubbed-command-shell",
            "createdAt": "2026-03-16T00:00:00Z"
        }
    }))
}

fn series_list(payload: &Value) -> Result<Value, RpcError> {
    let limit = optional_positive_u64(payload, "limit")?.unwrap_or(50);
    Ok(json!({
        "items": [
            {
                "id": "series-inbox",
                "name": "Inbox",
                "status": "active",
                "lastUpdatedAt": "2026-03-16T00:00:00Z",
                "latestExcerpt": "first-note",
                "createdAt": "2026-03-15T00:00:00Z"
            }
        ],
        "nextCursor": Value::Null,
        "limitEcho": limit
    }))
}

fn commit_append(payload: &Value) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    let content = required_non_empty_string(payload, "content")?;
    let latest_excerpt = excerpt(&content);

    Ok(json!({
        "commit": {
            "id": "stub-commit-001",
            "seriesId": series_id,
            "content": content,
            "createdAt": "2026-03-16T00:00:00Z"
        },
        "series": {
            "id": series_id,
            "name": "Stub Series",
            "status": "active",
            "lastUpdatedAt": "2026-03-16T00:00:00Z",
            "latestExcerpt": latest_excerpt,
            "createdAt": "2026-03-15T00:00:00Z"
        }
    }))
}

fn timeline_list(payload: &Value) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    Ok(json!({
        "seriesId": series_id,
        "items": [
            {
                "id": "stub-commit-001",
                "seriesId": series_id,
                "content": "first-note",
                "createdAt": "2026-03-16T00:00:00Z"
            }
        ],
        "nextCursor": Value::Null
    }))
}

fn series_archive(payload: &Value) -> Result<Value, RpcError> {
    let series_id = required_non_empty_string(payload, "seriesId")?;
    Ok(json!({
        "seriesId": series_id,
        "archivedAt": "2026-03-16T00:00:00Z"
    }))
}

fn series_scan_silent(payload: &Value) -> Result<Value, RpcError> {
    let threshold_days = optional_positive_u64(payload, "thresholdDays")?.unwrap_or(7);
    Ok(json!({
        "affectedSeriesIds": [],
        "thresholdDays": threshold_days
    }))
}

fn required_non_empty_string(payload: &Value, key: &str) -> Result<String, RpcError> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            RpcError::validation(format!(
                "field `{key}` is required and must be a non-empty string"
            ))
        })
}

fn optional_positive_u64(payload: &Value, key: &str) -> Result<Option<u64>, RpcError> {
    if let Some(raw_value) = payload.as_object().and_then(|object| object.get(key)) {
        return raw_value
            .as_u64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(|| RpcError::validation(format!("field `{key}` must be a positive integer")));
    }

    Ok(None)
}

fn excerpt(content: &str) -> String {
    let mut preview: String = content.chars().take(48).collect();
    if content.chars().count() > 48 {
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        handle_rpc, DUAL_WRITE_FAILED_CODE, PG_TIMEOUT_CODE, UNKNOWN_COMMAND_CODE,
        VALIDATION_ERROR_CODE,
    };
    use crate::application::config::{AppConfig, RuntimeConfigState, RuntimeMode};

    #[test]
    fn returns_success_envelope_for_known_path() {
        let state = test_state();
        let envelope = handle_rpc("series.create", serde_json::json!({ "name": "Inbox" }), &state);

        assert!(envelope.ok);
        assert!(envelope.error.is_none());
        assert_eq!(envelope.meta.path, "series.create");
        assert_eq!(envelope.meta.runtime_mode, "sqlite_only");
        assert!(envelope.data.is_some());
    }

    #[test]
    fn returns_validation_error_for_invalid_payload() {
        let state = test_state();
        let envelope = handle_rpc("series.create", serde_json::json!({ "name": "" }), &state);

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, VALIDATION_ERROR_CODE);
    }

    #[test]
    fn returns_unknown_command_error_for_unknown_path() {
        let state = test_state();
        let envelope = handle_rpc("series.unknown", serde_json::json!({}), &state);

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, UNKNOWN_COMMAND_CODE);
    }

    #[test]
    fn returns_pg_timeout_error_when_forced() {
        let state = test_state();
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox", "__forceErrorCode": "PG_TIMEOUT" }),
            &state,
        );

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, PG_TIMEOUT_CODE);
    }

    #[test]
    fn returns_dual_write_failed_error_when_forced() {
        let state = test_state();
        let envelope = handle_rpc(
            "series.create",
            serde_json::json!({ "name": "Inbox", "__forceErrorCode": "DUAL_WRITE_FAILED" }),
            &state,
        );

        assert!(!envelope.ok);
        let error = envelope.error.expect("error should exist");
        assert_eq!(error.code, DUAL_WRITE_FAILED_CODE);
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
}
