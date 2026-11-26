# L3 试金石程序与 Kotlin 风格人机工程评审（分词器、借用迭代器、区域分配）

> **性质**：非规范候选试金石与人机工程评审 · **状态**：未启用（仅源码评审，不求当前编译器可编译） · **读取时机**：评审路线 L 的 L3（含借用字段的只读视图）及前置泛型子步骤“写出来是否像 Kotlin”，或起草 L3 / TX1 / `Iterator` 正式提案前 · **唯一真源**：本页维护三组 L3 试金石源码与人机工程评审记录；L3 规则见[演进总纲之二](evolution-02-route-loan-kernel.md)，文本视图见[之六](evolution-06-text-globals-extensions-metadata.md)，迭代接口见[之十一](evolution-11-failure-interfaces-units.md) §2.3–§2.4，现行语法基线以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。按[演进总纲之十三](evolution-13-implementation-roadmap.md) §7 第 3 项要求，本页手写三组代表性 L3 程序——**零拷贝分词器**、**借用迭代器**、**区域分配与只读视图**——分别观察**库作者侧**与**应用调用侧**的写法，集中评审一个核心问题：**在不引入显式生命周期参数（`'a`）的前提下，L3 写出来是否像 Kotlin，摩擦落在哪里**。

## 0. 语法基线与候选标注说明

三段程序均以 [Guide v0.47](../guide/README.md) 为表面语法基线（遵守无 `when (val ...)`、枚举/参数列表无末尾逗号、`inout` 仅写声明侧、调用点写 `&`、`borrow` 调用点不写标注等现行规则），仅在 L2 / L3 候选用到的位置使用[之二](evolution-02-route-loan-kernel.md)与[之十一](evolution-11-failure-interfaces-units.md)的占位拼写：

| 候选语法（未启用） | 含义 | 来源 |
|---|---|---|
| `view class Name(borrow val f: T, ...)` | 含借用字段的只读视图类型（`Escapable = false`） | 之二 L3 R3.1 |
| `borrow T?` | 可空共享借用类型（配合 `?.`、`?:`、判空 smart cast） | 之二 L2 R2.1–R2.5 |
| `from field` / `from this` / `from this.origins` | 转发借用字段来源 / 依赖接收者本身 / 转发类型参数或接口携带的全部来源 | 之二 §2、L3 R3.4 |
| `view T`（泛型参数占位） | 显式放开泛型参数默认的 `Escapable` 上界，允许实例化为不可逃逸类型 | 之二 §6、之十一 §2.4 |
| `Step<view T>` 与 `Iterator<view T>` | 不折叠 `null` 的迭代步进和类型与转发式迭代器接口 | 之十一 §2.3–§2.4 |

## 1. 试金石一：零拷贝 CSV / 键值分词器

对应[之二](evolution-02-route-loan-kernel.md) L3 验收项与[之六](evolution-06-text-globals-extensions-metadata.md) TX1：全程不分配新 `String`、不复制字节，直接产出指向源文本切片的 `Token` 视图；同时区分 `Forwards`（`from source`）与 `DependsOn`（`from this`）。

### 1.1 库作者侧源码

