# RUNBOOK

## 1) 前置检查（必须）
```powershell
Get-Command npx -ErrorAction SilentlyContinue
npx --yes --package @playwright/cli playwright-cli --help | Select-Object -First 20
Test-Path "$env:USERPROFILE\.codex\skills\screenshot\scripts\take_screenshot.ps1"
```

## 2) 环境变量（示例）
```powershell
$env:TARGET_URL = 'http://127.0.0.1:3000'
$env:APP_WINDOW = 'Remember'
$env:ENV_ID = 'ENV-SQLITE'
$env:TESTER = 'codex'
$env:RUN_DATE = (Get-Date -Format 'yyyyMMdd')
$env:PW_BROWSER = 'msedge'
```

## 3) 执行规范
1. 每个子任务执行顺序: `VG-PASS -> IG-PASS -> VG-FAIL -> IG-FAIL`
2. `web_url` 场景优先用 `playwright`
3. `desktop_window` 场景补充使用 `screenshot`
4. 每条用例必须产出截图/录屏/文本证据

## 4) 命令模板
### Playwright 模板
```powershell
npx --yes --package @playwright/cli playwright-cli -s=<SESSION> open $env:TARGET_URL --browser $env:PW_BROWSER
npx --yes --package @playwright/cli playwright-cli -s=<SESSION> snapshot
npx --yes --package @playwright/cli playwright-cli -s=<SESSION> screenshot
npx --yes --package @playwright/cli playwright-cli -s=<SESSION> close
```

### Screenshot 模板
```powershell
powershell -ExecutionPolicy Bypass -File "$env:USERPROFILE\.codex\skills\screenshot\scripts\take_screenshot.ps1" -Mode temp -ActiveWindow
```

## 5) 失败回退
1. 立即 `playwright-cli close`
2. 保留当前截图、日志、复现步骤
3. 用 `BLOCKED` 或 `FAIL` 更新矩阵并记录原因

## 6) P1-T2 双轨补充
1. `VG` 用例优先 `web_url`，通过 URL 参数注入 `runtime_mode` 与 `warning` 做页面可视判定。
2. `IG` 用例必须走 `desktop_window`，以修改 `config.toml + 重启` 验证真实生效链路。
3. 文本证据固定结构：`输入动作 -> 可视反馈 -> 日志佐证 -> 结论`。
