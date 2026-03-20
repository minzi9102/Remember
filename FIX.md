失败原因
这次 P5-T1 失败，不是单一原因，而是“后端真实失败 + 回归脚本误判”叠加造成的。执行结论已经写在 p5-t1-full-regression.md:121。

【已解决】真正阻断发布的是 Rust 自动化基线没全绿。
p3_t2_parallel_tx_timeout.rs:75 明确要求超时应接近 3 秒，且不能超过 4.5 秒；本次实际跑到约 24.25 秒，说明 Postgres 写入超时控制在真实环境里没有按预期生效。这个失败是产品/仓库层面的真实问题，不是脚本噪音。



p3_t5_startup_self_heal 也有真实行为不符合预期。
在 p3_t5_startup_self_heal.rs:158 到 p3_t5_startup_self_heal.rs:170，测试期望 archive_series 的异常类型是 RepositoryError::DualWriteFailed(_)，但实际不是。另一个自愈断言在 p3_t5_startup_self_heal.rs:315 也曾暴露过计数不一致，说明启动自愈和错误分类还不稳定。

桌面交互证据“看起来通过”，但数据库结果并没有证明闭环成功。
比如 P5-T1-IG-PASS_20260318_ENV-DUAL_codex.txt:13 里，Inbox 出现了重复记录，Project-A 仍然是 silent，并没有被证明真的归档。也就是说，动作链写的是“create -> commit -> archive”，但数据库并没有完全支持这个结论。

当前回归脚本的判定条件太宽，导致“无异常”被当成“通过”。
在 run-p5-t1-full-regression.ps1:726 和 run-p5-t1-full-regression.ps1:811，IG-PASS / IG-FAIL 默认都是 PASS，只有抛异常才会变成 BLOCKED。它没有对“唯一创建成功”“归档状态变化”“双库字段一致”这些业务条件做强断言，所以会出现“证据文件存在，但结论过于乐观”的情况。

交互实现本身也比较脆弱。
归档时间线尝试是靠固定窗口相对坐标点击完成的，见 run-p5-t1-full-regression.ps1:705。这种方式对窗口尺寸、焦点、渲染时机很敏感，容易出现“脚本执行了，但点到的不是预期控件”的情况。

用例隔离不够，导致证据串味。
同一模式下四个 case 是连续在一个 app 会话里跑的，见 run-p5-t1-full-regression.ps1:918。这会让 IG-FAIL 里创建的 Archive-Me、rollback-check 污染后续证明。你也能在 P5-T1-IG-PASS_20260318_ENV-PG_codex.txt:13 看到 postgres_only 的证据里混入了 SQLite 侧残留数据，这说明模式隔离和 case 隔离都还不够严。

取证策略和计划目标也有偏差。
脚本现在在 postgres_only / dual_sync 下仍会默认附加 SQLite 证明，见 run-p5-t1-full-regression.ps1:772 和 run-p5-t1-full-regression.ps1:859。这会让“应该验证哪个后端”变得模糊，也不利于快速判断单模式是否真的符合预期。

应该改进的点
优先级上，我建议先修“产品正确性”，再修“测试可信度”。

先修 p3_t2 的真实超时问题。
重点看 postgres.rs:86 和 postgres.rs:436。当前代码确实有 SET LOCAL statement_timeout = '3s'，但真实运行结果说明它没有在目标阻塞路径上生效。要检查的是：
超时 SQL 是否落在了和实际阻塞写入相同的事务里
被锁住的语句是不是发生在设置超时之前
是否还需要同时设置 lock_timeout
是否应在测试里打印 SHOW statement_timeout
修 p3_t5 的错误分类和自愈语义。
dual_sync.rs:1077 到 dual_sync.rs:1086 显示，单边失败时如果被识别成 PG timeout，会返回 PgTimeout，否则才是 DualWriteFailed。这块要和测试期望重新对齐：
到底 archive_series 的这条路径应该归类为 PgTimeout 还是 DualWriteFailed
自愈后 unresolved/repaired/scanned 计数是否应保证幂等
alert resolve 的时机是否和测试假设一致
把回归脚本改成“业务断言驱动”，不要再用“没报错就 PASS”。
当前脚本最大的问题不是能不能截图，而是 oracle 太弱。建议把每个 case 的 PASS 条件写死成数据库断言，例如：
create Inbox 后只允许出现 1 条 Inbox
archive Project-A 后必须是 archived
postgres_only 禁止 SQLite 落库
dual_sync 必须校验双库 commit_id / created_at 一致
任一断言不满足就直接 FAIL
每个 case 独立重置基线，别共用一个 app 会话。
run-p5-t1-full-regression.ps1:925 到 run-p5-t1-full-regression.ps1:928 现在是 VG-PASS -> VG-FAIL -> IG-PASS -> IG-FAIL 串行跑。更稳的方式是：
每个 case 前重建数据库
每个 case 单独启动 app
每个 case 结束后清理进程和数据
这样证据不会互相污染。
把 mode-specific proof 做干净。
建议明确：
sqlite_only 只查 SQLite
postgres_only 只查 Postgres
dual_sync 同时查双库并比对
现在这种“PG 模式也附 SQLite 结果”的方式，容易让人误以为双写是预期行为。
降低桌面操作的脆弱性。
固定坐标点击只是临时方案，不适合长期门禁。更稳的方向是：
优先用可识别的控件语义或可访问性接口
如果只能桌面层操作，至少加截图前后的状态校验
对关键动作增加重试和焦点确认
对每一步输入后读取实际 UI/DB 状态，而不是只记录 action chain
让脚本输出“失败明细”而不只是总结果。
现在脚本能给出 overall=FAIL，但还不够可诊断。建议在 summary 里直接列出：
哪个断言失败
哪个环境失败
哪条 SQL/哪组 series 状态不对
对应证据文件路径
这样后面 rerun 时会省很多时间。