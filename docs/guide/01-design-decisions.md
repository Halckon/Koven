# Koven 语言设计规范 · 核心设计决策

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第一、二部分），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。现行内容版本：v0.32。
> v0.13 拆分只重组文件结构，不改变任何已定义语义；v0.14 的 `&` 调用点语义及同步修改见
> [`07-changelog-archive.md`](./07-changelog-archive.md)。本文档覆盖第 1–24 节的现行设计决策；
> 第 25 节是 v0.25 已启用的现行规则；第 26 节是 v0.26 已启用的现行规则；第 27 节是
> v0.27 已启用的现行规则；第 28 节是 v0.28 已启用的现行规则；第 29 节是 v0.29 已启用的
> 现行规则；第 30–32 节分别是 v0.30–v0.32 已启用的现行规则；
> 第 33–35 节分别是尚未启用的 v0.33–v0.35 候选；附录收录原第二部分的核心结构声明总览。

> **阅读说明（v0.20 更新）**：本部分示例使用的 control-flow 已由
> [04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md) §12 正式定义；
> class-family 的表面语法、成员边界、AST 与恢复契约已由同文件 §13 正式定义。示例只用于
> 解释设计意图；与产生式冲突时以 §12、§13 的明确规则为准，不能从 Kotlin 或示例补出
> 未列出的构造器、匿名对象、继承或成员形式。

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
    const val VERSION: String = "1.0"
}
```

具名 `object` 同时声明一个名义类型和该类型的唯一值。v1 的该值不携带运行时存储状态：
body 只允许 `const val` 与普通成员函数，不允许普通 `val` / `var`、初始化块或惰性状态。
`const val` 在编译期求值；成员函数可以执行普通 v1 运行时代码并可使用 `this`，因为函数代码
本身不是 singleton 存储状态。具名 `object` 可以实现接口并参与单态化静态分发，但 v1 不把
它擦除为裸 interface 或 `dyn`。运行时状态、惰性初始化与共享可变 singleton 统一延后到 v2。

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
  第 9 节的 callable 参数契约：无标记或显式 `borrow` 都是共享借用 `Borrow`；显式 `own`
  映射既有 `ParameterMode::Value`（`Copyable` 则复制、否则移动）；显式 `inout` 是独占可变
  借用。具名函数参数使用同一契约，`(T) -> R` 与 `(borrow T) -> R` 规范化为同一函数类型，
  `(own T) -> R` 则是不同的 Value contract。**调用点是否需要
  书写 `borrow` 由编译器按 callee 已声明的契约自动判定；只有 `Inout` 契约仍要求调用点
  显式标注，但调用点的拼写是符号 `&` 而不是关键字 `inout`**（`&x` 而非 `inout x`），
  Value 调用也保持无 marker：声明写 `own`，调用仍写 `consume(x)`，不写 `consume(own x)`。
  详见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节的完整规则与设计说明。
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
- **使用规则由能力决定**：满足 `Copyable` 的值在赋值、返回或传给声明端 `own` 的
  `Value` 参数时可以隐式复制，
  原值仍可使用；不满足 `Copyable` 时，同样的位置转移所有权，原值随后不可使用。调用实参
  仍须遵守 callee 的 `Value` / `Borrow` / `Inout` 契约；`Inout` 在调用点仍强制要求显式
  标注，不因类型可复制而省略——**这是唯一强制要求调用点 marker 的契约，调用点写作符号
  `&`（例如 `mutate(&x)`），不是关键字 `inout`；关键字 `inout` 只出现在声明侧**，理由见
  [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节的设计说明。`Value` 与 `Borrow` 均不要求调用点 marker
  （`borrow` 仍可选择显式写出，纯粹为了可读性）。
  因此若声明是 `fun consume(own value: T)`，`consume(x)` 对 `Copyable` 的 `x` 交付一个
  owned copy，对不可复制的 `x` 则移动
  原值，调用点都不需要额外标注。
- 本文出现的“取得所有权的参数”专指声明端显式 `own` 所映射的 `Value` 参数；v0.26 没有
  恢复 v0.10/v0.11 的独立 `Own` 契约，只恢复了 Value 的表面拼写。标准库 Spec 必须按
  [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节登记具体契约；不得按函数名或参数类型把
  无标记 Borrow 猜成 `Value` 或 `Inout`。
- class / value-class 主构造器的 `val` / `var` 字段与 enum payload 是天然-owned 存储形态，
  构造时按 `ParameterMode::Value` 交付但不重复写 `own`。这只是存储声明的专用语法，不允许
  普通 callable 用 `val` / `var` 代替 `own`，也不改变无标记 callable 参数的 Borrow 默认值。
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
移动。`Box(...)` 的构造参数是声明端 `own` 所映射的 `Value` 契约，调用点不需要写任何标注：

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
    Circle(radius: Double),
    Rectangle(w: Double, h: Double),
    Point;

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
  形式：callee contract 将 `size`、`initializer` 都登记为 `Borrow`；`size`
  的类型为 `Int`，`initializer` 的类型为 `(Int) -> T`，其中无标记的索引参数同样是 Borrow；
  随后按 `0` 到 `size - 1` 的升序，
  以每个索引恰好调用一次。已有 place 或临时表达式作为实参都不需要写参数模式（`borrow`
  仍可选择显式写在 `initializer` 实参前，纯粹为了可读性）；
- 空的 `MutableList<T>()` 配合取得元素所有权的 `add` 等 Phase 5 API，从未知数量的运行时
  数据源逐步构造动态容器。

这些拼写是编译器预声明、不可被用户重载的核心构造操作，不依赖用户声明 `vararg`。它们在
编译器的统一 callable contract 中保存有序参数契约；列表式构造使用内部重复 `Value`
（等价于重复声明端 `own`）形状，运行时长度构造使用上条固定的两个 `Borrow`。Parser 仍把它们解析为普通调用 AST；
名称解析确认预声明符号后，类型检查才建立专用的 typed construction 节点。因此 Phase 1
不按名称硬编码语义，也不提前推导元素类型。

列表式构造按源码从左到右对每个元素表达式求值一次，并把完整结果直接初始化进缓冲区。临时
表达式与已有 place 交付元素都写作普通调用实参，不需要额外标注，例如
`listOf(endpoint)`。`Copyable` place 交付 owned copy，其他 place 发生移动。运行时长度
构造遵守同一规则：无论 `size`、`initializer` 是已有 place 还是临时表达式，调用点都写作
`Array<T>(size, initializer)`，不需要额外标注；编译器按 callee 已声明的两个 `Borrow`
契约在本次调用期内借用 `size` 与 `initializer`。构造先对
`size` 求值一次并拒绝负值，再对 initializer 求值一次。随后以目标地址宽度受检计算
`size * stride(T)` 并在需要物理字节时取得完整缓冲区；确定性大小溢出或分配失败发生在任何
initializer 调用之前，但已经完成的 initializer 表达式求值副作用不回滚。分配成功后在整个
同步调用期间共享借用来自已有 place 的 initializer，并按索引升序调用；构造器不消费该
initializer，临时函数值在调用结束后按普通 ASAP 规则析构。每次返回的完整 `T` 直接交付对应
槽位。
`MutableList.add` 取得元素所有权（声明端 `own` 映射的 `Value` 契约），因此调用点写作 `list.add(element)` 即可，
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

- `T: Copyable` 时，普通值读取，或向声明端 `own` 的 `Value` 参数写
  `consume(container[i])`（调用点不需要标注），都会从 place 取得 owned copy；
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

## 12. 类型级关联成员：`companion object`

```kotlin
class Point(val x: Int, val y: Int) {
    companion object {
        const val DIMENSIONS: Int = 2
        fun origin(): Point = Point(0, 0)
    }
}

