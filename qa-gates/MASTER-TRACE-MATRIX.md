# MASTER TRACE MATRIX

本矩阵用于追踪 SQLite-only 分支门禁文件与 `DEV.md` / `ROADMAP.md` 的映射关系；其中 Phase-4 交互门禁已按 Cross-Axis vNext 目标态重标定。

| ID | 子任务文件 | DEV.md 对应项 | ROADMAP.md 对应项 | 用例数 | 状态 |
|---|---|---|---|---:|---|
| P1-T1 | phase-1/p1-t1-project-layering.md | 分层架构(UI->Adapter->Service->Repository) | Phase 1 / 子任务 1 | 4 | TODO |
| P1-T2 | phase-1/p1-t2-config-runtime-mode.md | 本地配置文件(config.toml) | Phase 1 / 子任务 2 | 4 | TODO |
| P1-T3 | phase-1/p1-t3-command-envelope.md | 统一响应 {ok,data,error,meta} | Phase 1 / 子任务 3 | 4 | TODO |
| P1-T4 | phase-1/p1-t4-tracing-error-mapping.md | tracing + 错误映射 | Phase 1 / 子任务 4 | 4 | TODO |
| P1-T5 | phase-1/p1-t5-shared-dto.md | SeriesSummary/CommitItem | Phase 1 / 子任务 5 | 4 | TODO |
| P2-T3 | phase-2/p2-t3-application-service-flow.md | Application Service | Phase 2 / 子任务 3 | 4 | PASS |
| P2-T5 | phase-2/p2-t5-basic-read-write-query.md | list/search/archive/timeline | Phase 2 / 子任务 5 | 4 | PASS |
| P4-T1 | phase-4/p4-t1-global-hotkey.md | tauri-plugin-global-shortcut | Phase 3 / 已完成能力 | 4 | TODO |
| P4-T2 | phase-4/p4-t2-list-timeline-navigation.md | 十字架构主副轴导航（←/→ + ↓ + Esc） | Phase 6 / 子任务 2-3 | 4 | TODO |
| P4-T3 | phase-4/p4-t3-keyboard-shortcuts.md | 主轨道/时间线/浮层键位契约 | Phase 6 / 子任务 3-4 | 4 | TODO |
| P4-T4 | phase-4/p4-t4-submit-and-reorder.md | 提交后置顶到最左 + 最新摘录刷新 | Phase 6 / 子任务 2-4 | 4 | TODO |
| P4-T5 | phase-4/p4-t5-silent-detection.md | series.scan_silent（主轨道上下文） | Phase 6 / 子任务 2 | 4 | TODO |
| P4-T6 | phase-4/p4-t6-archive-readonly.md | Archived 按钮切换 + Timeline 只读 | Phase 6 / 子任务 5 | 4 | TODO |
| P5-T2 | phase-5/p5-t2-performance-stability.md | 提交延迟/热键响应 | Phase 5 / 子任务 2 | 4 | TODO |
| P5-T31 | phase-5/p5-t31-release-config-troubleshooting.md | 发布配置/排障文档 | Phase 5 / 子任务 3.1 | 4 | TODO |
| P5-T32 | phase-5/p5-t32-release-gate-rollback.md | 发布清单与回滚策略 | Phase 5 / 子任务 3.2 | 4 | TODO |

## 覆盖统计
- 子任务文件数：16
- 每任务用例数：4（VG-PASS / VG-FAIL / IG-PASS / IG-FAIL）
- 总用例数：64

## 状态定义
- `TODO`：未执行
- `RUNNING`：执行中
- `PASS`：通过
- `FAIL`：失败
- `BLOCKED`：阻塞