```kotlin
// 候选源码（依赖 L2 borrow T?、L3 view class、TX1 StringView）
enum class TokenKind {
    Ident,
    Equals,
    Comma,
    Number
}

// Token 持有不可逃逸的 StringView 字段，因此 Token 自身也是 view class
view class Token(
    val kind: TokenKind,
    val text: StringView,
    val byteOffset: Int
)

view class ConfigTokenizer(
    borrow val source: StringView,
    var cursor: Int
) {
    // 转发合同（Forwards）：返回的 Token 依赖 source，而不依赖 tokenizer 本身。
    // 因此调用方可以继续推进 tokenizer，并同时持有多个 Token。
    inout fun nextToken(): Token? from source {
        this.skipSpaces()
        val len = this.source.byteLength()
        if (this.cursor >= len) {
            return null
        }

        val start = this.cursor
        val ch = this.source.byteAt(start)
        if (ch == 61) { // '='
            this.cursor = start + 1
            return Token(TokenKind.Equals, this.source.sliceBytes(start, start + 1), start)
        }
        if (ch == 44) { // ','
            this.cursor = start + 1
            return Token(TokenKind.Comma, this.source.sliceBytes(start, start + 1), start)
        }

        while (this.cursor < len) {
            val b = this.source.byteAt(this.cursor)
            if (b == 32 || b == 61 || b == 44) {
                break
            }
            this.cursor = this.cursor + 1
        }
        val slice = this.source.sliceBytes(start, this.cursor)
        val kind = if (isAllDigits(slice)) TokenKind.Number else TokenKind.Ident
        return Token(kind, slice, start)
    }

    // 对照（DependsOn）：借用 tokenizer 自身字段，结果不得活过 tokenizer 本身
    fun cursorRef(): borrow Int from this = this.cursor

    private inout fun skipSpaces(): Unit {
        val len = this.source.byteLength()
        while (this.cursor < len && this.source.byteAt(this.cursor) == 32) {
            this.cursor = this.cursor + 1
        }
    }
}

fun isAllDigits(view: StringView): Boolean {
    val len = view.byteLength()
    if (len == 0) {
        return false
    }
    var i = 0
    while (i < len) {
        val b = view.byteAt(i)
        if (b < 48 || b > 57) {
            return false
        }
        i = i + 1
    }
    return true
}
```

### 1.2 应用层调用侧源码

```kotlin
// 应用层：零 from、零生命周期参数、零 String 拷贝
fun parseSinglePair(line: String): Int? {
    var tz = ConfigTokenizer(line.view(), 0)

    val key = tz.nextToken()
    if (key == null) {
        return null
    }
    val eq = tz.nextToken()
    if (eq == null) {
        return null
    }
    val value = tz.nextToken()
    if (value == null) {
        return null
    }

    // 关键检验：key 与 value 同时存活，且 tz 已经多次推进（inout）甚至可以提前死亡
    val valid = when (eq.kind) {
        is TokenKind.Equals -> true
        else -> false
    }
    if (!valid) {
        return null
    }

    // 仅在真正需要比较或解析时读取零拷贝视图
    if (key.text.equalsLiteral("port")) {
        return value.text.toIntOrNull()
    }
    return null
}

// 预期被 L3 拒绝的反例（不应通过借用检查）：
// 1. 缺 from：fun badFirst(line: String): Token? { ... }
// 2. 存入普通类字段：class Saved(val t: Token)              // Token 不可逃逸
// 3. 依赖局部视图：fun badCursor(line: String): borrow Int from line {
//        val tz = ConfigTokenizer(line.view(), 0)
//        return tz.cursorRef()                               // DependsOn(tz)，tz 在返回前死亡
//    }
// 4. 视图存活期间修改来源：持有 key 期间对 var buf: String 重新赋值或 move（L0135）
```

### 1.3 分词器人机工程观察

- **像 Kotlin 的地方**：应用层 `parseSinglePair` 完全没有生命周期注解或 `from`，`Token?` 的判空、`when` 与 `?.` / `?:` 读感与 Kotlin 几乎一致。
- **核心验证点**：`nextToken(): Token? from source` 的转发合同是整个体验成立的关键——如果写成 `from this`，`val key = tz.nextToken()` 之后紧接着调用 `tz.nextToken()`（需要 `inout this`）就会立刻报独占借用冲突。
- **摩擦点**：
  1. 应用层仍需显式写 `line.view()`（按之六 J2 先显式、后加糖）；若长期保留显式 `.view()`，每次传 `String` 给 `StringView` 参数会略显重复。
  2. `Token` 与 `ConfigTokenizer` 都是 `view class`，库作者必须记住“只要字段里含 `StringView`，外层承载结构就必须标 `view class`”。编译器若能给出“字段 `text` 不可逃逸，请将 `class Token` 改为 `view class Token`”的直白提示，心智负担可控。

## 2. 试金石二：借用迭代器与 `for` 循环

对应[之十一](evolution-11-failure-interfaces-units.md) §2.3–§2.4（2026-11-15 裁决：沿用 Kotlin 命名 `Iterable` / `Iterator`，采用不折叠 `null` 的 `Step<T>` 与转发式合同 `from this.origins`），覆盖空序列、仅含 `null`、`null` 之后仍有元素三组边界，并检验**同一套泛型算法同时服务按值迭代与借用迭代**。

