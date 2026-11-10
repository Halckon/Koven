# Unit 不变循环输入的后缀消费

> **性质**：实现事实 · **状态**：current · **读取时机**：核对正常、裸 Continue 或裸 Break While 后真实输入消费与旧观察隔离 · **唯一真源**：loops/invariant/suffix 收集器与 post_loop_transport_v1 封印

本页补充[不变输入循环边界](unit-invariant-loop-owner.md)，不修改 Guide ASAP、循环存活或
[闭合 body](unit-loop-owner-transport.md)合同。只准入原三语句 String 输入的直线正常 While、末尾唯一裸 Continue 或裸 Break；
末尾 If、其它载体、外层 owner 的循环内使用与不精确 suffix 均不取得本能力。

## 实际 suffix 与旧观察隔离

`UnitOwnerInvariantLoopObservationV1` 与 `UnitOwnerInvariantBreakObservationV1` 各自提供
独立 optional `post_loop_transport_v1()`。
原 `is_consistent()` 与边界 getter 不依赖它成功，原 Continue 能力也不会被转换成正常完成。
While 之前保存真实 Complete prefix collector，保留其原 Result/Binding、producer 及 Arc；
不是按 AST 或 Drop 重造 owner。原 While 继续拒绝普通观察，definitions/events 为空。

仅在精确 suffix statement 到达且真实对应循环产物完整时，核 scope、无 loop/branch/Choice、
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
消费检查之外，还要求各自精确控制门、无 Choice/额外 suffix events、单真实 producer、
精确输入 catalog/analysis/callable 身份、同一个 owner、真实 suffix expression/statement、
不同的退休 symbol，以及恰好 AfterStatement(suffix) 的 Drop。body-local catalog 仍另有 Arc，
不得混入 outer graph；真实 suffix entry 独立读取，不复制旧 exit。正常四份为 entry、
backedge、exit、suffix-entry；Continue/Break 五份为 entry、post-jump、body-completion、exit、suffix-entry。

返回的图只含 outer prefix/suffix，须与本对象原 entry、实际 body 完成边、exit 及闭合模板联合使用。
它证明上述准入形式的退出路径上输入转移与最终消费，不能把省略循环执行的线性图当作整个程序轨迹。
条件由 body 不改变的 Boolean 参数经 Group/Not 构成；不能将静态 checker 的边界见证描述成实际运行若干轮
后退出。正常 body 的 suffix 是条件 false 路径的规划；裸 Break 在条件 false 时不激活 body，
条件 true 时沿裸 Break 在一次 body 激活后退出。仍只提供静态检查证据，不是 runtime 执行测量。
这不证明循环终止、每轮动态实例、changing owner、header/exit phi、一般控制合流或 Phase 4/native。
普通源码、双入口、精确门禁与未执行守卫范围见 SPEC-0341 §166、§168。

## 裸 Break 的独立控制门

Break optional 先经过原 `is_consistent()`，核真实 Break expression、post-jump witness、
body Arc、body-completion 与准确 BreakExit 到该 While 的 LoopExit；backedge optional 只接受实际 Fallthrough 或带独立
post-jump 见证的 Continue。内部共用消费校验不单独授予控制能力，所有 conditional
变体均不进入 suffix 收集。激活时还要求 transaction expected 与实际 closed completion 完全相等。

suffix 再读的完整 singleton 与旧 Break exit 相等，随后原 Name 消费、Binding 和唯一 ASAP
Drop闭合该输入。LoopExit 仍清理原 condition state，没有合入 Break state；安全性只覆盖
四份旧状态与第五份 suffix-entry 全部同一个完整 OwnedValue，不是不同输入的出口 phi。
缺失实际跳转见证、外来 Arc/binding、错目标或错误 suffix 图均不能取得新能力。旧 normal、
Break 基础边界能力和所有 cleanup 规则不变，未把新 graph 当成有限循环运行实例。


## 裸 Continue 的条件 false 出口

Continue 后缀仍由 `UnitOwnerInvariantLoopObservationV1` 的 optional getter 暴露，原
ContinueBackedge 指向完整原条件，不能替换为普通 Backedge 或 BreakExit。ConditionFalseExit
仍从原条件状态取得，entry、post-jump、body-completion、exit 与 suffix-entry 必须保有同一
完整 OwnedValue。未更改循环清理、状态合并或重新执行 body；原五事件消费图只描述 outer owner。

后缀产物封存实际收集时的 completion（含原跳转ID）和新鲜 body Arc，getter 再精确比对
当前闭合边界。它们不能从后来替换的 evidence 回填；即使把旧正常证据自洽包装成 Continue，
或在同源二次分析之间同时换 body graph/Arc/transfer，也不能挪用原后缀图。
原输入、catalog、callable、完整状态和消费校验继续保留。

不变 Boolean 参数经 Group/Not 构成条件；条件 true 的裸 Continue 循环不因此证明会退出。
本能力只给 condition-false 出口之后的静态消费证据，不是每轮执行、终止性或 header/exit phi。
末尾 If 的所有组合仍不取得后缀能力，与本片无关的未完成工作不纳入。验收见 SPEC-0341 §174。
