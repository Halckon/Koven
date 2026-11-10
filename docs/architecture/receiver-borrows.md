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

仅 owned-result 方法、Shared receiver 与 owned 或声明端 Borrow/Inout 参数的具名根及普通字段路径，
以及具体 nominal 的 this、首层直接根/字段来源借用绑定准入；
参数的 nested reborrow 结束不结束原参数能力，也不析构 caller owner。正常返回、
Exclusive receiver reservation、StaticSelf this、索引/this 字段投影、temporary、provider、调用返回/父子链借用绑定与 borrowed-result
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
反推执行。正常完成准入 owned 名称根、声明端 Borrow/Inout 参数根及其普通字段路径、具体 nominal 的 this，及首层直接根/字段来源借用绑定；
都要求 Shared receiver 与 owned-result 方法。非 owning 参数只结束本次 nested Shared reborrow，
原参数能力与 caller owner 不被结束或析构；prefix 查询使用相同来源资格。
Exclusive receiver reservation、StaticSelf this、索引/this 字段投影、temporary、provider、调用返回/父子链借用绑定与 borrowed-result
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


具名根的普通字段 receiver 现保留完整 typed field path，Group 透明；语法链只接受
Name/Group/非 safe Member，最终观察目标必须等于完整 place，不能以其 backing root
代替字段身份。正常与 Return/Break/Continue 结束均来自原 token；owner、字段 replacement、
loop depth 与两阶段 activation 不变。同根兄弟字段保持独立路径，aggregate 只承担原有
唯一 drop，non-owning 根不增加析构义务。索引、this 字段、provider、alias 上再次投影的字段
及 borrowed-result 继续排除。临时 owner 字段 receiver 的未形成 place 错误，以及 safe
字段链的最终类型拒绝，在本片基线仍存在；它们不是已准入后的空结束表证明。
六份旧排除源码迁移、跨文件字段声明身份及真实撤销见
[具名字段子借用切片](../specs/active/0341-shared-lowering-core.md#133-具名字段路径的-shared-receiver-reborrow-结束2026-11-09)。


首层 `borrow val alias = root`（含 Group）现在观察其 Shared receiver 子 token 的正常及
Return/Break/Continue 退休。准入须为普通 BorrowedStorage、无 parent/source loan，完整
initializer place 等于原 origin 根；来源仍须满足 owned/Borrow/Inout 根资格及 provider
排除。receiver source 保留 alias operand，target 保留 canonical source lease，不能把
alias 解释成 owner，或把来源 lease 当作 callee 内投影的 payload 地址。父 binding 可继续
供后续调用使用，其实际结束及来源唯一 drop 保持原计划；存活父 binding 仍阻止 source
变异；本片 class alias 的 Exclusive 调用仍因父 Shared loan 冲突报 L0135。循环内新建 provider 元素 binding 仍在准入阶段
L0164，并非合法源码的空事件证明。调用返回来源、父子 binding 链、alias 字段、
carrier 及 borrowed-result 不由本片放行。原 Single 见证保留；真实 token、父子终止点、
跨 source 身份和撤销证据见
[直接绑定子借用切片](../specs/active/0341-shared-lowering-core.md#135-首层直接借用绑定的-shared-receiver-结束2026-11-09)。


首层直接 binding 的 initializer 现也可为具名根的普通字段链，含透明 Group。initializer
与 receiver 共用 Name/Group/非 safe Member 语法检查，完整 typed place 必须等于原
binding origin 和实际 receiver target；字段符号与 source identity 不可退化为根。
BorrowedStorage、无 parent/source loan、alias operand 为完整根及原来源能力/provider
排除保持。父 binding 的最后使用、字段 replacement 与 aggregate 唯一 drop 仍按原计划；
同根兄弟字段保持独立，父借用存活期间允许不重叠字段变异，重叠字段/整根 move 仍 L0135。
调用返回来源、父子链、alias.field 与 this 字段来源不扩张。MoveOnly sequential index
initializer 的实际 L0136 是前置拒绝，不是合法空观察证明；不放行 safe/temporary/index
来源或 native 权限恢复。原两份字段来源排除源码已原样迁为非空 oracle，证据见
[字段绑定子借用切片](../specs/active/0341-shared-lowering-core.md#137-首层字段来源借用绑定的-shared-receiver-结束2026-11-09)。


Shared receiver 的正常/前缀观察现与 String.clone 独立完成表共同接入同一分析产物。
组合合同以真实源码同时形成三表，核 Return/Break/Continue 保留已完成 inner clone、
不可达后缀不伪造完成事件，并核后来源错误/deferred 对 raw 与公开三表的共同撤销。
owner 观察开关比较完整 Analysis；这些有界组合证据仍不提供跨表动作总序或消费者退休。
见[生命周期候选合批验收](../specs/active/0341-shared-lowering-core.md#139-receiver-与-stringclone-生命周期候选合批2026-11-09)。

Receiver、直接调用和借用结果 handoff 的根分类共用当前源文件 checker 的只读索引。
首次查询建立已形成 owner、迭代绑定与 provider 根、借用结果绑定三个集合。索引不替换
checker 的回滚记录或顺序产物，也不改变实际退出动作和事实发布门禁。

同一索引按 source-qualified 初始化表达式登记本地借用绑定的原位置。正常调用的 handoff
和绑定运输只查询当前 initializer 的候选，仍按原顺序验证 pending、父依赖和各自的来源条件。

转发借用单独按实际调用惰性索引已验证来源 loan 与返回来源。返回表达式沿用原有分组归一，
同键仍取原先首项。索引只参与候选查找，token 仍由真实 pending 集合移交到实际返回边。

索引仅改变查询成本；Shared receiver 的 Borrow/Inout、普通字段、具体 this 与首层直接
binding 资格和实际 token 退休保持。索引在 alias 解析为 canonical source place 后查询；
This provider 仍使用原 This 身份检查，不混入只含 Place 根的集合。

## 有界 owned 参数 caller 的 refine 路由

SPEC-0341 的有界路由现使单一显式 `own` 非 Resource 普通 class 参数、Unit block
仅一条零实参 Shared receiver 调用的 caller 进入 refine。首片仅非泛型 caller、
无类型实参的 class/call instance，方法 owned 结果限 Int/Unit。来源必须为原始 Name 根与
非 safe Member；typed descriptor、Owned binding、真实 CallReturn receiver end 及
同根 Drop 均匹配，且整个函数仍通过原 signature/body/drop subset。
迁移触发不读取 loan observation 或 protocol 2 Complete；caller/callee 各自选路，
原样 Node/run 中 run 迁移而 touch 保持原路径。已有 refine producer 唯一生成
RootPlace、BorrowBegin、DirectCall、BorrowEnd 与 NamedDrop，不增加 P2 receipt。
真实 frontend off 重分析后原样 lowering 的等价性与原实体顺序 oracle 保留；不扩大
implicit this、temporary、projection、two-phase、borrow-result、控制流或 replacement。
这不是全体 receiver 迁移或旧 Compatibility/oracle 退休；其它既有迁移触发仍独立生效。
