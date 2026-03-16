# P1-T2 config 与运行模式解析 Codex 执行版门禁测试

## 目标
- 验证 config 配置生效并可稳定切换运行模式。
- 门禁焦点: 模式显示与错误提示
- 双轨策略: `web_url` 负责页面可视判定，`desktop_window` 负责真实配置生效闭环。

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P1-T2-VG-PASS` / `P1-T2-VG-FAIL` / `P1-T2-IG-PASS` / `P1-T2-IG-FAIL` |
| skill_chain | `playwright`（web） / `playwright + screenshot`（desktop） |
| target_mode | `web_url`（VG） / `desktop_window`（IG） |
| setup | 黑盒启动 + 环境校验 |
| steps | Codex 命令级步骤 |
| oracle | 可观察判定（UI/日志/查询） |
| evidence | `qa-gates/EVIDENCE-NAMING.md` 命名规范 |

## 执行前变量
```powershell
$env:TARGET_URL = 'http://127.0.0.1:3000'
$env:TARGET_URL_PASS = "$env:TARGET_URL/?runtime_mode=sqlite_only"
$env:TARGET_URL_FAIL = "$env:TARGET_URL/?runtime_mode=invalid_mode&warning=invalid-runtime-mode"
$env:APP_WINDOW = 'Remember'
$env:CONFIG_PATH = "$env:APPDATA\\com.remember.app\\config.toml"
$env:ENV_ID = 'ENV-SQLITE'
$env:TESTER = 'codex'
$env:RUN_DATE = (Get-Date -Format 'yyyyMMdd')
$env:PW_BROWSER = 'msedge'
```

## 视觉门禁
### P1-T2-VG-PASS
- case_id: `P1-T2-VG-PASS`
- skill_chain: `playwright`
- target_mode: `web_url`
- setup:
  1. 确认应用已启动并可访问 `$env:TARGET_URL_PASS`。
  2. 准备会话 ID：`P1T2-VG-PASS`。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P1T2-VG-PASS open $env:TARGET_URL_PASS --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P1T2-VG-PASS snapshot`
  3. 截图并核对 `runtime-mode-badge` 为 `sqlite_only`，`config-ok-banner` 可见。
  4. 记录 `模式显示与错误提示` 对应的可见结果。
  5. `npx --yes --package @playwright/cli playwright-cli -s=P1T2-VG-PASS close`
- oracle:
  1. 关键界面元素完整显示，无错位/遮挡。
  2. 页面存在 `runtime-mode-badge`，且值与 URL 指定模式一致。
- evidence:
  - `P1-T2-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P1-T2-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P1-T2-VG-FAIL
- case_id: `P1-T2-VG-FAIL`
- skill_chain: `playwright + screenshot`
- target_mode: `web_url`
- setup:
  1. 使用非法模式参数访问 `$env:TARGET_URL_FAIL`。
  2. 若浏览器路径不可执行，切换 `desktop_window` + `screenshot` 仅做证据补图。
- steps:
  1. `npx --yes --package @playwright/cli playwright-cli -s=P1T2-VG-FAIL open $env:TARGET_URL_FAIL --browser $env:PW_BROWSER`
  2. `npx --yes --package @playwright/cli playwright-cli -s=P1T2-VG-FAIL snapshot`
  3. 捕获 `config-warning-banner` 和 fallback 标签截图。
  4. 使用 `take_screenshot.ps1 -Mode temp -ActiveWindow` 补充桌面证据（可选）。
- oracle:
  1. 存在明确失败反馈，不能静默失败。
  2. `runtime-mode-badge` 回退为 `sqlite_only`，且 warning 明确包含非法模式信息。
- evidence:
  - `P1-T2-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P1-T2-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P1-T2-IG-PASS
- case_id: `P1-T2-IG-PASS`
- skill_chain: `playwright + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 在 `$env:CONFIG_PATH` 写入合法配置（例如 `runtime_mode="dual_sync"`）。
  2. 重启 Tauri 应用并确认窗口标题变化为 `...[dual_sync]`。
- steps:
  1. 捕获应用窗口截图（标题含 `dual_sync`）。
  2. 记录配置修改动作、重启动作、重启后 UI 诊断区模式值。
  3. 导出启动日志中 `runtime_mode=dual_sync` 片段作为佐证。
- oracle:
  1. 配置修改后重启生效，模式显示发生对应变化。
  2. 结果可通过 UI + 日志交叉验证。
- evidence:
  - `P1-T2-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P1-T2-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P1-T2-IG-FAIL
- case_id: `P1-T2-IG-FAIL`
- skill_chain: `playwright`（必要时 `+ screenshot`）
- target_mode: `desktop_window`
- setup:
  1. 在 `$env:CONFIG_PATH` 写入非法配置（例如 `runtime_mode="invalid_mode"`）。
  2. 重启应用，准备恢复配置步骤。
- steps:
  1. 启动后观察 UI 诊断区：模式应回退 `sqlite_only`，warning 可见。
  2. 捕获错误提示与系统稳定性证据（标题或诊断区含 fallback 信息）。
  3. 恢复合法配置并重启，确认可恢复到预期模式。
- oracle:
  1. 非法配置被拦截并给出明确提示（非静默）。
  2. 系统不崩溃，恢复后合法配置可继续完成。
- evidence:
  - `P1-T2-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P1-T2-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-1/p1-t2-config-runtime-mode.md`
- DEV.md 映射: 本地配置文件(config.toml)
- ROADMAP.md 映射: Phase 1 / 子任务 2

## 文本证据模板
每个 `*.txt` 至少按以下结构记录：
1. 输入动作：配置或 URL 参数改动、启动/重启动作。
2. 可视反馈：`runtime-mode-badge`、`config-warning-banner`、窗口标题结果。
3. 日志佐证：`[remember][config]` 关键行。
4. 结论：PASS/FAIL/BLOCKED 与原因。
