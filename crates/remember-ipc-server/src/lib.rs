use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use remember_core::config::RuntimeConfigState;
use remember_core::repository::{DynMemoRepository, StartupSelfHealSummary};
use remember_core::rpc::{handle_rpc, RpcEnvelope, RpcError, RpcInvocation, RpcMeta};
use remember_core::service::{ApplicationService, ApplicationServiceState};
use remember_sqlite::{connect_pool, migrations::run_sqlite_migrations, SqliteRepository};
use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing_subscriber::{fmt, EnvFilter};

const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\remember-ipc-v1";
const DEFAULT_LOOPBACK_ADDR: &str = "127.0.0.1:18777";
const DEFAULT_AUTH_TOKEN: &str = "remember-local-dev-token";
const AUTH_ENV: &str = "REMEMBER_IPC_AUTH_TOKEN";
const PIPE_ENV: &str = "REMEMBER_IPC_PIPE";
const LOOPBACK_ENABLE_ENV: &str = "REMEMBER_ENABLE_LOOPBACK";
const LOOPBACK_ADDR_ENV: &str = "REMEMBER_LOOPBACK_ADDR";

#[derive(Clone)]
struct ServerState {
    service_state: ApplicationServiceState,
    auth_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IpcRequest {
    id: String,
    path: String,
    #[serde(default)]
    payload: Value,
    auth_token: String,
}

pub async fn run() -> Result<()> {
    init_tracing();
    let config_state = RuntimeConfigState::load();
    for warning in &config_state.warnings {
        tracing::warn!(component = "config", warning = %warning, "runtime config warning");
    }

    let pool = connect_pool(&config_state.database_path)
        .await
        .with_context(|| {
            format!(
                "failed to connect sqlite database {}",
                config_state.database_path.display()
            )
        })?;
    run_sqlite_migrations(&pool)
        .await
        .context("failed to run sqlite migrations")?;

    let repository: DynMemoRepository = Arc::new(SqliteRepository::new(pool));
    let service = ApplicationService::new(repository, config_state.config.silent_days_threshold);
    let service_state = ApplicationServiceState::new(service, StartupSelfHealSummary::clean());

    let pipe_name = std::env::var(PIPE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PIPE_NAME.to_string());
    let auth_token = std::env::var(AUTH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_AUTH_TOKEN.to_string());
    let state = Arc::new(ServerState {
        service_state,
        auth_token,
    });

    let loopback_handle = maybe_spawn_loopback(state.clone()).await?;
    tracing::info!(
        component = "server",
        pipe = %pipe_name,
        loopback_enabled = loopback_handle.is_some(),
        database = %config_state.database_path.display(),
        "ipc server started"
    );

    let mut named_pipe_task = tokio::spawn(run_named_pipe_server(pipe_name.clone(), state));
    let mut loopback_handle = loopback_handle;
    tokio::select! {
        result = &mut named_pipe_task => {
            result??;
        }
        signal = tokio::signal::ctrl_c() => {
            match signal {
                Ok(()) => tracing::info!(component = "server", "shutdown signal received"),
                Err(error) => tracing::warn!(component = "server", error = %error, "failed to listen for shutdown signal"),
            }
            named_pipe_task.abort();
            if let Some(handle) = loopback_handle.take() {
                handle.abort();
            }
        }
    }

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .json()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_current_span(false)
        .with_span_list(false)
        .try_init();
}

async fn maybe_spawn_loopback(state: Arc<ServerState>) -> Result<Option<JoinHandle<()>>> {
    let enabled = std::env::var(LOOPBACK_ENABLE_ENV)
        .ok()
        .map(|value| value == "1")
        .unwrap_or(false);
    if !enabled {
        return Ok(None);
    }

    let addr = std::env::var(LOOPBACK_ADDR_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOOPBACK_ADDR.to_string());
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind loopback listener on {addr}"))?;
    tracing::info!(component = "server", addr = %addr, "loopback transport enabled");

    let handle = tokio::spawn(async move {
        loop {
            let accept = listener.accept().await;
            let (mut stream, remote) = match accept {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(component = "server", error = %error, "loopback accept failed");
                    continue;
                }
            };
            let state = state.clone();
            tokio::spawn(async move {
                let mut line = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    match reader.read_line(&mut line).await {
                        Ok(0) => return,
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(component = "server", error = %error, peer = %remote, "loopback read failed");
                            return;
                        }
                    }
                }
                let response = process_request_line(&line, "loopback", &state).await;
                if let Err(error) = stream.write_all(response.as_bytes()).await {
                    tracing::warn!(component = "server", error = %error, peer = %remote, "loopback write failed");
                    return;
                }
                let _ = stream.write_all(b"\n").await;
            });
        }
    });

    Ok(Some(handle))
}

