# 演进总纲之一：调研、基底能力内核与决断

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在裁决所有权/引用/dyn 演进中的设计分歧，或判断某能力属于“内核、糖还是库”时 · **唯一真源**：本页维护判据、基底能力清单与对[总纲](ownership-references-dyn-evolution.md) §12 各问题的决断建议；现行语义仍以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。它回答三个问题：
外部研究给了哪些可用结论（§1）；怎样判断一项能力该不该进编译器（§2、§3）；总纲留下的分歧如何裁决（§4）。
推进路线的详细计划见 [路线 L 借用内核](evolution-02-route-loan-kernel.md)、[路线 X 抽象打包](evolution-03-route-existential.md)、
[路线 C/U/S 能力、裸内存与挂起](evolution-04-route-capability-unsafe-suspend.md)。同一判据在其他领域的应用（决断 J1–J55）见 §8；总体分层与内核之间的接口见[之九](evolution-09-architecture.md)，尚未覆盖的领域见[之十](evolution-10-coverage-gaps.md)，其中失败模型、通用接口与编译单元依赖展开于[之十一](evolution-11-failure-interfaces-units.md)，标准库清单与平台适配见[之十二](evolution-12-stdlib-platform.md)；各步骤的实施先后与 v1 冻结前须裁决的事项见[之十三](evolution-13-implementation-roadmap.md)。

## 1. 调研结论

标注“已核”的条目在 2026-11-10 经联网检索确认了出处或现状（来源见 §6）；其余凭既有知识整理，引用前应复核。

### 1.1 核与糖：怎样算“基底能力”

| 来源 | 观点 | 对 Koven 的用处 |
|---|---|---|
| Landin, *The Next 700 Programming Languages*, 1966 | 语言 = 小的应用式内核 + 可替换的表面记法；“糖”一词即出于此 | 表面语法与语义内核应分两层评审 |
| Felleisen, *On the Expressive Power of Programming Languages*, 1991（已核） | 给出形式判据：若一个构造能被**局部的、保持其余程序不变的**翻译消去，它就是可表达的（糖）；需要全局重写才能模拟的构造才增加表达力 | §2 的判据 T1 直接取自这里 |
| Scheme 报告序言 | 语言不应靠堆叠特性来设计，而应消除那些使额外特性显得必要的弱点与限制 | 每个新特性提案先回答“是哪条限制逼出了它” |
| Steele, *Growing a Language*, 1998 | 设计“让用户长出语言”的机制；用户定义与内建应尽量平权 | 验收标准是库作者能否用公开机制重写内建 |
| GHC Core / Rust MIR / Swift SIL 的工程实践 | 表面语言很大，但都先脱糖到一个小的、带类型的中间内核，再在内核上做检查 | 所有权检查应作用在脱糖后的统一内核上，而不是按语法家族分别实现 |

最后一条与仓库现状直接相关：[所有权架构](../architecture/ownership.md)记录了大量按构造分别发布的计划与事实
（迭代、非空断言、nullable when、调用前缀、捕获根替换等各有专门产物，且 Single/Unit 两条路径并存）。
这正是“在表面语言上做检查”的形态，也是评估文档所说“语言简单、编译器复杂”的根源。

### 1.2 借用与生命周期

