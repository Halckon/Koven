# 集合算法所有权候选设计

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审顺序容器算法 API 的所有权契约时 · **唯一真源**：现行语义仍以 [现行 guide](../guide/README.md) 为准

本文不修改、取代或启用 Koven v0.37，也不批准 guide、Spec 或 ADR，不授权实现。它给出顺序
容器算法的候选所有权契约：**默认借用视图、显式 `consume()` 消费、按 expected type 或
`.toList()` 物化**。算法声明语法依赖
[受限扩展函数候选设计](restricted-extension-functions.md)；相关取舍见
[String Copyable 候选取舍](string-copyability.md) 与 [显式 clone() 候选设计](explicit-clone.md)。

## 1. 问题

目标是让下面的写法可用，且对 `List<String>` 与 `List<Int>` 一致：

```kotlin
val a = "sss"
val result = list.filter { it == a }
```

现行规范下这段代码的每一部分都能工作，但组合起来对 `List<String>` 不成立。障碍不在 lambda，
在算法结果的所有权归属。

## 2. 现行事实

- 顺序容器迭代是借用式的：单名称 binding 是只在本轮 body 可见的 `Borrow T`；`T: Copyable`
  时普通值使用从该借用读取 owned copy，`MoveOnly` `T` 只能读取或继续 Borrow
  （[借用式顺序容器迭代 provider](../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider)）。
- 索引是 place 而非隐式 owned 返回：`T` 不满足 `Copyable` 时，普通 owned 读取与按值实参
  都产生所有权诊断，禁止移出元素留下未初始化洞（[集合、索引与解构](../guide/12-collections-destructuring.md)）。
- 三种顺序容器自身不满足 `Copyable`，即使元素 `Copyable` 也不例外。
- borrowed closure 已提供"一个值其有效范围被 loan 约束在当前 callable 内、不得逃逸"的完整
  机制与诊断（L0137），见 [调用、Lambda 与 Closure](../guide/07-calls-lambdas-closures.md)。
- 核心原语（构造、`size`、索引）之外的方法属于 Phase 5，尚未发布。

## 3. 候选设计：三层结构

| 层 | 语法 | 元素拷贝 | 源容器 | 结果可逃逸 |
|---|---|---|---|---|
| 借用视图（默认） | `list.filter{}` | 0 | 保留（loan 冻结） | ❌ 需物化 |
| 消费式 | `list.consume().filter{}` | 0（移动） | 失效 | ✅ |
| 物化 | 类型标注 / `.toList()` | clone k 个 | 保留 | ✅ |

### 3.1 默认：借用视图

子序列类算法默认返回**借用视图**：它不拥有元素，对源建立 shared loan，迭代时以 `Borrow T`
读取原元素。

**`View<T>` 是编译器绑定的 intrinsic 类型**，与 `Array<T>` / `List<T>` / `MutableList<T>` 并列：
由 `TypeEnvironment` 绑定、不按源码名称识别、只读、不能由用户构造。本候选以该假定为前提，
它也是 [受限扩展函数](restricted-extension-functions.md) L1 把 `View<T>` 列为 receiver 类型的
依据。

```kotlin
for (s in list.filter { it == a }) { use(s) }   // 零元素拷贝
list.filter { it == a }.size                    // 零元素拷贝
```

### 3.2 消费式：`consume()`

`list.consume()` 取得源容器的所有权，返回消费式视图；其上的算法把元素**移动**到结果容器。
源 binding 在此之后不可用。

```kotlin
val kept: List<String> = list.consume().filter { it == a }   // 零元素拷贝
// list 之后不可用
```

命名与返回类型见 §11 待决策点。

### 3.3 物化：expected type 与 `.toList()`

```kotlin
val a: List<String> = list.filter { it == a }            // 按类型标注物化
val b: List<String> = list.filter { it == a }.toList()   // 显式物化，与上行等价
val c = list.filter { it == a }.toList()                 // 无标注时的显式形式
```

有 expected type 时 `.toList()` 冗余但合法，与 Kotlin 中 `val x: List<Int> = m.toList()`
的情形一致。物化对 `MoveOnly` 元素调用 [clone()](explicit-clone.md)。

## 4. 算法分类与返回类型

视图只对"**原元素的子序列**"成立，这决定了每个算法的返回类型：

| 类别 | 算法 | 返回 | 理由 |
|---|---|---|---|
| 子序列 | `filter` / `take` / `drop` / `dropLast` | `View<T>` | 结果元素是原元素的子序列 |
| 变换 | `map` / `flatMap` | `List<U>` | 结果元素由 `f` 产生，原容器里不存在 |
| 聚合 | `fold` / `count` / `any` / `all` / `forEach` | 单值 / `Unit` | 不涉及元素集合 |

**`map` / `flatMap` 天然就是物化点，不需要额外物化语法**：

```kotlin
val names = users.map { it.name }   // List<String>，直接可用，无需 toList()
```

`flatMap` 更彻底：其结果来自多个子容器，连"单一源的子序列"这个视图语义基础都不存在，
因此永远返回容器，没有视图版本。

