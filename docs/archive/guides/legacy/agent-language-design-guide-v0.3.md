# AGENT 开发指导文档：自研编译型语言设计规范 v0.3

> **历史版本：本文已被后续 guide 取代；当前唯一语言规范入口是
> [`guide/00-index.md`](../v0.34-pre-restructure/00-index.md) 导航的 v0.14 文档集。本文只保留 v0.3 当时的
> 语义记录，不参与现行语义优先级。**

> 本文档是给开发 Agent 的权威规范，取代 v0.2（`agent-language-design-guide.md`）。语法设计原则：**尽量贴近 Kotlin 命名与语法习惯**，内存模型为 Rust 式简化所有权/借用，编译器用 Rust 实现，LLVM 后端。本文档与此前所有文档（`language-tech-stack.md`、`language-spec-full.md`、v0.2 guide）如有冲突，一律以本文档为准。

> 归档说明：上述 v0.2 guide、旧技术栈/语言规格以及下文提到的审计报告尚未随当前仓库
> 归档，仅作为本版变更的历史来源，不参与现行规范优先级。旧版本保留规则从 v0.3 起执行；
> 后续补入更早材料时应作为历史资料保留。

## 本版（v0.3）变更记录

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
value class Point(val x: Int, val y: Int)     // 值语义：赋值/传参按值拷贝

class Node(var value: Int, var next: Node?)    // 引用语义：堆分配，遵循所有权/借用规则
```

- **值语义 ≠ 栈分配**：`value class` 表示"赋值/传参按值拷贝、按内联布局存储"——作为局部变量时确实在栈帧上，但作为其他类型的字段或数组元素时是内联存储在容器内部，不是独立的"栈对象"。避免用"栈分配"这个不准确的说法，防止在设计内存布局/codegen 时产生"value class 永远绑在栈帧上"的错误假设。
- **字段可拷贝规则**：`value class` 的**所有字段**（无论 `val` 还是 `var`）的类型都必须满足 `Copyable`。可变性（val/var）和可拷贝性是两个独立维度——一个 `val` 字段如果类型是不满足 `Copyable` 的 `class`（独占堆资源的引用类型），复制这个 `value class` 照样会产生两个指向同一堆对象的引用，破坏单一所有权，这跟字段是不是 `val` 无关。
- 普通 `class` = 引用语义，堆分配，所有权/借用规则生效。

**`Box<T>` 的定位**：既然 `class` 本身已经是堆分配引用类型，`Box<T>` 不再用于"把 class 包一层"（那是多余的），而是专门用于**把一个 `value class` 实例显式搬到堆上**：

```kotlin
val boxed: Box<Point> = Box(Point(1, 2))   // 把值类型 Point 装箱到堆上
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
internal fun helper() { ... }
private fun impl() { ... }
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

采用 Kotlin 的 `componentN()` 约定：`val (a, b) = e` 脱糖为 `val a = e.component1(); val b = e.component2()`。

