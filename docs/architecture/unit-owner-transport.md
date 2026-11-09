# Unit owner transport 观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对 Unit owner 观察边界 · **唯一真源**：owner transport 生产代码与测试

## 狭窄 owner transport 观察

Unit 产物另外保存逐 callable 的 `owner_observations()`。当前实现限普通函数内的
String Value 参数、非插值 String、Name/Group/local binding 与 checked root replace，
以及下节限定的显式双支 Unit If 与顶层 leaf capture 顺序组合。
定义 token 带 source-qualified callable 与分析内身份；结果使用逐求值帧局部槽，pending
使用 call/argument 配对。旧值在 commit 交给结果，新 RHS 值进入 root；事件在真实转换点
记录，不从 drop、span 或 formed history 推导。`is_consistent()` 核对运输守恒与身份。

观察整体的相等比较保留完整、有序的结构事实：只对应各自观察内的 token，逐项比较
source-qualified callable、定义来源、局部编号、状态及全部事件字段。单个 token、
定义、事件的相等与 `definition()` 查询仍严格核对产品身份；不同分析的 token 不可互换。

不支持的控制流、普通调用、其他 capture、非 String 根 swap 或其他值形态使该 callable 的观察标为
Unsupported 并清空该观察的前缀；原合法性和 cleanup 产品不受这个状态影响。此层不提供
通用 capture lineage、条件 Choice/phi 或新的 Phase 4 执行能力，原 Single witness 保留。
本地纵切的 Rust 编译/行为验收尚未执行，状态见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#72-e2-owner-transport-a-的本地纵切2026-11-03)。

## 显式 Unit If 的无 phi 观察切片

仅准入显式 else、结果类型为 Unit、两边均继续且 cleanup 后全部存活绑定保留相同 token
的 If。condition 在分叉前求值，各边的真实 control body 与 branch-exit cleanup 分别
记录为 `UnitOwnerTransportEvent::If` 的 true/false 有序子序列；不把互斥路径拼成线性流。

validator 从同一父 location 分叉重放：继续状态中的 owner 必须一致；只在某支创建并在
该支真实 Drop 的定义可在 join 后标为不可用。定义只能在一个静态创建点首次登记，嵌套
控制身份不可重复，BranchExit 必须属于当前边；跨分析整体相等比较递归保留两支事实，
独立 token 与 lookup 仍不可互换。此校验证明已发事件的内部守恒，不重推 AST 合法性。

不同存活 owner 的 join、String-valued If、return 分支、隐式 else 和 When 仍整体拒绝。
旧 cleanup 的 symbol 交集不能证明 owner 相同；无 else 的旧路径仍真实执行边级 ASAP
清理，不能当作 location 原样直通。完整 Choice/phi 需要独立的 incoming-edge 覆盖、
非继续路径与 return payload 交付合同；本片没有定义或实现 Choice。

回归源码包括两支消费同一输入 owner、分支局部创建/清理、相同 owner 跨 join 后 replace、
嵌套 If，以及扁平化/缺边/外来 token/重复创建/错误 cleanup edge mutation。
全部保留原七类不支持边界源与 observation enabled/disabled 全 planner 产品比较。
Rust 行为尚未执行，静态检查不代表精确头官方 CI 验收。

## Capture formation

基于 #294 `0bb1f847` 的本地候选只接纳普通函数直线路径中非空、全部 Owned/Move String
的 leaf 环境。形成点在实际 `take_named` 移出处保存当前 token，单独定义环境 token，
以 source-qualified binding 标识 capture 槽。Name/Group/binding 转交同一环境 token；
slot 输入不会随原 binding 的声明位置重建。实际 captured cleanup 发布内容释放，再发布
环境释放。CaptureDrop 的 BranchExit 与普通 Drop 一样必须属于当前 replay 边；
线性事件不能借用同源的伪分支出口。共享和独占资源的旧清理顺序不变。

