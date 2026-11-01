# 所有权细化层

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 `ssa::refine`、子集预检或 inline nullable 表示时 · **唯一真源**：`lang-codegen` 代码与测试 · [ADR-0032](../adr/accepted/0032-ownership-refinement-layer.md) · [ADR-0033](../adr/accepted/0033-inline-nullable-representation.md) · [SPEC-0350](../archive/specs/0350-ownership-refinement-nullable-pilot.md)

## 分流

`lower_owned_unit_with_entry` 对每个无类型替换、无借用/carrier 返回的函数实例先做
确定性子集预检（`ssa::refine::subset`），只读 typed/ownership facts 与 AST 构造种类，不试跑 lowering。
receiver 只接受 Copyable enum（Value/Borrow）与普通 class（Borrow/Inout）；Borrow 参数接受 Copyable 类型与
普通 class（后者只经 loan 借出）。预检通过且函数签名或函数体
触及任一 nullable 类型、或使用已迁入的家族（`when`、enum `is`、字段替换、`Inout` 参数或 receiver、
`&` 实参、`Rc` 构造与操作）时，函数体由
`ssa::refine` lowering；失败即报错，不回退。其余函数仍走 `unit_lower` 的按家族路径；带 `Inout` 参数的函数若未进入细化层，以 UnsupportedNode
拒绝（unit planning 已为 `Inout` 参数分配 exclusive loan 实体）。精确子集以
SPEC-0350 §3 的已实现清单与 SPEC-0356 为准。

## 两段 lowering

- 细化（`refine/build`）：facts 映射为槽位 CFG。析构点、Borrow 实参 loan、Value 交付、null 比较的
  proof view 都是显式语句；view 只在 `if`/`while` 条件真边上建立，并在 owner 析构、移动或控制转移
  前结束。null 比较 operand 的析构点在两条出边各发一次。
- SSA 构造（`refine/emit`）：先把单前驱的直线后继并入其 `Goto` 前驱，再按槽位活跃性生成 block
  参数，非空目标末尾追加 view。abort 块继承同一分支另一条边的活跃集合，存活 owner 因此在 Abort
  边上有归属；产物经完整 verifier。

