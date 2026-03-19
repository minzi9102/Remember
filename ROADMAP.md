# Remember 当前分支路线图（SQLite-only）

## Summary
- 当前分支已完成 SQLite-only 主体工程与既有交互闭环。
- `Cross-Axis vNext` 前端交互方案已于 2026-03-19 定稿，状态为“待实现”。
- 本路线图新增 Phase 6，用于承接十字架构布局、键位重映射、浮层分层和归档切换策略收敛。

## 全局约束
- 运行模式固定为 `sqlite_only`。
- API 形态固定为 Tauri `invoke` + `RPC 点号命名`。
- Commit 语义固定为不可变历史，不新增编辑/删除路径。
- 归档为同库逻辑归档，不引入独立归档数据库。
- `runtime_mode` 与 `postgres_dsn` 仅保留兼容 warning 语义。
- Phase 6 默认不引入后端契约变更；若出现接口新增，需独立提案。

## 阶段总览
| Phase | 状态 | 目标 |
|---|---|---|
| Phase 1 | 已完成 | 工程骨架与 invoke/RPC 合同 |
| Phase 2 | 已完成 | SQLite 核心读写闭环与 schema |
| Phase 3 | 已完成 | 既有主界面交互、热键、搜索、时间线、沉寂与归档 |
| Phase 4 | 已完成 | SQLite-only 文档收束与遗留叙事清理 |
| Phase 5 | 进行中 | SQLite-only 发布前回归与发布文档 |
| Phase 6 | 进行中 | Cross-Axis vNext 前端交互重构 |

## Phase 5: 发布前收尾（SQLite-only）
- 状态：进行中
- 目标：在 SQLite-only 前提下完成回归验证、发布文档与交付清单。
- 当前成果：
  - P5-T1 已完成：SQLite-only 全链路回归与兼容 warning 验收。
  - P5-T2 已完成：性能与稳定性基线检查。
- 剩余子任务：
  1. P5-T3 发布配置、排障文档、发布清单与回滚策略。
- 验收口径：发布候选版本具备完整回归记录、可追踪问题清单和明确发布/回滚步骤。

## Phase 6: Cross-Axis vNext 前端交互重构
- 状态：进行中
- 目标：按定稿方案落地“横向主轨道 + 纵向 Timeline + 输入分层 + Archived 点击切换”。
- 子任务：
  1. P6-T1 合并定义文档并冻结目标态口径（已完成）。
  2. P6-T2 重构主界面空间布局为十字架构（待完成）。
  3. P6-T3 重映射键盘导航与焦点状态机（待完成）。
  4. P6-T4 实现全局悬浮命令条与附着草稿框分层（待完成）。
  5. P6-T5 收敛 Archived 切换策略为右上按钮 mouse-only（待完成）。
  6. P6-T6 更新并执行 phase-4 门禁回归（待完成）。
- 验收口径：
  - 主轨道 `←/→`、进入 Timeline `↓`、Timeline 浏览 `↑/↓`、返回 `Esc` 行为一致。
  - 提交后目标 Series 置顶最左且摘录刷新。
  - `Archived` 仅支持按钮点击切换，归档集合与 Timeline 全程只读。

## 里程碑关系
- Phase 1-2 提供稳定数据与接口底座。
- Phase 3 提供既有交互闭环，作为 Phase 6 重构起点。
- Phase 5 与 Phase 6 并行：前者面向当前可发布基线，后者面向下一交互版本。

## 当前发布焦点
- 当前可发布焦点仍是 SQLite-only 稳定性与发布文档闭环（Phase 5）。
- Cross-Axis vNext 为后续版本目标，不应在未实现前写成当前已交付能力。
