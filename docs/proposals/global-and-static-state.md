# 顶层状态、静态存储与全局生命周期（候选）

> **性质**：非规范候选设计 · **状态**：未启用 / 等待接纳面决定与分层评审 · **读取时机**：评审顶层 `val`/`var`、带状态 `object`、静态存储、惰性初始化或全局可变状态时 · **唯一真源**：本页维护候选分层、待决问题与分期；现行语义以 [Guide](../guide/README.md) 为准

本文不修改、取代或启用现行 Guide（v0.45），也不批准 Guide、Spec 或 ADR，不授权实现。
它回答三个问题：Koven 的范式定位是否需要“全是对象”；顶层/静态状态应如何分层；
各层的初始化、分配、访问与销毁合同以及启用时机。

## 1. 问题

- 前端与后端对顶层运行时 `val`/`var` 的态度不一致：Parser 与 compilation-unit 类型检查接受
  （[SPEC-0197](../archive/specs/0197-multifile-type-checking.md) 检查顶层 initializer），
  native object 发射则以 `UnsupportedSource` 拒绝，测试断言为
  “runtime globals still require an explicit runtime contract”。用户写出的源码能过类型检查，
  却在 Phase 4 失败。
- [v0.44 提案](v0.44-language-completion.md)已记录：顶层 runtime `val`/`var` 的初始化/析构
  仍未闭合；“仅允许 `const val`”的接纳面收窄另列待决，需定接纳边界与结构化诊断、保留 `const val`。
- `object` 与 companion 在 v1 只是无状态关联命名空间；“运行时状态、惰性初始化与共享可变
  singleton 统一延后到 v2”，但 v2 的目标形态尚无候选。

## 2. 现行事实