async fn run_named_pipe_server(pipe_name: String, state: Arc<ServerState>) -> Result<()> {
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)
        .with_context(|| format!("failed to create named pipe server at {pipe_name}"))?;

    loop {
        server.connect().await.with_context(|| {
            format!("failed to accept named pipe client on {pipe_name}")
        })?;

        let next = ServerOptions::new()
            .create(&pipe_name)
            .with_context(|| format!("failed to rotate named pipe listener on {pipe_name}"))?;
        let connected = std::mem::replace(&mut server, next);
        let state = state.clone();

        tokio::spawn(async move {
            if let Err(error) = handle_named_pipe_client(connected, state).await {
                tracing::warn!(component = "server", error = %error, "named pipe client handling failed");
            }
        });
    }
}

async fn handle_named_pipe_client(mut pipe: NamedPipeServer, state: Arc<ServerState>) -> Result<()> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut pipe);
        let bytes = reader
            .read_line(&mut line)
            .await
            .context("failed to read named pipe request")?;
        if bytes == 0 {
            return Ok(());
        }
    }

    let response = process_request_line(&line, "named_pipe", &state).await;
    pipe.write_all(response.as_bytes())
        .await
        .context("failed to write named pipe response")?;
    pipe.write_all(b"\n")
        .await
        .context("failed to write named pipe newline")?;
    pipe.flush()
        .await
        .context("failed to flush named pipe response")?;
    let _ = pipe.disconnect();
    Ok(())
}

async fn process_request_line(line: &str, transport: &str, state: &ServerState) -> String {
    let parsed = serde_json::from_str::<IpcRequest>(line.trim());
    let request = match parsed {
        Ok(value) => value,
        Err(error) => {
            let envelope = rpc_error_envelope(
                "invalid-request".to_string(),
                "invalid.request".to_string(),
                transport.to_string(),
                "VALIDATION_ERROR",
                format!("invalid request json: {error}"),
            );
            return serialize_envelope(&envelope);
        }
    };

    if request.auth_token != state.auth_token {
        let envelope = rpc_error_envelope(
            request.id,
            request.path,
            transport.to_string(),
            "VALIDATION_ERROR",
            "invalid auth token".to_string(),
        );
        return serialize_envelope(&envelope);
    }

    let invocation = RpcInvocation {
        request_id: request.id,
        path: request.path,
        payload: request.payload,
        transport: transport.to_string(),
    };
    let envelope = handle_rpc(invocation, &state.service_state).await;
    serialize_envelope(&envelope)
}

fn rpc_error_envelope(
    request_id: String,
    path: String,
    transport: String,
    code: &'static str,
    message: String,
) -> RpcEnvelope {
    RpcEnvelope {
        ok: false,
        data: None,
        error: Some(RpcError { code, message }),
        meta: RpcMeta {
            request_id,
            path,
            transport,
            responded_at_unix_ms: now_unix_ms(),
        },
    }
}

fn serialize_envelope(envelope: &RpcEnvelope) -> String {
    match serde_json::to_string(envelope) {
        Ok(value) => value,
        Err(error) => format!(
            "{{\"ok\":false,\"data\":null,\"error\":{{\"code\":\"INTERNAL_ERROR\",\"message\":\"failed to serialize response: {}\"}},\"meta\":{{\"requestId\":\"serialization-error\",\"path\":\"internal.serialize\",\"transport\":\"internal\",\"respondedAtUnixMs\":{}}}}}",
            escape_json_error(&error.to_string()),
            now_unix_ms()
        ),
    }
}

fn escape_json_error(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
