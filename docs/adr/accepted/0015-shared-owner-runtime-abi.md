# ADR-0015: 单线程共享 owner runtime ABI

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-26 依据当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据
依赖图推进”的站立授权接受。本 ADR 只决定现行 v0.30 §30.2 留给 runtime ABI 的 control
block、SSA 与 LLVM 表示，不改变 `Rc<T>` 的源码语义。

## 背景

v0.30 已确定 `Rc<T>` 是单线程 MoveOnly shared owner：构造取得 payload 所有权，普通赋值
移动 handle，只有 `.share()` 显式增加 strong count，`.value` 只形成 shared Borrow，最后一个
handle 自动析构 payload 并释放 allocation。ADR-0008 已固定 target `DataLayout`、系统
`malloc/free/abort` 和无 header 的独占 class/Box，但明确没有定义共享 owner header。

SPEC-0045 因而还需要统一回答：counter 的宽度与布局、retain/release 如何进入 typed SSA、
payload place 如何保持只读、溢出和归零如何处理，以及是否新增 runtime crate。若 frontend
lowering 和 LLVM adapter 各自猜测这些行为，就会产生无法由 verifier 证明的一套隐式 ABI。

## 决策

### Control block 与 handle

- target-independent SSA 新增 `SharedOwner<payload>` 类型身份；它始终是 MoveOnly，并以一个
  非空 handle 表示。该类型不能复用独占 `HeapOwner`，因为二者的复制来源和 drop effect 不同。
- LLVM control block 是按目标 `DataLayout` 构造的 `{ strong: usize, payload: T }` struct；
  `usize` 使用 target pointer-width unsigned integer，strong 字段固定在 payload 之前，padding
  和最终 allocation size 只由真实 target layout 决定。
- 一个 live `Rc<T>` 是指向该 control block 的非空 pointer。`Rc<T>?` 使用 null 作为 nullable
  handle；非空分支仍指向同一 control block，不增加外层 allocation 或 tag。
- control block 没有 weak count、allocator identity、vtable、RTTI 或锁。不同单态 `T` 具有不同
  静态 block/drop glue；该内部 ABI 不承诺跨编译器版本或公开 FFI 稳定性。

### 分配、retain 与 release

- `SharedAllocate(payload)` 消费一个完整 payload，受检计算 control-block storage size 后调用
  ADR-0008 的集中 `malloc/abort` adapter；成功路径先写入 strong `1` 和完整 payload，再发布
  shared owner。大小溢出或 null allocation 直接 abort，不建立部分 owner。
- `SharedRetain(owner)` 要求输入 owner Available，但不消费它，生成同类型的新 MoveOnly owner。
  LLVM 先读取 strong；若已等于 `usize::MAX`，在任何写回前 abort，否则写回 `strong + 1`。
  counter 是非原子的，禁止生成 atomic RMW、锁或线程同步。
- 对 `SharedOwner` 的既有 `Drop` lower 为 release：读取非零 strong，写回 `strong - 1`；结果
  非零时结束，结果为零时先调用 payload 的静态递归 drop glue，再把原 control block pointer
  精确传给一次 `free`。verified SSA 必须排除已 move/dropped handle 的重复 release；counter
  下溢属于编译器不变量破坏，不是用户可触发的恢复分支。
- payload 可以是 ZST；control block 仍因 strong 字段具有非零大小并只进行一次 allocation。
  payload 内嵌其他 Rc 时使用对应 release glue；strong cycle 不会归零，v0.30 已把 Weak/cycle
  回收排除在 v1 外。

### SSA 与借用交接

- SSA 明确提供 `SharedAllocate`、`SharedRetain` 和 `SharedPayloadPlace` operation；render、model
  verifier 与 ownership verifier 必须锁定 operand/result 类型、source order 和 owner 状态。
  LLVM adapter 不按 `Rc`、`share` 或 `value` 字符串猜测 operation。
