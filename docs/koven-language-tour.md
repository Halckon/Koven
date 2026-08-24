# Koven 语言教程

> 本教程基于当前权威的 Koven 语言设计规范 v0.28 文档集整理,面向使用 Koven 编写程序的开发者,组织方式参考了 Go Tour、The Rust Book 与 Kotlin 官方文档。原始设计规范是写给负责实现编译器的 AI agent 看的实现契约,充满词法/语法分析的内部细节;这份教程要做的事情,是把其中已经确定的语言设计,重新组织成一份面向人的语言导览。

## 关于当前状态,需要提前说明

Koven 编译器已经完成 **Phase 1（词法分析 + 语法分析）**，Phase 2 已实现单文件名称解析、
基础与名义/泛型/interface 类型检查、`when` 穷尽性与 smart cast、条件 `Copyable`、有限
内联布局、结构化解构、单态 callable/member 选择与顺序容器类型检查。Phase 3 已实现整变量
所有权状态、use-after-move、条件复制、结构化移动、调用期 loan 与 owned-value ASAP
析构点、顺序容器核心 element place 的读取、借用与替换所有权，以及 v0.27 的简化 closure
capture、`Transferable` 与 compiler-bound 跨线程 effect；Phase 5 容器增删/重排 relocation
API、代码生成（Phase 4）和
标准库（Phase 5）仍待实施。跨文件 package/import 名称解析也尚未完成。
也就是说：

- 本教程里的 Phase 1 语法——基础类型、变量、函数、`value class`/`class`、调用标注、lambda、
  control-flow 与文件结构——已经有完整、可执行的 Parser。SPEC-0176 已让 frontend 接受
  v0.26 的“无标记 / 显式 `borrow` = `Borrow`、显式 `own` = `Value`”声明契约。
- **控制流已在 v0.18 定稿；class 家族、类型级 companion、匿名内部类边界与窄化接口委托已在 v0.20 定稿。**
- **单文件名称、作用域、重载组与未解析名称诊断已在 v0.21 定稿，并由 SPEC-0018 实现。**
- **名义/泛型/interface 类型检查已由 SPEC-0020 实现；v0.24 的 enum case type、有限域
  `when` 穷尽性与 smart cast 已由 SPEC-0021 实现。**
- **v0.25 的条件 `Copyable`、intrinsic `Box`、有限内联布局和局部 value-class 解构类型事实
  已由 SPEC-0022 实现。**
- **v0.26 已将普通 callable 的声明侧契约调整为“无标记 `Borrow`、显式 `own` 消费”；调用点
  仍不写 `own`，向 `own` 参数交付 MoveOnly place 时会隐式移动。该表面语法和 typed contract
  已由 SPEC-0176 实现；调用期 loan 与 owned-value ASAP 析构点已由 SPEC-0029 实现。**
- **顺序容器核心 element place 所有权已由 SPEC-0030 实现：MoveOnly 元素不能按值从索引
  移出，借用按逻辑索引判定冲突，成功替换会记录旧元素的唯一析构点。**
- **v0.27 的默认 shared capture、显式 `move` owned capture、borrowed closure 逃逸边界、
  结构化 `Transferable` 与 compiler-bound 跨线程检查已由 SPEC-0032 实现。**
- **v0.28 已启用，但尚未由 frontend 实施。** 泛型 callable 实例化与 overload-lambda 候选
  隔离已成为现行语义；泛型 callable 已由 SPEC-0177 实现，在 SPEC-0174 完成前可先把 lambda 绑定到带显式函数
  类型的局部变量，再传给重载函数。
- `Map`/`MutableMap` 的所有权契约仍是候选设计；`Copyable` opt-out 已明确不进入 v1；错误传播 `?` 已由 v0.19 定稿并完成 Phase 1 Parser。

换句话说,这份教程描述的是 Koven v1 **应该长成的样子**,而不是"现在就能装个编译器跑起来"的使用手册。

## 目录

