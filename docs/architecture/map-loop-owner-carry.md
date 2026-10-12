# single Map loop owner 运输

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 single guarded Map IterationPhi 运输时 · **唯一真源**：`lower_frontend/map_loop_owner` 与普通源码测试

`lower_frontend/map_loop_owner` 消费非 Resource 具名 Map 的精确 IterationPhi 图：唯一
Header/Exit、静态 Entry/Fallthrough/Exhaustion 输入和精确 guarded drop。首片 source 限于
Array/List/MutableList，Map 与 source 不同 root；seed 是真实 Map 构造或拥有型参数。
同类型的多个 Map 按独立 owner identity 配对，不按名称或类型绕过 guarded-drop 门。

实际 SSA 记录 preheader、header、body、exhaustion 的同一 carried slot，并验证所有进入边：
entry 追溯至真实 seed，回边及最后 drop 仅经 MapPut/MapRemove 的 owner 结果与 block
参数追溯。耗尽清理仍使用 Header；清理完成后才允许 live Exit 继续使用，dead Exit 不复活。
orchestrate、普通 drop 和 iteration Action::Drop 共用此证明；新路径缺失 binding/carrier
显式失败，verifier 与 LLVM/runtime 未改。

普通源码测试覆盖 0/1/3 轮、loop 后 contains/put/remove 或不再使用、重复 remove 的 missing
分支、String/MoveOnly inline、own 参数与同函数双 Map；single 与 unit 分别验证并运行。
stdout 只证明可观察结果，唯一消费及 drop 时机另由真实 SSA/verifier 核验。
一般 snapshot/closure、额外 Abort-survival guard、break/continue/return、连续或嵌套
IterationPhi seed、返回 Map 的调用 seed 及 RangeView source 保持显式边界；这些是未覆盖
组合，不是语言禁止。Resource 分类及旧 constant-presence 路径不变。
