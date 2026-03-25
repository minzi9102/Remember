# Remember 后端技术定义（Rust Core + Local IPC）

## Summary
- 本项目技术承诺固定为 `Rust Core + Local IPC`，不承诺任何壳层或前端实现（含 WinUI/Tauri/React）。
- 对外业务语义保持稳定：RPC 路径、DTO 含义、错误码语义保持不变。
- 传输入口固定为本地 IPC；当前不提供 HTTP API，不提供远程网络访问能力。
- 运行模式固定为 `sqlite_only`，不支持运行时后端切换。

## 文档边界
- 本文档只定义后端项目事实与契约：
  - 核心业务模块
  - IPC 传输与诊断元数据
  - SQLite 持久化与约束
- 客户端（桌面壳、Web UI、桥接层）属于外部项目，不在本定义承诺范围。

## 发布与回滚文档入口
- 发布总手册：`RELEASE.md`
- 发布配置与排障门禁定义：`qa-gates/phase-5/p5-t31-release-config-troubleshooting.md`
- 发布清单与回滚门禁定义：`qa-gates/phase-5/p5-t32-release-gate-rollback.md`
- Codex 执行版门禁定义：
  - `qa-gates-codex/phase-5/p5-t31-release-config-troubleshooting.md`
  - `qa-gates-codex/phase-5/p5-t32-release-gate-rollback.md`

## 后端实现快照
- 语言与运行时：`Rust`、`tokio`、`serde`、`tracing`、`uuid v7`、`chrono`。
- 持久化：`sqlx` 启用 `sqlite / migrate / macros / runtime-tokio`，当前唯一仓储实现为 `SQLiteRepository`。
- 运行形态：单机本地数据库文件 `remember.sqlite3`，不依赖外部数据库服务。

## 模块边界（固定）
- `remember-core`：业务用例、参数校验、DTO、错误语义、`MemoRepository` trait。
- `remember-ipc-server`：本地 IPC 监听、鉴权、路由分发、请求生命周期与诊断元数据生成。
- `repository-sqlite`：`MemoRepository` 的 SQLite 实现与迁移执行。

## 运行时真相

### 运行模式
- 启动后固定运行于 `sqlite_only`。
- 旧配置中的 `runtime_mode` 即使存在，也仅产生 warning，不改变实际运行路径。

### 传输入口
- 本地 IPC 为唯一承诺入口。
- 生产通道：`Named Pipe`。
- 调试通道：`Loopback`（仅开发诊断使用，默认关闭）。
- `invoke("rpc_invoke")` 不在本技术定义的兼容承诺中。

### 配置解析
- 配置文件名固定为 `config.toml`。
- 若环境变量 `REMEMBER_APPDATA_DIR` 存在且目录可创建，配置路径为 `<override>/config.toml`。
- 否则使用平台 app data 目录下的 `config.toml`。
- 若平台目录不可解析，则回退到当前工作目录中的 `config.toml`。

### 当前有效配置项
| Key | 类型 | 默认值 | 说明 |
|---|---|---:|---|
| `hotkey` | string | `Alt+Space` | 供外部接入层读取的唤醒热键配置 |
| `silent_days_threshold` | u32 | `7` | 沉寂判定阈值 |

### 兼容保留字段
| Key | 当前行为 |
|---|---|
| `runtime_mode` | 兼容读取并输出 warning，不影响实际运行 |
| `postgres_dsn` | 兼容读取并输出 warning，不影响实际运行 |

### SQLite 数据库路径
- 若 `REMEMBER_APPDATA_DIR` 存在且目录可创建，数据库路径为 `<override>/remember.sqlite3`。
- 否则使用平台 app data 目录下的 `remember.sqlite3`。
- 若平台目录不可用，则回退到当前工作目录下的 `remember.sqlite3`。

## 公共契约

