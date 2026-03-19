# P4-T1 全局热键呼出/隐藏 Codex 执行版门禁测试

## 目标
- 为真实全局热键门禁提供环境诊断、注入限制证据和人工交接信息。
- 明确区分“Codex 可自动诊断的内容”和“必须由物理键盘执行的真实验收”。

## 当前决议
- `qa-gates/phase-4/p4-t1-global-hotkey.md` 仍是唯一权威的真实热键验收基线。
- `qa-gates-codex/scripts/run-p4-t1-global-hotkey.ps1` 只负责：
  1. 预检 Tauri 可执行文件、WebDriver、截图脚本、`uv` Python、helper 与诊断脚本。
  2. 验证桌面窗口可被 WinAppDriver 根会话与 attach-window probe 发现。
  3. 生成注入限制诊断证据，证明 `SendInput` 不能作为真实全局热键的权威判断依据。
- 只要 P4-T1 仍依赖注入式输入，Codex 版状态一律写为 `BLOCKED`，原因固定包含 `physical keyboard verification required`。

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P4-T1-VG-PASS` / `P4-T1-VG-FAIL` / `P4-T1-IG-PASS` / `P4-T1-IG-FAIL` |
| skill_chain | `webdriver + diagnostic` |
| target_mode | `desktop_window` |
| setup | 黑盒启动 + 环境校验 + 注入限制诊断 |
| steps | Codex 命令级预检、attach-window probe、`SendInput` 对照诊断 |
| oracle | 是否已证明“注入输入不等价于真实全局热键”，并给出人工执行下一步 |
| evidence | `qa-gates/EVIDENCE-NAMING.md` 命名规范 + `P4-T1-HOTKEY-DIAG_*` 诊断证据 |

## 执行前变量
```powershell
$env:TARGET_URL = 'http://127.0.0.1:3000'
$env:APP_WINDOW = 'Remember'
$env:ENV_ID = 'ENV-SQLITE'
$env:TESTER = 'codex'
$env:RUN_DATE = (Get-Date -Format 'yyyyMMdd')
$env:PW_BROWSER = 'msedge'
```

## Codex 诊断门禁
### 共同行为
- setup:
  1. 确认 `src-tauri/target/debug/tauri-app.exe` 存在。
  2. 确认 `http://127.0.0.1:4723` 可完成 root-session 与 attach-window probe。
  3. 运行 `qa-gates-codex/scripts/diagnose-hotkey-injection-limits.ps1`。
- oracle:
  1. 诊断证据必须能说明 `SendInput` 仅是合成输入，不能替代真实物理热键。
  2. 每个 case txt 必须明确写出 `physical keyboard verification required`。
  3. 每个 case txt 必须给出人工门禁文档的下一步路径。
- evidence:
  - `P4-T1-*-*_...txt`
  - `P4-T1-HOTKEY-DIAG_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`

### P4-T1-VG-PASS / P4-T1-VG-FAIL / P4-T1-IG-PASS / P4-T1-IG-FAIL
- Codex 版不再尝试用注入式热键直接判定 `PASS` 或 `FAIL`。
- 若环境预检失败：标记 `BLOCKED`，记录具体 precheck / root-session / attach-window 卡点。
- 若环境预检通过：仍标记 `BLOCKED`，并附注入限制诊断证据，提示转交人工真实热键执行。

## 人工交接要求
- 人工执行入口：`qa-gates/phase-4/p4-t1-global-hotkey.md`
- 执行方式：必须使用真实物理键盘触发热键，不接受 `SendInput`、WebDriver `/actions`、脚本模拟键盘作为最终结论。
- 只有人工真实热键门禁才允许把 P4-T1 记为 `PASS` 或 `FAIL`。

## 追踪映射
- source gate: `qa-gates/phase-4/p4-t1-global-hotkey.md`
- diagnostic runner: `qa-gates-codex/scripts/run-p4-t1-global-hotkey.ps1`
- injection diagnostic: `qa-gates-codex/scripts/diagnose-hotkey-injection-limits.ps1`
- DEV.md 映射: tauri-plugin-global-shortcut
- ROADMAP.md 映射: Phase 4 / 子任务 1
