# AGENT 开发指导文档：Koven 语言设计规范 v0.6（候选，尚未生效）

> **候选状态：本文档尚未生效。当前现行权威规范仍是
> [`agent-language-design-guide-v0.5.md`](./agent-language-design-guide-v0.5.md)。只有用户明确指定
> v0.6 取代 v0.5 后，本文档才获得现行规范地位；文件名版本较新不代表已经批准。**
>
> 本候选完整保留 v0.5 已确定语义，并补齐 Phase 1 表达式与类型引用的可执行语法边界。
> 语法设计原则：
> **尽量贴近 Kotlin 命名与语法习惯**，内存模型为 Rust 式简化所有权/借用，编译器用
> Rust 实现，LLVM 后端。在本候选生效后，v0.5 及更早资料如与本文档冲突，以本文档为准。

> 版本说明：v0.5 当前仍是现行版本；v0.4 与 v0.3 是历史版本。本候选不得用于覆盖 v0.5，
> 也不得作为实现授权，直至用户明确启用。
> 更早的 v0.2 guide、旧技术栈/
> 语言规格以及下文提到的审计报告尚未随当前仓库归档，仅作为历史来源，不参与现行规范
> 优先级。

> 文档记号：示意代码和签名中的 `{ ... }`、`(...)`、`error(...)` 等省略号表示未展开内容，
> 不是 Koven 源码 token；第三部分明确规定 v1 不支持 `...` 运算符。

## 本候选（v0.6）变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 给出可执行的 primary、postfix、prefix、binary 与 assignment 表达式语法，并明确独立表达式入口和插值 stop token | 🔴 语义补全 |
| 2 | 将 v1 中缀调用封闭为 `to`，并分别定义区间、成员关系、比较、相等组的不结合约束 | 🔴 语义补全 |
| 3 | 定义限定路径、递归泛型、单层可空与函数类型组成的无歧义最小 `type_ref` 语法；可空函数类型暂不表达 | 🔴 语义补全 |
| 4 | 明确基本调用仅接受位置实参，索引恰好接受一个表达式，并把命名 / 模式实参与 use-site 类型实参延后 | 🟡 分阶段边界补全 |
| 5 | 定义无 trivia 相邻的不支持运算符组合、Lexer 错误 token 的 parser 消费规则及最小局部恢复类别 | 🟡 诊断与恢复边界补全 |
| 6 | 定义各表达式 AST 节点的合成 `Span` 规则 | 🟡 AST 契约补全 |
| 7 | 将 Phase 1 拆为 SPEC-0007 至 SPEC-0011 的依赖顺序，并同步 Lexer 已完成事实 | 🟡 路线图同步 |

## v0.5 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 补齐 UTF-8、ASCII 标识符、关键字边界和大小写敏感规则 | 🔴 语义补全 |
| 2 | 将已用于闭包和函数类型的 `move` 明确列为硬关键字 | 🔴 冲突修复 |
| 3 | 定义十进制整数 / 浮点、`Char`、单行 `String` 及 `${...}` 插值的最小 v1 词法 | 🔴 语义补全 |
| 4 | 定义 ASCII 空白、LF / CRLF 换行、裸 CR 诊断以及非嵌套行 / 块注释 | 🔴 语义补全 |
| 5 | 定义固定运算符 / 标点集、Phase 5 `@` 预留 token、最长匹配与相邻组合规则 | 🔴 语义补全 |
| 6 | 定义 token / trivia / EOF 的 `Span` 契约与词法错误恢复边界 | 🟡 实现边界补全 |
| 7 | 把 Phase 1 Lexer 验收收敛为可执行的正反例与稳定诊断 | 🟡 路线图同步 |
| 8 | 同步 Phase 0 已完成事实，不把尚未实现的 Lexer / Parser 写入工程骨架验收 | 🟡 已确认事实同步 |

## v0.4 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 将 `value class` 的内联值语义与 `Copyable` 能力分离，允许值类型包含不可复制字段 | 🔴 语义修正 |
| 2 | `value class` 按字段递归、按实际泛型实参自动获得条件 `Copyable`；该预声明 marker trait 可作泛型上界，但 v1 不允许用户手动实现、覆盖或用同名声明冒充 | 🔴 语义补全 |
| 3 | `Pair<A, B>` 不再要求 `A`、`B` 可复制；含 `Sender` / `Receiver` 的 `Pair` 使用移动与消费式解构 | 🔴 冲突修复 |
| 4 | 解构右值只求值一次；不可复制聚合的解构作为一次原子所有权转移，不再按多次独立 `componentN()` 调用解释 | 🔴 语义修正 |
| 5 | 不可复制字段只可投影借用；禁止普通字段部分移动，消费式解构必须完整覆盖全部分量 | 🔴 语义补全 |
| 6 | 明确 `Copyable` 不允许复制 glue 或唯一析构义务；`Box<T>` 在 v1 只接受 `value class` | 🔴 契约补全 |
| 7 | Phase 2–5 的任务与验收同步覆盖条件复制、移动后使用和资源只析构一次 | 🟡 路线图同步 |
| 8 | 测试扩展名统一为 `.ko`，Phase 0 验收改为不依赖临时 parser 的工程骨架验收 | 🟡 已确认事实同步 |
| 9 | 修正示例中遗漏的显式 `Unit` 返回类型，使其符合既有函数签名规则 | 🟢 示例勘误 |

## v0.3 历史变更记录

对照 `agent-language-design-guide-audit.md` 的审计结果逐条修复，编号与审计报告一致：

| # | 变更 | 类型 |
|---|---|---|
| 1 | `thread()` 等跨线程 API 的函数类型参数改为 `move (...) -> T`，强制要求闭包不含借用捕获 | 🔴 修复 |
| 2 | `Indexable<K, V>` 与 `Map` 拆开，`Map`/`MutableMap` 改为独立接口，不复用非空 `get` 签名 | 🔴 修复 |
| 3 | `class Node` 示例去掉多余的 `Box<Node>` 包装，`Box<T>` 重新定位为"把 value class 显式装箱到堆上" | 🔴 修复 |
| 4 | 明确 `dyn`（trait object 动态分发）降级为 v2 特性 | 🔴 修复 |
| 5 | 运算符优先级表按 Kotlin 官方语法核实后重写，Elvis(`?:`) 优先级大幅上调 | 🔴 修复 |
| 6 | `value class` 字段可拷贝规则改为"所有字段类型需满足 `Copyable`"，不再区分 val/var | 🟡 修复 |
| 7 | 不再用"栈分配"描述 `value class`，改用"值语义/内联布局" | 🟡 修复 |
| 8 | `error` 从硬关键字表移除，改为标准库顶层函数 | 🟡 修复 |
| 9 | 补充 `super<Interface>.method()` 的语义（接口默认方法冲突消歧义） | 🟡 修复 |
| 10 | 补充解构声明的 `componentN()` 约定机制 | 🟡 修复 |
| 11 | 补充 `enum class` 变体内部共享方法的语法（`when (this)` 分派） | 🟡 修复 |
| 12 | 智能类型转换（smart cast）列为 Phase 2 显式交付项 | 🟡 修复 |
| 13 | 关键字表补回 `vararg` | 🟢 修复 |
| 14 | 明确 `!!` 保留，定义为 `?: error(...)` 的语法糖 | 🟢 修复 |
| 15 | `own`/`inout`/`borrow` 从通用运算符优先级表移除，改为调用实参位置的专属语法 | 🟢 修复 |
| 16 | Phase 1 验收范例扩充，覆盖 lambda、索引、Elvis 等 | 🟢 修复 |
| 17 | Phase 4 补充闭包环境捕获的 codegen 任务 | 🟢 修复 |
| 18 | Phase 3 补充 `Shareable`/`Transferable` 标记 trait 检查任务 | 🟢 修复 |
| 19 | 补充 `companion object` 作为类型级静态成员机制 | 🟢 修复 |
| 20 | 明确砍掉自定义属性 getter/setter 语法 | 🟢 修复 |
| 21 | Phase 4 调试信息验收标准补充 `lldb` | 🟢 修复 |
| 22 | 全文中英文标点混用问题统一修正 | 🟢 修复 |

---

# 第一部分：核心设计决策

## 1. 基础类型对齐 Kotlin，新增 `object`

采用与 Kotlin 完全一致的基础类型命名：

