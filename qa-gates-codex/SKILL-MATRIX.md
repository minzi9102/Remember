# SKILL MATRIX

| Skill | 来源 | 当前状态 | 用途 | 风险级别 | 首批纳入 |
|---|---|---|---|---|---|
| playwright | local installed | 已安装 | Web/Electron 黑盒视觉+交互自动化 | 低 | 是 |
| screenshot | openai/skills curated | 已安装 | 桌面窗口/系统级截图补充 | 低-中 | 是 |
| playwright-interactive | openai/skills curated | 未安装 | 持久化 js_repl 交互调试 | 高（需 danger-full-access） | 否（延后） |

## 默认 skill_chain
1. `web_url`: `playwright`
2. `desktop_window`: `playwright + screenshot`

## 决策规则
1. 首选低风险、可审计、命令可复现的 skill。
2. 需要 `danger-full-access` 或长期 REPL 的 skill 默认不纳入首批执行链。
3. 每次安装前必须先更新 `SAFETY-REVIEW.md`。
