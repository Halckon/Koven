# Unit loan 实际动作源

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改普通 loan 观察、cleanup 投影与覆盖边界时 · **唯一真源**：`compilation_unit/loan_actions.rs` 与 `dataflow/drop_planner/action_source.rs`


## 当前有界来源

Unit drop planner 的 acquisition 注册与原 `iteration_actions` cleanup 现保存于同一
append-only `ActionSource`。旧 direct/prefix/provider 切片只迭代其中 Cleanup，旧 cleanup
下标保持，Begin 不参与旧计数；不从旧表排序拼出顺序，也不增加第二次 End/Drop 执行。
`loan_action_observations()` 按 source 发布 Begin 和已有实际 cleanup 的观察，错误/deferred
仍在 raw 与公开边界全局撤销。静态 record 的封闭 catalog ID 不可跨 source 产品或重分析解析；
结构快照相等不表示 catalog 相同，记录顺序不是跨分支、循环或 callable 的执行时间线。

当前只发布 Partial 或 Unsupported：有记录仍可能遗漏 result handoff、receiver activation
和权限恢复；空表为 Unsupported，不能证明 absence。没有 Complete/Complete-empty 认证，
没有 callable/path/activation 覆盖证明，未退休 Single 生命周期 oracle。已有 receiver/result
观察仍独立保留，不将其旧表补接为统一完成证据。实施与未运行项见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md)。