| 类型 | 说明 |
|---|---|
| `Byte`, `Short`, `Int`, `Long` | 有符号整数，8/16/32/64 位，默认整数字面量类型是 `Int` |
| `UByte`, `UShort`, `UInt`, `ULong` | 无符号整数 |
| `Float`, `Double` | 32/64 位浮点，默认浮点字面量类型是 `Double` |
| `Boolean` | 布尔 |
| `Char` | 单个 Unicode 标量值 |
| `String` | UTF-8，不可变 |
| `Unit` | 无返回值 |
| `Nothing` | 不返回的函数标注，`error()` 的返回类型即为 `Nothing` |
| `Any` | 所有类型的顶层类型（泛型上界默认约束，非 Java `Object` 那种运行时反射基类） |

**`object` 关键字**（单例声明）：

```kotlin
object Config {
    val version: String = "1.0"
}
```

`object` 编译为编译期确定的单一静态实例。**v1 仅支持内部属性为编译期可求值（`const`/字面量/常量表达式）的 `object`**，避免运行时懒加载初始化竞争带来的实现复杂度；运行时惰性初始化的 `object` 列为 v2 特性。

## 2. 关键字与保留字

见第三部分完整表格。

## 3. `error()` 与空安全相关运算符

`panic()` 采用 Kotlin 命名 `error()`：

```kotlin
fun divide(a: Int, b: Int): Int {
    if (b == 0) error("division by zero")
    return a / b
}
```

- `error` **不是关键字，是标准库顶层函数**：`fun error(message: String): Nothing`，和 `println` 地位相同（v0.2 曾把它列为硬关键字，这里已改正——`Nothing` 返回类型的特殊处理是类型系统对所有返回 `Nothing` 的函数都生效的通用规则，不需要绑定在特定函数名上，这样用户也能像真实 Kotlin 里一样自由使用 `error` 作标识符，只是不建议这么做）。
- 语义：**不是抛出可捕获异常**，而是终止进程（abort），这是与真实 Kotlin `error()`（抛 `IllegalStateException`，可 catch）语义上的唯一差异，需要在教程里明确标注。
- `Nothing` 参与 bottom-type 类型推导：`if` 一个分支返回 `Nothing`，另一分支返回 `T`，整体类型推导为 `T`。

**非空断言 `!!` 保留**，定义为纯语法糖，不引入新的运行时机制：

```kotlin
val len = name!!.length
```

脱糖规则：`e!!` 等价于 `e ?: error("Non-null assertion failed")`，类型从 `T?` 收窄为 `T`。因为最终还是落到 `error()`（abort 语义），实现成本几乎为零，同时保留了 Kotlin 开发者熟悉的写法。

## 4. 高阶函数与一等公民支持

```kotlin
val double: (Int) -> Int = { x -> x * 2 }

fun apply(f: (Int) -> Int, x: Int): Int = f(x)

fun makeAdder(n: Int): (Int) -> Int {
    return { x -> x + n }
}

val ref = ::globalFunction
val boundRef = obj::method
```

- 函数类型 `(ParamTypes) -> ReturnType` 是一等类型。
- 单态化为具体闭包结构体（捕获环境 + 函数指针），无捕获的函数值直接是裸函数指针，零成本抽象。
- 函数类型可以带 `move` 前缀（`move (ParamTypes) -> ReturnType`），表示只接受不含任何借用捕获的闭包，详见第 9 节。
- `::globalFunction` 与 `obj::method` 在 Phase 1 只建立未绑定 / 绑定 callable reference AST；
  名称是否存在、可见性、重载选择和最终 callable 类型都由后续名称解析与类型检查判定。

## 5. `value class` 替代 `struct`

```kotlin
value class Point(val x: Int, val y: Int)      // 内联值语义；字段均可复制，因此 Point 可复制

class Node(var value: Int, var next: Node?)    // 引用语义：堆分配，遵循所有权/借用规则
```

- **值语义描述布局与身份，不承诺复制**：`value class` 是没有独立对象身份的内联聚合。
  作为局部变量时通常位于栈帧中；作为字段、数组元素或其他聚合的分量时直接内联在容器
  布局中。它不是“永远在栈上”，也不因采用值语义就必然可复制。
- **声明合法性与可复制能力分离**：`value class` 可以包含不满足 `Copyable` 的字段。类型在
  且仅在其全部字段类型都满足 `Copyable` 时，由编译器自动标记为 `Copyable`。泛型
  `value class` 按实际类型实参计算该条件，例如 `Pair<Int, Int>` 可复制，
  `Pair<Sender<Int>, Receiver<Int>>` 不可复制。字段是 `val` 还是 `var` 不参与判定。
- **v1 的 `Copyable` 是编译器预声明的规范 marker trait**：数值类型、`Boolean`、`Char` 和
  `Unit` 满足 `Copyable`；`value class` 按上一条规则递归获得该能力。该规范能力由编译器
  内部标识识别，用户同名声明不能冒充。用户可以把它写成泛型上界，例如
  `<T : Copyable>`，并在该泛型体内把约束作为复制能力的证明；未约束的 `T` 一律不得假设
  为 `Copyable`。v1 不提供用户手动实现、否定实现或覆盖自动推导的语法。泛型
  `value class` 的条件能力是字段类型形成的约束谓词，在具体类型实参已知时求值。
- **复制契约**：`Copyable` 表示一个有效值可以无用户可观察的 copy / retain / clone glue，
  按字段递归复制；实现可以在布局允许时用等价的按位复制优化。满足该能力的类型不得承担
  唯一资源释放或其他非平凡析构义务。需要引用计数递增、深拷贝、用户 copy hook 或独占
  drop 的类型均不得满足 `Copyable`。标准库 Spec 指定任何额外 `Copyable` 类型时也必须满足
  这份契约，不能把 marker 当作绕过所有权检查的白名单。
- **使用规则由能力决定**：满足 `Copyable` 的值在赋值、返回或传给取得所有权的参数时可以
  隐式复制，原值仍可使用；不满足 `Copyable` 时，同样的位置转移所有权，原值随后不可使用。
  调用实参仍须遵守 `own` / `borrow` / `inout` 的专用语法，不因类型可复制而省略参数模式。
  因此 `consume(own x)` 对 `Copyable` 的 `x` 交付一个 owned copy，对不可复制的 `x` 则移动
  原值。
- **字段访问不允许隐式部分移动**：`aggregate.field` 是一个 place。`Copyable` 字段可复制
  读取；字段 place 可以在调用实参中被 `borrow` 或（字段可变时）被 `inout` 借用。v1 禁止
  用普通字段读取或 `own aggregate.field` 从聚合中移出不可复制字段。不可复制聚合只能整体
  移动，或按第 11 节的完整结构解构一次性消费；不能产生需要追踪“哪些字段已经移走”的
  部分移动状态。
- 普通 `class` 是堆分配的引用语义值，本身不满足 `Copyable`，转交所有权时发生移动；它与
  非 `Copyable value class` 都受移动后使用检查约束，二者差异在布局而不在“一个复制、一个
  移动”的固定分类。

**`Box<T>` 的定位**：既然 `class` 本身已经是堆分配引用类型，v1 的 `Box<T>` **只允许 `T`
是 `value class`**；`Box<Node>` 这类把普通 `class` 再包一层的类型实例化必须产生类型错误。
`Box<T>` 是不可复制的独占所有权类型，用于把一个 `value class` 实例显式搬到堆上。装箱
取得传入值的所有权；`own` 对可复制值交付 owned copy，因此源值仍可用，对不可复制值则
发生移动：

```kotlin
val point = Point(1, 2)
val boxedPoint: Box<Point> = Box(own point)   // Point 可复制；point 仍可使用
println(point.x)

value class Endpoint(val sender: Sender<Int>)
val endpoint = Endpoint(own sender)
val owned: Box<Endpoint> = Box(own endpoint) // Endpoint 不可复制；这里移动 endpoint
// 此后再次使用 endpoint 是移动后使用错误
```

## 6. 函数默认实现保留，去掉返回类型推导

```kotlin
interface Shape {
    fun area(): Double
    fun describe(): String = "a shape"   // 接口默认方法实现，保留
}

fun add(a: Int, b: Int): Int = a + b     // 表达式体语法保留，返回类型必须显式写出
```

**规则：所有函数签名（含表达式体 `= expr` 形式）必须显式声明返回类型，不做函数级返回类型推导。** 局部变量 `val`/`var` 的类型推导保留。

## 7. `public` 替代 `pub`；`enum class` 语法融合 Kotlin 命名与 Rust ADT 能力

```kotlin
public class Circle { ... }
internal fun helper(): Unit { ... }
private fun impl(): Unit { ... }
```

`enum class` 采用 Kotlin 命名，但赋予 Rust 风格的"每个变体带不同关联数据"的能力：

```kotlin
enum class Shape {
    Circle(radius: Double)
    Rectangle(w: Double, h: Double)
    Point

    fun area(): Double = when (this) {
        is Circle -> 3.14159 * radius * radius
        is Rectangle -> w * h
        is Point -> 0.0
    }
}
```

