# MASTER TRACE MATRIX

本矩阵用于追踪 26 个子任务门禁文件与 `DEV.md` / `ROADMAP.md` 的要求映射。

| ID | 子任务文件 | DEV.md 对应项 | ROADMAP.md 对应项 | 用例数 | 状态 |
|---|---|---|---|---:|---|
| P1-T1 | phase-1/p1-t1-project-layering.md | 分层架构(UI->Adapter->Service->Repository) | Phase 1 / 子任务 1 | 4 | TODO |
| P1-T2 | phase-1/p1-t2-config-runtime-mode.md | 本地配置文件(config.toml) | Phase 1 / 子任务 2 | 4 | TODO |
| P1-T3 | phase-1/p1-t3-command-envelope.md | 统一响应 {ok,data,error,meta} | Phase 1 / 子任务 3 | 4 | TODO |
| P1-T4 | phase-1/p1-t4-tracing-error-mapping.md | tracing + PG_TIMEOUT/DUAL_WRITE_FAILED | Phase 1 / 子任务 4 | 4 | TODO |
| P1-T5 | phase-1/p1-t5-shared-dto.md | SeriesSummary/CommitItem | Phase 1 / 子任务 5 | 4 | TODO |
| P2-T1 | phase-2/p2-t1-isomorphic-migrations.md | series/commits/consistency_alerts/app_settings | Phase 2 / 子任务 1 | 4 | TODO |
| P2-T2 | phase-2/p2-t2-repository-implementations.md | SQLiteRepository/PostgresRepository | Phase 2 / 子任务 2 | 4 | TODO |
| P2-T3 | phase-2/p2-t3-application-service-flow.md | Application Service | Phase 2 / 子任务 3 | 4 | PASS |
| P2-T4 | phase-2/p2-t4-runtime-mode-injection.md | runtime_mode 注入 | Phase 2 / 子任务 4 | 4 | TODO |
| P2-T5 | phase-2/p2-t5-basic-read-write-query.md | list/search/archive/timeline | Phase 2 / 子任务 5 | 4 | PASS |
| P3-T1 | phase-3/p3-t1-dualsync-repository.md | DualSyncRepository | Phase 3 / 子任务 1 | 4 | PASS |
| P3-T2 | phase-3/p3-t2-parallel-tx-timeout.md | Postgres 超时 3s | Phase 3 / 子任务 2 | 4 | TODO |
| P3-T3 | phase-3/p3-t3-rollback-error-codes.md | PG_TIMEOUT / DUAL_WRITE_FAILED | Phase 3 / 子任务 3 | 4 | TODO |
| P3-T4 | phase-3/p3-t4-single-side-compensation-alerts.md | consistency_alerts | Phase 3 / 子任务 4 | 4 | PASS |
| P3-T5 | phase-3/p3-t5-startup-self-heal.md | 启动自愈扫描 | Phase 3 / 子任务 5 | 4 | PASS |
| P4-T1 | phase-4/p4-t1-global-hotkey.md | tauri-plugin-global-shortcut | Phase 4 / 子任务 1 | 4 | TODO |
| P4-T2 | phase-4/p4-t2-list-timeline-navigation.md | 列表/时间线二级视图 | Phase 4 / 子任务 2 | 4 | TODO |
| P4-T3 | phase-4/p4-t3-keyboard-shortcuts.md | ↑/↓/Enter/Esc/←/→//Shift+N/a | Phase 4 / 子任务 3 | 4 | BLOCKED |
| P4-T4 | phase-4/p4-t4-submit-and-reorder.md | 提交后排序刷新 | Phase 4 / 子任务 4 | 4 | PASS |
| P4-T5 | phase-4/p4-t5-silent-detection.md | series.scan_silent | Phase 4 / 子任务 5 | 4 | PASS |
| P4-T6 | phase-4/p4-t6-archive-readonly.md | series.archive + timeline readonly | Phase 4 / 子任务 6 | 4 | PASS |
| P5-T1 | phase-5/p5-t1-full-regression.md | 三模式启动与读写 | Phase 5 / 子任务 1 | 4 | TODO |
| P5-T2 | phase-5/p5-t2-dualwrite-fault-drill.md | dual_sync 失败/补偿 | Phase 5 / 子任务 2 | 4 | TODO |
| P5-T3 | phase-5/p5-t3-performance-stability.md | 提交延迟/热键响应 | Phase 5 / 子任务 3 | 4 | TODO |
| P5-T4 | phase-5/p5-t4-release-config-troubleshooting.md | 发布配置/排障文档 | Phase 5 / 子任务 4 | 4 | TODO |
| P5-T5 | phase-5/p5-t5-release-gate-rollback.md | 发布清单与回滚策略 | Phase 5 / 子任务 5 | 4 | TODO |

## 覆盖统计
- 子任务文件数：26
- 每任务用例数：4（VG-PASS / VG-FAIL / IG-PASS / IG-FAIL）
- 总用例数：104

## 状态定义
- `TODO`：未执行
- `RUNNING`：执行中
- `PASS`：通过
- `FAIL`：失败
- `BLOCKED`：阻塞
