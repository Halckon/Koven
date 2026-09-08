# ADR-0008: 内部值表示与系统分配 ABI

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：任务涉及本 ADR 决策边界时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-08-25 依据当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权接受。
本决策只封闭现行 v0.28 留给 runtime ABI ADR 的后端表示、内部调用和系统分配边界，不改变
类型、所有权、析构点、求值顺序或其他源语言语义。

## 背景

SPEC-0035 需要生成内联聚合、普通 `class`、intrinsic `Box<T>` 和显式 drop/free；后续
SPEC-0036 还需要顺序容器 header、连续缓冲区及零大小元素 sentinel。现行 guide 已经确定：

- `value class` / `enum class` 是内联值，普通 `class` 和 `Box<T>` 是独占间接 handle；
- 类型大小或调用约定不得触发隐式 `Box`、堆分配或表示切换；
- v1 使用系统 `malloc` / `free`，分配失败与大小溢出 abort，不做异常展开；
- 顺序容器只有单一连续缓冲区表示，ZST 仍保留逻辑元素身份和 drop 次数；
- 首个 codegen target 已由 ADR-0007 固定为 LLVM 21 / AArch64 macOS。

尚未确定的是 LLVM 类型布局、Koven 内部函数怎样传递聚合、class/Box 是否携带 header、
allocator 调用放在哪个 crate，以及 ZST 容器使用什么物理 sentinel。若这些选择散落在各个
lowering 中，SPEC-0035、0036、0038 和 0039 会形成彼此不兼容的私有 ABI。

## 决策

### 目标布局是唯一大小与对齐真源

- `lang-codegen` 从 ADR-0007 已配置的 `TargetMachine` 取得 LLVM `DataLayout`，用它计算
  storage size、ABI alignment、struct field offset 和 stride；不得在 frontend 或 SSA 中复制
  AArch64 常量表。LLVM 类型和 module 的 data layout 必须来自同一个 target machine。
- `value class` 按主构造器字段声明顺序表示为 LLVM struct；padding 只由目标 data layout
  导出。有限 `enum class` 使用源码顺序从零开始的整数 tag，以及能容纳任一 case payload 的
  对齐 payload storage；payload storage 的大小和对齐同样由实际 case LLVM 类型导出。
- 普通 `class` 表示为指向 payload struct 的非空独占 pointer，payload 字段保持声明顺序。
  intrinsic `Box<T>` 表示为指向一个 `T` payload 的非空独占 pointer。两者都不携带 vtable、
  RTTI/type tag、引用计数或 allocator header；静态名义类型和单态化 drop glue 已足以确定
  布局与释放行为。
- 非 nullable class/Box owner 的 null pointer 是无效内部状态；nullable class/Box 使用 null
  表示 `null`，不再增加外层 tag。其他 nullable 值仍使用与其实际类型相符的显式 tag/payload，
  本 ADR 不把该 niche 规则扩展到任意类型。

目标相关的大小、对齐或 offset 无法由 `DataLayout` 表示时，lowering 必须在构造非法 LLVM
类型、GEP 或 allocator 调用之前失败；不能截断、环绕或退化成隐式 heap boxing。

### 内部调用不等于公开 FFI ABI

- 标量与 class/Box pointer 直接作为 LLVM 参数和返回值。固定大小 value/enum aggregate 也
  保持 LLVM first-class aggregate 参数或返回值，由已固定 target backend 选择寄存器或间接
  传递；Koven 不维护第二套手写的“大型值阈值”。
- target backend 产生的间接传递只允许使用调用者/被调用者临时存储，不授予新 owner、不让
  地址逃逸，也不得调用 allocator。MoveOnly 的唯一消费和 Copyable 的重复使用仍由 verified
  SSA 决定，而不是由 LLVM `byval`、pointer 拼写或 memcpy 猜测。
- `Borrow` / `Inout` 在 LLVM 边界使用指向现有 storage 的 pointer；其有效期和排他性完全来自
  frontend loan facts 与 SSA verifier。后端只能添加已经被这些事实证明成立的 LLVM 属性。
- 本 ABI 只约束同一 Koven 编译单元及其单态化实例，不承诺稳定符号、跨编译器版本兼容或 C
  互操作。public `extern` ABI 仍由 SPEC-0041 的新 guide/ADR 决定；object/linker 和确定性符号
  mangling 仍由 SPEC-0039 的 linker 决策封闭。

LLVM 明确区分 first-class aggregate、`byval`、`byref` 和 `sret`，并要求 pointer 属性满足
对应 storage/alignment 前提。因此 adapter 必须从真实 `DataLayout` 构造签名，不得仅凭源码
类型名称附加这些属性。

### 系统分配边界集中在 codegen adapter

- 不新增第六个 runtime crate，也不把 allocator shim 放入 `lang-frontend` 或 `lang-std`。
  `lang-codegen` 的单一 runtime-ABI adapter 负责声明并调用目标 C runtime 的
  `malloc(size_t)`、`free(void*)` 和 `abort()`；其他 lowering 不直接拼写这些符号或签名。
- 首个 AArch64 macOS target 的 `size_t` 使用目标 pointer-width integer。class/Box 的 payload
  allocation size 为 `max(storage_size, 1)`，确保零大小 owner 仍取得可释放的非空唯一 handle。
  `malloc` 返回 null 时立即调用 `abort()` 并终止该路径；成功结果才可进入初始化。
- 普通 class drop 先对仍 owned 的 payload 递归执行静态 drop glue，再对原始 allocation 调用
  `free`；Box 同理。内联 value/enum drop 不调用 `free`。nullable handle 只在非 null 分支执行
  drop/free。已经 move 的 storage 和 `Copyable` 值不得生成重复 drop 或 free。
