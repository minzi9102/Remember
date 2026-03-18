# P4-T1 全局热键呼出/隐藏 Codex 执行版门禁测试

## 目标
- 验证热键可稳定呼出与隐藏主界面。
- 门禁焦点: 热键可见切换

## 可执行测试接口
| 字段 | 值 |
|---|---|
| case_id | `P4-T1-VG-PASS` / `P4-T1-VG-FAIL` / `P4-T1-IG-PASS` / `P4-T1-IG-FAIL` |
| skill_chain | `webdriver + screenshot`（默认） / `playwright + screenshot`（回退） |
| target_mode | `desktop_window`（默认） / `web_url`（回退） |
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
- 本任务 4 条用例全部迁入 `WebDriver` 主路径。
- 迁移原因：该任务验证对象是 `tauri-plugin-global-shortcut`，必须在真实 `desktop_window` 中证明热键呼出/隐藏，而不是仅在 `web_url` 中观察页面状态。
- 执行策略：`WebDriver` 负责真实窗口激活、热键注入、可见性断言；`screenshot` 继续保留为桌面证据采集；`playwright` 仅作为环境受限时的回退链路。

## 视觉门禁
### P4-T1-VG-PASS
- case_id: `P4-T1-VG-PASS`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 确认 Tauri 桌面应用已启动，窗口标题匹配 `$env:APP_WINDOW`。
  2. 准备 WebDriver 会话 ID：`P4T1-VG-PASS`。
- steps:
  1. 通过 `WebDriver` 连接 Tauri 驱动并定位主窗口。
  2. 激活非应用窗口或桌面，建立“应用当前隐藏/失焦”的前置状态。
  3. 发送全局热键并断言 `$env:APP_WINDOW` 被拉起且可见。
  4. 使用 `screenshot` 记录呼出后的窗口可见状态。
  5. 再次发送全局热键并断言窗口隐藏或回到后台。
- oracle:
  1. 热键在真实桌面环境下可从非应用焦点状态呼出主窗口。
  2. 第二次热键触发后窗口状态按设计切回隐藏或后台。
- evidence:
  - `P4-T1-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P4-T1-VG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并保留现有截图。
  2. 标记 `FAIL/BLOCKED`，记录卡点与复现步骤。

### P4-T1-VG-FAIL
- case_id: `P4-T1-VG-FAIL`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 制造反向条件（热键冲突、插件注册失败、窗口句柄不可用）。
  2. 准备 WebDriver 会话并记录当前桌面焦点窗口。
- steps:
  1. 在反向条件下发送全局热键。
  2. 断言窗口未发生错误呼出，或出现明确降级提示/日志。
  3. 使用 `screenshot` 采集失败态与当前焦点窗口证据。
  4. 清理反向条件后复跑一次 PASS 路径确认可恢复。
- oracle:
  1. 失败路径存在明确反馈，不能出现“热键无响应但无日志/提示”的静默失败。
  2. 清理后热键能力可恢复。
- evidence:
  - `P4-T1-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.png`
  - `P4-T1-VG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭当前会话。
  2. 恢复正常配置后复测一次 PASS 路径。

## 交互门禁
### P4-T1-IG-PASS
- case_id: `P4-T1-IG-PASS`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 使用 WebDriver 会话 ID：`P4T1-IG-PASS`。
  2. 准备“失焦 -> 热键呼出 -> 聚焦输入 -> 再次热键隐藏”的合法交互链路。
- steps:
  1. 通过 WebDriver 让应用处于后台或被其他窗口遮挡。
  2. 发送全局热键，断言应用前置并可接收键盘焦点。
  3. 在主窗口执行一段最小合法输入或导航，证明呼出的窗口可交互。
  4. 再次发送全局热键，断言窗口隐藏后不会残留焦点异常。
  5. 导出日志或窗口状态记录作为交互佐证。
- oracle:
  1. 热键驱动的前后台切换与后续交互链路完整，无卡死或焦点丢失。
  2. 结果可通过窗口状态 + 日志交叉验证。
- evidence:
  - `P4-T1-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P4-T1-IG-PASS_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 保存最后一步前后状态截图。
  2. 标记缺陷并附复现步骤。

### P4-T1-IG-FAIL
- case_id: `P4-T1-IG-FAIL`
- skill_chain: `webdriver + screenshot`
- target_mode: `desktop_window`
- setup:
  1. 准备非法交互（重复热键、冲突快捷键、窗口销毁后重发热键等）。
- steps:
  1. 通过 WebDriver 触发非法热键交互并观察系统拦截。
  2. 捕获错误提示、窗口异常状态或日志中的降级记录。
  3. 立即执行一次合法热键呼出验证系统可恢复。
- oracle:
  1. 非法热键交互被拒绝并给出明确提示或日志。
  2. 系统不崩溃，后续合法热键操作可继续完成。
- evidence:
  - `P4-T1-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.mp4`
  - `P4-T1-IG-FAIL_$env:RUN_DATE_$env:ENV_ID_$env:TESTER.txt`
- failure rollback:
  1. 关闭会话并重启应用。
  2. 若复测仍失败，升级为阻断级。

## 追踪映射
- source gate: `qa-gates/phase-4/p4-t1-global-hotkey.md`
- DEV.md 映射: tauri-plugin-global-shortcut
- ROADMAP.md 映射: Phase 4 / 子任务 1
