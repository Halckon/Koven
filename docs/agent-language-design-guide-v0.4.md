# AGENT 开发指导文档：Koven 语言设计规范 v0.4

> **归档状态：已由
> [`agent-language-design-guide-v0.5.md`](./agent-language-design-guide-v0.5.md) 取代，不再是现行规范。**
> 本文档保留为 v0.4 的历史语义记录；当时它取代
> [`agent-language-design-guide-v0.3.md`](./agent-language-design-guide-v0.3.md)。语法设计原则：
> **尽量贴近 Kotlin 命名与语法习惯**，内存模型为 Rust 式简化所有权/借用，编译器用
> Rust 实现，LLVM 后端。v0.3 及更早资料如与本文档冲突，以本文档为准。

> 归档说明：v0.3 保留为历史版本，不再接收 v0.4 语义修改。更早的 v0.2 guide、旧技术栈/
> 语言规格以及下文提到的审计报告尚未随当前仓库归档，仅作为历史来源，不参与现行规范
> 优先级。

## 本版（v0.4）变更记录

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

区间/切片索引（`arr[1..3]`）列为 v2 特性。

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
| 中缀函数（infix） | 保留但严格限制：只允许标准库内部使用，v1 不对用户代码开放自定义 `infix fun` |

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

# 第三部分：完整关键字 / 保留字表

## 硬关键字（41 个，按用途分类，不可作为标识符）

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
borrow      inout       own         unsafe
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

## 软关键字（仅特定上下文有特殊含义，其余场景可作普通标识符）

```
to          infix（仅标准库内部使用）
```

> v0.2 中的 `get`/`set` 已从软关键字表移除：随着自定义属性访问器被砍掉（第一部分第 13 节），`get`/`set` 只会出现在 `operator fun get(...)`/`operator fun set(...)` 声明里，此处 `operator` 关键字已足够触发特殊解析，`get`/`set` 本身不再需要额外的上下文关键字地位。

## 保留但当前版本未使用（预留给 v2/v3，禁止用作标识符）

```
async       await       suspend     actor       spawn
sealed      dyn         where       yield       macro
reify
```

> `dyn` 已在第一部分第 15 节明确说明：关键字保留，但对应语法降级到 v2，v1 不实现。

> Agent 注意：即便这些保留字当前版本没有对应语法功能，**lexer 阶段也应将其识别为保留标识符并拒绝用户用作变量/函数/类型名**，防止未来版本引入新语法时出现向后兼容问题。

---

# 第四部分：运算符优先级表（已核实修正，从高到低）

v0.2 的表格与 Kotlin 官方语法实际顺序不符（Elvis 运算符优先级被显著低估），下表已核对 Kotlin 语法规则重新排定：

| 优先级 | 运算符 | 结合性 |
|---|---|---|
| 1 | `.` `?.` `[]` `()`（成员访问/索引/调用） | 左结合 |
| 2 | `!` `-`（一元） `+`（一元） | 右结合 |
| 3 | `as` `as?`（类型转换） | 左结合 |
| 4 | `*` `/` `%` | 左结合 |
| 5 | `+` `-`（二元） | 左结合 |
| 6 | `..` `..<`（区间构造） | 不结合 |
| 7 | 中缀函数调用（标准库内置 `to` 等） | 左结合 |
| 8 | `?:`（Elvis） | 右结合 |
| 9 | `in` `!in` `is` `!is` | 不结合 |
| 10 | `<` `>` `<=` `>=` | 不结合 |
| 11 | `==` `!=` | 不结合 |
| 12 | `&&` | 左结合 |
| 13 | `\|\|` | 左结合 |
| 14 | `=` `+=` `-=` `*=` `/=` `%=`（赋值） | 右结合 |

关键变化：`?:` 从 v0.2 的第 12 档大幅上调至第 8 档，高于 `in`/`is`/比较/相等/逻辑运算符，与 Kotlin 官方语法一致。

**`own`/`inout`/`borrow` 不在此表中**：v0.2 曾把它们当作通用前缀一元运算符放进优先级表，但它们语法上只应出现在函数调用实参列表的参数前缀位置（如 `consume(own n)`），不是可以修饰任意表达式的通用运算符。这三个关键字改由 parser 在解析调用实参时用专门的语法产生式单独处理，不走 Pratt parser 的通用前缀运算符路径，因此不存在"优先级该排第几"的问题，也避免了 `own a + b` 这类无意义组合被意外解析成功。

Agent 实现 Pratt parser 时以上表数值作为 binding power 依据。

---

# 第五部分：开发阶段优先级路线图

## Phase 0：项目骨架（预计 1 周内完成）

- [ ] 建立 Cargo workspace：`lang-frontend` / `lang-codegen` / `lang-cli` / `lang-lsp` / `lang-std`
- [ ] 定义 AST 数据结构（索引式节点）
- [ ] 搭建诊断输出框架（错误码格式 `L0001` 等，从第一天就用）
- [ ] 建立语言测试套件目录结构（`.ko` 源文件 + 期望输出/期望错误码）

**验收标准**：五个 workspace member 均有有效 Cargo target，workspace 基线命令可执行；测试
驱动能枚举并读取至少一个 `.ko` fixture，统一 source / `Span` 基础设施能对人工构造的 AST
和结构化诊断做稳定断言。Phase 0 不引入临时 parser，也不要求源码已能解析或执行。

## Phase 1：词法 + 语法分析（Lexer/Parser）

- [ ] 实现第三部分关键字表中所有硬关键字的词法识别
- [ ] 实现 Pratt parser，覆盖第四部分优先级表全部运算符
- [ ] 覆盖核心语法结构：`val`/`var`/`const`、`fun`、`if`/`when`/`for`/`while`/`loop`、`value class`/`class`/`interface`/`enum class`/`object`/`companion object`
- [ ] 泛型参数列表与约束（`<T : Bound>`）、lambda 字面量、索引表达式 `[]`、解构声明 `val (a, b) = e` 的解析
- [ ] `own`/`inout`/`borrow` 作为调用实参前缀的专属语法产生式（不进入通用表达式优先级体系）
- [ ] 错误恢复：单个语法错误不应导致整个文件解析中断

**验收标准**：能完整解析以下代码为 AST，语法错误有准确的行列号定位（相比 v0.2 的验收范例，本版本补充了 lambda、索引、Elvis、智能类型转换分支，覆盖面更完整）：

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
