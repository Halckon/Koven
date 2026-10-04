# 顺序迭代事实与provider

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改迭代事实校验、资源元素或provider provenance时 · **唯一真源**：frontend/codegen实现与测试

## Frontend事实与消费次序

出口phi仅在binding仍存活时保留非ASAP resource义务；不因缺少后继读取丢弃它，也不复活已消费binding。
`IterationExitPlan::condition`只读发布规划时的实际path，发布阶段不重读后续状态。
`validate_iteration_facts`以同轮typed descriptor核对source求值、Shared bindings、退出condition与
element/provider/source清理顺序；temporary-root替换和漏Return descriptor被拒绝。
codegen按identity→diagnostics→deferred→constants→for schema消费；字段封闭，无public mutation入口。
Return descriptor只要求最近callable内的循环退出；读取现有名称作用域排除嵌套lambda/函数Return，
循环自身位于lambda内时仍必须保留真实Return退出，不建立跨callable返回语义。

## SSA与资源边界

本机恢复增加direct nongeneric resource class容器元素与resource value-class字段支持。
借用Cell字段保留root loan，Copyable字段读取副本，MoveOnly字段作为Borrow交付；
constant-present drop只接受exact source或resource binding的有限Header/Exit入边图，
全部实际运输边必须保留同一symbol，并归于唯一Always Entry来源；独立control条件仍需DAG蕴含。
动态来源和普通非source owner不由此放行，不新建存在位ABI。

provider lifetime另用独立path-correlated provenance状态，沿实际edge并行重绑定loan/place，
保留结束来源的tombstone，拒绝stale place和reborrow revival；固定预算耗尽明确拒绝。
原SharedFieldLoan全局alias配对的误报边界保持，不宣称通用verifier重写。
test-only SyntheticZero仅供内部SSA的MoveOnly ZST计数夹具；实际container drop loop保持，
插桩区分logical element/container drop与storage malloc/free，不提供源码ZST能力。
本次本机通过范围见[恢复账本](../development/recovery-local-delivery.md)。

## 实现入口

- frontend：`ownership_checking/iteration_validation.rs`、`drop_planner/iteration/phi_state.rs`。
- lowering：`lower_frontend/constant_presence.rs`、`aggregate.rs`、`borrow_argument.rs`。
- verifier：`verify_ownership/provider_lifetime.rs`，沿真实CFG独立检查有限状态。
- 实际native与ZST测试：`native_sequential_for_tests/owned_source_tests.rs`、`llvm/synthetic_zst_tests.rs`。

unit 普通与 constant-enabled driver 已消费同轮 source-qualified iteration 事实：
Array/List/MutableList 的 named owned、Borrow 参数与 temporary source 均可真实 object/link/run。
长度在 preheader 求一次快照，source 仅求值一次；source/length/cursor/element 经已有 pending
运输跨 CFG，复用 single provider 的 snapshot、guard、element 与固定步长 primitive，无 provider 分配。

名称、discard 与 nongeneric value-class 借用解构支持 fallthrough、continue、break、return 及
嵌套 for/while；cleanup 按前端完整动作序列执行一次，call/receiver loan 以 fact 身份映射实际
pending 槽，conditional receiver 按具体 Copyable/MoveOnly 类型执行。return 的 owner/copy 先交付，
再清理局部 owner、element、source 与临时源；Abort 不展开，source 中转移不清理尚未 Acquire 的 provider。

field source（包括 Field symbol root）、Inout source 与 captured Borrow closure 的 native 表示
仍明确 Unsupported。两视图边界负例核 LLVM 调用为零并逐字节保全旧产物；ordinary/constant
混轮 facts 仍在 lowering 前拒绝。实际 for 的 allocator oracle 逐 pointer 核 class 元素、临时源、
局部/外围 owner 与 conditional receiver 的唯一释放顺序，Report 直接读取教程原源码另核 String drop。
测试入口为 `native/unit_for_*tests.rs`；single provider/native 回归单独保留。

## Compilation-unit 前端事实

`UnitSequentialIterationDescriptor` 与 `UnitIterationOwnershipPlan` 通过 source-qualified statement
交接；Array/List/MutableList 的 owned、Borrow、Inout、field、temporary source 均建立 Shared
source/element loan。具名与解构 binding 只借用元素；临时源及临时字段接收者保活到 EndSource。
`source_access` 描述能力而非源形状：`this.field` 的 place root 是 Field symbol 且 projection
列表可为空，消费者必须读取已验证的 symbol/projection 身份，不能把它误认作 named Borrow 参数。

`iteration_cleanup_at(point)` 发布同点完整清理序列，嵌套 return 各 plan 引用等值动作，消费者
执行一次且不再重复执行平面 drops。实际已求值 call 前缀先 EndCallLoan，局部 closure 先结束
Shared capture，然后清理 element、provider/source 与临时 owner；最近 loop/callable 限定退出。
Abort 和 source 自身在 Acquire 前的转移不发布该 provider 的清理。常量入口仅为实际可达
provider 发布计划，与普通 capability 隔离；两出口均验证模板身份、完整退出及有序动作。

实现入口为 frontend `compilation_unit/iteration.rs`、`dataflow/iteration.rs`、
`dataflow/drop_planner/iteration.rs` 与 `iteration_validation.rs`。unit native 消费入口为
`ssa/unit_lower/loop_control/iteration.rs` 与 `iteration_cleanup.rs`；保留上节列明的表示边界。

conditional `StaticSelf` receiver 的析构也以 `DropConditionalReceiver` 进入同点完整序列；
下游仅在具体MoveOnly实例执行，Copyable跳过，不重复消费同点平面conditional facts。
原 `preceding_drops` 保留非迭代消费兼容；它只计普通drop，不能定位provider动作之间的位置。
验证器另核pending receiver形成时的provider集合，确保body内receiver在EndElement之前、
外围receiver在EndSource之后；嵌套return按每个frame的loop depth清理pending owner。