| 来源 | 观点 | 对 Koven 的用处 |
|---|---|---|
| Tofte & Talpin 1997；Cyclone（Grossman 等 2002） | 区域作为类型参数；Cyclone 证明了可行，也暴露了标注负担 | 反例：区域/生命周期一旦进类型就会沿泛型扩散 |
| Rust NLL（RFC 2094）与 Polonius（已核） | Polonius 把“生命周期”重述为“该引用可能来自哪些 loan 的集合”；检查变成 loan 存活性问题。Polonius Alpha 自 2026-08 起在 nightly 默认启用，目标是 2026 年内稳定；本次未查到稳定公告 | Koven 的内部模型可直接采用“origin = loan 集合”，且因无生命周期变量而不需约束求解。Rust 花了约八年才把这一表述推到接近稳定，主要成本在与既有生命周期系统的兼容，Koven 没有这一包袱 |
| Weiss 等, *Oxide*, 2019 | 用一个小演算刻画 Rust 借用的本质 | 路线 L 的参考模型应写成同样规模的小演算 |
| Osvald 等, OOPSLA 2016（已核） | 二等值：只能向下传、不能存储或返回的值，生命周期受栈约束；带 Coq 证明 | “不可逃逸”可以是简单、可证明的性质 |
| C# `ref struct` 与 ref-safe-context（已核） | 逃逸范围只有固定的三级（声明块、函数成员、调用方上下文），没有命名生命周期；`scoped` 参数不贡献到结果 | 证明工业语言可在无生命周期名字的前提下提供 `Span<T>`。但 C# 有 GC，不需处理“来源被修改或释放”，所以 Koven 不能只用作用域层级，必须跟踪 loan |
| Swift SE-0446 `~Escapable`、SE-0447 `Span`、SE-0467 `MutableSpan`（已核） | 不可逃逸类型已接受；但表达“结果依赖哪个参数”的生命周期依赖标注至 2026 年仍未定稿：2026-02 发布第三版 pitch，2026-08 的论坛讨论仍在使用下划线形式 `@_lifetime` | 关键教训：不可逃逸类型本身容易，**依赖声明语法才是难以稳定的部分**。Koven 已有 `from`，等于先解决了难的一半 |
| Hylo（原 Val）（已核） | subscript 不返回值而是“投影”一个值，授予调用方临时读写访问；任一时刻只能经由一个绑定修改 | 印证“访问器 + 二等引用”足以支撑泛型容器库 |
| Mojo origin（已核，官方手册） | origin 跟踪引用借自何处，并用于延长被引用值的生命周期；ASAP 析构在每个子表达式后运行；**通配 origin 的指针存活期间，其作用域内所有值的 ASAP 析构被禁用**，官方不鼓励使用 | 与 Koven 最近的同类；其 origin 仍是类型参数，Koven 不必跟进到那一步。通配 origin 的处理方式正是 D8/D10 所说的“裸指针屏障” |
| *Oxidizing OCaml*，ICFP 2024（已核） | 把 locality / uniqueness / affinity 做成**模式**（绑定的属性）而非类型；可完全推断、向后兼容 | Koven 的参数 mode 已经是模式；`borrow T` 作为类型与 `borrow` 作为模式应是同一内核对象的两种写法 |

### 1.3 析构时机

Swift 5.7 起采用词法生命周期并引入 **deinit barrier**：weak/unowned 引用、unsafe 指针与外部可见副作用会形成屏障，
优化器不得把释放提前越过它们；没有屏障时仍可缩短生命周期。同时 CoW 标准类型被特别赋予“优化生命周期”，
而可能带自定义 deinit 的 `~Copyable` 值采用“严格生命周期”（已核）。

这与 Koven 的双轨析构是同一设计的两种表述。Swift 的版本多出的正是总纲 §4.1 指出的两类屏障（weak、裸指针）。
Mojo 的 ASAP 也被记录到会让调试器看到已析构的变量（已核，社区来源）。

### 1.4 别名模型、存在类型与共享

| 来源 | 观点 | 对 Koven 的用处 |
|---|---|---|
| Stacked Borrows（POPL 2020）、Tree Borrows（PLDI 2025，已核） | unsafe 代码必须遵守的别名规则需要一个独立的操作语义模型；Tree Borrows 在 3 万个 crate 上比前者少拒绝 54% 的测试 | 开放 `unsafe` 之前必须先有别名合同；Koven 的合同可以远比 Rust 简单 |
| Rust `dyn`、Swift `any`（SE-0335）与隐式打开（SE-0352）、Go interface、Haskell existential | 存在类型 = 载荷 + 操作表；两门语言后来都要求显式标注；Swift 额外补了“existential 传给泛型” | dyn 的内核是 pack / open 两个操作；标注与否是表面问题 |
| Perceus（PLDI 2021）、*Counting Immutable Beans*（2019） | 所有权信息充分时，引用计数可大量消除与复用 | `Rc` 路径的长期优化余地 |
| Vale（已核，来源为作者网站） | 单一 owner + 代际引用的运行时检查；“Higher RAII”即必须显式消费的线性类型 | R3 句柄方案的先例；线性“必须消费”是 `DropPolicy` 的自然第三档 |
| Pony、Verona、Gordon 等 2012 | 用引用能力区分隔离/不可变/可变，保证并发安全 | 能力格是成熟思路；Koven 取其最小子集（Transferable / Shareable） |

