# Unit 不变输入 owner 的循环边界

> **性质**：实现事实 · **状态**：current · **读取时机**：核对非空循环输入、条件臂、正常合流及完成边 · **唯一真源**：owner transport loops/invariant 收集器与公开封印类型

[闭合 While body](unit-loop-owner-transport.md)说明共用 body-local 合同与真实控制边。
本页只描述单个不变输入的独立能力，不改变旧观察、Guide ASAP 或 loop liveness。

## 单个不变输入 owner 的非空边界

独立 `invariant_loop_observation_v1()` 不放宽旧空入边 getter；两者互斥。仅一个无 mode marker
Boolean 参数，函数恰三句：精确 String 的 `val held = "outer"`、直线闭合 While、精确 String
的 `val retired = held`。前缀只收无插值 literal，后缀只收同 symbol 的 Name；两者不接受
Group/var/nullable。body 复用[闭合 body](unit-loop-owner-transport.md)的 String 本地构造、move、Group、root Replace，可在末尾
追加唯一裸 Continue；此回边能力仍拒绝 Break 或任何 If，Break 另见下节的独立出口能力。
locals 从空集合开始，不能引用、替换或移动 held；后缀未来使用使其按既有
liveness 跨过 body 和 LoopExit，未修改 Guide 的 ASAP 或 loop 固定点。

普通 While rejection 清空前，核真实 prefix collector Complete、唯一 String producer 与
恰好 Result/Binding 两事件，token、initializer、symbol 都对应实际单个 OwnedValue。
只从该 collector 保留 producer catalog，不从 AST、drop 或 symbol 反造 owner。随后照常
拒绝普通观察，旧定义和事件为空。基础边界本身不提供后缀事件或最终消费证明；正常直线
While 可另外取得[后缀消费能力](unit-invariant-loop-suffix.md)。输入 catalog 自身不能代替闭合运输。

entry 在实际 condition 正常完成后读取；正常 backedge 在实际 body scope cleanup 后读取，
且必须返回 Ok(true) 并通过原闭合 V1；exit 在实际 drop_loop_exit 后读取原 condition state。
Continue 则在原 pending loan、deeper-scope drop 和 iteration cleanup 完成后，先过精确
expression/kind、单一最近 loop、scope、fresh sink Arc 与一次性完成门，再独立读取一份
post-jump binding。它成功后才登记 completed；body 返回 Ok(false) 后仍独立读取 backedge，
并核 expected/completed 和 collected Arc 相同、原 body V1 封闭。该提前返回不执行正常
body drop_scope，不能称为正常 scope fallthrough。原空入边 jump 仍要求完整空边界。

`continue_boundary()` 返回真实 Continue expression 与其清理后 binding；普通 fallthrough
必须为 None，Continue 必须有同 source/jump/body Arc 的快照且与其它边界 binding 相等。
Continue 只有 ContinueBackedge(ControlTransfer(jump), condition)，不额外生成普通 Backedge，
也不将已有 ASAP Drop 搬到 ControlTransfer。所有快照分别读取实际状态，不复制 entry 冒充
其它边。ValueState 共17字段，除唯一 OwnedValue 的 values 外16槽为空；owner/token、
symbol、origin、declaration、scope depth 全部相等。
body 使用不同 Arc 的新 catalog，只拥有 body-local producers；外层 owner 不进入该模板。
公开 binding 可查询真实 symbol/token，三条结构边仍核实际 While/condition/body 身份。
最终成功且无 deferred 才保留能力，错误分析整体清除。它是非空入边的身份前置，不是 phi、
不同路径 owner 的 join、运行实例追踪、外层运输闭合或终止证明。

## 单个不变输入 owner 的真实 Break 出口

`invariant_break_observation_v1()` 返回独立 `UnitOwnerInvariantBreakObservationV1`，
不提供 `backedge()`。准入复用同一个三语句 String 输入合同，body 只能以唯一裸 Break
结束；不接收 If、Group/非末尾/嵌套跳转、其它 carrier 或对外层输入的读取/move/replace。
旧回边类型、方法签名及 normal/Continue 的范围不变，其 getter 不接收 Break。

实际 Break 的 pending loan、deeper-scope owner 和 iteration cleanup 后，经过同一精确
expression/kind、最近 loop、scope、fresh sink Arc 与一次性门，独立读取 post-jump
binding。body 实际返回 Ok(false) 后再次读取 `body_completion()`，同时核真实 completed
与原闭合 V1；不执行或伪称正常 body scope cleanup。`break_boundary()` 必须包含真实
Break expression 及其独立 binding。实际 condition 完成后的 entry 与实际 LoopExit 清理后
的 exit 仍各自读取，四处均完整核17字段，values 外16槽为空，完整 OwnedValue 身份一致。

原 planner 仍丢弃 body state，并对原 condition state 执行 LoopExit cleanup。本片不改
这一路径；仅在 post-Break、post-body、entry、exit 的完整单例状态相等时发布能力，不能把
它扩称为一般出口合流或 exit phi。输入 producer catalog 与闭合 body-local transport 分离，
保留既有 ASAP Drop，不制造 ControlTransfer Drop；边为 EnterBody、真实
BreakExit(ControlTransfer(jump), LoopExit(While))、ConditionFalseExit，没有任何回边。

