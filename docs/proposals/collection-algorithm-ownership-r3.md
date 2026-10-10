# 集合算法所有权候选设计（修订版 r3）

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审顺序容器算法 API 的所有权契约时 · **唯一真源**：现行语义仍以 [现行 guide](../guide/README.md) 为准

本文不修改、取代或启用 Koven 的任何现行规则，也不批准 guide、Spec 或 ADR，不授权实现。它给出顺序容器算法的候选所有权契约：**默认借用视图、显式 `consume()` 消费、显式 `.toList()` 物化**，并把视图建立在一项独立的**非逃逸类型能力**之上（§6）。算法声明语法依赖 [受限扩展函数候选设计](restricted-extension-functions.md)；相关取舍见 [String Copyable 候选取舍](string-copyability.md) 与 [显式 clone() 候选设计](explicit-clone.md)。

> **基线说明**：r1 以 v0.37 为讨论基线，当前 guide 已更新。本文所有"现行事实"均需对照当前 guide 重新核对；凡与现行 guide 的措辞（接收者模式写法、章节编号、诊断码）不一致处，以 guide 为准，示例语法仅为示意。
>
> **采纳状态**：核心路线（显式物化、`consume()` 立即移交所有权、首片立即产出）已接受为后续规划；本文整体**尚未启用**，尚不能作为实现合同。§6 与 §3.2.1 是实现前必须逐条验收的条款。

---

## 0. 修订摘要

### 0.1 r1 → r2（已采纳）

| 编号 | 变更 | 原因 |
|---|---|---|
| C1 | 删除"按 expected type 隐式物化"，物化必须显式 `.toList()` | 与"不隐藏成本"自相矛盾；并非所有 `MoveOnly` 类型都可克隆 |
| C2 | `View<T>` 改为通用非逃逸类型能力的实例 | 与"借用是访问模式而不是类型"一致，避免每个携带借用的类型各做内建 |
| C3 | 持有 loan 的局部绑定一律写 `borrow val` | 与参数 `borrow x: T`、结果 `: borrow T from p` 统一 |
| C4 | `consume()` 立即取得所有权，结果改名 `ConsumingSeq<T>`（暂名） | 它是拥有者而非视图；沿用普通 move / drop 规则 |
| C5 | 首片只做立即产出 | 惰性链依赖迭代器与闭包生命周期 |
| C6 | 修正 `map` 成本论述 | 借用元素的字段投影需要显式 `clone()` |
| C7 | 删除"临时容器可隐式移动" | for-source 的延寿规则不是元素移动规则 |
| C8 | 采纳 `T?` 展平，元素返回类算法暂不纳入（r3 细化为 C17） | 展平后 `T?` 丢失存在性 |
| C9 | 签名补充来源 `from` | 与 M2B 来源合同对齐 |

### 0.2 r2 → r3（本版）

| 编号 | 变更 | 原因 | 影响章节 |
|---|---|---|---|
| C11 | 视图拆为 **N1a（纯来源）** 与 **N1b（来源加自有元数据）**，首片只交付 N1a 与消费路径 | r2 的 `filter` 视图拥有索引缓冲却借用元素，需要新的"自有存储加受约束访问"能力，不能套用只携带来源的 `borrow val` | §3.1、§6 |
| C12 | 链式视图保存**根索引**，`from this` 对非逃逸接收者表示继承其来源；临时 owner **不延寿** | r2 只写"形成 loan 链"，缺生命周期合同 | §5、§6.4 |
| C13 | 非逃逸类型能力是**独立前置**，不由借用返回或借用闭包推导 | 闭包可捕获多个来源，且现有限制与允许 `from` 返回的视图不同 | §6.6、§11 |
| C14 | 补齐清理与回调合同：谓词借用结束后才移动或销毁元素；短路算法的剩余元素清理 | r2 只覆盖中止和非局部退出 | §3.2.1 |
| C15 | `ConsumingSeq` 首片仅为表达式链内的单次使用接收者，不可绑定或存储 | r2 的 §3.2 与 §12 对保存规则前后不一致 | §3.2、§12 |
| C16 | §9 改名为"为什么默认不是拥有值的物化"，重写成本对比，写明 `MoveOnly` 不等于可深拷贝 | 默认立即索引视图本身就是 eager；r2 对 Swift 等的成本概括过粗 | §9 |
| C17 | 元素返回类算法拆分：`first`/`last` 用"缺失即中止"的确定接口，owned 取出路径列入，`find`/`firstOrNull` 才等待 `Option<T>` | r2 过严，且遗漏了已交付的 owned 取出 | §4.2 |

---

## 1. 问题

目标是让下面的写法可用，且对 `List<String>` 与 `List<Int>` 一致：

