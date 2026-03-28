use std::io;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;
use tokio::net::TcpStream;
use tokio::time::sleep;
use uuid::Uuid;

const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\remember-ipc-v1";
const DEFAULT_LOOPBACK_ADDR: &str = "127.0.0.1:18777";
const DEFAULT_AUTH_TOKEN: &str = "remember-local-dev-token";
const AUTH_ENV: &str = "REMEMBER_IPC_AUTH_TOKEN";
const PIPE_ENV: &str = "REMEMBER_IPC_PIPE";
const LOOPBACK_ADDR_ENV: &str = "REMEMBER_LOOPBACK_ADDR";
const NAMED_PIPE_RETRY_DELAYS_MS: [u64; 3] = [0, 50, 150];
const ERROR_BROKEN_PIPE: i32 = 109;
const ERROR_SEM_TIMEOUT: i32 = 121;
const ERROR_NO_DATA: i32 = 232;
const ERROR_PIPE_NOT_CONNECTED: i32 = 233;
const ERROR_PIPE_BUSY: i32 = 231;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    id: String,
    path: &'a str,
    payload: Value,
    auth_token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        bail!("missing command");
    };

    match command {
        "health" => run_health(&args[1..]).await,
        "rpc" => run_rpc(&args[1..]).await,
        _ => {
            print_usage();
            bail!("unknown command `{command}`");
        }
    }
}

async fn run_health(args: &[String]) -> Result<()> {
    let transport =
        parse_flag_value(args, "--transport").unwrap_or_else(|| "named_pipe".to_string());
    let response = invoke(
        &transport,
        "series.list",
        json!({"query":"","includeArchived":false,"cursor":null,"limit":1}),
    )
    .await?;
    let parsed: Value =
        serde_json::from_str(&response).context("failed to parse health response")?;
    let ok = parsed.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if ok {
        println!("healthy");
        return Ok(());
    }

    println!("{response}");
    bail!("service responded but health probe failed");
}

async fn run_rpc(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) != Some("call") {
        bail!("usage: remember-cli rpc call --path <rpc.path> --payload <json> [--transport named_pipe|loopback]");
    }

    let path = parse_flag_value(args, "--path").ok_or_else(|| anyhow::anyhow!("missing --path"))?;
    let payload_raw =
        parse_flag_value(args, "--payload").ok_or_else(|| anyhow::anyhow!("missing --payload"))?;
    let payload: Value = serde_json::from_str(&payload_raw)
        .with_context(|| format!("invalid payload json: {payload_raw}"))?;
    let transport =
        parse_flag_value(args, "--transport").unwrap_or_else(|| "named_pipe".to_string());

    let response = invoke(&transport, &path, payload).await?;
    println!("{response}");
    Ok(())
}

async fn invoke(transport: &str, path: &str, payload: Value) -> Result<String> {
    let auth_token = std::env::var(AUTH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_AUTH_TOKEN.to_string());
    let request = Request {
        id: Uuid::now_v7().to_string(),
        path,
        payload,
        auth_token,
    };
    let encoded = serde_json::to_string(&request).context("failed to encode request")?;

    match transport {
        "named_pipe" => invoke_named_pipe(&encoded).await,
        "loopback" => invoke_loopback(&encoded).await,
        other => bail!("unsupported transport `{other}`"),
    }
}

