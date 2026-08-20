# Koven 语言设计规范 · 核心设计决策

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第一、二部分），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。内容版本：v0.19。
> v0.13 拆分只重组文件结构，不改变任何已定义语义；v0.14 的 `&` 调用点语义及同步修改见
> [`07-changelog-archive.md`](./07-changelog-archive.md)。本文档覆盖第 1–20 节的设计决策，
> 附录收录原第二部分的核心结构声明总览。

> **阅读说明（v0.18 更新）**：本部分大量示例代码使用了 `if`、`when`、`super`、循环以及
> `class`/`interface`/`enum class`/`object` 等 class-family 结构。这些结构反映的是 v1 已经
> 确定的**目标设计意图**，用来说明其他设计决策（例如 `enum class` 的能力、`super` 的用途）
> 在最终语言里如何使用。control-flow 已由
> [04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md) §12 正式定义；
> class-family 仍由 SPEC-0017 后续独立定义。class-family 示例继续只能理解为设计意图，
> 不能反推成已经验收的产生式。

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

见[02-lexical-spec.md](./02-lexical-spec.md)完整表格。

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
- `ParamTypes` 的每一项使用[03-grammar-core.md](./03-grammar-core.md)第 3 节与[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)
  第 9 节的 callable 参数契约：无标记是 `Value`
  （按值参数，`Copyable` 则复制、否则移动，调用点从不需要标注），也可显式写 `borrow` /
  `inout`（分别是共享借用与独占可变借用）；具名函数参数使用同一契约。**调用点是否需要
  重复书写 `borrow` 由编译器按 callee 已声明的契约自动判定；只有 `Inout` 契约仍要求调用点
  显式标注，但调用点的拼写是符号 `&` 而不是关键字 `inout`**（`&x` 而非 `inout x`），
  详见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节的完整规则与设计说明（v0.12/v0.14 变更）。
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
  作为局部变量时通常位于栈帧中；作为字段、顺序容器元素或其他聚合的分量时直接内联在
  所属布局中。容器元素内联只描述元素在缓冲区中的表示，不承诺该缓冲区位于栈上。它不是
  “永远在栈上”，也不因采用值语义就必然可复制。
- **内联布局必须有限**：直接或间接只经过内联字段形成的递归环是类型错误；递归必须经过普通
  `class`、`Box`、动态顺序容器或其他固定大小间接 handle 打断。具体单态化类型的大小和对齐
  在目标 DataLayout 可用后计算；计算溢出或超过目标可表示对象大小必须在构造 LLVM 类型前
  形成结构化布局诊断，不能让无效类型进入 LLVM。
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
- **可空值条件复制**：`T?` 在且仅在 `T` 满足 `Copyable` 时也满足 `Copyable`；复制其
  null / non-null tag 与 payload 不调用 retain、clone 或析构 glue。普通 `class?`、`String?`、
  `Box<T>?` 等不会仅因可空而获得该能力。
- **复制契约**：`Copyable` 表示一个有效值可以无用户可观察的 copy / retain / clone glue，
  按字段递归复制；实现可以在布局允许时用等价的按位复制优化。满足该能力的类型不得承担
  唯一资源释放或其他非平凡析构义务。需要引用计数递增、深拷贝、用户 copy hook 或独占
  drop 的类型均不得满足 `Copyable`。标准库 Spec 指定任何额外 `Copyable` 类型时也必须满足
  这份契约，不能把 marker 当作绕过所有权检查的白名单。