```kotlin
val a = "sss"
val result = list.filter { it == a }
```

现行规范下这段代码的每一部分都能工作，但组合起来对 `List<String>` 不成立。障碍不在 lambda，在算法结果的所有权归属。

本设计的结论是：**`val result = list.filter { ... }` 不再作为合法写法**，对 `List<String>` 与 `List<Int>` 都一样。它必须改写成下面四种之一，每一种的所有权与成本都在调用点可见：

| 写法 | 含义 | 所需能力 |
|---|---|---|
| ① `for (s in list.filter { it == a }) { use(s) }` | 视图作为表达式链中的临时值 | N1b（临时） |
| ② `borrow val view = list.filter { it == a }` | 显式持有借用 | N1b（完整） |
| ③ `val owned: List<String> = list.filter { it == a }.toList()` | 显式物化，要求元素可 clone | N1b（临时） |
| ④ `val moved: List<String> = list.consume().filter { it == a }` | 消费源容器 | 消费路径（首片） |

同时 `list.take(n)`、`list.drop(n)`、`list.dropLast(n)` 返回的范围视图属于 N1a，随首片交付。借用路径上的 `filter` 依赖 N1b，不在首片内（§6.5、§12）。

这是对 Kotlin 表面形态的有意偏离，代价见 §9.2。

---

## 2. 现行事实与上游决定

### 2.1 现行事实（以当前 guide 核对）

- 顺序容器迭代是借用式的：单名称 binding 是只在本轮 body 可见的 `Borrow T`；`T: Copyable` 时普通值使用从该借用读取 owned copy，`MoveOnly` `T` 只能读取或继续 Borrow（[借用式顺序容器迭代 provider](../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider)）。
- 索引是 place 而非隐式 owned 返回：`T` 不满足 `Copyable` 时，普通 owned 读取与按值实参都产生所有权诊断，禁止移出元素留下未初始化洞（[集合、索引与解构](../guide/12-collections-destructuring.md)）。
- 三种顺序容器自身不满足 `Copyable`，即使元素 `Copyable` 也不例外。
- `MutableList` 已有 owned 取出路径：`removeAt`、`removeFirst`、`removeLast`（SPEC-0282–0284，应以当前 guide 为准）。它们是"从容器取出元素并持有"在 v1 内已经可用的方式。
- borrowed closure 已提供"一个值其有效范围被 loan 约束在当前 callable 内、不得逃逸"的机制与诊断（L0137），见 [调用、Lambda 与 Closure](../guide/07-calls-lambdas-closures.md)。
- 核心原语（构造、`size`、索引）之外的算法属于 Phase 5，尚未发布。

### 2.2 本文采用的上游决定

| 编号 | 决定 |
|---|---|
| U1 | **`T?` 按 Kotlin 展平**：`T??` 等同于 `T?`，给出冗余可空诊断。需要保留嵌套存在性时使用独立的 `Option<T>` |
| U2 | **借用是访问模式，不是类型**：只出现在参数、结果与 `borrow val` 局部绑定；不能是字段类型、泛型实参、集合元素 |
| U3 | **参数写 `borrow x: T` / `inout x: T`，结果写 `: borrow T from p`，局部写 `borrow val s = expr`**；普通 `val` 不隐式把借用结果变成拥有值 |
| U4 | **可选借用暂缺**：位置可能不存在的条件借用不作为类型，也不在本设计中使用 |
| U5 | **通用非逃逸类型能力**：携带 loan 的值类型由统一机制表达，初期收窄范围（§6） |

---

## 3. 候选设计：三层结构

| 层 | 语法 | 元素拷贝 | 源容器 | 结果可逃逸 |
|---|---|---|---|---|
| 借用视图（默认） | `list.take(n)` / `list.filter{}` | 0 | 保留（loan 冻结） | ❌ |
| 消费式 | `list.consume().filter{}` | 0（移动） | 失效 | ✅ |
| 物化 | `.toList()` | clone k 个，要求元素可 clone | 保留 | ✅ |

### 3.1 默认：借用视图

子序列类算法默认返回**借用视图**：它不拥有元素，对源建立 shared loan，迭代时以 `Borrow T` 读取原元素。

**`View<T>` 是非逃逸类型能力（§6）的第一个实例**，由编译器绑定（`TypeEnvironment`），不按源码名称识别，只读，不能由用户构造。

视图按**描述符是否拥有堆存储**分两级：

| 级别 | 描述符 | 例子 | 析构 | 首片 |
|---|---|---|---|---|
| **N1a** | 只含来源和定长标量（起止下标等），内联、可平凡析构 | `take`、`drop`、`dropLast`、切片 | 无 | ✅ |
| **N1b** | 另外拥有一块堆存储（匹配元素的索引缓冲） | `filter` | 需要释放缓冲 | ❌（§6.5） |

