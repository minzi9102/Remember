# Remember 纯后端路线图（Windows + Rust Core + Local IPC）

## Summary
- 当前主线已从 `Tauri + React` 硬切为纯后端工程，仓库主交付为 `IPC 服务 + CLI`。
- 平台承诺锁定 Windows，本地 IPC 以 Named Pipe 为生产通道，Loopback 仅调试可选。
- 业务契约保持不变：6 条 RPC 路径、SQLite 语义、错误码语义持续兼容。

## 全局约束
- 运行模式固定 `sqlite_only`，不支持运行时后端切换。
- 远程网络 API 不开放，默认只允许本机 IPC。
- 任务编号体系切换为 `B*`，历史 `P*` 仅用于追溯。
- 旧 `qa-gates/` 与 `qa-gates-codex/` 已退役，不再作为发布门禁主入口。

## 阶段总览
| Phase | 状态 | 目标 |
|---|---|---|
| B0 | 已完成 | 基线冻结与路线切换 |
| B1 | 已完成 | 仓库硬切到 Rust workspace |
| B2 | 已完成 | IPC 服务化（Named Pipe + 调试 Loopback） |
| B3 | 进行中 | 契约兼容固化与回归 |
| B4 | 进行中 | CLI 交付与联调脚本 |
| B5 | 已完成 | 文档与治理收口 |
| B6 | 待开始 | 切换验收与发布候选 |

## B0-B6 任务清单
| Phase | Task ID | 单功能任务 | 当前状态 | DoD |
|---|---|---|---|---|
| B0 | B0-T1 | 冻结旧主线 tag `tauri-last-stable` | 已完成 | 可从 tag 恢复旧版 |
|  | B0-T2 | 全量重写 ROADMAP 为后端阶段结构 | 已完成 | 无前端/Tauri 主叙事 |
|  | B0-T3 | 旧 `P*` -> 新 `B*` 映射附录 | 已完成 | 历史追溯可用 |
|  | B0-T4 | 改写 PLAN.md 为后端改造计划 | 已完成 | WinUI/WebView2 叙事退出 |
| B1 | B1-T1 | 根目录建立 Cargo workspace | 已完成 | `cargo metadata` 正常 |
|  | B1-T2 | Rust 代码拆分到 `crates/*` | 已完成 | crate 边界稳定 |
|  | B1-T3 | 移除前端与 Tauri 工程资产 | 已完成 | 仓库仅保留后端主链路 |
|  | B1-T4 | 清理 Tauri/Node 构建链 | 已完成 | 主构建链无 tauri/node 依赖 |
| B2 | B2-T1 | Named Pipe 生产通道 | 已完成 | 可处理请求与响应 |
|  | B2-T2 | Loopback 调试通道（默认关） | 已完成 | 环境开关可控 |
|  | B2-T3 | IPC 鉴权（token） | 已完成 | 非法 token 被拒绝 |
|  | B2-T4 | 健康检查与优雅退出 | 已完成 | 健康可观测、停机可控 |
| B3 | B3-T1 | 固化 6 条 RPC 语义 | 已完成 | 契约测试通过 |
|  | B3-T2 | 固化错误码语义 | 已完成 | 映射一致 |
|  | B3-T3 | 固化 SQLite 非破坏兼容 | 进行中 | 历史库可启动 |
|  | B3-T4 | 保留 legacy 配置 warning | 进行中 | 行为与文档一致 |
| B4 | B4-T1 | `remember-cli health` | 已完成 | 可返回服务健康 |
|  | B4-T2 | `remember-cli rpc call` | 已完成 | 覆盖 6 条 RPC 调用 |
|  | B4-T3 | 服务启动/连通 smoke 脚本 | 已完成 | 一键联通验证 |
| B5 | B5-T1 | 重写 README/RELEASE 后端口径 | 已完成 | 无前端/Tauri 主路径引用 |
|  | B5-T2 | 删除 `qa-gates*` | 已完成 | 目录清理完成 |
|  | B5-T3 | ROADMAP 轻量验收矩阵 | 已完成 | 发布前验收可执行 |
|  | B5-T4 | `task.jsonl` 切换到 `B*` | 已完成 | 任务追踪一致 |
| B6 | B6-T1 | 全量测试与回归 | 待开始 | 单元/集成/契约通过 |
|  | B6-T2 | 性能基线重建 | 待开始 | 指标可追溯 |
|  | B6-T3 | 发布候选与回滚说明 | 待开始 | Go/No-Go 可执行 |

## 轻量验收矩阵（替代旧 qa-gates）
| 维度 | 最低通过标准 | 当前状态 |
|---|---|---|
| 构建 | `cargo check --workspace` 通过 | 已通过 |
| IPC | Named Pipe 请求/响应闭环可用 | 已通过 |
| CLI | `health` 与 `rpc call` 可调用 | 已通过 |
| 契约 | 6 条 RPC 路径语义保持兼容 | 已通过 |
| 数据 | 历史 SQLite 非破坏兼容 | 进行中 |
| 发布 | 后端发布清单与回滚可执行 | 待开始 |

## 历史映射（P* -> B*）
| 旧编号 | 新编号 | 说明 |
|---|---|---|
| P4-T4（旧 ROADMAP 重建） | B0-T2 | 路线图从 SQLite-only 前端叙事切换为纯后端 |
| P5-T3（发布文档） | B5-T1/B6-T3 | 迁移为后端发布与回滚文档体系 |
| P6-T2~P6-T6（前端交互重构） | 退役 | 前端目标在本主线不再承诺 |
