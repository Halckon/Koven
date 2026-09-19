# ADR-0023：无分配的借用式顺序迭代 provider

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：实现或评审顺序迭代 provider 时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-09-19 用户明确启用 v0.37，并将 temporary source 纳入首轮 native。依据持续 Goal
“继续实施 guide 和推进分阶段 specs”的站立授权，接受本 ADR；语言契约以
[现行 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider)为准。
接受不表示 provider primitives 或真实 for lowering 已实现。

## 背景

现有 frontend 已保存 `for` source、名称/解构 binding、body 与最近词法 jump target，但类型阶段
只检查 source 表达式，未发布 element/provider fact；所有权阶段也没有持久 source loan、逐轮
element binding 或退出清理。后端明确拒绝 `Statement::For`。

三种内建顺序容器已经具有固定 owner header、连续缓冲区、checked element place、shared loan、
loan 跨 CFG edge 传递、循环 block parameter 和递归 drop glue。缺少的是一个跨 Phase 一致的
provider identity、logical length/cursor 契约，以及正常/提前退出时的线性清理顺序。

把历史草案的抽象 `iterator()` / `hasNext()` / `next()` 当作普通方法并不可行：现行普通
receiver 选择不授予 intrinsic provider 身份；泛型 iterator 需要 associated provider 或动态
interface；`next(): T` 还会把
borrowed element 错写成 owned Value delivery。若各 Spec 分别选择 iterator object、索引循环或
runtime helper，会形成不兼容的 frontend facts 与 SSA/LLVM ABI。

另有一项既存表示漂移必须在 provider primitive 中收口：guide 把顺序容器 `size` 定义为
`Int`，LLVM header 使用 target `size_t`；当前 `ContainerLength` 只接受 owned `ValueId`，且
手工 SSA 测试把返回值当 signed 64-bit。借用 source 的真实 `for` 不能依赖这两个未闭合假设。

## 决策

### provider 是 compiler-bound 线性状态，不是 runtime object

- 首个 provider identity 只由 intrinsic `Array<T>`、`List<T>`、`MutableList<T>` 产生；不按
  源码类型或方法名称匹配。
- provider 的规范状态是 shared source access、一次 length snapshot、hidden cursor 与至多一个
  当前 element access。它只存在于 validated frontend plan 和 typed SSA CFG 中，不是 first-class
  value，不能存储、返回、捕获、动态分发或取得地址。
- LLVM 不声明 `iterator` / `hasNext` / `next` runtime symbol，不分配 iterator object，也不增加
  vtable、control block 或新 runtime crate。provider lower 为既有 container header length load、
  cursor compare/increment、checked `ContainerElementPlace` 和 loan/drop operations。
- `AcquireProvider` / `HasNext` / `NextPlace` / `FinishProvider` 是验证阶段的状态转换名称，不是
  必须新增四个 SSA operation；实现可以用既有 operation 组合表达。通用 operation/ownership
  verifier 负责证明 type、loan active-state、edge entity 与 cleanup 安全；length snapshot、cursor
  初值/递增和 guarded `NextPlace` 的 canonical traversal 形状由职责明确的 builder 与结构测试锁定，
  不声称通用 verifier 能从任意整数 CFG 反推出完整 provider 算法。

### length 与 cursor bridge

- source-visible logical length 始终是非负 `Int`，有效 container 保持
  `logical_length <= 2^31 - 1`。runtime header 继续使用 ADR-0008 的 target `size_t`，不得把
  Koven `Long` 或手工 i64 当作 `Int`。
- `ContainerLength` 扩为接受 container Value 或 active shared Loan，并产生真正的 Koven `Int`。
  LLVM 从 header 读取 `size_t`，先验证已建立的 representability invariant，再转换为 `Int`；
  缺少证明或超界是内部/构造边界错误，不能静默 truncate。
- hidden cursor 使用同一逻辑 `Int` 域，从零开始；只有在 `cursor < length` 的 edge 进入
  `NextPlace`。因此 `cursor + 1` 不溢出。checked element address 仍在 LLVM 边界受检转换为
  target index，不用新的 source-visible machine-size 类型。

### loan 与 CFG 生命周期

- owned place/temporary source 在 preheader 建立 shared root loan；Borrow source 可以复用
  dominating shared loan，Inout source 建立 shared reborrow。source loan 覆盖 header、body、
  backedge 与当前 element access。
- local source loan 通过 `EntityType::Loan` block parameter 显式跨 CFG edge；callee-entry shared
  loan 可按 ADR-0016 直接用于其支配的 successor。owner value 与 loan identity 都必须保持，
  以便结束 loan 后继续使用或析构 owner。