- `SharedRetain` 是唯一允许从一个 Available MoveOnly shared owner 产生第二个 owner obligation
  的 operation。它不读取或修改 payload，可与 shared payload loan 共存；不存在 v1 的
  exclusive payload loan。源 owner 后续移动/drop 和新 owner 的 drop 分别由 verifier 跟踪。
- `SharedPayloadPlace` 产生带 shared-read-only 能力的 payload place。它可建立 `Read`/Borrow，
  `T: Copyable` 时可沿既有规则复制值；verifier 必须拒绝 `Mutate`、Inout、MoveOnly owned read
  和任何把 place 生命周期延长到 owner 之外的路径。
- frontend 负责把 compiler-bound Rc construction/share/value facts lower 到上述 operation；
  codegen 不重新决定类型实参、Copyable、Transferable、loan 或 drop point。

### Runtime 边界

- 不新增 `lang-runtime` crate。`lang-codegen` 现有 runtime adapter 继续唯一声明
  `malloc/free/abort`，并在内部生成单态 retain/release/drop glue；`lang-frontend` 与
  `lang-std` 不接触 LLVM pointer、header offset 或 counter 宽度。
- 首个目标继续是 ADR-0007 的 AArch64 macOS；新增目标必须重新验证 `DataLayout`、系统
  allocator alignment、pointer width 和生成的 control-block IR。

## 替代方案

### 让普通赋值隐式 retain

不采用。它会让 MoveOnly 赋值语义出现 Rc 特例并隐藏非平凡成本，还会违反 Koven
`Copyable` 不允许 retain/copy glue 的现行契约。显式 `SharedRetain` 只承接源码 `.share()`。

### 只在标准库源码实现计数

不采用。v1 没有 unsafe pointer、allocator、用户析构器或足以表达 shared payload place 的
公共能力；伪造普通 class 会失去 verifier 可见的 retain/release effect，并可能重复 free。

### 统一使用原子计数的 Arc

不采用。v1 的 Rc 恒不满足 Transferable，原子计数不能单独让 payload 线程安全，却会让每次
share/release 承担不必要的同步成本。Arc/Shareable 必须和跨线程共享语义一起设计。

### Arena/handle 取代 Rc

不采用作为通用替代。Arena 适合整批同生命周期对象图，但不能独立回收对象，且源语言尚无
安全表达 handle 与特定 arena identity 的能力；v0.30 已将其保留为后续互补方案。

### 外置通用 retain/release C runtime

不采用。payload drop glue 与单态布局都已在 codegen 内可知，外置 erased runtime 需要额外
metadata/function pointer、发布边界和 ABI 稳定承诺，却没有当前消费者。

## 后果

收益：

- Rc 的每一次 owner 分叉、loan 和 release 都进入 typed SSA/verifier，不依赖 LLVM 名称猜测；
- 复用 ADR-0008 的 target layout 与系统分配边界，不增加 workspace member；
- 普通赋值继续保持 MoveOnly 语义，retain 成本只出现在显式 `.share()`；
- 单态 payload drop glue 能覆盖嵌套 Rc、class、Box、enum 与容器，并在归零时只执行一次。

代价与风险：

- 每个 Rc allocation 至少增加一个 pointer-width counter 和对齐 padding；
- 非原子计数使 Rc 永远不能跨线程，未来 Arc 需要不同类型/operation/ABI；
- strong cycle 会泄漏，Weak 或 cycle-aware 方案必须由后续 guide/ADR 新增；
- SSA verifier 必须支持“非消费输入产生新 MoveOnly obligation”这一受限特例，测试不足时可能
  造成遗漏 release 或重复 free；
- nullable Rc 的 null niche 与新增 target 都需要单独 LLVM 正反验收。

## 关联

- 相关 Spec：SPEC-0045
- 相关 ADR：[ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0007](./0007-llvm-toolchain-and-first-target.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
