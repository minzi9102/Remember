# EXECUTION CHECKLIST

## A. 可用性
- [ ] `npx` 可用
- [ ] `playwright-cli` 可执行
- [ ] `screenshot` 脚本存在且可调用

## B. 覆盖性
- [ ] 26 个子任务文件存在
- [ ] 每个文件包含 4 条用例（VG/IG + PASS/FAIL）
- [ ] 每条用例都含 `case_id/skill_chain/target_mode/setup/steps/oracle/evidence`

## C. 执行性
- [ ] 每个 Phase 至少抽样 1 个子任务 dry-run
- [ ] PASS/FAIL 路径均可落证
- [ ] `MASTER-TRACE-MATRIX.md` 状态完整

## D. 发布前
- [ ] 阻断级问题为 0
- [ ] 高优问题有结论
- [ ] 证据可追溯
