# Remember v1 技术实现计划（Tauri + Rust，同步双写）

## Summary
- 基于 GitHub 取证，采用 `Tauri 2 + React + Rust`：Tauri 主项目活跃且体量优势明显，且有全局快捷键官方插件与真实“快速捕获”样例可参考。  
  参考：https://github.com/tauri-apps/tauri ｜ https://github.com/tauri-apps/plugins-workspace ｜ https://github.com/satbunch/sokki
- 数据层采用 `sqlx` 同时支持 SQLite/Postgres，借鉴 Joplin 的“同接口多驱动 + 配置切换”实践。  
  参考：https://github.com/launchbadge/sqlx ｜ https://github.com/laurent22/joplin
- API 合约按你选择的 `Invoke Only + RPC 点号命名` 实现；保留统一请求/响应 envelope，后续可无痛映射 HTTP。
- 运行模式定为三态枚举：`sqlite_only | postgres_only | dual_sync`；当前重点实现 `dual_sync` 严格同步提交策略。

## Key Changes
- `技术栈`: `Tauri 2`、`React + TypeScript`、`Rust`、`sqlx(sqlite, postgres, tokio-rustls)`、`tauri-plugin-global-shortcut`、`serde`、`uuidv7`、`chrono`、`tracing`。
- `分层架构`: `UI(React)` → `Command Adapter(Tauri invoke)` → `Application Service` → `Repository Trait` → `SQLiteRepository/PostgresRepository/DualSyncRepository`。
- `Repository 接口（公共类型变更）`: 固定方法集 `create_series`、`list_series`、`append_commit`、`list_timeline`、`archive_series`、`mark_silent_series`、`search_series_by_name`，三个后端实现同签名。
- `dual_sync 写入算法`: 单次提交生成统一 `commit_id + created_at`，并行开启两库事务，双库写入成功后才返回；若 Postgres 超时（固定 3s）或失败，回滚并返回错误，前端保留草稿；若出现单边提交成功，执行补偿并记录 `consistency_alert` 供启动时自愈扫描。
- `API 合约雏形（Invoke 路径 + JSON）`：统一响应 `{ ok, data?, error?, meta }`，错误码含 `PG_TIMEOUT`、`DUAL_WRITE_FAILED`、`VALIDATION_ERROR`。
  
| Command Path | Request | Response(data) |
|---|---|---|
| `series.create` | `{ "name": "Inbox" }` | `{ "series": { "id","name","status","lastUpdatedAt","latestExcerpt","createdAt" } }` |
| `series.list` | `{ "query":"","includeArchived":false,"cursor":null,"limit":50 }` | `{ "items":[SeriesSummary], "nextCursor":null }` |
| `commit.append` | `{ "seriesId":"...","content":"...","clientTs":"2026-03-11T14:00:00Z" }` | `{ "commit": CommitItem, "series": SeriesSummary }` |
| `timeline.list` | `{ "seriesId":"...","cursor":null,"limit":100 }` | `{ "items":[CommitItem], "nextCursor":null }` |
| `series.archive` | `{ "seriesId":"..." }` | `{ "seriesId":"...","archivedAt":"..." }` |
| `series.scan_silent` | `{ "now":"2026-03-11T14:00:00Z","thresholdDays":7 }` | `{ "affectedSeriesIds":["..."] }` |

- `数据模型设计（双库同构）`：
  
| Table | Fields |
|---|---|
| `series` | `id(uuid pk)`, `name(text)`, `status(enum: active/silent/archived)`, `latest_excerpt(text)`, `last_updated_at(timestamptz)`, `created_at(timestamptz)`, `archived_at(timestamptz null)` |
| `commits` | `id(uuid pk)`, `series_id(uuid fk->series.id)`, `content(text)`, `created_at(timestamptz, 秒级)` |
| `consistency_alerts` | `id(uuid pk)`, `op_type(text)`, `commit_id(uuid)`, `reason(text)`, `created_at(timestamptz)`, `resolved_at(timestamptz null)` |
| `app_settings` | `key(text pk)`, `value(text)`（非敏感配置） |

- `关系与约束`: `series 1:N commits`；`commits` 禁止更新/删除（应用层不暴露更新接口 + DB 权限/触发器限制）；列表按 `series.last_updated_at DESC`；搜索仅 `series.name`。
- `外部依赖与集成点`:
  - OS 全局热键：`tauri-plugin-global-shortcut`。
  - PostgreSQL：桌面端直连（你指定），连接串明文本地配置文件。
  - 本地配置文件：`app data dir/config.toml`，包含 `runtime_mode`、`postgres_dsn`、`silent_days_threshold`、`hotkey`。

## Test Plan
- 功能验收：热键呼出→输入→`Enter` 提交→列表置顶刷新，100% 可用。
- 不可变性：尝试更新/覆盖历史 commit 必须失败（API 无更新命令 + 数据层拒绝）。
- 双写成功路径：`dual_sync` 下 SQLite/Postgres 同时出现同 `commit_id` 与同时间戳。
- 双写失败路径：模拟 Postgres 超时 >3s，返回 `PG_TIMEOUT`，两库均不落库，输入框草稿保留。
- 单边异常补偿：注入“单边提交成功”故障，补偿任务可恢复一致并产生/清理 `consistency_alert`。
- 视图与规则：沉寂判定（7 天）下沉显示、`a` 归档后主列表不可见、Timeline 仅倒序只读。
- 模式切换：`sqlite_only`、`postgres_only`、`dual_sync` 三模式启动与读写行为正确。

## Assumptions
- 单用户桌面应用，当前版本不做账号体系与权限模型。
- API 仅 `invoke`，采用 `RPC 点号命名` 作为“路径”标准。
- PostgreSQL 凭据按你的选择明文保存在本地配置文件（仅建议用于开发/内网场景）。
- 时间统一存储为 UTC，显示层按本地时区渲染；时间戳精度到秒。
- 归档采用同表 `status=archived`（非独立归档库）。
