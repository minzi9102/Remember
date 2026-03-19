# Remember v1 技术定义（SQLite-only）

## Summary
- 当前分支唯一运行模式是 `sqlite_only`，技术栈固定为 `Tauri 2 + React + TypeScript + Rust + sqlx(sqlite)`。
- 前端通过 Tauri `invoke` 调用 Rust RPC 适配层；当前没有 HTTP API，也没有运行时后端切换。
- 本文档包含两类内容：`已实现技术事实` 与 `Cross-Axis vNext 前端目标契约`。
- 截至 2026-03-19，vNext 前端契约尚未全部落地，属于定稿待实现范围。

## 当前实现快照
- 前端：`React 19`、`TypeScript`、`Vite`。
- 桌面壳：`Tauri 2`、`tauri-plugin-global-shortcut`、`tauri-plugin-opener`。
- 后端：`Rust`、`tokio`、`serde`、`tracing`、`uuid v7`、`chrono`。
- 持久化：`sqlx` 启用 `sqlite / migrate / macros / runtime-tokio`，唯一仓储实现为 `SQLiteRepository`。
- 运行形态：单机本地数据库文件 `remember.sqlite3`，不依赖外部数据库服务。

## 分层结构
- UI：React 视图与交互组件。
- Adapter：运行时状态适配、Tauri RPC、全局热键接入。
- Application：参数校验、时间规范化、业务流程编排。
- Repository：`MemoRepository` trait 与 `SQLiteRepository` 实现。
- Migration：SQLite schema 初始化与幂等迁移测试。

## 运行时真相

### 运行模式
- 应用启动后固定报告 `sqlite_only`。
- 旧配置中的 `runtime_mode` 即使存在，也只会生成 warning，不会切换实际路径。
- 运行时标题与 RPC meta 会继续携带 `sqlite_only` 标记，供前端诊断层读取。

### 配置解析
- 配置文件名固定为 `config.toml`。
- 若环境变量 `REMEMBER_APPDATA_DIR` 存在且目录可创建，配置路径为 `<override>/config.toml`。
- 否则使用平台 app data 目录下的 `config.toml`。
- 若平台目录不可解析，则回退到当前工作目录中的 `config.toml`。

### 当前有效配置项
| Key | 类型 | 默认值 | 说明 |
|---|---|---:|---|
| `hotkey` | string | `Alt+Space` | 全局唤醒热键 |
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

## 公共契约（已实现）

### RPC 入口
- 调用方式固定为 Tauri `invoke`。
- 路径风格固定为 `RPC 点号命名`。
- 当前已实现命令如下：

| Path | Request | Response(data) |
|---|---|---|
| `series.create` | `{ "name": string }` | `{ "series": SeriesSummary }` |
| `series.list` | `{ "query": string, "includeArchived": boolean, "cursor": string \| null, "limit": number }` | `{ "items": SeriesSummary[], "nextCursor": string \| null, "limitEcho": number }` |
| `commit.append` | `{ "seriesId": string, "content": string, "clientTs": string }` | `{ "commit": CommitItem, "series": SeriesSummary }` |
| `timeline.list` | `{ "seriesId": string, "cursor": string \| null, "limit": number }` | `{ "seriesId": string, "items": CommitItem[], "nextCursor": string \| null }` |
| `series.archive` | `{ "seriesId": string }` | `{ "seriesId": string, "archivedAt": string }` |
| `series.scan_silent` | `{ "now": string, "thresholdDays": number }` | `{ "affectedSeriesIds": string[], "thresholdDays": number }` |

### RPC Envelope
```json
{
  "ok": true,
  "data": {},
  "error": null,
  "meta": {
    "path": "series.list",
    "runtimeMode": "sqlite_only",
    "usedFallback": false,
    "respondedAtUnixMs": 0,
    "startupSelfHeal": {
      "scannedAlerts": 0,
      "repairedAlerts": 0,
      "unresolvedAlerts": 0,
      "failedAlerts": 0,
      "completedAt": "1970-01-01T00:00:00Z",
      "messages": []
    }
  }
}
```

### DTO 与状态
- `SeriesStatus`: `active | silent | archived`
- `SeriesSummary`: `id / name / status / lastUpdatedAt / latestExcerpt / createdAt / archivedAt?`
- `CommitItem`: `id / seriesId / content / createdAt`
- `SeriesListData`: `items / nextCursor / limitEcho`
- `TimelineListData`: `seriesId / items / nextCursor`
- `SeriesScanSilentData`: `affectedSeriesIds / thresholdDays`