两种视图可以共享同一个类型 `View<T>`，区别在表示。首片中 `View<T>` 只有范围表示，因此其析构始终平凡。

```kotlin
for (s in list.take(3)) { use(s) }       // N1a：零元素拷贝，零分配
list.drop(1).size                        // N1a
```

N1b 的 `filter` 视图使用**立即**表示：构造时对源扫描一次，记录匹配元素的索引。

- 谓词每个元素恰好调用一次；
- 代价是 O(k) 的索引分配，**不是**"零分配"；
- 惰性视图（携带谓词、零分配、但谓词调用次数随迭代方式变化）留作后续评估。

### 3.2 消费式：`consume()`

`list.consume()` **立即取得源容器的所有权**，返回拥有该容器的消费序列 `ConsumingSeq<T>`（暂名）；原 binding 在 `consume()` 成功求值后不可用。其上的算法把元素**移动**到结果容器。

```kotlin
val kept: List<String> = list.consume().filter { it == a }   // 零元素拷贝
// list 之后不可用
```

要点：

- `ConsumingSeq<T>` 是**拥有者**，不是视图，不是非逃逸类型。
- **首片限制**：它只能作为表达式链中的单次使用接收者，**不可绑定到变量、不可存储、不可传参、不可返回**。每个算法以 `own` 接收者执行，调用后其自身失效。这样不存在"已保存但未被使用的消费序列"，析构规则也最小。绑定与传递留到后续放开（§12）。
- 选择"立即转移"而非"仅调用链标记"，是为了沿用普通 move / drop 规则，不为求值顺序与提前退出另写特例。
- 活跃借用期间不能 `consume()` 同一来源（沿用 L0135 的冲突诊断）。
- 只有独立的 owned 转换才消耗；普通只读算法不要求先 `consume()`。
- 不同时保留 `consume().toMap()` 与 `intoMap()` 两套等价主 API。

#### 3.2.1 清理与回调合同

**基本规则**

| 场景 | 行为 |
|---|---|
| `consume()` 之后的源 binding | 不可用（已移动） |
| `consume()` 结果在链中被后续算法接收 | 由该算法接管所有权 |
| `filter` 中被谓词拒绝的元素 | 谓词返回 `false` 后立即 drop |
| `filter` 中保留的元素 | 按源顺序移入结果 `List<T>` |
| 谓词内中止 | 中止不展开、不运行 drop，不产生清理义务 |
| 谓词内的非局部退出（`return` / `break` 跳出算法） | **首片禁止** |
| `map` 的变换 lambda | 以 `own` 接收元素；**局部移出被禁止**，lambda 必须消耗整个元素或自行构造新值 |

**回调约束**

- 谓词以 `Borrow` 接收元素；**借用在谓词返回时结束，之后**才能移动或销毁该元素。
- 谓词不得保留借用，也不得把借用或捕获存入结果；结果类型 `U` 必须可逃逸，这由"泛型参数默认可逃逸"（§6.3）保证，不需要额外规则。
- 谓词内的借用捕获遵循 L0137，不能逃出谓词调用。

**短路算法**：`take`、`any`、`all`、`first` 等在得到结果后会提前停止遍历，消费路径必须在返回前保证剩余元素按源顺序各 drop 恰好一次。

| 场景 | 行为 |
|---|---|
| `consume().take(n)` | 前 n 个元素移入结果；其余在返回前逐个 drop |
| `consume().any(p)` / `all(p)` | 命中后停止；剩余元素在返回前 drop；结果是 `Bool` |
| `consume().first()` | 返回首个元素（空序列时中止，见 §4.2）；其余在返回前 drop |
| 借用路径的 `any` / `take` | 无元素所有权，只处理视图自身的存储（N1b 时释放索引缓冲） |

首片只做**立即产出**：`consume().filter { ... }` 直接返回 `List<T>`，而不是继续返回延迟的 `ConsumingSeq<T>`。惰性消费链留到迭代器与闭包生命周期成熟后评估。

### 3.3 物化：显式 `.toList()`

```kotlin
val b: List<String> = list.filter { it == a }.toList()   // 显式物化
val c = list.filter { it == a }.toList()                 // 无标注时同样合法
```

规则：

- **物化必须显式写 `.toList()`**。给视图标注 `List<T>`、或在返回位置写 `List<T>`，都**不**触发物化，而是报错并指向修复建议。
- `.toList()` 对元素调用 [clone()](explicit-clone.md)，因此要求 `T` 具备相应 clone 能力（`Copyable` 或显式 `Clone`）。`MoveOnly` **不等于**可深拷贝，元素不可克隆时，诊断应建议改用 `consume()`。
- 拥有值的搬移走 `consume()`，不走 `.toList()`。