### 业务 RPC 路径（语义保持不变）
| Path | Request | Response(data) |
|---|---|---|
| `series.create` | `{ "name": string }` | `{ "series": SeriesSummary }` |
| `series.list` | `{ "query": string, "includeArchived": boolean, "cursor": string \| null, "limit": number }` | `{ "items": SeriesSummary[], "nextCursor": string \| null, "limitEcho": number }` |
| `commit.append` | `{ "seriesId": string, "content": string, "clientTs": string }` | `{ "commit": CommitItem, "series": SeriesSummary }` |
| `timeline.list` | `{ "seriesId": string, "cursor": string \| null, "limit": number }` | `{ "seriesId": string, "items": CommitItem[], "nextCursor": string \| null }` |
| `series.archive` | `{ "seriesId": string }` | `{ "seriesId": string, "archivedAt": string }` |
| `series.scan_silent` | `{ "now": string, "thresholdDays": number }` | `{ "affectedSeriesIds": string[], "thresholdDays": number }` |

### IPC v1 请求包
```json
{
  "id": "string",
  "path": "series.list",
  "payload": {},
  "authToken": "string"
}
```

### IPC v1 响应 Envelope
- 固定外壳：`{ ok, data, error, meta }`。
- `meta` 仅定义 IPC 元数据，不承诺历史 runtime 字段。

```json
{
  "ok": true,
  "data": {},
  "error": null,
  "meta": {
    "requestId": "string",
    "path": "series.list",
    "transport": "named_pipe",
    "respondedAtUnixMs": 0
  }
}
```

### 错误码语义（保持不变）
- `VALIDATION_ERROR`
- `NOT_FOUND`
- `CONFLICT`
- `UNKNOWN_COMMAND`
- `INTERNAL_ERROR`

### DTO 与状态
- `SeriesStatus`: `active | silent | archived`
- `SeriesSummary`: `id / name / status / lastUpdatedAt / latestExcerpt / createdAt / archivedAt?`
- `CommitItem`: `id / seriesId / content / createdAt`
- `SeriesListData`: `items / nextCursor / limitEcho`
- `TimelineListData`: `seriesId / items / nextCursor`
- `SeriesScanSilentData`: `affectedSeriesIds / thresholdDays`

### Repository 契约
`MemoRepository` 固定方法集：
- `create_series`
- `list_series`
- `append_commit`
- `list_timeline`
- `archive_series`
- `mark_silent_series`
- `search_series_by_name`

## SQLite Schema

### 核心表
| Table | Fields | 说明 |
|---|---|---|
| `series` | `id`, `name`, `status`, `latest_excerpt`, `last_updated_at`, `created_at`, `archived_at` | Series 主表；状态受 `active/silent/archived` 约束 |
| `commits` | `id`, `series_id`, `content`, `created_at` | 不可变 Commit 时间线 |
| `app_settings` | `key`, `value` | 预留应用级键值配置 |

### 索引
- `idx_series_last_updated_at`
- `idx_commits_series_created_at`

### 规则与限制
- `series.status` 仅允许 `active / silent / archived`。
- `commits.series_id` 外键指向 `series.id`。
- 已归档 Series 会在仓储层拒绝追加 Commit。
- 搜索仅按 `series.name` 执行。
- 分页排序以 `last_updated_at DESC, id DESC` 或 `created_at DESC, id DESC` 为准。

## 遗留结构说明

### `consistency_alerts`
- 该表仍在 SQLite migration 中存在。
- 当前分支无双写链路，不基于该表执行活跃补偿流程。
- 该表属于遗留诊断结构，不代表已交付自愈流程。

### 历史元数据字段
- 历史接入层可能出现 `runtimeMode`、`usedFallback`、`startupSelfHeal` 等字段。
- 这些字段不属于当前后端技术定义承诺，客户端不得作为兼容依赖。

### `src-tauri/migrations/postgres`
- 仓库仍保留历史 Postgres migration 目录。
- 该目录不属于当前运行模式与技术承诺范围。

## 测试基线（本次文档任务）
- `npm run test:unit`：未在本次任务中执行。
- `cargo test`（`src-tauri`）：未在本次任务中执行。
- 本次仅更新技术定义文档，不涉及代码路径改动。

## 不再承诺的路线
- 不承诺任何壳层或前端实现（WinUI/Tauri/React）。
- 不承诺 Tauri `invoke("rpc_invoke")` 兼容入口。
- 不承诺 Postgres 运行时支持。
- 不承诺 DualSync、双写回滚或三模式运行。
- 不承诺通过配置切换仓储实现。