val p = Point.origin()
```

没有二级构造器、没有 `init` 块的情况下，类型级常量和工厂函数通过可选的
`companion object` 表达；没有类型级成员的类型完全不需要 companion。v1 把 companion
定义成**关联命名空间**，不是隐藏 singleton：

- 每个 `class` / `value class` / `enum class` / `interface` 至多一个匿名 companion；不接受
  `companion object Name`，也不产生可观察的 `Type.Companion` 值或对象身份；
- body 只允许 `const val` 与关联函数，不允许普通 `val` / `var`、嵌套类型或初始化块；
- 关联函数可以执行普通运行时代码、构造对象并返回 `Result`，但没有 `this`，也不能直接读取
  实例字段；“编译期可求值”只约束 `const val` initializer，不约束函数体；
- companion 不实现接口，不捕获 enclosing 类型参数。泛型关联函数必须自行声明类型参数，
  例如 `fun <T> identity(own value: T): T = value`；这里必须显式取得可能为 MoveOnly 的 `T`
  才能把它作为 owned 返回值交付。这里不使用 `Box<T>` 作为示例，因为第 5 节
  已要求 `Box` 的实参是具体 `value class`，未约束的 `T` 不能证明这一点；
- `Type.member` 在名称解析后直接指向关联函数符号或内联常量，不分配 singleton、不生成
  初始化 guard，也没有退出时析构。

interface 的固定协议常量同样放在 companion 中，例如
`interface Http { companion object { const val DEFAULT_PORT: Int = 80 } }`，并以
`Http.DEFAULT_PORT` 访问。它不被实现类型继承或 override。要求“每个实现类型各自提供一个
常量”的 associated-constant contract 是另一项未来能力，v1 不用相同语法悄悄引入。

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

v1 另保留窄化的**接口实现委托**作为组合工具：普通 `class` 可以在 supertype list 写
`Interface by field`，其中 `field` 必须是同一主构造器中的不可变 `val` 字段，字段的具体类型
必须静态满足该接口。`by` 只在此位置作为上下文软关键字；不允许任意 delegate expression、
`var` delegate、value/enum/object delegation 或运行时代理。编译器生成保持原 callable
规范化 Value/Borrow/Inout 与返回契约的转发，手写 `override` 优先，多来源冲突必须显式 override。
这项语法由 class-family 之后的独立 Spec 实施，不混入 SPEC-0017。

Kotlin 风格的属性委托 `val/var property by expression` 不属于 v1：它需要自定义 getter /
setter、惰性初始化与属性元数据，和第 13 节及无运行时反射边界冲突。

## 15. `dyn`（trait object 动态分发）降级为 v2

早前文档中曾出现“`dyn Shape` 语法用于异构集合的动态分发”的表述，与关键字表状态不一致。**明确结论：`dyn` 是保留关键字，但对应的动态分发语法在 v1 不实现**，v1 泛型/接口全部走单态化静态分发。如需要异构集合场景，v1 阶段用 `enum class` 包装各具体类型来模拟（这也是 Rust 在没有 trait object 时的常见替代方案）。

v1 也不提供 Kotlin/Java 风格的匿名内部类或 `object : Interface { ... }` object expression。
lambda 只实现函数类型这一个 callable 行为，不伪装成任意接口实例；单回调 API 应直接接受
函数类型，多方法或有字段的实现使用具名 class，简单包装优先使用上一节的接口委托。
匿名对象必须等 `dyn`、隐藏捕获布局、对象身份与逃逸所有权规则一并在 v2 设计后才能加入。

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

- **v1 的跨线程 API 只转移所有权**（`thread()`、`Sender.send()` 都使用声明端 `own` 映射的
  `Value` 参数，调用点保持无 marker；没有“值仍保留在原线程、同时可被子线程引用”的共享
  原语），因此 v1 实际只需要判断“这个值能否被
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
候选设计，风格上对齐第 8 节顺序容器已经采用的 place / temporary、声明端 `own` 所映射的
`Value` / 默认 `Borrow` / `Inout`
术语体系，但**这仍然是候选方向，不是本 candidate 直接批准的实施契约**——按本文档既有
的治理惯例，它需要独立走完设计评审（类比第 8 节顺序容器从 v0.5 表面契约到
现在走过的过程），才能进入实施 Spec。在被正式批准前，Phase 2/3/5 的编译器实现不得
依据本节内容加入 `V : Copyable`、key 借用、`put` 或下标赋值的具体检查。

**18.1 Key 等价关系：`Hashable`**

- 新增编译器预声明的规范 marker trait `Hashable`。它与 `Copyable` / `Transferable` 正交：
  数值类型、`Boolean`、`Char`、`String` 满足 `Hashable`；`value class` 当且仅当全部字段满足
  `Hashable` 时满足 `Hashable`，按字段递归计算结构相等与结构哈希。与其他编译器能力一样，
  v1 不开放用户手动实现或覆盖。
- **`Hashable` 不蕴含 `Copyable`**。v0.31 的 `String` 是 MoveOnly 但仍可稳定按 UTF-8 bytes
  哈希；查询只 Borrow 调用者的 key，插入则把 key owner 移入 Map，因此不需要为哈希而复制
  key。普通 `class`、`Box<T>`、`Rc<T>` 及包含这些 identity-bearing 字段的 `value class`
  默认不满足 `Hashable`；未来若发布 identity hash 必须另行定义，不能使用地址或引用计数
  control block 的偶然值。
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

- `MutableMap.put(key, value)` 的契约固定为声明端 `own` 的 `Value K`、`Value V`：插入新条目取得两者
  所有权；覆盖已有 key 时，新 `value` 移入、旧 `value` 按顺序容器替换协议（
  第 8 节）的思路析构一次，新 key 替换旧 key，旧 key 也按自身实际类型精确析构一次。
  MoveOnly `String` 等 key 因此没有复制或泄漏特例。调用点不需要任何标注。
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

## 20. `Copyable` 显式 opt-out（v0.20 定案：v1 不支持）

第 5 节的 `Copyable` 完全由字段结构自动、递归推导，v1 明确不提供用户手动
否定或覆盖的语法。这是一个刻意的简化，但也意味着丢失了一个常见模式：即使一个
`value class` 的全部字段都是数值类型，设计者也没有办法强制它“只能移动、不能被意外
复制”（例如用来防止两个语义不同的 ID 被复制后混用）。

v0.20 已结合 class-family 修饰符集合完成取舍：v1 保持结构化自动推导，不增加 `nocopy`
或等价否定修饰符。以下两个方向作为决策记录保留：

- 方向 A：引入声明修饰符（例如 `nocopy value class UserId(val raw: Int)`），显式让该
  类型及递归包含它的聚合退出自动 `Copyable` 推导，即使结构上满足条件。
- 方向 B：保持现状，把“防止 ID 混用”这类需求交给命名 / lint 层面而非类型系统解决，
  避免为一个相对小众的需求增加新关键字和新的 Phase 2 特判分支。

两个方向各有成本：方向 A 增加语言复杂度（新修饰符、`Copyable` 推导规则出现例外
分支），方向 B 保留现有简洁性但放弃这类强类型保证。v1 选择方向 B；方向 A 如需恢复，
必须由 v2+ 新 guide 重新定义关键字、推导和兼容性，SPEC-0017 不接受该拼写。

---

## 21. 单文件名称、作用域与预声明环境（v0.21）

v0.21 封闭 Phase 2 的第一条可执行边界：SPEC-0018 只处理一份已经成功解析的
`ParsedFile`，建立稳定符号身份、词法作用域和名称引用；不从文件路径猜测 package，也不
展开 `import`。名称解析与类型检查是两个阶段：前者回答“这个拼写指向哪个声明或候选组”，
后者才回答“该声明是否适用于这里”。

### 21.1 双命名空间与声明身份

- 每个词法作用域分别维护**类型命名空间**和**值命名空间**。类型参数以及
  `value class` / `class` / `interface` / `enum class` / 具名 `object` 进入类型命名空间；
  `val` / `var` / `const val`、参数、局部绑定、函数和 enum 变体进入值命名空间。
- 同一拼写可以在同一作用域的两个不同命名空间各出现一次；引用所处的语法上下文决定查询
  哪一个命名空间。`type_ref` 查询类型空间，普通名称表达式查询值空间。classifier 在调用
  或 `Type.member` 位置形成的构造器 / 关联命名空间候选由后续类型检查从已解析的 type
  symbol 建立，不把 classifier 复制成第二个用户 value 声明。具名 `object` 额外产生同名
  singleton value，但两个身份仍共同追溯到同一声明。
- 每个成功收集的声明、参数和局部绑定获得一个只在本次解析产物内有效的稳定 `SymbolId`；
  每个作用域获得 `ScopeId`。ID 按确定性的源码遍历顺序分配，不等同于名称哈希或跨编译
  持久标识，也不得暴露随机集合的迭代顺序。
- 同一值作用域中的多个函数声明形成一个源码有序的 overload set。函数与非函数值同名、
  两个非函数值同名、同一类型作用域两个类型同名，均使用 L0079。两个函数是否具有重复
  签名、哪个 overload 最终适用，等待 SPEC-0019/0020 获得类型后判断；SPEC-0018 不按参数
  数量或源码 TypeRef 文本自行淘汰候选。

Kotlin 风格的 `UpperCamelCase` / `lowerCamelCase` / 常量大写只是源码风格，不是名称身份
的一部分，也不产生 Phase 2 错误；语言仍按第 2 节的 ASCII、大小写敏感 Identifier 精确匹配。

### 21.2 作用域与可见时点

- 文件作用域先按源码顺序收集全部顶层声明，再解析任何签名或 body，因此顶层类型、函数、
  常量和变量允许同文件前向引用与函数递归。package/import 不是本地声明；未来
  SPEC-0025 根据 package ADR 构造外部环境后，仍使用本节相同查询规则。
- 每个 classifier 建立独立成员作用域。主构造器字段与 body 成员在成员 body 解析前完整
  收集，允许成员前向引用；函数可形成 overload set。companion 拥有单独的类型级值作用域，
  不把关联函数 / 常量注入实例成员作用域，实例成员也不反向注入 companion。
- 具名函数建立类型参数作用域和值参数作用域；参数在整个 body 中可见。函数自己的递归名称
  来自外层已收集的 overload set。类型参数在其后续上界、参数、返回类型和 body 内可见；
  同一参数列表或类型参数列表重名使用 L0079。
- lambda 参数在该 lambda body 内可见；未被 lambda 自己声明、但解析到外层值的引用记录为
  外层 symbol，捕获方式与 `move` 合法性仍由 Phase 3 判断。`for` binding 只在 loop body
  可见，不在 iterable/source 表达式中可见。
- block 按 element 源码顺序解析。局部 `val` / `var` 的 initializer 先在当前可见环境中
  解析，随后才把新名称加入当前 block；局部解构的所有 binding 同时在 initializer 完成后
  加入。名称不在自己的 initializer 或同 block 更早 element 中可见。
- 同一作用域重复声明使用 L0079；嵌套 block、lambda 或 loop 可以遮蔽外层同命名空间的
  symbol，查询总是选择最近的已可见声明，不产生隐式 warning。声明前若已有可见的外层同名
  symbol，较早引用继续解析到外层；只有没有任何可见声明、但当前顺序作用域稍后会声明同名
  local 时，才使用 L0081，而不是 L0080。

### 21.3 引用、外部环境与诊断

- SPEC-0018 的入口必须显式接收一个不可变 `NameEnvironment`。它提供预声明类型、值与函数
  overload，但不读取文件系统、不隐式加载标准库，也不按名称硬编码“可能来自 import”。
  后续 prelude / package 阶段可以构造环境；测试可构造最小环境。环境中不存在且词法作用域
  也找不到的名称使用 L0080。
- 非限定名称从当前作用域向父作用域查询对应命名空间，再查询外部环境。`type_ref` 的首个
  segment 必须解析为类型或外部类型；后续 segment 的嵌套类型 / package 解释由 SPEC-0020 /
  0025 继续处理。`.` / `?.` 后的 member、`super<Interface>.member`、constructor、companion
  与 enum variant 的最终选择依赖 receiver 类型，SPEC-0018 只解析 receiver 与显式 type
  segment，不对 member name 发未定义诊断。
- `public` / `internal` / `private` 在单文件内部均可见；跨文件与跨 package 可见性由
  SPEC-0025 执行。`this`、`super`、jump target、override、接口委托目标类型和 smart cast
  都有后续专用检查，不能伪装成普通未定义名称。
- L0079 `duplicate name in scope` 的 primary 覆盖后出现的声明名称，label 指向同命名空间
  的首个冲突声明；函数 overload 组不触发本码。L0080 `unresolved name` 的 primary 覆盖
  引用 Identifier。L0081 `name used before local declaration` 的 primary 覆盖较早引用，label
  指向同一顺序作用域稍后的 local 声明。诊断与 symbol/reference 表均按源码位置和既有
  `ordered_diagnostics` 规则确定性排序。

SPEC-0018 不推导表达式类型，不选择 overload / constructor / member，不检查泛型 arity、
visibility 跨文件规则、override、调用实参映射、捕获所有权或 package/import 冲突。它的
输出必须允许 SPEC-0019 在不重新遍历源码字符串的前提下读取 scope、symbol、overload set
与每个已解析名称引用。

---

## 22. 基础类型检查与局部推导（v0.22）

> v0.22 已于 2026-08-21 由用户明确启用并取代 v0.21；数值后缀的 Phase 1 交接由
> SPEC-0066 实施，基础类型检查由 SPEC-0019 实施。

本节把“基础类型检查”收敛为一个可独立验收的阶段：它解析内建标量与函数类型，给局部
绑定和已支持表达式建立确定类型，检查具名函数的返回契约，并把需要 nominal member、泛型
实例化或 overload 选择的节点显式标为 deferred。`deferred` 是编译器阶段状态，不是用户可见
类型，也不得被当作类型正确；后续 Spec 必须填充它，完整编译流水线不能携带 deferred 节点
进入所有权检查或 codegen。

### 22.1 显式语义环境与类型身份

- 类型检查入口接收 `SourceMap`、`ParsedFile`、SPEC-0018 的 `NameResolution` 和不可变
  `TypeEnvironment`。环境按 `ExternalSymbolId` 绑定内建类型或单态外部 value / function
  签名；不得按 `Int`、`error` 等字符串硬编码语义，也不得隐式加载 prelude。
- `TypeId` 只在一次 typed 产物内有效。类型表至少封闭表示 builtin、nullable、function、
  integer-literal constraint、`Error` 与 `Deferred`；相同结构必须确定性规范化，不能依赖
  随机 hash 迭代。`Error` 用于抑制已诊断级联，`Deferred` 精确保留后续阶段责任。
- SPEC-0019 识别 `Byte`、`Short`、`Int`、`Long`、`UByte`、`UShort`、`UInt`、`ULong`、
  `Float`、`Double`、`Boolean`、`Char`、`String`、`Unit`、`Nothing` 与 `Any` 的环境身份。
  裸 builtin 不接受类型实参；nullable 后缀形成 `T?`。`Any` 在本阶段只作为后续泛型检查的
  顶层约束，不建立需要运行时擦除的普通 value 表示；把它直接用于 local、参数或返回值时
  暂记 deferred，由 SPEC-0020 结合 nominal / 表示规则封闭。
- TypeRef 首段若已由名称阶段报 L0080，类型检查只产生 `Error`，不重复报错；限定名后续段、
  源码 nominal classifier、type parameter 和泛型实例统一 deferred 给 SPEC-0020。已解析的
  builtin 携带任何类型实参时使用 L0082。

### 22.2 相容、字面量与运算符最小闭包

- `Nothing` 是所有类型的 bottom type；`Nothing?` 只包含 `null`，可适配任意 nullable 类型。
  对任意非空 `T`，`T` 可适配 `T?`；除此之外 SPEC-0019 不引入隐式子类型或数值 widening。
  两个相同规范化类型相容，`Error` 与任何类型相容以抑制级联，`Deferred` 不参与成功判定。
- 无后缀十进制整数字面量先保留有符号数学整数约束；存在 `Byte` / `Short` / `Int` / `Long`
  expected type 时只要值落入范围即可适配。无 expected type 时先默认 `Int`，超出 `Int` 但
  落入 `Long` 时默认 `Long`，再超出使用 L0090。`L` 后缀固定为 `Long`。
- `u` / `U` 后缀建立无符号整数约束：存在 `UByte` / `UShort` / `UInt` / `ULong` expected
  type 时按范围适配；否则先默认 `UInt`，超出后默认 `ULong`，再超出使用 L0090。`uL` /
  `UL` 固定为 `ULong`。无后缀整数不得适配无符号 expected type，带 `u` 的整数也不得适配
  有符号 expected type；这只是字面量定型，不允许已定型变量发生隐式数值转换。
- 一元负号与紧随的无后缀或 `L` 整数字面量合并做范围判断，使各有符号类型的最小值可表达；
  对无符号字面量应用负号使用 L0085。
- 无后缀浮点字面量固定为 `Double`；`f` / `F` 后缀固定为 `Float`，包括 `1f`。不把
  `Double` 字面量按 expected type 静默缩窄。`true` / `false`、Char、String 分别固定为
  `Boolean`、`Char`、`String`；无 expected nullable type 的独立 `null` 无法推导。
- Byte / Short 没有专用后缀；`D` / `d`、`I` / `i`、小写 `l` 与 Rust 风格完整类型名后缀
  均未定义。类型检查器不得接受词法规范没有定义的拼写补齐这些能力。
- `!` 只接受 `Boolean`；一元 `+` / `-` 只接受数值 builtin。`* / % -` 与数值 `+` 要求两侧
  已定型为同一数值类型并返回该类型；`String + String` 返回 `String`，不提供隐式
  `String + Any`。`< > <= >=` 接受同型数值或同型 `Char`，返回 `Boolean`；`==` / `!=`
  接受相同类型、`T` 与 `T?` 或任一侧 `Nothing`，返回 `Boolean`；`&&` / `||` 只接受
  `Boolean`。不满足操作数契约使用 L0085，primary 为运算符，左右 operand 作为 label。
- Elvis `left ?: right` 要求左侧为 `T?` 或 `Nothing?`，并以 expected type 检查右侧可适配
  `T`；结果为 `T`。`!!` 要求 nullable operand 并返回非空 `T`。range、`to`、cast/type-test、
  assignment、member、index、call/callable reference 与 postfix `?` 需要 nominal、place、
  callable 或 `Result` 信息，SPEC-0019 只遍历 child 并将自身结果标为 deferred，不臆造类型。

### 22.3 expected type、local 与 lambda

- 类型检查采用单向 expected-type 传播，不做全局双向约束求解。显式 local 标注、函数返回
  标注、callable 参数位置和 control 分支的外层 expected type可以向 child 传播；不得从赋值
  之后的使用、overload 选择或另一文件反推声明类型。
- `val` / `var` 有显式 TypeRef 时先解析标注，再用它检查 initializer；不相容使用 L0084，
  primary 覆盖 initializer，label 指向 expected TypeRef。无标注时采用 initializer 已知类型；
  integer-literal constraint在此默认，`null`、带参数但无 expected function type 的 lambda，
  或真正缺少约束的表达式使用 L0083。若 initializer 自身因后续阶段能力而 deferred，则 local
  也 deferred，不提前发 L0083。
- lambda 是独立 callable。存在 expected function type 时，参数数量和 `move` / mode 结构先
  做精确匹配，不匹配使用 L0084，再以对应参数类型检查 body；参数类型不从 body 反推。无参数 lambda 可以从
  body 已知尾值推导 `() -> R`；带参数 lambda 没有 expected function type 时使用 L0083。
  普通 block 固定为 `Unit`，`LambdaBody` / `ControlBody` 才读取尾 expression；以声明或 jump
  结束、或空 body 的尾值为 `Unit`。
- 这不是 Kotlin 的完整局部双向约束求解：Kotlin 可把使用位置、overload candidate 和 lambda
  body 共同纳入局部约束；SPEC-0019 只允许已经确定的外层 expected type 单向流入 child。
  因此本阶段不会从后续使用反推 local，不会用 lambda body 选择 overload，也不会从带参
  lambda 的 body 猜参数类型；这些限制是确定性阶段边界，不是待实现的隐式行为。

### 22.4 具名函数、控制流与 `Nothing`

- 所有顶层和 member 函数先建立签名再检查任一 body，支持同文件递归。SPEC-0019 只提交
  全部 TypeRef 均为本节已知类型且不需要 overload/member 选择的单态签名；泛型、nominal 或
  deferred TypeRef 的函数签名保留给 SPEC-0020，不产生伪造的部分签名。
- `ImplicitUnitAbsent` 与 `ImplicitUnitBlock` 的返回类型精确为 `Unit`；显式 TypeRef 决定返回
  类型，绝不从 body 改写。表达式 body 以声明返回类型作为 expected type，不相容使用 L0084。
- `return`、`break`、`continue` 的表达式类型均为 `Nothing`。裸 `return` 只适配 `Unit`；带值
  `return e` 以最近 callable 的返回类型检查 `e`。lambda 建立独立 return 边界；文件 initializer
  等无 callable 上下文的 `return` 使用 L0086；裸 return 与非 `Unit` callable、带值 return 与
  `Unit` callable 的形态冲突使用 L0087，表达式值类型不匹配仍使用 L0084。break/continue
  target 留给后续 control-flow 语义检查，本 Spec 只赋 bottom type。
- 显式非 `Unit` block-body 函数若存在可到达的 body 末尾，使用 L0088，primary 为右花括号
  或 EOF 恢复点，label 指向返回 TypeRef。`if` 两分支或 control body 尾值在两侧已知时取最小
  join：同型保持原类型，任一侧 `Nothing` 取另一侧，`T` 与 `T?` 取 `T?`；否则 L0089。
  缺 `else` 的 statement-context `if` 固定 `Unit`。`when` 穷尽性和 smart cast 留给 SPEC-0021；
  在此之前 `when` 的整体类型为 deferred，但其 child 仍接受局部检查。

### 22.5 诊断与分阶段完成条件

| 错误码 | 含义 | primary / 关联位置 |
|---|---|---|
| L0082 | builtin type 不接受当前类型实参 | primary 为首个实参或参数列表；label 指向 builtin 名称 |
| L0083 | 无法从当前合法上下文推导类型 | primary 为 initializer、literal 或 lambda；不得用于后续阶段 deferred |
| L0084 | expression 或 lambda 结构与 expected type 不相容 | primary 为 expression / lambda header；label 指向产生 expected type 的标注或参数 |
| L0085 | 运算符的 operand 类型无效 | primary 为运算符；label 按左、右源码顺序列出已知 operand 类型 |
| L0086 | `return` 不在任何 callable 内 | primary 为 `return` keyword |
| L0087 | `return` 的有值 / 无值形态与 callable 返回类型冲突 | primary 为 `return` 或其值；label 返回标注 |
| L0088 | 显式非 `Unit` block body 可以到达末尾 | primary 为 body 结束位置；label 返回标注 |
| L0089 | control 分支没有本阶段可确定的公共类型 | primary 为 `else` / 第二分支；label 指向第一分支尾值 |
| L0090 | 数值字面量无法由后缀、expected type 或默认规则表示 | primary 为完整字面量；label 在存在 expected type 时指向该类型 |

所有 typed 表、deferred reason 与诊断顺序必须确定；同一根因产生 `Error` 后，下游不得再发
同范围类型级联。SPEC-0019 的完成不代表完整文件已无 deferred：它必须证明本节封闭子集全部
得到 known / error，且每一种 deferred reason 都精确对应 SPEC-0020、后续 callable 检查、
SPEC-0021 或 SPEC-0063 的既定责任，不能用单一 `Unsupported` 垃圾桶掩盖遗漏。

---

## 23. 名义类型、泛型与接口实现（v0.23）

> 本节已随 v0.23 于 2026-08-21 由用户明确启用并取代 v0.22。其目的是把
> §5、§7、§14–15、§21–22 与
> [04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md) §13 已有设计意图收敛成
> 可执行的单文件类型阶段；它不提前实现 member/call 选择、smart cast 或所有权。

### 23.1 名义身份与泛型实例

- 每个源码 `value class` / `class` / `interface` / `enum class` / 具名 `object` 声明产生一个
  稳定名义身份。身份来自声明 symbol，不由名称字符串或结构相同推导；两个字段完全相同的
  class 仍是不同类型。具名 object 的类型和值共享声明来源，但仍保留不同的 type/value
  symbol。
- `TypeId` 增加 `Nominal(nominal_id, arguments)` 与 `TypeParameter(symbol_id)`。相同名义身份
  和相同有序实参规范化为同一类型；参数顺序是身份的一部分。类型参数以声明 symbol 区分，
  不按拼写合并。普通 class 是固定大小 owner/reference handle，value/enum 是内联名义值；
  精确布局和 `Copyable` 仍分别交给 Phase 4 与 SPEC-0022。
- 声明处类型参数按源码顺序编号，使用点必须提供精确数量的实参；v1 没有 raw type、默认
  类型实参、星投影、型变、隐式缺参或多余实参。泛型是 invariant：`Box<A>` 与 `Box<B>`
  只有在 `A == B` 时相同，不从接口关系推导协变/逆变。
- 无显式 bound 的类型参数等价于顶层约束 `Any`，但参数本身在单态化后有具体表示，不等于把
  值擦除成运行时 `Any`。显式单一 bound 只允许：一个可带类型实参的 interface，或编译器
  预声明的 `Copyable` / `Transferable` 能力；显式 `Any` 与省略 bound 等价。普通
  class/value/enum/object、nullable、函数
  类型和另一个类型参数不能作为 v1 上界；这种精确限制避免引入类继承、交集类型和递归
  F-bound。
- SPEC-0020 检查 interface bound；`Copyable` 满足性由 SPEC-0022 检查，`Transferable`
  满足性由 Phase 3 检查。它们在 SPEC-0020 只保存为不同的能力谓词，不能伪装成普通
  interface，也不能因暂未求值而把整个名义类型降格为单一 deferred。
- `TypeEnvironment` 以外部 symbol identity 显式绑定 `Copyable` / `Transferable` 能力，方式
  与 builtin 类型绑定同样不依赖拼写；未绑定的同名外部 symbol 不是能力。源码中同名
  interface 仍只是普通 interface，不能冒充编译器能力，但可按普通 interface bound 使用。
- TypeRef 中的源码名义类型与类型参数在 SPEC-0020 后必须成为 known/error；同文件限定类型
  仍不开放 nested type。多 segment TypeRef 继续只可能由后续 package/import 解析，因此在
  SPEC-0025 前保持 `QualifiedType` deferred，不能把 `Outer.Inner` 猜成嵌套类型。

### 23.2 interface 位置与名义关系

- v1 没有 `dyn`，因此 interface 实例只允许出现在三类静态位置：类型参数 bound、class-family
  supertype list、接口委托的目标。不得把裸 interface 或 `Interface?` 用作 local、字段、
  普通值参数、返回类型、enum 关联数据、函数类型分量或容器元素；这些位置需要运行时值表示，
  必须使用满足该接口的具体名义类型或由该接口约束的类型参数。
- class/value/enum/object 的每个 supertype 必须是已实例化 interface；interface 的每个
  supertype 必须是父 interface。Koven 不接受普通 class 继承。同一直接列表重复 interface
  是错误；经不同路径传递得到的完全相同实例合并为一个 requirement 来源，而同一 interface
  声明以不同 invariant 实参到达则是冲突。interface 继承图必须无环。诊断按源码中首次闭合
  重复或环的 edge 定位，不能依赖图容器迭代顺序。
- 名义类型 `C<Args>` 静态满足 interface `I<Actuals>`，当且仅当按 `C` 的实际类型实参替换后，
  `I<Actuals>` 出现在其直接或传递 interface 集合中。关系按完整 invariant 实例判断；实现
  `I<Int>` 不满足 `I<Long>`，也不产生裸 `I` 值。
- `this` 在 class/value/enum/object 实例成员中是当前 `C<T...>`；在 interface 默认方法中是
  受当前 interface 约束但保持静态分发的 `Self`，不产生 interface runtime value。companion
  没有 `this`，其检查仍由 SPEC-0026 完成。

### 23.3 callable 签名、实现与 override

- 类型检查先为全部顶层函数和 classifier 收集 callable 签名，并为 classifier 额外收集类型
  参数、字段、enum 变体参数与 supertype，再检查图和 body。顶层/member 函数自己的类型
  参数使用独立作用域；签名中的 classifier 参数按其声明身份保存。替换是捕获规避、按 typed
  identity 进行的确定性结构替换，不能重解析源码文本。泛型函数 body 在其类型参数环境中
  检查；不因尚未实例化就把合法 `T` 当成 deferred，也不在本 Spec 推导调用点类型实参。
- 同一文件或成员值作用域的 **overload shape** 由名称、callable 类型参数数量和源码顺序
  参数类型组成；
  参数名、返回类型、类型参数 bound 和 `Value`/`Borrow`/`Inout` 模式都不参与 overload
  区分。无标记参数与显式 `borrow` 先规范化为同一个 Borrow contract；声明端 `own` 是 Value，
  但 Value 与 Borrow 的调用点都可写成普通 `f(x)`，按模式重载仍会产生重叠。shape 相同的
  后一个声明是重复签名，即使返回类型、bound 或规范化模式不同
  也不能形成 overload。泛型参数只改名但结构相同的 shape 按 alpha-equivalent 视为重复。
  完整 callable contract 仍保存参数模式和返回类型；interface 替换/override 必须在 shape
  匹配后再精确比较**规范化后的** contract，不能因为模式不参与 overload 就忽略模式不一致，
  也不能把无标记 Borrow 与显式 `borrow` 误判为不一致。
- 顶层 overload 同样使用该 shape 拒绝重复。interface 成员可以无体（abstract requirement）
  或有体（default）。子 interface 的本地
  同键声明替换继承 requirement/default；由于 Phase 1 不接受 interface `override` token，
  这里不要求该关键字。其模式、参数与返回类型必须和被替换签名精确一致。
- class/value/enum/object 的实例成员必须有 expression 或 block body。若其签名匹配任一直接
  或传递 interface member，必须显式写 `override`，并与该 member 的类型参数数量、模式、
  参数类型和返回类型精确一致；v1 不引入返回协变或参数逆变。没有任何匹配来源的
  `override` 同样是错误。实现 interface requirement 的 member 必须保持 `public`（省略
  visibility 即 public），不能用 `internal` / `private` 缩窄协议可见性。
- 具体类型必须覆盖全部 abstract requirement。单个继承 default 可直接使用；来自两个互不
  替换来源的同键 default，或 abstract/default 与不同委托来源产生的冲突，必须由具体类型
  提供显式 `override`。手写 override 胜过 default 和委托生成项。`super<I>.method()` 的
  receiver/interface 归属与调用检查留给后续 member/call Spec，但 SPEC-0020 必须保存冲突
  来源，不能提前任选一个实现。

### 23.4 窄化接口委托

- `Interface by field` 只在 ordinary `class` 生效。target 必须精确指向同一主构造器的
  `val` 字段；`var`、body local、companion value、任意表达式或其他同名 symbol 均无效。
- 字段静态类型必须是具体 class/value/enum/object 或受足够 interface bound 约束的类型参数，
  且在替换 class 实参后满足被委托的完整 interface 实例。裸 interface 字段仍因没有运行时
  表示而非法；委托不隐式创建 `dyn`、代理或共享 owner。
- 委托生成的实现精确复制 interface callable 的类型参数、参数模式、参数类型和返回类型；
  不改变 `Result`、不插入 `?`，也不参与用户可见 overload。手写同键 `override` 优先。
  两个 delegate、delegate 与继承 default、或 delegate 与另一个未消歧来源提供同键实现时，
  必须显式 override；诊断保存全部源码有序来源。
- SPEC-0020 只验证和记录静态转发计划；member call lowering、字段借用/移动与实际转发代码
  分别属于后续 call、Phase 3 和 Phase 4，不在此阶段生成隐藏 AST Item。

### 23.5 typed 产物、deferred 交接与诊断

- typed 产物增加源码有序 `NominalId`、classifier descriptor、类型参数/上界、已替换
  interface closure、字段/变体/member 签名与委托计划。公开查询只返回 typed identity，
  内部索引使用有序 Vec/BTreeMap；图遍历必须有确定的灰/黑状态并报告第一条源码有序闭环。
  `TypeEnvironment::bind_capability` 只接受 type symbol 和封闭的 `Copyable`/`Transferable`
  identity，重复或 kind 不匹配继续作为内部环境构造错误 fail loud。
- SPEC-0020 完成后，源码 nominal/type-parameter TypeRef、顶层/member 泛型函数签名和合法
  `this` 不再使用
  `NominalOrTypeParameter` / `FunctionContainsDeferred` / `ThisType`。member、constructor、
  overload/call argument mapping、callable reference、cast/type-test、when/smart cast、
  destructuring、capability 满足性和 package-qualified type 继续保留各自 reason；不得用
  `NominalOrTypeParameter` 作为遗留垃圾桶。
- 新诊断按既有全序聚合，产生 `Error` 后抑制同根级联：

| 错误码 | 稳定含义 | primary / label |
|---|---|---|
| L0091 | 名义类型实参数量不等于声明参数数量 | primary 为 use-site 类型实参表或类型名；label 指向声明参数表/名称 |
| L0092 | 类型参数 bound 不是 `Any`、interface 或预声明能力 | primary 为 bound TypeRef；label 指向类型参数名称 |
| L0093 | 类型实参不满足已可判定的 interface bound | primary 为该实参；label 指向声明 bound |
| L0094 | interface 被用于需要运行时值表示的位置 | primary 为完整 TypeRef；label 指向 interface 声明 |
| L0095 | class-family supertype 不是 interface，或同一 interface 声明重复实例化 | primary 为后出现的 supertype；label 指向实际声明或首次实例 |
| L0096 | interface 继承图形成环 | primary 为闭环 edge 的 TypeRef；label 按路径顺序指向先前 edge |
| L0097 | 同一成员作用域存在重复 callable 签名 | primary 为后出现的函数名；label 指向首个同键声明 |
| L0098 | concrete member 缺少必需的 body | primary 为 member 名称；label 指向 concrete owner |
| L0099 | 子 interface 的本地替换签名与继承 member 不一致 | primary 为本地 member 名称；label 指向被替换 member |
| L0100 | `override` 缺失、无目标、签名不一致或缩窄可见性 | primary 为 `override` token（缺失时为 member 名称）；label 指向相关 interface member |
| L0101 | 具体类型未实现 abstract interface member | primary 为 classifier 名称；label 指向未满足的 requirement |
| L0102 | 多个 interface default 存在未显式消歧的同键冲突 | primary 为 classifier 名称；label 按源码顺序指向冲突来源 |
| L0103 | 委托 target 不是同一主构造器的不可变 `val` 字段 | primary 为 target 名称；label 在存在同名字段时指向该字段 |
| L0104 | delegate 字段类型不满足目标 interface 实例 | primary 为 target 名称；label 指向目标 interface TypeRef |
| L0105 | 多个委托/default 为同键成员提供未显式消歧的实现 | primary 为后出现的 `by`；label 指向先前来源和 member requirement |

级联抑制是本表契约的一部分：L0100 已指出同 shape 的错误实现后，不再为同一 requirement
追加 L0101；L0095/L0096 使一条 hierarchy edge 失效后，不从该 edge 派生 requirement 或
default 冲突；L0103/L0104 已使委托失效后，不为原本期望由该委托满足的每个 member 逐条追加
L0101/L0105。独立的另一条合法 interface requirement 仍照常检查，不能用一次 Error 吞掉
无关诊断。

v0.23 本节只封闭 SPEC-0020。调用表达式/构造器/member access 与具名实参映射应在其后单独
物化 callable Spec；SPEC-0021 仍只负责 `when` 穷尽性与 smart cast，SPEC-0022 负责
`Copyable`/解构与内联递归，SPEC-0026 负责 companion/const。这样 nominal graph 是后续阶段
共享的稳定输入，而不是把整个 Phase 2 塞进一个不可独立验收的提交。

---

## 24. `when` 穷尽性与 smart cast（v0.24）

> v0.24 已于 2026-08-21 由用户明确启用并取代 v0.23；本节是 SPEC-0021 的现行契约，
> L0106–L0114 已随该 Spec 完成实施与验收。

### 24.1 enum case 的双重身份

- `enum class E { C(...), D }` 中每个 case 同时声明同拼写的值构造器和嵌套 case type；二者
  共享一个 `EnumCaseId`，分别进入 `E` 的值/类型命名空间。case type 不是可独立实现
  interface 的普通 classifier，也不能出现在 supertype、泛型实参或公开签名中；只允许作为
  `is` / `!is` 的目标和 smart-cast 后的内部流类型。其 runtime 公共类型始终是 `E<...>`；
  其他显式 TypeRef 位置使用 L0114，而不是把 case type 当作 root enum 的别名。
- enum 本体作用域内可写短名 `C`；外部源码必须写限定名 `E.C`。同一限定拼写在值位置表示
  case value/constructor，在 `is` / `!is` 的目标位置表示 case type。名称阶段解析完整限定链，
  不允许把“首段已解析、尾段 deferred”伪装为成功；跨 package 的前缀展开仍后置 SPEC-0025。
- case payload 字段属于对应 case type。enum 自身方法内的裸 `radius` 是“隐式 `this` 的
  case payload 候选”，名称阶段保留候选而不提前报 unresolved；只有当前流事实唯一证明
  `this` 为声明该字段的 case 时才能取其类型。`this.radius` 遵循相同规则。没有该事实、多个
  case 同名字段无法唯一选择或在 enum 外裸用时产生 L0113。
- 普通 class/value class/object/interface 不因此获得继承或 runtime tag；v1 的 `is` 不提供
  任意 RTTI，也不能用 interface、类型参数或 `Any` 对未知具体类型作动态探测。

### 24.2 类型测试与流事实

- 合法 `e is T` / `e !is T` 的结果固定为 `Boolean`。v1 的有效测试关系仅包括：同一 enum
  root 与其 case、同一已知 nominal 的 nullable/non-null 分离，以及已经静态相同的具体类型；
  interface、类型参数、无关 nominal 和需要运行时泛型反射的测试使用 L0106。`as` / `as?`
  仍不属于 SPEC-0021。
- smart-cast key 只表示一次求值的稳定 place：`this`、value parameter、local `val`，以及未被
  捕获且从事实建立点到使用点没有赋值的 local `var`。对 `var` 的任意赋值先检查右值，再清除
  该 symbol 的全部事实；捕获进 lambda 的 `var` 不跨 lambda 或调用边界保留事实。普通字段、
  index、call、任意 member chain 和有副作用表达式不作为稳定 key。
- `if` 的 then/else 分别接收条件为真/假时的事实；`!` 交换两侧事实，`&&` 的右操作数接收
  左侧为真的事实，`||` 的右操作数接收左侧为假的事实。分支结束后的事实取所有可 fall-through
  出口的交集；`Nothing` 出口不参与交集。无法表示的析取事实保守丢弃，不猜测第三套类型。
- subjectful `when` 的 subject 只求值一次并获得临时 key；若源码 subject 本身是稳定 place，
  case 事实同时绑定到该 place。一个 entry 用逗号列出多个条件时，body 只获得所有可进入
  alternative 事实的交集，不能把仅由其中一个条件证明的 payload 字段暴露给整个 body。
- `when (shape) { is Shape.Circle -> ... }` 是外部作用域的规范写法；enum 自身方法内允许
  `when (this) { is Circle -> ... }`。无 payload case 也可在普通条件写 `Shape.Point` / `Point`，
  按 case value 与 subject 做等值比较；有 payload case 的构造器名称本身不是一个 case value。
- `x != null` / `x == null` 为稳定 nullable key 建立非空/为空事实。事实只能收窄，不能改变
  声明类型或写回类型；赋值仍按声明类型检查。循环回边、未知 call 的副作用和 lambda 捕获
  使用保守 kill，不实现完整 SSA 数据流或 NLL。

### 24.3 `when` 条件、覆盖域与重复

- subjectless `when` 的普通条件必须是 `Boolean`；类型/包含条件仍为语法错误恢复产物，类型
  阶段使用 L0107。subjectful 普通条件按 `subject == condition` 检查可比较性；`in` / `!in`
  的协议选择后置到 member/call Spec，在此保持专用 deferred，不据此证明穷尽。
- 编译器只对有限且封闭的域证明无 `else` 穷尽：`Boolean` 的 `{true,false}`、enum root 的全部
  case，以及它们的 nullable 形式（额外包含 `null`）。泛型类型参数、普通 class、整数、
  String、interface、`Any` 和 subjectless predicate 集合都不是封闭域。
- enum case 的正 `is` 覆盖该 case，`!is` 覆盖当前有限域的补集；`null`、Boolean literal 和
  enum 无 payload case 的等值条件可贡献单点覆盖。一个条件对当前剩余域不增加覆盖时产生
  L0110；poisoned/未知条件不参与覆盖，也不制造后续重复诊断。
- `else` 最多一次且必须是最后一个 entry；重复使用 L0108，非末尾使用 L0109。即使前面已
  穷尽，显式末尾 `else` 仍允许，作为未来兼容兜底，不报冗余。

### 24.4 value/statement context 与分支类型

- initializer、assignment RHS、return value、call argument、表达式体以及另一个 value
  expression 的嵌套位置都是 value context。value-context `when` 必须有 `else` 或被有限域
  证明穷尽，否则 L0111；statement element 位置允许非穷尽，结果固定为 `Unit`。
- checker 必须从 AST owner 显式传递 `ExpressionUse::{Value,Statement}`（或等价封闭状态）：
  普通 block 中非尾 expression element 是 statement use；control/lambda body 的尾 expression、
  initializer 与表达式 body 是 value use。`expected == None` 同时可能表示推导和值被丢弃，
  禁止用它推断上下文。
- value-context 分支先接受外部 expected type。无 expected type 时按源码顺序求最小公共类型：
  忽略 `Nothing`；完全相同类型保持不变；`T` 与 `T?` 合并为 `T?`；同一 enum 的 case 流类型
  合并为 enum root；其他已知类型合并为 `Any`。Error 抑制同根级联，Deferred 只保留其专用
  reason。分支不满足显式 expected type 时沿用 L0086；无法形成上述 join 时使用 L0112。
- 穷尽 `when` 只有全部可到达 entry 都不 fall through 时才是 `Nothing`。诊断、typed 结果和
  coverage 顺序必须只依赖源码顺序；不得用随机 hash 迭代决定遗漏 case 的顺序。

### 24.5 诊断预留

| 错误码 | 含义 | 主范围与关联信息 |
|---|---|---|
| L0106 | `is` / `!is` 目标不可运行时判定或与被测类型无合法关系 | primary 为运算符；label 指向目标 TypeRef |
| L0107 | `when` 条件形态或类型与有/无 subject 规则不匹配 | primary 为条件；label 可指向 subject |
| L0108 | 同一 `when` 出现多个 `else` | primary 为后出现的 `else`；label 指向第一个 |
| L0109 | `else` 不是最后一个 entry | primary 为 `else`；label 指向后续首 entry |
| L0110 | 有限域中的条件不增加任何新覆盖 | primary 为该条件；label 指向首次覆盖来源 |
| L0111 | value-context `when` 未覆盖封闭域或无法证明穷尽 | primary 为 `when`；labels 按声明顺序列出遗漏 case，非封闭域建议添加 `else` |
| L0112 | 无 expected type 的可达分支无法形成合法公共类型 | primary 为后出现分支尾值；label 指向首个冲突分支 |
| L0113 | enum case payload 在当前流事实下不可唯一访问 | primary 为字段名称；labels 指向候选 case 声明 |
| L0114 | enum case type 出现在 `is` / `!is` 目标之外的显式 TypeRef 位置 | primary 为 case TypeRef；label 指向 root enum 声明 |

L0106/L0107 已使条件 poisoned 后，不追加同条件的 L0110；L0108/L0109 不阻止仍可确定的
entry body 类型检查；L0111 只产生一条并聚合遗漏项。SPEC-0021 不顺带实现一般 member/call
选择、`as`、包含协议、所有权或跨文件 sealed hierarchy。

---

## 25. 条件 `Copyable`、内联递归与结构化解构（v0.25）

> **现行状态**：v0.25 已于 2026-08-21 由用户明确启用并取代 v0.24；本节现为 SPEC-0022
> 的实施契约。

### 25.1 封闭的 `Copyable` 判定

`Copyable` 是由编译器绑定的内建能力身份，不按源码拼写识别。判定对已替换的实际类型递归
进行，结果精确区分 `Copyable`、`MoveOnly`、`Unknown` 与 `Error`：`MoveOnly` 表示在当前
静态类型下没有复制证明、按值使用必须按移动处理；`Unknown` 只保留给尚待后续选择的 deferred
类型；`Error` 只抑制同根级联。v1 使用以下封闭规则：

- 数值类型、`Boolean`、`Char`、`Unit` 和 bottom type `Nothing` 满足 `Copyable`。`Nothing`
  没有可构造的有效值，因此是平凡满足；这也使只有 `null` 值域的 `Nothing?` 满足规则。
- `T?` 当且仅当 `T` 满足 `Copyable`；nullable wrapper 不引入 retain、clone 或唯一析构。
- `value class C<A...>` 当且仅当按实际类型实参替换后，每个主构造器字段类型都满足
  `Copyable`。字段的 `val` / `var` 不参与判定。
- 有限 `enum class E<A...>` 当且仅当按实际类型实参替换后，每个 case 的每个 payload 类型
  都满足 `Copyable`；无 payload 的 enum 因而满足。case type 沿用其 root enum 实例的能力，
  不形成可独立声明或实现的 marker。
- 类型参数只在其声明具有编译器绑定的 `Copyable` 上界时满足；`T : SomeInterface`、`T : Any`、
  `T : Transferable` 或无上界在当前泛型体内均为 `MoveOnly`。对类型实参检查 `Copyable`
  上界时使用本节同一判定。
- `String`、普通 `class`、具名 `object`、函数 / lambda 类型和内建 `Box<T>` 均为
  `MoveOnly`，不得因字段或类型实参可复制而提升。`Any` 也为 `MoveOnly`：其静态类型不能
  证明动态 payload 可复制。裸 interface / capability 不是合法 runtime value type；若因恢复
  到达能力查询则为 `Error`。deferred type 为 `Unknown`，error type 为 `Error`。
- 用户声明的同名 `Copyable` 或 `Box` 不获得内建身份，也不能冒充能力或 intrinsic 类型。

该规则只产生类型能力事实。赋值、传参、返回或解构后是否构成 move-after-use 仍由 Phase 3
检查；SPEC-0022 不建立所有权状态机。

### 25.2 有限内联布局图

类型声明的内联边由 `value class` 主构造器字段与 `enum class` case payload 产生；nullable
包装对布局递归是透明边。普通 `class`、具名 `object`、函数 / lambda、内建 `Box` 与动态
顺序容器都是固定大小的间接 handle，打断内联环。类型参数本身不构成声明递归边；沿未被
handle 打断的路径再次遇到同一 value/enum 名义声明时，无论类型实参是否变化都构成无限
内联递归（例如 `A<T>` 包含 `A<List<T>>` 仍非法），避免为变化中的实例无限展开。实际类型
实参替换仍用于判断边上的 nullable / handle 类别与 `Copyable` 条件。

编译器必须按声明顺序建立确定性图。每个循环强连通分量只报告一条 L0116；多个循环分量按
最早声明顺序各报告一条。分量内以源码顺序 DFS 得到首条代表环，primary 为闭环边，labels
按环上其余边的声明顺序排列。被判定为无限内联布局的名义类型保留其 `TypeId` 供错误恢复，
但布局状态与 `Copyability` 均为 `Error`，不得流入后续 DataLayout / LLVM 类型构造。目标相关
的实际大小、对齐和对象大小上限仍属于 Phase 4。

### 25.3 内建 `Box` 身份与实参边界

v1 的 `Box` 是由 `TypeEnvironment` 显式绑定的 intrinsic type constructor，不按名称字符串
特判。它精确接受一个类型实参，且该实参必须能静态证明为一个具体 `value class` 名义实例；
普通 `class`、enum、interface、object、基础类型、函数类型与类型参数均不合法。当前上界
语言没有“是 value class”这种 kind bound，因此即使 `T : Copyable` 也不足以让 `Box<T>`
合法；泛型代码必须在具体 value-class 实例已知的位置使用 `Box`。

`Box<T>` 自身始终 `MoveOnly`，并打断内联递归。源码中声明 `class Box<T>` 只产生普通名义
class，不取得 intrinsic 语义；外部环境没有绑定 intrinsic `Box` 时，编译器不得按拼写猜测。
intrinsic `Box` 的实参数量不是 type-kind 约束：零个或多于一个类型实参沿用 L0091
`type argument arity`，只有数量为一但 type kind 不合法时才使用 L0117。

### 25.4 局部结构化解构

SPEC-0022 只检查 SPEC-0013 已接纳的局部 `val (a, b) = expression`：initializer 只类型
检查和求值一次。源类型为 `value class` 时，分量精确对应主构造器字段的声明顺序，绑定数
必须与字段数相等；不能缺少或多出分量。typed 结果记录稳定的 statement identity、源类型、
按序的绑定 symbol / 分量类型，以及整次操作是 `Copy` 还是 `Consume`：源类型满足
`Copyable` 时为 `Copy`，否则为 `Consume`。

这里的 `Consume` 只是交给 Phase 3 的原子所有权动作描述；SPEC-0022 不判定解构后再次使用、
字段析构或部分移动。非 `value class` 的 `componentN()` 选择依赖尚未实施的一般 member/call
选择，继续保留为专用 deferred reason，不猜测结构分量。

第 11 节“占位、跳过或丢弃尚未定义”精确约束局部 `val` 解构：`val (_, x) = pair` 继续由
既有 Parser 以 L0042 `unsupported destructuring form` 拒绝，SPEC-0022 不改变这一语法，
也不会为 `_` 创建普通绑定或 typed 分量。
[`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md) §12 中 `for` binding
已经解析的 `_` 是仅属于 `for` 的专用 discard 形态；其迭代类型与所有权语义由后续 iterable
Spec 定义，不得反向扩展成通用解构占位符。

