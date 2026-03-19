# P5-T1 全模式回归门禁 Codex 执行版

## 目标
- 把 `P5-T1` 改为发布级可复验的混合门禁。
- 视觉门禁由脚本采证，交由 Codex 多模态审核。
- 交互门禁由脚本编排步骤、人工执行真实桌面操作、脚本回收 `PASS/FAIL` 与数据库证明。

## 执行脚本
- `qa-gates-codex/scripts/run-p5-t1-full-regression.ps1`

## 核心变化
1. 不再把“脚本未抛异常”视为 `PASS`。
2. 不再用固定窗口坐标作为业务通过依据。
3. 每个 `case x mode` 单独重置基线、单独启动应用、单独产出证据。
4. `PASS` 的必要条件变为：
   - `Codex verdict = PASS` 或 `human verdict = PASS`
   - `db assertion = PASS`
5. `postgres_only` / `sqlite_only` / `dual_sync` 的数据库证明严格区分，不再混查。

## 视觉门禁
### 运行方式
- 脚本自动：
  1. 重置模式基线。
  2. 启动 Tauri 窗口。
  3. 采集视觉截图和数据库证明。
  4. 在终端打印 Codex 多模态 review checklist。
  5. 等待操作者输入 `codex_verdict=PASS|FAIL` 与可选备注。

### 固定 checklist
- 关键面板是否完整显示。
- 系列顺序与状态标签是否符合基线。
- 时间线截图是否打开到正确对象。
- 失败态截图是否存在明确反馈。
- 是否存在空白块、裁切、重叠、假成功。

### 证据输出
- `P5-T1-VG-PASS_*.txt`
- `P5-T1-VG-FAIL_*.txt`
- 对应截图文件
- mode-specific 数据库证明

## 交互门禁
### 运行方式
- 脚本自动：
  1. 重置模式基线。
  2. 启动 Tauri 窗口。
  3. 逐步打印操作指令。
  4. 每一步暂停，等待人工输入 `PASS|FAIL`。
  5. 记录可选现象说明。
  6. 采集当前窗口截图。
  7. 执行当前步的数据库断言。

### 人工输入格式
- `PASS` 或 `FAIL`
- 可选 `note`
- 任一步人工结果或数据库断言为 `FAIL`，该 case 直接停止并记为 `FAIL`
- 基础设施异常才记为 `BLOCKED`

### IG-PASS 最小链路
1. 创建唯一 Series
2. 提交唯一 Commit
3. 搜索往返
4. 归档 `Project-A`
5. 在 `Archived` 中打开 `Project-A` 时间线

### IG-FAIL 最小链路
1. 空创建
2. 空提交
3. 恢复路径：创建唯一恢复 Series 并提交唯一恢复 Commit

### 数据库断言
- `create_series`: 唯一 Series 数量为 1，状态为 `active`
- `append_commit`: `latest_excerpt` 与 Commit 落库正确，且系列置顶
- `archive_project_a`: `Project-A.status = archived` 且 `archived_at` 非空
- `failure_without_side_effects`: 非法步骤不产生恢复用唯一标识的脏数据
- `dual_sync`: 双库 `series_id / status / excerpt / Project-A status` 一致

## 证据文件结构
### 视觉 txt
- `environment`
- `visual_artifacts`
- `codex_checklist`
- `codex_verdict`
- `codex_note`
- `db_assertion_result`
- `db_proof`
- `conclusion`

### 交互 txt
- `environment`
- 每步固定块：
  - `step_id`
  - `instruction`
  - `human_result`
  - `human_note`
  - `screenshot`
  - `db_assertion_result`
  - `db_proof`
- 结尾：
  - `human_verdict`
  - `final_db_verdict`
  - `conclusion`

## 执行建议
- 首次运行建议全量执行四个 case。
- 若只复测单个 case，可使用脚本参数 `-Cases P5-T1-IG-PASS` 这类形式缩小范围。
- 若只想复用交互门禁而跳过 Rust 自动化基线，可使用 `-SkipAutomationBaseline`。

## 历史说明
- 2026-03-18 产出的旧版 `P5-T1` 证据仍可用于复盘，但不再符合当前发布级证明标准。
- 新版脚本的目标是消除“桌面动作像是通过，但数据库未证明闭环”的误判。

## 追踪映射
- source gate: `qa-gates/phase-5/p5-t1-full-regression.md`
- DEV.md 映射: 三模式启动与读写
- ROADMAP.md 映射: Phase 5 / 子任务 1
