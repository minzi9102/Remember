# Remember v1 分阶段开发计划（`ROADMAP.md`，SQLite-only 版）

## Summary
基于 [DEV.md](./DEV.md) 生成独立的阶段化执行文档 [ROADMAP.md](./ROADMAP.md)，采用 5 个按依赖推进的阶段。每个阶段固定包含：`目标`、`子任务`、`交付物`、`验收门槛`、`风险与依赖`，确保实现时无需再做关键决策。

## Key Changes
- `ROADMAP.md` 固定结构：
  1. 项目目标与边界
  2. 全局技术约束（不可变历史、SQLite-only、单库一致性）
  3. 五阶段执行计划（每阶段五要素）
  4. 里程碑与阶段依赖关系
  5. 总体验收矩阵（映射 DEV.md Test Plan）
- 公共接口与类型冻结（写入 ROADMAP 全局约束）：
  - `Invoke Only + RPC`：`series.create`、`series.list`、`commit.append`、`timeline.list`、`series.archive`、`series.scan_silent`
  - 响应 envelope：`{ ok, data?, error?, meta }`
  - Repository 固定方法集：`create_series/list_series/append_commit/list_timeline/archive_series/mark_silent_series/search_series_by_name`
  - 运行模式：`sqlite_only`
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

### Phase 2: SQLite 能力闭环
- 目标：完成 SQLite-only 模式下的完整业务闭环。
- 子任务：
  1. 实现 SQLite migration。
  2. 定义并实现 Repository Trait 与 SQLite 后端。
  3. 完成 Application Service（创建系列、提交、列表、时间线、归档、搜索）。
  4. 固定注入 SQLite 实现，并兼容旧配置字段。
  5. 完成基础读写与查询测试。
- 交付物：`sqlite_only` 完成全链路读写。
- 验收门槛：SQLite 模式下列表排序/搜索/归档正确。
- 风险与依赖：依赖 Phase 1 接口冻结；风险是 mock 与真实 SQLite 行为漂移，需测试统一语义。

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
- 风险与依赖：依赖 Phase 2 服务稳定；风险是 UI 状态与后端状态不同步，需端到端交互回归。

### Phase 5: 稳定性强化与发布验收
- 目标：完成 SQLite-only 回归、稳定性检查与发布前验证，形成可交付版本。
- 子任务：
  1. 执行全量功能回归（SQLite-only）。
  3. 完成性能与稳定性基础检查（提交延迟、热键响应）。
  4. 完成发布配置与故障排查文档。
  5. 形成发布清单与回滚策略。
- 交付物：发布候选版本（RC）与验收记录。
- 验收门槛：`DEV.md` Test Plan 场景通过率 100%，无阻断级缺陷。
- 风险与依赖：依赖前四阶段完成；风险是环境差异导致回归不稳定，需固定测试环境与基线数据。

## Test Plan
- 阶段验收映射：
  1. Phase 1：启动与命令骨架可用、响应协议固定。
  2. Phase 2：SQLite 功能闭环、排序与查询正确。
  3. Phase 4：键盘优先交互、沉寂判定、归档与只读时间线通过。
  4. Phase 5：全链路回归、配置兼容与发布检查通过。
- 关键场景保底：不可变 Commit、SQLite 列表/时间线一致、旧配置兼容告警、归档与沉寂规则正确。

## Assumptions
- 目标落盘文件固定为 `ROADMAP.md`，`DEV.md` 保持不变。
- 阶段粒度固定为 5 阶段，不再拆到 7 阶段。
- 单用户桌面应用，不引入账号体系与权限模型。
- 时间统一存 UTC、显示按本地时区，时间精度到秒。
- 旧 `runtime_mode` 与 `postgres_dsn` 仅保留兼容读取与 warning，不再控制运行路径。
