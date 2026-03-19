# P5-T1 SQLite-only 自动化回归门禁 Codex 执行版

## 目标
- 将 `P5-T1` 收口为可重复执行的 SQLite-only 自动化验收。
- 以自动化回归结果作为唯一结论来源，不再依赖人工交互步骤回收 verdict。
- 明确验证兼容字段 `runtime_mode` 与 `postgres_dsn` 仅产生 warning，不改变运行路径。

## 执行脚本
- `qa-gates-codex/scripts/run-p5-t1-sqlite-regression.ps1`

## 执行内容
脚本一次执行会完成以下检查：
1. 运行前端单测基线：`npm run test:unit`
2. 运行 Rust 全量测试基线：`cargo test --manifest-path src-tauri/Cargo.toml`
3. 运行 Rust 兼容 warning 断言：
   - `legacy_runtime_modes_are_accepted_but_ignored`
   - `warns_when_legacy_postgres_dsn_is_present`
4. 运行前端 runtime adapter 兼容 warning 断言：
   - `warns when legacy runtime modes are present`
   - `keeps warning collection from query parameters`

## 证据产物
- 汇总报告：
  - `qa-gates-codex/P5-T1-AUTO-REGRESSION_{YYYYMMDD}_ENV-SQLITE_codex.txt`
- 每项检查日志：
  - `stdout/stderr` 输出到临时日志目录（报告中会写入完整路径）

## 通过标准
- 所有检查项均为 `PASS`
- `warning_acceptance_verdict = PASS`
- 报告中的 `conclusion` 与 `overall` 均为 `PASS`

## 失败判定
- 任一检查失败即 `overall=FAIL`
- 状态文件不更新通过状态，保留 `P5-T1` 未完成
- 失败细节以报告中的失败检查项与日志路径为准

## 历史说明
- 2026-03-18 / 2026-03-19 的旧版 `P5-T1` 视觉/交互证据保留用于复盘。
- 旧证据不再作为当前 SQLite-only 发布门禁的通过依据。

## 追踪映射
- DEV.md 映射: SQLite-only 运行时与兼容 warning 行为
- ROADMAP.md 映射: Phase 5 / 子任务 1