## 2. 判据：四个测试

“最小作用量”在语言设计里的对应物是：**编译器只承担库无法承担的那部分**。每项提案依次过以下四个测试。

| 测试 | 问题 | 结论 |
|---|---|---|
| T1 可表达性 | 能否用现有内核经局部翻译得到（Felleisen 判据）？ | 能 → 糖或库，不进内核 |
| T2 不变量归属 | 它是否引入一条必须由编译器强制、不能交给库作者自律的不变量？ | 是 → 内核；否 → 库（必要时借助 `unsafe`） |
| T3 可逆性 | 以后放宽是否向后兼容？以后收紧是否破坏代码？ | 只能放宽的方向 → 现在取严；两个方向都可逆 → 推迟决定 |
| T4 表示承诺 | 是否影响 ABI、布局或跨阶段产物？ | 是 → 在第一个消费者出现前写 ADR |

推论（“先严后宽”）：省略规则、隐式转换、默认值一类设计，一律先交付显式形态，积累用例后再加糖。
加糖是兼容变更，撤糖不是。

## 3. 基底能力内核

按上述判据，编译器需要承担的只有八项（后经[之六](evolution-06-text-globals-extensions-metadata.md)检验补记第九项 KB9，它是已有能力）。代号 KB 仅在本系列文档使用。

| 代号 | 基底能力 | 强制的不变量 | 现状 | 缺口 |
|---|---|---|---|---|
| KB1 | 值与析构义务 | 每个需析构的 owner 在每条路径上恰好清理一次；move 后不可用 | 已有（双轨） | 析构策略应是类型属性 `DropPolicy`，由单一规划器消费。**缺一项基底操作（2026-11-14 补）**：“消费并解除析构”——在声明该类型的模块内，把一个带 `deinit` 的 owned 值拆成它的字段，且不运行 `deinit`。显式 `close(): Result`、作用域守卫的 `dismiss`、把资源转交给另一个 owner 都由它组合出来（之四 C1） |
| KB2 | Place 与 Loan | 重叠 place 上不得同时存在冲突访问 | 已有（含两阶段借用） | 多个按家族实现的 loan 需归一为一种 |
| KB3 | Origin 携带 | 持有 loan 的值存活期间，loan 的来源不得失效；持有者不得逃逸出来源的范围 | 部分（`from` 单来源、`borrow val`、borrowed closure 各自特判） | 统一为“不可逃逸值 = 值 + loan 集合” |
| KB4 | 结构能力推导 | 能力判定只由类型结构与叶子公理决定，用户不可伪造 | 已有 `Copyable`、`Transferable` | 引擎通用化；新增 `Escapable`、`Shareable`、`DropPolicy` |
| KB5 | 类型抽象 | 泛型实例与被擦除的载荷都保持能力与析构的精确性 | 单态化已有 | 存在类型的 pack / open |
| KB6 | 内部可变单元 | “共享借用即不可变”的唯一例外，且例外被类型标出 | 无 | 一个裸的内部可变标记类型；安全的 `Cell`、原子类型都是它之上的库（见[之五](evolution-05-data-structure-fitness.md) §4） |
| KB7 | 裸内存 | 安全代码的不变量不依赖 unsafe 代码以外的任何东西 | 仅编译器内建与 runtime | `RawPtr`、`extern`、布局、最小 intrinsic 集 |
| KB8 | 帧挂起 | 挂起点两侧的所有权与 loan 状态守恒 | 无 | v3；此前只需写死帧地址稳定 |
| KB9 | 编译期求值（增补，见 §8） | 封闭、无副作用、结果确定 | 已有（`const val` 封闭求值） | 全部使用点共用一个求值器；长期落在所有权图上 |

### 3.1 派生表：表面能力 = 基底的组合