- **编译器为 `value class` 的主构造函数参数自动生成 `componentN()`**（对应 Kotlin data class 的行为），比如 `Point(val x: Int, val y: Int)` 自动可解构为 `val (x, y) = point`。
- 非 `value class` 的类型（如上面的 `channel()` 返回值类型 `Pair`，本身也是 value class 所以其实也自动生成）若要支持解构，需要手动实现对应的 `componentN()` 约定函数。

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
    override fun log(msg: String) {
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
value class Point(val x: Int, val y: Int)                // 值语义，编译器自动生成 componentN()

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
- [ ] 建立语言测试套件目录结构（`.lang` 源文件 + 期望输出/期望错误码）

**验收标准**：能跑通一个空 `main` 函数的 "hello world" 从源码到诊断输出。

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

fun main() {
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
- [ ] 编译器为 `value class` 主构造函数参数自动生成 `componentN()`，并支持解构声明的类型检查

**验收标准**：能对 Phase 1 能解析的全部语法结构做类型检查，类型错误有清晰的错误码和定位；`when (this) { is Circle -> radius }` 这类智能类型转换场景能正确通过类型检查。

## Phase 3：所有权 / 借用检查

- [ ] 实现简化版单一所有者 + ASAP 析构（不做完整 NLL）
- [ ] 三态参数语义 `borrow`/`inout`/`own` 的检查
- [ ] 移动后使用（use-after-move）检测
- [ ] `value class` 的拷贝语义 vs `class` 的移动语义区分检查（含"所有字段类型需满足 `Copyable`"的校验）
- [ ] `move (...) -> T` 函数类型的检查：验证传给此类参数的闭包字面量必须带 `move` 前缀，且闭包体内不能捕获任何借用语义的外部变量
- [ ] `Shareable`/`Transferable` 标记 trait 的检查：跨线程 API（`thread` 等）传递的值类型必须满足对应约束

**验收标准**：能正确拒绝典型的"移动后使用"和"重复可变借用"错误用例；能正确拒绝"把借用捕获的普通闭包传给 `thread()`"这类用例（必须报错要求改用 `move { ... }`）。

## Phase 4：LLVM 代码生成

- [ ] 自建 SSA IR，从 AST/类型检查结果 lower 到该 IR
- [ ] IR 到 LLVM IR 的映射（用 `inkwell`）
- [ ] `value class`（内联布局）vs `class`（堆分配）的 codegen 差异实现
- [ ] **闭包环境捕获的 codegen**：捕获环境结构体的内存布局设计，`move` 闭包与默认借用闭包在捕获方式上的差异实现，无捕获场景下降级为裸函数指针
- [ ] 析构函数插入（对应 Phase 3 的 ASAP 析构点）
- [ ] `error()` 编译为 abort 语义（不生成栈展开代码）
- [ ] DWARF 调试信息生成

**验收标准**：能编译并运行第二部分示例代码，产出正确结果的可执行文件，且能用 `gdb`（Linux）或 `lldb`（macOS，现代 macOS 工具链下 `gdb` 需要额外签名权限，`lldb` 摩擦更小，两者都基于 DWARF）设断点单步调试。

## Phase 5：最小标准库（用目标语言自身编写）

- [ ] 基础容器：`Array`/`List`/`MutableList`/`Map`/`MutableMap`（`Map` 独立于 `Indexable` 接口体系，见第一部分第 8 节）
- [ ] `Result<T, E>`、`Pair<A, B>`（自动解构支持）
- [ ] `Rc<T>`/`Box<T>`（`Box<T>` 用于 value class 显式装箱）
- [ ] 高阶函数支持的集合操作：`map`/`filter`/`reduce`/`forEach`
- [ ] 基础 IO：`File`、`BufferedReader`、标准流
- [ ] 线程/channel API，`thread()` 签名使用 `move (...) -> Unit`
- [ ] `@Test` 注解 + 断言函数，跑通自身的测试套件

**验收标准**：标准库自身的测试套件全部用目标语言编写并通过。

## Phase 6：工具链完善

- [ ] 包管理器 CLI（`project.toml`/`project.lock`）
- [ ] LSP 基础功能（语法高亮、诊断、跳转定义）
- [ ] 代码格式化工具
- [ ] TextMate/Tree-sitter 语法文件

**Phase 6 之后**：并发编译期检查完善、泛型型变、`dyn` 动态分发、`async`/`await` 等 v2/v3 特性按需排期，不在 v1 范围内。

---

# 第六部分：Rust 工程规范

- **crate 划分严格遵循 Phase 0 的 workspace 结构**，`lang-frontend` 不得依赖 `inkwell`/LLVM 相关 crate。
- **每个 Phase 的功能提交都必须配套测试**：语法/类型检查类改动配套 `.lang` 测试用例，编译器内部逻辑配套 Rust 单元测试。
- **诊断信息优先级高于功能完整度**：宁可先实现"检测到错误 + 准确报告"，再实现"错误恢复继续编译"。
- **每个新增关键字/语法结构必须同步更新本文档第三部分关键字表**，避免文档与实现脱节。
- **每次对本文档做出会影响已实现代码的修改，都必须在"本版变更记录"里补一条**，保持文档可追溯——这是本次从 v0.2 升级到 v0.3 沿用的规则，后续版本继续遵循。
- **命名规范**：Rust 代码本身遵循标准 Rust 命名约定，与目标语言的 Kotlin 风格命名是两套独立的命名体系，不要混淆。