- `NextPlace` 只接受 active shared container loan，并建立当前动态索引的 element place/shared
  loan。名称和解构分量都映射为 loop-scoped shared borrow binding；Copyable read 可由 `Read`
  产生 copy，MoveOnly 不产生 owned value。
- normal fallthrough / `continue` 在 backedge 前结束 element-derived loans；`break` / exhaustion /
  `return` 还必须在线性路径上 finish provider 并结束本层 source loan。temporary source 只在这些
  动作之后 drop；abort 没有 unwind cleanup。
- operation verifier 要求 borrowed length/element place 使用 active shared container loan；
  ownership verifier 继续拒绝 inactive/重复 BorrowEnd、active loan 下的 replace/drop，以及 CFG
  edge 遗失或错误传递 owner/loan。逐轮只保留一个 current element 与各 exit 的结束顺序由
  SPEC-0211 facts、canonical provider builder 和 SPEC-0182 lowering structure/tests 锁定，不为此
  制造可存储 provider token，也不要求通用 verifier 证明 cursor 的完整遍历算法。

### 阶段与 ABI 边界

- frontend 负责发布 provider/binding typed plan 和 source/element/cleanup ownership facts；
  codegen 只消费 validated facts，不按 AST 名称重新推导 provider 或清理点。
- Phase 4 provider primitive 先以手工 SSA/LLVM 测试闭合 borrowed length、cursor、loan CFG、
  checked place 与 ZST；真实 `for` source lowering 由后继 integration Spec 完成。
- 本决策不改变 container header、allocator、元素布局、drop glue、外部符号或稳定跨 object ABI；
  它复用 ADR-0006/0008/0016 的现有内部边界。

## 替代方案

### 公开 `Iterable<T>` / `Iterator<T>` 与普通方法调用

不采用。普通 receiver 机制之外仍需 associated provider/borrow-return 或 dyn/type erasure，并
会让用户同名方法副作用成为可观察语义。它不是当前三种 intrinsic container 的最小闭环。

### `next(): T` 每轮交付 owned element

不采用。这会移动 MoveOnly element、在 container 中留下未初始化洞，或迫使实现做隐式 clone/
retain。消费式迭代需要取得整个 container owner并定义剩余元素清理，必须由后续 guide 单独设计。

### heap-allocated iterator owner

不采用。顺序容器的 length/index/place 已足够表达 traversal；额外 allocation、drop 和间接调用
没有语义消费者，并会扩大 runtime ABI 与失败点。

### 只生成裸索引循环，不在 SSA 验证 provider 生命周期

不采用。它能生成机器代码，但 source loan、temporary 延寿、element binding 与各 jump edge 的
清理只存在于 frontend 假设中，无法由 owner-aware SSA 拒绝遗漏或错误重排。

### 使用 pointer-width unsigned 作为新的源语言 size 类型

不采用。guide 已把 `size` 定义为 `Int`；暴露 machine-size 类型会改变语言和跨 target 行为。
内部 header 保持 `size_t`，在已证明 `Int` representability 的边界转换即可。

## 后果

收益：

- 三种顺序容器共享一个无分配、静态可验证的 provider，不依赖普通 receiver 选择或标准库迭代 API；
- MoveOnly element 保持在 container 中，Copyable 与 Borrow 使用沿用现有所有权规则；
- source loan、element loan、temporary owner 和 jump cleanup 可跨 frontend/SSA/LLVM 逐层验证；
- 不新增外部/runtime ABI，同时消除 `ContainerLength` 的 Borrow 与 `Int`/`size_t` 漂移。

代价与风险：

- loop lowering 必须显式携带 owner、source loan、cursor 和绑定状态，CFG merge/verifier 复杂度增加；
- `MutableList` 在整个循环期间保守 shared-borrow，不能安全地“修改不同索引”；
- 首轮不支持用户自定义、Map/range/String/IO 或 consuming iteration，后续扩展必须定义新的
  provider identity 和所有权交付，不能复用同名方法猜测；
- existing container length SSA/LLVM contracts 与部分手工测试需要迁移为真实 `Int`。

## 关联

- 相关 Spec：SPEC-0179、SPEC-0211、SPEC-0212、SPEC-0182
- 相关 ADR：[ADR-0006](0006-typed-ssa-block-parameters.md)、
  [ADR-0008](0008-internal-value-and-allocation-abi.md)、
  [ADR-0016](0016-interprocedural-borrow-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
