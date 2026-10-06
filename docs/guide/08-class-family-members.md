# Koven v0.41：Class Family、成员与 Receiver

> **性质**：规范性语言规范 · **状态**：current（v0.41） · **读取时机**：实现或评审 class/value/interface/enum/object、成员与 receiver 时 · **唯一真源**：本页

本页是现行 Koven v0.41 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## Value Class

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
  drop 的类型均不得满足 `Copyable`。标准库新增任何 `Copyable` 类型时也必须满足这份契约，
  不能把 marker 当作绕过所有权检查的白名单。
- **使用规则由能力决定**：满足 `Copyable` 的值在赋值、返回或传给声明端 `own` 的
  `Value` 参数时可以隐式复制，
  原值仍可使用；不满足 `Copyable` 时，同样的位置转移所有权，原值随后不可使用。调用实参
  仍须遵守 callee 的 `Value` / `Borrow` / `Inout` 契约；`Inout` 在调用点仍强制要求显式
  标注，不因类型可复制而省略——**这是唯一强制要求调用点 marker 的契约，调用点写作符号
  `&`（例如 `mutate(&x)`），不是关键字 `inout`；关键字 `inout` 只出现在声明侧**，理由见
  [调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)的设计说明。`Value` 与 `Borrow` 均不写调用点 marker；
  `Borrow` 由 callee 契约自动确定，调用点 `borrow x` 不是合法语法。
  因此若声明是 `fun consume(own value: T)`，`consume(x)` 对 `Copyable` 的 `x` 交付一个
  owned copy，对不可复制的 `x` 则移动
  原值，调用点都不需要额外标注。
- 本文出现的“取得所有权的参数”只指声明端显式 `own` 所映射的 `Value` 参数。标准库 API
  必须按[调用、Lambda 与 Closure](07-calls-lambdas-closures.md)声明具体契约；不得按函数名
  或参数类型把无标记 Borrow 猜成 `Value` 或 `Inout`。
- class / value-class 主构造器的 `val` / `var` 字段与 enum payload 是天然-owned 存储形态，
  构造时按 `ParameterMode::Value` 交付但不重复写 `own`。这只是存储声明的专用语法，不允许
  普通 callable 用 `val` / `var` 代替 `own`，也不改变无标记 callable 参数的 Borrow 默认值。
