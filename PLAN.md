# Remember 后端改造计划（Hard Cut）

## Summary
- 目标：将仓库改造为纯后端项目，主交付为 `remember-ipc-server` 与 `remember-cli`。
- 平台：Windows 锁定，生产通道 Named Pipe，Loopback 仅调试。
- 兼容目标：保持 6 条 RPC 业务语义、错误码语义与 SQLite 非破坏兼容。

## 已落地结构
- 根目录已切换为 Cargo workspace。
- crate 划分：
  - `crates/remember-core`
  - `crates/remember-sqlite`
  - `crates/remember-ipc-server`
  - `crates/remember-cli`
- 已删除旧前端/Tauri 资产与旧 qa-gates 目录。

## 固定接口
1. RPC path（不变）：
   - `series.create`
   - `series.list`
   - `commit.append`
   - `timeline.list`
   - `series.archive`
   - `series.scan_silent`
2. 响应外壳（不变）：`{ ok, data, error, meta }`
3. 错误码（不变）：
   - `VALIDATION_ERROR`
   - `NOT_FOUND`
   - `CONFLICT`
   - `UNKNOWN_COMMAND`
   - `INTERNAL_ERROR`
4. IPC 请求包（v1）：`{ id, path, payload, authToken }`
5. IPC 元数据（v1）：`meta.requestId/path/transport/respondedAtUnixMs`

## 剩余工作（面向 B6）
1. 追加契约测试，覆盖 6 条 RPC 成功/失败路径。
2. 完成历史 SQLite 文件回归验证。
3. 增补优雅退出与恢复性测试。
4. 固化发布候选流程与回滚演练记录。

## 执行纪律
- 单任务单功能、完成即记录。
- 仅本机 IPC，不开放远程网络 API。
- 配置兼容字段 `runtime_mode` / `postgres_dsn` 仅 warning，不改变运行路径。