- **`enum class` 内部可以直接定义共享方法**（如上面的 `area()`），方法体内通过 `when (this)` 对各变体分派，依赖第 12 节的智能类型转换在每个分支里访问该变体专属的字段（如 `radius`、`w`、`h`）。这一点在真实 Kotlin 的 `enum class` 里做不到（要用更繁琐的 `sealed class` 才行），是本语言相比 Kotlin 的核心增强点，教程里需要重点说明，避免 Kotlin 背景的开发者按固有认知（每个变体只是单例常量）理解。
- `when` 表达式对 `enum class` 做穷尽性检查（强制覆盖所有变体，除非有 `else`）。

## 8. 索引访问语义（Array / List / Map）

```kotlin
interface Indexable<K, V> {
    operator fun get(key: K): V
}

interface MutableIndexable<K, V> : Indexable<K, V> {
    operator fun set(key: K, value: V)
}
```

`Indexable`/`MutableIndexable` 只用于 `Array`/`List`/`MutableList` 这类"非空返回、越界即错误"的容器。**`Map`/`MutableMap` 不实现这两个接口**，因为它们的语义是"可空返回、永不报错"，和 `Indexable` 的契约不兼容——这是对 v0.2 里"共用一个接口"设计错误的修正：

```kotlin
interface Map<K, V> {
    operator fun get(key: K): V?       // 键不存在返回 null，不报错
    fun getValue(key: K): V             // 键不存在则 error()
}

interface MutableMap<K, V> : Map<K, V> {
    operator fun set(key: K, value: V)
}
```

具体规则：

| 类型 | 越界/缺失行为 | 安全访问方式 |
|---|---|---|
| `Array<T>` / `List<T>` | `arr[i]` 越界触发 `error()` | `arr.getOrNull(i): T?` |
| `MutableList<T>` | `list[i] = v` 越界同样 `error()` | — |
| `Map<K, V>` | `map[key]` 返回 `V?`，键不存在返回 `null` | `map.getValue(key): V` 键不存在时 `error()` |
| `MutableMap<K, V>` | `map[key] = v` 插入或覆盖，不会失败 | — |

索引括号内语法上接受任意单个表达式，因此 `arr[1..3]` 在 Phase 1 被解析成“以 range
表达式为单个 key 的索引”，**不产生切片 AST 或切片语义**。Phase 2 中，`Array` / `List`
要求整数 key，因类型不匹配拒绝该写法；若容器本来就是 `Map<Range<...>, V>`，range 可作为
普通 key。区间切片语义仍列为 v2 特性。

## 9. 并发模型：v1 线程 + channel，跨线程闭包强制 `move`

```kotlin
val handle = thread(move { println("running in new thread") })
handle.join()

val (sender, receiver) = channel<Int>()
thread(move { sender.send(42) })
val value = receiver.receive()
```

**关键修正**：`thread()` 的函数类型参数标注为 `move () -> Unit`（而非普通 `() -> Unit`）。函数类型可以带 `move` 前缀，表示**只接受不含任何借用捕获的闭包**——编译器在调用点强制要求实参闭包字面量显式带 `move` 关键字，且检查该闭包体内不能捕获任何借用语义的外部变量。这对应 Rust `std::thread::spawn` 要求闭包满足 `'static` 的设计初衷：一个借用捕获的闭包若被传进新起的 OS 线程，原函数栈帧完全可能在子线程还在跑的时候就已经返回销毁，产生悬垂引用。所有跨线程 API（`thread`、`spawn` 类）的函数类型参数都必须用 `move (...)  -> T` 标注，这是所有权模型在多线程场景里真正发挥作用的地方，不能留空子。

- 配合 `Shareable`/`Transferable` 标记 trait 做编译期数据竞争防护（跨线程传递的值必须满足对应约束）。**v1 只做标记 trait 检查，不做完整的多线程借用数据流分析**（列为 v2），具体任务见 Phase 3。
- 协程明确排到 v3，技术路线推荐 Rust 式 `async`/`await` + `Future` 状态机而非 Kotlin 式 `suspend` + CPS 变换（后者与借用检查器交互过于复杂）：

| 方案 | 优点 | 缺点 | 建议 |
|---|---|---|---|
| Kotlin 式 `suspend` + CPS 变换 | 语法直观 | 与借用检查器交互复杂，实现难度最高 | 不采用 |
| Rust 式 `async`/`await` + `Future` | 与所有权模型天然契合，零成本抽象 | 需要引入 `Future<T>` 和执行器概念 | **推荐**，v3 目标 |
| 有栈协程（stackful） | 实现相对简单 | 运行时需管理独立栈，和 FFI/OS 线程交互复杂 | 备选方案 |

v1/v2 不做协程，先用线程池 + 阻塞 IO 覆盖常见场景。

## 10. IO / 网络与其他基础设施

```kotlin
val content = File.readText("path/to/file")
File.writeText("out.txt", "hello")

val reader = BufferedReader(File.open("data.csv"))
for (line in reader.lines()) { ... }
```

- v1 只做**同步阻塞 IO**（文件、网络），网络 IO 和线程模型配套使用（每连接一线程）。异步 IO 依赖第 9 节的协程决策，v3 再做。

| 主题 | 决策 |
|---|---|
| 泛型型变（variance） | v1 泛型全部不变型（invariant），不支持 `in`/`out` 声明处型变，列为 v2 |
| 反射 | 不做任何运行时反射/RTTI，`is`/`as` 只在编译期已知的类型层级内工作 |
| 内存分配器 | v1 直接用系统分配器（libc `malloc`/`free`），自定义 allocator trait 列为 v2+ |
| 诊断错误码 | 参考 rustc 的 `E0382` 风格，从 v1 起给每类编译错误分配稳定错误码（如 `L0001`） |
| 编辑器语法高亮 | LSP 之外单独提供 TextMate/Tree-sitter 语法文件 |
| 自举计划 | 标准库从第一天用目标语言自身写；编译器本身"自举"列为长期目标（v4+） |
| 数组/集合字面量 | `arrayOf(1, 2, 3)`、`listOf(1, 2, 3)`、`mapOf("a" to 1)`（`to` 中缀函数构造 `Pair`） |
| 中缀函数（infix） | v1 表达式语法中的中缀调用集合精确封闭为软词 `to`；`infix` 自身只是普通标识符，后续仅可在标准库声明上下文解释，用户代码不开放自定义 `infix fun` |

## 11. 解构声明与 `componentN()` 约定

```kotlin
value class Pair<A, B>(val first: A, val second: B)

fun <T> channel(): Pair<Sender<T>, Receiver<T>> { ... }

val (sender, receiver) = channel<Int>()
```

`Pair<A, B>` 对任意合法的 `A`、`B` 都可以实例化，不要求类型实参满足 `Copyable`。它仅在
`A`、`B` 都满足 `Copyable` 时自动满足 `Copyable`，因此上面的 channel 返回值合法但不可
复制。

解构沿用 Kotlin 风格的 `componentN()` 命名约定。对所有类型，`val (a, b) = e` 都必须先把
`e` **只求值一次**并保存为编译器内部临时值；带副作用的右值不得因分量数量重复执行。随后
按源类型分成两条互不混用的规则。

对 `value class`，编译器使用内建结构解构，不展开为普通方法调用：

1. 编译器把所有分量绑定作为一次结构化操作检查。若源类型满足 `Copyable`，绑定获得分量
   的复制，源值仍可使用。
2. 若源类型不满足 `Copyable`，解构消费整个源值并一次性转移各分量的所有权；操作完成后
   源值不可使用，所有拥有资源的字段最终只能析构一次。这里的“一次性”是所有权检查与
   lowering 的原子边界，不允许观察或使用半解构状态。
3. v1 的消费式结构解构必须覆盖主构造器的全部分量，并且每个分量恰好绑定一次；部分结构
   解构不支持。占位、跳过或丢弃分量的语法尚未定义，不得自行把它们解释为隐式移动或析构。
   `val (a, b) = pair` 对两字段 `Pair` 是完整解构。

- 编译器按 `value class` 主构造参数的声明顺序提供结构分量，逻辑名称为 `component1()`、
  `component2()` 等；完整解构可使用上述编译器内建的结构化操作，不要求先生成一串普通
  方法调用。
- 对单独的 `x.componentN()` 调用：只有对应分量类型满足 `Copyable` 时，自动分量才可从
  `x` 复制返回。不可复制分量不能通过一次独立调用从聚合中移出；应使用消费式结构解构，
  防止产生部分移动状态。