### 2.1 库作者侧源码

```kotlin
// 候选源码（依赖 L3 提前子步骤：非逃逸泛型实参 view T、接口方法来源合同 from this.origins）
enum class Step<view T> {
    Yield(value: T),
    Done
}

interface Iterator<view T> {
    // 转发式合同：产出的元素依赖迭代器持有的外部来源，不依赖迭代器本身。
    // 因此调用方可以同时持有多个产出元素，元素也可以活过迭代器。
    inout fun next(): Step<T> from this.origins
}

// 借用外部数组的只读切片迭代器
view class ArrayIter<T>(
    borrow val source: Array<T>,
    var index: Int
) : Iterator<borrow T> {
    // 合同细化：source 被 this.origins 覆盖，满足接口合同
    override inout fun next(): Step<borrow T> from source {
        if (this.index >= this.source.size()) {
            return Step.Done
        }
        val i = this.index
        this.index = i + 1
        return Step.Yield(this.source[i])
    }
}

// 泛型算法只写一次：既可接收按值迭代器（T = String），也可接收借用迭代器（T = borrow String）
fun <view T, I : Iterator<T>> countMatching(
    inout iter: I,
    predicate: (T) -> Boolean
): Int {
    var count = 0
    while (true) {
        val step = iter.next()
        when (step) {
            is Step.Yield -> {
                if (predicate(step.value)) {
                    count = count + 1
                }
            }
            is Step.Done -> return count
        }
    }
}
```

### 2.2 应用层调用侧源码

```kotlin
// 1. 普通 for 消费借用迭代器：与 Kotlin 完全一致
fun totalBytes(names: Array<String>): Int {
    var sum = 0
    for (name in ArrayIter(names, 0)) {
        sum = sum + name.byteLength()
    }
    return sum
}

// 2. 同一泛型算法直接消费借用迭代器（非逃逸 lambda）
fun countLongNames(names: Array<String>, minBytes: Int): Int {
    var iter = ArrayIter(names, 0)
    return countMatching(&iter, { name -> name.byteLength() >= minBytes })
}

// 3. 覆盖之十一 §2.4 必测三边界：空序列、仅含 null、null 之后仍有元素
//    由于 Step<borrow String?> 不折叠 null，Step.Yield(null) 与 Step.Done 严格区分
fun summarizeNullable(items: Array<String?>): Int {
    var nonNullCount = 0
    var nullCount = 0
    var iter = ArrayIter(items, 0)
    while (true) {
        val step = iter.next()
        when (step) {
            is Step.Yield -> {
                val entry = step.value
                if (entry == null) {
                    nullCount = nullCount + 1
                } else {
                    nonNullCount = nonNullCount + 1
                }
            }
            is Step.Done -> break
        }
    }
    return nonNullCount * 1000 + nullCount
}

// 4. 同时持有两个借用元素（验证转发式合同，不需要出借式约束）
fun firstTwoEqual(names: Array<String>): Boolean {
    var iter = ArrayIter(names, 0)
    val s1 = iter.next()
    val s2 = iter.next()
    val first = when (s1) {
        is Step.Yield -> s1.value
        is Step.Done -> return false
    }
    val second = when (s2) {
        is Step.Yield -> s2.value
        is Step.Done -> return false
    }
    return first == second
}
```

### 2.3 借用迭代器人机工程观察