- **字段访问不允许隐式部分移动**：`aggregate.field` 是一个 place。`Copyable` 字段可复制
  读取；字段 place 可以在调用实参中被借用（`Borrow` 契约，标注可选）或（字段可变时）被
  `Inout` 契约借用（标注必须）。v1 禁止用普通字段读取从聚合中移出不可复制字段：不可复制
  聚合只能整体移动，或按[局部结构化解构](11-copyability-layout-construction.md#局部结构化解构)
  一次性完整消费；不能产生需要追踪“哪些字段已经移走”的部分移动状态。
- 普通 `class` 是堆分配的引用语义值，本身不满足 `Copyable`，转交所有权时发生移动；它与
  非 `Copyable value class` 都受移动后使用检查约束，二者差异在布局而不在“一个复制、一个
  移动”的固定分类。

**`Box<T>` 的定位**：既然 `class` 本身已经是堆分配引用类型，v1 的 `Box<T>` **只允许 `T`
是具体 `value class` 或 `enum class`**；`Box<Node>` 这类把普通 `class` 再包一层的类型实例化必须产生类型错误。
`Box<T>` 是不可复制的独占所有权类型，用于把一个 `value class` 或 `enum class` 实例显式搬到堆上。装箱
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

`Box<T>` 内部值的访问遵循借用投影 `box.value`（获得只读借用）与显式消费拆箱 `box.unbox()`（消耗 Box 所有权并返还内部值），详见[内建 Box 身份与实参边界](11-copyability-layout-construction.md#内建-box-身份与实参边界)。

## 可见性与 Enum Class

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

- **`enum class` 内部可以直接定义共享方法**（如上面的 `area()`），方法体内通过
  `when (this)` 对各变体分派，依赖[`when` 穷尽性与 Smart Cast](06-blocks-control-flow.md#when-穷尽性与-smart-cast)
  在每个分支里访问该变体专属的字段（如 `radius`、`w`、`h`）。这与 Kotlin 的普通枚举不同，
  不得把每个变体仅理解为无 payload 的单例常量。
- `when` 表达式对 `enum class` 做穷尽性检查（强制覆盖所有变体，除非有 `else`）。

## Companion Object 与关联成员

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
  实例字段；`const val` 遵循[关联常量规则](05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)，不改变关联函数体；
- companion 不实现接口，不捕获 enclosing 类型参数。泛型关联函数必须自行声明类型参数，
  例如 `fun <T> identity(own value: T): T = value`；这里必须显式取得可能为 MoveOnly 的 `T`
  才能把它作为 owned 返回值交付。这里不使用 `Box<T>` 作为示例，因为
  [内建 `Box` 的实参边界](11-copyability-layout-construction.md#内建-box-身份与实参边界)
  要求实参是具体 `value class`，未约束的 `T` 不能证明这一点；
- `Type.member` 在名称解析后可以指向关联函数符号；它不分配 singleton、不生成初始化 guard，
  也没有退出时析构。常量选择与物化见
  [常量阶段交接](15-conformance-and-staging.md#const-val-的阶段交接)。

interface 的固定协议常量同样可以声明在 companion 中，例如
`interface Http { companion object { const val DEFAULT_PORT: Int = 80 } }`；
`Http.DEFAULT_PORT` 按关联常量规则选择与求值。该声明不被实现类型继承或 override。要求“每个实现类型各自
提供一个常量”的 associated-constant contract 是另一项未来能力，v1 不用相同语法悄悄引入。

## `super` 与默认方法

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
该委托只提供本页定义的窄化静态转发，不扩张 class-family 的其他规则。

Kotlin 风格的属性委托 `val/var property by expression` 不属于 v1：它需要自定义 getter /
setter、惰性初始化与属性元数据，超出[动态分发与匿名对象边界](15-conformance-and-staging.md#动态分发与匿名对象边界)。

## Instance Receiver 与静态分发

### 声明语法与规范化 Receiver

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
- `borrow`、`inout`、`own` 只按本节的 receiver modifier 位置合法；`nocopy` 及其他未列
  modifier 继续 unsupported。Parser 接受该语法不代表类型、所有权或 lowering 已具备对应事实。
- receiver mode 不参与 overload shape；缺省 Borrow 与显式 Borrow 不能形成重载。interface
  replacement、concrete `override`、default 冲突与 `super<I>` 选择必须精确比较规范化 receiver
  mode，如同既有显式参数 contract，不允许用返回类型或 mode 区分同 shape overload。

### 调用顺序、`this` 与所有权能力

- `receiver.member(arguments)` 只按 receiver 的静态名义类型、完整 interface closure 与已替换的
  owner/callable 类型实参选择 target；不产生 interface runtime value、vtable、RTTI 或代理。
  receiver expression 精确求值一次并先于显式 argument；选定 mode 的 loan/copy/move 在第一个
  argument 求值前生效，随后沿用[源码顺序 argument 交付与同步调用期 loan](10-ownership-borrowing-drop.md#所有者参数绑定与调用期-loan)。
- 调用点不新增 receiver marker。Borrow receiver 对 stable place 或 temporary 建 shared loan，
  temporary 延命到返回；Inout receiver 要求[可独占的 mutable place](10-ownership-borrowing-drop.md#place-重叠与冲突矩阵)且拒绝
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
- 裸 instance field/member 先按[作用域与可见时点](02-names-files-packages.md#作用域与可见时点)
  解析；没有局部/参数遮蔽时精确等价于同一
  receiver binding 上的 `this.name`。显式 `this.name` 可绕过局部遮蔽，但不得为一次调用构造
  第二个 receiver 求值。[Capture 规则](07-calls-lambdas-closures.md#capture-身份与普通闭包)
  禁止直接捕获 `this`/field；三种 receiver mode
  都不能让 `move` closure 直接捕获 `this`；Value `this` 可先显式整体移动到 local，再按普通
  local capture、`Transferable` 与 use-after-move 规则处理。
- `super<I>.method()` 仍是对当前 `this` 的静态 default 调用。当前 receiver capability 必须能
  满足目标 contract：Borrow 只能提供 Borrow，Inout 可 shared/exclusive reborrow，Value 可
  Borrow 或整体 Value 交付但不能提供 Inout。Value 交付后当前 `this` 不再可用。

### 窄化接口委托

v1 的 `Interface by valField` 只接受**全部可转发 requirement 都是 Borrow receiver** 的接口。
编译器生成的 forwarder 精确复制 interface member 的显式参数 mode/type、泛型参数、返回类型
与 effect，并依次 shared-borrow outer receiver、投影唯一 delegate field、以 Borrow receiver
静态调用 delegate 实现；receiver 与每个显式 argument 都只求值一次，不创建隐藏 AST、owner、
retain、proxy 或 `dyn`。

若经过手写 override/default 解析后仍需由 delegate 提供的有效 requirement 中存在 Inout 或
Value receiver，使用 `by` 形成 L0152；调用者必须写显式 `override`，自行决定如何取得可变
receiver 或消费 owner。“完整保持 receiver”不得被解释为从不可变 delegate 字段隐式部分移动、
替换或授予特殊
exclusive access。手写 override、default 与多 delegate 冲突仍沿用
[窄化接口委托](03-types-generics.md#窄化接口委托)的优先级和 L0100/L0105 级联抑制。

### 产物边界与非目标

Parser/AST 保存 receiver marker；类型检查发布规范化 receiver contract、`this`/member call 与
Borrow-only delegate forwarder facts；所有权检查据此建立 receiver loan/move/drop、字段冲突与
capture facts；lowering 使用既有 Value ABI 或
[Borrow/Inout pointer ABI](../adr/accepted/0016-interprocedural-borrow-abi.md)，不得在后端重新推导契约。

本节不定义 callable reference/绑定 method value、extension method、safe call、borrow-return、
动态 interface value、反射或 vtable，也不授权尚未启用的迭代设计。具体集合、IO、thread API
由各自规范定义，不能按 member 名称硬编码。

L0152 稳定表示“interface delegation cannot forward a non-Borrow receiver”；
primary 为 `by`/delegate target，label 指向首个不兼容 interface member。其余 receiver 失败复用
L0099/L0100（contract）、L0131–L0135（move/loan/mutable place），不得另造重叠错误类别。

---

## Class Family 声明

现行 v1 封闭 `value class` / `class` / `interface` / `enum class` / 具名 `object` 与
`companion object` 的表面语法。以下产生式中的普通 token 间允许 trivia；需要实际换行
或 `;` 的位置单独写成 separator：

```ebnf
visibility_modifier = "public" | "internal" | "private" ;
declaration_modifiers = [ visibility_modifier ] ;
method_modifiers = [ visibility_modifier ], [ "override" ] ;

simple_declaration = declaration_modifiers,
                     ( variable_declaration | constant_declaration
                     | function_declaration | classifier_declaration ) ;

classifier_declaration = value_class_declaration | class_declaration
                       | interface_declaration | enum_class_declaration
                       | object_declaration ;

value_class_declaration = "value", "class", Identifier,
                          [ type_parameter_list ], value_primary_constructor,
                          [ supertype_list ], [ class_body ] ;
class_declaration = "class", Identifier, [ type_parameter_list ],
                    [ class_primary_constructor ], [ supertype_list ],
                    [ class_body ] ;
interface_declaration = "interface", Identifier, [ type_parameter_list ],
                        [ supertype_list ], [ interface_body ] ;
enum_class_declaration = "enum", "class", Identifier,
                         [ type_parameter_list ], [ supertype_list ], enum_body ;
object_declaration = "object", Identifier, [ supertype_list ], [ object_body ] ;

value_primary_constructor = "(", class_field,
                            { ",", class_field }, ")" ;
class_primary_constructor = "(", [ class_field,
                            { ",", class_field } ], ")" ;
class_field = [ visibility_modifier ], ( "val" | "var" ), Identifier,
              ":", type_ref ;

supertype_list = ":", supertype_entry, { ",", supertype_entry } ;
supertype_entry = type_ref, [ delegation_clause ] ;
delegation_clause = soft_identifier_by, Identifier ;

class_body = "{", [ class_member,
             { member_separator, class_member } ], [ member_separator ], "}" ;
interface_body = "{", [ interface_member,
                 { member_separator, interface_member } ],
                 [ member_separator ], "}" ;
object_body = "{", [ object_member,
              { member_separator, object_member } ], [ member_separator ], "}" ;

class_member = method_modifiers, function_declaration
             | declaration_modifiers, companion_object ;
interface_member = [ "public" ], function_declaration
                 | declaration_modifiers, companion_object ;
object_member = method_modifiers, function_declaration
              | declaration_modifiers, constant_declaration ;

companion_object = "companion", "object", companion_body ;
companion_body = "{", [ companion_member,
                 { member_separator, companion_member } ],
                 [ member_separator ], "}" ;
companion_member = declaration_modifiers, constant_declaration
                 | declaration_modifiers, function_declaration ;

enum_body = "{", enum_variant, { ",", enum_variant },
            ( "}" | ";", enum_member,
              { member_separator, enum_member }, [ member_separator ], "}" ) ;
enum_variant = Identifier, [ "(", enum_variant_parameter,
               { ",", enum_variant_parameter }, ")" ] ;
enum_variant_parameter = Identifier, ":", type_ref ;
enum_member = method_modifiers, function_declaration
            | declaration_modifiers, companion_object ;

member_separator = trivia_with_line_break | trivia*, ";", trivia* ;
```

`type_parameter_list` 与 `type_ref` 复用[类型语法](03-types-generics.md#typeref-与函数类型语法)，
`function_declaration` 与 `constant_declaration` 复用[声明语法](05-declarations-callables.md#声明语法)；
不得为成员复制第二套 callable 参数、返回标注或 TypeRef AST。

### 声明头、构造器字段与修饰符

- `value class` 必须有一个非空主构造器字段列表；普通 `class` 可以省略构造器，等价于可调用
  的空构造器，也可以显式写空 `()`。两者的主构造器参数都必须以 `val` / `var` 声明存储字段，
  不接受未存储参数、callable 的 `own` / `borrow` / `inout` marker、默认值、`vararg` 或 trailing comma。
- 主构造器 `val` / `var` 直接建立存储字段，是专用的天然-owned 形态：构造调用向
  每个字段交付 `ParameterMode::Value`，但源码不重复写 `own val` / `own var`。复制、移动与
  字段可变性由 Phase 2/3 检查；这项例外不能扩张到普通 callable 参数。
  v1 不提供 `constructor` 关键字、二级构造器、`init` block 或 body 内新增存储字段。
- class-family 名称后的泛型参数复用单一内联上界规则。`object` 不携带类型参数；companion
  不能捕获 enclosing 类型参数，关联泛型函数必须自行声明类型参数。
- `public` / `internal` / `private` 可作为顶层声明、class-family、构造器字段、普通成员与
  companion 成员的单一 visibility；省略精确表示 `public`。Phase 1 保存显式 token 与缺省
  的差异，文件私有、package 可见和成员访问检查属于 Phase 2。
- 修饰符顺序固定为 visibility 后接可选 `override`。`override` 只接受在 class/value/enum/
  object 的实例成员函数上；interface 成员只允许省略 visibility 或显式 `public`。顶层
  `override`、重复/逆序 visibility、`extern`、`operator`、`unsafe`、`own`、`nocopy` 及其他
  未列修饰符均为 unsupported class-family form，不能由名称或 Kotlin 经验补齐。这里拒绝的
  `own` 是成员声明的修饰符位置；成员函数参数列表内的 `own parameter: T` 仍按
  [声明语法](05-declarations-callables.md#声明语法)合法。
- `class`、`value class`、`enum class` 与具名 `object` 的 supertype list 在 Phase 1 保存
  TypeRef 源码顺序；interface 的同形列表表示父接口。Phase 2 必须证明每项都是接口，Koven
  不支持 class implementation inheritance、构造器调用、`by` 之外的 delegation specifier
  或 Kotlin `Base(...)` 基类初始化。

### Body、Enum 变体与成员边界

- class/value class body 只含实例函数和至多一个 companion；interface 只含抽象/默认函数和
  至多一个 companion；enum 在变体区后只含共享实例函数和至多一个 companion；具名 object
  只含实例函数与 `const val`。所有 body 都拒绝普通 `val` / `var`、嵌套 class-family、局部
  类型、getter/setter、constructor/init 和匿名 object。
- 具名 object 同时引入一个名义类型和唯一值；它没有运行时字段或初始化状态，但实例函数可
  执行普通 v1 代码、使用 `this` 并实现接口。companion 与它不同：companion 是纯关联命名
  空间，不能作为值、没有 `this`、不实现接口，也不接受名称。
- companion 可出现在 class/value/enum/interface 中，body 只接受 `const val` 与关联函数。
  Phase 1 保存 `const val` initializer；Phase 2 的选择、求值与允许类型集合见关联常量规则，
  该 initializer 不作为普通运行时初始化。关联函数体不受此边界影响。interface companion 常量
  不被实现类型继承或 override。
- parser 复用既有三形态函数节点；Phase 2 要求 class/value/enum/object/companion 的函数必须
  有 expression body 或 block body，只有 interface 函数可以无体。不得把无体 concrete
  member 偷换成未定义的 `abstract` 机制。
- enum 必须至少有一个变体。变体使用 Kotlin 风格逗号分隔；有共享成员时，最后一个变体后
  必须有一个 `;`。无成员时不接受多余 `;`。无数据变体写 `Point`，不得写空 `Point()`；带
  数据变体参数必须非空且只写 `name: type_ref`，不写 `val` / `var` / marker / default。
  变体 payload 与构造器字段相同，是天然-owned 存储位置，调用时按 `ParameterMode::Value`
  交付而不在声明中重复写 `own`。变体和参数列表都不接受 trailing comma。
- 普通 body member 之间使用实际 LF/CRLF 或 `;`；同行多个 member 必须写 `;`，最后一个
  member 后允许一个可选 separator。该 separator 只属于 class-family body，不把 `;` 或换行
  提升为通用 block statement separator。enum 变体区的逗号与变体/成员之间的 `;` 不复用
  member separator，也不依赖空行猜测边界。
- named class-family 只允许作为完整文件顶层声明；block/lambda/control body 内仍按既有
  unsupported element 处理。`object : Interface { ... }`、`object { ... }` expression 与 Java
  匿名内部类都不进入 primary 产生式；单一 callable 行为使用函数类型/lambda，多方法或有
  字段实现使用具名 class。

### 接口委托边界

`by` 在 Lexer 中始终是普通 Identifier，只在 ordinary class 的 supertype entry 中按拼写
提交 `delegation_clause`。完整契约如下：

- target 必须是同一主构造器的不可变 `val` 字段名称；不接受 `var`、任意表达式或未存储参数；
- delegate 字段持有具体名义类型，Phase 2 必须证明该类型静态满足目标 interface；裸
  interface、`dyn`、反射代理或运行时查找不属于 v1；
- 自动转发完整保持原成员的显式参数模式、类型、返回类型与 `Result` 契约，不插入隐式 `?`
  或异常层；自动转发只接受 Borrow receiver；手写 override 优先，多来源同签名冲突必须显式
  override；
- delegate field 的移动、借用和析构与普通 owned field 相同，不获得隐藏共享或生命周期；
- `val/var property by expression` 属性委托明确不支持。

生产 Parser 只在 ordinary class supertype entry 中提交 `by Identifier`；其他上下文
或任意 delegate expression 仍以 unsupported class-family form 定向拒绝。

### AST、`Span`、诊断与恢复

- `Item` 增加 class-family payload，封闭区分 value/class/interface/enum/object；保存显式
  visibility/override、名称 marker、类型参数、主构造器字段、源码有序 supertype、enum
  variant 与 member ItemId。函数和常量 member 复用既有 Item payload，增加所属上下文与
  modifier 源码表示；companion 使用独立 member payload，不伪装成具名 object。
- constructor field、enum variant parameter 与 supertype 保存既有 TypeRefId；缺失名称仍用
  Present/Missing/Error 三态。所有列表保存真实 delimiter Span，缺失 token 用边界处空 Span，
  不为恢复伪造 identifier、关键字、`,`、`;` 或 brace。
- classifier Span 从最早显式 modifier（否则从 `value`/`class`/`interface`/`enum`/`object`）
  到 body closer；无 body 时止于 header 最后真实 token。member、field、variant、companion
  同样只覆盖自身最后实际消费位置；零宽 error child 不跨 trivia 扩张父范围。
- class-family 诊断分配 L0066–L0077：expected `class` keyword、expected classifier name、expected
  constructor field、expected constructor separator、expected supertype、expected member、
  expected member separator、expected enum variant、expected enum variant separator、
  expected enum member delimiter、invalid declaration modifier、unsupported class-family form。
  缺通用 `:` / TypeRef / `)` / `}` 继续复用既有稳定类别，不改变旧错误码含义。interface
  delegation 使用 L0078 `expected delegation target`，覆盖缺失或非法目标 Identifier。
- 文件 soft boundary 增量识别 class-family starter 及其单一 visibility 前缀；body recovery
  只在当前 brace/string/interpolation owner 回到 member baseline 后识别 member/variant starter。
  nested delimiter 内同形 token 不提升。恢复抵达外层 `}`、下一顶层声明或 EOF 时保留 owner
  closer/boundary；Lexer poison 不重复分类，每轮必须消费非空错误区或抵达 stop。
- enum 的 Identifier variant starter、普通 member starter、visibility 前缀与 contextual `by`
  使用有限状态试探；失败试探不分配 AST、不发诊断、不移动正式游标。一个含 n 个 lexeme、
  d 层 delimiter 的 class-family 声明解析与恢复必须为 `O(n)` 时间、`O(d)` owner 空间，不从
  每个 member/variant/type parameter 回扫整个声明。

### Receiver Grammar

现行 instance receiver grammar 如下：

```ebnf
method_receiver_mode = "borrow" | "inout" | "own" ;
method_modifiers = [ visibility_modifier ], [ "override" ],
                   [ method_receiver_mode ] ;

class_member = method_modifiers, function_declaration
             | deinit_declaration
             | declaration_modifiers, companion_object ;
deinit_declaration = "deinit", "(", ")", block ;
interface_member = [ "public" ], [ method_receiver_mode ],
                   function_declaration
                 | declaration_modifiers, companion_object ;
object_member = method_modifiers, function_declaration
              | declaration_modifiers, constant_declaration ;
enum_member = method_modifiers, function_declaration
            | declaration_modifiers, companion_object ;
```

- 固定顺序是 visibility、`override`、receiver mode、`fun`；缺省 receiver mode 与显式
  `borrow` 保留不同源码 Span，但后续统一规范化为 Borrow。重复、逆序、marker 后缺 `fun`
  继续使用 L0076/L0077 与既有 owner-aware member recovery，不新增 Parser 错误码。
- receiver marker 只在 class/value/interface/enum/object 的 instance-function slot 提交。顶层
  function、companion member、field、constant、classifier 与其他声明位置仍定向拒绝，不能把
  marker 解释为函数名或普通参数 mode。
- object 的 Borrow/Inout/Value 三种显式形态都先进入 AST；其无状态 singleton 只允许 Borrow
  的语言限制由类型检查验证，Parser 不从 classifier kind 提前做 Phase 2 判断。
- 本节不改变普通 callable 参数、调用点 argument marker、function type、extension receiver、
  callable reference 或 safe-call grammar；formatter 与 grammar bridge 必须保存原 token，不能
  把省略形式重写成显式 `borrow`。

## `deinit` 成员语法与资源析构契约

为了支持 RAII 确定性资源释放（如关闭文件、释放底层缓冲区、释放锁守卫等），普通 `class` 允许声明显式析构函数 `deinit`：

```kotlin
class MutexGuard(val mutex: Mutex) {
    deinit() {
        this.mutex.rawUnlock()
    }
}
```

1. **产生式与签名约束**：
   - 仅限普通 `class`（引用类型）声明 `deinit`，不允许在 `value class`、`enum class` 或 `interface` 中声明；
   - `deinit` 无参数，返回类型固定为 `Unit`；
   - `deinit` 不允许携带访问修饰符（`public`/`private`/`internal`），不可被外部代码作为普通方法显式调用（用户代码禁止写 `guard.deinit()`，必须由编译器在生命周期终止点自动插入析构调用）；
   - 每个 class 最多只能声明一个 `deinit` 成员。
   - body 中的 `this` 固定为当前实例的只读 `Borrow`；可读取字段、复制 `Copyable` 字段或
     建立同步 shared reborrow，但不能变异字段、建立 exclusive loan、移出 `MoveOnly` 字段，
     也不能把 `this` 再次作为 owned value 消费或显式析构自身。
   - 一次正常析构先完整执行用户 `deinit` body；body 正常返回后，再按字段声明顺序的逆序
     递归析构仍由该实例拥有的字段，最后释放实例存储。字段在 body 执行期间保持可读取，
     不得先析构字段再运行 body，也不得重复析构同一 owner。body 中的 `error()` 仍是 abort，
     不新增异常展开或保证 abort 后继续字段清理。
2. **能力与生命周期约束**：
   - 声明了 `deinit` 的 class 必然是 `MoveOnly` 类型，且不得满足 `Copyable`；
   - 声明了 `deinit` 的类型属于**资源类型**，在所有权分析中激活**词法作用域逆序析构（Lexical Scope Drop）**，保障其生命周期严格维持至作用域结束（见[所有权、借用与析构规则](10-ownership-borrowing-drop.md)）。
