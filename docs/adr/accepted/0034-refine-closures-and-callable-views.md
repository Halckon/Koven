# ADR-0034：细化层中的 lambda、callable 值与 Borrow 函数参数的 callable view

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：设计或审查 E3 闭包家族（N04）的细化层 lowering、callable SSA 类型与调用 ABI 时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-10-28 用户明确确认：“按建议的方式来，然后要设计和做好逃逸分析”。即采用本记录推荐的 Borrow 函数参数 callable view（不采用方案 A 单态化），并把第 6 节的逃逸分析作为实施与验收的组成部分。

## 背景

路线图 N04 的五个 probe 在 main `e0127ffb` 上两种模式结果一致，都失败：

| probe | 结果 | 实际失败点 |
|---|---|---|
| p17 / p17b | BE:UnsupportedNode | `apply(f: (Int) -> Int, x)` 体内的 `f(x)`：无标记函数类型参数是 Borrow，旧路径只会调用 Value 绑定的 callable |
| q02 | BE:UnsupportedNode | 同上（`twice` 内 `f(f(1))`） |
| p18 | BE:UnsupportedNode | `fun make(own s: String): () -> Boolean = move { s == "x" }`：返回捕获 String 的 move closure |
| q03 | FE:L0165 | `val f = ::inc` 的 overload 选择未完成，属于 frontend 缺口 |

另外，旧路径只在绑定上下文中构造带捕获的 lambda（`closure_binding_context`），直接作为实参的
带捕获 lambda 会被拒绝。

已有基础：

- frontend 按 lambda 发布 capture 事实（`UnitClosureCaptureDescriptor`：来源、类型、Shared/Owned、
  Borrow/Copy/Move effect、引用 Span）、`UnitClosureDescriptor`（是否 move、`Transferability`）、
  `UnitCallTarget::FunctionValue` 调用目标，以及 `LambdaEntry` 等 drop 点。
- [ADR-0009](0009-concrete-closure-internal-abi.md) 已定 SSA/LLVM 表示：无捕获用 function
  pointer，有捕获用 concrete closure `{code, inline environment}`；thunk 签名为“environment 的 shared
  loan、用户参数 → 返回值”；`ClosureConstruct`、`FunctionAddress`、`CallableInvoke` 与
  `closure_escape` guard 已在 SSA 中。ADR-0009 同时禁止运行时类型擦除，并要求
  higher-order 使用点保留 concrete provenance；需要把多种 layout 放进同一运行时位置时必须拒绝，
  直到新 ADR 决定。
- [ADR-0032](0032-ownership-refinement-layer.md) 规定细化层是终态唯一的函数体 lowering，
  每迁一个家族就删掉被取代的旧模块。

还没定的长期问题有两个：

1. lambda 体与 closure 形成如何进入细化层；
2. Borrow 函数参数（Guide 中无标记函数类型参数的默认契约）在 callee 里看到什么。callee 只知道
   frontend 函数类型，看不到调用方的 concrete closure layout。p17/q02 正卡在这里。

## 决策

### 1. lambda 体由细化层作为 thunk 函数实例 lowering

- 每个函数实例里的每个 lambda 产生一个 thunk 函数实例，与普通函数使用同一个 refine builder、
  子集预检和 SSA 构造。thunk 的 `RefineInputs` 以 lambda body 为函数体，以 lambda 参数为参数，
  参数 mode 取自函数类型契约；类型替换与 static `Self` 沿用外层实例（SPEC-0361 的具体化规则）。
- thunk 的入口实体依次为：环境的 shared loan（仅有捕获时），然后是用户参数。shared capture 从
  环境字段读出被借 storage 的 loan；owned capture 在 thunk 内只能读取（经环境字段的 shared
  loan），不能移出也不能赋值，与 Guide 的 capture 规则一致。thunk 不取得也不析构环境。
- lambda 内的 drop、loan、delivery 事实（包括 `LambdaEntry`）都在 thunk 的 slot CFG 中落点，
  和普通函数一样做到恰好一次。

### 2. closure 形成在外层函数体中显式化

- 无捕获的 lambda 和函数引用是 `FunctionAddress`；有捕获的 lambda 是 `ClosureConstruct`，
  operand 顺序就是 capture descriptor 的顺序。
- shared capture 在形成点对来源 binding 做 `BorrowBegin`；这个 loan 由 closure owner 持有，
  在 frontend 给出的 closure ASAP drop 点先 `Drop` closure，再结束这些 loan。closure 在当前
  callable 内绑定或移动时，loan 跟着 owner 走，SSA 构造按普通槽位状态校验。
- owned capture 按 effect 处理：Copy 读出 snapshot，Move 消费来源 owner（来源在此路径上不再可用）。
  closure 的析构仍是 ADR-0009 定的 type-directed drop glue，按 capture 逆序进行。

### 3. Borrow 函数参数使用不拥有的 callable view

