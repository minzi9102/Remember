## Remember 项目功能说明（基于 `product.md`）

### Summary
1. 构建一个桌面端“极速记录”应用，核心是不可变 Commit 时间线、键盘优先交互、单 Series 单时间线模型。  
2. 交付两级视图（Series 列表 + Timeline）、沉寂与归档机制，以及 SQLite-only 持久化架构。  
3. 成功标准采用你选择的“仅功能验收”口径。  

### 描述要实现的功能和目标
1. 用户可通过全局热键一键呼出/隐藏主界面，快速进入记录状态。  
2. 在 Series 列表中高亮任一 Series 后，直接输入并按 `Enter` 即创建不可编辑的 Commit（文本 + 秒级时间戳）。  
3. 系列按最后更新时间倒序展示，最新发生记录的 Series 自动置顶。  
4. 系统自动识别“7 天无更新”的沉寂 Series，并支持一键归档。  
5. 用户可进入第二级时间线只读回顾，不提供修改历史入口。  
6. 系统通过 `MemoRepository` 抽象数据访问，并固定使用 SQLite 持久化。  

### 用户故事
1. 作为记录者，我希望按热键后直接输入并回车保存，避免打断思路。  
2. 作为键盘用户，我希望用 `↑/↓`、`/`、`Shift+N`、`Enter`、`Esc`、`←/→` 完成高频操作。  
3. 作为回顾者，我希望在时间线中按时间倒序查看历史，不被编辑能力干扰。  
4. 作为整理者，我希望系统自动标记沉寂 Series，并能按 `a` 快速归档。  
5. 作为开发者，我希望在不改业务代码的前提下切换存储后端，并在学习期开启双写保护。  

### 功能需求（含接口/类型约束）
1. 交互层必须支持：全局热键呼出、列表上下高亮、`Shift+N` 新建、`/` 搜索、`Esc` 退出搜索/返回上级。  
2. 提交流程必须支持：在一级视图高亮 Series 时任意字符触发输入框聚焦，`Enter` 提交后立即刷新摘录并重新排序。  
3. 一级视图只显示“Series 名称 + 最新 Commit 摘录”，并按 `last_updated_at` 严格倒序。  
4. 超过 7 天无新 Commit 的 Series 必须标记为 `silent` 并下沉；高亮时按 `a` 必须归档并移出主列表。  
5. 二级 Timeline 必须按时间倒序展示，每条仅含 `content` 与 `created_at`，且全程只读。  
6. 数据层必须定义 `MemoRepository` 统一接口，至少覆盖：创建 Series、查询列表、搜索 Series、写入 Commit、读取 Timeline、归档 Series。  
7. 必须提供 `SQLiteRepository` 实现，并通过统一仓储接口承载全部读写。  

### 关键实体（数据模型）
1. `Series(id, name, status[active|silent|archived], last_updated_at, created_at, archived_at?)`。  
2. `Commit(id, series_id, content, created_at[秒级])`。  
3. `AppConfig(hotkey, silent_days_threshold=7)`；旧 `runtime_mode` / `postgres_dsn` 仅作兼容 warning。  

### Test Plan（验收场景）
1. 热键呼出后输入文本并 `Enter`，应创建新 Commit 且无编辑入口。  
2. 连续提交多个 Series 后，列表顺序始终与最近提交时间一致。  
3. `→`/双击进入 Timeline，`←`/`Esc` 返回列表，导航一致。  
4. `/` 搜索仅过滤 Series 名称，`Esc` 后恢复完整列表。  
5. 超过 7 天未更新的 Series 自动变为沉寂并下沉显示。  
6. 对沉寂 Series 按 `a` 后，Series 从主列表移除且保留归档数据。  
7. SQLite-only 模式下读写稳定；旧配置字段存在时系统仍能正常启动。  

### 成功标准（仅功能验收）
1. 关键路径“唤醒→输入→提交→置顶刷新”可稳定执行。  
2. 已提交 Commit 在系统内不可修改、不可覆盖。  
3. 两级视图、搜索、沉寂、归档行为全部符合定义。  
4. `sqlite_only` 模式稳定运行，旧多后端配置不会阻断启动。  
5. Test Plan 场景通过率达到 100%。  

### 假设与默认
1. 应用为桌面端（Windows/macOS），快捷键按平台映射。  
2. 沉寂判定依据 `last_updated_at` 与当前时间差（默认阈值 7 天）。  
3. 搜索范围默认仅 Series 名称，不检索 Commit 正文。  
4. 归档为逻辑迁移：从主列表移除但数据保留。  
