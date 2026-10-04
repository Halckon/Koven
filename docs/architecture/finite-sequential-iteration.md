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

unit driver仍拒绝真实for；拒绝原子性由actual-for分析链验证，失败后普通受支持unit可真实发射、链接和运行。
single driver的真实for成功及拒绝矩阵独立保留，不从ordinary unit成功推断unit-for能力。

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
`dataflow/drop_planner/iteration.rs` 与 `iteration_validation.rs`；不改变本页现有 unit native
拒绝边界，A5 尚未交付。