| 表面能力 | 由什么组成 | 层 |
|---|---|---|
| `borrow val x = …` | 类型为 `borrow T` 的局部（KB3） | 糖 |
| borrowed / mutating closure | 匿名的、含借用字段的积类型（KB3）+ 单方法见证（KB5） | 糖 |
| move closure、匿名对象 | 匿名 owned 积类型 + 见证（KB5） | 糖 |
| `view class` | 含借用字段的积类型；`Escapable` 由 KB4 推导为假 | 关键字是声明式断言，见 §4 D12 |
| 迭代器、`for` | interface + 视图 | 库 + 糖 |
| `map.get(k)?.let { … }` | `borrow T` 的 nullable（KB3）+ 现有 nullable 糖 | 糖 |
| `a[i] = v`、`a[i].f += 1` | 返回 `inout T from this` 的访问器（KB2、KB3） | 糖 |
| `withLock {}`、`use {}` | 非逃逸闭包 + `inout` 参数 | 库 |
| `Guard<T>` | 视图 + `LexicalRequired` | 库 |
| `Box`、`Rc`、`Weak`、`Arc`、`Arena`、`List`、`Map`、`String` | KB7 + KB1 + KB4 的叶子公理 | 库 |
| `RefCell` 类、`Mutex`、原子类型 | KB6 + KB7 | 库 |
| `replace`、`swap` | KB7 的读写原语 | 库（当前为内建） |
| 不相交切分 `splitAt` | KB7 + 受信任的 `from` 签名 | 库 |
| `sealed interface` | 枚举 + 静态分发 | 糖 |
| `Shape` 作为值 | pack / open（KB5） | 内核 |
| `value class` 赋给 `Shape` 时装箱 | 先 `Box` 再 pack | 糖（隐式转换） |
| 省略 `from this` | 签名补全 | 糖 |
| 有限 NLL | 不是特性：是 KB2/KB3 分析精度的提升，接受的程序集单调增大 | 内核精度 |
| 作用域线程 | 非逃逸闭包 + `Shareable` + KB7 | 库 |
| 生成器、收尾型访问器、`suspend` 函数 | KB8 + 库 | 糖 / 库 |
| 静态存储、返回静态借用 | KB3 的 `Static` 来源 + KB9 | 内核取值（之六 J5） |
| `StringView`、字节视图 | 视图（KB3）+ KB7 | 库 |
| `Lazy`、作用域值 | KB6 + KB7；非逃逸闭包 | 库 |
| 扩展函数 | 接收者为首个参数的顶层函数 | 糖 |
| 注解、内建派生、生成器产物 | KB9（实参求值）+ 工具 | 元数据 / 工具 |
| 任务作用域、channel、`select`、actor、执行器 | 不可逃逸资源类型 + `Shareable` + KB7（+ KB8） | 库 |
| 向量类型 | KB7 的向量内在表示 | 库 |

### 3.2 由此得到的内核收缩清单

下列能力当前各有专门的编译器路径，按上表可在相应路线完成后降级为派生物（具体对象需逐项核对实现）：
闭包捕获规则、借用迭代 provider、范围 carrier、scoped Map 访问、callable view、`replace`/`swap` 内建、
封闭的 element-place 借用、各内建容器的析构 glue。这张清单的缩短速度是一项度量，但不是首要度量（2026-11-14 修订，评审 A1）：把编译器里的复杂度搬进 `unsafe` 库，不等于复杂度或可信计算基变小。
首要度量是：独立的库作者能否仅凭公开合同正确实现；抽象的组合是否成立；受信任声明的数量是否在增长；安全代码能否保持局部推理；优化前后语义是否一致。容器库化是验收手段，不是目的。

## 4. 决断

D1–D9 对应总纲 §12 的 Q1–Q9；D10–D13 是调研后新增的。倾向一栏相对总纲有变化的已标出。

