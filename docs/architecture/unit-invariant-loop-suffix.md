# Unit 不变循环输入的后缀消费

> **性质**：实现事实 · **状态**：current · **读取时机**：核对正常 While 后真实输入消费与旧观察隔离 · **唯一真源**：loops/invariant/suffix 收集器与 post_loop_transport_v1 封印

本页补充[不变输入循环边界](unit-invariant-loop-owner.md)，不修改 Guide ASAP、循环存活或
[闭合 body](unit-loop-owner-transport.md)合同。只准入原三语句 String 输入的直线正常 While；
Continue、Break、末尾 If、其它载体、外层 owner 的循环内使用与不精确 suffix 均不取得本能力。

## 实际 suffix 与旧观察隔离

`UnitOwnerInvariantLoopObservationV1::post_loop_transport_v1()` 是独立 optional 能力。
原 `is_consistent()` 与边界 getter 不依赖它成功，原 Continue 能力也不会被转换成正常完成。
While 之前保存真实 Complete prefix collector，保留其原 Result/Binding、producer 及 Arc；
不是按 AST 或 Drop 重造 owner。原 While 继续拒绝普通观察，definitions/events 为空。

仅在精确 suffix statement 到达且真实正常循环产物完整时，核 scope、无 loop/branch/Choice、
原 While/Syntax Unsupported 与 callable/analysis 身份，并再次从全部17槽读取实际 singleton，
比较完整 OwnedValue 后核其 binding 等于 exit。prefix 只能被消费一次，临时换回其 Complete
collector。suffix 仍由原 statement 路径执行一次，包含 borrow-end wrapper；真实 Name Move、
Result、Binding 与原 AfterStatement ASAP Drop 都在该期间记录。不能只旁路 owner_event，
因为 Unsupported 路径本来会清空表达式返回 token，失去后续 Binding/Drop 身份。

保存实际 Result，先无条件恢复原 Unsupported collector，再判断结果或传播错误。
只有实际 Ok(true)、scope/loop/branch/Choice 未变、全部17槽为空时，才封存 suffix。
原 statement/borrow/ASAP cleanup 次序不改，不新增 producer、不延迟析构、不重遍历源码。
最终 unsuccessful 或 deferred 仍由原发布门清除整个 invariant 槽及其 optional 证据。

## 精确运输与范围

原 prefix 的两项与 suffix 的四项 raw 事件经既有 V1 投影去除重复 Result，形成唯一五项
Create(literal)、Bind(held)、Move(held)、Bind(retired)、Drop(retired)。新 seal 在通用 V1 完整
消费检查之外，还要求正常 Fallthrough、无 Choice/额外 suffix events、单真实 producer、
精确输入 catalog/analysis/callable 身份、同一个 owner、真实 suffix expression/statement、
不同的退休 symbol，以及恰好 AfterStatement(suffix) 的 Drop。body-local catalog 仍另有 Arc，
不得混入 outer graph；真实 suffix entry 是独立读取的第四份边界，不复制旧 exit。

返回的图只含 outer prefix/suffix，须与本对象原 entry/backedge/exit 及闭合 body 模板联合使用。
它证明正常退出路径上的输入转移与最终消费，不能把省略循环执行的线性图当作整个程序轨迹。
条件是 body 不改变的 Boolean 参数；不能将静态 checker 的边界见证描述成实际运行若干轮
后退出。suffix 是条件 false 路径的规划，不证明有限 runtime 轨迹。
这不证明循环终止、每轮动态实例、changing owner、header/exit phi、一般控制合流或 Phase 4/native。
普通源码、双入口、精确门禁与未执行守卫范围见 SPEC-0341 §166。