async fn invoke_named_pipe(encoded: &str) -> Result<String> {
    let pipe_name = std::env::var(PIPE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_PIPE_NAME.to_string());
    for (attempt_index, delay_ms) in NAMED_PIPE_RETRY_DELAYS_MS.iter().enumerate() {
        if attempt_index > 0 {
            sleep(Duration::from_millis(*delay_ms)).await;
        }

        match invoke_named_pipe_once(&pipe_name, encoded).await {
            Ok(raw_response) => {
                let response = raw_response.trim().to_string();
                if !response.is_empty() {
                    return Ok(response);
                }
                let error = anyhow!("empty named pipe response");
                if attempt_index + 1 == NAMED_PIPE_RETRY_DELAYS_MS.len() {
                    return Err(with_retry_context(error, attempt_index + 1));
                }
            }
            Err(error) => {
                if !is_retryable_named_pipe_error(&error)
                    || attempt_index + 1 == NAMED_PIPE_RETRY_DELAYS_MS.len()
                {
                    return Err(with_retry_context(error, attempt_index + 1));
                }
            }
        }
    }

    Err(anyhow!(
        "named pipe request failed after {} attempt(s); last_os_error=unknown",
        NAMED_PIPE_RETRY_DELAYS_MS.len()
    ))
}

async fn invoke_named_pipe_once(pipe_name: &str, encoded: &str) -> Result<String> {
    let mut client = ClientOptions::new()
        .open(pipe_name)
        .with_context(|| format!("failed to open named pipe {pipe_name}"))?;
    client
        .write_all(encoded.as_bytes())
        .await
        .context("failed to write named pipe request")?;
    client
        .write_all(b"\n")
        .await
        .context("failed to write named pipe newline")?;
    client
        .flush()
        .await
        .context("failed to flush named pipe request")?;

    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut client);
        reader
            .read_line(&mut line)
            .await
            .context("failed to read named pipe response")?;
    }
    Ok(line)
}

async fn invoke_loopback(encoded: &str) -> Result<String> {
    let addr = std::env::var(LOOPBACK_ADDR_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_LOOPBACK_ADDR.to_string());
    let mut stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("failed to connect loopback server at {addr}"))?;
    stream
        .write_all(encoded.as_bytes())
        .await
        .context("failed to write loopback request")?;
    stream
        .write_all(b"\n")
        .await
        .context("failed to write loopback newline")?;
    stream
        .flush()
        .await
        .context("failed to flush loopback request")?;

    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader
            .read_line(&mut line)
            .await
            .context("failed to read loopback response")?;
    }
    Ok(line.trim().to_string())
}

fn parse_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  remember-cli health [--transport named_pipe|loopback]");
    eprintln!("  remember-cli rpc call --path <rpc.path> --payload <json> [--transport named_pipe|loopback]");
}

fn is_retryable_named_pipe_error(error: &anyhow::Error) -> bool {
    matches!(
        extract_os_error_code(error),
        Some(
            ERROR_BROKEN_PIPE
                | ERROR_SEM_TIMEOUT
                | ERROR_PIPE_BUSY
                | ERROR_NO_DATA
                | ERROR_PIPE_NOT_CONNECTED
        )
    )
}

fn extract_os_error_code(error: &anyhow::Error) -> Option<i32> {
    error.chain().find_map(|source| {
        source
            .downcast_ref::<io::Error>()
            .and_then(io::Error::raw_os_error)
    })
}

fn with_retry_context(error: anyhow::Error, attempts: usize) -> anyhow::Error {
    let os_error = extract_os_error_code(&error)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    error.context(format!(
        "named pipe request failed after {attempts} attempt(s); last_os_error={os_error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_error_codes_are_classified() {
        let retryable = anyhow::Error::new(io::Error::from_raw_os_error(ERROR_PIPE_NOT_CONNECTED));
        assert!(is_retryable_named_pipe_error(&retryable));

        let non_retryable = anyhow::Error::new(io::Error::from_raw_os_error(5));
        assert!(!is_retryable_named_pipe_error(&non_retryable));
    }

    #[test]
    fn retry_context_includes_attempts_and_os_error_code() {
        let base = anyhow::Error::new(io::Error::from_raw_os_error(ERROR_PIPE_NOT_CONNECTED));
        let wrapped = with_retry_context(base, 3);
        let message = format!("{wrapped:#}");
        assert!(message.contains("attempt(s)"));
        assert!(message.contains("last_os_error=233"));
    }
}
