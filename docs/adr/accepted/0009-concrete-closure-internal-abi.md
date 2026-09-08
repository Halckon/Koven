# ADR-0009：具体闭包使用函数指针与内联环境的内部 ABI

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-25 依据当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权接受。本
决策只封闭现行 v0.28 留给 SPEC-0038 的 typed SSA / LLVM 内部表示，不改变 lambda capture、
函数类型、所有权、逃逸、`Transferable` 或析构语义。

## 背景

现行 guide 已经确定：捕获闭包单态化为具体的“捕获环境 + 函数指针”，无捕获函数值退化为
裸函数指针；v0.27 又固定了 shared/owned capture、borrowed closure 逃逸限制和逆序 owned
capture 析构。SPEC-0032 已发布按 lambda 查询的 capture mode/effect/type/drop facts，
SPEC-0034/0035 已建立 owner-aware typed SSA、first-class aggregate 和内部调用边界。

尚未确定的是环境是否隐式堆分配、函数指针如何接收环境、shared capture 在 LLVM 中保存什么，
以及不同 closure layout 是否通过运行时类型擦除统一。若这些选择散落在 SSA model、verifier、
drop glue 和 LLVM adapter 中，同一源码函数类型可能获得互不兼容的表示，也可能绕过 guide
禁止的隐式 `Box` 或借用逃逸边界。

## 决策

### typed SSA 保留具体 callable identity

- IR 区分无环境的 function-pointer type 与有环境的 concrete-closure type。两者 identity 都
  包含规范化的有序参数契约和返回类型；closure identity 还包含源码顺序的 capture slot
  mode/type。它们不复用 frontend map-local `TypeId`，也不把 LLVM pointer 当作长期类型身份。
- 无 capture 的函数引用或 lambda 必须使用 function-pointer type，不构造空环境 closure。
  有 capture 的每个 lambda/单态实例使用一个 concrete-closure type；不同 capture layout 不
  因源码函数类型签名相同而合并。
- 与 frontend 已发布的 `TypeKind::Function` 一致，v1 的 function pointer 和 closure value 都按
  MoveOnly owned value进入 SSA。调用只读取 callable，不消费它；显式 move、return、CFG
  transfer 与 drop 仍由既有 owner verifier 处理。

### LLVM 表示与调用

- function pointer lower 为 LLVM opaque pointer，指向签名为“用户参数 → 返回值”的已声明
  Koven function。取得地址不分配内存，也不生成 drop glue。
- concrete closure lower 为 first-class `{function pointer, environment}` aggregate；environment
  是字段顺序与 capture descriptor 完全一致的 concrete LLVM struct。owned capture 以内联值
  保存，shared capture 保存指向既有 storage 的 pointer；v1 closure 不产生 exclusive capture
  slot。
- closure thunk 的内部签名为“environment、用户参数 → 返回值”。调用点从 closure value
  提取函数指针和 environment，并把 environment 作为只读首参数执行 indirect call；这次物理
  参数传递不产生新的语义 owner，thunk 不析构或取得 environment 所有权。
- 环境和 closure aggregate 的大小、对齐与字段 offset 只来自 ADR-0008 已固定的 target
  `DataLayout`。参数/返回 ABI 可由 LLVM 选择寄存器或临时内存，但不得因此调用 allocator、
  改写为 `Box` 或增加运行时 storage tag。

### formation、loan 与 drop

- owned environment formation 按 capture descriptor 顺序求值：Copyable capture 读取 owned
  copy，MoveOnly capture 消费原 owner；构造只在全部 capture operand 就绪后产生完整 closure
  owner，不向 SSA 暴露部分初始化环境。
- shared environment formation 保存现有 shared loan 对应的 storage pointer。该 loan 必须在
  closure owner 存活期间继续参与 move/mutation/drop 冲突，并在 frontend 指定的 closure ASAP
  drop point 结束；LLVM pointer 本身不能替代或缩短这一证明。
- closure drop 按 capture 逆序对 owned MoveOnly slot 调用现有 type-directed drop glue；
  Copyable slot 不调用 glue，shared slot 只结束对应 loan，不析构来源。function pointer drop 是
  无运行时动作的所有权终点。
- abort 路径没有 unwind cleanup。正常路径必须继续满足每个 closure owner 与 owned capture
  恰好消费或析构一次。

### 不引入运行时类型擦除

- SPEC-0038 不建立 `{code, void* environment, drop function}` existential fat pointer、统一 heap
  box、引用计数或动态 dispatcher。higher-order callable 的 frontend→SSA lowering 必须保留
  编译器可查询的 concrete closure provenance，并在调用实例中使用相应 concrete type。
- 当一个尚未封闭的 join、存储或外部边界要求把多个不同 closure layout 放进同一运行时
  storage 时，lowering 必须明确拒绝该未支持边界；不得临时堆分配或按最大布局猜测 union。
  若现行 guide 的完整一等函数值验收最终需要 existential closure coercion，应先由新 guide / ADR
  明确其所有权与 drop ABI，再取代本记录的这一边界。

## 替代方案

### 统一 heap-allocated type-erased environment

不采用。`{code, env pointer, drop pointer}` 容易统一不同 closure layout，但会让捕获 closure
形成时发生隐式 allocation，并引入 erased drop 与额外间接调用。现行 guide 要求具体闭包结构体
与单态化，且没有授权这项运行时表示或失败路径。

### 所有 closure 都使用栈上 environment pointer

不采用。它适合 borrowed closure，却不能表示允许 return/字段存储的 move closure；让指针指向
形成点栈帧会直接违反 v0.27 的逃逸规则。

### 把 capture 展平为 thunk 的隐藏参数而不形成 environment value

不采用。它可以调用立即使用的 lambda，但不能让 closure 作为 first-class owner 绑定、移动、
return 或按指定位置析构，也无法把 shared loan 生命周期绑定到 closure owner。

### 使用 LLVM trampoline 或宿主 closure ABI

不采用。trampoline 具有目标与可执行栈约束，宿主 Rust/C++ closure ABI 也不稳定；两者都会把
Koven 的所有权和确定性布局交给未授权的外部约定。

## 后果

收益：

- 无捕获函数值保持单指针且零分配，capturing closure 的布局和 drop 可由现有 typed SSA /
  `DataLayout` 静态验证；
- owned move closure 可作为 first-class aggregate 逃逸，不依赖形成点栈帧或隐式 heap owner；
- shared capture pointer 仍受显式 loan identity 约束，不把 LLVM pointer equality 当成借用证明；
- closure 与 aggregate/container 共用一套 owner、CFG transfer、drop glue 和 LLVM verifier 边界。

代价与风险：

- frontend function type 不携带 concrete closure identity，lowering 必须额外保存 provenance 并
  单态化 higher-order 使用点；
- shared closure 的 loan 随 owner 跨 block 转移需要 verifier 维护明确依赖，不能只验证裸 pointer；
- 多种 concrete closure 在同一 storage 的统一表示尚未封闭，对应源码边界在获得新决策前必须
  明确拒绝；
- first-class environment 参数的最终机器 ABI 由当前 LLVM target backend 决定，不形成跨版本
  或 public FFI 承诺。

## 关联

- 相关 Spec：SPEC-0032、SPEC-0033、SPEC-0034、SPEC-0035、SPEC-0038、SPEC-0039
- 相关 ADR：[ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0007](./0007-llvm-toolchain-and-first-target.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