| # | 问题 | 判据 | 决断建议 | 相对总纲 |
|---|---|---|---|---|
| D1 | `borrow T` 是否为类型、是否允许 `(borrow T)?` | T1：条件借用的控制流绑定无法由现有内核局部表达，且不能与 nullable 的既有糖组合；类型方案则可。T3：位置限制以后可放宽 | **承认为类型**，但首版只允许出现在参数 mode、局部、返回、视图字段及其 nullable 上；不得作泛型实参 | 不变，补位置限制 |
| D2 | `dyn` 关键字去留 | 内核只需独立的存在类型身份，与拼写无关；属表面问题，两个方向都可逆（Koven 尚未 1.0） | **推迟**。保持保留字（零成本），在启用 dyn 的那一版 Guide 再定；倾向仍是裸名字 | 由“退出保留字表”改为推迟 |
| D3 | 可否省略 `from this` | T3：加省略是兼容变更，撤销不是 | **首版必须显式**；省略规则作为后续糖 | 由“可省略”改为先严 |
| D4 | `Rc` 下的共享变异 | T1：`RefCell` 类、`Mutex`、原子类型都可由一个内部可变原语 + KB7 在库中表达；“class 实例动态排他检查”是第二套机制 | **内核只提供一个内部可变标记（KB6）**，`Cell`、带借用标志的单元、`Mutex`、原子类型均为库；不采用 class 级动态排他 | 明确化 |
| D5 | `sealed` 是否先行 | T1：可由枚举 + 静态分发局部翻译得到 | **是糖，不是内核**；与 dyn 解耦，任何时候都可独立交付，不构成路线依赖 | 由“先交付”改为解耦 |
| D6 | `value class` 成为 owned existential 是否隐式分配 | T1/T2：pack 只需接受“间接 owner”；隐式 `Box` 是可局部翻译的转换 | **内核的 pack 永不分配**；隐式装箱作为单独规定、可被工具列出的转换糖，建议与 dyn 同版启用 | 拆成内核与糖两层 |
| D7 | 用户何时获得 `unsafe` | 策略问题，不涉及内核形态 | **推迟**。KB7 先以标准库特权形态存在；开放时机取决于别名合同定稿与去特殊化进度 | 不变 |
| D8 | ASAP 是否对 `Weak`、`RawPtr` 让步 | T2：析构可观察性是编译器必须强制的不变量；Swift 的 deinit barrier 是同一结论 | **是**。`DropPolicy` 成为 KB4 的一项能力，见 D10 | 不变 |
| D9 | dyn 与视图的先后 | 两者分属 KB5 与 KB3，互不依赖 | **不设先后**，按路线并行推进，只约束依赖（§5） | 由排序改为依赖 |
| D10 | ASAP 的语义表述 | T2 | 把现行两条规则合并为一条：**析构不可观察的类型可在最后使用后析构，否则保持到词法作用域结束**。“可观察”的来源是封闭列表：声明 `deinit`、被 `Weak` 观察、派生过裸指针、字段递归。确定性的最早析构点继续保留（事实与黄金输出需要它）。**2026-11-14 修订**：最早析构点不再由规范强制，只是实现的许可；规范统一为“词法作用域结束时析构，不可观察时允许提前”一条规则，见[对比](drop-timing-comparison.md) | 新增 |
| D11 | 是否提供“必须显式消费”的线性类型 | T3：给 `DropPolicy` 预留第三档不影响现有代码 | **预留不实现**。线程句柄未 join 的处理（线程候选 H3）、事务一类需求出现时再启用 | 新增 |
| D12 | `view class` 关键字是否必要 | T1 说它可由“含借用字段的 `value class`”推导得到；但能力变化是 API 破坏性变更 | **保留关键字，作为受检查的声明式断言**（与 `value class` 断言内联值语义同理）：声明为视图而不含借用字段、或未声明却含借用字段，均报错 | 新增 |
| D13 | `borrow` 是模式还是类型 | OCaml 模式系统的经验：两者是同一对象的两种视角 | **内部只有一种表示**（KB3 的不可逃逸值）；参数位置继续写作 mode，类型位置写作 `borrow T`，两者等价 | 新增 |

### 4.1 对总纲的修订含义

- 总纲的 K0–K8 是一种交付排序；依 D9，更准确的结构是五条按基底能力划分的路线（§5），阶段只是路线上的截面。
- 总纲 §5.2 中“方法可省略 `from this`”依 D3 后移为糖。
- 总纲 §7.2 第 1 条“先交付 sealed”依 D5 改为独立的糖项目。
- 总纲 §4.1 的 `DropPolicy` 依 D10/D11 具体化为三档能力。

## 5. 推进路线

每条路线对应一组基底能力，路线内部严格有序，路线之间只有下表列出的依赖。