- 新增 SSA 类型 `CallableView(signature)`。它只作 **shared loan 的 target**，不是 value 类型：
  view 在 SSA 中就是 `Loan { Shared, CallableView(signature) }` 实体，因此天然不能被 return、
  存进 aggregate/heap/container、交给 Value 参数或被 drop，这些位置只接受 value。LLVM 中该 loan
  是指向调用方帧内 `{code pointer, environment pointer}` 描述符的指针。view 不拥有、不分配、
  没有 drop 函数。
- 调用方把实参交给 Borrow 函数参数时，先像其他 Borrow 实参一样对 callable owner 建立 shared
  loan（直接写在实参位置的 lambda 先形成临时 closure，调用返回后按 `CallReturn` 事实析构），
  再用新操作 `CallableViewOf { loan }` 从这个 loan **再借出** view。concrete closure 的 code
  是它的 thunk，environment 指针指向被借的环境；function pointer 的 code 是编译器为该函数生成的
  view adapter（签名为“environment 指针（忽略）、用户参数”，内部转调原函数）。这样 view 只有一种
  调用约定。
- callee 的 Borrow 函数参数就是这个 view loan。新操作 `CallableInvokeView { view, arguments }`
  经 view 调用，只读，不消费。view 可以作为 Borrow 实参继续往下传（`SharedReborrow`），也可以被
  borrowed closure 作为 shared capture 捕获；不能被 move closure 捕获（frontend L0138）。
- 生命期复用既有 loan 规则：`CallableViewOf` 的结果与 `SharedReborrow` 一样依赖源 loan，必须在
  源 loan 结束前 `BorrowEnd`；源 loan 结束后 view 的任何使用都是 `LoanInactive`。

这一条局部扩展 ADR-0009：后者禁止的是“拥有者”层面的类型擦除，即统一 heap box、erased drop
和引用计数。view 只出现在借用边界上，不拥有、不析构、不分配，所以 owned 表示与析构的决定
保持不变。

### 4. Value 位置保留 concrete identity

- 函数返回 closure 时（p18），函数的 SSA 返回类型取自它所有返回点上的 concrete callable 类型；
  各返回点的 layout 必须一致，否则在子集预检中拒绝（不合并、不擦除）。调用方的绑定沿用这个类型，
  `f()` 是对 owned callable value 的 `CallableInvoke`。
- Value 模式（`own`）的函数类型参数、字段或容器中的 callable，以及跨 layout 的 join，都不在
  首片范围内。这些位置需要按 concrete callable 类型实例化，或者另立 ADR；T1 的
  `move (...) -> T` 入口届时一并决定。在那之前，这些位置留在子集外，并且明确拒绝。

### 5. 边界与迁移

- 函数引用 `::f` 是 function pointer 的 `FunctionAddress`。q03 的 L0165 是 frontend overload 选择
  的缺口，另开 frontend Spec 处理，不在 codegen 中推断。
- 递归 closure 环境（[ADR-0025](../proposed/0025-recursive-closure-environment-handles.md)）、mutating closure
  的独占 capture、跨线程 `Transferable` 检查都不在本 ADR 范围。
- 路由：外层函数与 lambda 体（thunk）各自按子集预检进入细化层（SPEC-0370）；两者之间只隔着
  ADR-0009 固定的 thunk ABI（环境 shared loan、用户参数 → 返回值）。旧路径的 thunk 入口与返回捕获清理
  （`bind_callable_entry`、`end_thunk_capture_views`）及函数参数单态化查询（`CallableAbi::parameter_type`）
  随 SPEC-0370 删除；仅含未迁移 Map 迭代家族的无捕获 `() -> Unit` lambda 保留最小回退分支，待容器与
  Map 迭代迁入细化层后彻底移除 `unit_lower/closure*`。

### 6. 逃逸分析：三层，各管一件事

闭包的安全性由三层共同保证。语义判断只在 frontend 做；细化层只做结构约束，不推断；SSA verifier
独立复核，不依赖前两层的正确性。

**第一层：frontend（语义权威，源码诊断）**

| 逃逸形态 | 诊断 | 现状（main `e0127ffb` 实测） |
|---|---|---|
| Borrow 函数参数被 return、存字段、交给 Value 参数、经局部别名 return | L0133（不能从 Borrow 移出） | 已拒绝 |
| Borrow 函数参数被 move closure 捕获 | L0138 | 已拒绝 |
| borrowed closure 被 return、存字段或容器、交给 Value 参数 | L0137 | 已拒绝 |
| borrowed closure 经 move closure 再逃逸（含多层 move 嵌套） | L0137 | 已拒绝 |
| 捕获来源在 closure 存活期间被移动、修改；mutating closure 存活期间外部读写 | L0135 | 已拒绝 |
| **closure 经赋值或 block 结果流到外层 binding，而它借用的 binding 所在 scope 已结束**（`loop`/`if` 体内的局部被外层 `var` 持有的 closure 借用，或经 move closure 间接持有） | L0135 | **漏报**：frontend 接受，drop 事实把被借 binding 放在 scope 出口析构，loan 仍被外层持有，属于 use-after-free。由 SPEC-0363 修复 |