Shared/Copy/closure-valued capture、空环境、直接丢弃的 temporary 与调用执行仍使整个
callable 观察原子 Unsupported；lambda body 保持独立 Unsupported。此候选尚未编译或
动态验证，不表示旧 cleanup、Single witness、循环实例或 native 已完成迁移。

## 顶层 capture 与 If 顺序组合

同 callable 允许顶层 leaf capture 在显式 Unit If 前后形成和释放。跨 If 存活的环境与
captured slots 由同一父 replay state 分叉，两支 join 后必须保持相同 location 与 token；
不生成 phi 或重建 capture 来源。producer 只在真实 branch body 及其 cleanup 内增加深度，
退出时包括错误返回恢复深度，后续顶层 capture 不继承已结束的分支上下文。

分支内形成 capture 仍使整个 callable 的观察原子 Unsupported；已在顶层形成的环境可在
显式 If 两边分别释放，真实 traversal 与 cleanup 照旧执行。每条实际路径按原顺序释放
capture slot 再释放环境，BranchExit 必须属于当前边，join 的全部 location 必须一致。
validator 独立拒绝任何 If 子树中的 Capture，即使最终守恒；此处不准入 branch-local
或嵌套 capture 形成。共享/Copy、closure-valued、
调用执行与直接丢弃临时环境的原边界保留，lambda body 仍独立 Unsupported。

回归覆盖两种顺序、存活环境跨 join、嵌套 If 后深度恢复、分支形成拒绝、sibling 隔离、
双边和嵌套边释放，以及错边、漏释放、重复释放 mutation。释放仍从顶层形成事件按当前
环境 token 与 source-qualified slot 精确定位，不搜索无路径约束的递归历史。所有源码
保留 observation on/off 完整旧 Analysis 比较。此候选尚未运行 Rust 行为验收，不宣称支持分支内 capture 形成、
循环或新 Phase 4 执行能力。

## 循环拒绝回归切片

循环 owner 仍未准入。新增静态回归固定 While/Loop 在外层 statement 原子 Unsupported：
常量 false 的零轮边界、未改变条件的正常回边、continue、break 与嵌套循环都清空整个
callable 的观察前缀；独立 sibling 仍完整。另用独立准入前缀与非法事实注入检查防止空测。
同组源码比较 observation on/off 的完整旧 Analysis，要求 cleanup 的身份、次序与全部
字段不变。测试只分析源码，不运行循环，也不把有限回放前缀当作真实多轮终止证据。
本片不改生产支持；编译和行为仍待官方 CI，静态检查不替代执行。

## Choice 图验证边界

`owner_transport_tests::choice_model` 保存单选择的手工图测试，用真 definition catalog
身份构造图，分别验证两条替代路径及完整后续；Choice 不成为新 String producer。
原测试内算法已迁入 V1 库 validator，手工图仍只在测试中构造，不代表生产发出 Choice。
模型不证明 AST 类型、实际尾值交付或未记录的调用/退出，生产 String-result If 仍
Unsupported。实际消费者的 cleanup 和 Single witness 保留，准入接线须另立合同；
历史模型验收见 SPEC-0341 §83，本次迁移边界见 §87。

Control body 的私有结果通道保留实际尾表达式与既有求值帧 owner；旧 bool 调用方
仍只消费是否继续。Read 尾值不交付 owner，原 temporary/scope cleanup 不变。
该内部通道不改变 String-result If 的 Unsupported 边界，也不发布 Choice 协议。

## V1 value 投影与受限 Choice

V1 使用独立只读 String producer catalog，Definition 与 Choice 分离。旧 Complete 且全部
定义最终 named Drop 的直线观察可按需投影；有界 Choice 另由真实 planner 事务收集。
仅顶层无修饰、非泛型 Unit 函数，一个无模式 marker 的 Boolean 参数；初始 binding
必须精确 String/MoveOnly，initializer 为显式 If 或整个 If 的 Group 链。
条件仅同一参数的 Group/Not 链，每节点为非 constant-use 的精确 Boolean；原 planner
先求条件，再按真实 then/else 记边，不因 Not 交换输入。调用、常量、比较和短路仍拒绝。
每边允许无插值 String literal/Group（可包单尾 ControlBody），或一个精确 String/
MoveOnly 的局部 `val` 后接唯一 Name/Group 消费尾；局部 initializer 仅 literal/Group。
两边可混搭；函数体仅初始 binding 及下述可选唯一后继，不接受其它语句或使用。