### 4.1 `map` 的物化不是额外成本

| 算法 | 物化时发生什么 | 是否额外成本 |
|---|---|---|
| `filter` 物化 | clone 保留的元素 | ✅ 额外成本 |
| `map` 物化 | 把 `f` 的返回值移入新容器 | ❌ `f` 本来就要产生值 |

因此"需要额外付 clone 成本"的只有子序列类算法物化时——而它们默认是视图，不物化。

## 5. 链式调用

### 5.1 类型演化

```kotlin
users.filter { it.active }.map { it.name }
//    View<User>            List<String>      ← 最终是容器，可直接持有/返回

users.map { it.name }.filter { it == a }
//    List<String>       View<String>         ← 最终是视图，需物化才能逃逸

users.filter { it.active }.map { it.name }.filter { it == a }
//    View<User>            List<String>        View<String>
```

规则：**链的最终类型取决于最后一步**——子序列类结尾是视图，变换类结尾是容器。

Kotlin 中最常见的 `list.filter{}.map{}` 模式在此设计下最终类型是容器，可直接持有、返回、
传参，且全程零元素拷贝。

### 5.2 临时容器与移动

链式中间产生的容器是**临时值**，这使末端物化常常零成本：

```kotlin
// 源是具名变量 → 物化必须 clone
val r1: List<String> = names.filter { it != a }

// 源是链中的临时容器 → 物化可以移动
val r2: List<String> = users.map { it.name }.filter { it != a }
```

第二种情形中，`map` 产生的临时 `List<String>` 之后不再被使用，元素可移动到结果。这复用的
是既有的 temporary source 延寿与移动规则（12 页 §37.3 对 `for` 的 temporary source 即
如此处理），不是新机制。

## 6. 视图的 loan、逃逸与诊断

| 规则 | 内容 |
|---|---|
| loan | 视图对源建立 shared loan，存活到视图的 ASAP drop point |
| 源冻结 | loan 存活期间源不能被 move / 改元素 / 扩容，冲突沿用 L0135 |
| 嵌套 | 视图可再 `filter`，形成 loan 链 |
| 逃逸 | 视图**不得** return、写入字段、交给 `Value` 参数、被 escaping move closure 捕获；诊断推广自 L0137 |

```kotlin
fun names(): List<String> = list.filter { it == a }   // 视图不能逃逸
// 修正：写 List<String> 标注或 .toList()
```

与 borrowed closure 的机制关系：二者都是"值 + shared loan + 逃逸限制"，差别只在视图可被
重复迭代、可索引，而闭包不可。因此本设计**不需要生命周期参数、不需要可存储 borrow value、
不需要 `Shareable`**。

## 7. 视图的 API 表面与实现载体

链式调用会在视图上继续调方法，因此视图必须覆盖同一套算法：

| 在 `View<T>` 上调用 | 返回 |
|---|---|
| `filter` / `take` / `drop` | `View<T>`（继续借用） |
| `map` / `flatMap` | `List<U>`（物化） |
| `fold` / `count` / `any` | 单值 |
| `toList()` | `List<T>`（物化，对 `MoveOnly` 元素 clone） |
| `size` / 索引 | 借用读取 |

### 7.1 实现载体：依赖受限扩展函数

12 页规定算法"用目标语言实现"（Phase 5），但 `List<T>` / `View<T>` 是编译器绑定的 intrinsic
类型，现行语法无法为它们声明 member。因此本候选的算法表面**依赖
[受限扩展函数候选设计](restricted-extension-functions.md)**：

```kotlin
borrow fun <T> List<T>.filter(pred: (T) -> Boolean): View<T>
borrow fun <T> List<T>.take(n: Int): View<T>
borrow fun <T> View<T>.filter(pred: (T) -> Boolean): View<T>
own    fun <T> List<T>.consume(): ConsumingView<T>
```

两条性质要分开看：

- **普通算法**（`count`、`sum`、`joinToString` 等）只使用 receiver 的公开表面，可用纯 `.ko`
  实现；
- **返回或构造 `View<T>` 的算法**（`filter` / `take` / `consume`）还需要编译器提供 intrinsic
  构造能力（例如"按索引集合建立视图"），因为它无法由用户代码构造。

实现工作量来自"容器与视图各提供一套同签名算法"，属于 Phase 5；但**声明这些算法的语法
能力属于前置阻塞项**，不在 Phase 5 内解决。

## 8. 为什么默认不是立即求值（跨语言依据）

### 8.1 Kotlin / Swift：eager 默认，前提是元素复制廉价

Kotlin 官方文档明确把立即求值作为默认：

> When the processing of an Iterable includes multiple steps, they are executed **eagerly**:
> each processing step completes and returns its result – an **intermediate collection**.

Swift 官方对 `LazySequenceProtocol` 的描述是"a sequence on which **normally-eager** sequence
operations are implemented lazily"——同样以 eager 为常态。