- v1 没有用户自定义 destructor，guide 目前只要求已发布 drop fact 与每个 owned 字段恰好处理
  一次。本 adapter 使用确定性的逆声明顺序生成聚合 drop glue，但不把它宣布为新的源语言
  可观察保证；未来若开放自定义 destructor 或可观察字段析构顺序，必须先由新 guide 定义。
- abort 路径不生成 unwind edge、landing pad 或部分构造 cleanup。正常控制流只消费
  SPEC-0029/0030 已发布的 drop facts，不在 Phase 4 重新推导 owner liveness。

Apple 的 `malloc` 契约保证返回 storage 适合任意该平台数据类型；这覆盖现行 Koven 类型可产生
的目标 ABI alignment。将来若加入显式 over-aligned 类型，必须用新 ADR 扩展 allocator
边界，不能静默假定 `malloc` 继续满足。

### 顺序容器使用固定 header 和共享 ZST sentinel

- `Array<T>` 与 `List<T>` 的 owner header 为 `{buffer pointer, logical length}`；
  `MutableList<T>` 为 `{buffer pointer, logical length, capacity}`。header 没有 storage-kind tag、
  small-buffer 区域或逐元素 pointer。length/capacity 使用目标 pointer-width unsigned integer。
- `stride(T) > 0` 且 capacity 非零时，buffer 是一次受检 `capacity * stride(T)` 后得到的单个
  `malloc` allocation。正常 drop 先按 guide 规定的逆索引顺序处理 logical length 个元素，再
  `free` buffer。
- `stride(T) == 0` 或 capacity 为零时不请求 allocator。buffer 指向 module-private、具有当前
  target 所需最大 ZST alignment 的只读 sentinel；它永不传给 `free`。所有索引、loan 冲突和
  drop 次数继续按 container identity + logical index 判断，不能比较或解引用 sentinel 地址来
  合并元素 place。
- 固定 header 是 codegen/runtime 表示，不是用户可见 field 集合。后续 API 不得暴露 buffer、
  sentinel、capacity 或 allocator 调用次数。

## 替代方案

### 新增独立 `lang-runtime` crate

不采用。现行 workspace 已固定五个 member；当前 allocation 只需要三个系统符号和一个
codegen adapter。新增 crate 会扩大 bootstrap、链接和发布边界，却不能改善本阶段语义验证。
若线程、IO 或平台兼容层以后形成独立且真实的 runtime 职责，应另行通过 guide/ADR 决定。

### 在 `lang-std` 的 Rust target 中包装 allocator

不采用。`lang-std` 的公共实现以 `koven/**/*.ko` 为真源，当前 Rust target 只承载 Cargo
package 边界。使用 Rust global allocator 还会把现行 guide 已指定的 `malloc/free` 基线改成
另一项实现契约，并提前耦合尚未决定的 bootstrap/linker 方案。

### 所有聚合一律由手写 sret/byval 指针传递

不采用。它实现简单，但会把 target ABI 分类和 alignment 义务复制进 Koven adapter，并使小
聚合也固定经过 memory。让 LLVM 在已固定的 target/data layout 下 lower first-class aggregate
能满足“大型值可以间接传递但不得 heap boxing”的 guide 要求，同时保留后端优化空间。

### class/Box 使用统一带类型信息的对象 header

不采用。v1 没有 `dyn`、RTTI、类继承、共享所有权或自定义 allocator；header 没有消费者，
只会增加每次分配的大小并制造未来 ABI 承诺。普通 class 与 Box 共享 pointer 形状不代表它们
共享静态类型或可以互换。

### 对 ZST 容器执行 `malloc(1)`

不采用。它能为每个 owner 产生不同物理地址，但 Koven 已把元素 place identity 定义为逻辑
container + index，且不暴露 allocator 次数。共享对齐 sentinel 更直接，也避免零字节元素的
无意义 allocation；借用检查仍不得退化为运行时 pointer 比较。

## 后果

收益：

- SPEC-0035/0036 获得同一套 target layout、聚合调用、allocation、ZST 和 drop/free 边界；
- 不增加 workspace member 或 Rust runtime shim，`lang-frontend` 与 `lang-std` 保持 LLVM/allocator
  无关；
- 大型聚合可由目标 ABI 间接传递，但不会因类型大小产生隐式 Box 或 heap allocation；
- class/Box/container 均保持单一、静态可验证的 owner 表示，和现有 MoveOnly/Copyable facts
  一致。

代价与风险：

- 当前 ABI 只覆盖首个 AArch64 macOS target；新增 target 必须重新验证 data layout、system
  allocator alignment 与 aggregate calling convention；
- enum payload storage、ZST sentinel 和递归 drop glue 需要 adapter 提供专门测试，不能只检查
  LLVM verifier 通过；
- first-class aggregate 的最终机器 ABI 由 LLVM target backend 决定，跨版本/跨编译器稳定 ABI
  仍未建立；
- 没有 runtime object header，未来加入 `dyn`、RTTI、共享所有权或自定义 allocator 时需要新
  ADR 和显式迁移，不能原地假装已有 metadata。

## 关联

- 相关 Spec：SPEC-0035、SPEC-0036、SPEC-0037、SPEC-0038、SPEC-0039、SPEC-0042
- 相关 ADR：[ADR-0002](./0002-bootstrap-workspace-layout.md)、
  [ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0007](./0007-llvm-toolchain-and-first-target.md)
- 上游依据：[LLVM Language Reference：Data Layout 与参数属性](https://llvm.org/docs/LangRef.html)、
  [Apple malloc/free manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/free.3.html)、
  [POSIX free](https://pubs.opengroup.org/onlinepubs/9799919799/functions/free.html)
- 取代的 ADR：无
- 被以下 ADR 取代：无