String 构造登记真定义；Group/If 完成与双边实际 tail 交付必须唯一有序。局部 initializer
与 tail 使用独立帧/计数，symbol→value 槽仅在真实 insert 后接 Binding，实际 take_named
返回 Some 才转移到 Name 并清空槽。边出口不得残留 initializer 或 named owner。
整个 If 的 Group 链沿真实 Consume 完成运输同一 Choice，post-insert 仅接完整末帧并
清空结果；保留真实 control 和双边 producer。漏层、Read、错序或重复均撤销事务（§124）。

初始 `val` 可无后继，由实际移除后的精确 AfterStatement Drop 闭合；也可跟唯一不可变、
精确 String/MoveOnly 的 `val forwarded = ((chosen))`。实际 take 后用独立结果帧逐层
Consume/Forward，再 post-insert 清空帧；最后仅清理 forwarded，chosen 不再 named Drop。
失败 take、Read、错序、重复或错误清理均撤销；多次转交仍拒绝（SPEC-0341 §122）。

初始 `var` 仅在唯一后继为 `val previous = replace(&chosen, literal/Group)` 时准入。
必须是真实 checked String root Replace，非投影，RHS 精确 String 且无插值；独立帧
要求完整 Consume 后注册真实唯一 pending，再于 checked primitive 完成和 root 更新后
发布 Replace。原 Choice 成为返回结果，新 definition 安装到 chosen；真实 CallReturn
先释放新根，调用 Consume 完成、post-insert 后才绑定 previous，声明后释放旧 Choice。
漏 pending/commit、错误根、提前绑定/清理或重复均撤销；不复用旧 owner=None（§126）。

Choice 图的 CallReturn Drop 必须承接紧邻同 call/root/incoming 的 Replace；连续 cleanup 保留
该 commit 见证，仍验证 Named→Dropped；Swap 仍仅无 Choice。公开事件 API 不变。
BranchExit、控制退出、When 等未覆盖 point 仍不准入；图守恒不等于 typed AST 准入。
手工图可验证更一般的 Pending/Replace，但生产消费者仍只限上述合同。

旧观察仍原子 Unsupported，不接收新定义/Choice；原 cleanup/Single 见证保持。新能力
绑定 observation 的 product/callable，旧 Eq 仅比旧事实，新协议须独立验收。不从事件、
cleanup 或 owner=None 重建身份；任意退出、遗漏、重复或错误交付均清空新事务。
nullable、普通调用、capture、二次 replace、控制退出与真实循环/phi、通用后续消费和
Phase 4 仍未实现。本机证据见 §120/§122/§124/§126；原 §87/§88/§89/§101 依赖保留，
不表示 D10/E2 完成。

## String 根 swap 观察

checked `swap(&left, &right)` 的两个完整 String 根在真实 primitive commit 处交换
`OwnedValue.owner`，并记录交换前的两个源限定根及 owner token。交换不创建 owner，
不产生 owner result，也不生成替换析构；后续 replace、move 和实际 cleanup 使用交换后的 token。
独立守恒重放要求两个不同 token 分别位于对应根，拒绝遗漏、重复和别名交换。

V1 value 投影现在接受这一闭合 String 根 swap，并复用真实 producer catalog，
不合成结果或新 producer。两根在同一 call-return 的连续 cleanup 共享已验证的
commit 证据，逐根消费交换后的 token；遗漏、重复、别名和跨分析 token 均被拒绝。
Swap 暂不进入 Choice 图，不声明 Resource、字段/索引投影、循环或任意 branch phi
支持，也不退休 Single owner 见证。
证据为 `owner_transport_tests/swap.rs`、`choice_model/swap.rs` 和观察开关全 cleanup Analysis 等价测试。

