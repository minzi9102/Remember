# QA Gates Codex (Skill-Orchestrated Blackbox)

本目录是 `qa-gates/` 的 Codex 执行版协同测试包。
目标：让测试执行者在不了解内部代码细节时，也能通过 `Codex + skill` 完成视觉门禁与交互门禁测试。

## 目录说明
- `SKILL-MATRIX.md`: skill 选择、能力边界、风险等级与安装决策
- `SAFETY-REVIEW.md`: 安全评审记录与安装闸门
- `RUNBOOK.md`: Codex 执行手册（命令级）
- `EXECUTION-CHECKLIST.md`: 执行与发布前检查清单
- `MASTER-TRACE-MATRIX.md`: 26 子任务追踪矩阵（104 用例）
- `phase-1` ~ `phase-5`: 26 个子任务的 Codex 可执行门禁文件

## 使用方式
1. 按 `RUNBOOK.md` 完成前置检查与环境变量设置。
2. 按 `phase-1 -> phase-5` 顺序执行每个子任务文件中的 4 条用例。
3. 每条用例提交证据文件，并更新 `MASTER-TRACE-MATRIX.md` 状态。

## 产物规范
- 每个子任务固定 4 条用例：`VG-PASS`、`VG-FAIL`、`IG-PASS`、`IG-FAIL`
- 每条用例包含固定字段：`case_id`、`skill_chain`、`target_mode`、`setup`、`steps`、`oracle`、`evidence`
- 证据命名沿用 `qa-gates/EVIDENCE-NAMING.md`
