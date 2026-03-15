# Remember v1 分阶段开发计划（`ROADMAP.md`，5 Phases 细分版）

## Summary
基于 [DEV.md](./DEV.md) 生成独立的阶段化执行文档 [ROADMAP.md](./ROADMAP.md)，采用 5 个按依赖推进的阶段。每个阶段固定包含：`目标`、`子任务`、`交付物`、`验收门槛`、`风险与依赖`，确保实现时无需再做关键决策。

## Key Changes
- `ROADMAP.md` 固定结构：
  1. 项目目标与边界
  2. 全局技术约束（不可变历史、三运行模式、双写一致性）
  3. 五阶段执行计划（每阶段五要素）
  4. 里程碑与阶段依赖关系
  5. 总体验收矩阵（映射 DEV.md Test Plan）
- 公共接口与类型冻结（写入 ROADMAP 全局约束）：
  - `Invoke Only + RPC`：`series.create`、`series.list`、`commit.append`、`timeline.list`、`series.archive`、`series.scan_silent`
  - 响应 envelope：`{ ok, data?, error?, meta }`
  - Repository 固定方法集：`create_series/list_series/append_commit/list_timeline/archive_series/mark_silent_series/search_series_by_name`
  - 运行模式：`sqlite_only | postgres_only | dual_sync`
  - 双库同构表：`series`、`commits`、`consistency_alerts`、`app_settings`

## 五阶段计划（含子任务与目标）

### Phase 1: 工程基线与架构骨架
- 目标：搭建可运行的 Tauri+React+Rust 主干工程与分层骨架，打通 invoke 命令链路。
- 子任务：
  1. 初始化项目与目录分层（UI/Adapter/Application/Repository）。
  2. 建立 `config.toml` 读取与运行模式解析。
  3. 创建命令壳并返回统一 envelope。
  4. 接入 `tracing` 与错误码映射。
  5. 定义共享 DTO（`SeriesSummary`、`CommitItem` 等）。
- 交付物：可启动应用、可调用空实现命令、统一日志与错误响应。
- 验收门槛：应用在三模式配置下均可启动，命令路由与响应结构稳定。
- 风险与依赖：无前置依赖；风险是骨架与后续数据层接口不一致，需在本阶段冻结接口签名。

### Phase 2: 单库能力闭环（`sqlite_only` / `postgres_only`）
- 目标：先完成两种单库模式下的完整业务闭环，形成 `dual_sync` 的稳定前提。
- 子任务：
  1. 实现 SQLite/Postgres 同构 migration。
  2. 定义并实现 Repository Trait 与两个后端。
  3. 完成 Application Service（创建系列、提交、列表、时间线、归档、搜索）。
  4. 按 `runtime_mode` 注入后端实现。
  5. 完成基础读写与查询测试。
- 交付物：`sqlite_only` 与 `postgres_only` 均可独立完成全链路读写。
- 验收门槛：两单库模式下接口行为一致，列表排序/搜索/归档正确。
- 风险与依赖：依赖 Phase 1 接口冻结；风险是两后端行为漂移，需测试统一语义。

### Phase 3: `dual_sync` 一致性核心
- 目标：实现严格双写提交与异常补偿，确保两库最终一致。
- 子任务：
  1. 实现 `DualSyncRepository`（统一 `commit_id + created_at`）。
  2. 并行事务写入并设置 Postgres 3s 超时。
  3. 失败时双边回滚并返回 `PG_TIMEOUT` / `DUAL_WRITE_FAILED`。
  4. 注入单边成功场景并写入 `consistency_alerts`。
  5. 启动时执行自愈扫描与补偿修复。
- 交付物：`dual_sync` 可用，异常路径可观测、可恢复。
- 验收门槛：成功时两库 `commit_id/created_at` 一致；超时失败时两库均不落库；单边异常可被补偿并闭环告警。
- 风险与依赖：依赖 Phase 2 单库稳定；风险是补偿逻辑复杂，需保留故障注入测试与告警可视性。

### Phase 4: 交互能力与业务规则实现
- 目标：完成用户可感知的高频交互与规则（热键、键盘流、沉寂、归档、只读时间线）。
- 子任务：
  1. 接入全局热键呼出/隐藏。
  2. 实现一级列表与二级时间线视图切换。
  3. 实现键盘操作（`↑/↓`、`Enter`、`Esc`、`←/→`、`/`、`Shift+N`、`a`）。
  4. 提交后列表置顶刷新与摘录更新。
  5. 实现沉寂判定与下沉显示。
  6. 实现归档移出主列表、时间线只读。
- 交付物：完整可交互的 v1 UI 与规则行为。
- 验收门槛：关键路径“热键呼出 -> 输入 -> Enter -> 列表置顶刷新”稳定；沉寂/归档/搜索行为符合定义。
- 风险与依赖：依赖 Phase 2/3 服务稳定；风险是 UI 状态与后端状态不同步，需端到端交互回归。

### Phase 5: 稳定性强化与发布验收
- 目标：完成跨模式回归、故障演练、发布前验证，形成可交付版本。
- 子任务：
  1. 执行全量功能回归（含三模式）。
  2. 执行双写异常演练与恢复验证。
  3. 完成性能与稳定性基础检查（提交延迟、热键响应）。
  4. 完成发布配置与故障排查文档。
  5. 形成发布清单与回滚策略。
- 交付物：发布候选版本（RC）与验收记录。
- 验收门槛：`DEV.md` Test Plan 场景通过率 100%，无阻断级缺陷。
- 风险与依赖：依赖前四阶段完成；风险是环境差异导致回归不稳定，需固定测试环境与基线数据。

## Test Plan
- 阶段验收映射：
  1. Phase 1：启动与命令骨架可用、响应协议固定。
  2. Phase 2：两单库模式功能一致、排序与查询正确。
  3. Phase 3：双写成功/超时/单边异常三路径均可验证。
  4. Phase 4：键盘优先交互、沉寂判定、归档与只读时间线通过。
  5. Phase 5：全链路回归、模式切换、故障恢复与发布检查通过。
- 关键场景保底：不可变 Commit、`dual_sync` 一致性、`PG_TIMEOUT` 处理、`consistency_alerts` 自愈闭环。

## Assumptions
- 目标落盘文件固定为 `ROADMAP.md`，`DEV.md` 保持不变。
- 阶段粒度固定为 5 阶段，不再拆到 7 阶段。
- 单用户桌面应用，不引入账号体系与权限模型。
- 时间统一存 UTC、显示按本地时区，时间精度到秒。
- PostgreSQL 凭据按现有约定存本地配置（明文，开发/内网场景）。