```kotlin
fun names(): List<String> = list.filter { it == a }          // ❌ 视图不能逃逸；不会隐式物化
fun names(): List<String> = list.filter { it == a }.toList() // ✅ 显式
```

---

## 4. 算法分类与返回类型

视图只对"**原元素的子序列**"成立，这决定了每个算法的返回类型：

| 类别 | 算法 | 返回 | 理由 |
|---|---|---|---|
| 子序列 | `filter` / `take` / `drop` / `dropLast` | `View<T>` | 结果元素是原元素的子序列 |
| 变换 | `map` / `flatMap` | `List<U>` | 结果元素由 `f` 产生，原容器里不存在 |
| 聚合 | `fold` / `count` / `any` / `all` / `forEach` | 单值 / `Unit` | 不涉及元素集合 |

`flatMap` 的结果来自多个子容器，不存在"单一源的子序列"这个视图语义基础，因此永远返回容器，没有视图版本。

### 4.1 `map` 的成本

`map` 的成本取决于变换结果是否需要新值：

| 场景 | 借用路径（`list.map {}`） | 消费路径（`list.consume().map {}`） |
|---|---|---|
| 投影 `Copyable` 字段（`it.age`） | ✅ 读取为拷贝 | ✅ |
| 投影 `MoveOnly` 字段（`it.name`） | 必须显式 `it.name.clone()`；`it.name` 是借用，不能作为 owned 结果返回 | ❌ 需要局部移出，v1 禁止（见 §9.4） |
| 构造新值（`User(it.id, ...)`） | 成本来自构造本身 | 同左，元素以 `own` 传入 |

结论：

- `map` 的物化成本**在 lambda 内可见**（显式 `clone()`），不是隐藏的；
- 消费路径的字段投影依赖局部移出，不在首片范围；
- 链式最终类型规则（§5）不变：`map` 总是返回 `List<U>`，是物化点，不需要额外的 `.toList()`。

### 4.2 返回元素的算法

此类算法按"缺失是否属于正常结果"拆分：

| 算法 | 缺失语义 | 首片 | 说明 |
|---|---|---|---|
| `first()` / `last()`（借用） | 空容器时**中止** | ✅（依赖 M2B 的 `borrow T from this`） | 返回 `borrow T from this`，调用者用 `isEmpty()` 守卫；是确定的借用接口，不需要可选借用 |
| `consume().first()`（owned） | 空序列时中止 | ✅ | 返回拥有值，其余元素返回前 drop（§3.2.1） |
| `MutableList.removeAt` / `removeFirst` / `removeLast` | 越界或空时中止 | ✅ 已交付 | owned 取出的现行路径；不要重复设计 |
| `find` / `firstOrNull` / `lastOrNull` / `getOrNull` | 缺失是正常结果 | ❌ | 借用版需要可选借用（U4）；owned 版需要 `Option<T>`，因为展平后的 `T?` 无法区分"缺失"与"元素本身为 null" |

---

## 5. 链式调用

### 5.1 类型演化

```kotlin
users.take(10).map { it.name.clone() }
//    View<User>      List<String>      ← 最终是容器，可直接持有/返回

users.map { it.name.clone() }.take(3)
//    List<String>               View<String>  ← 最终是视图，需 .toList() 才能逃逸
```

规则：**链的最终类型取决于最后一步**——子序列类结尾是视图，变换类结尾是容器。

### 5.2 链式视图的来源：保存根

对任意视图链，每个视图的来源是**根 owner**，而不是它的父视图：

- `from this` 在接收者是非逃逸值时，含义是"**继承** `this` 的来源"，**不是**借用 `this` 本身；
- N1a：`v.take(n)` 在父范围内再取子范围，结果仍是对根容器的范围；
- N1b：`v.filter(p)` 遍历父视图的元素，产出的索引**相对根容器**，每层一份，扫描成本为父视图的长度，索引分配为该层的匹配数；
- 因此父视图（通常是临时值）可以在语句末正常释放，不需要为它延寿；根 owner 的 loan 一直存活到最后一层视图的最后一次使用。

### 5.3 临时 owner：不延寿

链中产生的临时 `List` 在语句结束时销毁，它不能充当视图的来源并存活到语句之后：

```kotlin
borrow val v = users.map { it.name.clone() }.take(3)     // ❌ 来源是临时 List，语句末销毁
val names = users.map { it.name.clone() }                 // ✅ 先把 owner 绑定到具名变量
borrow val v = names.take(3)
```

