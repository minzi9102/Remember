# P4-T3 键盘优先交互 Codex 执行版门禁测试

## 目标
- 验证核心快捷键在不同焦点场景下行为稳定。
- 门禁焦点: 全键盘流可操作

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P4-T3-VG-PASS` / `P4-T3-VG-FAIL` / `P4-T3-IG-PASS` / `P4-T3-IG-FAIL` |
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

## 2026-03-18 WebDriver 最小迁移决议
- 仅迁移交互门禁：`P4-T3-IG-PASS`、`P4-T3-IG-FAIL`。
- 保留视觉门禁在 `playwright`，因为页面布局与提示可继续在 `web_url` 下高效验证。
- 迁移原因：当前阻塞点是“真实桌面键盘注入是否稳定”，而不是纯视觉截图是否可见。

## 视觉门禁
### P4-T3-VG-PASS
- case_id: `P4-T3-VG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 确认应用已启动并可访问 `$env:TARGET_URL`。
  2. 准备会话 ID：`P4T3-VG-PASS`。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P4T3-VG-PASS open $env:TARGET_URL --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P4T3-VG-PASS snapshot`
  3. 根据当前子任务流程完成关键操作并截图（`screenshot`）。
  4. 记录 `全键盘流可操作` 对应的可见结果。
  5. `npx --yes --package @playwright/cli playwright-cli -s=P4T3-VG-PASS close`
- oracle:
  1. 关键界面元素完整显示，无错位/遮挡。
  2. `全键盘流可操作` 对应成功态可见。
- evidence:
  - `P4-T3-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P4-T3-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P4-T3-VG-FAIL
- case_id: `P4-T3-VG-FAIL`
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
  - `P4-T3-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P4-T3-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P4-T3-IG-PASS
- case_id: `P4-T3-IG-PASS`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 使用 WebDriver 会话 ID：`P4T3-IG-PASS`。
  2. 准备一条真实桌面键盘链路：`↑/↓/Enter/Esc/←/→/Shift+N/a` 中至少覆盖当前子任务要求的主路径组合。
- steps:
  1. 通过 WebDriver 聚焦 Tauri 主窗口并确认焦点在可交互区域。
  2. 发送目标快捷键序列，逐步断言选中项、视图切换、提交或返回动作的真实窗口反馈。
  3. 对关键节点使用 `screenshot` 补充桌面证据。
  4. 导出日志或查询结果，证明快捷键动作已落到真实状态变更而非仅页面假响应。
- oracle:
  1. 真实桌面键盘链路完整，无卡死、丢焦或输入被系统吞掉的情况。
  2. 结果可通过窗口反馈 + 日志/查询交叉验证。
- evidence:
  - `P4-T3-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P4-T3-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P4-T3-IG-FAIL
- case_id: `P4-T3-IG-FAIL`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 准备非法交互（无效输入、重复提交、冲突快捷键等）。
- steps:
  1. 通过 WebDriver 在真实窗口中触发非法快捷键组合或错误焦点场景。
  2. 捕获错误提示、无效态提示或被拒绝的窗口反馈，并用 `screenshot` 留证。
  3. 立即执行一次合法快捷键链路验证系统可恢复。
- oracle:
  1. 非法快捷键交互被拒绝并给出明确提示，不能静默失败。
  2. 系统不崩溃，合法快捷键操作可继续完成。
- evidence:
  - `P4-T3-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P4-T3-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-4/p4-t3-keyboard-shortcuts.md`
- DEV.md 映射: ↑/↓/Enter/Esc/←/→//Shift+N/a
- ROADMAP.md 映射: Phase 4 / 子任务 3

## 2026-03-18 执行结果
| case_id | result | evidence |
|---|---|---|
| `P4-T3-VG-PASS` | PASS | `qa-gates-codex/P4-T3-VG-PASS_20260318_ENV-SQLITE_codex.png` + `qa-gates-codex/P4-T3-VG-PASS_20260318_ENV-SQLITE_codex.txt` |
| `P4-T3-VG-FAIL` | PASS | `qa-gates-codex/P4-T3-VG-FAIL_20260318_ENV-SQLITE_codex.png` + `qa-gates-codex/P4-T3-VG-FAIL_20260318_ENV-SQLITE_codex.txt` |
| `P4-T3-IG-PASS` | BLOCKED | `qa-gates-codex/P4-T3-IG-PASS_20260318_ENV-SQLITE_codex.mp4` + `qa-gates-codex/P4-T3-IG-PASS_20260318_ENV-SQLITE_codex.txt` |
| `P4-T3-IG-FAIL` | BLOCKED | `qa-gates-codex/P4-T3-IG-FAIL_20260318_ENV-SQLITE_codex.mp4` + `qa-gates-codex/P4-T3-IG-FAIL_20260318_ENV-SQLITE_codex.txt` |

- target_mode: `desktop_window` required for `IG` cases, `playwright` sqlite-bridge fallback downgraded to temporary contingency only.
- overall: `BLOCKED`
- blocker: direct desktop keyboard injection stayed unstable; `IG` cases therefore remain pending migration to `WebDriver` primary execution so interaction persistence/write assertions can be certified on the real Tauri window.