- **像 Kotlin 的地方**：一旦 `for` 绑定 `Iterator`，`totalBytes` 与 `countLongNames` 的应用层代码几乎就是纯 Kotlin；而且 `firstTwoEqual` 能自然同时持有两个产出借用，避免了 Rust `LendingIterator` 在普通遍历上的限制。
- **摩擦点**：
  1. **库作者侧占位拼写最重**：`enum class Step<view T>`、`interface Iterator<view T>`、`from this.origins` 与 `fun <view T, I : Iterator<T>>` 是三组试金石中离 Kotlin 最远的部分。虽然普通业务代码不写这些签名，但写自定义容器/适配器的库作者会频繁遇到，正式提案须谨慎选定 `view T` 与 `this.origins` 的最终拼写。
  2. **手动 `while (true)` + `when (step)` 的样板**：在不走 `for` 时（例如需要提前提取前两个元素或手动拉取），`Step.Yield` / `Step.Done` 比 Kotlin 的 `hasNext()` / `next()` 多一层拆包。这是为了避免 `T?` 折叠（之十一 §2.3 评审 R6）与双方法隐含状态协议付出的合理代价；只要 `for` 与常用组合子由标准库覆盖，应用层很少手写 `Step` 循环。
  3. **出借式迭代明确不在 L3 范围内**：若迭代器内部复用一块缓冲区并交出指向自身缓冲区的借用（`from this`），它无法实现 `Iterator<view T>`（其合同要求 `from this.origins`），符合 2026-11-15 裁决——先不引入第二套接口，留待有真实流式缓冲需求时再增量加入。

## 3. 试金石三：区域分配器与只读节点视图（Arena Allocation）

对应[之五](evolution-05-data-structure-fitness.md) §2 与[之十三](evolution-13-implementation-roadmap.md) §7 第 3 项：展示无生命周期参数下，Koven 推荐的**两阶段区域分配模式**——构建阶段用 `Copyable` 句柄（`NodeId`）表达树/图链接，分析阶段用只读 `ArenaView` + 含借用字段的视图零拷贝读取与遍历。

### 3.1 库作者侧源码

```kotlin
// 候选源码（依赖 Guide v0.47 value class + L3 view class）
value class NodeId(val index: Int)

enum class Expr {
    Lit(value: Int),
    Add(left: NodeId, right: NodeId),
    Neg(inner: NodeId)
}

class ExprArena(private var nodes: MutableList<Expr>) {
    companion {
        fun empty(): ExprArena = ExprArena(MutableList())
    }

    inout fun alloc(own expr: Expr): NodeId {
        val id = NodeId(this.nodes.size())
        this.nodes.add(expr)
        return id
    }

    fun get(id: NodeId): borrow Expr from this = this.nodes[id.index]

    // 冻结为只读分析视图：在视图存活期间，arena 不可再 alloc，但可任意创建多个只读借用视图
    fun view(): ArenaView from this = ArenaView(this)
}

view class ArenaView(borrow val arena: ExprArena) {
    // 转发合同：返回的借用直接依赖底层 arena，不依赖临时的 ArenaView 包装
    fun node(id: NodeId): borrow Expr from arena = this.arena.get(id)

    fun resolve(id: NodeId): ResolvedExpr from arena =
        ResolvedExpr(id, this.arena.get(id))
}

// 同时携带句柄与零拷贝借用指针的分析期视图
view class ResolvedExpr(
    val id: NodeId,
    borrow val expr: Expr
)
```

### 3.2 应用层调用侧源码

```kotlin
// 阶段一（构建）：用 NodeId 句柄建树，alloc 之间零借用冲突
fun buildSampleAst(inout arena: ExprArena): NodeId {
    val a = arena.alloc(Expr.Lit(10))
    val b = arena.alloc(Expr.Lit(32))
    val sum = arena.alloc(Expr.Add(a, b))
    return arena.alloc(Expr.Neg(sum))
}

// 阶段二（求值与分析）：通过只读视图零拷贝访问节点，应用层无 from、无生命周期
fun evalExpr(view: ArenaView, id: NodeId): Int {
    val resolved = view.resolve(id)
    return when (resolved.expr) {
        is Expr.Lit -> resolved.expr.value
        is Expr.Neg -> 0 - evalExpr(view, resolved.expr.inner)
        is Expr.Add -> evalExpr(view, resolved.expr.left) + evalExpr(view, resolved.expr.right)
    }
}

fun runArenaExample(): Int {
    var arena = ExprArena.empty()
    val root = buildSampleAst(&arena)

    // L1 流敏感：view 在最后一次使用后自动结束 loan，随后仍可继续向 arena 追加节点
    val view = arena.view()
    val firstResult = evalExpr(view, root)

    // view 已不再使用，arena 恢复可写（拆成两步避免同表达式内双 inout 接收者冲突）
    val hundred = arena.alloc(Expr.Lit(100))
    val extra = arena.alloc(Expr.Add(root, hundred))
    val secondResult = evalExpr(arena.view(), extra)
    return firstResult + secondResult
}
```