### 25.5 诊断与 Phase 边界

| 错误码 | 含义 | 主范围与关联信息 |
|---|---|---|
| L0115 | 类型实参不满足内建 `Copyable` 上界 | primary 为实参 TypeRef；label 指向上界声明 |
| L0116 | 直接或间接形成无限内联布局环 | primary 为闭环字段 / payload TypeRef；labels 按环中声明顺序列出其余边 |
| L0117 | intrinsic `Box` 的实参不是可证明的具体 `value class` 实例 | primary 为实参 TypeRef；无 intrinsic 绑定时不使用此诊断 |
| L0118 | `value class` 结构化解构绑定数与字段数不一致 | primary 为解构 pattern；label 指向类型声明并给出期望数量 |

SPEC-0022 不实现一般 callable/member/constructor 选择、独立 `componentN()` 调用、字段投影
所有权、move-after-use、drop、容器类型、`Transferable`、companion 或 codegen。L0115–L0118
自 v0.25 起具有稳定含义。

---

## 26. 调用期借用与 ASAP 析构点（v0.26）

> **现行状态**：v0.26 已于 2026-08-23 由用户明确启用并取代 v0.25。本节及 L0133–L0135
> 已成为现行语义。SPEC-0176 已完成声明语法、参数默认 mode、函数类型、lambda expected mode
> 与预声明 callable contract 迁移；SPEC-0029 已实现名称/字段 place、调用期 loan、冲突诊断和
> owned-value ASAP drop-point。receiver、index element place 与 closure capture 仍按本节边界
> 保持 deferred。