- 普通字段访问遵循第 5 节的 place 规则，同样不能绕过上述限制移出不可复制分量。
- 非 `value class` 若要支持解构，需要显式提供相应 `componentN()` 方法。在右值求值一次
  后，编译器按绑定顺序对隐藏临时值各调用一次 `componentN()`；返回值和 receiver 的所有权
  完全按这些普通方法的签名与调用规则检查。它不自动复制或消费整个源值，也不获得
  `value class` 的原子聚合拆分能力；若前一个调用的所有权效果使后续调用非法，应产生正常
  所有权诊断。

## 12. 类型级静态成员：`companion object`

```kotlin
class Point(val x: Int, val y: Int) {
    companion object {
        fun origin(): Point = Point(0, 0)
    }
}

val p = Point.origin()
```

没有二级构造器、没有 `init` 块的情况下，类型级别的常量/工厂函数通过 `companion object` 表达。**限制与第 1 节的 `object` 一致：v1 只支持内部成员为编译期可求值的 `companion object`**，运行时状态列为 v2。

## 13. 明确砍掉：自定义属性 getter/setter

真实 Kotlin 的 `val x: Int get() = ...` / `var y: Int set(value) { ... }`（自定义属性访问器）**在本语言 v1 中不支持**，和扩展函数一样列入"去掉的语法糖"清单：属性访问始终直接对应存储字段，不允许拦截读写。原因：自定义访问器要求类型检查器处理访问器返回类型与声明属性类型的一致性校验、`field` 关键字访问底层存储等额外机制，复杂度收益比低，与"去掉语法糖、保留核心"的项目定位一致。

## 14. `super` 关键字的实际用途

由于不支持类实现继承，`super` 的唯一用途是**给多个接口的同名默认方法做冲突消歧义**：

```kotlin
interface Logger { fun log(msg: String): Unit = println("[Logger] ${msg}") }
interface Auditor { fun log(msg: String): Unit = println("[Auditor] ${msg}") }

class Service : Logger, Auditor {
    override fun log(msg: String): Unit {
        super<Logger>.log(msg)
        super<Auditor>.log(msg)
    }
}
```

`super<InterfaceName>.method()` 语法直接对齐 Kotlin 的实际机制。如果一个类实现的多个接口存在同名默认方法，编译器强制要求显式 `override` 并在方法体内用 `super<X>` 消歧义，否则报错。

## 15. `dyn`（trait object 动态分发）降级为 v2

早前文档中曾出现"`dyn Shape` 语法用于异构集合的动态分发"的表述，与关键字表状态不一致。**明确结论：`dyn` 是保留关键字，但对应的动态分发语法在 v1 不实现**，v1 泛型/接口全部走单态化静态分发。如需要异构集合场景，v1 阶段用 `enum class` 包装各具体类型来模拟（这也是 Rust 在没有 trait object 时的常见替代方案）。

---

# 第二部分：核心结构声明总览

```kotlin
value class Point(val x: Int, val y: Int)                  // 内联值语义；字段均 Copyable

class Node(var value: Int, var next: Node?)                // 引用语义，堆分配

interface Shape {
    fun area(): Double
    fun describe(): String = "a shape"
}

enum class Result<T, E> {
    Ok(value: T)
    Err(error: E)
}

object Config {
    val version: String = "1.0"
}

class Counter(var count: Int) {
    companion object {
        fun zero(): Counter = Counter(0)
    }
}

fun <T : Comparable<T>> max(a: T, b: T): T = if (a > b) a else b
```

---

# 第三部分：完整词法规范

## 1. 硬关键字（42 个，按用途分类，不可作为标识符）

**声明相关**
```
class       companion   const       enum        extern
fun         import      interface   module      object
typealias   val         value       var         vararg
```

**控制流**
```
break       continue    else        for         if
in          is          loop        return      when
while
```

**所有权 / 借用 / 安全**
```
borrow      inout       move        own         unsafe
```

**可见性**
```
internal    private     public
```

**其他（字面量 / 表达式相关）**
```
as          false       null        operator    override
super       this        true
```

> `error` **不在此表中**——它是标准库顶层函数，不是关键字（第一部分第 3 节）。

## 2. 软关键字（仅特定上下文有特殊含义，其余场景可作普通标识符）

```
to          infix（仅标准库内部使用）
```

两者在 lexer 中都始终是普通 `Identifier`。parser 只在第四部分规定的表达式位置把拼写 `to`
解释成中缀运算符；`infix` 在表达式中仍是标识符，只能由后续标准库声明语法在其专用上下文
解释。

> v0.2 中的 `get`/`set` 已从软关键字表移除：随着自定义属性访问器被砍掉（第一部分第 13 节），`get`/`set` 只会出现在 `operator fun get(...)`/`operator fun set(...)` 声明里，此处 `operator` 关键字已足够触发特殊解析，`get`/`set` 本身不再需要额外的上下文关键字地位。

## 3. 保留但当前版本未使用（预留给 v2/v3，禁止用作标识符）

```
async       await       suspend     actor       spawn
sealed      dyn         where       yield       macro
reify
```

> `dyn` 已在第一部分第 15 节明确说明：关键字保留，但对应语法降级到 v2，v1 不实现。

> Agent 注意：即便这些保留字当前版本没有对应语法功能，**lexer 阶段也应将其识别为保留标识符并拒绝用户用作变量/函数/类型名**，防止未来版本引入新语法时出现向后兼容问题。

## 4. 源文本与标识符

- Koven 源文件必须是有效 UTF-8。文件加载器在进入 lexer 前拒绝非 UTF-8 字节；lexer
  处理 Unicode scalar 并使用已有的 UTF-8 字节 `Span`。
- 非转义标识符的首字符匹配 ASCII `[A-Za-z_]`，后续字符匹配 ASCII
  `[A-Za-z0-9_]*`。单独 `_` 也是普通标识符；v1 不为它赋予丢弃或通配含义。
  Unicode scalar 仍可出现在注释、`Char` 和 `String` 中，但不能组成标识符。
- 标识符大小写敏感，不做 case folding。
- lexer 必须先扫描完整标识符，再与硬关键字和未来保留字做大小写敏感的
  精确比较。例如 `class` 和 `className` 分别是硬关键字和标识符；`classβ` 扫描为
  硬关键字 `class` 后跟非法字符 `β`，不得把整段当作 Unicode 标识符。
- 硬关键字产生对应 keyword token。软关键字在 lexer 中仍产生普通 identifier token，
  由 parser 在已定上下文解释。未来保留字当前没有合法语法位置；lexer 每次遇到
  都产生“未来保留字不可用”诊断，同时保留 reserved-word token 以便恢复。
- `error`、`get`、`set`、基础类型名和 `Copyable` 都按普通标识符扫描。v1 不支持
  反引号或其他转义标识符，也不允许通过转义绕过硬关键字或保留字。

后续若扩展为 Unicode 标识符，必须由新 guide 明确字符属性、Unicode 数据版本、规范化和
升级兼容策略，不得通过宿主语言或依赖版本静默扩大合法标识符集合。

## 5. Trivia、换行与注释

- 普通词法模式中的 trivia 包括 ASCII space `U+0020`、tab `U+0009`、换行以及注释。
  其他 Unicode 空白字符不是隐式分隔符，按非法字符处理；这条限制不把注释、`Char`
  或 `String` 内容中的同一 scalar 重新解释为 trivia 或非法字符。
- 换行只可写为 LF 或 CRLF，每个序列是一个 newline trivia。裸 CR 产生非法字符诊断，且
  不单独开始新行；这与现行 source / `Span` 位置模型一致。换行不会自动插入分号，
  v1 也不接受源码分号。parser 通过语法边界而不是换行结束语句。
- `//` 开始行注释，直到 CR、LF 或 EOF 之前；终止换行不属于注释 token。
- `/*` 开始块注释，并由遇到的第一个 `*/` 结束；块注释不嵌套。v1 不区分 doc
  comment；`///`、`/** ... */` 与普通注释相同。未终止块注释产生对应词法诊断。
- 普通词法模式不接受 BOM 或 shebang；其中不属于任何合法 token 的字符产生非法字符
  诊断。注释或字面量内容中的 `U+FEFF` 仍按该模式的普通内容处理。
- lexer 保留每个 trivia 及其 `Span`：连续 space / tab 合并为一个最大 whitespace
  trivia；每个 LF / CRLF、行注释和块注释分别形成独立 trivia。parser 可以确定性跳过，
  后续 formatter 可以复用；trivia 不会附着到 AST 节点或改变表达式语义。

## 6. 字面量

### 布尔与空值

`true`、`false` 和 `null` 是硬关键字 token，不由通用标识符或数字规则解释。

### 整数与浮点

- v1 整数字面量是一个或多个 ASCII 十进制数字 `[0-9]+`。前导零不改变进制；
  负号始终是独立 `-` token。
