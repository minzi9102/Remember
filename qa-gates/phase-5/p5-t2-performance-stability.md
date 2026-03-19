# P5-T2 性能与稳定性门禁 测试方案

## 目标
- 验证 SQLite-only 版本的热键响应与提交延迟满足发布前基线。
- 以可重复自动化报告作为本子任务最终验收依据。

## 前置条件
1. 可启动 `src-tauri/target/debug/tauri-app.exe`。
2. 项目内 `.venv` 已就绪（由 `uv` 管理）。
3. 可访问 `task.jsonl` 与 `qa-gates-codex/MASTER-TRACE-MATRIX.md`。

## 推荐执行入口
- `qa-gates-codex/scripts/run-p5-t2-performance-stability.ps1`

## 阈值口径（平衡档）
- Hotkey：`p75 <= 250ms` 且 `p95 <= 450ms`
- Commit：`p75 <= 350ms` 且 `p95 <= 800ms`
- Regression：相对最近基线 `p75/p95` 任一退化 `> 20%` 判 FAIL

## 报告要求
- 报告文件：`P5-T2-PERF-BASELINE_{YYYYMMDD}_ENV-SQLITE_{tester}.txt`
- 必含字段：
  - `hotkey_latency_ms(p75/p95/max)`
  - `commit_latency_ms(p75/p95/max)`
  - `stability(pass_rate/crash_count/timeout_count)`
  - `regression_delta`
  - `overall`

## 通过标准
1. 热键门禁通过。
2. 提交门禁通过。
3. 稳定性门禁通过。
4. 回归门禁通过。
5. 证据可复验且报告为 `overall=PASS`。

## 状态写回
- `overall=PASS`：允许更新 `task.jsonl` 的 `P5-T2=true`，并更新矩阵为 `PASS`。
- `overall=FAIL`：矩阵标记 `FAIL`，`task.jsonl` 保持未完成。

## 回归标签
phase-5 t2 automated-baseline performance stability sqlite-only

---

### 追踪映射
- DEV.md 映射：提交延迟/热键响应
- ROADMAP.md 映射：Phase 5 / 子任务 2