### 26.1 所有者、参数绑定与调用期 loan

v1 只有 owned value 和调用期 loan，不引入引用类型、生命周期参数或可存储的 borrow value。
局部变量、临时值和声明端 `own` 所映射的 `Value` 参数在其值为 `MoveOnly` 时拥有唯一析构
义务；无标记或显式 `borrow` 的 `Borrow` 参数以及 `Inout`
参数只是调用者 place 的非 owning 绑定，由调用者保持 owner，callee 退出时不得析构它们。
`Copyable` 值不产生唯一析构义务，也不因借用而生成 copy / retain / clone glue。

一次同步调用按以下固定顺序处理：

1. 先求值 callee，再按**源码顺序**各求值一次 argument operand；命名实参映射不改变顺序。
2. 每个 operand 求值完成后立即应用已由 Phase 2 选定的参数契约，再继续求值下一个 operand：
   声明端 `own` 的 `Value` 对 `Copyable` 产生 owned copy、对 `MoveOnly` 转移 owner；
   `Borrow` 建立 shared loan；
   `Inout` 建立 exclusive loan。因而较早实参的 loan 在较晚实参及其嵌套调用求值期间已经有效。
3. 所有成功建立的 loan 持续到同步 callee 返回；返回后同时结束。`Borrow temporary` 合法，
   temporary owner 延长到调用返回后再按本节析构。`Inout temporary` 继续非法。
4. operand 或 callee 产生 `Nothing` 时，不求值其后的 argument，也不为未求值 argument 建立
   loan。`error()` 是 abort，不做异常展开或沿栈析构。

这不是 NLL：loan 不因 callee 内或调用者表达式中的“最后一次实际访问”提前结束，也不跨越
本次同步调用存储、返回或挂起。调用期 loan 是所有权检查产物，不成为源码可命名的值。

### 26.2 `Value` / `Borrow` / `Inout` 参数体内能力

- 声明为 `own value: T` 的 `Value T` 参数是普通 owned local：满足 `Copyable` 时可复制，
  否则按普通移动规则使用；
  未移动的 `MoveOnly` 参数由 callee 在本节确定的析构点负责析构。
- 无标记 `value: T` 或显式 `borrow value: T` 都是同一个 `Borrow T` 参数，允许读取、建立
  嵌套 shared reborrow，以及在 `T : Copyable` 时产生 owned
  copy。它不允许赋值、建立 `Inout` reborrow、析构或从中移动 `MoveOnly` 值。
- `Inout T` 参数是已初始化 place 的 exclusive 非 owning 绑定。它允许读取、shared/exclusive
  reborrow 和以完整新 `T` 替换原值；替换必须先求值 RHS，再析构旧值并提交新值。它不允许
  把 `MoveOnly` 值移出后留下未初始化的调用者 place。
- 从 `Borrow` / `Inout` 参数返回或赋给 owned 目标时，只在 `T : Copyable` 时产生 owned copy；
  `MoveOnly` 情况属于非法移出，而不是借用逃逸。v1 没有 borrow-return 类型。
- nested reborrow 不得超过 nested call；callee 返回时原 `Inout` place 必须仍为一个完整、
  已初始化且由调用者拥有的 `T`。

闭包捕获会产生超出单次普通调用的环境 owner，仍由 SPEC-0032 封闭；本节不借“调用期 loan”
提前接受或拒绝捕获。instance member 的隐式 receiver mode 也尚无源码契约，本节只检查
`CallArgument` 的显式参数绑定，不从方法名、函数体或字段可变性猜测 receiver 是 `Borrow`
还是 `Inout`。

### 26.3 place 重叠与冲突矩阵

