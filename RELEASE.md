# Remember 发布与回滚手册（P5-T3）

## 1. 发布配置基线
- 项目构建配置来源：
  - `src-tauri/tauri.conf.json`：应用标识、窗口、打包目标与前端构建串联。
  - `package.json`：前端构建与测试脚本（`build`、`test:unit`）。
  - `src-tauri/Cargo.toml`：Rust 依赖与 Tauri 运行库。
- 运行模式基线：固定 `sqlite_only`，不允许切换到其他后端模式。
- 配置与数据路径基线：
  - 配置文件：`config.toml`
  - 数据库文件：`remember.sqlite3`
  - 若设置 `REMEMBER_APPDATA_DIR`，则优先落在该目录；否则使用平台 app data 目录；不可用时回退到当前工作目录。

## 2. 排障手册
- 配置回退排查：
  1. 检查 `config.toml` 是否存在且可读。
  2. 检查 `REMEMBER_APPDATA_DIR` 是否指向可创建目录。
  3. 若日志出现 fallback 提示，确认是否命中默认配置。
- 兼容 warning 排查：
  1. `runtime_mode` 与 `postgres_dsn` 仅兼容读取并告警，不会改变实际运行模式。
  2. 若 UI 出现 Config warning，先核对配置项是否包含遗留字段。
- 热键异常排查：
  1. 若标题含 `[HOTKEY_DISABLED]` 或日志出现 `global hotkey disabled`，表示全局热键注册失败。
  2. 先释放系统占用快捷键，再重启应用复验。
- 数据库路径核查：
  1. 确认实际 `remember.sqlite3` 路径与配置预期一致。
  2. 用本地 SQLite 工具抽查 `series`/`commits` 是否可读且有最新写入。

## 3. 发布清单
- 发布前检查：
  1. `task.jsonl` 中 `P5-T1`、`P5-T2`、`P5-T3` 已完成。
  2. `qa-gates-codex/MASTER-TRACE-MATRIX.md` 已同步 `P5-T31/P5-T32` 门禁定义。
  3. 运行模式、配置路径、数据库路径与文档基线一致。
- 建议执行命令：
  1. `npm run test:unit`
  2. `cargo test --manifest-path src-tauri/Cargo.toml`
  3. `npm run build`
- 产物核验：
  1. 前端产物目录存在且更新时间正确（`dist/`）。
  2. Tauri 打包配置可读取且无关键字段缺失。
  3. 发布说明、排障手册、回滚步骤均可追溯到本文件。
- Go/No-Go 判定：
  - Go：关键检查全部通过，阻断问题为 0。
  - No-Go：存在阻断级问题、测试失败或无法提供可执行回滚路径。

## 4. 回滚策略
- 回滚触发条件：
  1. 发布后出现阻断级故障（无法启动、无法提交、数据损坏风险）。
  2. 热键/配置错误导致主路径不可用且短时间不可修复。
- 备份与恢复：
  1. 发布前备份 `config.toml` 与 `remember.sqlite3`。
  2. 回滚时恢复上一稳定版本可执行文件与上述备份。
  3. 恢复后重启并验证窗口标题、运行模式、核心读写。
- 回滚后验证步骤：
  1. 执行一次 `series.create` 与 `commit.append` 基础链路。
  2. 校验列表与时间线可见、归档只读约束仍生效。
  3. 记录回滚时间、触发原因、恢复结果和后续修复 owner。