诊断：*"该视图的来源是语句内的临时值，语句结束时销毁。请先把来源绑定到具名变量，或改用 `.toList()` / `consume()`。"*

不对临时 owner 做延寿，是为了不引入隐式规则，也与 M2B 中"临时 owner 上的借用不能被保存"保持一致。同理，**对临时 owner 省略 `consume()`** 不是本提案的内容：需要移动时写出来：

```kotlin
val r: List<String> = users.map { it.name.clone() }.consume().filter { it != a }
```

"对临时 owner 省略 `consume()`"可以作为后续独立评审的人体工学优化（§12）。

---

## 6. 非逃逸类型能力

借用闭包（L0137）、`View<T>`、以后的切片、迭代器适配器、锁守卫，在抽象上都是"持有来自某个 owner 的 loan、不能逃出 owner 有效范围的值"。如果每一项都做成编译器内建，复杂度会随数量线性增长，因此引入一项统一能力，并且**作为独立的前置能力交付**（§6.6）。

### 6.1 定义

> **非逃逸类型**：其值携带至少一个来自某 owner 的 loan，值的有效范围被限制在该 loan 之内，不能逃出 owner 的作用域。

- 借用仍是访问模式，**不是类型**（U2）；
- 非逃逸类型是**携带借用的值类型**；
- 来源跟踪、冲突检测与逃逸诊断可以共享，但不假定任何一项已在别处实现。

### 6.2 两级能力

| 级别 | 值包含 | 额外要求 |
|---|---|---|
| **N1a：纯来源** | loan + 定长内联标量 | 无；析构平凡 |
| **N1b：来源加自有元数据** | loan + 一块自有堆存储 | 存储的移交、析构、与 loan 的释放顺序 |

### 6.3 N1a 的规则（首片）

**声明**：类型声明带"非逃逸"标记（语法待定，示意 `nonescaping value class View<T>`）。首片只允许编译器与 std 声明，用户代码不能声明非逃逸类型。

**来源**：每个值携带一条来源链，首片仅支持单一来源链；链式视图继承接收者来源（§5.2）。

**产生**：只能由声明了 `from` 的函数返回，或由对非逃逸值的借用调用返回：

```kotlin
fun <T> List<T>.take(n: Int): View<T> from this
fun <T> View<T>.take(n: Int): View<T> from this
```

**允许的使用位置**：

| 位置 | 是否允许 |
|---|---|
| 表达式链中的临时值（接收者、`for` 的 source、借用参数的实参） | ✅ |
| `borrow val v = expr` 局部绑定 | ✅，持有期到最后一次使用；若尚无最后使用分析，则保守到所在词法块结束 |
| 声明了 `from` 的函数的返回值 | ✅，来源必须是该函数的参数或接收者 |
| 普通 `val` / `var` 绑定 | ❌ 报错，诊断建议 `borrow val` 或 `.toList()` |
| 字段类型、泛型实参、集合元素 | ❌ |
| `own` 形参、escaping closure 捕获、跨线程传递 | ❌ |

**能力限制**：非逃逸类型不是 `Copyable`、不是 `Transferable`、不是 `Shareable`；可空的非逃逸类型（`View<T>?`）首片禁止（U4）。

**泛型**：类型参数默认要求"可逃逸"。因此 `List<View<T>>`、`Option<View<T>>` 一律被拒绝；将来如需对非逃逸类型抽象，再引入显式约束放宽，默认行为不变。

### 6.4 N1b 的附加规则（后续阶段）

N1b 在 N1a 的基础上增加自有存储，必须规定：

1. **存储的所有权**：缓冲区由视图值独占；视图值**不可 move**，只能借用传递，唯一的例外是经 `from` 函数返回，此时所有权随返回值移交给调用者。
2. **释放时机**：缓冲区与 loan 在同一个最后使用点释放；释放顺序先缓冲区，后 loan。
3. **来源保护**：源在视图整个有效期内被冻结，索引不会失效；返回后继续保护由返回值携带的 loan 完成，`from` 声明了保护哪个 owner。
4. **链式**：每层独立持有相对根的索引（§5.2），父层缓冲不被子层引用。
5. **分配失败**：按统一的中止策略。
6. **`borrow val` 的含义**：仍然是"绑定持有 loan"，缓冲区所有权是类型的内部细节，不改变该标记的语义。
7. **诊断**：绑定、存储、逃逸的诊断沿用 N1a，只新增与自有存储相关的消息。

### 6.5 首片范围

