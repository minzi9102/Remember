# P5-T1 全模式回归门禁 Codex 执行版门禁测试

## 目标
- 验证三运行模式关键路径回归结果完整。
- 门禁焦点: 三模式回归看板

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P5-T1-VG-PASS` / `P5-T1-VG-FAIL` / `P5-T1-IG-PASS` / `P5-T1-IG-FAIL` |
| skill_chain | `视觉: playwright` / `交互: webdriver + screenshot` |
| target_mode | `视觉: web_url` / `交互: desktop_window` |
| setup | 黑盒启动 + 环境校验 |
| steps | Codex 命令级步骤 |
| oracle | 可观察判定（UI/日志/查询） |
| evidence | `qa-gates/EVIDENCE-NAMING.md` 命名规范 |

## 执行前变量
```powershell
$env:TARGET_URL = 'http://127.0.0.1:3000'
$env:APP_WINDOW = 'Remember'
$env:ENV_ID = 'ENV-SQLITE'
$env:TESTER = 'codex'
$env:RUN_DATE = (Get-Date -Format 'yyyyMMdd')
$env:PW_BROWSER = 'msedge'
```

## 2026-03-18 WebDriver 第二批迁移决议
- 迁移交互门禁：`P5-T1-IG-PASS`、`P5-T1-IG-FAIL`。
- 保留视觉门禁在 `playwright`，继续用于三模式回归看板、环境切换与基础可见性检查。
- 迁移原因：`P5-T1` 现有失败记录已经表明，桌面采集链路虽可运行，但 `IG` 证据仍无法证明真实 Tauri 窗口上的“真闭环”；后续必须以 `WebDriver` 作为交互主路径补齐发布级证明。

## 视觉门禁
### P5-T1-VG-PASS
- case_id: `P5-T1-VG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 确认应用已启动并可访问 `$env:TARGET_URL`。
  2. 准备会话 ID：`P5T1-VG-PASS`。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P5T1-VG-PASS open $env:TARGET_URL --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P5T1-VG-PASS snapshot`
  3. 根据当前子任务流程完成关键操作并截图（`screenshot`）。
  4. 记录 `三模式回归看板` 对应的可见结果。
  5. `npx --yes --package @playwright/cli playwright-cli -s=P5T1-VG-PASS close`
- oracle:
  1. 关键界面元素完整显示，无错位/遮挡。
  2. `三模式回归看板` 对应成功态可见。
- evidence:
  - `P5-T1-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P5-T1-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P5-T1-VG-FAIL
- case_id: `P5-T1-VG-FAIL`
- skill_chain: `playwright + screenshot`
- target_mode: `web_url` 或 `desktop_window`
- setup:
  1. 制造反向条件（非法输入、模式冲突、依赖不可达）。
  2. 若浏览器路径不可执行，切换 `desktop_window` + `screenshot`。
- steps:
  1. 用与 PASS 相同流程触发反向路径。
  2. 捕获错误提示/降级状态截图。
  3. 使用 `take_screenshot.ps1 -Mode temp -ActiveWindow` 补充桌面证据（可选）。
- oracle:
  1. 存在明确失败反馈，不能静默失败。
  2. 失败后系统仍可继续操作。
- evidence:
  - `P5-T1-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P5-T1-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P5-T1-IG-PASS
- case_id: `P5-T1-IG-PASS`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 使用 WebDriver 会话 ID：`P5T1-IG-PASS`。
  2. 依次准备 `sqlite_only`、`postgres_only`、`dual_sync` 三种真实桌面运行模式。
  3. 为每种模式准备一条最小合法交互链路，至少覆盖一次真实窗口激活、一次输入/提交、一次结果落库或列表回刷验证。
- steps:
  1. 通过 WebDriver 连接当前模式的 Tauri 主窗口并记录模式标识。
  2. 在真实窗口中执行最小合法交互链路，验证输入、提交、导航或状态切换能完整闭环。
  3. 对关键节点使用 `screenshot` 固化桌面证据，并导出日志或查询结果。
  4. 在三种模式下重复执行，确认结果一致或按设计呈现差异。
- oracle:
  1. 三模式下的真实窗口交互链路完整，无卡死、假成功或不可恢复状态。
  2. 每种模式都能通过窗口状态 + 日志/查询证明“输入已生效且结果已闭环落地”。
- evidence:
  - `P5-T1-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P5-T1-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P5-T1-IG-FAIL
- case_id: `P5-T1-IG-FAIL`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 准备非法交互（无效输入、重复提交、冲突快捷键、模式依赖异常或写入失败场景）。
  2. 为三种运行模式分别准备对应的反向条件。
- steps:
  1. 通过 WebDriver 在真实窗口中触发反向路径并观察系统拦截。
  2. 捕获错误提示、失败日志、降级状态或写入未完成的桌面证据，并用 `screenshot` 留证。
  3. 立即执行一次合法交互验证系统可恢复，避免将失败态误判为应用失效。
- oracle:
  1. 反向路径被明确拦截或记录清晰失败信号，不能静默失败。
  2. 系统不崩溃，恢复后合法交互可继续完成。
- evidence:
  - `P5-T1-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P5-T1-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-5/p5-t1-full-regression.md`
