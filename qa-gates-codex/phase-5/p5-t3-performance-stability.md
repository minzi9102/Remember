# P5-T3 性能与稳定性门禁 Codex 执行版门禁测试

## 目标
- 验证提交延迟与热键响应满足稳定性目标。
- 门禁焦点: 性能趋势可视

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P5-T3-VG-PASS` / `P5-T3-VG-FAIL` / `P5-T3-IG-PASS` / `P5-T3-IG-FAIL` |
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
- 仅迁移交互门禁：`P5-T3-IG-PASS`、`P5-T3-IG-FAIL`。
- 保留视觉门禁在 `playwright`，继续用于趋势展示和基础可见性检查。
- 迁移原因：本任务同时覆盖“提交延迟”和“热键响应”，其中热键响应必须在真实 `desktop_window` 中测量才有发布意义。

## 视觉门禁
### P5-T3-VG-PASS
- case_id: `P5-T3-VG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 确认应用已启动并可访问 `$env:TARGET_URL`。
  2. 准备会话 ID：`P5T3-VG-PASS`。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P5T3-VG-PASS open $env:TARGET_URL --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P5T3-VG-PASS snapshot`
  3. 根据当前子任务流程完成关键操作并截图（`screenshot`）。
  4. 记录 `性能趋势可视` 对应的可见结果。
  5. `npx --yes --package @playwright/cli playwright-cli -s=P5T3-VG-PASS close`
- oracle:
  1. 关键界面元素完整显示，无错位/遮挡。
  2. `性能趋势可视` 对应成功态可见。
- evidence:
  - `P5-T3-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P5-T3-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P5-T3-VG-FAIL
- case_id: `P5-T3-VG-FAIL`
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
  - `P5-T3-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P5-T3-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P5-T3-IG-PASS
- case_id: `P5-T3-IG-PASS`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 使用 WebDriver 会话 ID：`P5T3-IG-PASS`。
  2. 准备一条真实桌面交互链路，至少覆盖一次热键呼出和一次提交操作。
- steps:
  1. 通过 WebDriver 连接真实窗口并记录起始时间戳。
  2. 发送热键呼出或聚焦动作，记录窗口响应时间。
  3. 在同一会话内执行最小提交链路，记录提交完成和界面刷新耗时。
  4. 使用 `screenshot` 与日志/查询结果固化性能证据。
- oracle:
  1. 热键响应与提交链路在真实桌面窗口下完整执行，无卡死或不可恢复状态。
  2. 结果可通过窗口状态、耗时记录和日志/查询交叉验证。
- evidence:
  - `P5-T3-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P5-T3-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P5-T3-IG-FAIL
- case_id: `P5-T3-IG-FAIL`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 准备非法交互（无效输入、重复提交、冲突快捷键等）。
- steps:
  1. 通过 WebDriver 触发会导致响应超时、重复提交或热键冲突的反向路径。
  2. 捕获错误提示、超时日志或性能降级证据，并用 `screenshot` 固化现场。
  3. 立即执行一次合法热键/提交链路验证系统可恢复。
- oracle:
  1. 反向路径被明确拦截或记录性能异常，不能静默失败。
  2. 系统不崩溃，恢复后合法链路可继续完成。
- evidence:
  - `P5-T3-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P5-T3-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-5/p5-t3-performance-stability.md`
- DEV.md 映射: 提交延迟/热键响应
- ROADMAP.md 映射: Phase 5 / 子任务 3