| 路线 | 基底 | 步骤 | 详细计划 |
|---|---|---|---|
| L 借用内核 | KB2、KB3 | L0 归一 → L1 流敏感 → L2 借用即类型 → L3 含借用字段的积类型 → L4 独占 origin → L5 泛型放开 | [路线 L](evolution-02-route-loan-kernel.md) |
| X 抽象打包 | KB5 | X0 callable 重建 → X1 存在类型身份 → X2 借用 existential → X3 owned existential → X4 糖 | [路线 X](evolution-03-route-existential.md) |
| C 能力与共享 | KB4、KB1、KB6 | C0 引擎通用化 → C1 `DropPolicy` → C2 内部可变标记 → C3 `Shareable` → C4 共享库 | [路线 C/U/S](evolution-04-route-capability-unsafe-suspend.md) |
| U 裸内存 | KB7 | U0 别名合同 → U1 特权原语 → U2 容器迁移 → U3 向用户开放 → U4 freestanding | 同上 |
| S 并发与挂起 | KB8 | S0 约束 → S1 线程合同 → S2 借用型并发 → S3 生成器 → S4 `suspend` 与异步 IO → S5 并行执行器 | [之七](evolution-07-concurrency-async-io-ffi.md) |

跨路线依赖（箭头表示“必须先完成”）：

```text
C0 ──> L3   Escapable 由通用能力引擎推导
C0 ──> X1   existential 携带能力位
L0 ──> X0   callable 重建建立在归一后的 loan 模型上
L3 ──> X0   borrowed closure 改写为匿名视图（X0 可先只做 move closure 与无捕获函数）
C1 ──> C4、U3   Weak 与用户裸指针之前必须有 DropPolicy
U0 ──> U1 ──> C4   共享库用 unsafe 实现
U1 ──> C2   内部可变标记只交付裸指针，安全封装在库中用 unsafe 实现
L4 ──> C4（部分）  条件变量的通用形态需要可移入移出的 guard；作用域形态不依赖
L4 ──> U2   容器迁移到库需要访问器返回独占 origin
C3 ──> S    结构化并发需要 Shareable
```

之六至之八增加的依赖（步骤代号见各页）：

```text
L3 ──> TX1          文本视图
C3、L3 ──> S2a      借用型分叉–合并
L1、IR3 ──> S3      生成器需要流敏感检查与图上的帧变换
S2b、S3、IO2 ──> S4 挂起函数与异步 IO 的合流点
U0 ──> FF0、VE1     外部边界合同；参数模式到后端属性
U1 ──> FF1 ──> IO2  标准库经 FFI 自行实现 IO
U1 ──> VE2          向量内在表示
细化层迁移完成 ──> IR1；IR1 与 L0 的参考检查器是同一项工作；IR2 ──> C1
J10 的一致性规则（之三 J37）──> X3
编译单元依赖无环（之十一 J45）──> J37
条件实现（之三 J38）──> U2   容器迁为库类型后须能在源码里写出条件判等
调试信息（之十 W11）──> S3   可调试性是 KB8 的验收条件（之七 J34）
IR2 ──> FD1 ──> FD2 ──> FD3   线程失败边界（之十一 J48）；FD3 另需 C4
U1、FF1 ──> PL2 ──> PL3、PL4   平台层迁为 Koven 源码后再扩展目标（之十二）
FF4 ──> BS2；FF3 ──> BS3   按部件自举与整体自举
```

没有箭头相连的步骤可以并行评审与实施。总纲的 K 阶段与路线步骤的对应：
K0 = L0 + C0；K1 = L1 + L2；K2 = X0–X4；K3 = L3；K4 = L4；K5 = C1–C4；K6 = U0–U3；K7 = S；K8 不变。

## 6. 本次核对过的来源

- Felleisen 1991：<https://www2.ccs.neu.edu/racket/pubs/scp91-felleisen.pdf>
- Swift SE-0446 接受公告：<https://forums.swift.org/t/accepted-with-modifications-se-0446-nonescapable-types/75504/1>；
  SE-0447：<https://forums.swift.org/t/accepted-se-0447-span-safe-access-to-contiguous-storage/75508>；
  SE-0467：<https://forums.swift.org/t/accepted-se-0467-mutablespan/78875>；
  生命周期依赖的实验状态：<https://forums.swift.org/t/experimental-support-for-lifetime-dependencies-in-swift-6-2-and-beyond/78638>
- Swift 词法生命周期与 deinit barrier：<https://forums.swift.org/t/a-roadmap-for-improving-swift-performance-predictability-arc-improvements-and-ownership-control/54206>、
  <https://forums.swift.org/t/what-is-the-current-status-of-lexical-lifetimes/76587>
