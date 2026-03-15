# QA Gates (Blackbox)

本目录为 Remember v1 的阶段化门禁测试方案，面向不了解内部代码实现的测试执行者。

## 目录结构
- `EXECUTION-RULES.md`：执行规范与门禁流程
- `EVIDENCE-NAMING.md`：证据命名规则与归档约束
- `MASTER-TRACE-MATRIX.md`：子任务与 `DEV.md` / `ROADMAP.md` 的追踪矩阵
- `phase-1` ~ `phase-5`：按阶段拆分的 26 个子任务门禁文件
- `templates/TASK-GATE-TEMPLATE.md`：标准子任务门禁模板

## 执行要求
1. 每个子任务必须执行 4 条用例：`VG-PASS`、`VG-FAIL`、`IG-PASS`、`IG-FAIL`。
2. 每条用例必须提交至少 1 份视觉证据与 1 份交互证据。
3. 所有步骤必须可在黑盒条件下完成，不依赖源码阅读或内部符号。

## 运行顺序
1. 先执行 `phase-1` 到 `phase-5` 的 PASS 用例。
2. 再执行各阶段 FAIL 用例验证拦截与回退。
3. 在 `MASTER-TRACE-MATRIX.md` 中更新状态并汇总结论。

## 完成定义
- 26 个子任务文件执行完成。
- 104 条用例证据完整可追溯。
- 阻断级/高优先级问题均形成复现记录与处理结论。