| 主题 | 现行规则 | 来源 |
|---|---|---|
| 顶层函数 | 一等声明；`main` 必须是顶层、非泛型、非 member 函数；可形成 overload set、可 import | [程序入口](../guide/13-program-runtime-standard-library.md#conventional-main)、[名称](../guide/02-names-files-packages.md) |
| `Any` | 类型层面的顶，不是运行时反射基类 | [类型](../guide/03-types-generics.md) |
| 类继承 | 无实现继承；只有 interface、`super<X>` 消歧与窄化 `by field` 委托 | [Class Family](../guide/08-class-family-members.md) |
| 分发 | v1 无 `dyn`，interface 单态化静态分发 | [一致性](../guide/15-conformance-and-staging.md#动态分发与匿名对象边界) |
| `const val` | 顶层 / `object` / companion；封闭类型；无运行时地址、owner、init guard 或 drop；每次 use 重新物化 | [关联常量](../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| `object` / companion | body 只允许 `const val` 与函数；不分配 singleton、不生成 init guard、无退出析构 | [类型](../guide/03-types-generics.md)、[Companion](../guide/08-class-family-members.md#companion-与关联成员) |
| closure capture | 顶层函数/常量和具名 object 是全局身份，不进入环境 | [Lambda 与 Closure](../guide/07-calls-lambdas-closures.md) |
| 析构分类 | 资源类型（声明 `deinit` 或递归含资源字段）与纯内存类型分轨 | [所有权](../guide/10-ownership-borrowing-drop.md) |
| 线程能力 | v1 只有 `Transferable`；`Shareable` 推迟到 v2；非原子 `Rc` 永不 Transferable | [所有权](../guide/10-ownership-borrowing-drop.md)、[线程候选](thread-transfer.md) |
| String 静态字节 | 字面量可引用随进程存活、不 `free` 的静态字节，但值仍遵守普通 owner 生命周期 | [String 分层](../guide/13-program-runtime-standard-library.md#静态字符串类型的延后边界) |

## 3. 范式定位

Koven 不采用 JVM / Smalltalk 式“一切皆对象”，而是 **Kotlin 语法外壳 + 数据与函数 + 静态多态**：
成员函数是带 receiver mode（Value / Borrow / Inout）的函数，`.` 调用是名称查找与语法糖，
不意味着统一对象头、vtable 或对象身份。

| 维度 | 一切皆对象（JVM 等） | Koven 现行方向 |
|---|---|---|
| 运行时表示 | 统一引用、对象头、GC | 按单态类型精确布局；`value class` 无身份内联 |
| 身份与别名 | 默认共享引用 | 单一 owner；别名只经显式借用 |
| 与所有权的关系 | 共享身份和单一 owner 冲突，通常依赖 GC | 借用检查可在函数内局部推理 |
| 多态 | 默认动态分发 | 默认静态分发；`dyn` 见 [v2 候选](v2-interface-values-and-dynamic-dispatch.md) |
| 代价 | — | 异构集合 v1 依赖 enum；无反射；框架式 DI 需替代方案 |

顶层 `fun` 不需要 Kotlin/JVM 的 `FileKt` 外壳：那是 JVM 以 class 为字节码单位的约束；
Koven 经 LLVM 发射，顶层函数就是按 package 修饰的符号。本提案**确认保持该定位**，
并要求全局状态设计不得反向引入隐藏对象身份。

## 4. 跨语言调研

全局状态可拆为六个维度：存储位置、初始化时机、初始化顺序、线程安全、可变性/别名、销毁。

| 语言 | 形式 | 初始化 | 顺序/循环 | 可变与线程 | 销毁 |
|---|---|---|---|---|---|
| C / C++ | static 存储期（.data/.bss/.rodata） | 常量初始化 + 动态初始化；函数内 static 惰性，C++11 起线程安全 | 跨翻译单元顺序未定义（init order fiasco） | 任意可变，竞争为 UB | `atexit` 逆序析构，存在 destruction order fiasco |
| Java / Kotlin JVM | class static 字段；Kotlin 顶层进 `XxxKt`，`object` 为 `INSTANCE` | 首次主动使用时 `<clinit>`，JVM 保证一次且线程安全 | 循环时可观察默认值 | 任意可变 | 不析构，依赖 GC / shutdown hook |
| Go | 包级 `var` + `init()` | main 前急切执行；包内按引用拓扑排序，包间按 import | 编译器拒绝初始化循环 | 任意可变，依赖 race detector | 不析构 |
| Rust | `const`（内联无地址）、`static`（固定地址，`'static`） | `static` 只允许编译期求值；惰性用 `OnceLock` / `LazyLock` | 编译期解决 | `static mut` 需 unsafe（2024 edition 拒绝对其取引用）；共享可变须 `Sync` 内部可变类型 | static 永不 drop；`thread_local!` 线程退出时析构 |
| Swift | 全局变量与 static 属性 | 惰性，`swift_once` 线程安全 | 访问驱动 | Swift 6 要求可变全局隔离到 global actor 或 `nonisolated(unsafe)` | 不析构 |
| Zig | 容器级 `var` | 初值必须 comptime 已知 | 无运行时顺序 | 可变 | 不析构 |

趋势：较新的语言回避“main 前运行时急切初始化”（顺序问题）和“退出时析构”（析构顺序、
仍在运行的线程、abort 路径），并把共享可变性收进带线程能力约束的类型。

## 5. 设计约束

由现行原则推出，后续各层须逐条满足：

- **C1 修改可见**：静默修改无法由类型系统发现，这正是调用处 `&` 的依据。安全代码中可从任意
  函数写入的全局 `var` 与此直接冲突，也使 `inout` 独占性无法跨调用证明。
- **C2 确定性析构**：全局 owner 没有词法作用域。若允许资源类型进入静态存储，就必须回答
  退出顺序、线程仍在运行、abort 时是否执行等问题；本提案选择不进入。
- **C3 副作用显式**：不在 `main` 之前隐式执行用户代码；首次访问不得隐式触发 IO 或 abort。
- **C4 无隐藏身份**：静态存储不得重新引入可观察的 `Type.Companion`、隐藏 singleton 或对象头。
- **C5 能力单一来源**：静态值的线程可见性必须由编译器能力（v1 `Transferable`，v2 `Shareable`）
  证明，不由“只读”约定或内部 ABI 推断。
- **C6 阶段闭合**：每层都须给出 Phase 2 接纳/诊断、Phase 3 loan/drop 事实与 Phase 4 存储/符号合同，
  缺一不得进入下游。

## 6. 候选分层

| 层 | 源码形态 | 语义摘要 | 定位 |
|---|---|---|---|
| G0 常量 | `const val` | 现行规则不变 | 已启用 |
| G1 接纳面收窄 | 顶层与 object/companion 只接受 `const val`、`fun` 与类型声明 | 顶层运行时 `val`/`var` 由 Phase 2 结构化拒绝 | v1 近期 |
| G2 静态不可变 | 顶层或 `object` 中 `val`，initializer 可静态构造 | 固定地址只读存储，无 init guard，永不 drop，只能 Borrow / Copyable 读取 | v1 后期，条件触发 |
| G3 显式惰性 | `val CONFIG = Lazy { loadConfig() }`（std 类型） | 首次访问执行一次；重入 abort | v2，依赖 Shareable |
| G4 共享可变 | `val HITS = AtomicInt(0)`、`val CACHE = Mutex(MutableMap())` | 外层仍是不可变静态；修改经 Shareable 类型的方法暴露 | v2，依赖 Shareable |
| G5 线程局部 | 形态待定（如 `threadLocal val`） | 每线程惰性；线程退出时析构 | v2 之后 |

### 6.1 G1：v1 接纳面收窄

候选规则：

- 顶层、具名 `object` body 与 companion 中，值声明只接受 `const val`。顶层运行时 `val` / `var`
  形成新的 Phase 2 诊断（如 `unsupported runtime global`），主 Span 覆盖引导关键字 `val` / `var`；
  诊断码由后继 Spec 分配，本提案不预占编号。
- Parser 不变：仍产生 `Item::Variable` 并保持现有 Span 与恢复；拒绝只在 Phase 2 发生，
  不改报 Phase 1 语法错误。
- 诊断帮助建议两条迁移路径：可常量化时改写为 `const val`；否则改为在 `main` 中构造并以参数传递。
- 拒绝后的声明仍按现有 initializer 检查发布 typed facts 以支持恢复与后续诊断，但不得进入
  ownership 与 native 产物（待决 Q1）。
- native 端现有 `UnsupportedSource` 拒绝保留为防御性断言，其测试改为验证 Phase 2 已先行拒绝。

被比较的替代方案：

| 方案 | 内容 | 结论 |
|---|---|---|
| A 收窄（推荐） | 如上 | 闭合前后端不一致；成本低；不影响 G2 以后放开 |
| B 急切初始化 | 保持接受，main 前按依赖拓扑序执行 initializer | 违反 C3；需定义跨文件顺序、abort 与析构，等价于 Go `init` 全套问题 |
| C 隐式惰性 | 保持接受，首次访问初始化（Swift/JVM 式） | 违反 C3；初始化失败点不可预测；需 once 原语，而线程共享规则在 v2 |

### 6.2 G2：静态不可变存储

用途：查找表、静态配置、较大的只读结构，避免 `const val` 每次 use 重新物化的成本。

- **initializer**：扩展 const 求值器的封闭子集即可构造的值，候选包括现有 const 类型、
  字段全部可静态构造的 `value class`、String 静态字节，以及（待决 Q3）元素可静态构造的固定
  长度 Array。不得包含 call、lambda、Box/Rc/容器分配或任何运行时副作用。
- **类型约束**：必须是纯内存类型（无 `deinit`、不递归含资源），必须 `Transferable`，且不含
  内部可变或非原子共享（因此拒绝 `Rc`）。v2 起统一改为要求 `Shareable`。
- **存储与身份**：每个声明一个固定地址的只读对象，按 package 修饰符号；跨 compilation unit
  是否导出稳定符号由后继 ADR 决定。无 init guard、无运行时初始化代码。
- **访问**：静态 place 只能被 Borrow，或在 `Copyable` 时复制读取；禁止 move out、禁止 `&`
  inout、禁止赋值。`useTable(TABLE)` 按参数默认 Borrow 契约成立。
- **loan 图**：静态 place 是全程存活的 root；从中取得的借用可以从任意函数返回而不依附参数
  （类似 `'static`）。Phase 3 需新增该 root 类别，现有普通借用返回规则只作调用方来源推理。
- **closure / 线程**：沿用“全局身份不进入环境”；`move` closure 与线程 closure 引用静态值不构成
  捕获，可用性由类型约束保证。
- **销毁**：永不 drop；纯内存约束保证跳过析构不可观察（静态段本身不需要 `free`）。
- **与 `const val` 的选择**：`const val` 无地址、每次物化；G2 有唯一地址、借用访问。两者不得
  互相隐式转换。

### 6.3 G3：显式惰性初始化（v2）

- 以 std 类型 `Lazy<T>`（或 `OnceCell<T>`）提供，不新增 `lazy` 关键字或属性委托
  （Guide 已移除自定义 getter/setter 与 delegate）。
- 构造本身满足 G2 静态可构造条件；初始化 closure 为 `move () -> T`，`T` 须 `Shareable`。
- 首次访问在调用线程执行初始化，其他线程阻塞等待；同线程重入 abort。
- 初始化中 abort 即进程 abort；v1/v2 无异常展开，因此不定义 poison 状态。
- 已初始化值永不 drop，类型约束同 G2（纯内存）；需要可观察清理的资源应由 `main` 持有。

### 6.4 G4：共享可变状态（v2）

- 不开放全局 `var`；共享可变只经 `val` 持有的 Shareable 内部可变类型（`Atomic*`、`Mutex<T>`、
  后续 `RwLock<T>`）。修改通过方法调用表达，满足 C1 的“修改可见”。
- `Mutex<T>` 的 guard 是资源类型，只能存在于局部作用域，不能放入静态存储。
- 这些类型的设计属于 v2 Shareable 与线程共享原语，不在本提案内定义。

### 6.5 带状态 `object` 与 companion

- `object` 只是命名空间化的静态声明：v2 允许其 body 中的 `val` 字段时，逐字段套用 G2/G3/G4
  规则，不引入整体初始化块、整体惰性或 `INSTANCE` 身份。
- companion 保持“关联命名空间”；若放开字段，同样按上述规则，且仍不产生 `Type.Companion` 值。

### 6.6 明确非目标

- 安全代码中的自由全局 `var`（含 Rust 式 `static mut`）。底层逃生口若需要，只能由未来 unsafe/FFI
  设计单独定义。
- Go 式 `init()` 或任何 main 前执行的用户代码。
- 全局或静态值的退出时析构。
- 隐式惰性的全局变量。

## 7. 生命周期总表

| 层 | 存储 | 初始化 | 顺序/循环 | 线程 | 访问 | 销毁 | 失败 |
|---|---|---|---|---|---|---|---|
| G0 | 无（每次 use 物化） | 编译期 | 复用 const 依赖图，循环为诊断 | 不涉及 | 值 | 物化值按普通 owner | 编译期诊断 |
| G2 | 只读静态段 | 编译期 | 同 G0 | Transferable（v2 起 Shareable） | Borrow / Copyable 读取 | 永不 | 编译期诊断 |
| G3 | 静态段 + once 状态 | 首次访问 | 访问驱动，重入 abort | Shareable | Borrow | 永不 | 进程 abort |
| G4 | 静态段 | 编译期构造外壳 | 同 G2 | Shareable | 方法调用 | 永不 | 依类型合同 |
| G5 | 线程局部 | 每线程首次访问 | 访问驱动 | 不跨线程 | Borrow | 线程退出时析构 | 待定 |

## 8. 待决问题

| ID | 问题 | 倾向 |
|---|---|---|
| Q1 | G1 被拒声明是否继续发布 typed facts | 继续发布以支持恢复，并打上不可下游的标记 |
| Q2 | G1 是否同时拒绝 `object` / companion 中的普通 `val`/`var` | 现行规则已拒绝；只需统一诊断措辞 |
| Q3 | G2 是否包含固定长度 Array | 需要新的“静态 buffer、不 free”ABI 标志，类比 ADR-0018，宜单独 ADR |
| Q4 | G2 跨 compilation unit 的符号导出与 ABI | 首片限定同一 compilation unit |
| Q5 | G2 能否作为借用结果的来源跨函数返回 | 可以，但 Phase 3 静态 root 须先有独立 Spec |
| Q6 | G5 中主线程退出时线程局部值是否析构 | 待线程合同（[M6](thread-transfer.md) H5）闭合后决定 |
| Q7 | Float/Double 进入 const 与 G2 类型集合的时机 | 跟随 const 后继扩展，不在本提案决定 |

## 9. 分期与启用路径

| 步骤 | 内容 | 前置 | 建议时机 |
|---|---|---|---|
| S1 | G1 接纳面收窄 + 结构化诊断 | 用户决定方案 A；统计既有 fixture、示例与 tutorial 中顶层运行时 `val`/`var` 的影响面 | 下一 Guide 小版本（v0.45 批次） |
| S2 | G2 静态不可变 | const 求值器扩展到 value class；Phase 3 静态 root；代表性应用确有需求 | v1 后期，条件触发；无需求可推迟到 v2 |
| S3 | G3 / G4 | v2 Shareable 与共享原语、once 原语 | v2，与 Shareable 同期 |
| S4 | 带状态 `object` / companion 字段 | S2 或 S3 | 随 S2/S3 |
| S5 | G5 线程局部 | M6 线程合同与 v2 共享模型 | v2 之后 |

每一步都按 [Proposals 收口流程](README.md#从候选-guide-到可实施-spec) 形成新 Guide 版本并由用户明确启用；
本提案的分层顺序不构成实施授权。

## 10. S1 验收草案

- Phase 1：顶层 `val` / `var` 的 AST、Span 与恢复保持不变（回归现有 parser fixture）。
- Phase 2：single 与 compilation-unit 两条路径对顶层运行时 `val` / `var` 均报新诊断，主 Span
  精确覆盖引导关键字；`const val`、object/companion 常量不受影响；同文件多个违规各报一次。
- Phase 3/4：被拒声明不产生 ownership facts 与 native 产物；原
  “runtime globals still require an explicit runtime contract” 测试改为断言 Phase 2 已拒绝，
  native 拒绝分支保留为防御断言。
- 文档：Guide 02/05/15 同步接纳面，15 页“v0.44 启用边界”中的待决项转为已决；
  `python3 scripts/check_docs.py` 与 `git diff --check` 通过。