SPEC-0029 的 place identity 由稳定 root `SymbolId` 与零个或多个已解析 field `SymbolId`
组成。两个 place 在 root 不同时不重叠；路径完全相同或一方是另一方前缀时重叠；同一 root
下首个不同字段代表可证明不重叠的存储。普通字段仍不得部分移动，但不同字段可以同时建立
不冲突的 loan。无法形成稳定 root/path 的表达式不是可借用 place。

index place 的逻辑索引证明、容器重分配冲突与 element replacement 由 SPEC-0030 封闭；
在该 Spec 完成前，SPEC-0029 必须把 index loan 保留为明确 deferred，不能当成已证明不重叠，
也不能按内存地址或常量折叠自行接受。

本节的 mutable place 也使用封闭规则，不等同于“任何 place”：完整 root 只有源码 `var`
绑定或 `Inout` 参数可变；`val`、`Value` / `Borrow` 参数、解构绑定和 `for` binding 的完整值
不可被 `&` 替换。字段必须声明为 `var`，并且普通 `class` 字段的 receiver owner 当前可独占，
或内联 value/enum receiver path 自身递归满足 mutable place，才是 mutable field place。因此
`val node: Node` 不允许 `&node` 替换 handle，但在 `Node` 是普通 class 且 `next` 为 `var` 时允许
`&node.next`；`val point: Point` 的内联 `var` 字段仍不可变，必须由 `var point` 或 `Inout Point`
投影。group 透明继承内部类别；temporary、`this` 与尚未封闭的 receiver/index 不由本规则
猜测为 mutable。

对同一或重叠 place，调用者侧的冲突规则只有以下一套：

| 已有效状态 | 新 shared loan / read / `Copyable` copy | 新 exclusive loan / mutation | move / drop |
|---|---|---|---|
| 无 loan | 合法 | 仅 mutable place 合法 | 合法 |
| 一个或多个 shared loan | 合法 | 冲突 | 冲突 |
| exclusive loan | 冲突 | 冲突 | 冲突 |

`Inout` holder 在 callee 内通过该绑定进行的读取、替换和 reborrow 是 exclusive loan 授予的
能力，不按“调用者再次访问”处理；对同一 place 的另一参数绑定仍应用上表。新 loan 的 primary
指向产生冲突的 argument operand 或 `&`，label 指向最早仍有效的冲突 loan；move、赋值或
drop 与 loan 冲突时 primary 指向该访问，label 同样指向 loan 来源。诊断和 label 顺序只按
源码顺序，不依赖 hash 迭代。

### 26.4 ASAP 析构的可执行定义

“ASAP”精确定义为：对每条正常控制流路径，在保持所有未来合法读取、借用、移动和赋值 RHS
求值不变的前提下，于 owner 不再 live 的最早边界析构仍 `Available` 的 `MoveOnly` 值。它是
owned-value liveness，不把调用期 loan 缩短为完整 NLL。所有权检查输出显式、源码有序的
drop facts；Phase 4 消费这些事实生成 drop/free，不得重新猜测生命周期。

- `MoveOnly` temporary 在所属完整表达式结束时析构；若作为 `Borrow` 实参，则延长到该调用
  返回后；若被声明端 `own` 的 `Value` 参数移走，则源 temporary 不再析构。
- named owner 在路径上的最后一次合法使用后析构。若 owner 从未使用，则在 initializer 完成
  且绑定建立后立即析构；initializer 自身仍只求值一次。
- 普通 `var` 替换先完整求值 RHS；若 RHS 正常返回，再析构旧值并写入新值。RHS 可读取旧值，
  但若已把旧值移动走，则本次赋值不再为旧值生成 drop。
- `return value` 先求值并交付返回值，再按内层到外层、同层声明逆序析构仍可用的 owner，
  最后转移控制；postfix `?` 的 `Err` 路径使用同一 return cleanup。`break` / `continue` 只析构
  被跳出词法 scope 中的 owner，不能析构目标 loop 下一次迭代仍需要的外层 owner。
- 正常 scope 结束时，仍 live 的 owner 按声明逆序析构。多个 temporary 在同一边界析构时按
  完成求值的逆序处理；`Copyable` 值不进入该顺序。
- 分支分别计算 liveness。若合流后没有未来使用，各条 incoming path 在最早安全边界析构仍
  可用的 owner；某条路径已移动时该路径不析构。若无法证明 branch-local 最后使用，则保守
  延迟到最近共同安全边界，不能提前析构。合流后的未来使用若可从已移动路径到达，仍产生
  L0131，而不是通过在其他路径插入 copy 修复。
- loop backedge 上仍可能在后续迭代使用的 owner 保持 live；只有离开 loop 的边或可证明不再
  回到使用点的路径可以析构。v1 不做跨调用、跨闭包或依赖运行时索引的 NLL 证明。

任何有效 loan 都把对应 owner 视为 live；drop 与 loan 冲突必须先报告借用错误，不能通过
提前结束 loan 或静默延后到不可复核的位置“修复”源码。程序已有所有权错误时可以保留用于
抑制级联的恢复状态，但不得据此生成可执行 drop 计划。

### 26.5 诊断、产物与实施边界

| 错误码 | 稳定含义 | 主范围与关联信息 |
|---|---|---|
| L0133 | 从 `Borrow` / `Inout` 绑定移出 `MoveOnly` 值 | primary 为消费位置；label 指向参数声明或 loan 来源 |
| L0134 | `Inout` operand 已是 place，但不是可独占的 mutable place | primary 为 `&`；label 可指向不可变声明 |
| L0135 | read / borrow / mutation / move / drop 与仍有效 loan 冲突 | primary 为后发生的冲突访问；label 指向最早冲突 loan |

L0131 use-after-move 与 L0132 partial-move 的含义不变；同一根因先产生 L0133–L0135 后，不再
追加 L0131/L0132 级联。非 place 或 temporary 的 `&operand` 继续由既有 L0122 参数模式不匹配
拒绝，不迁移到 L0134。有效所有权产物至少能按 expression/control-flow edge 查询 loan begin、
loan end 与 drop facts，并保留 owner/place identity、loan kind 和来源 `Span`。

本节只解除 SPEC-0029 的 call argument loan、参数体内 reborrow 与 owned-value drop-point
语义门禁。v0.26 的声明端 `own`、默认 Borrow 与 typed contract 已由 SPEC-0176 实现；
SPEC-0029 本身不再重复修改语法。index element place 的核心读取、loan、replacement 与 drop
facts 已由 SPEC-0030 实现；member/委托 receiver 及 Phase 5 尚未定义 API 的容器 relocation
属于后续独立 Goal。closure capture / `Transferable` 由后续 v0.27 §27 与 SPEC-0032 接续；
借用返回、用户生命周期语法、跨调用 loan、完整 NLL 和部分移动不进入 v1。在 SPEC-0032
实施前，被 lambda 引用的外层 owner 仍须保留明确 deferred 且不得生成提前 drop fact。
SPEC-0029 不新增语法、依赖或 LLVM 类型。

---

## 27. 简化 closure capture 与跨线程转移（v0.27）

> **现行状态**：v0.27 已于 2026-08-24 由用户明确启用并取代 v0.26；本节是 SPEC-0032
> 的实施契约，不引入引用类型、用户生命周期或完整 NLL。

### 27.1 capture 身份与普通闭包

lambda 的 capture 集由名称解析后的自由值引用决定，不按标识符文本猜测。local、参数、for /
解构 binding 属于 capture 候选；顶层函数/常量和具名 object 是全局身份，不进入环境。重复引用
只形成一个 capture，shadowing 按 `SymbolId` 区分；nested lambda 从紧邻 enclosing callable
环境捕获，不能绕过中间环境直接取得更外层 owner。

无 `move` 前缀且实际有 capture 的 lambda 是 **borrowed closure**：对每个候选只建立 shared
capture；owned、Borrow 和 Inout binding 均只提供 shared read/reborrow，捕获名称在 lambda 内
不能赋值或按值移出。loan 从 lambda 求值完成后持续到该 closure 值的 ASAP drop point。它可在
当前具名函数/lambda 内绑定、移动和作为 Borrow 实参同步调用，但不得 return、写入字段、交给
Value 参数或被 escaping move closure 捕获；这些逃逸产生 L0137。无 capture 的普通 lambda
没有 loan，可按普通函数值使用。

普通 closure 可 shared capture `this`；无前缀字段引用规范化为同一个 `this` capture。该规则
只授予只读能力，不提前决定 instance method receiver 的其他契约。

### 27.2 move closure

`move { ... }` 对每个自由 binding 建立 owned capture：静态满足 `Copyable` 时复制 snapshot，
否则必须从当前 owned、available binding 移动，lambda 形成后源 binding 不可再使用。Borrow /
Inout binding 的 MoveOnly 值不能形成 owned capture，产生 L0138；Copyable 值可从其 shared read
复制为 owned snapshot。local `var` 捕获的是形成时 snapshot，捕获名称在 v1 closure 内仍不可
赋值，避免引入独立的 mutable closure receiver 模型。

move closure 可以绑定、return、写入字段或交给 Value 参数。v1 的 move closure 不直接捕获
显式/隐式 `this` 或字段；需要先把所需字段复制/移动到 local，再捕获该 local，否则产生 L0138。
这条限制等待 instance receiver 所有权契约完成后再评估，不影响普通 shared `this` capture。

### 27.3 `Transferable` 完整域与跨线程 effect

`Transferable` 使用与 `Copyable` 相同的四态查询（满足、不满足、Unknown、Error），但两种
能力互不蕴含。数值、`Boolean`、`Char`、`Unit`、`String`、`Nothing` 满足；nullable、value
class、有限 enum、普通 class、`Box<T>` 与三种顺序容器按实际字段/payload/元素递归满足。
`Rc<T>`、具名 object、`Any` 和带 capture 的 borrowed closure 不满足；裸 interface/capability
与 Error 类型为 Error，deferred 为 Unknown。类型参数只在具有编译器绑定的
`Transferable` 上界时满足；`Copyable` 上界不自动构成证明。

无捕获函数引用/闭包满足 `Transferable`。move closure 当且仅当每个 owned capture 都满足。
仅有 `move (...) -> T` 静态函数类型但 provenance 不可查询的普通函数值不能证明其环境满足；
v1 跨线程入口只接受编译器可查询 capture facts 的 move lambda 或已知无捕获函数值。

跨线程转移不是从函数名或 `move` 函数类型推测的效果。typed callable target 必须携带由
`TypeEnvironment` 绑定的封闭 cross-thread effect；预声明 `thread(task)` 的 task 位置和
`Sender.send(value)` 的 value 位置具有该 effect，源码同名函数不获得。对应 operand 或 move
closure capture 不满足 `Transferable` 时产生 L0139。`Shareable`、跨线程借用和完整多线程
数据流继续属于 v2。

### 27.4 产物、析构与诊断

Phase 3 产物必须按 lambda 查询 capture symbol、类型、Shared/Owned 模式、来源 Span 和 owned
closure 的 `Transferable` 结果。borrowed closure loan 参与既有 move/mutation/drop 冲突；move
capture 参与 Available/Moved 与 ASAP drop，closure drop 时按 capture 逆序析构 owned
MoveOnly 字段。存在 capture/transfer 诊断时不发布可执行 capture/drop plan。

| 错误码 | 稳定含义 | primary / 关联位置 |
|---|---|---|
| L0137 | borrowed closure 逃出其 defining callable | primary 为 return/Value 交付/字段存储位置；label 指向 lambda |
| L0138 | move closure 不能取得当前 capture 的所有权 | primary 为 capture 引用；label 指向非 owning binding 或 `this` |
| L0139 | 跨线程 effect 收到不能证明 `Transferable` 的值/capture | primary 为实参或 capture 引用；label 指向类型/字段/capture 来源 |

---

## 28. 泛型 callable 实例化与 overload-lambda 隔离（v0.28）

> **现行状态**：v0.28 已于 2026-08-24 由用户明确启用并取代 v0.27；本节、L0140–L0141
> 已获得规范效力，frontend 已完成 SPEC-0177 / SPEC-0174。目的不是引入
> Kotlin 的完整局部约束求解，而是封闭进入 typed SSA 前仍缺失的最小 callable 实例 identity。

### 28.1 范围与简化边界

v1 只为源码具名顶层函数和实例 member 函数实例化 callable 类型参数。调用可以写完整显式
类型实参 `f<A, B>(...)`，也可以完全省略并从实参推导；不接受部分类型实参、`_` 占位、默认
类型实参、`where` 约束或把未填项留给返回上下文。普通函数值没有 callable 泛型参数，class
constructor、callable reference、safe call、跨文件 overload 与 intrinsic 泛型核心构造继续
遵守各自既有 Spec 边界。

泛型推导保持单向和局部：只读取本次调用中已经具有确定类型的非 lambda 实参，包括具名
函数值；不从调用结果的 expected type、赋值后的使用、lambda body、未定型 lambda 参数或
另一个文件反推类型实参。没有 expected type 的数字字面量先按 §22 的既有默认规则定型，再
参与推导。因此 `identity(1)` 推导 `T = Int`，而仅在 `factory<T>(): T` 返回位置出现的 `T`
不能从 `val value: String = factory()` 推导。

### 28.2 候选实例化与 bound

显式类型实参数量必须精确等于 callable 自身的类型参数数量；不匹配复用 L0091。省略类型
实参时，对候选参数类型与已定型实参类型做结构匹配：同一类型参数的每次出现必须得到完全
相同的规范化 `TypeId`；nullable、函数、名义和 intrinsic 类型只在外层身份、参数模式及
有序结构相同的路径上递归提取，不做型变、数值 widening、接口反向猜测或“最佳公共类型”
合并。每个 callable 类型参数都必须得到唯一解，否则该候选推导失败。

候选获得完整替换后，先替换其参数和返回类型，再执行既有 assignability 与参数 mode 检查。
interface、`Copyable` 和 `Transferable` bound 分别按现有静态能力查询验证；显式或唯一候选的
interface / `Copyable` 失败继续使用 L0093 / L0115，`Transferable` 失败使用 L0141。overload
trial 中某候选无法推导或不满足 bound 时只淘汰该候选，不泄漏试探诊断；全部候选被淘汰时
使用 L0123。唯一直接泛型候选无法得到完整一致替换时使用 L0140。

member 泛型实例先应用 receiver 的 classifier 类型实参，再解析 callable 自身类型实参。
typed 产物为每个成功调用发布实例 key：静态 callable target，以及按“owner 参数在前、
callable 参数在后”的声明顺序排列的完整替换类型。参数与返回类型必须已经替换；非泛型
call 的实例实参为空。该 key 是后续单态化的类型层 recipe，不在 Phase 2 克隆函数 body、
生成 IR 或判断递归实例图是否有限。

### 28.3 多 overload 候选中的 lambda

候选按源码顺序完成名称、实参映射、mode、泛型实例化和所有非 lambda 实参过滤后，才检查
lambda literal。每个剩余候选在隔离的 typed trial 中把对应函数类型单向传播给 lambda，
检查 move-only 形状、参数数量/mode、body 与嵌套调用，并保存完整的候选局部 typed 增量。
失败 trial 的 expression type、lambda 参数 mode、nested call descriptor 与诊断必须全部
丢弃，不能污染下一个候选或最终产物。

若恰有一个 trial 成功，只提交该候选的完整增量并记录一次 call；没有成功候选时只产生
L0123，多个成功候选时只产生 L0124，均不发布任何 trial 的 lambda/call facts。只有一个完成
映射的候选时沿用普通 expected-type 检查并保留 L0084 等精确诊断。lambda body 可以证明某个
候选不相容，但不会触发隐式转换、返回上下文推导或候选优先级；源码声明顺序只稳定输出，
不用于打破歧义。

trial 是类型检查事务，不是运行时求值。源码 operand 仍只按源码顺序求值一次；Phase 3 只
消费最终提交的 call / lambda facts。实现必须显式快照或使用候选局部结果，禁止依靠“先写
全局表、失败后只删诊断”的不完整回滚。

