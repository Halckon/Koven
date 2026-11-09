# Receiver 两阶段借用的阶段事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 receiver reservation/activation 或消费其事实时 · **唯一真源**：frontend/codegen 实现与定向测试

## Instance receiver 的两阶段调用借用

[前端定向验收](../archive/specs/0243-receiver-two-phase-borrows.md)为 single `LoanFact` 与
unit `UnitReceiverOwnershipFact` 发布 `is_receiver_reservation()` 及 `activation_point()`。
只有具名 Inout instance receiver 先预留；read/shared loan 可在实参求值期间存在，
move、mutation 与新的 exclusive 借用继续冲突。普通 `&` 实参不走预留。

activation 指向该 call 的 CallEntry，只有存在正常 continuation 才发布；Borrow 参数
继续活到目标 callee 返回，激活遇到重叠仍为 L0135。单文件 `LoanTarget::This` 与 unit
`UnitLoanTarget::This` 保留 callable receiver 的 nominal/declaration identity，隐式与
显式 this 不再因无普通 root symbol 而遗漏冲突。单文件隐式 receiver 的 loan 同样进入
pending-call 清理，return/break/continue 结束预留，abort 不生成 unwind 事实。

reservation 与既有 place overlap、borrow temporary 和 owner liveness 共用状态；不能
通过结束所有参数 loan 或提前 drop 来消除激活冲突。错误时执行 facts 原子清空。
unit 分支合流保留任一可达入边仍有效的 loan，不能丢掉条件闭包的 shared capture。
直接作为外层 Borrow 实参的 capture 继续阻止激活；嵌套同步调用已返回时，只释放该次
结果中的匿名 If/When/Lambda temporary capture，不提前释放具名闭包 owner 的借用。
字段/index 的所有权正反例不等于对应 native 投影路径已实现；single Inout receiver
native 仍是既有明确边界，普通 Borrow 的有限接入见下文。

## Receiver reservation 的直接消费

SPEC-0243 的 unit lowering 在 receiver 位置只求值一次并保存 RootPlace；所有实参正常
完成且 source-qualified activation point 与当前 call 相符后，才发出既有 Exclusive
`BorrowBegin`。不新增 SSA/LLVM reservation 指令，不重求值 receiver，不提前结束目标
callee 的 Borrow 参数。已有 Inout/this entry loan 继续复用；普通 Borrow this 参数使用
shared reborrow，并在嵌套 callee 返回后结束。

pending receiver place 与必要的 Copyable writeback binding 共同跨参数中的 checked
arithmetic success edge 运输；地址和旧值身份继续严格校验。return/break/continue 放弃
预留时没有尚未建立的 BorrowEnd；已完成参数的 loan/temporary 仍按 pending-call facts
清理，abort 不展开。class/value class 的源码顺序、单次求值及控制边由
`receiver_two_phase` 定向 SSA/native 回归验证。single Inout receiver native、unit
非根字段/index receiver native 的既有不支持范围没有因此解除。

## Single 普通 Borrow value-class 方法

[SPEC-0316](../archive/specs/0316-single-readonly-value-methods.md) 接通非泛型 value class 的
默认/显式 Borrow 方法，owned 返回 Int/Boolean。方法模板来自已验证 callable SymbolId；
只有既有顶层 roots 的真实已选 call 可达时才建实例，同名不同 owner 与顶层函数不串路。
隐藏 receiver 是首个 shared entry loan，独立于显式参数；裸字段与 `this.field` 消费真实
projection.field，只对直接 Int/Boolean 字段创建 child loan、Read 后结束，不复制整个 receiver。

named owner、普通 Borrow 参数、Copyable/MoveOnly temporary 和同一 receiver 的隐式调用
已由普通源码验证。receiver 先于显式参数且只求值一次，named 参数仍按源码求值、按声明槽
交付。新建 caller loan 经 pending 槽读取重绑定身份并在返回后结束，借入 entry loan 不结束；
MoveOnly temporary 的同一 owner 在 receiver loan 结束后唯一 drop。基础/常量同源、两入口
及真实 CLI build/run 有本地证据；不把本机成功替代双宿主 CI。

single 的泛型、Value/Inout、普通 class、borrow-result receiver、field/nested receiver 和
String 字段读取/clone 不由本片放行。non-null receiver（含多层 Group）按 typed fact 拒绝；
复用 receiver loan 必须实际定义于函数 entry，for 元素等派生 view 仍明确拒绝。Class deinit、
unit 既有能力不回退。0299 组合只证明
删除结果借给普通 Borrow 参数后，该参数内部调用本片方法，不证明任意 nullable receiver
或参数退出 CFG。现有一般 CFG/条件借用边界保持。


## 字段投影与显式成员调用

`value class` 的显式 `componentN()` 按普通成员调用处理。未声明的调用走一般未解析成员
路径，不发布调用或字段投影。局部结构解构与 `for` 借用解构各有独立产物。直接字段访问
按真实字段身份进行所有权检查；尝试从 owned 聚合移出不可复制字段时报告 L0132 并标注字段声明。


## Shared receiver 的控制转移前缀结束观察