- v1 浮点字面量是 `[0-9]+ '.' [0-9]+`。因此 `1.0` 合法，`.5` 和 `1.` 不是
  浮点 token。
- 数字扫描在范围运算符前停止；`1..2` 必须切分为 `1`、`..`、`2`，`1..<2`
  切分为 `1`、`..<`、`2`。
- v1 不支持十六进制、二进制、八进制、指数、数字分隔下划线或整数 / 浮点类型后缀。
  lexer 保留原始文本，不进行溢出、默认类型或目标类型判定；这些属于后续阶段。
- 扫描完整数或浮点候选后，如果紧邻 ASCII `[A-Za-z0-9_]`（如 `1e3`、`1.0e3`、
  `0x10`、`1L`、`1_0`），lexer 把完整数字候选及其后的最大连续 ASCII
  `[A-Za-z0-9_]*` 后缀合为一个非法数字区域并产生诊断，不得静默拆成合法数字和标识符。

### `Char`

- `Char` 字面量由单引号包围，解码后必须恰好是一个 Unicode scalar。
- 可用转义是 `\\`、`\'`、`\"`、`\n`、`\r`、`\t` 和 `\0`。v1 不定义数字形式的
  Unicode 转义。
- 空字符、多个 scalar、未知 / 非法转义、未闭合字面量以及未转义 CR / LF 都产生
  `Char` 字面量诊断。

### `String` 与插值

- 常规字符串由双引号包围，不得包含未转义 CR / LF。可用转义与 `Char` 相同，
  并额外允许 `\$`。v1 不支持三引号 / raw / 多行字符串。
- `${` 开始字符串插值表达式，与它嵌套深度匹配的 `}` 结束插值并返回字符串
  模式。插值内使用完整普通 token 规则，并可包含块、调用或嵌套字符串；只有插值普通
  模式产生的 `{` / `}` 改变该层深度，注释、`Char` 和嵌套字符串中的花括号不参与匹配。
- `$name` 简写不属于 v1；未紧跟 `{` 的 `$` 是普通字符串文本。`\$` 可在需要明确表达
  字面 `$` 时使用，尤其可用 `\${` 表示不会开始插值的字面 `${`。
- lexer 保留 string-start、string-text、interpolation-start、插值内普通 token、
  interpolation-end 和 string-end 的独立 `Span`。转义的值解码属于 parser / literal 边界，
  token 保留原始源文本。每个 string-text 是结束引号、`${`、非法转义或未转义 CR / LF
  之间的最大非空区域；合法转义保留在所属 string-text 中，不产生空 text token。
- 未闭合字符串在该字符串开始到未转义 CR / LF 之前或 EOF 的范围产生未终止字符串诊断，且
  不消费换行。插值表达式内的换行按普通 trivia 处理；只有到 EOF 仍未找到与 `${`
  匹配的 `}` 时，才以该 `${` 到 EOF 为主 `Span` 产生未终止插值诊断。EOF 处存在
  多层未闭合词法模式时只报告最内层错误，不再为外层字符串 / 插值产生级联诊断。
  非法转义产生独立的字符串转义诊断。

## 7. 固定运算符、标点与最长匹配

lexer 识别下列固定符号：

```text
( ) [ ] { } , : @
. ?. ? ?: !! !
:: ->
* / % + -
.. ..<
< > <= >= == !=
&& ||
+= -= *= /= %= =
```

- 在同一起点可匹配多个固定符号时必须取最长匹配。因此 `..<`、`?.`、`?:`、
  `!!`、`::`、`->`、`<=` 和复合赋值均不得拆分。注释开始符 `//` / `/*` 优先于
  单个 `/` 或 `/=` 规则。
- `as?`、`!in`、`!is` 是需要字符相邻的复合 token；`as ?`、`! in`、`! is` 按各自
  token 扫描。`!in` / `!is` 只有在 `in` / `is` 后为 EOF 或下一 scalar 不匹配
  ASCII `[A-Za-z0-9_]` 时才匹配；
  `!inside` 是 `!` 加 identifier `inside`。
- `(` / `)` 和 `[` / `]` 始终是各自独立的 delimiter token，不是成对复合 token。
- `own`、`inout`、`borrow`、`move`、`as`、`in`、`is` 的 keyword token 只记录词法分类；
  它们的合法语法位置由 parser 决定。
- `@` 是为 Phase 5 内建 `@Test` 预留的单字符 token；v1 不因此开放通用注解语法，
  在后续 guide 定义 `@Test` 的语法位置前，parser 应拒绝任何 `@` 用法。
- v1 不支持分号、`++`、`--`、shift / bitwise 运算符、`#`、shebang 或 `...`；
  它们不得因 Kotlin 中存在而被默认接受。若其中字符各自是合法固定符号（如 `++`、
  `--`、`<<`、`...`），lexer 只产生逐个最长合法 token，由 parser 拒绝该组合；没有单字符
  token 的 `;`、`#`、单个 `&` / `|` 等产生非法字符诊断。

## 8. Token、`Span`、EOF 与错误恢复

- 每个普通 token、trivia、string segment 和 invalid token 都保留其原始非空 `Span`。EOF 是唯一
  固定的空 token，范围为 `[source.len(), source.len())`。空文件只产生 EOF。
- 所有 lexeme 按源文本顺序返回，它们的 `Span` 不重叠且联合覆盖 EOF 之前的全部有效
  UTF-8 字节。该顺序不依赖 hash 集合、文件路径或加载顺序。
- lexer 返回 token 序列和结构化诊断序列；发现一条非法用户输入时不得 `panic!`。
  诊断按既有全序输出，且每条至少有稳定错误码、主消息与精确主 `Span`。

词法错误类别和恢复边界如下。实施 Spec 必须在集中目录为每一行分配独立稳定错误码，不得
把不同含义复用为同一错误码：

| 类别 | 主 `Span` 与恢复 | Lexeme 形态 |
|---|---|---|
| 非法字符 | 覆盖并消费一个不属于合法 token 的 Unicode scalar；裸 CR 也按此处理 | 同范围 invalid |
| 未来保留字 | 覆盖并消费完整保留字 | 同范围 reserved-word token，不另造 invalid |
| 未终止块注释 | 从 `/*` 覆盖并消费到 EOF | 同范围 invalid，不保留 block-comment trivia |
| 未终止字符串 | 从开始引号覆盖到未转义 CR / LF 之前或 EOF，不消费换行 | 保留所有已产生的 string / interpolation lexeme，不另造重叠 invalid |
| 未终止插值 | 从最内层未闭合 `${` 覆盖并消费到 EOF | 保留已产生的 string / interpolation lexeme，不另造重叠 invalid |
| 非法字符串转义 | 后有 scalar 时覆盖反斜杠和该 scalar 并继续字符串；反斜杠后直接是 CR / LF / EOF 时只覆盖反斜杠、结束当前字符串恢复，且不再追加未终止字符串诊断 | 同范围 invalid；前后 string-text 仍分别取最大非空区域 |
| 非法 `Char` 字面量 | 覆盖从开始引号到结束引号、未转义 CR / LF 之前或 EOF；不消费换行 | 整个范围为一个 invalid，不再拆 `Char` 内部 token |
| 非法数字 | 覆盖并消费本节定义的完整非法数字区域 | 同范围 invalid |

错误恢复后的 lexeme 流仍须覆盖全部输入。若错误产生 invalid lexeme，它只能覆盖尚未由其他
lexeme 表示的已消费字节，不得为覆盖整条诊断而与既有 string segment 等 lexeme 重叠，也
不得产生零长度错误 token 或在同一 offset 循环。EOF 处多层模式按上一节规则只报告最内层
错误；诊断 `Span` 可以包含多个相邻 lexeme 的范围，但 lexeme 自身不得重叠。

字符串模式恢复还必须满足以下例子，其中 `<LF>` / `<EOF>` 表示输入边界而不是源码文本：

- `"abc<LF>next` 产生未终止字符串诊断，弹出当前字符串模式且不消费 LF；随后 LF 是普通
  newline trivia，`next` 按普通模式扫描。
- `"${ "abc<LF>next }"` 在内层字符串上产生一次未终止字符串诊断，只弹出内层字符串；
  LF 及后续 `next` 继续按当前插值模式扫描，匹配 `}` 后回到外层字符串模式。
- `"abc\<EOF>` 只产生一次非法字符串转义诊断，反斜杠是 invalid lexeme；不再追加
  未终止字符串诊断。若该字符串嵌套于未终止插值，EOF 也不再为外层模式追加级联诊断。

---

# 第四部分：表达式与类型引用的可执行语法

