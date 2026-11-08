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

Map-for 与已退休整数内建/Range operation 的组合以同函数的 typed facts 激活现有
`scope_requires_refine`，仍逐项校验 Map provider 与完整函数子集；无关 sibling 中的操作
不改变纯 Map-for 路由。验证范围见 [SPEC-0383 §5](../specs/active/0383-refine-final-legacy-retirement.md#5-map-for-与整数家族的组合路由修复)。

逐实例通过细化准入的 callable 直接进入 refine。若完整 body 仍需兼容 lowering，数值区间
`for`、`Map.consume` 与委托 receiver 按当前 callable 的路由及实际操作事实判定，
不依赖编译单元是否另有 Map 迭代。兼容路径仍检查 source-qualified descriptor、资源身份
及最终 SSA。细化构建出错直接返回结构化错误。

普通借用返回转发按本次调用实参实际产生的 CFG 判定。直线求值与可合并的无条件跳转
保留来源身份。实参分支仍以 `UnsupportedNode` 拒绝，定位到实际分支的源码范围。
直接 `Map.requireValue` 结果走独立的 Map 入口。

普通调用结果的局部绑定可以与借用获取前、或有序且已验证的结束点之后的独立 `if`
共存。该判断只沿普通块的语句顺序传播，活跃 loan 的分支和实参仍走原有边界。
编译单元中的其他函数或 lambda 的所有权原语不改变这一路径。

## 两段 lowering

- 细化（`refine/build`）：facts 映射为槽位 CFG。析构点、Borrow 实参 loan、Value 交付、null 比较的
  proof view 都是显式语句；view 只在 `if`/`while` 条件真边上建立，并在 owner 析构、移动或控制转移
  前结束。null 比较 operand 的析构点在两条出边各发一次。
- SSA 构造（`refine/emit`）：先把单前驱的直线后继并入其 `Goto` 前驱，再按槽位活跃性生成 block
  参数，非空目标末尾追加 view。abort 块继承同一分支另一条边的活跃集合，存活 owner 因此在 Abort
  边上有归属；产物经完整 verifier。

调用后双 Abort 的测试 oracle 按 edge arguments 与目标参数的配对位置追踪入口 owner，
验证预期调用确实消费它，再检查调用后的边不续传其重命名身份。调用前 Abort 和其他
存活 owner 可正常运输；独立正反例的 CI 验收状态见
[SPEC-0383 §6 复审记录](../specs/active/0383-refine-final-legacy-retirement.md#6-pr-300-参数身份断言复审)。

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
`String` 的 `const val` 使用点物化。顺序容器（SPEC-0375/0376）：支持 `Array<T>`、`List<T>`、`MutableList<T>`
的字面量与空容器构造（`ContainerConstruct`）、运行时长度构造 `Array<T>(size, init)` 与 `List<T>(size, init)`
（`ContainerGenerateBorrowed` 及负长度守卫）、`.size`（`ContainerLength`）、Copyable 下标读取与 MoveOnly
元素借用实参（`ContainerElementPlace`）、元素赋值替换 `xs[i] = v`（`ContainerReplace` 消费 `AfterReplacement`
析构）、`MutableList<T>` 的 `add`/`clear`/`removeAt`/`removeFirst`/`removeLast`/`insertAt`，以及具名顺序容器的
`for` 遍历（`Discard`/`Name`/`Destructure`，按 `iteration_cleanup_at` 消费退出序列）。`Box<T>`、递归装箱枚举与
`Map` 核心操作（SPEC-0377）：支持 `Box(payload)` 构造（`HeapAllocate`）、`Box<T>?` 窄化与 `!!` 解包、含 `Box<T>`
负载的递归/MoveOnly `enum class` case 构造（`TaggedConstruct`）与参数/返回传递，以及 `Map<K, V>` /
`MutableMap<K, V>` 的 `mapOf`/`mutableMapOf`（`MapConstruct`）、`.size`（`MapSize`）、`containsKey`（`MapContains`）
与 `put`（`MapPut`）。复合 `class` 字段、通用 `Rc<T>` 负载与智能窄化接收者（SPEC-0378）：支持任意非递归 `storage_type`
的 `class` 字段读取/替换与 `SharedHeapFieldLoan` 借用实参、任意非递归 `storage_type` 的 `Rc<T>` 负载与 `Rc<T>?` 证明视图、
非条件位置的 MoveOnly 空值比较、智能窄化 `class` 接收者的成员调用以及完备双分支布尔字面量 `when`。MoveOnly 局部可变重赋值与
MoveOnly `enum class` `when`（SPEC-0379）：支持任意 `storage_type` 局部 `var` 绑定的重赋值（结束旧证明视图、消费
`BeforeOverwrite` 并恢复槽位活跃追踪），以及携带任意 `storage_type` 载荷的 MoveOnly `enum class` 的构造、`is`/`!is` 判别、
`when (subject)` 分支匹配（当 `subject` 在 `AfterExpression` 提前释放时先快照 `TaggedDiscriminant`）、标量载荷投影与
MoveOnly 载荷字段借用实参。`deinit` 析构体、多层字段借用、接口委托、无状态 `object`、`Map.withValue`、内联 `Inout` 接收者与一级字段 `replace`（SPEC-0380）：支持 `Item::Deinit`
析构体与带 `deinit()` 的资源 `class`、多层 `class` 字段读取与借用（按层 `SharedHeapFieldLoan` 与逆序 `BorrowEnd`，含字段路径上的
`String.clone()`）、含复合存储字段的 `value class` 与 `Box<T>` 载荷、MoveOnly 类型上的 `Value` 接收者成员调用（`own fun`、`this` 借用与
`this` 返回及 `UnitDropTarget::This` 释放）、临时 `Borrow`/`Value` 接收者、接口 `by` 委托链（`site.delegation()`）与 `StaticSelf`
默认方法特化（含条件 `Value` 接收者交付与析构）、无状态 `object` 单例、`Map.withValue`（`ROp::MapWithValue`）、内联 `Inout` 接收者（`value class` 与 `enum class` 的 `inout fun`、`InlineFieldReplace` 及调用后写回）、一级非泛型 `class` 字段 `replace(&holder.field, value)`（`HeapFieldExchange`）以及任意 Copyable `storage_type` 的 `Inout` 参数与 `&local` 实参。默认分流边界修复与旧路径退休（SPEC-0381）：按求值顺序与参数槽位映射正确支持命名实参乱序求值；`snapshot_binding_if_name` 仅对 Copyable 绑定生成快照，避免 MoveOnly 绑定残留槽位导致 InvalidSsa；短路逻辑运算（`&&`/`||`）按分支隔离 `self.dead` 与合流生命周期；一级字段 `replace(&holder.field, rhs)` 在求值 RHS 期间将字段独占借用纳入 `pending_call_loans`，完整支持 RHS 内提前退出（`return`/`break`/`continue`）；删除旧路径模块 `field_replace.rs` 与 `enum_type_test.rs`（共 705 行），并在 `unit_lower.rs` 与 `ownership_primitive.rs` 收敛为显式 `UnsupportedNode` 边界拒绝。`Map` 剩余单步操作、通用值类型与 `Unit` 根原语及旧路径退休（SPEC-0382）：支持 `Map<K, V>` / `MutableMap<K, V>` 的任意非递归 `storage_type` 值类型 `V`、`m[k] = v` 下标赋值、`Map.get(k)` / `m[k]`（`ROp::MapGet` 与 `ROp::MapResultUnwrap`）、`MutableMap.remove(k)`（`ROp::MapRemove`）、`MutableMap.consume()`（`ROp::MapConsume`）及查询/移除结果的空值与值判等；支持零大小 `Unit` 局部可变根的 `replace(&left, make())` / `swap(&left, &right)` 及 RHS 控制转移退出，并删除已 100% 由细化层接管的 `unit_lower/ownership_primitive.rs`。最终默认路由收敛与旧路径退休收尾（SPEC-0383）：激活整数运算、整数区间、`String.clone()`、可信文本原语、扩展函数、`Borrow Rc<T>`、`IntRange?`/`LongRange?` 与 `Map.withValue`/`Map.requireValue` 的默认 `ssa::refine` 路由，支持扩展函数内 `this` 借用、`borrow val` 下 `value class` 字段投影与多跳短路 `Abort` 路径活跃 owner 传播，删除旧路径 5 个按家族模块（`integer.rs` 100 行、`integer_range.rs` 74 行、`text_primitive.rs` 118 行、`map_with.rs` 108 行、`map_require.rs` 84 行，基线共 484 行手写代码），并保留遗留 `map_for` 支撑代码供独立循环切片推进。枚举、非空断言与可变列表变更旧路径退役及细化层准入补齐（SPEC-0384）：将非泛型 Copyable `enum` 的函数签名、构造、字段投影与 `this` 表达式全量纳入 `ssa::refine` 默认路由，支持 MoveOnly `enum` 的 Copyable 非标量载荷投影（如 `Result<String, TextError>` 的 `TextError` 载荷）、`for` 借用 / `borrow val` 绑定的 `String` `when` 主语、单分支完备布尔 `when`（`when (flag) { true, false -> ... }`）、`(when (...) { ... })!!` 非空断言临时操作数、`if (x == null)` 的 `else` 分支非空证明视图与带控制流退出的扩展方法借用接收者，并删除已 100% 由细化层接管的 `unit_lower/enum_lower.rs`（410 行）、`unit_lower/control/when_subject.rs`（137 行）、`unit_lower/non_null_assertion.rs`（215 行）及 `unit_lower/container.rs` 中的 6 个 `MutableList` 变更方法（合计清理约 1,030 行冗余旧代码）。`deinit` 字段借用、独立堆对象可空 `if` 与字符串字段接收者旧路径退役（SPEC-0385）：删除已 100% 由细化层接管的 `unit_lower/deinit_borrow.rs`（262 行）与 `unit_lower/receiver/string_field.rs`（62 行），并移除 `unit_lower/control/nullable_if/heap.rs` 与 `unit_lower/control/nullable_if.rs` 中的独立堆对象 `if` 证明入口（`prepare_heap_nullable_if_entry`、`heap_if_direct_borrow_call`、`heap_if_call_qualifies` 与 `standalone_if_needs_root_borrow`，合计再减约 755 行旧代码）。`Inout` 接收者、接口委托、无状态 `object` 与局部解构旧路径退役（SPEC-0386）：补齐 `ssa::refine` 括号接收者（`(this).field = v`）与具名 `Inout` 接收者/接口 `by` 委托调用默认路由准入，并在 `unit_lower` 中退役 `Inout` 接收者方法体字段替换（`lower_current_receiver_field_assignment` 与 `Inout` `this` 分支）、接口委托接收者（`lower_delegated_call_receiver`）、无状态 `object` 单例接收者（`lower_stateless_object_receiver`）与局部解构（`lower_destructuring`，合计再减约 558 行旧代码）。`String.clone()`、装箱构造、`value class` 字段借用与通用 `when` 旧路径退役（SPEC-0387）：补齐 `ssa::refine` 对 `value class` 字段、`Rc<String>` `.value` 载荷与顺序容器下标 `xs[i]` 接收者上 `String.clone()` 及非返回来源 `Borrow` `value class` 参数 MoveOnly 字段借用的准入与构建，删除 `unit_lower/string_clone.rs`（226 行），并在 `unit_lower` 中退役 `IntrinsicBox` 聚合构造、`field_borrow.rs` 中的 `value class` 字段借用（176 行）与 `control.rs` 中的通用 `when` 链（476 行，使 `unit_lower/control.rs` 从 844 行降至 396 行，合计再减约 903 行旧代码）。数值区间 `for`、运行时长度容器构造、`Map.get`、非可空 `Map.consume` 与 `$map.result` 旧路径退役（SPEC-0388）：补齐 `ssa::refine` 对运行时长度容器构造与 `Map` 操作中 `Nothing` 发散实参及块内提前终止后不可达语句的准入与构建，删除 `unit_lower/loop_control/numeric_iteration.rs`（529 行）、`unit_lower/container/runtime.rs`（242 行）与 `unit_lower/map_result.rs`（173 行），并移除 `unit_lower/map_consume.rs` 中非可空 `lower_map_consume`（147 行）与 `unit_lower/map.rs` 中 `lower_map_get`（合计再减约 1,260 行旧代码）。具名函数引用、共享闭包捕获、可调用视图借用与顺序容器元素操作旧路径退役（SPEC-0389）：补齐 `ssa::refine` 对具名函数引用（`KnownFunction`）、`Array<Unit>` / `List<Unit>` 运行时生成、嵌套 Copyable `value class` 投影、局部整型复合赋值（`+=` 等）及 `Abort` 边退役借用槽位过滤的准入与构建，删除 `unit_lower/closure/known_function.rs`（186 行）、`unit_lower/closure/shared.rs`（96 行）与 `unit_lower/callable_view.rs`（46 行），并移除 `unit_lower/container.rs` 与 `unit_lower/assignment.rs` 中的元素下标读取/借用/替换（约 340 行，合计再减约 670 行旧代码）。`RangeView`、借用结果函数与绑定、`Rc` 操作、`String.clone()` 及带捕获闭包旧路径退役（SPEC-0393、SPEC-0394）：将 `RangeView`（`View<T>`）与 `borrow_result`（`borrow val`、`borrow_return`、`carrier_return`）提升为 `ssa::refine` 无条件默认路由并补齐多出口短期 `RangeView` 清理，删除 `unit_lower/range.rs`（549 行），并在 `unit_lower` 中退役 `borrow_result.rs` 的 `BorrowCall` / `BorrowReturn` / 字段借用结果（605 行）、`Rc.share()` / `Rc.value`、直接 `String.clone()` 与带捕获闭包构造（合计再减约 1,480 行旧代码）。

编译单元启用旧前置门禁时，普通借用绑定仍有原整函数 CFG 边界。直接字段绑定核对 typed 投影与 owned 来源字段，
交细化层消费绑定和生命周期事实。已限定到同一稳定根 `Borrow` 别名的 Map provider
也可跳过原整函数扫描，由旧 lowering 核对直接来源、活动 shared loan 与 provider 交接。
RangeView、carrier 别名和其他绑定保留原有前置边界。

## inline nullable 布局

`SsaTypeKind::InlineNullable { inner, niche }`：String 与顺序容器用 bytes/buffer 指针 niche（空串与
空容器使用非 null 的 `koven.zst.sentinel`），其余 inner 用 `{i1, T}`。复用 NullableWrap/Null/IsNull/
Take 与 `NullableBranch`；drop glue 只在 present 时释放 payload。旧 `unit_lower` 的存储能力仍只到
pointer-like nullable。

实现入口：`crates/lang-codegen/src/ssa/refine.rs` 及 `refine/` 子模块、`ssa/inline_nullable.rs`、
`llvm/runtime/inline_nullable.rs`；测试在 `ssa/refine_tests.rs` 与 `llvm/inline_nullable_tests.rs`。
