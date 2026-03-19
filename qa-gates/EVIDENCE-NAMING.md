# EVIDENCE NAMING

## 命名规范
`{CaseID}_{YYYYMMDD}_{env}_{tester}.{png|mp4|txt}`

示例：
- `P3-T2-VG-FAIL_20260315_ENV-DUAL_alice.png`
- `P4-T3-IG-PASS_20260315_ENV-SQLITE_bob.mp4`
- `P5-T31-VG-PASS_20260319_ENV-SQLITE_codex.txt`
- `P5-T32-IG-FAIL_20260319_ENV-SQLITE_codex.mp4`

## 字段定义
- `CaseID`：固定格式 `P{n}-T{m}-VG-PASS|VG-FAIL|IG-PASS|IG-FAIL`（`m` 可为多位数，例如 `31`、`32`）
- `YYYYMMDD`：执行日期
- `env`：`ENV-SQLITE` / `ENV-PG` / `ENV-DUAL`
- `tester`：执行人英文名或工号

## 证据类型要求
1. `.png`：关键界面状态截图。
2. `.mp4`：连续交互流程录屏（建议 10~120 秒）。
3. `.txt`：操作步骤记录、日志摘录、SQL 查询结果。

## 归档建议
- 按阶段目录归档：`evidence/phase-{n}/`
- 每个子任务保留独立子目录：`evidence/phase-{n}/p{n}-t{m}/`
- 证据文件名必须与用例 ID 一一对应。
