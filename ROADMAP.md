# Remember 当前分支路线图（SQLite-only）

## Summary
- 当前分支已经完成 SQLite-only 主体工程、核心数据闭环和主要交互能力。
- 本轮文档收束完成后，Phase 1-4 视为已落地；剩余工作集中在 SQLite-only 回归验证、发布文档和发布清单。
- 路线图只描述“当前分支从现在到可交付版本”的阶段状态，不再回放已废弃的多数据库路线。

## 全局约束
- 运行模式固定为 `sqlite_only`。
- API 形态固定为 Tauri `invoke` + `RPC 点号命名`。
- Commit 语义固定为不可变历史，不新增编辑/删除路径。
- 归档为同库逻辑归档，不引入独立归档数据库。
- `runtime_mode` 与 `postgres_dsn` 仅保留兼容 warning 语义。
- `consistency_alerts` 与 `startup_self_heal` 当前仅按遗留诊断结构处理，不按活跃能力规划。

## 阶段总览
| Phase | 状态 | 目标 |
|---|---|---|
| Phase 1 | 已完成 | 工程骨架与 invoke/RPC 合同 |
| Phase 2 | 已完成 | SQLite 核心读写闭环与 schema |
| Phase 3 | 已完成 | 主界面交互、热键、搜索、时间线、沉寂与归档 |
| Phase 4 | 已完成 | SQLite-only 文档收束与遗留叙事清理 |
| Phase 5 | 待完成 | SQLite-only 全量回归、发布文档、发布清单 |

## Phase 1: 工程骨架与契约冻结
- 状态：已完成
- 目标：建立 Tauri + React + Rust 主干工程，打通前后端调用链路并冻结基础 RPC 契约。
- 当前成果：已具备 `series.create / series.list / commit.append / timeline.list / series.archive / series.scan_silent` 命令入口、统一 envelope 和基础 DTO。
- 剩余子任务：无。
- 验收口径：应用可启动，前端可获得统一运行时状态与 RPC 结果结构。

## Phase 2: SQLite 数据闭环
- 状态：已完成
- 目标：完成 SQLite-only 持久化、查询、分页、归档和沉寂扫描能力。
- 当前成果：SQLite migration、`MemoRepository` trait、`SQLiteRepository` 实现、Application Service 核心流程与仓储契约测试均已落地。
- 剩余子任务：无。
- 验收口径：Series 创建、Commit 追加、列表排序、Timeline 查询、归档、沉寂扫描均可通过当前测试基线验证。

## Phase 3: 交互与业务规则
- 状态：已完成
- 目标：完成用户主路径的桌面交互闭环与业务规则呈现。
- 当前成果：全局热键、列表/时间线切换、键盘输入流、搜索、沉寂标记、归档集合、只读 Timeline 均已接通。
- 剩余子任务：无。
- 验收口径：关键路径“唤醒 -> 输入 -> 提交 -> 列表刷新”可工作，归档与只读行为符合产品定义。

## Phase 4: 文档收束与遗留叙事清理
- 状态：已完成
- 目标：把项目文档收敛为 `product.md + DEV.md + ROADMAP.md + task.jsonl` 四件套，并退役重复定义文档。
- 当前成果：
  - `product.md` 作为唯一 PRD，只保留产品价值、交互契约、范围边界与验收场景。
  - `DEV.md` 作为唯一技术真相源，固定 SQLite-only 契约、配置、RPC、schema 与遗留结构说明。
  - `ROADMAP.md` 重建为当前分支的阶段状态文档。
  - `task.jsonl` 改写为 Phase 4-5 的原子任务清单。
  - `PLAN1.md` 已退役，避免与 PRD 重复。
- 剩余子任务：后续所有代码改动都需继续保持四件套同步。
- 验收口径：目标文档之间不再出现多数据库作为当前能力的叙事冲突；阶段任务与 backlog 一致。

## Phase 5: 发布前收尾
- 状态：待完成
- 目标：在 SQLite-only 前提下完成回归验证、发布文档与交付清单。
- 当前成果：已有单元测试与 Rust 测试基线，可作为发布前回归的起点。
- 剩余子任务：
  1. 执行 SQLite-only 全链路回归，并验证旧配置字段只产生 warning、不改变运行路径。
  2. 完成性能与稳定性基线检查，关注热键响应、提交延迟和主路径稳定性。
  3. 补齐发布配置、排障文档、发布清单与回滚策略。
  4. 决策是否继续保留 `consistency_alerts`、`startup_self_heal`、旧 Postgres migration 目录作为遗留结构，或拆分独立清理任务。
- 验收口径：发布候选版本具备清晰的回归记录、可追踪的问题清单和明确的发布/回滚步骤。

## 里程碑关系
- Phase 1 是 Phase 2 的接口前提。
- Phase 2 是 Phase 3 的数据能力前提。
- Phase 3 为 Phase 5 的回归验证提供完整用户路径。
- Phase 4 已完成文档真相收束，为 Phase 5 的发布沟通和验收口径提供统一依据。

## 当前发布焦点
- 发布阻塞项不再是“选哪种数据库”，而是“把现有 SQLite-only 版本验证到可发布”。
- 任何涉及清理遗留结构的动作，都应在不破坏当前 SQLite-only 稳定性的前提下单独排期。