### Repository 契约
`MemoRepository` 当前固定方法集：
- `create_series`
- `list_series`
- `append_commit`
- `list_timeline`
- `archive_series`
- `mark_silent_series`
- `search_series_by_name`

## 前端交互契约 vNext（目标态，待实现）

### 变更边界
- 本轮契约变更只涉及前端交互语义与输入焦点状态机。
- 后端 RPC 路径、DTO、Repository trait、SQLite schema 不发生新增或破坏性变更。
- `series.create / series.list / commit.append / timeline.list / series.archive / series.scan_silent` 继续作为唯一业务接口。

### 目标交互模型
- 主轴：屏幕上方横向 Active Series 轨道，最新更新项固定最左。
- 副轴：当前高亮 Series 的 Timeline 在卡片下方纵向展开。
- 输入分层：
  - 附着草稿框：直接输入触发，绑定当前高亮 Series。
  - 全局悬浮条：`/` 与 `Shift+N` 触发，浮于主轨道下方。
- 归档切换：右上 `Archived` 按钮为唯一集合切换入口（`mouse-only`）。

### 键位语义表（目标态）
| 场景 | 按键 | 目标行为 |
|---|---|---|
| 主轨道浏览 | `← / →` | 左右切换当前高亮 Series |
| 主轨道进入 Timeline | `↓` | 在当前卡片下方展开纵向 Timeline |
| Timeline 浏览 | `↑ / ↓` | 按时间倒序滚动历史 Commit |
| Timeline 返回主轨道 | `Esc` | 关闭 Timeline 并返回横向主轨道焦点 |
| 全局搜索浮层 | `/` | 打开全局搜索输入条（仅过滤 Series 名称） |
| 全局新建浮层 | `Shift+N` | 打开全局新建 Series 输入条 |
| 附着草稿 | 直接输入字符 | 在当前高亮 Series 下打开草稿输入 |
| 草稿提交 | `Enter` | 提交 Commit，并将该 Series 置顶到最左 |
| 一键归档 | `a` | 仅当高亮项是 silent 时执行逻辑归档 |

### 只读与约束
- Timeline 全程只读，不提供历史编辑入口。
- Archived 集合下不允许追加 Commit 与新建 Series。
- 搜索仅过滤 Series 名称，不检索 Commit 正文。

## SQLite Schema

### 核心表
| Table | Fields | 说明 |
|---|---|---|
| `series` | `id`, `name`, `status`, `latest_excerpt`, `last_updated_at`, `created_at`, `archived_at` | Series 主表；状态受 `active/silent/archived` 约束 |
| `commits` | `id`, `series_id`, `content`, `created_at` | 不可变 Commit 时间线 |
| `app_settings` | `key`, `value` | 预留的应用级键值配置 |

### 索引
- `idx_series_last_updated_at`
- `idx_commits_series_created_at`

### 规则与限制
- `series.status` 仅允许 `active / silent / archived`。
- `commits.series_id` 外键指向 `series.id`。
- 已归档 Series 会在仓储层拒绝追加 Commit。
- 搜索仅按 `series.name` 执行。
- 分页排序以 `last_updated_at DESC, id DESC` 或 `created_at DESC, id DESC` 为准。

## 遗留结构与兼容说明

### `consistency_alerts`
- 该表仍在 SQLite migration 中存在。
- 当前分支没有运行中的双写链路，也没有基于该表执行活跃补偿流程。
- 文档应把它视为遗留诊断结构，而不是当前已交付的自愈能力。

### `startup_self_heal`
- RPC meta 仍会返回 `startupSelfHeal` 对象。
- 当前 bootstrap 使用 `StartupSelfHealSummary::clean()`，默认值为零计数和固定占位时间。
- 这代表“诊断字段仍保留”，不代表系统已经实现启动期自愈扫描。

### `src-tauri/migrations/postgres`
- 仓库中仍保留旧的 Postgres migration 目录。
- 它不是当前运行模式的一部分，也不应再被表述为可选后端。
- 是否彻底删除该遗留目录，应由后续独立任务决策与执行。

## 测试基线（文档更新时已验证）
- `npm run test:unit`：未在本次文档任务中执行。
- `cargo test`（`src-tauri`）：未在本次文档任务中执行。
- 本次变更仅更新定义文档，不涉及代码路径变更。

## 不再承诺的路线
- 不再承诺 Postgres 运行时支持。
- 不再承诺 DualSync、双写回滚或三模式运行。
- 不再承诺通过配置切换仓储实现。
