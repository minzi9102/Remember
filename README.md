# Remember Backend Workspace

Remember is now a backend-only workspace targeting Windows local execution.

## Components
- `remember-ipc-server`: local IPC service process.
- `remember-cli`: operational CLI for health checks and RPC calls.
- `remember-core`: business contracts and application service layer.
- `remember-sqlite`: SQLite repository and migrations.

## Runtime
- Runtime mode: `sqlite_only`
- Production transport: Named Pipe (`\\.\pipe\remember-ipc-v1`)
- Debug transport: Loopback (disabled by default)

## Build
```powershell
cargo check --workspace
cargo build --workspace
```

## Run
```powershell
cargo run -p remember-ipc-server
cargo run -p remember-cli -- health
cargo run -p remember-cli -- rpc call --path series.list --payload '{"query":"","includeArchived":false,"cursor":null,"limit":20}'
```

## Environment Variables
- `REMEMBER_APPDATA_DIR`: override config/database directory.
- `REMEMBER_IPC_AUTH_TOKEN`: auth token for IPC requests.
- `REMEMBER_IPC_PIPE`: override named pipe path.
- `REMEMBER_ENABLE_LOOPBACK=1`: enable loopback transport.
- `REMEMBER_LOOPBACK_ADDR`: override loopback bind address.

## Frontend Integration
- `FRONTEND_INTEGRATION.md`: desktop-shell-first frontend integration manual.