### 28.4 诊断与分阶段实施

| 错误码 | 候选稳定含义 | primary / 关联位置 |
|---|---|---|
| L0140 | 唯一泛型 callable 无法从合法输入得到完整一致的类型实参 | primary 为 callee；label 指向未解/冲突的类型参数声明 |
| L0141 | callable 类型实参不满足编译器 `Transferable` bound | primary 为显式实参或触发推导的 operand；label 指向 bound 声明 |

SPEC-0177 先实现泛型 callable 显式/推导实例化、bound 和 instance key；SPEC-0174 再在该
实例化边界上实现 overload-lambda candidate isolation；二者均已完成。仍不在当前范围的
callable reference、safe call 等调用继续精确保留对应 `DeferredReason`，不得携带伪造实例
进入所有权检查或 SSA。

---

## 29. 名义、enum case 与 intrinsic `Box` 构造（v0.29）

> **现行状态**：v0.29 已于 2026-08-25 由用户明确启用并取代 v0.28；本节及 L0143–L0145
> 获得现行规范效力。该版本只封闭
> 已由 v0.20/v0.25 保存的构造表面形态到 typed/ownership/backend 的最小闭环，不引入二级
> 构造器、默认参数、继承构造、工厂优先级或完整 Kotlin 约束求解。

### 29.1 可构造目标与唯一身份

- 普通 `class` 和 `value class` 的类型名称是其唯一主构造目标；普通 class 省略显式主构造器
  时等价于零字段构造器。`interface`、`object` 与 enum root 不是构造目标。源码类型名称与
  value 函数同名时沿用双命名空间既有规则：value binding 优先，只有没有 value binding 时
  才把 type binding 解释为 constructor，不能把二者合成 overload set。
- `enum class E` 的每个 case 使用既有 `EnumCaseId` 作为构造身份。有 payload 的 case 写
  `E.C(args)`，无 payload 的 case 仍写值表达式 `E.C`，不得为泛型推导改写成禁止的空 `E.C()`。
  enum 本体内允许既有短名。case 构造结果的公开静态类型始终是替换后的 root `E<...>`，
  不把仅供 smart cast 使用的 case type 暴露为值类型。前者在 `Call` expression 上发布
  descriptor；后者直接在解析后的 `Name` / `Member` expression 上发布零 operand descriptor，
  不能先伪装成函数值或普通 call 再补写 construction identity。
- intrinsic `Box` 只有在 `TypeEnvironment` 显式绑定时是构造目标；源码同名 class 继续是普通
  nominal constructor。`Box<T>(operand)` 与 `Box(operand)` 都只有一个稳定名称为 `element` 的
  `Value T` 参数，结果为
  `Box<T>`，并继续服从 §25.3 的具体 value-class kind 限制。
- constructor 不声明或分配普通函数 `SymbolId`，也不伪装成 function value。typed target 使用
  `NominalId`、`EnumCaseId` 或 `IntrinsicTypeConstructor::Box`；callable reference、把构造器
  赋给变量和 constructor overload 都不在 v1 范围。

### 29.2 类型实参与受控 expected-result 推导

泛型 nominal/case/Box 构造只接受两种源码形态：完整显式类型实参，或完全省略。显式实参
写在现有 typed-call callee 后：`Pair<Int, String>(...)`、`Result.Ok<Int, Error>(...)`、
`Box<Point>(...)`；数量不匹配复用 L0091，不接受部分列表、`_`、默认实参或 `where`。

省略类型实参时按以下封闭顺序求解，不能交换步骤或从后续使用反推：

1. 先按源码顺序检查已经定型的非 lambda 构造 operand，并用 §28.2 的精确结构匹配从对应
   字段/payload/Box 参数提取类型参数；无 expected type 的数字字面量先按 §22 默认定型。
2. 若仍有未决参数，只能读取在进入本次 construction 检查前已经独立确定、完整规范化且不含
   constructor-local / deferred unknown 的 expected type，例如显式变量/返回标注、已定型的
   enclosing context，或唯一且已经实例化的 callable 参数。只有其外层 identity 精确等于本次
   nominal root 或 intrinsic Box 时，才按声明顺序补齐类型实参。尚有多个 overload 候选时的
   candidate-local expected type，以及仍需由本次 construction 反向完成的外层泛型 callable
   参数，都不能作为第 2 步输入；此时必须由 operand 得到完整解或显式写出 constructor 类型
   实参。operand 与合法 expected type 对同一参数给出不同规范化 `TypeId` 时推导失败，不做
   common type、widening 或型变。
3. 获得完整替换后检查 interface、`Copyable`、`Transferable` bound，再用替换后的 Value 参数
   类型单向检查全部 operand 与 lambda。lambda body 不参与第 1/2 步推导；只剩 lambda 能提供
   类型信息时必须写显式类型实参或提供第 2 步允许的独立、完整同 root expected type。

这项 expected-result 规则是 constructor/case 的窄化例外，不改变 §28 普通泛型 callable 的
单向规则。它使 `val ok: Result<Int, String> = Result.Ok(1)`、
`val empty: Empty<String> = Empty()` 和泛型无 payload case
`val none: Option<Int> = Option.None` 可确定；脱离同 root expected type 的 `Empty()` /
`Option.None`，或只有被第 2 步排除的 candidate-local expected 时使用 L0144，而不是发明
`Option<Int>.None` 新语法。成功实例 key 由构造 target
和声明顺序的完整类型实参组成，与源码是否显式无关。

### 29.3 参数映射、所有权与求值

- class/value-class 主构造器字段与 enum payload 按声明顺序形成稳定、可命名的参数；intrinsic
  Box 只有稳定参数名 `element`。位置/命名混排、重复、缺失、额外参数及 type/mode 检查复用
  §9 的 L0120–L0123 规则。字段 visibility 不删除 constructor 参数名；跨 package 可见性仍
  等待 SPEC-0025。
- 所有构造参数都是 v0.26 已规定的 `ParameterMode::Value`：class 字段与 enum payload 沿用
  §13 的天然-owned 声明形态，intrinsic Box 只有编译器内建抽象签名；两者调用点都不写
  `own`。显式 `borrow` / `&` 与 Value 参数不匹配并复用 L0122。operand 按源码顺序各求值
  一次，命名映射不改变求值顺序；每个 operand 完成后立即按 `Copyable` 复制或按 MoveOnly
  移动到尚未发布的 construction owner。
- 成功 typed 产物保存 expression、稳定 target/instance key、结果类型，以及按参数声明顺序的
  field/payload symbol、Value mode、源码 argument 与 argument evaluation index。无 payload case
  发布零参数 construction descriptor。constructor 专项分派优先于 ordinary callable/function-value
  分派；成功 construction 不发布 `CallDescriptor`，也不把 type/case callee 或 receiver 当作运行时
  operand 求值。descriptor 属于 §28.3 的完整 typed trial 状态，失败或未唯一提交的 overload trial
  必须连同 nested construction facts 一起回滚，不能只删除诊断。
- SPEC-0183 只发布上述名称/类型事实；SPEC-0188 再把 Value delivery、construction temporary、
  move/copy、失败后 use、ASAP drop 接入既有所有权产物，并发布源码求值顺序的 delivery effects
  与 construction root 的唯一 drop obligation。普通字段仍不形成可独立移动/析构的源码 place；
  SPEC-0184 按完整单态结果类型为该 root 生成递归字段/payload drop glue，再把已验证 facts lower
  到 ADR-0008 的 aggregate/class/enum/Box 表示，不在 Phase 4 重新推导参数映射或 owner liveness。

### 29.4 `Result` payload 勘误与诊断

现行词法规范把 `value` 保持为硬关键字；v0.29 不为一个标准库字段把它改成上下文软词，
避免扩大 Lexer/Parser 兼容面。核心 `Result` 声明固定为
`Ok(success: T), Err(error: E)`，取代附录中不可解析的 `Ok(value: T)` 示例；这只改 payload
名称，不改变 `Result<T, E>`、postfix `?` 或错误传播语义。

| 错误码 | 稳定含义 | primary / 关联位置 |
|---|---|---|
| L0143 | type-position callee 不是可构造的 class/value class/case/intrinsic Box | primary 为 callee 名称；label 指向实际 type 声明（若有） |
| L0144 | constructor/case 无法从 operand 与合法的独立同 root expected type 得到完整一致的类型实参 | primary 为 constructor/case 名称；labels 指向未决/冲突类型参数声明 |
| L0145 | 单态 nominal/enum/Box 的 target size/alignment/payload storage 无法表示 | primary 为触发实例化的 constructor/type use；label 指向来源类型声明或超限字段/case |

L0091、L0093、L0115、L0141 继续分别表示 arity、interface、`Copyable`、`Transferable` bound；
L0120–L0123 继续表示参数名称/数量/mode/类型候选失败。L0145 只把 SPEC-0186 已有 IR-local
preflight 映射为源码诊断，不把 target 阈值变成新的静态类型或隐式 boxing 规则。frontend
继续保持 target / LLVM 无关；L0145 由 codegen/native 边界使用 frontend 集中诊断目录和来源
`Span` 构造，在调用 LLVM object emitter 前返回，不能把 target 布局判断倒灌到类型检查器。

---

## 30. 约定程序入口与显式共享所有权（v0.30）

> **现行状态**：v0.30 已于 2026-08-26 由用户明确启用并取代 v0.29。本节封闭单文件
> conventional `main` 选择和单线程 `Rc<T>` 的语言契约；它不表示参数化入口、共享 owner
> runtime、一般 instance receiver 或一般 `String` runtime 已经实现，这些能力仍按本节的
> 分阶段边界由独立 Spec 验收。

### 30.1 单文件 conventional `main`

`main` 不是关键字，也不获得新的名称解析优先级。公开单文件 `kovenc build/run` 在调用者没有
显式传入 `--entry` 时，只在该源码文件的顶层值命名空间中选择名为 `main` 的具名函数。合法
conventional entry 只有以下两个完整单态签名：

```kotlin
fun main(): Unit { ... }
fun main(args: Array<String>): Unit { ... }
```

- entry 必须是顶层、非泛型、非 member 的具名函数，返回类型精确为 `Unit`。参数名称不参与
  签名匹配；一个参数的形式沿用 v0.26 默认 `Borrow`，不能写 `own` 或 `inout`。
- 省略 `--entry` 时，零个合法候选、存在同名但签名非法的候选，或两个合法形状同时存在，
  分别形成稳定的 missing、invalid-shape 或 ambiguous entry selection failure。它们属于构建
  操作错误，不分配语言诊断码，也不得被伪装成 Parser/type diagnostic。
- 显式 `--entry <name>` 继续使用 SPEC-0190 已发布的 `() -> Unit` 契约，并完全关闭
  conventional lookup；它可以选择任意满足该形状的顶层函数，包括名为 `main` 的函数。
- `main(args)` 收到不含可执行文件名的命令行参数，保持原顺序。native wrapper 拥有新建的
  `Array<String>`，在 entry 调用期间建立 Borrow，并在返回后按逆序析构；源平台参数不能转换
  为合法 UTF-8 时，在调用 Koven entry 前形成 operational failure，不以替换字符静默改写。
- 正常返回仍由既有 C ABI wrapper 映射为退出码 `0`；`error()`/abort、链接失败和启动失败
  沿用既有边界。v1 不允许 `main` 返回整数、`Result`、`Nothing` 或异步结果，也不定义多个
  package 的全局 main 搜索。

实施必须分两步：零参数默认选择只依赖 SPEC-0190；`main(args)` 还必须等待一般 `String`
runtime 与 argv `Array<String>` 构造/析构 ABI。前一步不得用 literal-only String、Rust `String`
或宿主指针伪造后一步。

### 30.2 `Rc<T>` 的共享所有权契约

普通 Borrow 仍是共享读取的默认方案；只有一个值必须在多个独立 owner 的生命周期中存活时
才使用 `Rc<T>`。`Rc` 是由 `TypeEnvironment` 显式绑定的 intrinsic type constructor，不按
源码拼写识别；源码同名 class 不取得任何 intrinsic 行为。

- `Rc(value)` / `Rc<T>(value)` 只有一个稳定名称为 `value` 的 `Value T` 参数。类型实参沿用
  §29.2 的“全部显式或完全省略”与精确推导规则；构造把 operand 复制或移动进单次 heap
  allocation 内的共享 owner payload，并建立初始 strong count `1`。
- `Rc<T>` 自身始终是 `MoveOnly`，即使 `T: Copyable` 也不满足 `Copyable`。普通赋值、返回和
  Value 交付移动 handle，不增加计数；禁止把 retain 隐藏在赋值或参数传递中。
- `owner.share()` 是唯一公开的 strong-owner 分叉操作。它以 shared Borrow 使用 receiver，
  不消费或修改 payload，返回指向同一 control block 的新 `Rc<T>`，并把非原子 strong count
  增加一次。它不是用户可覆盖的一般 member，也不能由同名源码函数冒充；计数溢出必须在
  写回前 abort，不能环绕。
- `owner.value` 是 compiler-bound、只读的 payload place。它建立受 owner 生命周期约束的
  shared Borrow；不能成为 `&` 实参、赋值目标或 MoveOnly 的 owned 读取来源。若 `T` 满足
  `Copyable`，既有 Copyable 规则可以从该 shared place 产生普通副本，但不因此复制 `Rc`
  handle。v1 不提供 `getMut`、interior mutability 或从共享 payload 移出值的后门。
- 每个 live `Rc` handle 在自己的 ASAP drop point 自动递减 strong count；归零的 handle 按
  `T` 的递归 drop glue 精确析构一次 payload，再释放整个 control block。用户不可调用
  `retain()`、`release()`，也不可读取或修改计数。
- `Rc<T>` 对所有 `T` 恒不满足 `Transferable`，与 §17/§27 的既有规则一致。v1 不引入
  `Arc<T>`、`Shareable`、`Weak<T>` 或跨线程共享；strong cycle 因而可能不释放，避免环必须由
  程序数据模型承担，后续版本在定义 `Weak` 前不得声称已解决 cycle。

`share()` 和 `value` 是 Rc intrinsic surface，只为封闭 SPEC-0045，不等价于提前实现一般
instance receiver、属性 getter 或 operator overloading。typed 产物必须发布稳定 intrinsic
operation identity、receiver/payload 类型和 Borrow/Value effect；所有权阶段消费这些 facts，
backend 不得按成员名称字符串重新推导语义。

### 30.3 Arena/handle 的边界

Arena 是对 Rc 的互补方案而不是别名：它适合 AST、IR 等整批同生命周期对象图，以一个 arena
owner 持有全部对象，引用使用 handle/index，arena 析构时批量释放，从而避免逐边 retain。
但是源语言若要安全暴露 `Arena<T>`，必须先定义 handle 与特定 arena 实例绑定的 identity、
跨 callable 逃逸和 arena 析构后的失效规则。v1 当前没有足够的生命周期参数或 generative
identity 表达这些约束，因此 v0.30 只确认该推荐方向，不发布标准库 `Arena` API，也不授权
用无检查裸指针实现。编译器内部 Rust arena 不受此源语言 API 门禁影响。

### 30.4 分阶段交接

1. conventional `main()` 的 CLI 选择单独实施并复用现有显式 entry/backend wrapper；
2. `main(args)` 在一般 String runtime 后实施，补齐 argv 转换、Array owner 和 operational
   failure；
3. `Rc` frontend/ownership/runtime 作为一个封闭 Goal 实施，但 runtime header 先由 accepted
   ADR 固定；
4. `Arc`/`Shareable`/`Weak`、Arena 源语言 API 和一般 instance receiver 各自等待后续 guide，
   不得由本节推断实现。

---

## 31. 一般 UTF-8 `String` owner 与最小运行时表面（v0.31）

