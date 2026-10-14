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