### 3.3 区域分配人机工程观察

- **像 Kotlin 的地方**：`buildSampleAst` 和 `evalExpr` 读起来非常像 Kotlin 的密封类 + 索引池写法；节点定义 `Expr` 本身是完全可逃逸的普通 `enum class`，不会被任何 `'arena` 生命周期参数污染。
- **L1 + L3 的协同收益**：在 `runArenaExample` 中，`view` 借用完 `arena` 后，凭借 L1 流敏感活性，无需手动加额外 `{ ... }` 块即可立刻再次调用 `arena.alloc(...)`。
- **必须诚实标出的边界（非 L3 缺陷，而是安全内存语义边界）**：
  - 上述纯安全 `ExprArena` 底层是可增长 `MutableList<Expr>`，因此**不能**在持有 `borrow Expr from arena` 的同时继续调用 `arena.alloc(...)`（否则扩容重分配会令既有借用悬垂，借用检查器以 L0135 拒绝）。
  - 如果某些编译器/解释器场景确实需要“边追加分配、边长期持有指向旧节点的直接借用”（类似 Rust 的 `bumpalo` / `typed-arena`），则必须依赖稳定分块内存 + 内部可变（阶段三 C2 `RawCell` + U1 `unsafe`），在库内部封装不可移动 chunk 并对外暴露 `&this`（`borrow` receiver）的 `alloc` 方法；这属于阶段三的库实现工作，不要求 L3 改动内核。

## 4. 综合评审结论：写出来是否像 Kotlin？

| 维度 | 分词器 | 借用迭代器 | 区域分配 | 总体评估 |
|---|---|---|---|---|
| **应用层是否像 Kotlin** | 很像（零 `from`，`Token?` 判空与 `when` 自然） | 很像（`for` 消费与非逃逸 lambda 无额外心智） | 很像（`NodeId` + `when` + L1 流敏感免额外作用域块） | **达标**：应用层代码几乎看不到所有权/生命周期噪音 |
| **库作者额外标注** | `view class`、`borrow val`、`from source` | `view T`、`from this.origins`、`Step<T>` | `view class`、`borrow val`、`from arena` | **可控但需定稿拼写**：集中落在库边界，不泄漏到业务层 |
| **暴露的语法/糖需求** | `String -> StringView` 显式 `.view()` 略啰嗦 | `for` 绑定 `Iterator` 是体验分水岭；手写 `Step` 略重 | 纯安全版不支持“边分配边持借用” | 均为已规划项（之六 J2 糖、之十一 `for`、阶段三 C2/U1） |

### 反馈给 L3 / 后续提案的 4 条具体建议

1. **尽早敲定 `view T` 与 `p.origins` 的正式拼写**：在三组试金石中，最容易让库作者觉得“不像 Kotlin”的不是 `from source`，而是泛型参数上的 `view T` 和 `from this.origins`。L3 提案应给出最终拼写候选，并与路线 F 的 F5 语法门一同核验。
2. **明确 `for` 绑定 `Iterator` 与 L3 的同步交付节奏**：若 L3 只交付 `view class` 而不把 `for` 泛化到用户定义的 `Iterator`，借用迭代器在应用层就会退化成手写 `while (true)` + `when (step)`，明显削弱“像 Kotlin”的体感。
3. **诊断文案面向用户动作，而非内部术语**：当用户把 `Token` 或 `ResolvedExpr` 放进普通 `class` 字段时，诊断应直接说明“不可逃逸视图不能存入普通类字段，若需构成组合视图请改用 `view class`”，避免只输出抽象能力名。
4. **评估 `borrow String -> StringView` 隐式转换糖的提前条件**：按之六 J2，TX1 先以显式 `s.view()` 落地是保守稳妥的；若实际分词器/解析器样本中 `.view()` 出现频率过高，可在 L3 稳定后单独立案加糖。