## Shared String capture 槽前置

直线、named environment 的纯 Shared String capture 现在观测形成时的真实命名来源
owner token；新环境有独立 token，来源不被移走。转移环境 binding 保留同一实例身份。
真实 EndCaptureLoan 边记录 source token，并与形成时冻结的 slot 比对；逆序 End 组必须
连续且紧邻同 point 的环境 Drop。多环境可以借同一来源，最后一个 slot 结束前不得消费来源。

独立 loan overlay 与原位置 replay 共同检查身份、成对退休及 Move/Drop/Replace/Swap/
Owned Capture 旁路。它只认证观测，不更改原 cleanup，不授予 lowering 执行能力。
借用参数缺 owner、临时/mixed/nested capture、分支逃逸及不一致退休、循环仍不准入；
已退休环境之前或之后的独立原支持语句不因本片禁用。V1 value 投影仍拒绝 Shared 事件，
循环 L0164 与 Single 条件 owner/phi 见证保留；条件 slots、循环实例及 native 尚未实现。

既有 Shared slots 可跨显式双臂 Unit If：每臂与嵌套 If 独立继承相同 loan 集合，退出时
两个完整有序集合须相等，且不能留下部分退休。允许保持原集合，或在两臂一致完成已有
环境的退休；每个叶边仍核完整逆序 End、同 point 环境 Drop，分支点必须对应当前边。
根出口仍要求集合为空，位置 replay 继续核对 owner 合流。
不构造不等状态的条件 slot/phi，不改变 cleanup、循环 L0164 或 V1 投影边界。

纯 Shared String 环境也可在显式 Unit If 臂内形成并在本臂完整退休；单臂产生的环境在
另一臂保持 Uncreated，在形成臂成为 Dropped，不能把 Named/Result 环境带到合流。
环境静态定义只允许一次形成，created 状态跨兄弟臂共享验证。Owned/mixed/Exclusive
分支形成、环境逃逸以及循环仍不准入；既有环境和同源新环境的 loan 分别保护来源。

## Unit Return 终止边前置

显式 Unit If 可有一个 bare Return 臂。真实 Return 先按原顺序完成 iteration cleanup 与
全部 owner cleanup，再发布 UnitReturn；事件记录真实 source-qualified expression，不新建
owner。主位置 replay 与 Shared loan overlay 分别验证终止状态：无活跃 owner/loan、无
部分退休、无事件后缀；只把继续臂状态交给后继，不拿终止臂的 Dropped 与继续臂求交。
V1 投影拒绝新终止事件，默认 cleanup 与诊断不依赖观测开关。

此片保守拒绝双臂终止、带返回值、循环及不支持调用；
没有动态 owner phi 或循环实例能力。事件一致性校验不重新判断 AST 节点种类；实际
bare Return 身份由 producer 接线和真实源码的精确 Unit ID / ControlTransfer point 验证。

return 臂局部构造并完整退休的 owner 可保留为静态不可用身份：仅在继续臂为 Uncreated、
终止臂已验证为 Dropped 时归一为 Dropped。继续臂 Named 等存活位置原样保留，两臂都
未形成仍为 Uncreated，不能凭归一满足完整性。这里不发布额外 runtime Drop，不构造
动态 phi；合流后该 token 不能 Result/Bind/Move/Drop，created 仍阻止兄弟臂重复形成。

既有 Owned move 环境的单 bare Return 见证沿用完整捕获输入：两臂把同一环境分别移动到
局部别名，各自按原 AST AfterStatement 点先释放原槽再释放环境，终止臂末尾保留真实
source-qualified UnitReturn，继续臂不带终止事件。双臂终止仍整体 Unsupported/Syntax；
观测开关比较完整 Analysis，既有清理身份与顺序不变，不据此扩展 native 或 V1 投影。