- DEV.md 映射: 三模式启动与读写
- ROADMAP.md 映射: Phase 5 / 子任务 1

## 2026-03-18 执行结果（Codex / Docker 临时 Postgres）

### 总结
- 执行脚本：`qa-gates-codex/scripts/run-p5-t1-full-regression.ps1`
- 临时 DSN：`postgres://remember_p5t1:remember_p5t1@localhost:55433/remember_p5t1`
- 运行日志：`C:\Users\99741\AppData\Local\Temp\p5t1-logs-f1d18a8f-9f11-4cf6-ac0f-4dfb0b97c6ae`
- 桌面标题基线：`tauri-app [sqlite_only|postgres_only|dual_sync]`
- 最终结论：`FAIL`
- 发布判定：`P5-T1` 未通过，`task.jsonl` 保持未完成

### 自动化基线
| 检查项 | 结果 | 备注 |
|---|---|---|
| `npm run test:unit` | PASS | 75 tests passed |
| `cargo test --manifest-path src-tauri\Cargo.toml --lib -- --nocapture` | PASS | lib tests 通过 |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p2_t5_basic_read_write_query -- --nocapture` | PASS | sqlite/postgres 基础读写通过 |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p3_t1_dual_sync_repository -- --nocapture` | PASS | dual_sync 基本双写通过 |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p3_t2_parallel_tx_timeout -- --nocapture` | FAIL | `tests/p3_t2_parallel_tx_timeout.rs:79` 期望 `<=4.5s`，实测约 `24.25s` |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p3_t3_rollback_error_codes -- --nocapture` | PASS | rollback/error-code 用例通过 |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p3_t4_single_side_compensation_alerts -- --nocapture` | PASS | consistency alert 用例通过 |
| `cargo test --manifest-path src-tauri\Cargo.toml --test p3_t5_startup_self_heal -- --nocapture` | FAIL | `tests/p3_t5_startup_self_heal.rs:170` 未得到期望的 `RepositoryError::DualWriteFailed(_)` |

### 桌面证据采集
| 环境 | VG-PASS | VG-FAIL | IG-PASS | IG-FAIL |
|---|---|---|---|---|
| `ENV-SQLITE` | 已采集 | 已采集 | 已采集 | 已采集 |
| `ENV-PG` | 已采集 | 已采集 | 已采集 | 已采集 |
| `ENV-DUAL` | 已采集 | 已采集 | 已采集 | 已采集 |

### 证据文件
- `qa-gates-codex/P5-T1-VG-PASS_20260318_ENV-SQLITE_codex.{png,txt}`
- `qa-gates-codex/P5-T1-VG-FAIL_20260318_ENV-SQLITE_codex.{png,txt}`
- `qa-gates-codex/P5-T1-IG-PASS_20260318_ENV-SQLITE_codex.{mp4,txt}`
- `qa-gates-codex/P5-T1-IG-FAIL_20260318_ENV-SQLITE_codex.{mp4,txt}`
- `qa-gates-codex/P5-T1-VG-PASS_20260318_ENV-PG_codex.{png,txt}`
- `qa-gates-codex/P5-T1-VG-FAIL_20260318_ENV-PG_codex.{png,txt}`
- `qa-gates-codex/P5-T1-IG-PASS_20260318_ENV-PG_codex.{mp4,txt}`
- `qa-gates-codex/P5-T1-IG-FAIL_20260318_ENV-PG_codex.{mp4,txt}`
- `qa-gates-codex/P5-T1-VG-PASS_20260318_ENV-DUAL_codex.{png,txt}`
- `qa-gates-codex/P5-T1-VG-FAIL_20260318_ENV-DUAL_codex.{png,txt}`
- `qa-gates-codex/P5-T1-IG-PASS_20260318_ENV-DUAL_codex.{mp4,txt}`
- `qa-gates-codex/P5-T1-IG-FAIL_20260318_ENV-DUAL_codex.{mp4,txt}`

### 失败原因
1. 自动化门禁未全绿：`cargo-p3t2` 与 `cargo-p3t5` 在临时 Docker Postgres 基线上稳定失败，已经满足 `P5-T1` 的失败条件。
2. 交互证据未证明“真闭环”：
   `P5-T1-IG-PASS_20260318_ENV-SQLITE_codex.txt`、`P5-T1-IG-PASS_20260318_ENV-PG_codex.txt`、`P5-T1-IG-PASS_20260318_ENV-DUAL_codex.txt` 都出现重复 `Inbox` 系列记录，且 `Project-A` 仍为 `silent`，没有被证明已归档。
3. 因第 1-2 项未满足，当前 12 组文件只能证明“桌面采集链路可运行”，不能证明 `P5-T1` 的发布级通过。
4. 后续执行要求：
   `P5-T1-IG-PASS` 与 `P5-T1-IG-FAIL` 改由 `WebDriver` 作为主执行链路，`playwright` 不再作为发布级交互证明的默认路径。

### 状态回写
- `qa-gates/MASTER-TRACE-MATRIX.md`：`P5-T1 -> FAIL`
- `qa-gates-codex/MASTER-TRACE-MATRIX.md`：`P5-T1 -> FAIL`
- `task.jsonl`：保持 `{\"task_name\":\"执行全量功能回归（含三模式）。\",\"completed\":false}`