> **现行状态**：v0.31 已于 2026-08-26 由用户明确启用并取代 v0.30。本节依据 SPEC-0196
> 完成后的 roadmap 依赖审计形成；ADR-0018/0019 已接受，SPEC-0192 已完成实施。

### 31.1 值、所有权与 UTF-8 不变量

`String` 继续使用 `TypeEnvironment` 显式绑定的 builtin identity，不由源码名称或 LLVM 布局
识别。它表示一段不可变、长度明确且始终合法的 UTF-8 字节序列；内容允许为空，也允许包含
U+0000，对外语义不依赖 NUL 终止。

- `String` 始终是 `MoveOnly` 且满足 `Transferable`，不满足 `Copyable`。赋值、返回、Value
  delivery 与 move closure capture 转移唯一 owner；它们不得隐式复制字节、retain 或建立共享
  control block。默认 Borrow 参数只在调用期间读取同一值。
- 普通无 interpolation 的 String literal 是一般 `String` 表达式，不再只对 `println`/`error`
  生效。实现可以让不可变 literal 引用静态只读字节，也可以在不改变可观察语义时消除临时
  allocation；SSA 类型、MoveOnly 状态与 drop obligation 仍必须与动态 String 保持同一契约。
- 动态 String 的 live owner 在既有 ASAP drop point 释放自己拥有的存储。静态 literal 不得被
  `free`；动态 owner 必须精确释放一次。具体 provenance/layout 由 String runtime ABI ADR
  决定，frontend 不发布 pointer、capacity 或 allocator 事实。
- v0.31 不改变 `String?` 的语言类型规则，但 pointer-like SPEC-0196 不适用于 String 的内联
  runtime value；`String?` native ABI 继续等待独立 inline-nullable 设计。

### 31.2 封闭的最小操作

v0.31 的一般 String runtime 只承接语言已经发布且不需要一般 instance receiver 的操作：

- `left + right` 在左到右各求值一次后，以同步 shared-read 方式读取两个 `String` operand，
  产生内容为精确字节拼接的新 `String` owner；不消费具名 operand。长度加法、目标布局或
  allocation size 无法表示时在发布部分结果前 abort。
- `==` / `!=` 比较 UTF-8 字节长度和全部内容，不执行 Unicode normalization、locale folding
  或 grapheme 处理。由于所有 String 均满足 UTF-8 不变量，字节相等与 Unicode scalar 序列
  相等一致。
- String 可以存入局部变量、作为普通 Value/默认 Borrow 参数、从函数返回、进入 closure
  capture，以及作为已经支持 drop glue 的 aggregate、enum、`Rc` 和顺序容器元素。现有
  ownership、loan、单态化和容器 relocation 规则不获得 String 特例。
- 标准 `println(value: String): Unit` 对任意 String Borrow 写出内容的全部字节，再写一个
  ASCII LF；内容中的 U+0000 不截断。短写或不可恢复的 stdout 失败沿用现有 abort 边界。
- 标准 `error(message: String): Nothing` 必须先按普通求值/借用规则形成 message，再进入既有
  abort effect；v0.31 不新增 stderr 文本格式或保证 message 一定被打印。

本最小表面不发布 `length`、索引、slice、builder、编码转换、用户构造器、可变 buffer、
intern、隐式共享或 `toString`/formatting protocol。String interpolation 虽已有 Lexer/Parser/类型
节点，但 operand 到 String 的转换契约尚未封闭；在后续 guide 定义可打印/转换协议前，native
lowering 必须确定性拒绝 interpolation，不能只支持若干 builtin 并假装成完整协议。

### 31.3 运行时和编译阶段边界

- frontend 只发布 builtin String identity、plain-literal bytes、既有 binary/call identity、
  Copyable/Transferable 与 ownership facts。SSA 必须使用专用 String owner/type/operation，LLVM
  不得按源码拼写或把 Rust/C 字符串对象直接塞入 Koven value。
- runtime 的字节指针、长度、存储 provenance、drop glue、concat、equality 与 stdout adapter
  必须由 accepted ADR 统一；内部 ABI 不承诺公共 C FFI 稳定性，也不得要求新增 workspace crate。
- 所有创建边界都必须保证合法 UTF-8。plain literal 由 Lexer/decoder 保证，concat 由两个合法
  operand 闭包保证；未来 argv 入口必须在创建 Koven String 前验证宿主参数，失败作为
  operational failure，不使用替换字符。
- runtime 必须使用真实 target `DataLayout` 和集中分配/abort 边界。不得以 host `usize`、
  `std::string::String` 布局或目标 C `char *` 的偶然表示代替 target-independent SSA 契约。

### 31.4 分阶段交接

1. SPEC-0192 实现 plain literal、传参/返回、`+`、`==`/`!=`、动态 `println`/`error`、drop glue，
   并验证 String 作为现有 aggregate/顺序容器元素的布局与析构；不接 argv；
2. SPEC-0194 只在 SPEC-0192 `done` 后构造不含 executable name 的 UTF-8 `Array<String>` argv
   owner，Borrow 调用参数化 main，返回后逆序析构；
3. interpolation、String member API、formatting/printable protocol、IO 和 `String?` native ABI
   分别等待后续 guide/Spec，不得为完成 0192/0194 提前固化。

---

## 32. package/import 绑定、跨文件可见性与 compilation unit（v0.32）

> **现行状态**：v0.32 已于 2026-08-26 由用户明确启用并取代 v0.31。本节及 L0146–L0151
> 已成为现行语义；ADR-0020 已接受，具体实现仍按 SPEC-0025→0197→0198 后分叉到
> SPEC-0199/0187 的依赖顺序推进。

### 32.1 compilation unit、package 与声明身份

- 编译 driver 向 frontend 显式交付一个 compilation unit：有限、显式且枚举顺序无语义的
  source root/source unit 集合，以及每个 source unit 的稳定 root identity、root 内逻辑路径和
  源码。frontend 不读取文件系统，也不从
  进程当前目录、绝对路径或输入枚举顺序猜测 package；路径映射继续遵守 ADR-0005。
- 有 `package a.b` 的文件，其 package 必须精确等于逻辑父目录 `a/b`；省略 package 只允许
  位于 source root 根目录。多个 source root 可以向同一 package 贡献文件，输入顺序不改变
  package 内容、声明身份或诊断顺序。
- frontend 为 package、source unit 和源码 symbol 发布 `PackageId`、`SourceUnitId` 与
  `UnitSymbolId`；可作为声明目标的 symbol 另有 `DeclarationId`。这些身份由规范化
  compilation-unit 输入确定，只在一次分析链内稳定，不是 LLVM symbol、公共 ABI 或持久化
  缓存格式；跨文件引用不得伪装成单文件 `SymbolId` 或外部未知 symbol。文件局部 AST / scope /
  symbol ID 不全局重编号，而是与 `SourceUnitId` 配对使用。
- compilation unit 先收集全部文件的顶层声明，再解析任一声明体。同一 package 沿用 §21 的
  类型/值双命名空间；函数只在同一 package、同一值绑定内形成有序 overload set。非函数
  重名、函数与非函数重名或不可合并的类型重名是跨文件声明冲突，不依赖文件装载顺序。

### 32.2 跨文件可见性

- 顶层 `public` 声明可被同一 compilation unit 的其他 package 通过 exact/wildcard import 或
  绝对限定名引用，并作为未来依赖项目的可导出表面。
- 顶层 `internal` 声明对同一 compilation unit 内所有 package 可见；跨 package 使用时仍须
  显式 import 或限定。它不向未来的依赖 compilation unit 导出。source set/依赖图尚未发布
  时，`internal` 的边界就是 driver 本次显式交付的 compilation unit。
- 顶层 `private` 声明只在其 source unit 内可见，不能由同 package 的其他文件导入或限定。
  member `private` 继续是 declaring classifier 内可见，不因多文件而扩大。
- 默认可见性继续遵守既有声明规则；本节不新增 package-private 修饰符，也不让 import
  绕过可见性检查。

### 32.3 exact、alias 与 wildcard import

- exact import 在类型和值命名空间中分别查找：一条路径可以同时绑定同名类型和值，`as Alias`
  同时作用于两者；至少一个命名空间存在可见目标即成功。值目标可以是顶层值声明，或同一
  package 中同名顶层函数构成的完整 overload set。alias 只改变当前 source unit 的本地绑定名，
  不改变目标 identity、声明名或导出表面。
- wildcard import 的目标必须是一个 package；它只按需暴露该 package 的可见顶层声明，
  不递归子 package，不导入类型 member，不形成 re-export，也不在文件头阶段物化无限绑定。
- 类型和值命名空间分别处理冲突。同一个 exact target 以同一个本地名重复导入是幂等的；
  不同 exact target 绑定到同一命名空间/本地名是错误。跨 package 的同名函数不会因两个
  exact import 自动合并为 overload set。
- 当前文件或同 package 自动可见声明若与 exact import 的本地绑定同名，是明确冲突而不是
  静默遮蔽；词法 local 仍按 §21 的正常规则遮蔽文件级绑定。
- exact import 比 wildcard 候选优先。多个 wildcard 只有在某个名称被实际查询且仍指向多个
  可见 target 时才产生歧义；未使用的潜在冲突不报错。没有隐式 prelude wildcard import。
  编译器显式注入的 builtin/prelude environment 也不视为源码 import，不产生 import reference。

### 32.4 限定名称解析

- import target 和静态限定名称先按最长 package 前缀解析，再在余下路径中选择一个顶层声明
  及现行允许的静态成员/case；路径必须整体成功，不得把“已解析前缀 + deferred 尾部”伪装
  为成功。import 始终是绝对 package 路径。
- 普通表达式中的裸名称先执行 §21 的词法/文件查询。package 只存在于静态名称路径，不是
  runtime value，不能赋值、传参、捕获或作为 member receiver；本节不引入 Kotlin/Rust
  风格的相对 package 别名、`self` / `super` / `crate` 路径。
- 单段 exact import 可引用默认 package 的顶层声明；同样的 `Foo.*` 仍按 package wildcard
  解释，要求存在 package `Foo`。import 的终端名称、alias 与普通/限定引用都必须发布目标
  identity 和精确引用 Span，供诊断及工具查询复用。

### 32.5 诊断与分阶段交接

v0.32 分配 L0146–L0151：package 与逻辑路径不匹配、同 package 跨文件声明冲突、import target
未解析、目标不可见、exact import 绑定冲突、wildcard 实际使用歧义。诊断必须包含发生使用或
声明冲突的主 `Span`，并在可用时附带目标/冲突声明位置；排序由稳定 source-unit key、字节
位置和错误码决定。省略 package 但文件不在 source root 根目录时，L0146 的 primary 是文件
起始处的空 Span；无效逻辑路径、重复 `(root identity, logical path)` 属于 driver/unit 输入错误，
不是可归因于 Koven 源码的 L-code。

1. SPEC-0025 只建立 compilation-unit package index、跨文件声明身份、import/可见性名称绑定
   与上述诊断，不做跨文件 body 类型检查；
2. SPEC-0197 在 0025 之后完成跨文件签名与 body 类型检查，发布 compilation-unit typed facts；
3. SPEC-0198 在 0197 之后检查跨文件调用/构造的所有权效果和 drop facts；
4. SPEC-0199 在 0198 之后完成 compilation-unit reachability、单态化、SSA/LLVM 与单 object；
5. SPEC-0187 同样在 0198 之后复用同一 package/typed/ownership 产物扩展 LSP，与 0199 并行，
   不能维护第二套 resolver；首个 host provider 候选由 ADR-0021 的版本化初始化 source set
   提供，不把打开 URI 集合或磁盘扫描当成隐式 unit。

本节不定义 manifest、依赖解析、package re-export、模块初始化、增量缓存、跨 compilation-unit
ABI 或多 object 链接策略；也不把 SPEC-0025 扩张为项目构建、类型、所有权或 codegen Spec。
ADR-0022/SPEC-0052 可独立起草工具侧 manifest→source-set adapter，但它不因此成为语言语义、
也不定义 dependency、target 或 process entry。

---

## 33. 本地 project process entry 与公开 build/run（v0.33 候选，未启用）

> **候选状态**：本节是 v0.32 多文件语义的后继工具契约。当前唯一权威版本仍是 v0.32；只有
> 用户明确启用 v0.33 并指定其取代 v0.32 后，本节才能约束公开 CLI。
> SPEC-0054 在此之前保持 `draft`；本候选不改变 ADR-0022 manifest version 1，因此不另建
> project-target schema ADR。

### 33.1 project mode 与 entry selector

公开 project mode 使用与单文件位置参数不可混淆的固定形式：

```text
kovenc build --project <project.toml> --entry <qualified-name> -o <executable>
kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]
```

- `<project.toml>` 必须显式提供并遵守 ADR-0022/SPEC-0052；CLI 不从 cwd、源码路径或祖先目录
  搜索 manifest，也不按参数是文件还是目录猜测模式。现有 `build/run <source.ko> ...` 单文件
  形式及 §30.1 的 conventional `main` 行为完全不变。
- project mode 首版强制 `--entry`，不扫描整个 unit 寻找 `main`，不读取 manifest target/entry
  默认值，也不猜默认 package。`main` 在 project mode 仍只是普通函数名。
- selector 是一个或多个点分 Koven Identifier：最后一段是顶层函数名，之前各段是绝对 package；
  单段 selector 精确表示默认 package 的函数。它不经过当前文件 import，不接受 alias、wildcard、
  root identity、logical file path、类型 member 或 overload signature 文本。不能按该 grammar
  解析的 selector 是 CLI usage error，不进入 package lookup。
- selector 在完整、validated compilation unit 的 package/declaration index 上解析。entry 必须是
  有 body、顶层、非泛型的 `public` 或 `internal` 具名函数；顶层 `private` 只具有 source-unit identity，
  不能由 project selector 绕过可见性或用文件路径消歧。

### 33.2 process shape 与选择失败

project selector 允许与 conventional main 相同的两个完整 process shape：

```kotlin
fun start(): Unit { ... }
fun start(args: Array<String>): Unit { ... }
```

- 零参数与参数化 shape 精确复用 §30.1 / ADR-0019：返回 `Unit`，参数化形式只有一个默认/shared
  Borrow `Array<String>` 参数；参数名不参与匹配。generic、`own`/`inout`、其他参数或返回类型
  都不是合法 process entry。
- 先按 selector 找到目标 package 的同名 declaration/overload set，再过滤可见、合法 shape。
  不存在目标、只有 private 目标、存在目标但没有合法 shape、存在多个合法 shape 分别形成
  missing、inaccessible、invalid-shape、ambiguous project-entry operational failure。一个合法
  shape 与任意数量非法 overload 共存时选择该唯一合法 shape。
- 这些失败不分配 `Ldddd`。所有 source/package/import/name/type/ownership 诊断必须先完成并按
  unit 规则发布；只有 validated unit 才进行 entry selection。CLI 把选中的 `DeclarationId` 和
  process shape 交给 codegen，SSA/LLVM/linker 不按字符串重新查找。
- project 显式 selector 支持两个 process shape；这不改变单文件显式 `--entry <name>` 已发布的
  零参数-only 兼容契约。`kovenc run -- ...` 的 argv 排除 executable name、UTF-8 预检、顺序、
  Borrow Array/String owner 与析构继续精确复用 §30.1/SPEC-0194。

### 33.3 产物、失败原子性与 Phase 边界

- `build` 仍要求显式 `-o` 并拒绝已经存在的最终路径；object 和 linker output 使用输出目录内
  的唯一临时路径，只有 codegen、link 与最终 no-clobber commit 全部成功才发布 executable。
  manifest、任一 source、object、临时 executable 与 final 不能重合。失败清理本次临时产物，
  不删除/覆盖调用者已有文件。`run` 继续使用进程拥有的临时目录并在
  子进程结束后清理。