已接入：Boolean/整数/String 标量与 checked 算术、String 字面量/拼接/比较、`println`、顶层直接调用、
`if`/`while`/短路、`when`（无 subject 时 Boolean 条件按序测试；标量 subject 按值比较或测整数 Range 成员；enum subject
比较判别值，值位置无 `else` 时落空边为不可达 Abort；命中边消费 `WhenAlternativeMatch`；String subject 的无插值 literal/Group 首核
消费 0357 的 `WhenComparisonCompleted` 与 `WhenAlternativeNoMatch`，比较先于候选清理，
直接 Borrow String 参数通过 caller loan 比较，未扩展 borrow-result binding/field/index/复杂候选；
新增真实源/计数测试的官方 CI 结果见 [0360](../specs/active/0360-string-when-refinement.md)）、
Copyable 且具体实例 payload 全为标量的 enum（含非泛型与泛型实例：case 构造、`is`/`!is`、无 payload case 等值、经
`TaggedPayloadPlace` 读取窄化名、`this` 或隐式 `this` 的 payload）及 enum 方法的 receiver、`loop` 与 `break`/`continue`（跳转前结束本轮循环打开的 view）、inline nullable、具体实例字段只含标量/String 的 class（含非泛型与泛型实例，SPEC-0372）及其 pointer-like nullable、
class 构造、Copyable 字段读取（经窄化 view、局部 owner 的临时 shared loan、Borrow/Inout 参数或当前
receiver 的 loan）、`!!`（Consume，null 边 Abort）；赋值与 `Inout`（SPEC-0358）：标量 `Inout` 参数经
`LoanMutate` 整体替换，`&local` 借出 root place 并在调用后读回（MoveOnly 经 `RootPlaceTake`），`&p` 继续
传递 `Inout` 参数的 loan；`owner.f = v` 先求值 RHS，再经 exclusive loan `HeapFieldReplace`，并由替换
消费 `BeforeReplacement` 析构；receiver 调用中 Borrow receiver 在实参前借出，`Inout` receiver 的
exclusive loan 在调用点激活。payload 为标量或普通 class 的 `Rc<T>`（SPEC-0359）：构造为 `SharedAllocate`、`share()` 为
`SharedRetain`，Copyable payload 的 `value` 经 `SharedPayloadPlace` 读取，`rc.value.f` 经 payload place 的
临时 shared loan 读取字段。泛型函数实例（SPEC-0361）：typed 事实中的模板类型经 `RefineInputs::concrete`
（实例替换与 static `Self`）具体化后再使用；Copyable、字段全为标量的 `value class` 构造为聚合值，字段读取
对 owned 值做 `AggregateProject`，对 loan 经 `SharedFieldLoan` 读出，receiver 可为 Value 或 Borrow。
`!!`（SPEC-0362）也接受括号名称与 `if` 临时 operand；Value 实参在调用点统一交付，实参中途的
`return`/`break`/`continue` 在跳转边释放已求值的命名 owner 实参（`ControlTransfer` 的 `Temporary` 事实）。
闭包家族（SPEC-0364/0365/0370、ADR-0034）：Borrow 函数参数接收 `CallableView` shared loan 并经
`CallableInvokeView` 调用；lambda 字面量形成 `FunctionAddress` 或 `ClosureConstruct`（shared capture
建立源借用，owned Move 捕获消费来源绑定），可直接借出为 view、作为函数返回值或绑定到局部 callable
变量（经 `CallableInvoke` 调用或借出为 view）；lambda 体（thunk）同样由 `ssa::refine` 构建，入口按序
投影环境字段借用（`SharedFieldLoan` 与 `SharedReferenceFollow`）并在 `Return` 前逆序 `BorrowEnd`。
整数内建、整数区间与文本原语（SPEC-0371）：支持 `inv()`、八整数受检转换（`CheckedIntegerConvert`）、
整数前缀 `+`/`-` 与位运算、`IntRange`/`LongRange` 值构造与 `in`/`!in`/`==`/`!=`、`String.clone()`
及可信标准文本原语（`CopyBytes`/`CopySlice`/`Utf8ErrorOffset`/`ByteLength`/`ByteAt`）。
局部解构与整数区间 `for`（SPEC-0373）：支持 Copyable/MoveOnly `value class` 的局部解构
（`AggregateCopyExplode`/`AggregateExplode`）以及 `IntRange`/`LongRange` 升序 `for` 循环。
所有权原语与 `const val` 物化（SPEC-0374）：支持局部可变根 `replace(&root, value)`（`RootReplace`）与
`swap(&left, &right)`（`RootSwap`，Copyable 根与新值先经 `Copy` 隔离 SSA 值身份），以及 `Boolean`/整数/
`String` 的 `const val` 使用点物化。顺序容器（SPEC-0375）：支持 `Array<T>`、`List<T>`、`MutableList<T>`
的字面量与空容器构造（`ContainerConstruct`）、`.size`（`ContainerLength`）、Copyable 下标读取与 MoveOnly
元素借用实参（`ContainerElementPlace`）、元素赋值替换 `xs[i] = v`（`ContainerReplace` 消费 `AfterReplacement`
析构）及 `MutableList<T>` 的 `add`/`clear`/`removeAt`/`removeFirst`/`removeLast`/`insertAt`。

## inline nullable 布局

`SsaTypeKind::InlineNullable { inner, niche }`：String 与顺序容器用 bytes/buffer 指针 niche（空串与
空容器使用非 null 的 `koven.zst.sentinel`），其余 inner 用 `{i1, T}`。复用 NullableWrap/Null/IsNull/
Take 与 `NullableBranch`；drop glue 只在 present 时释放 payload。旧 `unit_lower` 的存储能力仍只到
pointer-like nullable。

实现入口：`crates/lang-codegen/src/ssa/refine.rs` 及 `refine/` 子模块、`ssa/inline_nullable.rs`、
`llvm/runtime/inline_nullable.rs`；测试在 `ssa/refine_tests.rs` 与 `llvm/inline_nullable_tests.rs`。
