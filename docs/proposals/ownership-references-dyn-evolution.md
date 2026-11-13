# 所有权、可存储引用与 dyn 的中后期演进（候选总纲）

> **性质**：非规范候选设计（跨版本总纲） · **状态**：未启用 · **读取时机**：仅在规划 v1.x–v4 的所有权、引用、动态分发、并发与系统编程能力时 · **唯一真源**：本页只维护方向、阶段划分与待决问题；现行语义仍以 [Guide v0.47](../guide/README.md) 为准，各阶段细则由后续独立 proposal / Guide / ADR / Spec 承接

本文不修改、取代或启用任何 Guide 规则，不批准 Spec 或 ADR，不授权实现。它按“不受现行已定规则
束缚”的要求给出设计判断，因此其中若干建议与 v0.47 的现行决定相反，已在 [§12](#12-需要用户决定的问题) 逐条标出。
输入材料：仓库内《Koven 所有权与借用生命周期设计评估》《Growing a Language》解读、
[Guide 10](../guide/10-ownership-borrowing-drop.md)、[v2 interface 候选](v2-interface-values-and-dynamic-dispatch.md)、
[M2B 候选](general-borrow-access-results.md)、[unsafe/FFI 候选](unsafe-and-ffi-boundary.md)、[线程候选](thread-transfer.md)。

后续细化：判据、基底能力与对 §12 的决断见[之一](evolution-01-kernel-and-decisions.md)（其中 Q2、Q3、Q5、Q6、Q9 的倾向已修订，
以该页为准）；实施计划见[路线 L](evolution-02-route-loan-kernel.md)、[路线 X](evolution-03-route-existential.md)、
[路线 C/U/S](evolution-04-route-capability-unsafe-suspend.md)。本页的 K0–K8 保留为一种交付截面，不再是主要组织方式。
数据结构检验见[之五](evolution-05-data-structure-fitness.md)；文本、全局状态、扩展函数与元数据见[之六](evolution-06-text-globals-extensions-metadata.md)；
并发、异步、IO 与 FFI 见[之七](evolution-07-concurrency-async-io-ffi.md)；中间表示、逃逸分析、向量化与自举条件（取代本页 K8 的简述）见[之八](evolution-08-compiler-ir-escape-simd-selfhost.md)。
总体架构（分层、内核之间的接口、扩展点）见[之九](evolution-09-architecture.md)；尚未覆盖的领域见[之十](evolution-10-coverage-gaps.md)与[之十一](evolution-11-failure-interfaces-units.md)；标准库与平台见[之十二](evolution-12-stdlib-platform.md)；实施先后见[之十三](evolution-13-implementation-roadmap.md)。

## 1. 结论摘要

1. **永不引入生命周期参数。** `from` 子句是 Koven 唯一的、也是最终的“生命周期语法”。Rust 用一种机制
   （`&'a T`）同时解决三类需求；Koven 应当用三种成本递增的机制分别解决（[§3](#3-三类引用需求三种机制)）。
2. **把“不可逃逸”做成第四种结构推导能力 `Escapable`**，与 `Copyable` / `Transferable` / 未来 `Shareable`
   同构。现有的 borrowed closure（L0137）与 `borrow val`（L0163）其实已经是两个各自特判的不可逃逸值；
   统一后，“可存储引用”就是 `view class`——可以把借用存进字段、但自身不可逃逸的类型（[§5](#5-可存储引用escapable-能力与-view-class)）。
3. **长期引用不走静态证明，走“所有权句柄”**：`Rc`/`Weak`、`Arc`、Arena + 代际句柄，运行时检查、成本显式（[§6](#6-长期关系所有权句柄而不是引用)）。
4. **dyn 维持裸 interface 名作为表面语法**（同意现有候选），但补四条：先交付 `sealed` 封闭层级、
   existential 自满足其 interface、函数类型与 existential 共用 ABI、能力经 interface 继承携带（[§7](#7-dyn动态分发)）。
5. **Kotlin 式 lambda 作用域访问优先于新引用能力**：`withLock {}`、`use {}`、`withValue {}` 在今天的规则下
   就能表达大量 Rust 需要 guard + 生命周期才能表达的 API。
6. **系统编程靠三层安全阶梯**：静态安全 → 运行时检查 → `unsafe`；不靠把借用检查器做到 Rust 的通用度（[§9](#9-系统编程层)）。
7. **顺序**：K0 内核闭环 → K1 借用流敏感化 → K2 dyn → K3 只读视图 → K4 独占视图 → K5 共享与并发 →
   K6 unsafe/FFI 与标准库去特殊化 → K7 协程 → K8 自举（[§10](#10-分阶段计划)）。
8. **每一阶段的验收不是“新增了什么”，而是“哪些编译器特判被库代码替换掉了”**（Steele 的 library-first 检验）。

## 2. 设计公理

以下六条用于裁决后续每一项具体提案；与之冲突的提案默认拒绝。

| # | 公理 | 推论 |
|---|---|---|
| A1 | 应用代码零生命周期标注 | 写业务代码的人永远不需要理解 origin；只有写视图类型的库作者才写 `from` |
| A2 | 成本阶梯单调 | 每升一层表达力，用户付出的概念数与运行时成本都必须可见地增加，不能悄悄发生 |
| A3 | 一个机制长大，不是多个机制并列 | 新能力优先表达为已有机制（能力推导、`from`、参数 mode）的推广 |
| A4 | 拒绝必须可预测 | 用户能用一句规则解释“为什么不行”，不能依赖编译器内部支持矩阵（L0164 类诊断应随阶段收敛到零） |
| A5 | 语义内核封闭，库抽象开放 | 所有权不变量不可被库或未来的元编程绕过；`unsafe` 是唯一出口且必须显式 |
| A6 | 编译器复杂度增速低于特性增速 | 每个阶段必须附带“退役清单”：哪些内建家族改由公开语言机制实现 |

## 3. 三类引用需求，三种机制

“可存储引用”不是一个需求，而是三个被 Rust 合并处理的需求。分开后每个都简单得多：

| 需求 | 典型场景 | Rust 的手段 | Koven 候选手段 | 检查方式 | 用户新增概念 |
|---|---|---|---|---|---|
| R1 临时访问 | 传参、读字段、`list[i].x = 1` | `&T` / `&mut T` | 参数 mode、调用期 loan、`borrow val`（已有） | 静态 | 无 |
| R2 视图抽象 | slice、迭代器、字符串视图、guard、解析器游标 | `struct S<'a>` | `view class` + `from`（K3/K4） | 静态，origin 集合 | `view class`、`from` |
| R3 长期关系 | 图、回指父节点、缓存、观察者、跨任务共享 | `&'a` 贯穿整个程序，或 `Rc`/索引 | `Rc`/`Weak`/`Arc`、Arena 句柄（K5） | 运行时 | 句柄类型本身 |

关键取舍：**R3 放弃静态证明**。理由：

- R3 的静态方案必然要求生命周期在类型间传播（字段 → 类型参数 → 容器 → 接口），这正是 Rust 学习曲线的主体，与 A1 直接冲突；
- 工业实践表明 R3 用索引/句柄是可行甚至更优的：rustc 自身大量使用 arena + 索引与 interning，Zig 编译器与 ECS
  架构整体采用面向数据的句柄设计。这也是 K8 自举时 Koven 编译器自己会采用的写法；
- 二等引用的研究（Osvald 等 2016；Hylo/Val 的 mutable value semantics，Racordon 等 2022）与工业落地
  （C# `ref struct`/`Span<T>`、Swift `~Escapable`/`Span`、Mojo origin）都指向同一结论：
  “不可逃逸的引用”覆盖了绝大多数零拷贝需求，而且不需要生命周期变量。

## 4. 所有权内核的修订建议

### 4.1 单一 Drop 规划器 + `DropPolicy`

采纳评估文档的建议，并补一个来自 Swift 的教训。Swift 早期采用“最后使用即释放”的优化生命周期，后来因
weak 引用与 unsafe 指针上的意外行为，改为默认词法生命周期并引入 deinit barrier。Koven 的纯内存 ASAP
在今天是安全的，但以下两项能力一旦引入就会复现同样的问题：

- `Weak<T>`：强引用绑定未再使用即被 ASAP 释放，随后 `upgrade()` 意外失败；
- `RawPtr<T>`：指针仍在使用，但其来源 owner 已无“合法使用”而被提前释放。

因此 `DropPolicy` 应是类型的结构推导属性，而不是“有无 `deinit`”的二分：

```text
DropPolicy = EarlyEligible | LexicalRequired
LexicalRequired 的来源：声明 deinit；字段递归含 LexicalRequired；
                        内建标记（未来的 Rc-with-Weak 观察点、unsafe 块内派生过 RawPtr 的 owner）
```

具体触发集合留待 K5/K6 的 proposal 决定；K0 只需保证规划器以策略为参数，而不是两套算法。

### 4.2 统一 Lifetime IR：origin 是集合，不是名字

内部模型建议固定为（对外不暴露）：

```text
Value   = Owned | NonEscaping(origins: Set<Loan>)
Loan    = (place, Shared | Exclusive, 创建点)
规则    ：某 Loan 存活 ⇔ 存在一个 origins 含它的存活值
冲突    ：沿用 Guide 10 的 place 重叠矩阵，不新增
```

这是 Polonius 式“借用 = loan 集合”的表述，但因为 Koven 没有生命周期变量、没有引用子类型与型变，
只需要值级 liveness 数据流，不需要区域推断或约束求解。`from a, b` 就是 origin 并集；`borrow val`、
borrowed closure、未来的 `view class` 实例都只是 `NonEscaping` 值的不同表面形态。

编译器应持续可验证的五条不变量沿用评估文档 §10.2，不在此复制。

### 4.3 K1 的三项流敏感化增量

三项都不新增类型系统概念，只放宽现有受限规则：

1. **有限 NLL**：`NonEscaping` 局部值在最后一次使用后结束其 loan（按 CFG liveness，不按词法块）。
   对 Resource 类型的 owner 仍保持词法析构，两者不矛盾：loan 结束 ≠ owner 析构。
2. **多来源 `from a, b`**：返回值的 origin 为所列来源的并集，调用方保守地同时冻结全部来源。
3. **可空借用结果**：见 [§12 Q1](#12-需要用户决定的问题)。若把 `borrow T` 视为（不可逃逸的）类型，
   `(borrow T)?` 就是普通 nullable，Kotlin 惯用的 `map.get(k)?.let { … }`、`?:`、smart cast 全部自然成立，
   不需要为“条件借用”再设计一套控制流绑定。

```kotlin
// 候选伪代码，非现行语法
fun longer(a: String, b: String): borrow String from a, b =
    if (a.length >= b.length) a else b

fun find(key: K): borrow V? from this
```

### 4.4 lambda 作用域访问是一等设计手段

在引入任何新引用能力之前，标准库应先穷尽这一 Kotlin 惯用形态，它只依赖现行的非逃逸闭包与参数 mode：

```kotlin
// 候选伪代码
mutex.withLock { inout state -> state.count += 1 }
file.use { f -> f.readLine() }
map.withValue(key) { inout v -> v.add(x) }
```

指导规则：**能用作用域 lambda 表达的 API 不提供返回视图的版本**，直到出现作用域形态确实表达不了的组合
（典型是需要同时持有两个访问、或需要在循环间保留游标）。这直接压低了 K3/K4 的需求面。

## 5. 可存储引用：`Escapable` 能力与 `view class`

### 5.1 能力规则

`Escapable` 与 `Copyable`、`Transferable` 一样由编译器结构推导，用户不可手动实现或否定：

- 现行全部类型满足 `Escapable`；
- `borrow T`、`inout T` 作为类型不满足；含捕获的 borrowed / mutating closure 不满足（即现行 L0137 的重述）；
- `view class` 恒不满足；其字段可以是 `borrow` / `inout` 字段或其他不可逃逸类型；
- 普通 `class`、`value class`、`enum class` 的字段必须 `Escapable`（违反即现行 L0163 的推广）；
- 泛型参数默认带隐式上界 `T : Escapable`，因此现有泛型代码与容器语义完全不变。

不可逃逸值**可以**：绑定到局部、作为实参传递、作为带 `from` 的返回值、存入 `view class` 字段、被非逃逸闭包捕获。
**不可以**：存入普通类字段 / 全局 / 普通容器、被 move closure 捕获、成为 owned existential 的 payload、
跨越非结构化的线程或任务边界。

### 5.2 `view class`

```kotlin
// 候选伪代码，非现行语法
view class Slice<T>(borrow val source: Array<T>, val start: Int, val length: Int) {
    fun get(index: Int): borrow T from this = source[start + index]
}

fun <T> Array<T>.slice(start: Int, length: Int): Slice<T> from this =
    Slice(this, start, length)

fun sum(xs: Slice<Int>): Int { … }          // 应用代码：没有任何标注
val total = sum(buffer.slice(0, 16))
```

规则要点：

- 构造时，实例的 origin 集合 = 所有 `borrow`/`inout` 字段实参的 loan ∪ 不可逃逸字段实参的 origin；
- 返回不可逃逸类型的函数必须写 `from`（方法是否可省略为 `from this`，见 §12 Q3）；
- 只含 shared 借用的 `view class` 当全部字段 `Copyable` 时满足 `Copyable`（共享借用可自由复制）；
- 含 `inout` 字段的为独占视图，恒 `MoveOnly`，放在 K4；
- `view class` 可声明 `deinit`（guard 场景），此时按 `LexicalRequired` 处理；
- 一个实例只有一个 origin 集合。**不区分**“字段 x 来自 a、字段 y 来自 b”——这是相对 Rust 的主动损失，
  换来的是类型上没有任何生命周期参数。

### 5.3 相对 Rust 主动放弃的表达力

| 放弃的能力 | 后果 | 替代 |
|---|---|---|
| 一个值内区分多个生命周期 | 多来源视图更保守，来源被一并冻结 | 拆成两个视图，或缩短视图作用域 |
| 泛型上的生命周期约束、HRTB | 不能写“返回的视图活得与闭包参数一样长”的高阶 API | 作用域 lambda；必要时具体化 |
| 视图进入长寿命堆对象 | 不能把 `Slice` 存进普通 `class` | R3 句柄，或存 owned 数据 + 索引区间 |
| 自引用结构 | 不支持（Rust 安全子集同样不支持） | Arena 句柄；协程帧见 §8 |

### 5.4 容器与泛型的放开（后置）

`List<Slice<T>>` 一类需求通过“泛型参数显式放弃 `Escapable` 上界”支持，此时实例化类型自身变为不可逃逸、
origin 为元素 origin 之并。这要求标准库容器逐个审计，语法另议，**不属于 K3 首片**；首片只需要
Array/List/String 的切片与迭代器视图。

### 5.5 独占视图与访问器（K4）

- `fun at(index: Int): inout T from this`：独占借用结果，使 `list.at(i).field = v`、用户自定义索引、
  `getOrPut` 链式修改可由库表达，取代当前封闭的 element-place 内建；
- 含 `inout` 字段的 `view class`：可变迭代器、游标、`Guard<T>`；
- 独占 reborrow、与两阶段借用的交互、origin 恢复顺序是这一阶段的主要难点，应在 K3 稳定后单独立项；
- Swift `_modify` / Hylo `inout` subscript 式的“访问结束后还要执行收尾代码”的协程访问器**暂不引入**：
  需要收尾的场景先用作用域 lambda 或带 `deinit` 的独占视图表达。

## 6. 长期关系：所有权句柄而不是引用

| 机制 | 用途 | 检查 | 备注 |
|---|---|---|---|
| `Rc<T>` + `Weak<T>` | 共享所有权、回指、观察者 | 引用计数；`upgrade()` 返回可空 | Guide 13 已指出无 `Weak` 时环不释放；`Weak` 应与 §4.1 的 `DropPolicy` 一并设计 |
| 内部可变性 `Cell<T>` / `RefCell<T>` 类 | `Rc` 下的共享变异 | `Cell` 仅限 `Copyable`；动态借用标志 | 见 §12 Q4：也可选择 Swift 式“class 实例动态排他性检查”，二选一 |
| `Arena<T>` + `Handle<T>` | 图、AST、ECS、编译器 IR | 代际计数或 arena 整体释放 | 纯库实现（依赖 K6 的 `unsafe`）；句柄 `Copyable`、`Escapable` |
| `Arc<T>`、`Mutex<T>`、原子类型 | 跨线程共享 | 原子计数 + `Shareable` | K5 |

设计原则：句柄解引用优先采用**作用域形态**（`arena.with(handle) { node -> … }`、`rc.borrowMut { … }`），
返回视图的形态等 K3/K4 到位后再补。这样 K5 的句柄库不被视图进度阻塞。

Koka 的 Perceus（Reinking 等 2021）与 Lean 的 “Counting Immutable Beans”（Ullrich & de Moura 2019）
说明：在所有权信息充分的编译器里，RC 的 retain/release 可以被大量消除或复用。这是 `Rc` 路径的长期优化空间，
不是首版承诺。

## 7. dyn：动态分发

对[现有 v2 候选](v2-interface-values-and-dynamic-dispatch.md)的总体方向表示同意，以下只写补充与分歧。

### 7.1 为什么裸 `Shape` 在 Koven 成立，而 Rust 和 Swift 都走了回头路

Rust 2018 把 trait object 改为必须写 `dyn Trait`，Swift 5.6 起（SE-0335）要求 `any P`。两者的原因都是
“裸名字隐藏了一次表示变化与性能断崖”。这一论据在 Koven 弱得多：

- 普通 `class` 本来就是间接 owner，转为 owned existential 只多一个 witness 指针，不发生装箱；
- Borrow 参数位置的 existential 是 fat borrow，永不分配；
- 唯一的隐藏成本是 `value class` 成为 owned existential 时的装箱——而这正是 Kotlin 开发者已有的心智模型
  （Kotlin 的 value class 被当作接口使用时同样装箱）。

因此建议：**表面语法保持裸名字，成本可见性交给工具而非语法**（例如诊断级提示或编译报告列出装箱点），
`dyn` 关键字可退出保留字表。Swift 的另一个痛点“existential 不满足自己的协议”则必须正面解决，见 7.2。

### 7.2 四条补充

1. **先交付 `sealed interface` / 封闭层级**（`sealed` 已是保留字）。封闭世界下可按 tag 分发、`when` 穷尽检查、
   `value class` 实现者可内联为 union 而不装箱，且不需要新的 runtime ABI。它覆盖了业务代码里相当比例的
   “异构集合”需求，并为开放 existential 提供前端基础设施（独立的 existential 类型身份、expected-type 转换）。
2. **Existential 自满足**：动态兼容的 `Shape` 作为类型自动满足 `<T : Shape>`（方法经 witness 转发）。
   否则 `List<Shape>` 无法传给任何泛型算法，用户会被迫理解“existential 打开”这类概念，违反 A1。
3. **与函数类型统一 ABI**：函数类型本质是单方法 existential（环境指针 + 调用入口）。borrowed closure ≈ borrow
   existential，move closure ≈ owned existential。ABI ADR 应一次性覆盖两者，避免形成两套 fat pointer 约定
   与两套 drop/witness 机制（A6）。匿名对象 `object : Shape { … }` 随之成为“多方法闭包”，捕获规则直接复用闭包规则。
4. **能力经 interface 继承携带**：`interface Job : Transferable` 使 existential `Job` 满足 `Transferable`。
   不引入交集类型或 `Shape & Transferable` 语法。existential 默认 `MoveOnly`、`Escapable`，
   payload 必须 `Escapable`（首版不允许把视图装进 owned existential）。

### 7.3 表示

采用 **fat pointer + 每 (类型, interface) 一张 witness 表**，而不是对象头内嵌 vptr：Koven 没有类实现继承，
外置 witness 使 `value class`、内建类型乃至未来的扩展一致性都能参与，也不为不使用 dyn 的对象增加头部成本。
这与 Rust、Go、Swift 的做法一致；精确布局归 ABI ADR。

## 8. 并发与协程的交叉约束

这两项不在本文展开设计，但有三条必须**提前写死**的约束，否则前面各阶段的选择会被迫返工：

1. **`Shareable` 是第五个结构推导能力**；`borrow T` 可跨线程当且仅当 `T : Shareable`。
2. **结构化并发是借用跨线程的唯一入口**。Kotlin 的 `coroutineScope { launch { … } }` 形态恰好与二等引用
   契合：作用域是词法的，子任务闭包可以是非逃逸闭包，因而可以借用父作用域数据而无需生命周期
   （对照 Rust `thread::scope`、Swift task group）。非结构化的 `thread(move { … })` 继续只接受所有权转移。
3. **协程帧地址稳定**。帧一旦开始执行即不移动（堆分配或由调用方原位持有）。于是跨挂起点的局部借用、
   帧内自引用都是合法的，不需要 Rust `Pin` 那样的用户可见概念。此约束须在 K7 之前进入 ABI ADR，
   因为它限制了“协程值能否被 move”的语义：已启动的协程不可移动。

## 9. 系统编程层

### 9.1 三层安全阶梯

| 层 | 内容 | 谁在用 |
|---|---|---|
| S 静态安全 | 所有权、调用期 loan、`from`、`view class` | 所有人，默认 |
| D 动态检查 | `Rc`/`Weak`、`RefCell` 类、代际句柄、边界检查 | 需要 R3 的应用与库 |
| U `unsafe` | `RawPtr<T>`、`extern "C"`、布局控制、手动分配 | 标准库与系统库作者 |

Koven 在 U 层有一个结构性优势：安全子集里没有可存储引用，loan 都有清晰的静态范围，因此 unsafe 代码
需要遵守的别名合同可以比 Rust 的 Stacked/Tree Borrows 简单得多——大致是“对某 place 的裸访问不得与该 place
上存活的 loan 冲突，且不得越过其 owner 的析构点”。这份合同应作为独立文档先于 `unsafe` 向用户开放而定稿。

### 9.2 需要的能力清单（K6）

- `unsafe` 块与 `RawPtr<T>` 向非标准库 package 开放（门控方式另议，例如 manifest 显式声明）；
- `extern "C"` 声明与导出，C 兼容布局标注，显式对齐/填充；
- 显式存活延长原语（对应 §4.1 的 ASAP 风险）；
- 分配器抽象：先以库层 `Arena`/池满足，全局分配器可替换与分配器参数化容器后置；
- 无 runtime / freestanding 构建档位，`error()` abort 路径可定制；
- 常量求值扩展与静态存储，承接[全局状态候选](global-and-static-state.md)。

### 9.3 标准库去特殊化是这一层的验收

对应 unsafe 候选中的“自举演进与能力下沉”。K6 的出口不是“用户能写 unsafe”，而是
`Array`/`List`/`Map`/`String`/`Rc`/`Box` 中至少一个核心类型完全由 `koven/**/*.ko` 用公开机制实现并替换内建。

## 10. 分阶段计划

阶段代号 K0–K8 只在本文使用，避免与 roadmap 的 P0–P5、M 系列、审计的 D/E 编号混淆。
“版本”一列是建议归属，不是承诺。

| 阶段 | 版本 | 主题 | 语言增量 | 出口验收（library-first） | 退役清单候选 |
|---|---|---|---|---|---|
| K0 | v1 | 内核闭环 | 无 | Single/Unit 结论一致；单一 Drop 规划器；Lifetime IR 不变量可由 verifier 检查；Linux sanitizer 证据 | 旧 lowering 的重复 owner 运输路径 |
| K1 | v1.x | 借用流敏感化 | 有限 NLL、`from a, b`、可空借用结果 | `Map.get`、`longer(a, b)`、`find` 链式调用以库签名表达；L0164 触发面显著收窄 | 条件借用的各处特判 |
| K2 | v2 | dyn | `sealed` 层级 → 开放 existential → `List<Shape>` | 用户库实现插件表、事件总线、异构场景树；闭包与 existential 共用 ABI | 闭包专用 ABI 分支 |
| K3 | v2 | 只读视图 | `Escapable`、`view class`、返回视图的 `from` | 标准库 `Slice`/`StringView`/迭代器以 `.ko` 实现；用户写出零拷贝分词器 | 内建迭代 provider、范围 carrier、callable view 等家族（需逐项核对） |
| K4 | v2.x | 独占视图 | `inout … from`、含 `inout` 字段的视图 | 用户自定义索引、可变迭代器、`Guard<T>` 以库实现 | 封闭的 element-place 内建、scoped Map 访问特判 |
| K5 | v2.x | 共享与并发 | `Weak`、内部可变性、`Shareable`、`Arc`/`Mutex`、作用域线程 | 用户库实现双向链表/图（句柄）、观察者（`Weak`）、并行 map（作用域线程借用只读数据） | 线程入口的 capture 特判 |
| K6 | v3 前 | unsafe 与去特殊化 | 用户 `unsafe`、`RawPtr`、`extern "C"`、布局标注 | 至少一个核心容器由 `.ko` 完全实现并替换内建；C 库绑定样例 | 对应容器的编译器 intrinsic |
| K7 | v3 | 协程 | `suspend`、结构化并发 | 异步 IO 库在不使用 `unsafe` 的前提下借用调用方缓冲区 | — |
| K8 | v4+ | 自举 | 无（纯验证） | 编译器前端若干模块用 Koven 重写；记录被迫绕行之处作为下一轮设计输入 | — |

依赖关系：

```text
K0 ──> K1 ──> K3 ──> K4 ──┐
  └──> K2 ───────────────┼──> K5 ──> K6 ──> K7 ──> K8
                          │
K2 与 K1/K3 互不阻塞，可并行评审；K5 的句柄库只依赖 K0，视图形态的 API 依赖 K3/K4
之十三把 K6 中的裸内存与容器库化排在 K5 之前（阶段三先于阶段四）：共享类型要用 unsafe 实现；以之十三为准
```

排序理由：

- **K1 先于一切**：成本最低（不新增概念），且直接消除当前最常见的“借用不够用”场景；
- **K2（dyn）先于 K3（视图）**：Kotlin 背景的开发者第一天就会需要 interface 作为值，而视图是库作者需求；
  先做 dyn 也能尽早验证“闭包与 existential 共用 ABI”，避免 K3 的闭包能力重构两次；
- **K4 与 K3 分开**：独占视图的 reborrow 与恢复规则是整条路线上形式化风险最高的一段，不应阻塞只读视图的收益；
- **K6 在 K7 之前**：协程 runtime 与异步 IO 需要 FFI 与手动布局，由标准库以 `unsafe` 实现比做成编译器内建更符合 A6。

## 11. 实施指导

### 11.1 每个阶段的固定流程

沿用仓库既有生命周期，不新增流程：

1. 独立 proposal：只写该阶段的语义、反例与取舍，引用本页作为方向依据；
2. **试金石程序先行**：先用候选语法手写该阶段出口验收里的库与用例（不要求可编译），评审“写出来是否像 Kotlin、
   是否零标注”。写不顺就回到设计，不进入 Guide；
3. 用户明确启用新 Guide 版本；
4. 需要长期决定的内部表示写 ADR（Lifetime IR、existential/闭包 ABI、协程帧稳定性、unsafe 别名合同）；
5. 按 Phase 拆 Spec：Phase 2 类型与能力 → Phase 3 所有权事实 → Phase 4 SSA/verifier/native，
   每片保持“先失败测试、再最小实现”；
6. 出口时核对退役清单并同步 Architecture。

### 11.2 试金石程序集

建议作为长期回归资产维护（落盘位置与形式另定）。每个程序都应同时有“应通过”与“应拒绝且诊断可解释”的版本：

| 程序 | 检验的能力 | 最早阶段 |
|---|---|---|
| `Complex` / `Vector` 数值库 | value class、Copyable、运算符 | K0 |
| LRU 缓存（Map + 句柄链表） | R3 句柄、`replace`/`swap` | K0/K5 |
| 词频统计 `map.get(w)?.let { … }` | 可空借用结果 | K1 |
| 插件注册表 / 表达式树求值器 | sealed、existential、`List<Shape>` | K2 |
| 零拷贝 CSV / JSON 分词器 | `StringView`、迭代器视图 | K3 |
| 原地排序、矩阵行视图 | 独占视图、不相交切分 | K4 |
| 双向链表、DOM 式树（父回指） | `Weak`、Arena 句柄 | K5 |
| 并行 map-reduce | `Shareable`、作用域线程 | K5 |
| 自写 `Vec<T>` 与 libc 绑定 | `unsafe`、`RawPtr`、`extern` | K6 |
| 回显服务器 | 协程、借用缓冲区 | K7 |

### 11.3 复杂度预算

为 A6 提供可执行的度量，建议每阶段出口记录三个数字并与上一阶段比较：

- 所有权检查与 refinement 中“按语言家族分派”的分支数（应下降或持平）；
- 标准库中依赖编译器内建识别的类型/函数数（应下降）；
- `UnsupportedNode` / L0164 一类“能力尚未覆盖”诊断在试金石程序集上的触发次数（应趋零）。

### 11.4 主要风险

| 风险 | 表现 | 缓解 |
|---|---|---|
| 视图能力蔓延为生命周期系统 | 出现“给 origin 起名”“在泛型上约束 origin”的需求 | 以 §1 第 1 条为硬线；此类需求一律导向 R3 句柄或作用域 lambda |
| `Escapable` 隐式上界使容器对视图不可用 | 用户抱怨不能 `List<Slice>` | §5.4 后置放开；在此之前诊断必须直接给出替代写法 |
| dyn 隐藏装箱成本 | value class 热路径性能意外 | 工具报告装箱点；`sealed` 路径不装箱 |
| 独占视图不健全 | reborrow / 两阶段借用组合出别名 | K4 独立立项；先小范围形式化（可参考 Oxide 对 Rust 核心的建模方式）再实现 |
| ASAP 与 `Weak`/`RawPtr` 交互 | 对象提前释放 | §4.1 `DropPolicy` 在 K5/K6 之前定稿 |
| 阶段间表面语法不一致 | `borrow val`、`view class`、`from` 各有一套措辞 | K1 起就按 §4.2 的统一模型撰写 Guide 文字，即使能力尚未放开 |

## 12. 需要用户决定的问题

以下每项都与现行规则或既有候选存在分歧，本文只给出倾向，不替用户决定。

| # | 问题 | 现行 / 既有候选 | 本文倾向 | 影响阶段 |
|---|---|---|---|---|
| Q1 | 是否承认 `borrow T` 为（不可逃逸的）类型，从而允许 `(borrow T)?` | Guide 10 明确不引入，倾向控制流绑定 | 承认；可空借用即普通 nullable | K1 |
| Q2 | `dyn` 关键字去留 | 保留字，候选倾向不要求书写 | 退出保留字表 | K2 |
| Q3 | 方法返回视图时是否允许省略 `from this` | 现行必须显式 | 仅方法且唯一候选来源时可省略；自由函数必须写 | K3 |
| Q4 | `Rc` 下的共享变异模型 | 未定义 | `Cell`/`RefCell` 类显式包装（成本可见，符合 A2）；备选为 class 实例动态排他性检查 | K5 |
| Q5 | `sealed` 是否先于开放 existential 交付 | 候选未提及 | 先交付 | K2 |
| Q6 | `value class` 成为 owned existential 是否允许语义上的隐式分配 | 候选列为待答 | 允许，由工具提示 | K2 |
| Q7 | 用户 package 何时、以何种门控获得 `unsafe` | v2 再议 | K6，manifest 显式声明 | K6 |
| Q8 | ASAP 析构是否对 `Weak` 观察点与 `RawPtr` 来源让步 | 未涉及 | 让步，纳入 `DropPolicy` | K5/K6 |
| Q9 | dyn 与视图的先后 | roadmap 将 dyn 与 Shareable 同列 v2 | dyn 先行，Shareable 移到 K5 | 排期 |

## 13. 明确不做

- 命名生命周期、生命周期泛型参数、生命周期约束与型变；
- 普通 `class` 的借用字段；视图进入全局或静态存储；
- 类实现继承；用户自定义运算符与优先级；文本宏；
- 追踪式 GC，或把 ARC 设为默认对象模型；
- 默认携带 RTTI / 反射的 existential；
- 需要用户可见 `Pin` 概念的协程模型；
- 在 K6 定稿 unsafe 别名合同之前向用户开放裸指针。

## 14. 参考资料

以下条目凭既有知识整理，本次未逐条联网复核；落入正式 proposal 或 ADR 前应核对出处与编号。

语言设计方法

- G. L. Steele Jr., *Growing a Language*, OOPSLA 1998 / HOSC 1999。

线性、所有权与区域

- P. Wadler, *Linear Types Can Change the World!*, 1990。
- D. Walker, *Substructural Type Systems*，载 B. Pierce 编 *Advanced Topics in Types and Programming Languages*（ATTAPL）第 1 章。
- D. Clarke, J. Potter, J. Noble, *Ownership Types for Flexible Alias Protection*, OOPSLA 1998。
- M. Tofte, J.-P. Talpin, *Region-Based Memory Management*, 1997。
- D. Grossman 等, *Region-Based Memory Management in Cyclone*, PLDI 2002。

Rust 借用模型及其形式化

- Rust RFC 2094 *Non-Lexical Lifetimes*；N. Matsakis, *An alias-based formulation of the borrow checker*（Polonius），2018。
- A. Weiss 等, *Oxide: The Essence of Rust*, 2019。
- R. Jung 等, *RustBelt*, POPL 2018；*Stacked Borrows*, POPL 2020；N. Villani 等, *Tree Borrows*, PLDI 2025。
- J. Gjengset, *Rust for Rustaceans*；*The Rustonomicon*。

二等引用、值语义与不可逃逸类型

- L. Osvald 等, *Gentrification Gone Too Far? Affordable 2nd-Class Values for Fun and Profit*, OOPSLA 2016。
- D. Racordon 等, *Implementation Strategies for Mutable Value Semantics*, JOT 2022（Hylo/Val）。
- Swift *Ownership Manifesto*；SE-0377（borrowing/consuming）、SE-0390（noncopyable）、SE-0446（nonescapable）、SE-0447（Span）。
- Mojo 文档 *Lifetimes, origins, and references*；C# `ref struct` / `Span<T>` / `scoped` 规范。
- A. Lorenzen 等, *Oxidizing OCaml with Modal Memory Management*, ICFP 2024。
- M. Odersky 等, *Capturing Types*, TOPLAS 2023。

动态分发与 existential

- Swift SE-0335（`any`）、SE-0352（implicitly opened existentials）；Rust Reference *Trait objects*。
- Kotlin 文档 *Inline value classes*、*Sealed classes and interfaces*。

引用计数、句柄与并发能力

- A. Reinking 等, *Perceus: Garbage Free Reference Counting with Reuse*, PLDI 2021。
- S. Ullrich, L. de Moura, *Counting Immutable Beans*, IFL 2019。
- S. Clebsch 等, *Deny Capabilities for Safe, Fast Actors*, 2015（Pony）。
- C. Gordon 等, *Uniqueness and Reference Immutability for Safe Parallelism*, OOPSLA 2012。
- E. Arvidsson 等, *Reference Capabilities for Flexible Memory Management*, OOPSLA 2023（Verona）。
- R. Elizarov 等, *Kotlin Coroutines: Design and Implementation*, Onward! 2021；N. J. Smith, *Notes on structured concurrency*, 2018。
