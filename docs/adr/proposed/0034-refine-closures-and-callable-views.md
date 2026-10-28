# ADR-0034：细化层中的 lambda、callable 值与 Borrow 函数参数的 callable view

> **性质**：架构决策记录 · **状态**：proposed · **读取时机**：设计或审查 E3 闭包家族（N04）的细化层 lowering、callable SSA 类型与调用 ABI 时 · **唯一真源**：本 ADR

## 状态

proposed

## 接受依据

不适用（`proposed`）。2026-10-27 用户排定 E3 顺序时确定第 5 项“闭包（N04）先写 ADR”；本记录待用户确认。

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
- [ADR-0009](../accepted/0009-concrete-closure-internal-abi.md) 已定 SSA/LLVM 表示：无捕获用 function
  pointer，有捕获用 concrete closure `{code, inline environment}`；thunk 签名为“environment 的 shared
  loan、用户参数 → 返回值”；`ClosureConstruct`、`FunctionAddress`、`CallableInvoke` 与
  `closure_escape` guard 已在 SSA 中。ADR-0009 同时禁止运行时类型擦除，并要求
  higher-order 使用点保留 concrete provenance；需要把多种 layout 放进同一运行时位置时必须拒绝，
  直到新 ADR 决定。
- [ADR-0032](../accepted/0032-ownership-refinement-layer.md) 规定细化层是终态唯一的函数体 lowering，
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

- 新增 SSA 类型 `CallableView(signature)`，LLVM 表示为 `{code pointer, environment pointer}`。
  它不是 owner：没有 drop，不分配内存，也不带 drop 函数。它只在产生它的 shared loan 存活期间有效。
- 调用方把实参交给 Borrow 函数参数时，先像其他 Borrow 实参一样对 callable owner 建立 shared
  loan（直接写在实参位置的 lambda 先形成临时 closure，调用返回后按 `CallReturn` 事实析构），
  再用新操作 `CallableViewOf { loan }` 得到 view。concrete closure 的 code 是它的 thunk，
  environment 指针指向被借的环境。function pointer 的 code 是编译器为该函数生成的 view
  adapter：签名为“environment 指针（忽略）、用户参数”，内部转调原函数。这样 view 只有一种
  调用约定。
- callee 的 Borrow 函数参数在 SSA 中就是 view 值。`CallableInvoke` 的 callable operand 可以是
  concrete callable value，也可以是 view；调用都只读，不消费 callable。view 可以继续作为 Borrow
  实参往下传（复制 view，不新建 loan），但不能 return、存进字段或容器、交给 Value 参数，
  也不能被 move closure 捕获。这些位置在 frontend 已经报 L0137；SSA 的 `closure_escape` guard
  也把 view 当作“可能带 shared capture”的值，在 owned 交付处拒绝。
- verifier 要求每个 view 依附于一个活跃的 shared loan，用法与 `ReadLoan` 的依赖相同。loan
  结束后 view 不再可用。

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
- 递归 closure 环境（[ADR-0025](0025-recursive-closure-environment-handles.md)）、mutating closure
  的独占 capture、跨线程 `Transferable` 检查都不在本 ADR 范围。
- 路由：一个函数实例里有 lambda 或函数值调用时，它的全部 thunk 也必须通过子集预检，才整体进入
  细化层；否则整体走旧路径。这样不会出现外层和 thunk 分属两条路径的混合 ABI。
- 闭包家族迁入后，按 ADR-0032 删除被取代的旧模块，包括 `unit_lower/closure*` 和
  `callable_abi` 里的 per-family 部分。容器构造器回调（`List(n, f)`）改用同一套 thunk 与 view
  机制，随删除旧模块的那个 Spec 迁入。

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

- 新增一个 SSA 类型（`CallableView`）和一个操作（`CallableViewOf`）。verifier、LLVM adapter 与
  `closure_escape` 都要支持；function pointer 还需要 view adapter；
- 通过 view 调用比直接调用多一次间接；
- Value 模式函数参数与存储位置仍然拒绝，T1 之前还需要一份后续决定。

## 关联

- 相关 Spec：后继 E3 闭包 Spec（待本 ADR 接受后开）；q03 的 frontend overload Spec（待开）
- 相关 ADR：[ADR-0009](../accepted/0009-concrete-closure-internal-abi.md)（局部扩展：借用边界的 view）、[ADR-0025](0025-recursive-closure-environment-handles.md)、[ADR-0032](../accepted/0032-ownership-refinement-layer.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
