# P5-T1 全模式回归门禁 混合测试方案

## 目标
- 验证三运行模式关键路径回归结果完整。
- 本文件聚焦：三模式启动与读写的发布级闭环证明。

## 前置条件
1. 已获取本版本可执行程序、启动命令与配置文件访问权限。
2. 可重启应用并允许切换 `sqlite_only` / `postgres_only` / `dual_sync`。
3. 可查看日志目录，且可执行 SQLite / PostgreSQL 查询。
4. 允许在交互门禁中由人工执行真实桌面操作。

## 测试数据
| 数据项 | 规则 |
|---|---|
| 运行模式 | `sqlite_only` / `postgres_only` / `dual_sync` |
| 基线 Series | `Anchor Series`、`Project-A` |
| 交互新建 Series | 每次运行生成唯一名称，禁止复用 `Inbox` |
| 交互 Commit | 每次运行生成唯一内容，禁止复用通用短词 |

## 门禁结构
- 视觉门禁：脚本统一采集截图和数据库证明，再由 Codex 多模态按固定 checklist 审核 `PASS/FAIL`。
- 交互门禁：脚本负责环境重置、步骤提示、截图和数据库断言；人工执行真实操作并输入 `PASS/FAIL`，可补充现象说明。
- 结论规则：只有当 `Codex/人工结论 = PASS` 且 `数据库断言 = PASS` 时，case 才能记为 `PASS`。

## 视觉门禁（2例）
### P5-T1-VG-PASS
- 用例目的：验证正常路径下关键界面可见且无明显视觉回归。
- 脚本职责：
  1. 重置当前模式基线并启动应用。
  2. 采集至少 2 张截图：主列表、时间线。
  3. 导出当前模式的数据库基线证明。
- Codex 多模态 checklist：
  1. 一级列表和二级时间线面板完整可见。
  2. 基线顺序正确，顶部应为 `Anchor Series`。
  3. `Project-A` 的沉寂态未出现明显错标或错位。
  4. 无截断、重叠、空白块、假成功提示。
- 预期结果：
  1. Codex 多模态审核为 `PASS`。
  2. 数据库证明显示 `Anchor Series=active`、`Project-A=silent`。
- 必交证据：`P5-T1-VG-PASS_*.txt` + 对应截图 + 数据库证明

### P5-T1-VG-FAIL
- 用例目的：验证反向条件下存在明确可视保护。
- 脚本职责：
  1. 触发一次空创建等无效动作。
  2. 采集失败态截图。
  3. 导出基线未被破坏的数据库证明。
- Codex 多模态 checklist：
  1. 有明确验证/失败反馈。
  2. 界面不崩溃、不空白。
  3. 不能静默接受非法输入。
- 预期结果：
  1. Codex 多模态审核为 `PASS`。
  2. 数据库仍保持基线状态。
- 必交证据：`P5-T1-VG-FAIL_*.txt` + 对应截图 + 数据库证明

## 交互门禁（2例）
### P5-T1-IG-PASS
- 用例目的：验证正向交互可形成真实闭环。
- 执行方式：人工按脚本提示完成操作，脚本在每一步记录人工 `PASS/FAIL` 和数据库断言结果。
- 最小操作链：
  1. `Shift+N` 创建唯一 Series。
  2. 提交唯一 Commit。
  3. 执行一次搜索往返。
  4. 归档 `Project-A`。
  5. 切换到 `Archived` 并打开 `Project-A` 时间线。
- 预期结果：
  1. 每一步人工观察都为 `PASS`。
  2. 数据库断言至少覆盖：
     - 新建 Series 唯一且状态为 `active`
     - 提交后的 `latest_excerpt` 与 Commit 落库正确
     - `Project-A` 变为 `archived` 且 `archived_at` 非空
     - `dual_sync` 下双库关键字段一致
- 必交证据：`P5-T1-IG-PASS_*.txt` + 每步截图 + 数据库证明

### P5-T1-IG-FAIL
- 用例目的：验证反向交互被正确拦截且系统可恢复。
- 执行方式：人工按脚本提示执行非法动作，再执行一次恢复路径。
- 最小操作链：
  1. 空创建。
  2. 空提交。
  3. 恢复路径：创建唯一恢复 Series 并提交唯一恢复 Commit。
- 预期结果：
  1. 非法步骤人工观察都为 `PASS`，且明确看到失败提示。
  2. 非法步骤数据库无脏数据副作用。
  3. 恢复路径成功，系统回到可用状态。
- 必交证据：`P5-T1-IG-FAIL_*.txt` + 每步截图 + 数据库证明

## 证据格式
### 视觉门禁
- 输出：`txt + screenshot(s) + db-proof`
- 文本至少包含：
  - 环境信息
  - 截图路径
  - Codex checklist
  - `codex_verdict`
  - `codex_note`
  - `db_assertion_result`
  - `conclusion`

### 交互门禁
- 输出：`txt + per-step screenshot(s) + db-proof`
- 每一步固定记录：
  - `step_id`
  - `instruction`
  - `human_result`
  - `human_note`
  - `db_assertion_result`
  - `db_proof`
- 最终文本必须包含：
  - `human_verdict`
  - `final_db_verdict`
  - `conclusion`

## 通过标准
1. 四个 case 都形成完整证据链。
2. 任一 case 若 `观察结论 != PASS` 或 `数据库断言 != PASS`，该 case 视为 `FAIL`。
3. `postgres_only` 只允许 PostgreSQL 证明。
4. `sqlite_only` 只允许 SQLite 证明。
5. `dual_sync` 必须同时给出双库证明并标明一致性结果。

## 失败分级
| 等级 | 判定标准 | 示例 |
|---|---|---|
| 阻断 | 主路径不可用或闭环无法证明 | 交互无法继续、双库状态冲突、脚本无法采证 |
| 高 | 功能结果错误但仍可局部操作 | 归档未落库、恢复路径错误、假成功 |
| 中 | 证据不完整或局部状态偏差 | 缺少某步截图、状态标签与数据库不一致 |
| 低 | 轻微视觉问题 | 对齐偏差、局部样式异常 |

## 追踪映射
- DEV.md 映射：三模式启动与读写
- ROADMAP.md 映射：Phase 5 / 子任务 1