- C# ref safety：<https://learn.microsoft.com/dotnet/csharp/language-reference/proposals/csharp-11.0/low-level-struct-improvements>、
  <https://github.com/dotnet/csharplang/blob/master/proposals/csharp-7.2/span-safety.md>
- Hylo：<https://docs.hylo-lang.org/language-tour/bindings>、<https://github.com/hylo-lang/hylo-lang.github.io/blob/main/index.md>
- *Oxidizing OCaml*：<https://icfp24.sigplan.org/details/icfp-2024-papers/19/Oxidizing-OCaml-with-Modal-Memory-Management>、
  <https://blog.janestreet.com/oxidizing-ocaml-locality/>
- Tree Borrows：<https://pldi25.sigplan.org/details/pldi-2025-papers/42/Tree-Borrows>
- 二等值：<https://bibtex.github.io/OOPSLA-2016-OsvaldEWAR.html>；Capturing Types：<https://abgruszecki.github.io/publication/capturing-types/capturing-types.pdf>
- Vale：<https://vale.dev/linear-aliasing-model>、<https://vale.dev/blog/higher-raii-7drl>
- Mojo（社区镜像，非官方手册）：<https://mojomiji.pages.dev/miji/advanced/lifetimes>

第二轮补核（同日）：

- Swift 生命周期依赖：<https://forums.swift.org/t/pitch-3-compile-time-lifetime-dependency-annotations/84968>、
  <https://forums.swift.org/t/lifetime-dependency-annotations-and-function-subtyping/89243>
- Polonius Alpha：<https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/>、
  <https://rust-lang.github.io/rust-project-goals/2026/roadmap-borrow-checker-within.html>
- Mojo 官方手册：<https://docs.modular.com/mojo/manual/values/lifetimes/>、<https://docs.modular.com/mojo/manual/lifecycle/death/>

仍未确认：Swift 生命周期依赖是否已取得 SE 编号并进入正式评审；Polonius Alpha 是否已进入 stable。两者都不影响本文结论。

## 7. 对仓库现状的核对结果

前三份文档中标为“需核对”的事项，于 2026-11-10 在 `a775568f` 上以只读方式核对（未运行测试）：

| 事项 | 核对结果 | 影响 |
|---|---|---|
| `Copyable` 与 `Transferable` 是否共用推导 | 不共用。Single 在 `type_checking/checker/copyability.rs`，Unit 在 `type_checking/compilation_unit/bodies/checker/copyability.rs` 与 `signatures/capabilities.rs`；两种能力各有 `*_with` / `nominal_*` 函数；资源分类另在 `type_checking/resource.rs` | 路线 C 的 C0 确有必要：现状是三种推导 × 两条管线 |
| 析构的两轨是否各自实现 | Single 的 `checker/drop_planner/resource.rs` 与 Unit 的 `compilation_unit/dataflow/drop_planner/resource.rs` 各有资源专用模块 | 两轨 × 两管线，见之四 §2 C1 的单轨化说明 |
| 后端是否对借用参数加只读/无别名属性 | `lang-codegen/src` 非测试代码中未发现 `noalias`、`readonly`、`nocapture`、`dereferenceable` | KB6 的后端约束目前无需改动，是对未来优化的前置限制 |
| `String` 的内部表示能否 `Shareable` | [ADR 0018](../adr/accepted/0018-string-owner-runtime-abi.md) 规定不可变、唯一 owner，明确否决隐式共享 buffer | 可以 `Shareable` |
| nullable 借用能否用指针 niche | [ADR 0033](../adr/accepted/0033-inline-nullable-representation.md) 已采用 tag + payload 并做 niche（`String` 用 bytes 指针） | 与 L2 的表示设想一致，沿用即可 |
| `view` 是否已是关键字 | 否。未来保留字为 `async await suspend actor spawn sealed dyn where yield macro reify` | `view` 需作为上下文关键字引入，须过文法冲突检查（见 [Parser 评审](parser-grammar-verification-review.md) §5） |
| 闭包现有表示 | [ADR 0009](../adr/accepted/0009-concrete-closure-internal-abi.md)：具体闭包为函数指针 + 内联环境；[ADR 0034](../adr/accepted/0034-refine-closures-and-callable-views.md)：Borrow 函数参数采用 callable view 而非单态化 | callable view 已经是“借用形态的存在类型”，X0 是推广而不是推倒重来 |
| 内核收缩清单中的家族 | [ADR 0023](../adr/accepted/0023-borrowed-sequential-iteration-provider.md) 的迭代 provider 是 compiler-bound；[ADR 0030](../adr/accepted/0030-range-carrier-source-facts.md) 的范围 carrier 由类型环境显式绑定为 View 身份；`ownership_checking/` 下有 iteration、map_iteration、range_producer、element_index、nullable_when、non_null_assertion 等独立模块 | 清单成立；这些都是“编译器内建的视图”，正是 L3 之后可由库表达的对象 |
| `deinit` 能否移出字段 | 不能。Guide 08 规定 `deinit` 中 `this` 为只读 Borrow；字段析构由编译器在 body 之后递归进行 | 发现一处内核缺口，见之五 §4 |

