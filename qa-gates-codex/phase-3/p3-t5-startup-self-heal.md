# P3-T5 启动自愈扫描 Codex 执行版门禁测试

## 目标
- 验证应用启动时能扫描并尝试修复未决一致性告警。
- 门禁焦点: 自愈前后状态对比

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P3-T5-VG-PASS` / `P3-T5-VG-FAIL` / `P3-T5-IG-PASS` / `P3-T5-IG-FAIL` |
| skill_chain | `playwright`（默认） / `playwright + screenshot`（桌面回退） |
| target_mode | `web_url`（默认） / `desktop_window`（回退） |
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

## 视觉门禁
### P3-T5-VG-PASS
- case_id: `P3-T5-VG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 确认应用已启动并可访问 `$env:TARGET_URL`。
  2. 准备会话 ID：`P3T5-VG-PASS`。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P3T5-VG-PASS open $env:TARGET_URL --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P3T5-VG-PASS snapshot`
  3. 根据当前子任务流程完成关键操作并截图（`screenshot`）。
  4. 记录 `自愈前后状态对比` 对应的可见结果。
  5. `npx --yes --package @playwright/cli playwright-cli -s=P3T5-VG-PASS close`
- oracle:
  1. 关键界面元素完整显示，无错位/遮挡。
  2. `自愈前后状态对比` 对应成功态可见。
- evidence:
  - `P3-T5-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P3-T5-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P3-T5-VG-FAIL
- case_id: `P3-T5-VG-FAIL`
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
  - `P3-T5-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P3-T5-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P3-T5-IG-PASS
- case_id: `P3-T5-IG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 使用会话 ID：`P3T5-IG-PASS`。
  2. 准备一条合法交互链路（输入、提交、切换或导航）。
- steps:
  1. `open -> snapshot` 后执行合法交互链路。
  2. 记录每一步操作与系统反馈。
  3. 导出日志或查询结果作为交互佐证。
  4. 关闭会话。
- oracle:
  1. 交互链路完整，无卡死或不可恢复状态。
  2. 结果可通过 UI + 日志/查询交叉验证。
- evidence:
  - `P3-T5-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P3-T5-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P3-T5-IG-FAIL
- case_id: `P3-T5-IG-FAIL`
- skill_chain: `playwright`（必要时 `+ screenshot`）
- target_mode: `web_url` 或 `desktop_window`
- setup:
  1. 准备非法交互（无效输入、重复提交、冲突快捷键等）。
- steps:
  1. 触发非法交互并观察系统拦截。
  2. 捕获错误提示与系统稳定性证据。
  3. 立即执行一次合法交互验证可恢复。
- oracle:
  1. 非法交互被拒绝并给出明确提示。
  2. 系统不崩溃，合法操作可继续完成。
- evidence:
  - `P3-T5-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P3-T5-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-3/p3-t5-startup-self-heal.md`
- DEV.md 映射: 启动自愈扫描
- ROADMAP.md 映射: Phase 3 / 子任务 5