| 内容 | 首片 |
|---|---|
| N1a 视图（`take`/`drop`/`dropLast`/切片） | ✅ |
| 消费路径（`consume()` 加算法，立即产出） | ✅ |
| 借用路径 `filter`（N1b） | ❌ |
| 用户自定义非逃逸类型 | ❌ |
| 多来源（`from a, b`），属于 M2B 的另一项能力 | ❌ |
| 非逃逸类型作为另一个非逃逸类型的字段 | ❌ |
| 对非逃逸性做泛型抽象的约束语法 | ❌ |
| 可空的非逃逸类型 | ❌ |

这意味着首片中借用路径的过滤仍然缺失。§12 将"仅限表达式链内临时值的 N1b（N1b-temp）"列为可选的中间步骤。

### 6.6 与现有机制的关系

- 借用闭包与视图**可以复用检查基础**（来源跟踪、冲突、逃逸诊断），但**不能视为同一种已实现机制**：闭包可能捕获多个来源，现有闭包限制也不同于允许 `from` 返回的视图。
- N1 是**独立的前置能力**，有自己的验收，不从 M2B 或借用闭包的完成情况推导；借用闭包是否并入 N1 另行评审。
- 与 M2B 共用 `from` 来源合同，避免两套来源语法；诊断 L0137 是否推广为统一的"非逃逸值逃逸"诊断，诊断码待分配。
- 并发：非逃逸值永远不是 `Transferable`；以后结构化并发作用域内的子任务借用，属于对这一能力的受控放宽，另行评审。

---

## 7. 视图的 loan、逃逸与诊断

| 规则 | 内容 |
|---|---|
| loan | 视图对根 owner 建立 shared loan，存活到视图的最后一次使用（或保守的块末） |
| 源冻结 | loan 存活期间源不能被 move / 改元素 / 扩容，冲突沿用 L0135 |
| 嵌套 | 链式视图继承根来源（§5.2），不依赖父视图存活 |
| 逃逸 | 视图不得 return（`from` 函数的合法返回除外）、写入字段、交给 `own` 参数、被 escaping closure 捕获；诊断推广自 L0137 |
| 绑定 | 普通 `val` 绑定视图报错；必须写 `borrow val` |
| 临时来源 | 来源是语句内的临时值时报错（§5.3） |

```kotlin
fun names(): List<String> = list.take(3)         // ❌ 视图不能逃逸
val v = list.take(3)                             // ❌ 需要 borrow val、.toList() 或内联使用
borrow val v = list.take(3)                      // ✅
// 修复（物化）：list.take(3).toList()
```

本设计**不需要显式生命周期参数、不需要可存储的借用类型、不需要 `Shareable`**，但需要 §6 的非逃逸类型能力。

---

## 8. API 表面与实现载体

链式调用会在视图上继续调方法，因此视图必须覆盖同一套算法：

| 在 `View<T>` 上调用 | 返回 |
|---|---|
| `take` / `drop` / `dropLast` | `View<T>`（继续借用，继承来源） |
| `filter`（N1b，后续） | `View<T>` |
| `map` / `flatMap` | `List<U>`（物化） |
| `fold` / `count` / `any` | 单值 |
| `toList()` | `List<T>`（物化，要求元素可 clone） |
| `size` / 索引 | 借用读取 |

### 8.1 实现载体：依赖受限扩展函数

12 页规定算法"用目标语言实现"（Phase 5），但 `List<T>` / `View<T>` 是编译器绑定的 intrinsic 类型，现行语法无法为它们声明 member。因此算法表面**依赖 [受限扩展函数候选设计](restricted-extension-functions.md)**。下面的接收者模式写法**仅为示意**，应与受限扩展函数提案和当前 guide 对齐：

```kotlin
// 借用接收者，返回携带来源的非逃逸视图
fun <T> List<T>.take(n: Int): View<T> from this
fun <T> View<T>.take(n: Int): View<T> from this

// 取得接收者所有权
fun <T> own List<T>.consume(): ConsumingSeq<T>
fun <T> own ConsumingSeq<T>.filter(pred: (T) -> Boolean): List<T>
fun <T> own ConsumingSeq<T>.take(n: Int): List<T>
```

两条性质要分开看：

- **普通算法**（`count`、`sum`、`joinToString` 等）只使用 receiver 的公开表面，可用纯 `.ko` 实现；
- **构造视图或取得所有权的算法**（`take`、`consume`）还需要编译器提供 intrinsic 构造能力（"按范围建立视图"、"取得容器所有权"），因为它们无法由用户代码构造。这些构造能力应作为可复用的语义原语，而不是按算法名称各开一个 intrinsic，见 [容器 intrinsic 分层提案](container-intrinsics-std-layering-proposal.md)。

声明这些算法的语法能力属于前置阻塞项，不在 Phase 5 内解决。