但两者的中间集合成本都是 O(1) 每元素：Kotlin 是引用复制 + GC，Swift 是 retain/COW + ARC。

### 8.2 Koven 的前提不同

| | 中间集合的元素成本 |
|---|---|
| Kotlin | 引用复制 O(1) |
| Swift | retain/COW O(1) |
| **Koven** | **`MoveOnly` 元素深拷贝 O(元素字节数)** |

因此"立即求值默认"在 Koven 中必然二选一：对 `MoveOnly` 元素自动 clone（隐藏 O(总字节数)
成本，违反 13 页"禁止把 retain 隐藏在赋值或参数传递中"的取向），或对 `MoveOnly` 元素报错
（`List<Int>` 可用、`List<String>` 不可用，体验分裂）。

### 8.3 Rust：入口决定借用/消费，适配器惰性

Rust 把"借用还是消费"放在**入口**：`iter()` / `iter_mut()` / `into_iter()`，适配器本身惰性，
`.collect()` 物化。本候选采用同一组织方式，但把决定放在**结果类型**上（视图 vs 容器），
以避免用户必须写入口转换方法。

### 8.4 Mojo：借用视图已有先例

Mojo 的类型表区分 `String`（owned, heap-allocated）、`StringSpan`（non-owning view）、
`StaticString`（`StringSpan` over static read-only data）与 `Span`（non-owning view）。
这说明"owning / 借用视图 / 静态"三者并存是已被采用的设计，不是孤例。

### 8.5 Zig：不链式

Zig 不做链式适配器，使用显式 `for` 与 `std.ArrayList`。它的价值在于印证"不隐藏分配与控制
流"这一取向，不构成本候选的直接对照。

## 9. 与其他候选的关系

### 9.1 显式 `clone()`

物化对 `MoveOnly` 元素需要 [clone()](explicit-clone.md)。`clone()` 以 shared `Borrow` 使用
receiver 并返回新 owner，因此 `list[0].clone()` 可直接用于元素级取出。

### 9.2 String Copyable

若 `String` 改为 `Copyable`，子序列类算法的物化将不需要 clone，本设计的第 3.3 与第 4.1 节
代价相应消失。但该改动的收益、代价与阻塞见 [String Copyable 候选取舍](string-copyability.md)
§3–§6；本候选不以它为前置。

### 9.3 元素按值取出的能力缺口

"从容器取出一个元素并持有"是本议题的另一处表现。现行只有三条路：

| 做法 | 现状 | 结果 |
|---|---|---|
| 移动元素（留洞） | ❌ v1 禁止 partial move | 所有权诊断 |
| 共享元素（`Rc`） | ✅ 但类型噪音大、不可跨线程 | 新 `Rc` handle |
| 元素本身 `Copyable` | ✅ 仅限 `Int`、`Copyable` value class 等 | `val n = intList[0]` 可用 |

两条候选补充：显式 `clone()`（覆盖面大、不触碰类型系统），以及静态字符串类型（字面量专用、
无 owner、可 `Copyable`，增量窄）。候选倾向是优先 `clone()`。

## 10. Phase 边界

| Phase | 需要做什么 |
|---|---|
| 前置 | [受限扩展函数](restricted-extension-functions.md)：算法声明语法 |
| 2 | 视图类型 identity；按 expected type 选择表示（视图 / 物化）；算法返回类型规则 |
| 3 | 视图的 shared loan、源冻结、逃逸诊断；消费式的 `Value` 交付与源失效 |
| 4 | 视图的无分配表示（索引或谓词）；物化的分配与元素移动 / clone；视图的 intrinsic 构造 |
| 5 | 算法实现；容器与视图的同签名表面 |

## 11. 待决策点

1. **`consume()` 返回类型的命名与语义**：消费式视图上的子序列类算法是立即产出容器，还是
   返回消费式视图并延迟产出；
2. **视图的立即 vs 惰性**：立即视图记录匹配索引（O(k) 分配，pred 每元素一次，与 Kotlin
   语义一致）；惰性视图携带谓词（零分配，但 pred 调用次数随迭代方式变化）。候选倾向立即；
3. **`toList()` 之外的物化形式**是否必要（如 `toMutableList()`）；
4. **`map` 在链式下是否需要成本提示**（其成本来自 `f` 而非复制，可能不需要）；
5. **视图与 `List<T>` 参数的互操作**：传给 `List<T>` 参数是否允许按 expected type 物化；
6. **按 expected type 选择表示**是本设计唯一缺少直接先例的机制（lambda 与整数字面量是弱
   先例），需要单独评估 Phase 2 的实现代价；
7. **算法声明载体的前置性**：本候选依赖
   [受限扩展函数](restricted-extension-functions.md)。若该前置不被采纳，算法只能改为编译器
   绑定的 intrinsic member surface，那与 12 页"算法用目标语言实现"冲突，需要重新决策。

## 12. 非目标

本文不定义具体算法集合、不规定 Phase 5 的实现顺序、不授权在 v1 内新增任何集合方法，也不
提议修改 `Copyable` 定义或引入生命周期参数。