私有非空槽用 Backedge/Break 枚举互斥存储，两类公开 getter 分别核精确变体、原 While
Unsupported、callable、analysis Arc 与自身 control seal。共同私有边界复用身份、闭合 body
及结构边一致性检查，不单独授予控制能力；回边 seal 只接 Fallthrough/Continue，出口 seal 只接实际 Break。同一最终
successful/无 deferred 门清空或保留该槽。不能将完整自洽的另一种控制证据换入对应类型。
这仍是静态检查器的真实边界证据，不发布 suffix 事件、最终消费、有限运行轨迹、动态实例
或完整外层守恒；条件 false 的零轮路径也不等于实际执行过 Break。

## 不变输入 owner 的双 Continue 条件边

`invariant_split_continue_observation_v1()` 返回独立
`UnitOwnerInvariantSplitContinueObservationV1`。仍为精确三语句 String 输入/后缀合同，
但 While 与末尾 If 分别读取两个不同的普通 Boolean 参数，条件可带 Group；两臂必须各有
唯一裸 Continue。拒绝 Break 混合臂、空臂、arm-local owner、嵌套控制及外层输入操作。
旧空输入、normal/裸 Continue 和 Break getter 都不接收此能力，未扩大原接口合同。

While entry、真实 If condition 完成后的 split、两臂各自的 entry/post-Continue/completion、
原 condition state 经 LoopExit 后的 exit，共九份 binding 分别读取真实状态。每次均核
完整17字段及同一完整 OwnedValue，其它16槽为空。arm entry 在实际 branch-entry cleanup
后记录，jump 在原三步清理后记录，completion 在实际 evaluator 返回 false 后再次读取。
每臂的真实 control、序号、ControlBody、Continue expression 和 fresh body Arc 分别保留，
同一槽只能写一次；tail、pending jump、scope 与最近 loop 均按原 hook 核验。

原 If 只合入正常继续的 arm。双 Continue 的两个 state 均被丢弃，最终 merge 输入为空；
两臂完成且事件为空、正常 continuation 数为零后才登记 completed。整体 body 还须实际
Ok(false)、对应 completed/body Arc 与完整空状态，但不产生 body owner 快照。公开产物
只给按 then/else 排序的两个 `UnitOwnerContinueArmBoundaryV1`，没有共同 `backedge()`
或 `body_completion()`，不能从 entry 或某一臂回填合并状态。

闭合 body-local transport 与输入 catalog 仍隔离，六边为 EnterBody、IfBranch(0)、
ContinueBackedge(0)、IfBranch(1)、ContinueBackedge(1)、ConditionFalseExit。两跳转不同且
source-qualified，不新增普通 body 回边或移动原 ASAP Drop。私有单槽枚举增加独立变体，
其 seal 限定双 Continue，核条件/臂次序/端点、catalog/analysis/body 身份与九份 binding。
最终 successful/无 deferred 门保持；中途失败无法形成完整产物。此为两条互斥控制边的
静态检查证据，不是正常合流、header/exit phi、suffix 消费、每轮换臂或动态实例运输。

## 不变输入 owner 的条件 Break 出口

`invariant_split_exit_observation_v1()` 返回独立 `UnitOwnerInvariantSplitExitObservationV1`，
仅接 Continue/Break、Break/Continue、Break/Break。三语句与独立 Boolean 条件合同不变，
两臂仍各一个裸跳转；空臂、臂内 owner、其它控制及外层输入操作不准入。双 Continue
继续只走上一节能力，原 `UnitOwnerContinueArmBoundaryV1` 与 `after_continue()` 不变。

私有 terminal split 收集器在实际 jump cleanup 后保存真实 kind、expression 与 binding，
不在 finish 时根据 AST expected 补造 kind。中性 `UnitOwnerJumpArmBoundaryV1` 提供
entry、`after_jump()`、completion 和 `edge()`；Continue 精确回到 While condition，Break
精确到该 While 的 LoopExit。仅实际两条 Continue 边可转换旧 Continue-only 类型；新 seal
明确要求全为 Continue/Break 且至少一 Break，再核实际 edge、closed completion 与完整六边。
两种 getter 由私有枚举互斥，source、arm 次序、端点、catalog/analysis/body Arc 门均保持。

九份真实 binding、完整 singleton/OwnedValue 与其它16空槽要求不变。三种组合都没有
正常 continuation，整体 body 只核真实 false、完整空 merge 与完成封印，不生成共同 body
binding。LoopExit 仍清理原 condition state，未把 Break state 合流进去；这仅证明窄范围的
完整不变状态，不是一般出口运输或 exit phi。原 merge、branch/jump cleanup、ASAP 与
body-local Drop 顺序不变；本片不提供 suffix 消费、运行实例或有限 runtime trace。

## 不变输入 owner 的双正常臂与真实合流