本部分是 Phase 1 parser 的可执行边界。产生式中的终结符引用第三部分的 token 分类；parser
在每次取 token 前跳过 trivia，但仍可用相邻 token 的字节 `Span` 判断中间是否存在 trivia。
大括号 `{ X }` 表示重复零次或多次，方括号 `[ X ]` 表示可选，不是 Koven 源码字符。

## 1. 独立表达式入口与 primary

```ebnf
standalone_expression = trivia*, expression, trivia*, EOF ;
expression            = assignment_expression ;

primary_expression = Identifier
                   | IntegerLiteral
                   | FloatLiteral
                   | CharLiteral
                   | "true" | "false" | "null" | "this"
                   | grouped_expression
                   | unbound_reference
                   | string_expression ;

grouped_expression = "(", expression, ")" ;
unbound_reference  = "::", Identifier ;

string_expression  = StringStart,
                     { StringText | interpolation },
                     StringEnd ;
interpolation      = InterpolationStart,
                     expression,
                     InterpolationEnd ;
```

`Identifier` 指 lexer 的普通 identifier token，包含在普通标识符位置使用的软词拼写；硬关键字
和未来保留字不符合该终结符。Phase 1 的 primary **仅**包含上表项目。`if`、`when`、`super`
和 lambda 字面量延后到后续 Spec，不得由 SPEC-0007 猜测其形态。

独立入口必须在跳过首尾 trivia 后消费到唯一 EOF，不能以“已得到一个表达式”为由忽略后续
token。字符串插值递归调用同一表达式 parser，但以当前层的 `InterpolationEnd` 代替 EOF
作为 stop token；它不得吃掉结束插值的 `}`。嵌套的括号、索引、调用和字符串仍各自消费
自己的闭合 delimiter。

## 2. Postfix、调用、索引与函数引用

```ebnf
postfix_expression = primary_expression, { postfix_suffix } ;

postfix_suffix = ".", Identifier
               | "?.", Identifier
               | call_suffix
               | index_suffix
               | "!!"
               | "::", Identifier ;

call_suffix       = "(", [ positional_argument,
                            { ",", positional_argument } ], ")" ;
positional_argument = expression ;
index_suffix      = "[", expression, "]" ;
```

所有 suffix 同属最高优先级，从左到右逐个包裹 receiver；例如 `a!!.b(c)[i]::ref!!` 是一条
确定的左结合链，`!!` 可重复。`.`、`?.` 和有 receiver 的 `::` 后必须是普通
`Identifier`，不能用硬关键字或 reserved-word token 冒充名称。无 receiver 的 `::name`
属于 primary，有 receiver 的 `value::name` 属于 postfix。两类引用在此阶段只产生 AST，名称
解析、可见性与 callable 类型均属后续阶段。`e!!` 的既有语义不变，仍脱糖为
`e ?: error("Non-null assertion failed")`。

SPEC-0007 的调用只接受位置实参：允许空参数列表，不允许 trailing comma。调用参数入口若
直接看到无额外括号包裹的 `Identifier = expression`，必须报告“尚不支持的实参形式”，不得
把它误作普通赋值位置实参；该形态保留给 SPEC-0010 的命名实参。同理，入口直接出现
`own`、`inout` 或 `borrow` 时也按尚不支持的实参形式诊断，三种模式实参由 SPEC-0010
实现。赋值本身仍是表达式，所以显式分组的 `f((a = b))` 是合法位置实参。

索引后缀恰好包含一个表达式：不允许 `a[]`、`a[x, y]` 或 trailing comma。`arr[1..3]`
语法上是以 range 表达式为唯一 key 的普通索引；它不产生切片 AST，类型及 v2 切片边界见
第一部分第 8 节。

## 3. `type_ref` 最小语法

```ebnf
type_ref          = qualified_type, [ "?" ]
                  | function_type ;

qualified_type    = Identifier, { ".", Identifier }, [ type_arguments ] ;
type_arguments    = "<", type_ref, { ",", type_ref }, ">" ;

function_type     = [ "move" ], "(",
                    [ type_ref, { ",", type_ref } ],
                    ")", "->", type_ref ;
```

限定类型路径只允许末段携带泛型实参；泛型实参可以递归包含任意本节 `type_ref`。限定类型
最多带一个末尾 `?`，因此 `T??` 非法。函数返回类型仍递归使用 `type_ref`，所以
`() -> T?` 唯一表示“返回 `T?` 的函数”，不表示可空函数值。v0.6 暂不提供可空函数类型的
写法，也不新增类型分组语法来绕过该边界。v1 不支持 star projection、声明处或使用处型变；
不得把 `*`、`in T` 或 `out T` 塞入类型实参。`move` 只可作为函数类型前缀。

SPEC-0007 实现该语法是因为 `as` / `as?` / `is` / `!is` 必须能消费类型引用。SPEC-0008
复用同一 `type_ref`，并新增声明签名、泛型参数声明及调用点类型实参。表达式中的
`callee<TypeA, TypeB>(...)` 也归 SPEC-0008：只有当完整、合法的 type-argument list 后紧跟
`(` 时才把 `<...>` 解释为调用点类型实参，否则按比较运算符解析；SPEC-0007 不提前接受
该形态。

## 4. 运算符层级与结合性

从高到低的语法如下；实现可用 Pratt binding power，但优先级只允许在一个实现位置定义：

```ebnf
prefix_expression = ( "!" | "+" | "-" ), prefix_expression
                  | postfix_expression ;

cast_expression = prefix_expression,
                  { ( "as" | "as?" ), type_ref } ;

multiplicative_expression = cast_expression,
                            { ( "*" | "/" | "%" ), cast_expression } ;
additive_expression       = multiplicative_expression,
                            { ( "+" | "-" ), multiplicative_expression } ;

range_expression = additive_expression,
                   [ ( ".." | "..<" ), additive_expression ] ;
to_expression    = range_expression, { "to", range_expression } ;
elvis_expression = to_expression, [ "?:", elvis_expression ] ;

membership_expression = elvis_expression,
                        [ ( "in" | "!in" ), elvis_expression
                        | ( "is" | "!is" ), type_ref ] ;
comparison_expression = membership_expression,
                        [ ( "<" | ">" | "<=" | ">=" ),
                          membership_expression ] ;
equality_expression   = comparison_expression,
                        [ ( "==" | "!=" ), comparison_expression ] ;

and_expression = equality_expression, { "&&", equality_expression } ;
or_expression  = and_expression, { "||", and_expression } ;

assignment_expression = or_expression,
                        [ assignment_operator, assignment_expression ] ;
assignment_operator   = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;
```

对应的优先级与结合性为：

| 优先级 | 运算符 / 结构 | 结合性 |
|---|---|---|
| 1 | `.` `?.` `()` `[]` postfix `!!`、bound `::name` | 左结合，可连续 |
| 2 | prefix `!` `-` `+` | 右结合 |
| 3 | `as` `as?` | 左结合 |
| 4 | `*` `/` `%` | 左结合 |
| 5 | `+` `-` | 左结合 |
| 6 | `..` `..<` | 不结合 |
| 7 | 精确软词 `to` | 左结合 |
| 8 | `?:` | 右结合 |
| 9 | `in` `!in` `is` `!is` | 不结合 |
| 10 | `<` `>` `<=` `>=` | 不结合 |
| 11 | `==` `!=` | 不结合 |
| 12 | `&&` | 左结合 |
| 13 | `\|\|` | 左结合 |
| 14 | `=` `+=` `-=` `*=` `/=` `%=` | 右结合 |

v1 的中缀运算符集合精确封闭为 identifier 拼写 `to`；不存在“`to` 等”这一开放集合。
`infix` 在表达式中只是普通标识符，不能触发中缀解析；其标准库声明上下文形态延后定义。

`in` / `!in` 的右侧是表达式；`is` / `!is` 的右侧必须是 `type_ref`。四者共享同一
non-associative 组，而不是共享同一种右操作数产生式。

区间、成员 / 类型关系、比较、相等是四个**分别不结合**的组。每组在同一未分组表达式中
至多出现一次；同组任意第二个运算符都报告 non-associative chain，例如 `a < b <= c`、
`a in b is T`、`a == b != c` 和 `a..b..<c` 均非法。括号建立新的表达式边界，所以
`(a < b) < c` 在语法上合法；不同组之间仍按上表优先级正常组合。

assignment 是右结合表达式。parser 只建立 AST，不在 Phase 1 判断左侧是否为可赋值 place；
lhs 合法性由 Phase 2 验证。调用参数对未加括号的 `Identifier = ...` 另有第 2 节的上下文
保留规则。