---

## 9. 取舍与跨语言依据

### 9.1 为什么默认不是拥有值的物化

本设计的默认视图本身就是 eager 的：N1b 的 `filter` 在构造时就扫描一遍并记录索引，谓词调用次数与 Kotlin 一致。真正要避免的不是"立即求值"，而是**默认产生拥有值**，也就是在调用者不知情的情况下复制元素。

#### 各语言的集合元素复制方式

| 语言 | 元素的复制或共享方式 | 对中间集合成本的含义 |
|---|---|---|
| Kotlin | 对象元素通过引用共享，基本类型按值 | 中间集合主要复制引用或标量，由 GC 回收 |
| Swift | 值类型元素按值拷贝，集合本身使用 copy-on-write 延迟拷贝；引用类型元素需要 retain | 成本依元素类型与是否发生写时拷贝而不同，不能统一概括为 O(1) |
| **Koven** | `MoveOnly` 元素没有隐式拷贝；复制必须显式 `clone()`，并且不是所有 `MoveOnly` 类型都存在 `clone()` | 隐式物化要么偷偷 clone（成本不可见），要么对 `MoveOnly` 报错 |

要点：**`MoveOnly` 不等于可深拷贝**。因此"默认 eager 并物化"在 Koven 中必然二选一：对 `MoveOnly` 元素自动 clone（隐藏成本，违反 13 页"禁止把 retain 隐藏在赋值或参数传递中"的取向，且对不可克隆类型无法成立），或对 `MoveOnly` 元素报错（`List<Int>` 可用、`List<String>` 不可用，体验分裂）。本设计的默认视图加显式物化同时避免这两种结果。

Kotlin 与 Swift 的官方文档都把"立即求值"作为集合算法的常态：Kotlin 对多步处理 Iterable 的描述是每一步都立即完成并产生中间集合；Swift 把 `LazySequenceProtocol` 描述为让"通常立即求值的"序列操作变为惰性。这说明 eager 是常态，也说明 Koven 的区别不在求值时机，而在结果是否拥有元素。

#### Rust：入口决定借用/消费，适配器惰性

Rust 把"借用还是消费"放在**入口**：`iter()` / `iter_mut()` / `into_iter()`，适配器本身惰性，`.collect()` 物化。本候选的 `consume()` 对应 `into_iter()`，借用视图对应 `iter()`；区别在于 Koven 把默认放在"借用视图"并要求显式物化，而不是要求每次写入口。

#### Mojo：借用视图已有先例

Mojo 的类型表区分 `String`（owned, heap-allocated）、`StringSpan`（non-owning view）、`StaticString`（`StringSpan` over static read-only data）与 `Span`（non-owning view）。"owning / 借用视图 / 静态"三者并存是已被采用的设计，不是孤例。

#### Zig：不链式

Zig 不做链式适配器，使用显式 `for` 与 `std.ArrayList`。它的价值在于印证"不隐藏分配与控制流"这一取向，不构成本候选的直接对照。

### 9.2 表面代价

本设计让 `val result = list.filter { ... }` 不再合法，对习惯 Kotlin 的用户是显式的偏离：

- 收益：借用、移动、clone 三种成本在调用点各有对应写法；`List<String>` 与 `List<Int>` 行为一致；
- 代价：多一个 `.toList()`、`borrow val` 或 `consume()`；首片中借用路径的 `filter` 缺失；
- 缓解：诊断直接给出修复写法（§1、§7）；`map`、`flatMap` 与链式 `map` 结尾的最常见模式不受影响。

### 9.3 与其他候选的关系

**显式 `clone()`**：物化对 `MoveOnly` 元素需要 [clone()](explicit-clone.md)。`clone()` 以 shared `Borrow` 使用 receiver 并返回新 owner，因此 `list[0].clone()` 可直接用于元素级取出，`map` 内的投影也依赖它。

**String Copyable**：若 `String` 改为 `Copyable`，子序列类算法的物化将不需要 clone，§3.3 与 §4.1 的代价相应消失。但该改动的收益、代价与阻塞见 [String Copyable 候选取舍](string-copyability.md) §3–§6；本候选不以它为前置。

### 9.4 元素按值取出的能力缺口

"从容器取出一个元素并持有"是本议题的另一处表现。现行路径：

| 做法 | 现状 | 结果 |
|---|---|---|
| `MutableList.removeAt` / `removeFirst` / `removeLast` | ✅ 已交付（owned 取出并压缩） | 返回拥有值，越界或空时中止 |
| 移动元素（留洞） | ❌ v1 禁止 partial move | 所有权诊断 |
| 共享元素（`Rc`） | ✅ 但类型噪音大、不可跨线程 | 新 `Rc` handle |
| 元素本身 `Copyable` | ✅ 仅限 `Int`、`Copyable` value class 等 | `val n = intList[0]` 可用 |
| 显式 `clone()` | 候选 | 覆盖面大、不触碰类型系统 |