SPEC-0363 的规则：在每个词法 scope 结束时（正常出口、`break`、`continue` 三条流都检查），
若外层 binding 或 scope 结果值所持有的 closure（含经 capture 可达的 closure）仍持有对本 scope
binding 的 shared capture loan，就报 L0135。primary 是 capture 引用，label 指向被借 binding 的
声明。直接持有的 closure 的逃逸视为根因，只在没有直接逃逸时才报经 capture 间接可达的 loan，
保证每处逃逸只报一次。

**第二层：细化层（结构约束，不推断）**

- view 只在 Borrow 函数参数的实参位置由 `CallableViewOf` 产生；callee 中 view 只是 Borrow 参数。
  细化层不产生把 view 交给 owned 位置的指令。
- shared capture loan 的结束点固定在 closure 的 drop 之后；closure 的 drop 点、loan 结束点都只来自
  frontend 事实。事实与 CFG 不符时报 `InvalidModel`，不猜测修补。
- 子集预检拒绝第 4 节列出的未支持位置；不允许把多种 closure layout 汇入同一个槽位。

**第三层：SSA verifier（独立复核）**

- view 是 loan 实体，不是 value：return、Value 调用实参、aggregate/heap/container 存储、替换写入
  这些 owned 交付位置在类型层面就收不到它。verifier 另外拒绝以 `CallableView` 为 target 的 loan
  出现在 `CallableViewOf`/`SharedReborrow`/`CallableInvokeView`/shared capture/`BorrowEnd` 之外的位置。
- view 生命期：`CallableViewOf` 只接受活跃的 shared loan，其结果登记为该 loan 的 reborrow 依赖；
  源 loan 结束前 view 必须先结束，结束后任何使用都非法。
- `closure_escape` guard 不变：捕获了 view 的 closure 属于带 shared capture 的 borrowed closure，
  在 owned 交付处照旧拒绝。
- 既有 owner/loan 校验继续生效：closure 持有 loan 期间，被借 owner 不能被 drop 或移动；
  closure 的 drop 必须早于其 capture loan 的结束。

**验收要求**：frontend 逃逸矩阵（上表每一行至少一组拒绝与合法邻居）、SSA 层手工构造的 view 逃逸
反例（每类 owned 交付各一例）、细化层端到端 native 运行（含 ASan/LSan 干净）三者都要覆盖。

## 替代方案

### 方案 A：higher-order 函数按 concrete callable 单态化

即严格按 ADR-0009：`apply` 每遇到一种 closure layout 就生成一个实例。不采用。Borrow 参数只做
同步调用，单态化会让每个直接写 lambda 的调用点都多一份 callee 实例；递归 higher-order 函数、
project 模式下跨文件的同一 callee 也会让实例数膨胀。而且 Borrow 契约本来就是“借给你调用”，
不需要 callee 知道布局。

### 方案 B：统一的 heap 环境 + erased drop（existential fat pointer）

不采用，理由与 ADR-0009 相同：形成闭包时会隐式分配，还要引入 erased drop。Guide 没有授权这种
运行时表示。

### 方案 C：Borrow 函数参数在 callee 里收一个指向 concrete closure 的 loan

不采用。callee 不知道 target 类型，loan 的 `target` 无法确定。如果改成按 target 单态化，
就回到了方案 A。

## 后果

收益：

- p17、p17b、q02、p18 有了统一的 lowering 路线；lambda 体复用细化层，不再新增闭包专用的
  carry/merge；
- Borrow 函数参数与其他 Borrow 实参的规则一致，callee 只生成一份代码；
- owned closure 的表示、析构与逃逸规则（ADR-0009、L0137、`closure_escape`）都不变。

代价与风险：

- 新增一个 SSA loan target 类型（`CallableView`）和两个操作（`CallableViewOf`、`CallableInvokeView`）。
  verifier 与 LLVM adapter 都要支持；function pointer 还需要 view adapter；
- 通过 view 调用比直接调用多一次间接；
- Value 模式函数参数与存储位置仍然拒绝，T1 之前还需要一份后续决定。

## 关联

- 相关 Spec：[SPEC-0363](../../archive/specs/0363-closure-scope-escape.md)（frontend scope 逃逸）；[SPEC-0364](../../archive/specs/0364-refine-closures.md)（E3 闭包家族，步 1–3 已交付）；[SPEC-0365](../../archive/specs/0365-closure-return-facts.md)（闭包返回来源事实）；[SPEC-0370](../../specs/active/0370-refine-lambda-bodies.md)（lambda 体与局部 callable 迁入细化层）；q03 的 frontend overload Spec（待开）
- 相关 ADR：[ADR-0009](0009-concrete-closure-internal-abi.md)（局部扩展：借用边界的 view）、[ADR-0025](../proposed/0025-recursive-closure-environment-handles.md)、[ADR-0032](0032-ownership-refinement-layer.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