`own` / `inout` / `borrow` 不属于通用 prefix 层级；它们只属于 SPEC-0010 的调用实参专用
语法。无 trivia 相邻的 `++`、`--`、`<<`、`>>` 和 `...` 必须由 parser 整体识别并报告
unsupported operator，不能把它们接受为两次 prefix / binary 或 range 加 dot。若组成字符间
有 trivia，则按各自独立合法 token 和本节普通语法处理，最终是否成立由该 token 序列决定。
这条组合检查只在**表达式运算符位置**生效；`type_ref` 递归解析时，相邻的两个 `>` 可以分别
关闭内外两层泛型实参，例如 `Outer<Inner<T>>` 必须合法，不能被误报为 shift 运算符。

## 5. Lexer 错误交接、局部恢复与诊断类别

Lexer 产生的 invalid 或 reserved-word token 已有词法诊断。parser 在任何语法位置遇到它们
都须消费并在当前结构中放置 error node 以保证前进，不得在相同 `Span` 再发一条 parser
诊断；这也适用于独立入口的尾随位置和 delimiter / 名称恢复位置。除此之外，
若 Lexer 已对未终止 string / interpolation 报错并以恢复后的 segment 流抵达 EOF，parser
保留可构造的 string / error AST，但不得再为同一缺失结束符报告 expected closing delimiter；
Lexer 的对应诊断已经完整表达该根因。这项抑制不吞掉发生在字符串内容中的独立 parser 错误。
SPEC-0007 至少区分下列语法错误含义；稳定 `L` 码、固定消息与精确恢复用例由对应 Spec 分配：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected expression | 消费一个不可能开始表达式的 token 形成 error node；若已到当前 stop token / EOF，则不消费并形成空 error node |
| expected closing delimiter | 保留已解析的内部节点；在当前结构的匹配闭合符、调用 / 索引分隔符、插值 stop 或 EOF 处停止，不跨越外层安全边界 |
| expected member/reference name | `.`、`?.` 或 `::` 后缺名称时，在下一个 postfix / binary 边界前结束该 suffix，不把后续结构误挂为名称 |
| non-associative chain | 消费同组第二个运算符及其可解析右操作数作为错误区域，并保留第一段合法 AST |
| unexpected trailing token | 独立入口已得到表达式后仍有 token 时，从首个尾随 token 前进到当前 stop token，不静默成功 |
| expected type reference | 在 cast / type 参数需要类型处消费一个非法起始 token；遇当前 delimiter / stop 时不越界 |
| unsupported operator | 消费无 trivia 相邻的整个 `++`、`--`、`<<`、`>>` 或 `...` 组合，不把组合拆成合法 AST |
| unsupported argument form | 在当前实参层级消费命名 / 模式实参到顶层逗号或右括号；嵌套 delimiter 内的逗号不作同步点 |

这里仅要求表达式内部的最小、确定性恢复，并保证每次错误都消费输入或抵达明确 stop token；
完整文件、跨声明同步和“单个语法错误后继续解析后续声明”的策略归 SPEC-0011。

## 6. 表达式与 TypeRef AST 的合成 `Span`

所有范围都是现有 UTF-8 字节半开区间，并来自同一 `SourceId`：

| 节点 | 合成范围 |
|---|---|
| identifier / 数值 / `Char` / 关键字字面量 / `this` | 对应 primary token |
| 未绑定引用 | `::` 起点至名称 token 终点 |
| 分组 | 左括号起点至右括号终点；缺右括号时至该组最后消费位置 |
| prefix | 运算符起点至 operand 终点 |
| cast / `is` / `!is` | lhs 起点至 `type_ref` 终点 |
| 其他 binary / assignment | lhs 起点至 rhs 终点 |
| member / bound reference | receiver 起点至名称 token 终点 |
| call / index | receiver 起点至闭合 `)` / `]` 终点；缺闭合符时至该 suffix 最后消费位置 |
| postfix `!!` | receiver 起点至 `!!` 终点 |
| string | 开始引号起点至结束引号终点；恢复时至该字符串最后消费位置 |
| interpolation | `${` 起点至匹配 `}` 终点；恢复时至该插值最后消费位置 |
| error | 覆盖本次实际消费的错误区域；只有位于 stop token / EOF 且没有可消费 token 时可以为空 |

TypeRef 节点遵循以下唯一合成规则：

| 节点 | 合成范围 |
|---|---|
| qualified type | 从首段 `Identifier` 起，到末段 `Identifier` 终；若有 type arguments，则改为到匹配 `>` 终；若再有 nullable `?`，最终到该 `?` 终 |
| function type | 若有 `move`，从 `move` 起，否则从 `(` 起；到 return `type_ref` 终 |
| type arguments | 从 `<` 起到匹配 `>` 终；若实现不为它单建节点，该范围仍完整纳入所属 qualified type |
| TypeRef error | 与通用 error 相同：覆盖实际消费的错误区域；只有位于 stop token / delimiter / EOF 且没有可消费 token 时可以为空 |

缺失泛型 `>`、函数参数 `)`、`->` 后返回类型或其他 TypeRef delimiter 时，恢复节点止于本构造
最后实际消费位置，不得越过外层 stop token 或 delimiter。所有表达式与 TypeRef 合成范围都
不得用不存在的 token 伪造超出已消费输入的坐标。

---

# 第五部分：开发阶段优先级路线图

## Phase 0：项目骨架（已完成）

- [x] 建立 Cargo workspace：`lang-frontend` / `lang-codegen` / `lang-cli` / `lang-lsp` / `lang-std`
- [x] 定义 AST 数据结构（索引式节点）
- [x] 搭建诊断输出框架（错误码格式 `L0001` 等，从第一天就用）
- [x] 建立真实 Cargo test target，枚举至少一个 `.ko` fixture，并让零用例成为配置错误

**验收标准**：五个 workspace member 均有有效 Cargo target，workspace 基线命令可执行；测试
驱动能枚举并读取至少一个 `.ko` fixture，统一 source / `Span` 基础设施能对人工构造的 AST
和结构化诊断做稳定断言。Phase 0 不引入临时 parser，也不要求源码已能解析或执行。

## Phase 1：词法 + 语法分析（Lexer/Parser）

- [x] 实现第三部分完整词法规范：ASCII 标识符、42 个硬关键字、2 个软关键字、11 个未来
      保留字、trivia、字面量、字符串插值、固定符号、EOF 和错误恢复
- [x] 所有 token / trivia / invalid 区域保留精确 UTF-8 字节 `Span`，非法源码返回结构化
      诊断而不 `panic!`
- [ ] **SPEC-0007**：实现第四部分独立表达式入口、全部运算符层级、`type_ref`、仅位置实参的
      basic call、单表达式索引、局部恢复及表达式 AST `Span`
- [ ] **SPEC-0008**：实现 `val` / `var` / `const` / `fun` 等声明、泛型参数声明与约束、声明
      签名中的 `type_ref`，以及仅在完整 `<type_ref, ...>` 后跟 `(` 时成立的调用点类型实参
- [ ] **SPEC-0009**：实现 `if` / `when` / `super` / `for` / `while` / `loop` 和
      `value class` / `class` / `interface` / `enum class` / `object` / `companion object` 等
      control / class-family 结构
- [ ] **SPEC-0010**：实现 lambda 字面量、命名实参、`own` / `inout` / `borrow` 模式实参，
      以及解构声明 `val (a, b) = e`
- [ ] **SPEC-0011**：实现完整文件与跨声明错误恢复，并让 Phase 1 完整验收范例由真实 fixture
      覆盖；单个语法错误不得导致整个文件解析中断

前两项 Lexer 工作已经实现；上述 SPEC-0007 至 SPEC-0011 是候选规范建议的后续实施边界，
其未勾选状态不表示已经批准或已有代码。各 Spec 必须按编号依赖顺序独立验收和提交。

**Lexer 验收标准**：穷举验证全部硬 / 软 / 未来保留字及前后缀边界；正例覆盖 ASCII
标识符、Unicode `Char` / `String` 内容、全部字面量、`${...}` 嵌套插值、LF / CRLF、两类
注释、全部固定符号和最长匹配；反例逐类覆盖上表八种错误，并断言稳定错误码、精确字节
`Span`、恢复后的后续 token 及确定性顺序。数字测试必须锁定 `1..2` / `1..<2` 与
`1e3` / `0x10` / `1L` / `1_0` 的边界。空文件只产生一个 EOF，所有非 EOF lexeme
非空、不重叠并覆盖全部输入字节；真实 pass / fail `.ko` fixture 必须被 Cargo test target
枚举，零用例必须失败。

**Phase 1 最终 Parser 验收标准（SPEC-0011）**：能完整解析以下代码为 AST，语法错误有
准确的行列号定位。该范例同时依赖 SPEC-0007 至 SPEC-0010，不能作为提前扩大单个 Spec
范围的理由：