另有静态字符串类型（字面量专用、无 owner、可 `Copyable`，增量窄）。候选倾向是优先 `clone()`。

### 9.5 `T?` 展平与 `Option<T>`

按 U1，`T?` 展平，嵌套存在性由 `Option<T>` 表达。对本提案的影响：

- 元素类型本身可空（`List<String?>`）不影响 `take`、`map`、`consume` 等算法；
- 缺失属于正常结果的算法（§4.2 的 `find` 等）因此暂不纳入；
- `Map.remove` 等需要三态（缺失 / 移出 null / 移出值）的 API，在 `Option<V>` 就绪前，首片限定 `V` 为非空类型。

---

## 10. 非目标

本文不定义具体算法集合、不规定 Phase 5 的实现顺序、不授权在 v1 内新增任何集合方法，也不提议修改 `Copyable` 定义或引入生命周期参数。不设计用户自定义非逃逸类型、可选借用、惰性消费链、`Map` 算法与迭代器协议。

---

## 11. Phase 边界

| Phase | 需要做什么 |
|---|---|
| 前置 A | [受限扩展函数](restricted-extension-functions.md)：算法声明语法 |
| 前置 B | M2B 来源合同（`from`）与 `borrow val` 局部绑定规则 |
| 前置 C-a | **N1a**：非逃逸标记、来源链、使用位置规则、泛型默认"可逃逸"（独立验收，不由 M2B 或借用闭包推导） |
| 前置 C-b | **N1b**：自有存储的移交、释放顺序、链式根索引（后续，不阻塞首片） |
| 前置 D | 显式 `clone()` 的能力判定（`Copyable` 或 `Clone`） |
| 2 | `View<T>`、`ConsumingSeq<T>` 的类型 identity；算法返回类型规则 |
| 3 | N1a 的 shared loan、源冻结、逃逸与绑定诊断；消费式的 `own` 交付与源失效；§3.2.1 清理与回调合同 |
| 4 | 范围视图表示；物化的分配与元素 clone；消费路径的元素移动；视图与消费序列的 intrinsic 构造；N1b 的索引表示（后续） |
| 5 | 算法实现；容器与视图的同签名表面 |

N1a 随 M2B 之后启动，N1b 不进入首片。

---

## 12. 待决策点

**已在 r3 中收敛**（不再作为开放问题）：物化必须显式；`consume()` 立即取得所有权；首片立即产出；`ConsumingSeq` 首片仅为链内单次使用接收者；局部绑定用 `borrow val`；`T?` 展平；链式视图保存根并且临时 owner 不延寿；N1 是独立前置能力。

**仍待决策**：

1. **N1b-temp 是否作为中间步骤**：只允许 N1b 视图作为表达式链内的临时值（不可 `borrow val` 绑定、不可经 `from` 返回、不可移动），描述符的存储在语句末释放。这样 `list.filter{}.map{}`、`for (x in list.filter{})`、`list.filter{}.toList()` 这些最常见的借用路径用法可以提前可用，而不必等待完整的 N1b 移交规则。
2. **`ConsumingSeq<T>` 的最终命名**，以及后续何时放开绑定与传参。
3. **N1b 的索引表示**：内存布局、分配策略；惰性视图是否在后续阶段引入。
4. **`borrow val` 的借用结束点**：是否在 N1a 就依赖"最后一次使用"分析，还是先保守到词法块末。
5. **"对临时 owner 省略 `consume()`"** 是否作为独立的人体工学优化评审。
6. **物化形式**：是否需要 `toList()` 之外的 `toMutableList()` 等；clone 能力究竟用 `Copyable` 还是独立的 `Clone` 能力。
7. **非逃逸标记的语法**，以及用户自定义非逃逸类型何时开放；借用闭包是否并入 N1。
8. **非局部退出**：谓词内的 `return` / `break` 在什么条件下可以放开。
9. **视图与 `List<T>` 参数的互操作**：传给 `List<T>` 形参是否一律报错并提示 `.toList()`，还是提供受控的物化重载。
10. **缺失属于正常结果的算法**（§4.2）何时与 `Option<T>`、可选借用一起评审。
11. **算法声明载体的前置性**：本候选依赖 [受限扩展函数](restricted-extension-functions.md)。若该前置不被采纳，算法只能改为编译器绑定的 intrinsic member surface，那与 12 页"算法用目标语言实现"冲突，需要重新决策。