Unit `receiver_call_prefix_ends()` 只投影实际 return/break/continue unwind 的 `EndReceiverLoan`，保留真实
receiver identity 与同点此前普通 Drop、call-loan end、receiver-loan end 的数量。计数
包含被切片排除的真实动作，按不同控制转移点重置，不提供全局执行索引或完整结束流。

仅 owned-result 方法、Shared receiver 与 owned 或声明端 Borrow/Inout 参数的 Name/Group 根，
以及具体 nominal 的 this 准入；
参数的 nested reborrow 结束不结束原参数能力，也不析构 caller owner。正常返回、
Exclusive receiver reservation、StaticSelf this、投影、temporary、provider、显式借用绑定与 borrowed-result
方法不扩张。错误、deferred 或 typed 无效时 raw 与公开产物一并撤销。原 cleanup/native
行为不变；来源、边界与本地验证见[receiver 前缀观测](../specs/active/0341-shared-lowering-core.md#shared-receiver-return-前缀的实际结束观测2026-11-08)。

循环内只观察原 depth-aware unwind 实际移出的 receiver token，内层 jump 不结束外层
pending receiver；continue 保留仍 live 的循环外 owner，for 的 provider 清理顺序不变。
计数描述每个静态控制点，不表示动态迭代次数；同根多 token 仍按实际逆序逐条保留。
新增证据见 [CFG 前缀切片](../specs/active/0341-shared-lowering-core.md#121-shared-receiver-循环控制转移前缀2026-11-09)。
Borrow 参数的 Return/Break/Continue 现观察同一实际退休 token；先完成 inner 的正常
结束不在 outer 放弃时重发，内层循环跳转也不结束更外层 pending receiver。默认/显式
Borrow、Group、同根嵌套逆序、for 清理顺序、后续参数使用、跨 source 身份及原子撤销见
[参数前缀切片](../specs/active/0341-shared-lowering-core.md#127-borrow-参数-receiver-的控制转移前缀结束2026-11-09)。

## Shared receiver 的正常完成观察

Unit `receiver_call_return_ends()` 在实际正常调用完成时，从原 pending 队列移出完整
receiver token，发布独立 `UnitReceiverCallReturnEndFact`；不从最终 receiver descriptor
反推执行。正常完成准入 owned 名称根、声明端 Borrow/Inout 参数的 Name/Group 根及具体 nominal 的 this；
都要求 Shared receiver 与 owned-result 方法。非 owning 参数只结束本次 nested Shared reborrow，
原参数能力与 caller owner 不被结束或析构；prefix 查询使用相同根资格。
Exclusive receiver reservation、StaticSelf this、投影、temporary、provider、显式借用绑定与 borrowed-result
receiver 继续排除；String.clone intrinsic 的独立完成路径不在此查询内。

正常完成仍移除原 retain 所选的 call/receiver tokens，其他调用及 action 的顺序和 loop
depth 不变，不新增 iteration action 或改变 owner 清理。已完成 inner token 不在后续
outer unwind 再出现；分支两侧的同一 receiver 可分别有 CallReturn 与 ControlTransfer
事实，两者互斥，不是重复动态执行。事件只证明相应静态完成点，不承诺参数/receiver/drop
全序、全局 absence 或完整借用流。错误/deferred 与无效 typed 输入撤销新表；证据见
[SPEC-0341 正常完成切片](../specs/active/0341-shared-lowering-core.md#123-shared-receiver-正常调用完成2026-11-09)。

声明端默认/显式 Borrow、嵌套及重复 reborrow、分支/循环静态点、被捕获的参数及资源参数
的无析构义务由 [Borrow 参数切片](../specs/active/0341-shared-lowering-core.md#125-borrow-参数-receiver-的正常-reborrow-结束2026-11-09)固定。
调用者后续读取和其唯一 owner drop 另有源码正控；静态 caller/callee 参数身份不充当动态别名映射。


Inout 参数的根 binding 为 Exclusive，不改变其内部普通方法的 Shared receiver token。
正常返回与 Return/Break/Continue 查询观察该子 reborrow 的实际退休，不结束父 Inout
能力。后续变异准入、callee 无参数 drop、caller 唯一 owner drop 与未激活 reservation
继续排除的证据见 [Inout 子借用切片](../specs/active/0341-shared-lowering-core.md#129-inout-参数的-shared-receiver-reborrow-结束2026-11-09)。
这些是 frontend 静态事实及源码准入证据，不是 native 动态权限恢复或完整生命周期证明。


具体 class/value class 的隐式、显式和 Group this 现使用同一真实退休 token 查询；
nominal 声明须与 This target 相同，隐式来源核 owner，显式来源核 source-qualified this。
查询不依赖收集前缀时已切换的 callable context。正常完成、控制转移、循环深度、两阶段
activation 与原 owner 清理均未改动；子 Shared 结束不移出或析构 non-owning this。
StaticSelf、provider this、Exclusive receiver 和 borrowed-result 仍不进入此切片；
未支持的 from-this 结果延续仍 L0164，存活 Shared 下的新 Exclusive 仍 L0135。
四份旧 this 排除源码原样转为非空 oracle，完整身份、模式、跨 source 及撤销证据见
[this 子借用切片](../specs/active/0341-shared-lowering-core.md#131-具体-this-的-shared-receiver-reborrow-结束2026-11-09)。