1. [认识 Koven](#1-认识-koven)
2. [快速开始](#2-快速开始)
3. [基础语法](#3-基础语法)
4. [函数](#4-函数)
5. [所有权与借用:和 Kotlin 最大的不同](#5-所有权与借用和-kotlin-最大的不同)
6. [用类型建模](#6-用类型建模)
7. [控制流](#7-控制流)
8. [集合类型](#8-集合类型)
9. [错误处理](#9-错误处理)
10. [并发](#10-并发)
11. [输入输出](#11-输入输出)
12. [v1 特性一览:支持与不支持](#12-v1-特性一览支持与不支持)
13. [从 Kotlin / Rust 迁移过来的注意事项](#13-从-kotlin--rust-迁移过来的注意事项)
14. [关键字与运算符速查](#14-关键字与运算符速查)
15. [路线图](#15-路线图)

---

## 1. 认识 Koven

### 1.1 一句话介绍

Koven 是一门编译型语言:语法尽量贴近 Kotlin 的命名与书写习惯,内存管理采用 Rust 式的所有权/借用模型(没有垃圾回收器),编译器用 Rust 实现,后端基于 LLVM。

### 1.2 设计哲学

Koven 想把两种开发体验拼接在一起:

- **读起来像 Kotlin**:`fun`、`val`/`var`、`class`、`interface`、`when`、`object`、可见性修饰符(`public`/`internal`/`private`)都直接沿用 Kotlin 的拼写和大部分语义。
- **管起来像 Rust**:没有 GC,值的生命周期由编译期所有权/借用规则管理;`value class`(内联值类型)与 `class`(堆分配引用类型)的区分,对应 Rust 里值类型与堆分配类型的区分。

这两件事拼在一起并不是没有代价的——第 5 章会专门讲这一点,因为它是整门语言里最容易产生错误预期的地方。

### 1.3 现在能做、不能做什么

- **已完成当前前端主线**:基础类型、变量与常量声明、函数、控制流、class-family、单文件名称
  解析、名义/泛型/interface 类型检查，以及本教程描述的 v0.27 核心所有权与 closure capture。
- **仍有明确门禁**:跨文件 package/import 解析、容器增删/重排 relocation API、instance/delegation
  receiver 所有权，以及代码生成与标准库实现仍待后续 Spec 或 guide 封闭。
- **完全尚未设计**:`Map`/`MutableMap` 的可实施契约、用户自定义索引运算符。
- **已确定不支持**:自定义属性访问器、扩展函数、异常。

完整列表见第 12 章。

---

## 2. 快速开始

下面这段代码符合当前 guide 锁定的 v1 语法,展示的是编译器完工后一个 Koven 程序大致的样子(目前还不能真的拿去编译执行):

```kotlin
value class Point(val x: Int, val y: Int)

enum class Shape {
    Circle(radius: Double),
    Point;

    fun area(): Double = when (this) {
        is Circle -> 3.14159 * radius * radius
        is Point -> 0.0
    }
}

fun main(): Unit {
    val points: List<Point> = listOf(Point(x = 1, y = 2), Point(x = 3, y = 4))
    val first = points[0]
    val double: (Int) -> Int = { x -> x * 2 }
    println(double(first.x))
}
```

Koven 源文件建议使用 `.ko` 扩展名(编译器测试套件里统一采用这一约定)。源文件必须是合法 UTF-8;标识符只能由 ASCII 字母、数字、下划线组成,区分大小写。顶层声明通常用换行分隔;若要在同一行写多个顶层声明,必须用 `;` 分隔。`;` 不是通用语句结束符,不能用来分隔函数 block 内的 element。

---

## 3. 基础语法

### 3.1 注释

```kotlin
// 行注释,直到行尾结束
/* 块注释,不支持嵌套 */
```

v1 不区分文档注释,`///`、`/** ... */` 和普通注释处理方式相同。

### 3.2 变量与常量

```kotlin
val name: String = "Koven"     // 不可变绑定,类型标注可省略
var count = 0                  // 可变绑定,类型由初始化式推导
const val Pi: Double = 3.14159 // 编译期常量,固定前缀 const val
```

三种声明都必须有初始化式——Koven 没有"先声明后赋值"的空绑定。`const` 不能单独出现(没有 `const var`),只有 `const val` 这一种固定拼写。

### 3.3 基础类型

| 类型 | 说明 |
|---|---|
| `Byte` `Short` `Int` `Long` | 有符号整数,8/16/32/64 位;整数字面量默认类型是 `Int` |
| `UByte` `UShort` `UInt` `ULong` | 无符号整数 |
| `Float` `Double` | 32/64 位浮点;浮点字面量默认类型是 `Double` |
| `Boolean` | 布尔 |
| `Char` | 单个 Unicode 标量值 |
| `String` | UTF-8,不可变 |
| `Unit` | 无返回值 |
| `Nothing` | 不返回的函数的返回类型标注,例如 `error()` |
| `Any` | 所有类型的顶层类型(仅用作泛型上界的编译期概念,不是 Java `Object` 那种可反射的运行时基类) |

几个和其他语言不太一样的地方:

- 数值字面量目前只支持十进制,没有十六进制/八进制/二进制、指数或数字分隔下划线。
  v0.22 支持 Kotlin 风格最小类型后缀：`1L` 是 `Long`，`1u` / `1U` 是无符号整数，
  `1uL` / `1UL` 是 `ULong`，`1.0f` / `1f` 是 `Float`。无约束整数按范围默认
  `Int`→`Long`，无后缀浮点数固定为 `Double`。
- 浮点字面量必须同时有整数和小数部分:`1.0` 合法,`.5` 和 `1.` 都不合法。
- `Any` 虽然存在,但在 v1 里几乎没法真正拿来"装任何东西"用:它不能作为 `Array`/`List`/`MutableList` 的元素类型,语言也不做运行时反射,所以更多时候它只是一个默认的泛型上界占位符,而不是 Kotlin/Java 里那种随手能用来做弱类型容器的顶层类型。

### 3.4 字符串与插值

```kotlin
val name = "Koven"
val greeting = "Hello, ${name}!"
```

字符串只支持单行、双引号写法,**没有三引号/多行/raw 字符串**。插值用 `${expr}`,支持任意嵌套表达式(包括调用、嵌套字符串);`$name` 这种不带花括号的简写形式在 v1 里不生效,`$` 后面不紧跟 `{` 时会被当成普通字符。

### 3.5 运算符与优先级

从高到低:

| 优先级 | 运算符 | 结合性 |
|---|---|---|
| 1 | `.` `?.` `()` `[]` `!!`、绑定的 `::name` | 左结合,可连续 |
| 2 | 前缀 `!` `-` `+` | 右结合 |
| 3 | `as` `as?` | 左结合 |
| 4 | `*` `/` `%` | 左结合 |
| 5 | `+` `-` | 左结合 |
| 6 | `..` `..<` | **不结合** |
| 7 | `to` | 左结合 |
| 8 | `?:` | 右结合 |
| 9 | `in` `!in` `is` `!is` | **不结合** |
| 10 | `<` `>` `<=` `>=` | **不结合** |
| 11 | `==` `!=` | **不结合** |
| 12 | `&&` | 左结合 |
| 13 | `\|\|` | 左结合 |
| 14 | `=` `+=` `-=` `*=` `/=` `%=` | 右结合 |

几个值得记住的点:

- 区间、成员关系(`in`/`is`)、比较、相等这四组运算符**各自不能连续出现两次**:`a < b <= c`、`a in b is T`、`a == b != c` 都是语法错误,想表达类似意思必须加括号,比如 `(a < b) && (b <= c)`。这是刻意的设计,能避免"链式比较"带来的隐蔽 bug。
- **v1 没有位运算符**(`&`、`|`、`^`、`~`、`<<`、`>>`),也没有 `++`/`--`。单字符 `&` 虽然已是合法固定符号,但在 v1 中只能用作调用实参的 `Inout` 标注,不是按位与运算符。
- 中缀调用在 v1 里收得很紧,**只有 `to` 这一个词**生效,不支持用户自定义 `infix fun`。

### 3.6 名称与作用域

Koven 名称大小写敏感。类型和类型参数位于类型命名空间，变量、常量、参数和函数位于值
命名空间；同一拼写可以分别作为一个类型名和一个值名。顶层声明与类型成员允许前向引用，
同一作用域的多个函数形成重载组。局部变量则从 initializer 完成后才可见：它不在自己的
initializer 或同一 block 更早的位置生效。

嵌套 block、lambda 和 loop 可以遮蔽外层名称，解析总是选择最近的已可见声明。同一作用域
的非函数重复名称是错误；找不到名称、以及在没有外层同名声明时提前使用稍后 local，分别
是不同诊断。`UpperCamelCase`、`lowerCamelCase` 等 Kotlin 风格是推荐写法，不改变名称身份，
也不会在 v1 中成为编译错误。package/import 的跨文件绑定仍是后续能力。

---

## 4. 函数

### 4.1 三种函数体形态

Koven 的具名函数有三种互斥的写法:

```kotlin
fun helper(): Unit { }                          // 无体:参数列表后直接结束(或跟返回类型)
fun log(message: String) { println(message) }   // block body,省略返回类型标注,固定为 Unit
fun add(a: Int, b: Int): Int = a + b             // 表达式体,必须显式写返回类型
```

规则很直接:**只有"无体"和"block body"这两种形态可以省略返回类型标注**,省略后精确固定为 `Unit`;**表达式体(`= expression`)必须显式写出返回类型**,哪怕它也是 `Unit`。这不是类型推导——编译器不会去看函数体反推返回类型,单纯是"写了 `=` 就必须自己写类型"的语法规则。

### 4.2 泛型参数

```kotlin
fun <T : Comparable<T>> max(own a: T, own b: T): T = if (a > b) a else b
```

v1 的泛型参数**最多只能有一个内联上界**,不支持多重上界、`where` 子句、默认类型实参,也不支持声明处型变(`in`/`out`)——所有泛型在 v1 里都是不变型(invariant),型变支持要等到 v2。

### 4.3 高阶函数与函数类型

```kotlin
val double: (Int) -> Int = { x -> x * 2 }

fun apply(f: (Int) -> Int, x: Int): Int = f(x)

fun makeAdder(n: Int): (Int) -> Int {
    return { x -> x + n }
}

val ref = ::globalFunction
val boundRef = obj::method
```

函数类型 `(ParamTypes) -> ReturnType` 是一等类型,会被单态化成"捕获环境 + 函数指针"的闭包结构体;不捕获任何变量的函数值会直接退化成裸函数指针,零成本。函数类型也能带 `move` 前缀(`move (Int) -> Unit`),表示只接受不含任何借用捕获的闭包——第 10 章讲并发的时候会用到。

v0.27 中，普通捕获 lambda 只共享借用外层值，可以在当前 callable 内保存和同步调用，但不能
返回、写入字段或交给取得所有权的参数。`move { ... }` 会复制 `Copyable` capture、移动 owned
MoveOnly capture，因而可以逃逸；Borrow/Inout 的 MoveOnly 值不能被它取得所有权。

函数类型的每个参数使用第 5 章要讲的三种契约之一(`Value`/`Borrow`/`Inout`)：无标记的
`(Int) -> Int` 是 `Borrow`，显式 `(borrow Int) -> Int` 是完全相同的可读性写法；
`(own Int) -> Int` 是取得实参所有权的 `Value` 契约，`(inout Int) -> Int` 是独占可变借用。
`(Int) -> Int` 与 `(borrow Int) -> Int` 是同一个函数类型，不能靠是否写出 `borrow` 区分重载。

### 4.4 重载中的 lambda：当前如何明确选择

先区分两件容易混在一起的事：lambda 的**函数类型**决定它接收什么参数、返回什么值；重载
选择决定这次调用指向哪一个具名函数。只要实参自身已经有确定的函数类型，普通重载选择就
可以直接使用它：

```kotlin
fun choose(action: () -> Int): Unit { }
fun choose(action: () -> String): Unit { }

val intAction: () -> Int = { 42 }
choose(intAction) // 明确选择第一个 overload
```

当前推荐把“显式函数类型的局部绑定”作为歧义时的逃生口。它不引入转换，也不会依赖编译器
猜测 lambda body。现行 v0.28 要求每个 overload 用自己的期望函数类型隔离检查
同一个 lambda：只有一个候选检查成功时直接选中，多个候选都成功时仍报告歧义。该语义已
启用，但 SPEC-0174 尚未实现，不能当作当前 frontend 已有能力。

Kotlin 调用中的 `a(Runnable { ... })` 不是通用的“给 lambda 标类型”语法，而是为单抽象方法接口
创建实例的 [SAM constructor/conversion](https://kotlinlang.org/docs/fun-interfaces.html#sam-conversions)。Koven v1 的 lambda 只产生函数类型，不产生匿名
class/interface 实现，也没有 Kotlin 式 SAM 转换，因此不接受 `Runnable { ... }`。Koven 也
不支持尾随 lambda，lambda 实参始终放在调用括号内。若未来确实需要单表达式的显式消歧，
更合适的方向是为所有函数类型设计统一的 type-ascription 语法，而不是只为接口引入一套
SAM 对象模型；这仍需要后续 guide 明确启用。

---

## 5. 所有权与借用:和 Kotlin 最大的不同

这是整门语言里最值得慢慢读的一章。如果你是 Kotlin 背景,前四章的内容基本可以"望文生义";从这一章开始,语法看着还是 Kotlin,但底层的执行模型已经完全是另一套东西了。

### 5.1 心智模型:没有 GC

Koven 没有垃圾回收器。每个值在任意时刻都有唯一的所有者;所有者离开作用域时,值被自动析构。这和 Rust 是同一套底层思路,但 Koven 把参数契约写在函数声明上：普通参数默认共享借用，取得所有权必须显式声明 `own`。调用点通常由编译器按 callee 契约自动判定;只有可变借用必须在实参前显式写 `&`。

`val` / `var` 只回答“这个绑定之后能不能重新赋值”，不回答“参数是 owning 还是 borrowed”。
普通局部变量、字段和容器拥有其中保存的值；只有进入 callable 边界时，才由参数声明上的
`Borrow` / `Value` / `Inout` 契约决定是临时借用、转交所有权还是独占修改：

```kotlin
class Session(val name: String)

val fixed = Session("primary") // fixed 不能重新赋值，但拥有 Session
var current = Session("backup") // current 可以重新赋值，也拥有 Session
```

声明侧显式不等于调用点也重复标记。向 `own` 参数传值时仍直接写 `consume(value)`：实参满足
`Copyable` 时复制，否则隐式移动；调用点写 `own value` 反而非法。`Borrow` 参数由 callee
签名确定，调用点可选写 `borrow` 强调只读借用；只有 `Inout` 契约必须写 `&`，让可能修改
调用者变量的副作用保持显眼。

### 5.2 三种参数契约

```kotlin
fun peek(x: Point): Unit { }           // 无标记 = Borrow
fun verbosePeek(borrow x: Point): Unit { } // 与上一行相同，只是显式强调
fun consume(own x: Point): Unit { }    // 显式 own = Value
fun mutate(inout x: Point): Unit { }
```

| 契约 | 标记 | 含义 |
|---|---|---|
| `Value` | 声明侧必须 `own`;调用点无标记 | callee 取得普通 owned value;实参满足 `Copyable` 则复制,否则移动 |
| `Borrow` | 声明侧无标记或显式 `borrow`;调用点无标记或显式 `borrow` | 调用期间共享借用;两种声明拼写是同一个契约 |
| `Inout` | 声明侧 `inout`;调用点必须 `&` | 调用期间独占可变借用,实参必须是可变的 place |

这里的 `own` 只标记普通具名 callable 参数和函数类型参数，不能用 `val` / `var` 代替。
主构造器的 `val` / `var` 字段和 enum case payload 是存储声明：存储本身已经明确表示构造器
取得并保存一个值，所以它们天然按 `Value` 交付，不额外书写 `own`。例如
`value class Point(val x: Int, val y: Int)` 与 `enum class Maybe<T> { Some(value: T), None }`
保持原语法；这不是把普通函数参数的默认模式改回 `Value`。

### 5.3 调用点标注速查表

"place" 指已经有身份的既有变量/字段/元素(比如一个局部变量、`obj.field`、`list[i]`);"temporary" 指这次调用现算出来、之前不存在绑定的值(比如字面量、另一个调用的返回值)。

| 调用点写法 | 对 `Value` | 对 `Borrow` | 对 `Inout` |
|---|---|---|---|
| 无标记 | 合法(复制或移动) | 合法(自动借用) | 不合法,缺 `&` |
| `borrow x` | 不合法 | 合法,与无标记写法语义相同 | 不合法 |
| `&x` | 不合法 | 不合法 | 仅当 `x` 是可变 place 时合法 |
| `own x` | 非法；`own` 只写在声明侧 | 非法 | 非法 |

```kotlin
val point = Point(1, 2)
val boxed: Box<Point> = Box(point)   // Point 可复制,point 之后仍可用
println(point.x)

value class Endpoint(val sender: Sender<Int>)
val endpoint = Endpoint(sender)
val owned: Box<Endpoint> = Box(endpoint) // Endpoint 不可复制,这里是移动
// 再用 endpoint 就是"移动后使用"错误
```

`Box<T>` 的构造参数契约在声明元数据中是 `own T`，但调用仍写 `Box(endpoint)`。因此最后一行
对 MoveOnly `Endpoint` 的移动既是隐式的，也是由 callee 的显式 `own` 契约静态决定的；
编译器不会根据函数名或函数体猜测是否消费实参。

把三种调用放在一起看更直观：

```kotlin
class Session(val name: String)

fun inspect(session: Session): Unit { }       // Borrow
fun close(own session: Session): Unit { }     // Value
fun replace(inout session: Session): Unit { } // Inout

var session = Session("primary")
inspect(session)          // 自动共享借用，调用结束后 session 仍可用
inspect(borrow session)   // 同一语义，只是把借用意图写出来
replace(&session)         // 独占可变借用，调用点必须显式写 &
close(session)            // Session 是 MoveOnly，这里隐式移动
// inspect(session)       // 错误：移动后使用
```

因此阅读调用时应先看 callee 签名，而不是寻找调用点的 `move` / `own` 标记。Koven 没有
`move value` 这种通用表达式，也不允许 `close(own session)`；MoveOnly 值是否移动由所有权
交付位置和 callee 的 `own` 契约静态决定。

### 5.4 `Copyable`:什么类型可以随手复制

`Copyable` 是编译器根据字段结构**自动、递归**推导出来的能力,不是用户手写的接口:

- 数值类型、`Boolean`、`Char`、`Unit` 天生满足 `Copyable`。
- `value class` 当且仅当**它的每一个字段类型都满足 `Copyable`** 时,才自动满足 `Copyable`(字段是 `val` 还是 `var` 不影响判定)。
- 泛型 `value class` 按实际类型实参递归计算,比如 `Pair<Int, Int>` 可复制,`Pair<Sender<Int>, Receiver<Int>>` 不可复制。
- 普通 `class`(堆分配、引用语义)永远不满足 `Copyable`。
- 可以把 `Copyable` 写成泛型上界使用,例如 `<T : Copyable>`;但 **v1 不提供用户手动实现、否定或覆盖这个能力的语法**——没法让一个字段全是数值类型的 `value class` 强制变成"不可复制、只能移动"。如果你熟悉 Rust,会发现这丢掉了一个常见模式:Rust 里可以故意不给一个字段都很简单的结构体 derive `Copy`,用类型系统强制"这个 ID 只能移动、不能被意外复制粘贴"。Koven v1 目前做不到这件事。

### 5.5 移动语义与字段访问

不满足 `Copyable` 的值,一旦交出所有权(赋值、返回、传给取得所有权的参数),原绑定就不能再用,再用会在 Phase 3(所有权检查阶段)报"移动后使用"错误。

“交出所有权”不只发生在函数调用中。用 MoveOnly 值初始化另一个 owning local、从函数返回
它，或把它装入字段/容器，都会移动：

```kotlin
class Session(val name: String)

val first = Session("primary")
val second = first       // 移动；没有也不需要写 move first
// println(first.name)   // 错误：first 已移动
println(second.name)
```

字段访问同样遵守这套规则:`aggregate.field` 是一个 place,可复制字段能直接读出复制值,但**不能通过普通字段读取把一个不可复制字段单独"抠"出来**——不可复制的聚合类型只能整体移动,或者用解构一次性、完整地消费掉所有分量,不存在"这个字段已经被移走、那个还没有"的中间状态。

### 5.6 借用只在需要的调用区间内有效

无标记参数和显式 `borrow` 参数都建立共享 loan；`inout` 建立独占 loan。共享 loan 之间可以
重叠，但同一 place 的共享 loan 与独占 loan、两个独占 loan 不能在同一有效区间重叠。Koven
v1 使用按调用边界定义的简化检查，不实现 Rust 的完整 NLL：

```kotlin
fun compare(left: Session, right: Session): Unit { }
fun update(inout session: Session): Unit { }

var session = Session("primary")
compare(session, session) // 两个共享借用可以共存
update(&session)          // 上一次调用已经结束，可以独占借用
```

借用不能被移动出参数绑定，也不能被普通 closure 带出定义它的 callable。拥有者仍负责唯一
析构；借用只提供临时访问能力。

### 5.7 lambda capture：这里的 `move` 到底移动什么

`move` 关键字在 v1 中只前缀 lambda literal 或函数类型。它控制的是**捕获环境**，不改变
lambda 参数本身的契约；lambda 参数仍由期望函数类型提供 `Borrow` / `Value` / `Inout`，源码
header 只写参数名，例如 `{ item -> use(item) }`。

普通 capturing lambda 共享借用它使用的外层值：

```kotlin
class Session(val name: String)

val session = Session("primary")
val show = { println(session.name) } // shared capture，不消费 session
show()
println(session.name)                // session 仍归外层所有
```

`show` 的 shared loan 持续到这个 closure 的 ASAP drop point；在示例中 `show()` 是最后一次
使用，loan 随后结束，所以外层可以继续访问 `session`。这种 borrowed closure 可以在当前
callable 内保存和同步调用，但不能被返回、写入字段，或交给 `own` 参数。需要 closure 逃逸时
显式写 `move`：

```kotlin
fun keep(own action: move () -> Unit): Unit { }

val retries = 3
val session = Session("primary")
val job: move () -> Unit = move {
    println(retries)     // Int 是 Copyable，capture 得到复制品
    println(session.name) // Session 是 MoveOnly，capture 取得所有权
}

println(retries) // 仍可用
// println(session.name) // 错误：创建 job 时 session 已移动进 closure
keep(job)        // job 自身也是 owned value，这里把它交给 keep
```

`move` closure 不能从 Borrow/Inout 绑定中取得 MoveOnly 值，也不能直接捕获 `this` 或写成
`this.field` 的隐式 capture；确实需要某个字段时，先在外层把所需值绑定到 local，再由 closure
按上述规则捕获。跨线程还要额外满足 `Transferable`，见第 10 章。

---

## 6. 用类型建模

> **状态说明**:本章的 class-family 契约已由 v0.20 正式确定，Parser、单文件名称解析、
> 名义/泛型/interface 类型检查与窄化接口委托检查已经实现；instance/delegation receiver 的
> 所有权仍等待后续 Spec。

### 6.1 `value class`:内联值类型

```kotlin
value class Point(val x: Int, val y: Int)
```

`value class` 描述的是**布局和身份**,不是"永远在栈上"或者"永远可复制"。作为局部变量时通常在栈帧里;作为字段、容器元素或其他聚合的一部分时,直接内联进所属的内存布局里,不会额外分配。它能不能复制,完全由 5.4 节的 `Copyable` 规则决定,和它是不是 `value class` 是两件独立的事。

内联布局必须是有限的:如果一个 `value class` 直接或间接地把自己作为字段(形成无限递归的内联结构),这是类型错误;想表达递归结构,必须经过 `class`、`Box`、动态容器这类"固定大小的间接层"来打断循环。

### 6.2 `class`:堆分配引用类型

```kotlin
class Node(var value: Int, var next: Node?)
```

普通 `class` 是堆分配的引用语义类型,遵循所有权/借用规则,本身永远不满足 `Copyable`,转交所有权即是移动。

### 6.3 `Box<T>`:把值类型放到堆上

```kotlin
val point = Point(1, 2)
val boxed: Box<Point> = Box(point)
```

`Box<T>` **只接受 `value class`**;`Box<Node>` 这种把已经是堆分配引用类型的 `class` 再装一层箱,是类型错误——`class` 本身已经在堆上了,不需要 `Box`。`Box<T>` 是独占所有权、不可复制的类型,装箱这个动作会取得传入值的所有权(可复制值给出的是复制品,不可复制值则发生移动)。

### 6.4 `enum class`:代数数据类型

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

Koven 的 `enum class` 沿用 Kotlin 的命名,但赋予了 Rust ADT(代数数据类型)式的能力:**每个变体可以携带不同的关联数据**,并且可以在 `enum class` 内部直接定义所有变体共享的方法,方法体里用 `when (this)` 对各变体分派,配合智能类型转换直接访问该变体专属的字段(如上面例子里 `Circle` 分支访问 `radius`)。

这一点值得专门提一句:**真实 Kotlin 的 `enum class` 做不到这个**——想要"变体带不同数据 + 共享方法"通常得改用更繁琐的 `sealed class` 搭配单独的类。这是 Koven 相对 Kotlin 一个比较实在的增强,如果你是 Kotlin 背景,值得专门留意,不要按 Kotlin 里"每个变体只是一个单例常量"的固有印象去理解它。

`when` 表达式对 `enum class` 做穷尽性检查:必须覆盖所有变体,除非写了 `else`。

### 6.5 `interface` 与默认方法

```kotlin
interface Shape {
    fun reset(): Unit
    fun area(): Double
    fun describe(): String = "a shape"   // 接口默认方法,保留
}
```

接口默认方法被保留了下来。因为 Koven **不支持类的实现继承**(没有 `class A : B` 这种基类继承,只有接口实现),`super` 关键字唯一的用途就变成了**给多个接口的同名默认方法做冲突消歧义**:

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

如果一个类实现的多个接口里存在同名默认方法,编译器会强制要求显式 `override` 并在方法体里用 `super<接口名>` 消歧义,否则报错。

关于异构集合:v1 的泛型和接口全部走单态化静态分发,`dyn`(trait object 动态分发)虽然是保留关键字,但对应语法被降级到了 v2。v1 阶段如果需要类似"一个集合里装不同实现"的场景,workaround 是用 `enum class` 把各个具体类型包起来——这也是 Rust 在没有 trait object 之前的常见做法。

### 6.6 `object` 与 `companion object`

```kotlin
object Config {
    const val VERSION: String = "1.0"

    fun describe(): String = "Koven ${VERSION}"
}

class Point(val x: Int, val y: Int) {
    companion object {
        fun origin(): Point = Point(0, 0)
    }
}

val p = Point.origin()
```

两者不是同一种运行时概念。具名 `object` 是有唯一值的名义 singleton,可有普通成员函数并在
函数里使用 `this`,但 v1 不允许运行时存储字段或惰性初始化。`companion object` 只是类型级
关联命名空间:不创建 `Point.Companion` 值,只允许 `const val` 和不使用 `this` 的关联函数。

接口也可以用 companion 暴露类型级常量:

```kotlin
interface Protocol {
    companion object {
        const val VERSION: Int = 1
    }
}

val version = Protocol.VERSION
```

该常量不被实现类继承或 override。v1 也不支持 Kotlin/Java 风格匿名内部类或 `object { ... }`
表达式；lambda 只适合单一函数类型。需要多方法实现时声明具名 class；若只是把一个接口的
方法机械转发给构造器字段,可使用窄化委托:

```kotlin
class TracingLogger(private val delegate: Logger) : Logger by delegate
```

委托目标必须是同一主构造器的不可变 `val` 字段,手写 `override` 优先；属性委托、任意表达式
委托、动态代理与运行时 `dyn` 都不在 v1 范围内。

### 6.7 解构声明

```kotlin
value class Pair<A, B>(val first: A, val second: B)

fun <T> channel(): Pair<Sender<T>, Receiver<T>> { ... }

val (sender, receiver) = channel<Int>()
```

解构沿用 Kotlin 风格的 `componentN()` 约定,右值只求值一次。对 `value class`,编译器用内建的结构化解构:如果源类型满足 `Copyable`,各分量拿到的是复制品,源值仍可用;如果不满足,解构会**一次性、原子地**消费掉整个源值并转移全部分量的所有权,之后源值不可用。**v1 只支持完整解构**——分量必须一次性全部绑定,不支持只解构一部分,也没有占位符跳过某个分量的语法。

---

## 7. 控制流

> **状态说明**：v0.18 已锁定本章控制流的 Phase 1 语法；v0.24 的条件类型、有限域 `when`
> 穷尽性与 smart cast 已由 SPEC-0021 实现，jump target 等其余静态语义仍在后续 Phase 处理。

### 7.1 `if`

```kotlin
fun divide(a: Int, b: Int): Int {
    if (b == 0) error("division by zero")
    return a / b
}
```

缺 `else` 的 `if` 只能作为完整语句使用；初始化器、赋值右侧、实参、`return` 值或其他需要
值的位置必须写 `else`。两条分支都存在时按分支推导结果类型；如果一条分支是 `Nothing`，
整体按另一条分支推导。

### 7.2 `when`

```kotlin
fun area(shape: Shape): Double = when (shape) {
    is Shape.Circle -> 3.14159 * shape.radius * shape.radius
    is Shape.Rectangle -> shape.w * shape.h
    else -> 0.0
}
```

`when` 搭配 `is` 分支对 `enum class` 做穷尽性检查,配合智能类型转换在分支内直接访问对应变体的字段。

### 7.3 循环

`while`、`for`、`loop` 都要求 `{ ... }` body。`for` 的 source 只求值一次，再经
`iterator()` / `hasNext()` / `next()` 推进；名称解析与协议类型检查留给 Phase 2/5。

### 7.4 lambda 内快速退出

lambda 是独立的返回边界，裸 `return` 退出最近的 lambda 或具名函数：

```kotlin
val parse = { text ->
    if (text.isEmpty()) return 0
    text.length
}
```

Koven v1 不支持 Kotlin 的 `return@label`、隐式调用名标签或 inline 非局部返回。嵌套 lambda
中的 `return` 只退出最内层 lambda，不会意外退出外层函数。

---

## 8. 集合类型

### 8.1 `Array` / `List` / `MutableList`

三种都是**运行时确定长度**的独占所有权容器,彼此是完全不同的名义类型,互相之间没有隐式转换:

| 类型 | 长度 | 可变性 |
|---|---|---|
| `Array<T>` | 构造后固定 | 元素可替换,不可增删 |
| `List<T>` | 运行时确定,构造后固定 | 只读 |
| `MutableList<T>` | 可增可减 | 元素可替换 |

三者都预声明一个只读的 `size: Int` 属性。内存表示上,每个实例是一个固定大小的 owner header(逻辑上持有一段连续缓冲区和当前 `size`,`MutableList` 还多存一个 `capacity`),元素紧挨着存在 header 之外的一段连续内存里。基础标量和 `value class` 直接内联存储在缓冲区里,**不会给每个元素单独分配对象头或自动装箱**;只有显式写 `List<Box<A>>` 时,元素才会变成各自独立堆分配的 `Box<A>`。

三种容器本身都不满足 `Copyable`(哪怕元素类型是),因为它们都独占一段缓冲区的所有权;移动容器只是转移这个所有权,原容器之后不可用。

### 8.2 索引是"位置",不是方法调用

```kotlin
val first = points[0]
array[i] = newValue
```

`[]` 在 v1 里是编译器内建的能力,**不是名为 `Indexable`/`MutableIndexable` 的用户接口**,用户没法把它实现在自己的类型上,也不能拿来当泛型上界——`.get(i)`/`.set(i, v)` 这种成员方法调用不会绕过这套规则,单纯会因为"成员不存在"报错。越界访问触发 `error()`(直接终止进程),安全访问需要自己显式检查 `i in 0..<container.size`。v1 也没有 `getOrNull`。

如果你想给自己的类型加下标语法——v1 做不到,这个能力与 place 返回和生命周期语法一起延后到 v2。

### 8.3 构造集合

```kotlin
val a = arrayOf(1, 2, 3)
val l = listOf(Point(x = 1, y = 2), Point(x = 3, y = 4))
val m = mutableListOf<Int>()

val runtime: Array<Point> = Array<Point>(3, { i -> Point(x = i, y = i) })
```

`arrayOf`/`listOf`/`mutableListOf` 按元素类型检查，并在 callable 契约中把每个元素位置声明为
`own T`；调用点仍不书写 `own`。已有变量和临时值都直接传入：元素满足 `Copyable` 时复制，
否则移动。运行时长度的构造函数 `Array<T>(size, initializer)` 会按索引从 `0` 到 `size - 1`
依次调用一次 `initializer`,负长度会直接终止进程。注意 Koven 不支持尾随 lambda 写法
(`f { ... }`),lambda 实参必须写在括号内。

### 8.4 `Map` —— 候选设计,尚未授权实施

`map[key]`、`getValue(key)`、`mutableMap[key] = value` 这些表面写法从 v0.5 就保留至今。v0.14 的设计决策文档第 18 节已给出 `Hashable`、借用查询和 `put`/`remove` 所有权契约的**候选设计**,但它仍需要独立评审并进入相应 Spec 才能成为实施契约。因此现阶段不应把候选中的类型、所有权或运行时细节当成已授权功能。

---

## 9. 错误处理

### 9.1 `error()`:和真实 Kotlin 最容易踩坑的地方

```kotlin
fun divide(a: Int, b: Int): Int {
    if (b == 0) error("division by zero")
    return a / b
}
```

`error()` 不是关键字,是标准库里一个普通的顶层函数:`fun error(message: String): Nothing`。
无标记的 `message` 按 v0.26 是 `Borrow`，`error()` 不消费调用者的字符串。名字和真实 Kotlin
的 `error()` 一样,**但语义完全不同**:真实 Kotlin 的 `error()` 会抛出可以被 `catch` 住的
`IllegalStateException`;Koven 的 `error()` 直接**终止进程(abort)**,不可捕获,也不做栈展开。

如果你是从 Kotlin 迁移过来,靠肌肉记忆写 `error(...)` 期待它能被上层 `catch` 住,在 Koven 里会直接让进程退出。这不是 bug,是刻意的设计,但确实是整门语言里名字相同、行为却南辕北辙的一个例子。

`Nothing` 会参与 bottom-type 推导:`if` 的一个分支类型是 `Nothing`(比如走到 `error(...)`)时,整体类型按另一分支推导。

### 9.2 `!!` 非空断言

```kotlin
val len = name!!.length
```

`e!!` 是纯语法糖,脱糖为 `e ?: error("Non-null assertion failed")`,类型从 `T?` 收窄为 `T`。因为最终还是走到 `error()`,它同样是不可恢复的 abort,不是抛异常。

### 9.3 `Result<T, E>` 与 `?`:错误是值,不是异常

Koven **没有异常,也没有 `throw`/`try`/`catch`/`finally`/`throws`**。可恢复失败是普通的 `Result<T, E>` 返回值；函数签名不需要第二个声明关键字。`expr?` 在 `Ok` 时产生内部值,在 `Err` 时把整个错误值从最近 callable 提前返回。lambda 是独立边界,所以 lambda 内的 `?` 只退出该 lambda。v1 要求错误类型 `E` 精确相同,错误转换必须显式完成。

---

## 10. 并发

### 10.1 `thread` 与 `move` 闭包

```kotlin
val handle = thread(move { println("running in new thread") })
handle.join()
```

跨线程 API(比如 `thread()`)的函数类型参数必须标注 `move (...) -> T`(而不是普通的 `(...) -> T`),表示只接受**不包含任何借用捕获**的闭包:调用点的闭包字面量必须显式带 `move` 前缀,编译器还会检查闭包体内没有捕获任何借用语义的外部变量。这对应 Rust `std::thread::spawn` 要求闭包满足 `'static` 的设计初衷——一个借用捕获的闭包如果被丢进新线程,原来的栈帧完全可能在子线程还在跑的时候就已经销毁,变成悬垂引用。

跨线程传递的值还必须满足 v1 的 `Transferable` 标记能力。它由编译器按字段结构递归推导,可作为泛型上界,但 v1 不允许用户手动实现或覆盖;`Rc<T>` 始终不满足 `Transferable`。`Shareable` 与跨线程共享原语一起延后到 v2。

### 10.2 `channel`

```kotlin
val (sender, receiver) = channel<Int>()
thread(move { sender.send(42) })
val value = receiver.receive()
```

### 10.3 现阶段的限制

v1/v2 阶段的并发模型就是"线程池 + 阻塞 IO",**没有协程、没有 `async`/`await`**。网络 IO 配套的做法是"每个连接一个线程"——这个模型能撑住的并发连接数是有上限的,不适合高并发网络服务场景。协程被排到了 v3,技术路线上倾向于 Rust 式的 `async`/`await` + `Future` 状态机(而不是 Kotlin 式的 `suspend` + CPS 变换,理由是后者和借用检查器的交互过于复杂)。按目前 Phase 1 单是解析器就被拆成十几个独立子任务的节奏来看,v3 大概率还需要相当长的时间。

---

## 11. 输入输出

```kotlin
val content = File.readText("path/to/file")
File.writeText("out.txt", "hello")

val reader = BufferedReader(File.open("data.csv"))
for (line in reader.lines()) { ... }
```

v1 只提供**同步阻塞 IO**(文件、网络),异步 IO 依赖协程,要等 v3。

---

## 12. v1 特性一览:支持与不支持

对 Kotlin/Rust 背景的开发者来说,这张表大概是最实用的一页。

| 特性 | v1 状态 |
|---|---|
| 自定义属性 getter/setter | 不支持(明确砍掉) |
| 扩展函数 | 不支持(明确砍掉) |
| 用户自定义索引运算符(`operator get`/`set`) | v1 不支持,延后到 v2 |
| 异常 / `try`-`catch` | 不支持;可恢复失败使用 `Result<T, E>`,不可恢复失败使用 `error()` abort |
| 错误传播语法糖(类似 Rust `?`) | v0.19 已定稿；传播 `Err` 到最近 callable |
| 位运算符(`&` `\|` `^` `~` `<<` `>>`) | 延后到 v2;v1 的 `&` 只是 `Inout` 调用点标注 |
| `++` / `--` | 不支持 |
| 十六进制/二进制/指数数字字面量、数字分隔下划线 | 不支持；类型后缀已在 v0.22 加入最小集合 |
| 多行 / 三引号 / raw 字符串 | 不支持 |
| 默认参数值 / `vararg` | 不支持 |
| 泛型型变(`in`/`out`) | 不支持,延后到 v2 |
| `dyn` 动态分发(trait object) | 关键字保留,语法延后到 v2 |
| 运行时反射 / RTTI | 不支持 |
| 协程 / `async`/`await` | 延后到 v3 |
| 自定义 allocator | 延后到 v2+ |
| `Map` / `MutableMap` 可实施契约 | 有候选设计,尚未独立评审或授权实施 |
| `object` / `companion object` 运行时存储状态或惰性初始化 | 延后到 v2；v1 的具名 `object` 可有普通函数,companion 是无对象身份的关联命名空间 |
| 匿名内部类 / `object { ... }` expression | v1 不支持；单回调用 lambda,多方法用具名 class 或窄化接口委托 |
| SAM conversion / `Runnable { ... }` | v1 不支持；lambda 只形成函数类型，歧义时先绑定到显式函数类型 local |
| 属性委托 / 任意 delegate expression | v1 不支持；只保留 `Interface by valField` 接口实现委托 |
| `Copyable` 用户手动实现/覆盖/opt-out | v1 不支持；v0.20 已明确不引入 `nocopy` |

---

## 13. 从 Kotlin / Rust 迁移过来的注意事项

**如果你熟悉 Kotlin:**

- 语法上大部分能直接搬过来(`fun`/`val`/`var`/`when`/可见性修饰符),但**内存模型是所有权/借用,不是 GC**——不能假设值可以随便传来传去、随便持有多份引用。
- `error()` 这个名字会骗人——它是 abort,不是可以 `catch` 的异常(见 9.1)。
- 自定义属性访问器、扩展函数**没有了**,这两个是 Kotlin 里用得非常多的语法糖,迁移代码时要留意。
- `Runnable { ... }` 属于 Kotlin 的 SAM conversion，不是普通 lambda 类型标注；Koven v1
  没有 SAM 转换，重载歧义时使用带显式函数类型的 local。
- `enum class` 反而比真实 Kotlin **更强**:可以直接给变体挂不同数据、共享方法(见 6.4),不需要绕道 `sealed class`。

**如果你熟悉 Rust:**

- 所有权模型的思路是一致的(单一所有者、移动语义、借用检查),但 Koven 声明侧无标记参数
  默认 `Borrow`，消费参数必须写 `own`；调用点仍由 callee 契约自动判定，向 `own` 参数交付
  MoveOnly place 时不写 marker 而直接移动。`borrow` 可选写出，只有 `Inout` 实参必须显式写 `&`。
- `Copyable` 是结构化自动推导、不能手动覆盖的,不像 Rust 的 `Copy` 需要显式 `derive` 且可以选择不加。
- `?` 只传播 `Result<T, E>`；它不是 throw,lambda 内也不会非局部退出外层函数。
- 没有 trait object(`dyn`),异构集合要用 `enum class` 包一层,这也是 Rust 早期没有 trait object 时的常见做法。

---

## 14. 关键字与运算符速查

文件可以用 Kotlin 风格的头部声明源码组织：

```kotlin
package com.example.app

import koven.io.println
import koven.collections.*
import koven.math.Vector as Vec
```

`package` 可省略；`import` 必须位于普通声明之前。Koven 不接受 Rust 的 `mod`、`use`、
`::` 或花括号分组导入，源码路径到 package 的映射及跨文件名称解析仍等待后续阶段实现。

**硬关键字(42 个,不可作标识符)**

```
声明相关:class companion const enum extern fun import interface object
         package typealias val value var vararg
控制流:  break continue else for if in is loop return when while
所有权:  borrow inout move own unsafe
可见性:  internal private public
其他:    as false null operator override super this true
```

**上下文软拼写(lexer 仍产出普通 identifier)**:`to`、`infix`(仅标准库内部使用)、`by`(仅 class supertype 委托位置)

**保留但当前版本未使用**(禁止用作标识符,给 v2/v3 预留):

```
async await suspend actor spawn sealed dyn where yield macro reify
```

**`error` 不是关键字**,是标准库函数,可以(但不建议)被用作标识符名。

`own` 在 v0.26 重新成为声明侧参数标记：`fun consume(own value: T)` 与 `(own T) -> R` 表示
`Value` 契约。它不是调用点标记，调用仍写 `consume(value)`；`consume(own value)` 非法。
主构造器 `val` / `var` 字段与 enum payload 由存储声明天然取得所有权，不额外写 `own`。

固定运算符与标点见第 3.5 节的优先级表;完整符号集是:

```
( ) [ ] { } , : @
. ?. ? ?: !! !
:: ->
* / % + -
.. ..<
< > <= >= == !=
& && ||
+= -= *= /= %= =
```

---

## 15. 路线图

Koven 编译器按下面的阶段推进,每个阶段完成后才会开始下一个:

| 阶段 | 内容 | 状态 |
|---|---|---|
| Phase 0 | 项目骨架(Cargo workspace、AST、诊断框架) | 已完成 |
| Phase 1 | 词法 + 语法分析 | 已完成(`&`、callable、表达式、声明、block、lambda、局部解构、完整文件、`package`/`import`、控制流、postfix `?`、class-family 与接口委托均已实现) |
| Phase 2 | 类型检查(不含所有权/借用) | 主线已完成；多文件名称解析仍有 guide 门禁 |
| Phase 3 | 所有权 / 借用检查 | 进行中（变量/结构移动、调用期 loan、ASAP drop facts 与顺序容器核心 element place 已实现） |
| Phase 4 | LLVM 代码生成 | 未开始 |
| Phase 5 | 最小标准库(用 Koven 自身编写) | 未开始 |
| Phase 6 | 工具链(包管理器、LSP、格式化工具) | 部分完成（TextMate 与 Tree-sitter grammar） |

Phase 6 之后:并发编译期检查完善、泛型型变、`dyn` 动态分发、`async`/`await` 等按需排期,不在 v1 范围内。编译器本身"自举"(用 Koven 重写 Koven 编译器)是长期目标,排在 v4 之后。

---

*本教程根据 Koven 语言设计规范整理,随设计推进持续更新;若教程内容与最新 guide 冲突,以 guide 为准。*
