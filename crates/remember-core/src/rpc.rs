use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{to_value, Value};

use crate::service::{ApplicationError, ApplicationService, ApplicationServiceState};

const VALIDATION_ERROR_CODE: &str = "VALIDATION_ERROR";
const UNKNOWN_COMMAND_CODE: &str = "UNKNOWN_COMMAND";
const NOT_FOUND_CODE: &str = "NOT_FOUND";
const CONFLICT_CODE: &str = "CONFLICT";
const INTERNAL_ERROR_CODE: &str = "INTERNAL_ERROR";
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
    pub request_id: String,
    pub path: String,
    pub transport: String,
    pub responded_at_unix_ms: u128,
}

#[derive(Debug, Clone, Copy)]
enum RpcErrorKind {
    Validation,
    UnknownCommand,
    NotFound,
    Conflict,
    Internal,
}

#[derive(Debug, Clone)]
pub struct RpcInvocation {
    pub request_id: String,
    pub path: String,
    pub payload: Value,
    pub transport: String,
}

pub async fn handle_rpc(
    invocation: RpcInvocation,
    service_state: &ApplicationServiceState,
) -> RpcEnvelope {
    let dispatch_result = match resolve_forced_error(&invocation.payload) {
        Ok(Some(error)) => Err(error),
        Ok(None) => dispatch(&invocation.path, &invocation.payload, service_state.service()).await,
        Err(error) => Err(error),
    };

    match dispatch_result {
        Ok(data) => RpcEnvelope::success(&invocation, data),
        Err(error) => RpcEnvelope::failure(&invocation, error),
    }
}

impl RpcEnvelope {
    fn success(invocation: &RpcInvocation, data: Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
            meta: build_meta(invocation),
        }
    }

    fn failure(invocation: &RpcInvocation, error: RpcError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
            meta: build_meta(invocation),
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
            Self::Internal => INTERNAL_ERROR_CODE,
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

    fn internal(message: impl Into<String>) -> Self {
        Self::from_kind(RpcErrorKind::Internal, message)
    }
}

fn build_meta(invocation: &RpcInvocation) -> RpcMeta {
    RpcMeta {
        request_id: invocation.request_id.clone(),
        path: invocation.path.clone(),
        transport: invocation.transport.clone(),
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
        VALIDATION_ERROR_CODE => RpcError::validation("simulated validation error for diagnostics"),
        _ => {
            return Err(RpcError::validation(format!(
                "field `{FORCE_ERROR_CODE_FIELD}` must be one of {VALIDATION_ERROR_CODE}"
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
        ApplicationError::NotImplemented(message) => RpcError::internal(message),
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
        .ok_or_else(|| {
            RpcError::validation(format!("field `{key}` must be a non-negative integer"))
        })
}
