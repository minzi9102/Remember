# Remember Backend 发布与回滚手册

## 1. 发布基线
- 构建入口：Cargo workspace（不再依赖 Node/Tauri）。
- 主服务：`remember-ipc-server`
- 运维工具：`remember-cli`
- 运行模式：`sqlite_only`
- 生产通道：Named Pipe（本机）

## 2. 发布前检查
1. `cargo check --workspace`
2. `cargo test --workspace`
3. `cargo run -p remember-cli -- health`
4. `cargo run -p remember-cli -- rpc call --path series.list --payload '{"query":"","includeArchived":false,"cursor":null,"limit":10}'`
5. 校验配置路径与数据库路径解析（`REMEMBER_APPDATA_DIR` 覆盖与默认路径）。

## 3. Go / No-Go
- Go：构建、测试、CLI 健康与关键 RPC 调用全部通过。
- No-Go：任一关键项失败，或无法提供有效回滚路径。

## 4. 回滚策略
1. 回滚代码到 tag：`tauri-last-stable`（旧主线）或上一个稳定后端 tag。
2. 恢复部署前备份：`config.toml` 与 `remember.sqlite3`。
3. 回滚后执行最小验证：
   - `series.create`
   - `commit.append`
   - `timeline.list`
4. 记录回滚原因、影响范围、恢复时间与修复 owner。

## 5. 关键环境变量
- `REMEMBER_APPDATA_DIR`
- `REMEMBER_IPC_AUTH_TOKEN`
- `REMEMBER_IPC_PIPE`
- `REMEMBER_ENABLE_LOOPBACK`
- `REMEMBER_LOOPBACK_ADDR`