- manifest/provider、entry/link/launch/cleanup failure 是具体 operational error；frontend
  diagnostics 继续遵守 human/JSON Lines 选择，项目错误不得伪造成语言 diagnostic。成功 build
  stdout/stderr 为空；run 继续转发程序 stdout/stderr 与可表示的退出状态。
- 实施顺序固定为：SPEC-0052 产生 base source set；SPEC-0025/0197/0198 形成 validated unit；
  SPEC-0199 生成单 object；SPEC-0054 才增加公开 project CLI、entry selection 与 executable
  commit。任一前置未完成时不得用拼接源码、逐文件 object 或单文件 bootstrap 循环假实现。

本候选不定义 manifest target/default entry、依赖解析、lock、跨 compilation-unit import/ABI、
多 object、library artifact、安装/发布、cross target、缓存或全项目 conventional main。无依赖
本地 executable 完成后，dependency-aware build 继续等待 SPEC-0053/0200 与新的 ABI 决策。

---

## 34. 显式 instance receiver 契约与静态分发调用（v0.34 候选，未启用）

> **候选状态**：本节以现行 v0.32 为基线，只增加 receiver 契约；版本号不自动包含或启用
> 同样尚未启用的 §33 project build 候选。只有用户明确启用 v0.34 并指定其取代 v0.32 后，
> 本节才能改变 member 声明或调用；除非用户同时明确启用 §33，否则 §33 继续保持候选。
> SPEC-0201、0180、0181、0191 在此之前保持 `draft`。本节复用 ADR-0016 已接受的
> Value/Borrow/Inout 内部 callable ABI，不新增 receiver ABI ADR。这里的“静态分发”指
> instance member target 在编译期确定，不是 companion/type-level static member。

### 34.1 声明语法与规范化 receiver

instance member 在既有固定 modifier 顺序末尾增加可选 receiver mode：

```text
method_receiver_mode = "borrow" | "inout" | "own" ;
method_modifiers = [ visibility_modifier ], [ "override" ],
                   [ method_receiver_mode ] ;
interface_member = [ "public" ], [ method_receiver_mode ],
                   function_declaration ;
```

```kotlin
class Buffer(var size: Int) {
    fun inspect(): Int = this.size
    borrow fun sameInspect(): Int = this.size
    inout fun clear(): Unit { this.size = 0 }
    own fun finish(): Int = this.size
}
```

- 缺失 marker 与显式 `borrow` 精确规范化为 `Borrow` receiver；`inout` 形成 exclusive
  non-owning receiver，`own` 形成内部 `Value` receiver。receiver 是隐藏的第一个 callable
  operand，类型为替换 owner 实参后的名义实例；interface body 中使用保持静态分发的 `Self`。
- receiver mode 只允许修饰 class/value class/interface/enum class/object 的实例函数；具名
  `object` 没有运行时状态，只接受缺省或显式 Borrow。顶层函数、companion 关联函数与其他
  声明不接受 receiver marker。顺序固定为 visibility、`override`、receiver mode、`fun`；
  重复或乱序使用既有 invalid/unsupported declaration modifier 诊断，不把 marker 当成函数名。
- 本节启用后，整体取代 §13.1 对 instance-function modifier 的旧封闭列表：`borrow`、`inout`、
  `own` 只按本节位置合法，`nocopy` 及其他未列 modifier 继续 unsupported；在 v0.34 未启用时，
  §13.1 的现行 Parser 规则继续有效。
- receiver mode 不参与 overload shape；缺省 Borrow 与显式 Borrow 不能形成重载。interface
  replacement、concrete `override`、default 冲突与 `super<I>` 选择必须精确比较规范化 receiver
  mode，如同既有显式参数 contract，不允许用返回类型或 mode 区分同 shape overload。

### 34.2 调用顺序、`this` 与所有权能力

- `receiver.member(arguments)` 只按 receiver 的静态名义类型、完整 interface closure 与已替换的
  owner/callable 类型实参选择 target；不产生 interface runtime value、vtable、RTTI 或代理。
  receiver expression 精确求值一次并先于显式 argument；选定 mode 的 loan/copy/move 在第一个
  argument 求值前生效，随后沿用 §26.1 的源码顺序 argument 交付与同步调用期 loan。
- 调用点不新增 receiver marker。Borrow receiver 对 stable place 或 temporary 建 shared loan，
  temporary 延命到返回；Inout receiver 要求 §26.3 意义上的可独占 receiver place且拒绝
  temporary；Value receiver 对 `Copyable` 产生 owned copy，对 MoveOnly 移动整个 owner。
  Inout receiver 的 loan identity 覆盖整个 owner place，callee 不得重新绑定或完整替换 `this`，
  只能修改允许写入的 `var` 字段。普通 class 的不可变 handle binding 可在 owner 可独占时调用
  Inout method：内部 ABI 仍借用现有 handle storage，callee load 同一非空 handle 后修改 payload，
  不得写回另一 handle；内联 value/enum 会直接修改 storage，因而继续要求递归 mutable root。
- `this` 是不可重新绑定的隐式 binding。Borrow `this` 只读或 shared reborrow；Inout `this`
  还可修改 `var` 字段和建立 exclusive reborrow，但不能把普通字段移出后留下洞；Value `this`
  是与普通 Value 参数一致的不可变 owned binding：可读取、shared reborrow 或整体移动，但不
  修改字段、不形成 exclusive reborrow，也不能直接调用 Inout member。需要继续修改时，必须
  先把整个 `this` 移入显式 `var` local；该移动后 `this` 不再可用。未消费的 Value `this` 由
  callable 在正常退出路径负责唯一 drop。普通字段部分移动禁令对三种 receiver 都不放宽。
- 裸 instance field/member 先按 §21 的局部词法规则解析；没有局部/参数遮蔽时精确等价于同一
  receiver binding 上的 `this.name`。显式 `this.name` 可绕过局部遮蔽，但不得为一次调用构造
  第二个 receiver 求值。§27.2 对直接捕获 `this`/field 的禁令保持不变，三种 receiver mode
  都不能让 `move` closure 直接捕获 `this`；Value `this` 可先显式整体移动到 local，再按普通
  local capture、`Transferable` 与 use-after-move 规则处理。
- `super<I>.method()` 仍是对当前 `this` 的静态 default 调用。当前 receiver capability 必须能
  满足目标 contract：Borrow 只能提供 Borrow，Inout 可 shared/exclusive reborrow，Value 可
  Borrow 或整体 Value 交付但不能提供 Inout。Value 交付后当前 `this` 不再可用。

### 34.3 窄化接口委托

v1 的 `Interface by valField` 只接受**全部可转发 requirement 都是 Borrow receiver** 的接口。
编译器生成的 forwarder 精确复制 interface member 的显式参数 mode/type、泛型参数、返回类型
与 effect，并依次 shared-borrow outer receiver、投影唯一 delegate field、以 Borrow receiver
静态调用 delegate 实现；receiver 与每个显式 argument 都只求值一次，不创建隐藏 AST、owner、
retain、proxy 或 `dyn`。

若经过手写 override/default 解析后仍需由 delegate 提供的有效 requirement 中存在 Inout 或
Value receiver，使用 `by` 形成 L0152；调用者必须写显式 `override`，自行决定如何取得可变
receiver 或消费 owner。该限制收窄 §23.4 与 grammar §13.3 在 receiver 尚未定义时留下的
“完整保持 receiver”表述，避免从不可变 delegate 字段隐式部分移动、替换或授予特殊
exclusive access。手写 override、default 与多 delegate 冲突仍沿用 §23 的优先级和
L0100/L0105 级联抑制。

### 34.4 分阶段交接与非目标

1. SPEC-0201 只扩展 member modifier Parser/AST/恢复；
2. SPEC-0180 发布规范化 receiver contract、`this`/member call 与 Borrow-only delegate
   forwarder typed facts；
3. SPEC-0181 消费上述 facts，建立 receiver loan/move/drop、字段冲突与 capture 所有权事实；
4. SPEC-0191 把隐藏 receiver lower 到既有 Value ABI 或 ADR-0016 Borrow/Inout pointer ABI，
   完成静态分发的 member/default/override/delegate native 闭环。

本候选不定义 callable reference/绑定 method value、extension method、safe call、borrow-return、
动态 interface value、反射或 vtable。`for` 仍需独立候选封闭 Iterable/Iterator identity、provider
是否拥有 source、`next()` Value delivery 与提前退出清理；receiver 完成不自动授权 SPEC-0179/
0182。具体集合、IO、thread API 也仍由各自 guide/Spec 定义，不能按 member 名称硬编码。

L0152 自 v0.34 启用后稳定表示“interface delegation cannot forward a non-Borrow receiver”；
primary 为 `by`/delegate target，label 指向首个不兼容 interface member。其余 receiver 失败复用
L0099/L0100（contract）、L0131–L0135（move/loan/mutable place），不得另造重叠错误类别。

---

## 35. nullable `when` 剩余域与 `!!` 所有权（v0.35 候选，未启用）

> **候选状态**：本节直接以现行 v0.32 为基线，只闭合既有 nullable 控制形式的 frontend
> facts、所有权和分阶段 lowering；它不自动包含或启用候选 §33 project build、§34 receiver。
> 只有用户明确启用 v0.35 并指定其取代 v0.32 后，本节才能改变 `when`/`!!` 的实现契约；
> SPEC-0202–0207 在此之前保持 `draft`。本节不改变既有语法、`T?` 类型规则或 ADR-0017
> pointer-like null-niche ABI。

### 35.1 共同求值、证明与 owner 原则

- nullable subject/operand 精确求值一次。null 判别只读取 nullable source，不复制、retain、移动
  或改变 wrapper；成功的 non-null edge 产生绑定到同一 root/place/loan identity 的 non-owning
  proof/view。Borrow/Inout binding、普通字段和容器元素可形成只读 proof，但不因此获得 owned
  inner 或可移动 root。
- proof 只证明同一 source 在当前 CFG edge 非空，不产生第二个 `T` owner。shared read、projection
  或显式 `Rc.share()` 可复用该 view；owner/place move/drop、赋值、冲突调用及 branch join 继续
  按 §24/§26 终止 proof。ADR-0017 首版 SSA 只表示 owned nullable Value 的 branch；loan/place
  subject 的 native proof 需要后继 nullable-place branch ADR，不能把 loan 伪装成 owner。
- 从已证明非空的 `T?` 取得普通 Value `T` 时统一采用 extraction：`T : Copyable` 复制 inner，
  原 nullable root 仍可用；MoveOnly `T` 消费**整个** nullable root 或 temporary，并把唯一 inner
  obligation 转交给结果。不得从 Borrow/Inout binding、普通字段、容器元素或其他不能整体
  Value-deliver 的 place 移出 inner 后留下 wrapper/owner 洞。
- MoveOnly extraction 只能在已有 non-null proof 的 edge 执行 take，消费 wrapper 并转移 inner；
  nullable `when` 直接使用 branch proof，`!!` 则先自行建立 null/non-null branch。`!!` 的 null
  edge不执行 take，而是直接进入 compiler-bound Abort，因此没有正常后继，也不生成 unwind
  cleanup。只有成功 take 的 non-null edge继续执行，原 binding 在该 edge 已 moved，后续使用
  沿用 L0131。

### 35.2 nullable `when` 的剩余域

- subject 仍按 §12/§24 只求值一次。对稳定 `T?` subject，显式 `null` condition 的匹配 edge
  获得 null fact，不匹配 edge 获得 non-null fact；后续 entry 接收之前所有未匹配 condition 的
  剩余域，最终 `else` 接收完整补集。
- 同一 entry 的逗号 alternatives 独立从该 entry 的输入域判断，body 只保留所有可到达匹配
  alternative 共同成立的事实。因此 `null, SomeCase -> ...` 不把 body 错误收窄为非空；只有
  每条可达 alternative 都证明非空时，body 才获得 `T` view。
- nullable enum/Boolean 的 case coverage 与 null coverage 组合现行有限域规则；重复覆盖、非穷尽、
  branch type 与非法 condition 继续使用 L0108–L0112。临时 subject 没有可供源码复用的 stable
  binding，但 lowering 仍必须在内部携带同一 subject owner，不能重新求值表达式。
- 分支内只读/借用 subject 使用 non-owning view；若表达式上下文要求 Value `T`，则按 §35.1
  Copy/Consume extraction。未提取的 temporary/owned subject 在每条正常分支出口按既有 ASAP
  规则恰好 drop 一次；已提取分支不得再次 drop wrapper 或 inner。

### 35.3 非空断言 `!!`

- `e!!` 继续具有 §3 已定义的表面语义：要求 `e : T?`、结果为 `T`，失败等价于
  `error("Non-null assertion failed")`；本候选补充其所有权效果，不把脱糖文本当作重复求值。
- `e` 为 `Copyable` nullable 时，非空 edge 复制 inner；若 `e` 是可继续访问的 place，原值保持
  可用。`e` 为 MoveOnly nullable 时，`!!` 是 Value extraction，必须整体消费合法 owner
  root/temporary。Borrow/Inout binding extraction 使用 L0133，普通字段 partial extraction 使用
  L0132，顺序容器 element extraction 使用 L0136，active-loan 冲突使用 L0135；这些拒绝不适用于
  Copyable inner 的普通复制。
- `!!` 不提供 place-preserving borrow unwrap，也不根据外层 Borrow receiver/call argument
  静默改变结果契约。需要非消费访问时使用显式 null check/nullable `when` 的 non-null view；
  borrow-return 或可存储 nullable view 等待后续语言设计。
- 非 nullable operand 继续使用 L0085；move/partial-move/loan 失败使用上述
  L0131–L0133/L0135/L0136 稳定分类，不把 L0134 的 mutable-place 含义挪作 extraction。
  本候选不分配 L0153，也不把 codegen 尚未接线伪装成新的源码错误。

### 35.4 分阶段交接与非目标

1. SPEC-0202 发布 nullable `when` 的稳定 subject、entry 输入/剩余域、alternative 交集与 body
   non-null typed facts；
2. SPEC-0203 消费 0202，发布 view/extraction、branch owner/drop 与 join 所有权事实；
3. SPEC-0204 把 pointer-like nullable `when` lower 到 ADR-0017 的 `NullableBranch/Take`；
4. SPEC-0205 发布 `!!` 的 operand/category、nullable/inner 类型、extraction 与 compiler-bound
   assertion Abort effect typed descriptor；该 effect 是语法内建身份，不解析或调用同名函数；
5. SPEC-0206 消费 0205，建立 Copy/Consume、abort edge、move/loan/drop 事实；
6. SPEC-0207 把 pointer-like `!!` lower 到 `NullableBranch/Take` 与既有 SSA Abort primitive。

上述 frontend facts/ownership 适用于所有已接受的 nullable 类型；首轮 native 实施只覆盖
ADR-0017 已支持、且由 owned whole-root/temporary 承载的普通 class、`Box`、`Rc` pointer-like
nullable。pointer-like Borrow/Inout/field/element subject 的 proof lowering 等待 nullable-place/
loan branch ADR，不能交给 owner-only `NullableBranch`；这不反向否定其 frontend 合法性。
scalar/value/enum/String/顺序容器等 inline/tagged nullable 需要独立 SSA/LLVM ABI ADR 与后继 Spec；
Elvis、safe call、`as?`、nullable function value、nullable borrow-return 和跨 nullable 的 place-return
也继续延后。v0.35 不改变这些类型/语法的既有 frontend 接受边界，只禁止后端凭表示猜测接线。

---

## 附录：核心结构声明总览（原第二部分）

> 原文档第二部分独立成章，本次拆分中并入设计决策文档作为收尾附录：这段示例把第 1–21
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
    Ok(success: T),
    Err(error: E)
}

object Config {
    const val VERSION: String = "1.0"
}

class Counter(var count: Int) {
    companion object {
        fun zero(): Counter = Counter(0)
    }
}

fun <T : Comparable<T>> max(own a: T, own b: T): T = if (a > b) a else b
```

---
