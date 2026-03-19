# P5-T2 性能与稳定性门禁 Codex 执行版

## 目标
- 将 `P5-T2` 收口为可重复执行的性能与稳定性自动化基线。
- 统一使用单脚本输出报告，不再以 VG/IG 四用例作为最终 verdict 来源。

## 执行脚本
- `qa-gates-codex/scripts/run-p5-t2-performance-stability.ps1`

## 执行参数
- `EnvId`：默认 `ENV-SQLITE`
- `Tester`：默认 `codex`
- `SampleCount`：默认 `20`
- `RegressionWindow`：默认 `1`
- `HotkeyTimeoutMs`：默认 `2000`
- `CommitTimeoutMs`：默认 `1200`
- `UpdateState`：默认 `true`（`PASS` 时回写 `task.jsonl` 与矩阵）

## 执行内容
脚本一次执行会完成以下检查：
1. precheck：`tauri-app.exe`、`.venv` Python、矩阵文件、`task.jsonl`
2. 热键采样：使用 Win32 `SendInput` 触发 `Alt+Space`，采集窗口隐藏/显示延迟
3. 提交采样：对当前 SQLite 数据库执行 commit 写入链路（insert + series update）延迟采样
4. 稳定性统计：`pass_rate / crash_count / timeout_count`
5. 回归比较：与最近一份基线报告对比 `p75/p95` 退化比例

## 门禁阈值（平衡档）
- Hotkey：`p75 <= 250ms` 且 `p95 <= 450ms`
- Commit：`p75 <= 350ms` 且 `p95 <= 800ms`
- Regression：相对最近基线 `p75/p95` 任一退化 `> 20%` 判 FAIL

## 证据产物
- 汇总报告：
  - `qa-gates-codex/P5-T2-PERF-BASELINE_{YYYYMMDD}_ENV-SQLITE_{tester}.txt`
- 运行日志：
  - `log_dir` 写入报告（stdout/stderr、临时脚本日志）

## 通过标准
- `hotkey_gate = PASS`
- `commit_gate = PASS`
- `stability_gate = PASS`
- `regression_gate = PASS`
- 报告中的 `overall = PASS`

## 状态写回规则
- `UpdateState=true` 且 `overall=PASS`：
  - `task.jsonl`：`P5-T2` 设置为 `completed=true`
  - `qa-gates-codex/MASTER-TRACE-MATRIX.md`：`P5-T2` 设置为 `PASS`
- `UpdateState=true` 且 `overall=FAIL`：
  - 仅将矩阵 `P5-T2` 设置为 `FAIL`
  - `task.jsonl` 保持未完成

## 追踪映射
- source gate: `qa-gates/phase-5/p5-t2-performance-stability.md`
- DEV.md 映射: 提交延迟/热键响应
- ROADMAP.md 映射: Phase 5 / 子任务 2