## 8. 判据在其他领域的应用（之六至之八）

2026-11-10 起，同一套判据被用于相邻领域。下表列出最初的十个领域（之六至之八）；其后的架构、失败模型、通用接口、编译单元与平台层见之九至之十三。决断编号 J1–J55 连续：

| 领域 | 结论一句话 | 内核影响 | 位置 |
|---|---|---|---|
| String / Str | 两个类型承载三个角色，不设 `Str` | 无 | [之六](evolution-06-text-globals-extensions-metadata.md) J1–J4 |
| 静态与全局 | 静态存储是 Origin 的一个取值；不开放全局 `var` | KB3 多一种来源；补记 KB9 | 之六 J5–J8 |
| 扩展函数 | 纯糖；与“追溯实现 interface”分开裁决 | 无（后者属 KB5） | 之六 J9–J10 |
| 元数据 | 语义用关键字、指示用注解；无运行时反射；生成分三级 | 无 | 之六 J11–J13 |
| 并发与异步 | 一套能力规则；无栈帧变换；Kotlin 式 `suspend`，无调用点标记、无 future 值；父子关系即作用域树；取消是值；可调试性是验收条件 | KB8 | [之七](evolution-07-concurrency-async-io-ffi.md) J14–J21、J34–J36、J39 |
| IO 与 FFI | 只承诺 C ABI；关闭双通道；平台差异在源集粒度 | KB7（已在路线 U） | 之七 J22–J26 |
| 中间表示 | 不要 HIR，增加一层“所有权图” | 无（实现载体） | [之八](evolution-08-compiler-ir-escape-simd-selfhost.md) J27–J30 |
| 逃逸分析 | 语义逃逸归类型系统，优化逃逸不可观察 | 无 | 之八 J31 |
| 向量化 | 先自动向量化，再特权向量表示 | U1 增一项表示原语 | 之八 J32 |
| 自举 | 是检验不是目标；按部件渐进 | 无 | 之八 J33 |
| interface 的终态 | 一份声明两种消费；多约束、条件实现要做，关联类型不做；一致性规则分三级 | KB5（条件实现是 U2 的前提） | [之三](evolution-03-route-existential.md) §6 J37–J38 |
| 失败模型 | 长期承诺“没有可捕获的异常”；线程是失败边界（目标方向，v1 不变）；`Result` 的人机工程五步 | 调用增加失败出口（J48 引入时） | [之十一](evolution-11-failure-interfaces-units.md) J40–J42、J48 |
| 通用接口 | 十个，命名靠拢 Kotlin，三处协议偏离 | 能力从“只能推导”扩为“也可声明” | 之十一 J43–J44 |
| 编译单元依赖 | 单元之间无环，单元之内不限；语言版本按单元声明 | 无 | 之十一 J45–J47 |
| 任务间的数据传递 | 返回值与 channel 为默认，共享可变排最后；并发概念收敛为五个 | 无 | 之七 J49–J50 |
| 标准库与平台 | 只经一个平台层接触系统；可移植接口取公分母；缺失的能力是编译错误 | 无 | [之十二](evolution-12-stdlib-platform.md) J51–J55 |

十项里新增的内核成分只有两处：KB8（原本就在清单上）与 U1 的向量表示。其余全部落在库、糖、工具或实现载体上，
这与“内核应当很小”的预期一致，也是这套判据没有被新领域推翻的一次检验。