`invariant_normal_join_observation_v1()` 返回独立 `UnitOwnerInvariantNormalJoinObservationV1`，
仅接末尾 If 两个正常空 ControlBody。三语句 String 输入/后缀、两个独立 Boolean 参数与
闭合 body-local 前缀合同保持。正常/跳转混合臂、臂内 owner、嵌套、其它尾值和外层输入操作
仍拒绝。旧空入边、直线/裸 Continue、裸 Break、双 Continue 与条件 Break getter 均不扩展。

两臂分别经过真实 entry cleanup、空 ControlBody evaluator 和 branch-exit cleanup；空臂
本身不执行 drop_scope。各 arm 的 entry/completion 独立读取完整 singleton state，保存
实际 control、序号、body 与 active body Arc。原完成门核两臂实际 true、None tail、无
owner result/pending jump、两个正常 continuations 和空事件；不是根据预扫的空臂补造完成。

原 merge_resource_states 完成后再调用只读 hook，在相同 scope、最近 loop、active body
Arc 和精确 completed 条件下独立读取 `join_boundary()`。不能只信原 merge 的按 symbol
相交或 same_owners 比较；两份 arm completion 和 join 各自核完整 OwnedValue 与17字段。
whole-body 正常 scope cleanup 后另读 `backedge()`，并要求实际 Ok(true)、相同 completed/
collected Arc 与原闭合 body V1。LoopExit 仍读取清理后的原 condition state。

共九份实际 binding：While entry、If split、两臂各 entry/completion、join、backedge、exit。
所有完整 OwnedValue 相同，values 外16槽为空；未复制 entry 冒充后继状态。公开
`UnitOwnerNormalArmBoundaryV1` 仅含真实正常臂的 entry/completion 和 IfFallthrough edge，
不提供 jump。新 seal 限定两臂皆 Fallthrough，逐项核 source/control/序号/body、input/body
catalog、split/join/arm Arc 与九份 binding；完整七边包含两条 IfFallthrough 和唯一 body
Backedge，不能把 arm 完成冒充整个 body 清理。新私有变体与旧能力互斥，最终成功/无 deferred
门保持，终端臂的空 merge 要求不放宽。merge、branch/jump cleanup 和 ASAP 顺序未改。

此为同一不变 owner 的真实正常合流证据，不接收不同 owner 或可变 loop-carried 输入，
不证明一般 header/exit phi、suffix 最终消费、动态运行实例、外层运输闭合或 Phase 4/native。
普通源码与公开入口、开关对照和未执行守卫范围见 SPEC-0341 §162。


## 不变输入 owner 的正常臂与跳转臂

`invariant_mixed_join_observation_v1()` 返回独立 `UnitOwnerInvariantMixedJoinObservationV1`，
只接一个正常空臂与一个裸 Continue/Break 的四种排列。三语句输入、独立 Boolean 条件及
body-local 闭合合同不变；NN 和双终端仍只取得各自旧能力，不放宽旧公开类型。

私有 joining 收集器由双正常臂实现扩展，保存 evaluator 实际返回的 continues 与 completion
binding。只有实际 true 且无 jump 才形成 `UnitOwnerMixedArmBoundaryV1::Normal`；实际
false 且已有清理后的真实 kind/ID/binding 才形成 Jump，缺失或互相矛盾时拒绝。正常臂
经过 branch-exit cleanup；跳转臂经原 jump cleanup 后返回 false，不执行正常 branch-exit
清理，state 不进入 merge。两者在结束时分别保留真实 entry/completion；Jump 另有独立
`after_jump()`。不能根据 expected AST 或没有 jump 就推定正常完成。

实际完成门对 mixed 要求恰一个 continuation，NN 仍为两个，双终端仍为零。原 If merge
只收到正常臂的 state，原 post-merge hook 才读取 join；这里不是将跳转臂与正常臂合流。
whole-body 实际正常 scope cleanup 后再读 backedge，仍须 Ok(true)、同 completed/body
Arc 与原闭合 V1。共十份真实 singleton binding：entry/split、正常臂两份、跳转臂三份、
join/backedge/exit；完整 OwnedValue 相同且 values 外16槽为空，没有伪造共同跳转快照。

新 seal 要求恰一个 Fallthrough，逐项核 Normal/Jump 实际变体与 expected completion、
序号/control/body/source、catalog及各 Arc、十份 binding、真实 jump ID/edge 与完整七边。
Continue 组合同时有一个 ContinueBackedge 和一个普通 body Backedge；Break 组合同时有
一个 BreakExit 和一个普通 Backedge，不能把整个 body 宣称为终端出口。两正常实际变体
只转换原 NN 产物并过旧 seal；全终端继续独立收集并要求完整空 body merge。

branch.rs 的 push/merge/清理顺序、旧 public NN/JumpArm 类型与普通 Choice 均不改。
LoopExit 仍清理原 condition state，没有把 Break state 回灌；这不是一般出口运输、phi、
变更 owner、suffix 最终消费、动态实例或 Phase 4/native。证据与未运行范围见 SPEC-0341 §164。