- **使用规则由能力决定**：满足 `Copyable` 的值在赋值、返回或传给按值参数时可以隐式复制，
  原值仍可使用；不满足 `Copyable` 时，同样的位置转移所有权，原值随后不可使用。调用实参
  仍须遵守 callee 的 `Value` / `Borrow` / `Inout` 契约；`Inout` 在调用点仍强制要求显式
  标注，不因类型可复制而省略——**这是唯一强制要求调用点 marker 的契约，调用点写作符号
  `&`（例如 `mutate(&x)`），不是关键字 `inout`；关键字 `inout` 只出现在声明侧**，理由见
  [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节的设计说明。`Value` 与 `Borrow` 均不要求调用点 marker
  （`borrow` 仍可选择显式写出，纯粹为了可读性）。
  因此 `consume(x)` 对 `Copyable` 的 `x` 交付一个 owned copy，对不可复制的 `x` 则移动
  原值，调用点都不需要额外标注。
- 本文出现的“取得所有权的参数”专指普通 `Value` 参数接收一个不满足 `Copyable` 的按值结果
  时发生的移动（v0.10/v0.11 曾经存在的独立 `Own` 契约已在 v0.12 并入 `Value`，不再是
  单独概念）。标准库 Spec 必须按[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节登记具体契约；不得按函数名或参数类型把
  无标记 `Value` 猜成 `Borrow` 或 `Inout`。
- **字段访问不允许隐式部分移动**：`aggregate.field` 是一个 place。`Copyable` 字段可复制
  读取；字段 place 可以在调用实参中被借用（`Borrow` 契约，标注可选）或（字段可变时）被
  `Inout` 契约借用（标注必须）。v1 禁止用普通字段读取从聚合中移出不可复制字段：不可复制
  聚合只能整体移动，或按第 11 节的完整结构解构一次性消费；不能产生需要追踪“哪些字段已经
  移走”的部分移动状态。
- 普通 `class` 是堆分配的引用语义值，本身不满足 `Copyable`，转交所有权时发生移动；它与
  非 `Copyable value class` 都受移动后使用检查约束，二者差异在布局而不在“一个复制、一个
  移动”的固定分类。

**`Box<T>` 的定位**：既然 `class` 本身已经是堆分配引用类型，v1 的 `Box<T>` **只允许 `T`
是 `value class`**；`Box<Node>` 这类把普通 `class` 再包一层的类型实例化必须产生类型错误。
`Box<T>` 是不可复制的独占所有权类型，用于把一个 `value class` 实例显式搬到堆上。装箱
取得传入值的所有权；对可复制值交付 owned copy，因此源值仍可用，对不可复制值则发生
移动。`Box(...)` 的构造参数是 `Value` 契约，调用点不需要写任何标注：

```kotlin
val point = Point(1, 2)
val boxedPoint: Box<Point> = Box(point)   // Point 可复制；point 仍可使用
println(point.x)

value class Endpoint(val sender: Sender<Int>)
val endpoint = Endpoint(sender)
val owned: Box<Endpoint> = Box(endpoint) // Endpoint 不可复制；这里移动 endpoint
// 此后再次使用 endpoint 是移动后使用错误
```

## 6. 函数默认实现保留；省略返回标注只表示 `Unit`

```kotlin
interface Shape {
    fun reset()
    fun area(): Double
    fun describe(): String = "a shape"   // 接口默认方法实现，保留
}

fun log(message: String) { println(message) } // block body 省略标注，固定返回 Unit
fun add(a: Int, b: Int): Int = a + b     // 表达式体语法保留，返回类型必须显式写出
```

**规则：具名函数只有在无体或 block body 形态才可省略 `: type_ref`；一旦省略，其返回类型
精确固定为内建 `Unit`。表达式体 `= expression` 必须显式写出 `: type_ref`。** 显式
`: Unit` 与省略标注的类型结果相同，但 AST 必须区分二者的源码形态。该规则不是返回类型
推导：parser 不检查 body 结果，Phase 2 也不得从 body、被覆盖声明或调用上下文反推另一返回
类型。lambda literal、函数类型、构造器和显式 `: Nothing` 不受该省略规则影响；局部变量
`val` / `var` 的类型推导继续保留。

## 7. `public` 替代 `pub`；`enum class` 语法融合 Kotlin 命名与 Rust ADT 能力

```kotlin
public class Circle { ... }
internal fun helper(): Unit { ... }
private fun impl(): Unit { ... }
```

`enum class` 采用 Kotlin 命名，但赋予 Rust 风格的“每个变体带不同关联数据”的能力：

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

## 8. 顺序容器、内存表示与索引语义（Array / List / MutableList）

本节把语言契约与实现层次分开。`Array`、`List`、`MutableList` 及本节列出的核心构造、
`size` 和索引操作是编译器预声明的语言原语，不是 Phase 5 再声明的同名 `.ko` API。各阶段
职责唯一如下：

- Phase 1 只建立普通 call / member / index AST，不按名称硬编码容器语义；
- Phase 2 识别预声明符号、容器类型、元素类型、可变性与类型层面的 element-place 类别，不判定
  该 place 此刻能否移动、复制、借用或借用有效期；
- Phase 3 检查构造、读取、替换、整体移动与元素借用的所有权效果；
- Phase 4 实现这些预声明原语的 SSA / runtime 基元及堆分配基线；
- Phase 5 只在它们之上用目标语言实现普通集合方法与算法，不重新声明核心构造、
  `size` 或索引原语，也不引入第二套容器表示或所有权规则。

精确 runtime ABI 仍须由 ADR 决定；本节不把尚未实现的库或 ABI 描述成当前工程事实。

### 顺序容器的角色与长度

v1 对顺序容器作封闭定义：

- `Array<T>` 是**运行时确定长度、构造后长度固定、元素可替换**的独占 owning container。
  “长度固定”只表示同一个 `Array<T>` owner 不能增删槽位；长度仍不属于类型，也不要求编译期
  已知。
- `List<T>` 是**运行时确定长度、只读**的独占 owning container。通过 `List<T>` 不能替换元素
  或改变长度；这里的“动态长度”只表示不同实例可在运行时具有不同 `size`。
- `MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变
  `size` 的操作。

三者都预声明只读属性 `size: Int`。读取 `container.size` 对 receiver 求值一次，只在本次
读取期间访问 header，不消费、复制或转移容器 owner；`size` 不能作为赋值或 `Inout`（调用点
符号 `&`）目标。`capacity` 是 `MutableList` 的实现不变量，v1 不因此自动暴露同名公共属性。

三者是不同的名义类型，v1 不提供隐式相互转换、隐式只读视图或隐式共享所有权。后续标准库
若提供零复制转换，必须是显式且消费源 owner 的操作；若提供借用视图，则必须等借用返回与
视图生命周期另行规范后使用独立类型表达，不能把它隐藏在 `List<T>` 中。

### 唯一 owner 表示与连续缓冲区

每个具体单态化实例只有一种规范逻辑表示：

- `Array<T>` 与 `List<T>` 是固定大小的 owner header，逻辑上持有一个元素缓冲区和当前
  `size`；
- `MutableList<T>` 是固定大小的 owner header，逻辑上还持有 `capacity`，并保持
  `0 <= size <= capacity`；
- header 的大小不随元素数量变化，可按普通值规则位于寄存器、栈槽、字段或其他聚合中；
- 元素位于 header 之外的一段连续缓冲区中；`[0, size)` 全部已初始化，
  `MutableList<T>` 的 `[size, capacity)` 不包含有效 `T`，不得读取或析构；
- 精确字段顺序、指针宽度、空缓冲区 sentinel、padding、对齐和调用 ABI 由后续 runtime ABI
  ADR 决定，不是源语言可观察布局。

三个容器都唯一拥有其逻辑缓冲区；需要物理字节时，它们也唯一拥有并负责释放对应 allocation，
因此自身不满足 `Copyable`，即使 `T` 满足 `Copyable` 也不例外。移动容器只转移 header 代表的
逻辑缓冲区所有权，源值随后不可使用；只读 `List<T>` 不得因此隐式 retain 或获得共享所有权。
空容器或零大小元素可以不请求零字节 allocation，但仍使用同一种 header 与 owner 语义，不构成
第二种表示。

### 元素布局与显式间接层

顺序容器按具体 `T` 单态化；缓冲区满足 `T` 的对齐，相邻元素按目标布局给出的 `stride(T)`
排列：

- 基础标量和 `value class A` 直接作为元素内联。`List<A>` 的缓冲区是连续的 `A` 值，不得
  为每个元素自动创建对象头、`Box<A>` 或额外指针；
- 若 `A` 的字段包含普通 `class`、`Box`、`Rc` 或其他间接 owner，这些 handle 作为 `A` 的
  字段内联，只有其 payload 保持间接存储；
- 若 `C` 是普通 `class`，`List<C>` 的缓冲区连续存储 `C` 的 owner/reference handle，
  `C` 对象本体仍按普通 `class` 规则分配；
- 只有用户显式写出 `List<Box<A>>` 时，缓冲区才存储 `Box<A>` handle，并由各个显式 `Box`
  分别拥有对应堆对象。

容器元素 `T` 必须是 **structurally storable type**：完成类型替换 / 单态化后有有限且具体的
逻辑表示。Phase 2 只判这一与目标无关的 layout kind，不读取 LLVM DataLayout；Phase 4 才对
具体 target 判断 size / alignment / stride 是否可表示。

- 数值类型、`Boolean`、`Char`、`Unit`、`String`、普通 `class` / `Box` / `Rc` handle、有限
  `value class` 与有限 `enum class` 在结构上可存储；这不表示它们满足 `Copyable`；
- 函数值只有在第 4 节所述单态化后具体函数指针 / 闭包环境布局已经确定时可存储；
- `T?` 在 `T` structurally storable 时也 structurally storable，tag / niche 的具体布局由
  runtime ABI ADR 和目标布局决定；
- `Any` 只是顶层泛型上界，裸 interface 没有 v1 `dyn` 表示，`Nothing` 没有可构造值；三者及
  `Nothing?` 在 v1 都不能直接作为顺序容器元素；
- 泛型声明中的 `Array<T>` 可以先保留 `T`，但每个实际单态化实例都必须通过该检查；不得为
  通过检查而擦除成 `Any` 或自动改成 `Box<T>`。

### 封闭的构造操作

v1 只预声明以下顺序容器构造操作：

- `arrayOf(...)`、`listOf(...)`、`mutableListOf(...)` 分别构造三种容器。存在显式 expected
  container type 时，其唯一类型实参就是 `T`，每个元素都按该类型检查；`null` 只能在该
  expected `T` 是可空类型时作为元素，`Nothing` 表达式可以按 bottom-type 规则适配该 `T`。
  没有 expected type 时，至少要有一个元素，由第一个元素的静态类型确定 `T`，其余元素必须
  具有同一静态类型，
  不在这里推导公共父类型或擦除为 `Any`；首元素自身若只有 `Nothing` / 无上下文 `null` 类型，
  同样不能据此确定可存储的 `T`。空调用必须由 expected type 或 SPEC-0008 的显式
  use-site 类型实参提供 `T`，否则产生“无法推导元素类型”的类型诊断；
- `Array<T>(size, initializer)` 与 `List<T>(size, initializer)` 是运行时长度构造的调用
  形式：callee contract 将 `size` 登记为 `Value`、`initializer` 登记为 `Borrow`；`size`
  的类型为 `Int`，`initializer` 的类型为 `(Int) -> T`；随后按 `0` 到 `size - 1` 的升序，
  以每个索引恰好调用一次。已有 place 或临时表达式作为实参都不需要写参数模式（`borrow`
  仍可选择显式写在 `initializer` 实参前，纯粹为了可读性）；
- 空的 `MutableList<T>()` 配合取得元素所有权的 `add` 等 Phase 5 API，从未知数量的运行时
  数据源逐步构造动态容器。

这些拼写是编译器预声明、不可被用户重载的核心构造操作，不依赖用户声明 `vararg`。它们在
编译器的统一 callable contract 中保存有序参数契约；列表式构造使用内部重复 `Value` 形状，
运行时长度构造使用上条固定的 `Value` / `Borrow` 形状。Parser 仍把它们解析为普通调用 AST；
名称解析确认预声明符号后，类型检查才建立专用的 typed construction 节点。因此 Phase 1
不按名称硬编码语义，也不提前推导元素类型。

列表式构造按源码从左到右对每个元素表达式求值一次，并把完整结果直接初始化进缓冲区。临时
表达式与已有 place 交付元素都写作普通调用实参，不需要额外标注，例如
`listOf(endpoint)`。`Copyable` place 交付 owned copy，其他 place 发生移动。运行时长度
构造遵守同一规则：无论 `size`、`initializer` 是已有 place 还是临时表达式，调用点都写作
`Array<T>(size, initializer)`，不需要额外标注；编译器按 callee 已声明的 `Value` /
`Borrow` 契约自动决定复制或移动 `size`，并在本次调用期内借用 `initializer`。构造先对
`size` 求值一次并拒绝负值，再对 initializer 求值一次。随后以目标地址宽度受检计算
`size * stride(T)` 并在需要物理字节时取得完整缓冲区；确定性大小溢出或分配失败发生在任何
initializer 调用之前，但已经完成的 initializer 表达式求值副作用不回滚。分配成功后在整个
同步调用期间共享借用来自已有 place 的 initializer，并按索引升序调用；构造器不消费该
initializer，临时函数值在调用结束后按普通 ASAP 规则析构。每次返回的完整 `T` 直接交付对应
槽位。
`MutableList.add` 取得元素所有权（`Value` 契约），因此调用点写作 `list.add(element)` 即可，
无论 `element` 是已有 place 还是临时值都不需要标注；`Copyable` 元素交付 owned copy，否则
移动。所有形式都不得隐式 clone、retain 或装箱。

v1 没有异常展开。负长度、分配失败、受检大小溢出或元素求值中的 `error()` 都以 abort 终止
进程，不承诺在该路径运行部分构造清理；不得把“清理发生”作为可观察语义。正常完成构造后，
owner 析构时按高索引到低索引的顺序恰好析构每个元素，随后释放缓冲区。即使 `T` 是零大小但
带非平凡 drop glue，也必须按逻辑 `size` 执行对应次数。

`MutableList<T>` 扩容或重排时可以把已初始化元素移动到新缓冲区。该操作是 relocation / move
而不是用户可观察的复制；旧位置不再拥有值。存在任何有效元素借用时，不得执行会改变元素
地址或销毁该元素的扩容、缩容、删除、替换或重排。

### 索引是 place，不是隐式 owned 返回

顺序容器的 element-place 索引是编译器预声明的封闭能力，不是名为 `Indexable` /
`MutableIndexable` 的用户 interface，也不能作为用户泛型上界或由用户实现。三种内建
顺序容器通过该能力支持 `receiver[index]` / `receiver[index] = value`；`receiver.get(index)`
和 `receiver.set(index, value)` 不会绕过 place 规则，而是按普通成员查找得到“成员不存在”诊断。
开放自定义索引能力必须等待 place 返回与生命周期语法由后续 guide 定义，**目标排期为 v2**
（v0.11 新增标注）：与型变、`dyn` 一样属于明确延后而非无限期悬空的特性；具体产生式与
生命周期语法仍留给到时的独立 guide，本条不提前批准任何语法。

普通值读取以及调用实参中借用 / 独占借用索引先按源码顺序对 receiver 和 index 各求值
一次，紧接着做边界检查，再形成绑定到 receiver 有效期的元素 place。`container[i] = value`
是唯一例外，它不在 RHS 之前建立元素 place 或做边界检查，而是唯一遵循下文的替换顺序：

- `T: Copyable` 时，普通值读取，或在按值调用实参中写 `consume(container[i])`（`Value`
  契约，调用点不需要标注），都会从 place 取得 owned copy；
- `T` 不满足 `Copyable` 时，普通 owned 读取和作为按值调用实参的 `container[i]` 都必须产生
  所有权诊断，不得移出元素、留下未初始化洞，也不得自动改为 `Box<T>`；
- 在调用实参中，`use(container[i])` 对三种顺序容器都合法（`Borrow` 契约，标注可选，写作
  `use(borrow container[i])` 效果相同）；`mutate(&container[i])` 只对 `Array<T>` 和
  `MutableList<T>` 合法，且 `&` 标注仍是必需项（`Inout` 契约的调用点拼写是符号 `&`，
  不是关键字 `inout`）；
- `container[i] = value` 只对 `Array<T>` 和 `MutableList<T>` 合法，并使用下述唯一替换顺序；
- 只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代
  才能移出不可复制元素，其精确 API 由标准库 Spec 定义。

元素替换严格执行：receiver place 求值一次 → index 求值一次 → 右值求值一次并形成完整 owned
临时值 → 确认 receiver owner 仍有效且没有存续的冲突借用 → 按 RHS 执行后的当前 `size` 做一次
边界检查 → 把旧值移入编译器临时槽 → 把新值移入元素槽 → 析构旧值。形成 RHS 期间不保留元素
地址或提前建立可变借用，因此 RHS 可以按普通所有权规则修改同一容器；其副作用不会被回滚。
若 RHS 移动 / 析构 receiver，或新值携带指向待替换元素的借用，Phase 3 必须拒绝。最后两个
move / store 是不可失败的提交步骤；旧值的析构即使 abort，容器槽中也已经是有效新值。

v1 不提供顺序容器 `getOrNull`。当前类型系统既不能用普通 `T?` 表达可空借用，也没有声明
“仅当 `T: Copyable` 时才出现某成员”的规则；为它添加隐式 clone、条件成员或编译器特判都会
扩大模型。安全访问必须显式检查 `index in 0..<container.size`，随后在成立分支中复制
`Copyable` 元素，或把非 `Copyable` 元素 place 作为 `borrow` / `&` 调用实参。未来若引入
可空借用或条件 API，再由新 guide 定义安全访问器。

具体越界与可变性规则如下：

| 类型 | 长度 / 可变性 | 越界或缺失行为 | 安全访问方式 |
|---|---|---|---|
| `Array<T>` | 构造后长度固定，元素可替换 | `array[i]` 越界触发 `error()` | 显式检查 `i in 0..<array.size` |
| `List<T>` | 运行时长度，只读 | `list[i]` 越界触发 `error()` | 显式检查 `i in 0..<list.size` |
| `MutableList<T>` | 可增删，元素可替换 | `list[i]` / `list[i] = v` 越界触发 `error()` | 显式检查 `i in 0..<list.size` |

### `Map` / `MutableMap` 保持 v0.5 表面契约，实施延后

`Map` / `MutableMap` 不实现上述顺序容器 indexed-place 能力，因为“键可能不存在”与
“槽位一定存在”的契约不同。本版不改变 v0.5 已有的表面契约：`map[key]` 返回
`V?`，`getValue(key)` 缺失时调用 `error()`，`mutableMap[key] = value` 插入或覆盖。这些只是
保留的可观察表面，不表示已经获得可实施的类型、所有权或 runtime 契约。

本 guide 的 `==` 只定义表达式语法和优先级，没有定义任意可存储 `K` 的通用等价关系；v1 也尚未
定义 `Hashable` 或其他 key capability。因此不得把对 `K` 调用 `==`、使用对象地址或临时引入
哈希当作本版已批准的 Map 实现。Map 的 key 等价性、value 返回所有权、查询对 key 的借用、
修改 API 的参数模式与表示 / 算法必须由后续新 guide 一次封闭，然后才能进入实施 Spec。

后续设计可优先评估“查询借用 key、限制 value 为 `Copyable`、修改使用显式 `put` 参数模式”
的方向，但该句只是后续设计方向，不是本版语义，不得据此实现或编写验收。

### 分配、禁止的隐式表示与 codegen 优化

规范基线中，需要物理字节的非空顺序容器缓冲区由 v1 系统分配器取得，并由唯一 owner 在其
ASAP 析构点释放。`size == 0` 或 `stride(T) == 0` 时可以不请求分配，但逻辑 `size`、边界检查
和每个元素应执行的 drop glue 次数仍必须保持；这不构成第二种容器表示。分配字节数必须以
目标地址宽度受检计算 `size * stride(T)`；计算溢出、无法表示的布局或分配失败必须终止程序，
不能环绕后继续访问较小缓冲区。owner header 位于栈帧不表示元素缓冲区也位于栈帧。v1 不
暴露 allocator hook、原始分配地址或稳定的分配次数；元素 place 的身份 / 别名、求值与析构
顺序属于语义，物理 allocator 调用及资源耗尽发生点不属于程序可依赖的可移植语义。

元素 place 的源语言身份是“逻辑容器身份 + 索引”；移动 owner 转移该身份，不创建新容器。
即使 `stride(T) == 0`、不同索引最终使用同一对齐 sentinel，它们也仍是不同 place，Phase 3
按逻辑索引而非物理地址判断借用冲突；索引确定相同则冲突，无法证明不同则保守视为可能冲突。
runtime ABI ADR 决定零大小元素的 sentinel；LLVM lowering 不得把零步长 GEP 当成可解引用的
真实元素字节，但仍须按逻辑索引执行边界检查和 drop。

v1 明确禁止：

- 根据元素大小、数量、逃逸结果、泛型调用或优化级别，在 `T` 与 `Box<T>` 之间自动转换；
- 在插入、索引、参数或返回边界隐式装箱 / 拆箱元素；
- 在标准 `Array<T>`、`List<T>` 或 `MutableList<T>` header 内预留 small-buffer storage；
- 使用“短容器内联、长容器堆分配”的 tagged 双表示；
- 先在栈上创建缓冲区，再因运行时发现逃逸而搬到堆上。

编译器可以按 as-if 规则把缓冲区分配替换为固定栈存储、SSA 标量或完全消除，但该优化不是
源语言保证，也不是 Phase 4 正确性门槛。优化必须保持源语言可观察的元素 place 身份 / 别名、
求值顺序、边界检查、所有权、借用有效期和析构次数 / 顺序。v1 禁止为运行时长度生成动态
`alloca`，也禁止运行时栈 / 堆迁移。正确的堆基线完成后，可由独立的 Phase 4+ 优化 Spec 从
“编译期已知小尺寸且证明不逃逸”的保守场景开始；具体分析、阈值和是否成功不进入语言规范。

目标 ABI 可以用等价的间接地址形式传递大型 `value class` 或容器 header。这只是调用约定，
不创建 `Box<T>`、不授予独立堆所有权，也不允许地址逃逸源语言生命周期；后端不得仅为按值
参数或返回约定而隐式请求系统堆。具体 calling convention 术语与字段规则留给 runtime ABI ADR。

类型大小同样不得触发表示变化。目标布局确定后，编译器应以对应 Spec 分配的稳定 warning
code 报告超过目标相关阈值的静态栈帧或隐式 `Copyable` 大值复制，并建议用户显式选择
`Box<A>`、动态顺序容器或调整算法。阈值属于编译器 / target 配置而非语言常量；warning
不得授权自动装箱，也不得把本可通过 ABI 间接传递消除的临时复制误计为必然成本。

### 为未来 `Array<T, N>` 保留边界

v1 只支持单类型实参 `Array<T>`，其长度在运行时确定并在构造后固定。即使 `arrayOf(...)`
的元素数量是编译期常量，结果类型仍是 `Array<T>`，不产生隐藏的长度实参。

`Array<T, N>` 保留给未来“编译期长度属于类型”的内联数组设计，v1 不新增 `FixedArray`，
标准库也不得用一个普通的双类型参数同名声明占用该拼写。未来启用时必须由新 guide 同时定义
const argument 语法、类型等价、有限布局、所有权、ABI、过大内联值诊断以及它与动态
`Array<T>` 的显式转换；在此之前一律拒绝。

索引括号内语法上接受任意单个表达式，因此 `arr[1..3]` 在 Phase 1 被解析成“以 range
表达式为单个 key 的索引”，**不产生切片 AST 或切片语义**。Phase 2 中，顺序容器要求整数
key，因类型不匹配拒绝该写法。其他 indexed receiver 是否接受 range key 由它们各自的后续
契约决定；本版不用尚未封闭的 Map 契约提前批准该类型。区间切片语义仍列为 v2 特性。

## 9. 并发模型：v1 线程 + channel，跨线程闭包强制 `move`

```kotlin
val handle = thread(move { println("running in new thread") })
handle.join()

val (sender, receiver) = channel<Int>()
thread(move { sender.send(42) })
val value = receiver.receive()
```

**关键修正**：`thread()` 的函数类型参数标注为 `move () -> Unit`（而非普通 `() -> Unit`）。函数类型可以带 `move` 前缀，表示**只接受不含任何借用捕获的闭包**——编译器在调用点强制要求实参闭包字面量显式带 `move` 关键字，且检查该闭包体内不能捕获任何借用语义的外部变量。这对应 Rust `std::thread::spawn` 要求闭包满足 `'static` 的设计初衷：一个借用捕获的闭包若被传进新起的 OS 线程，原函数栈帧完全可能在子线程还在跑的时候就已经返回销毁，产生悬垂引用。所有跨线程 API（`thread`、`spawn` 类）的函数类型参数都必须用 `move (...)  -> T` 标注，这是所有权模型在多线程场景里真正发挥作用的地方，不能留空子。

- 配合 `Transferable` 标记能力做编译期数据竞争防护（跨线程传递的值必须满足该约束，具体
  判定规则见第 17 节）。**v1 只做标记能力检查，不做完整的多线程借用数据流分析**（列为
  v2），具体任务见 Phase 3。（v0.11 修订：v1 的跨线程 API 只转移所有权、没有共享/别名
  原语，因此实际只需要 `Transferable`；`Shareable` 连同“共享而不转移”的跨线程原语一并
  简化 / 推迟到 v2，详见第 17 节说明。）
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
| 内存分配器 | v1 直接用系统分配器（libc `malloc`/`free`）；顺序容器的单一表示与可选分配消除见第 8 节；自定义 allocator trait 列为 v2+，引入时不得静默改变既有容器语义 |
| 诊断错误码 | 参考 rustc 的 `E0382` 风格，从 v1 起给每类编译错误分配稳定错误码（如 `L0001`） |
| 编辑器语法高亮 | LSP 之外单独提供 TextMate/Tree-sitter 语法文件 |
| 自举计划 | 标准库从第一天用目标语言自身写；编译器本身“自举”列为长期目标（v4+） |
| 数组/集合字面量 | `arrayOf(1, 2, 3)`、`listOf(1, 2, 3)`；`mapOf("a" to 1)` 的表面拼写沿用 v0.5，但实施须等待第 8 节要求的新 guide 封闭 Map 契约 |
| 中缀函数（infix） | v1 表达式语法中的中缀调用集合精确封闭为软词 `to`；`infix` 自身只是普通标识符，后续仅可在标准库声明上下文解释，用户代码不开放自定义 `infix fun` |
| 位运算符（v0.11 新增，v0.14 更新） | `&`、`\|`、`^`、`~`、`<<`、`>>` 明确延后到 v2，与其他 v1 裁剪特性一样给出去向，不再是无版本标注的沉默省略；**v0.14 起 `&` 与其余 5 个不再对等**：`&` 已经是合法固定符号（用于第 9 节调用实参的 `Inout` 标注，见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)），`\|`/`^`/`~`/`<<`/`>>` 仍是完全未占用的非法字符/未定义 token。v2 把 `&` 设计成通用按位与运算符时不会与现有用法产生语法歧义——`&` 作为调用实参模式只能出现在实参的第一个 token 位置，作为二元运算符只会出现在两个操作数之间，两者在 parser 状态机里互斥，不需要额外消歧义规则；但 v2 设计者仍需要显式确认这条结论没有被后续语法演进打破，不能想当然假设“以前没问题所以现在也没问题”。**提醒**：`\|`/`^`/`~`/`<<`/`>>` 五个运算符一旦要在 v1 补回，涉及[02-lexical-spec.md](./02-lexical-spec.md)固定符号表与[03-grammar-core.md](./03-grammar-core.md)运算符优先级表（均已实现验收），补回成本会随后续 Phase 推进而上升，建议团队尽快明确是否需要提前到 v1，而不是拖到 v2 规划时才决定 |

## 11. 解构声明与 `componentN()` 约定

```kotlin
value class Pair<A, B>(val first: A, val second: B)

fun <T> channel(): Pair<Sender<T>, Receiver<T>> { ... }

val (sender, receiver) = channel<Int>()
```

该示例说明解构一旦被相应语法上下文接纳后的类型与所有权语义，不表示本版已经开放顶层
解构。本版的 Phase 1 只由 SPEC-0013 接纳 block / lambda body 内的局部 `val` 解构；独立
声明入口和未来完整文件顶层继续以 unsupported destructuring context 拒绝。若后续 guide
开放其他上下文，必须复用本节“一次求值、完整分量、复制或原子消费”的语义并建立独立 Spec。

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

真实 Kotlin 的 `val x: Int get() = ...` / `var y: Int set(value) { ... }`（自定义属性访问器）**在本语言 v1 中不支持**，和扩展函数一样列入“去掉的语法糖”清单：属性访问始终直接对应存储字段，不允许拦截读写。原因：自定义访问器要求类型检查器处理访问器返回类型与声明属性类型的一致性校验、`field` 关键字访问底层存储等额外机制，复杂度收益比低，与“去掉语法糖、保留核心”的项目定位一致。

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

早前文档中曾出现“`dyn Shape` 语法用于异构集合的动态分发”的表述，与关键字表状态不一致。**明确结论：`dyn` 是保留关键字，但对应的动态分发语法在 v1 不实现**，v1 泛型/接口全部走单态化静态分发。如需要异构集合场景，v1 阶段用 `enum class` 包装各具体类型来模拟（这也是 Rust 在没有 trait object 时的常见替代方案）。

## 16. 整数运算的溢出与除零语义（v0.11 新增）

此前 guide 没有为 `Int`/`Long` 等整数类型的算术运算定义溢出行为，也没有为整数除零定义
语言级契约（本文档附录“核心结构声明总览”的除法示例依赖用户手写 `if (b == 0) error(...)` 自行 guard，没有说明
不 guard 时会发生什么）。本节补齐这一空白，不影响已实现的词法/语法产生式。

- **v1 采用 checked 语义**：`+`、`-`、`*` 在结果超出目标类型可表示范围时触发 `error()`
  （abort），与第 8 节顺序容器分配大小溢出已经采用的“溢出即终止进程”惯例保持
  一致，不做静默环绕（wrapping）。
- **整数除法 `/` 和取余 `%` 在除数为零时同样触发 `error()`**；`Int.MIN_VALUE / -1`（结果超出
  可表示范围）按上一条溢出规则同样触发 `error()`。
- **浮点（`Float`/`Double`）不适用本节的 checked 语义**，按 IEEE 754 语义处理：除以零产生
  `Infinity`/`-Infinity`/`NaN`，不触发 `error()`，不视为程序错误。
- 编译器可以在证明操作数范围后，把 checked 检查优化为 as-if 等价的更快代码路径，但不能
  把它优化成静默环绕——这与第 8 节“编译器可以按 as-if 规则优化分配，但不能改变
  可观察语义”的原则一致。
- 无符号类型（`UByte`/`UShort`/`UInt`/`ULong`）的溢出同样按 checked 语义处理，不做隐式
  环绕。
- v1 不提供显式的 wrapping / saturating / checked 变体方法（如 Rust 的 `wrapping_add` /
  `saturating_add`）；如果后续证明性能敏感场景确实需要环绕语义作为显式操作，应作为标准
  库方法在 Phase 5 之后按需引入，不修改默认 `+`/`-`/`*` 的 checked 语义。

这一节把“溢出 / 除零”从“用户必须自己记得 guard 的隐性契约”变成一条显式、和容器分配
溢出一致的语言级规则；具体 codegen（如何插入溢出检查、能否用 LLVM intrinsic）留给
Phase 4，本节只封闭源语言可观察语义。

## 17. `Transferable` 标记能力（v0.11 新增）

第 9 节要求跨线程 API 检查 `Shareable`/`Transferable` 标记能力，但两者都没有给出
具体判定规则。本节补齐，并简化范围：

- **v1 的跨线程 API 只转移所有权**（`thread()`、`Sender.send()` 都是 `Value` 契约的按值
  参数，没有“值仍保留在原线程、同时可被子线程引用”的共享原语），因此 v1 实际只需要判断“这个值能否被
  安全地整体移动到另一个线程”，不需要“这个值能否被多个线程同时引用”——后者
  （`Shareable`，类似 Rust `Sync`）要等到共享 / 引用计数式跨线程原语被设计出来才有意义。
  **本节把 `Shareable` 的具体规则推迟到 v2**，与该跨线程共享原语一起设计；v1 只定义并
  检查 `Transferable`。
- **`Transferable` 是编译器结构化递归推导的能力**，规则与 `Copyable`（第 5 节）
  平行：
  - 数值类型、`Boolean`、`Char`、`Unit`、`String` 满足 `Transferable`；
  - `value class` 当且仅当其全部字段类型都满足 `Transferable` 时满足 `Transferable`；
  - 普通 `class` 满足 `Transferable`，当且仅当其全部字段类型都满足 `Transferable`——转移
    一个 `class` 实例的唯一所有权本身是安全的（所有权检查已经保证原绑定不再可用），真正
    的风险只在于它内部是否持有不适合跨线程移动的资源；
  - `Box<T>` 满足 `Transferable` 当且仅当 `T` 满足 `Transferable`；
  - **`Rc<T>` 恒不满足 `Transferable`**，不论 `T` 是什么：`Rc<T>` 依赖非原子引用计数，移动
    一个 `Rc` 到另一线程后，原线程完全可能仍持有其他指向同一 refcount 的 `Rc` 句柄，两个
    线程同时触碰非原子计数即是数据竞争。这条规则直接对齐 Rust `Rc<T>: !Send` 的结论；
  - `Array<T>` / `List<T>` / `MutableList<T>` 满足 `Transferable` 当且仅当 `T` 满足
    `Transferable`（容器本身独占所有权，规则随元素递归）。
  - 与 `Copyable` 一样，`Transferable` 由编译器内部标识识别，可用作泛型上界
    （`<T : Transferable>`），v1 不提供用户手动实现、否定或覆盖的语法。
- 第 9 节“配合 `Shareable`/`Transferable` 标记 trait 做编译期数据竞争防护”一句已
  同步简化为“配合 `Transferable` 标记能力做编译期数据竞争防护”；Phase 3 的验收标准同步
  只检查 `Transferable`。

## 18. `Map` / `MutableMap` 所有权契约（候选设计，v0.11 新增）

第 8 节把 `Map`/`MutableMap` 的可实施契约明确留给“后续 guide”。本节给出一份
候选设计，风格上对齐第 8 节顺序容器已经采用的 place / temporary、`Value`/`Borrow`/`Inout`
术语体系，但**这仍然是候选方向，不是本 candidate 直接批准的实施契约**——按本文档既有
的治理惯例，它需要独立走完设计评审（类比第 8 节顺序容器从 v0.5 表面契约到
现在走过的过程），才能进入实施 Spec。在被正式批准前，Phase 2/3/5 的编译器实现不得
依据本节内容加入 `V : Copyable`、key 借用、`put` 或下标赋值的具体检查。

**18.1 Key 等价关系：`Hashable`**

- 新增编译器预声明的规范 marker trait `Hashable`，判定规则与 `Copyable`/`Transferable`
  同构：数值类型、`Boolean`、`Char`、`String` 满足 `Hashable`；`value class` 当且仅当全部
  字段满足 `Hashable` 时满足 `Hashable`，按字段递归计算结构相等与结构哈希。
- **`Hashable` 蕴含 `Copyable`**：v1 只允许 `Copyable` 的 key 类型，回避“查询时是否需要
  借用 key、key 是否可能被移动导致哈希失效”这一整类问题。普通 `class`、`Box<T>`、包含
  它们的 `value class` 都不满足 `Hashable`，因此不能作 key。这是一个保守但可以先落地的
  起点；是否放开不可复制 key 留给后续版本评估。
- `==` 对满足 `Hashable` 的类型按字段结构相等定义（数值/`Boolean`/`Char`/`String` 走对应
  类型的原生相等）；这填补了第 10 节遗留的“`==` 未定义任意可存储 `K` 的通用
  等价关系”的空白，但**只在 `Hashable` 类型范围内**，不为 `Hashable` 之外的类型定义结构
  相等。

**18.2 容器角色与所有权**

比照第 8 节顺序容器的分工：

- `Map<K, V>` 是运行时确定大小、构造后**只读**的独占 owning container（`K : Hashable`，
  `V` 任意 structurally storable 类型）。
- `MutableMap<K, V>` 是可插入、可覆盖、可删除的独占 owning container；只有它提供改变
  条目数量的操作。
- 二者预声明只读属性 `size: Int`，与顺序容器一致不满足 `Copyable`（独占一段哈希表存储
  的所有权）。
- 二者是不同名义类型，v1 不提供隐式相互转换。

**18.3 查询：自动借用 key，按 `V` 的 `Copyable` 能力决定返回**

```kotlin
val v: V? = map[key]               // 查询自动借用 key,不取得 key 所有权
val exists: Boolean = key in map   // 复用第 4 节 in 运算符
```

- 下标查询 `map[key]` 的 `key` 是 `Borrow` 契约：调用点不需要标注，编译器自动按借用交付
  key（写作 `map[borrow key]` 效果相同，仍然合法，纯粹是可选的可读性写法）。查询过程只
  需要 key 的值用于比较 / 哈希，不需要取得其所有权。
- 返回值固定为 `V?`：`V` 满足 `Copyable` 时返回复制的值或 `null`；`V` 不满足 `Copyable`
  时，`map[key]` 的普通按值查询在 Phase 2 产生所有权诊断（不能移出一个仍在 map 里的
  元素），必须改用借用形式的成员查询（例如 `map.getRef(key)`，`key` 同样是 `Borrow`
  契约、标注可选，返回值的借用绑定到 `map` 的有效期，精确的借用返回语法留给
  第 8 节已经预告的“place 返回与生命周期语法”到位后再定）。
- `getValue(key)` 沿用 v0.5 表面契约：缺失时调用 `error()`；返回类型规则与 `map[key]`
  相同。

**18.4 修改：显式 `put`，参数模式对齐顺序容器**

```kotlin
mutableMap.put(key, value)   // 插入或覆盖,取得key与value的所有权,调用点不需要标注
mutableMap.remove(key)       // 按key删除,不取得key所有权,调用点不需要标注
```

- `MutableMap.put(key, value)` 的契约固定为 `Value K`、`Value V`：插入新条目取得两者
  所有权；覆盖已有 key 时，新 `value` 移入、旧 `value` 按顺序容器替换协议（
  第 8 节）的思路析构一次，旧 `key` 同样析构一次（`Hashable` 蕴含 `Copyable`，析构总是
  平凡的）。调用点不需要任何标注。
- `MutableMap.remove(key)` 的契约固定为 `Borrow K`（查询用，不需要取得删除目标 key 的
  所有权，只需要用来定位条目；调用点标注可选）；返回被移除的 `value`（`Value` 语义
  交付给调用者，`V` 不满足 `Copyable` 时同样合法，因为整条记录被移出后不再有第二个
  访问路径）。
- `mutableMap[key] = value` 是 `put` 的下标语法糖，契约与顺序容器的 `container[i] = value`
  类似，但由于 key 可能不存在，不能复用“槽位一定存在”的替换协议，统一脱糖为 `put`。
- v1 不提供 `getOrPut`、`merge` 等高阶修改方法，这些留给 Phase 5 在本契约之上按普通
  方法实现。

**18.5 与顺序容器共享的边界**

- Map 的哈希表内部结构（开放寻址 / 链式、扩容策略、迭代顺序）属于运行时 ABI 决定，
  不是源语言可观察语义，与第 8 节对顺序容器 allocator 细节的态度一致。
- `MutableMap` 扩容 / rehash 时，正在被借用的 `value`（通过 18.3 节的借用查询）与顺序
  容器的元素借用一样，构成借用冲突，必须被 Phase 3 拒绝。
- v0.18 已定义 `for ((k, v) in map)` 的单次求值与 `iterator()` / `hasNext()` / `next()`
  调用形状；具体名义接口声明、Map iterator 的元素所有权和运行时布局仍留给 Phase 2/5。

## 19. `Result<T, E>` 错误值与 postfix `?`（v0.19 正式启用）

v1 把可预期、可恢复的失败表达为普通返回值，不提供异常体系。源语言精确没有 `throw`、
`try`、`catch`、`finally`、`throws`、可捕获异常类层级或异常栈展开；函数返回类型
`Result<T, E>` 本身就是完整失败契约，不再用第二个关键字重复声明。合法但没有值使用 `T?`，
可恢复失败使用 `Result<T, E>`，程序不变量破坏使用不可捕获的 `error()` abort，三者不得
混用。

```kotlin
fun readConfig(path: String): Result<Config, IoError> {
    val text = File.readText(path)?    // Err 分支在此提前返回
    val parsed = parse(text)?
    return Ok(parsed)
}
```

- `expr?` 只传播 `Result`。它所在最近 callable 的返回类型必须是 `Result<T, E>`，operand
  类型必须是 `Result<U, E>`，其中 `E` 精确相同。v1 不把 `?` 开放为用户可实现协议，不传播
  `T?`，也不做 `Into`/`From` 式隐式错误转换；错误类型变化必须由显式 `when` 或后续标准库
  `mapError` 完成。
- 语义上，`expr?` 等价于：

  ```kotlin
  val __tmp = expr
  when (__tmp) {
      is Ok -> __tmp.value
      is Err -> return __tmp
  }
  ```

  即：`expr` 只求值一次；`Ok` 分支时整个表达式的值是内部的 `value`（按 `Copyable` 规则
  复制或移动，和第 11 节 `enum class` 变体字段访问的规则一致）；`Err` 分支时
  从最近 callable `return` 整个 `__tmp`（而不是重新构造一个新 `Err`）。具名函数和 lambda
  都是 callable boundary；lambda 内的 `?` 只退出该 lambda，绝不从外层具名函数非局部返回。
- `?` 的优先级与 `!!` 相同，归入[03-grammar-core.md](./03-grammar-core.md)第 4 节运算符层级表的第 1 级（postfix，左结合，
  可连续）。由于 `?.` 是 Kotlin safe-member 的单一最长匹配 token，传播后立即访问普通成员
  必须显式分组为 `(foo()?).bar`；`foo()?.bar` 永远表示 safe member，不解释为 `foo()?` 后
  接 `.bar`。
- `?` 与 `!!` 的差异：`!!` 面向 `T?`，失败时 `error()`（abort，不可恢复）；`?` 面向
  `Result<T, E>`，失败时是**callable 级别的普通提前返回**，把错误值交还给调用者，不终止进程。
  二者不能混用（不能对 `Result<T, E>` 用 `!!`，也不能对 `T?` 用 `?`）。
- 与第 5 节所有权规则的交互：`expr?` 对不满足 `Copyable` 的 `T` 同样成立，`Ok`
  分支消费 `__tmp` 并移出其 `value`（单一分量的消费式解构，复用第 11 节的
  机制）；`Err` 分支整体移动 `__tmp` 用于 `return`。

Phase 1 只建立 postfix AST，不拥有 callable 返回类型或 `Result` 名称绑定信息，因此在所有
expression context 接受 `?`；上述 callable、operand 与 `E` 约束由 Phase 2 形成类型 / 上下文
诊断。Phase 3 把 `Err` 传播视为普通 return 所有权路径，Phase 4 在该路径生成正常 drop，
不得引入 unwind cleanup。应用边界使用 `when` 选择恢复、转换、报告或调用 `error()`；
`Result` 不“接住异常”，因为该模型中没有异常被抛出。

## 20. `Copyable` 显式 opt-out（候选方向，v0.11 新增）

第 5 节的 `Copyable` 完全由字段结构自动、递归推导，v1 明确不提供用户手动
否定或覆盖的语法。这是一个刻意的简化，但也意味着丢失了一个常见模式：即使一个
`value class` 的全部字段都是数值类型，设计者也没有办法强制它“只能移动、不能被意外
复制”（例如用来防止两个语义不同的 ID 被复制后混用）。

本节**不改变** v1 的既有规则，只记录一个候选方向，供 SPEC-0017（class-family 语法）在
设计 `value class` 声明语法时一并评估：

- 方向 A：引入声明修饰符（例如 `nocopy value class UserId(val raw: Int)`），显式让该
  类型及递归包含它的聚合退出自动 `Copyable` 推导，即使结构上满足条件。
- 方向 B：保持现状，把“防止 ID 混用”这类需求交给命名 / lint 层面而非类型系统解决，
  避免为一个相对小众的需求增加新关键字和新的 Phase 2 特判分支。

两个方向各有成本：方向 A 增加语言复杂度（新修饰符、`Copyable` 推导规则出现例外
分支），方向 B 保留现有简洁性但放弃这类强类型保证。本候选方向不做取舍，留给
SPEC-0017 连同其他 class-family 声明修饰符一并决定。

---


---

## 附录：核心结构声明总览（原第二部分）

> 原文档第二部分独立成章，本次拆分中并入设计决策文档作为收尾附录：这段示例把第 1–20
> 节讨论过的各类声明（`value class`、`class`、`interface`、`enum class`、`object`、
> `companion object`、泛型函数）放在一起，给出一个整体印象。

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