```kotlin
value class Point(val x: Int, val y: Int)

enum class Shape {
    Circle(radius: Double)
    Point

    fun area(): Double = when (this) {
        is Circle -> 3.14159 * radius * radius
        is Point -> 0.0
    }
}

fun main(): Unit {
    val shapes: List<Shape> = listOf(Shape.Circle(radius = 2.0), Shape.Point)
    val first = shapes.getOrNull(0) ?: error("empty list")
    val double: (Double) -> Double = { x -> x * 2 }
    println(double(first.area()))
}
```

## Phase 2：类型检查（不含所有权/借用）

- [ ] 局部类型推导（`val`/`var`）
- [ ] 函数签名类型检查（显式返回类型规则）
- [ ] 接口/`enum class` 变体的类型检查，`when` 穷尽性检查
- [ ] **智能类型转换（smart cast）**：`is`/`when` 分支内的类型收窄及其失效规则（变量在收窄后被重新赋值则收窄失效）
- [ ] 泛型单态化的类型层面准备（类型替换，不接编译期计算）
- [ ] `Nothing` 类型的 bottom-type 特殊处理
- [ ] 计算 `value class` 的条件 `Copyable`：允许不可复制字段，按实际字段类型和泛型实参递归
      推导；支持把预声明的 `Copyable` 用作泛型上界，但不接受用户手动实现、覆盖或同名冒充
- [ ] 对内建 `Box<T>` 执行 type-kind 检查：只接受 `value class` 类型实参，拒绝普通 `class`
- [ ] 检查字段投影的使用模式：可复制字段可读出 owned copy，不可复制字段只允许投影借用，
      禁止把普通字段读取标记为所有权移出
- [ ] 为 `value class` 建立有序结构分量并支持解构类型检查；右值只求值一次，类型结果标记为
      复制式或消费式解构；不可复制类型的消费式解构必须覆盖全部分量

**验收标准**：能对 Phase 1 能解析的全部语法结构做类型检查，类型错误有清晰的错误码和
定位；`when (this) { is Circle -> radius }` 这类智能类型转换场景能正确通过类型检查；包含
不可复制字段的 `value class` 以及 `Pair<Sender<Int>, Receiver<Int>>` 均是合法类型，而
`Pair<Int, Int>` 被推导为 `Copyable`；`<T : Copyable>` 可以满足要求该上界的调用或类型
约束，未约束的 `<T>` 不能。未约束 `T` 的普通所有权转移仍然合法，不应在 Phase 2 因缺少
`Copyable` 报错；其移动后使用由 Phase 3 判断。`Box<Node>` 必须产生类型诊断。

## Phase 3：所有权 / 借用检查

- [ ] 实现简化版单一所有者 + ASAP 析构（不做完整 NLL）
- [ ] 三态参数语义 `borrow`/`inout`/`own` 的检查
- [ ] 移动后使用（use-after-move）检测
- [ ] 按类型能力区分复制与移动：`Copyable value class` 可以复制；非 `Copyable value class`
      与普通 `class` 转交所有权后都禁止再次使用
- [ ] 检查消费式解构：不可复制聚合解构后源值不可用，所有分量作为一个所有权动作转移
- [ ] 拒绝通过普通字段访问或单独 `componentN()` 移出不可复制分量，不建立部分移动状态
- [ ] `move (...) -> T` 函数类型的检查：验证传给此类参数的闭包字面量必须带 `move` 前缀，且闭包体内不能捕获任何借用语义的外部变量
- [ ] `Shareable`/`Transferable` 标记 trait 的检查：跨线程 API（`thread` 等）传递的值类型必须满足对应约束

**验收标准**：能正确拒绝典型的“移动后使用”和“重复可变借用”错误用例；复制
`Pair<Int, Int>` 后源值仍可用，复制 `Pair<Sender<Int>, Receiver<Int>>` 被拒绝，后者消费式
解构后再次使用源值也被拒绝；遗漏任一分量的消费式解构、`own pair.first` 这类不可复制
字段部分移动均被拒绝；能正确拒绝“把借用捕获的普通闭包传给 `thread()`”这类用例（必须
报错要求改用 `move { ... }`）。泛型 `<T>` 的 owned 转移后再次使用源值被拒绝，而
`<T : Copyable>` 的同类操作交付 owned copy，源值仍可用。

## Phase 4：LLVM 代码生成

- [ ] 自建 SSA IR，从 AST/类型检查结果 lower 到该 IR
- [ ] IR 到 LLVM IR 的映射（用 `inkwell`）
- [ ] `value class`（内联布局）vs `class`（堆分配）的 codegen 差异实现；布局策略与
      `Copyable` 能力保持正交
- [ ] 生成复制/移动/消费式解构：复制只用于 `Copyable` 类型，非可复制内联字段转移后不
      重复析构
- [ ] `Copyable` 复制不调用 retain / clone glue，也不为被复制值生成唯一析构义务
- [ ] **闭包环境捕获的 codegen**：捕获环境结构体的内存布局设计，`move` 闭包与默认借用闭包在捕获方式上的差异实现，无捕获场景下降级为裸函数指针
- [ ] 析构函数插入（对应 Phase 3 的 ASAP 析构点）
- [ ] `error()` 编译为 abort 语义（不生成栈展开代码）
- [ ] DWARF 调试信息生成

**验收标准**：能编译并运行第二部分示例代码，产出正确结果的可执行文件；带副作用的解构
右值只执行一次，消费式解构后的每个不可复制字段恰好析构一次，不可复制 `value class`
移入 `Box` 后源存储不再析构，可复制 `Point` 通过 `own` 装箱后源值仍有效，复制
`Pair<Int, Int>` 不调用 glue 且源值仍有效；能用 `gdb`（Linux）或 `lldb`（macOS，现代
macOS 工具链下 `gdb` 需要额外签名权限，`lldb` 摩擦更小，两者都基于 DWARF）设断点单步
调试。

## Phase 5：最小标准库（用目标语言自身编写）

- [ ] 基础容器：`Array`/`List`/`MutableList`/`Map`/`MutableMap`（`Map` 独立于 `Indexable` 接口体系，见第一部分第 8 节）
- [ ] `Result<T, E>`、`Pair<A, B>`（自动解构支持；`Pair` 按类型实参条件满足 `Copyable`）
- [ ] `Rc<T>`/`Box<T>`（`Box<T>` 只接受 value class 并取得传入值所有权；`Rc<T>` 需要
      retain，因此本身不满足 `Copyable`）
- [ ] 高阶函数支持的集合操作：`map`/`filter`/`reduce`/`forEach`
- [ ] 基础 IO：`File`、`BufferedReader`、标准流
- [ ] 线程/channel API，`thread()` 签名使用 `move (...) -> Unit`
- [ ] `@Test` 注解 + 断言函数，跑通自身的测试套件

**验收标准**：标准库自身的测试套件全部用目标语言编写并通过；至少覆盖
`Pair<Int, Int>` 的复制、`Pair<Sender<Int>, Receiver<Int>>` 的构造与消费式解构，以及把
可复制 `Point` 交给 `Box` 后继续使用源值、把不可复制 `value class` 移入 `Box` 后禁止再次
使用源值、`Box<Node>` 的 compile-fail 和 `Rc<T>` 不被误判为 `Copyable`。

## Phase 6：工具链完善

- [ ] 包管理器 CLI（`project.toml`/`project.lock`）
- [ ] LSP 基础功能（语法高亮、诊断、跳转定义）
- [ ] 代码格式化工具
- [ ] TextMate/Tree-sitter 语法文件

**Phase 6 之后**：并发编译期检查完善、泛型型变、`dyn` 动态分发、`async`/`await` 等 v2/v3 特性按需排期，不在 v1 范围内。

---

# 第六部分：Rust 工程规范

- **crate 划分严格遵循 Phase 0 的 workspace 结构**，`lang-frontend` 不得依赖 `inkwell`/LLVM 相关 crate。
- **每个 Phase 的功能提交都必须配套测试**：语法/类型检查类改动配套 `.ko` 测试用例，编译器内部逻辑配套 Rust 单元测试。
- **诊断信息优先级高于功能完整度**：宁可先实现"检测到错误 + 准确报告"，再实现"错误恢复继续编译"。
- **每个新增关键字/语法结构必须同步更新本文档第三部分关键字表**，避免文档与实现脱节。
- **每次对本文档做出会影响已实现代码的修改，都必须在“本版变更记录”里补一条**，保持文档可追溯；后续版本继续遵循。
- **命名规范**：Rust 代码本身遵循标准 Rust 命名约定，与目标语言的 Kotlin 风格命名是两套独立的命名体系，不要混淆。
