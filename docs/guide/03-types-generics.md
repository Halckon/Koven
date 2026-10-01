# Koven v0.38：类型与泛型

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审类型引用、类型检查、泛型实例化与名义关系时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 基础类型与类型种类

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
`const val` 的求值与使用遵循
[常量阶段交接](15-conformance-and-staging.md#const-val-的阶段交接)。成员函数可以执行普通 v1
运行时代码并可使用 `this`，因为函数代码本身不是 singleton 存储状态。具名 `object` 可以实现
接口并参与单态化静态分发，但 v1 不把它擦除为裸 interface 或 `dyn`。运行时状态、惰性初始化与
共享可变 singleton 统一延后到 v2。

## 基础类型检查与局部推导

基础类型检查解析内建标量与函数类型，给局部
绑定和已支持表达式建立确定类型，检查具名函数的返回契约，并把需要 nominal member、泛型
实例化或 overload 选择的节点显式标为 deferred。`deferred` 是编译器阶段状态，不是用户可见
类型，也不得被当作类型正确；负责相邻语义的分析必须填充它，完整编译流水线不能携带 deferred 节点
进入所有权检查或 codegen。

### 语义环境与类型身份

- 类型检查入口接收 `SourceMap`、`ParsedFile`、名称解析规则 的 `NameResolution` 和不可变
  `TypeEnvironment`。环境按 `ExternalSymbolId` 绑定内建类型或单态外部 value / function
  签名；不得按 `Int`、`error` 等字符串硬编码语义，也不得隐式加载 prelude。
- `TypeId` 只在一次 typed 产物内有效。类型表至少封闭表示 builtin、nullable、function、
  integer-literal constraint、`Error` 与 `Deferred`；相同结构必须确定性规范化，不能依赖
  随机 hash 迭代。`Error` 用于抑制已诊断级联，`Deferred` 精确保留后续阶段责任。
- 基础类型检查 识别 `Byte`、`Short`、`Int`、`Long`、`UByte`、`UShort`、`UInt`、`ULong`、
  `Float`、`Double`、`Boolean`、`Char`、`String`、`Unit`、`Nothing` 与 `Any` 的环境身份。
  裸 builtin 不接受类型实参；nullable 后缀形成 `T?`。`Any` 在本阶段只作为后续泛型检查的
  顶层约束，不建立需要运行时擦除的普通 value 表示；把它直接用于 local、参数或返回值时
  暂记 deferred，由 名义类型检查 结合 nominal / 表示规则封闭。
- TypeRef 首段若已由名称阶段报 L0080，类型检查只产生 `Error`，不重复报错；限定名后续段、
  源码 nominal classifier、type parameter 和泛型实例统一 deferred 给 名义类型检查。已解析的
  builtin 携带任何类型实参时使用 L0082。

### 相容、字面量与运算符

- `Nothing` 是所有类型的 bottom type；`Nothing?` 只包含 `null`，可适配任意 nullable 类型。
  对任意非空 `T`，`T` 可适配 `T?`；除此之外 基础类型检查 不引入隐式子类型或数值 widening。
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
  callable 或 `Result` 信息，基础类型检查 只遍历 child 并将自身结果标为 deferred，不臆造类型。

### Expected Type、Local 与 Lambda

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
  body 共同纳入局部约束；基础类型检查 只允许已经确定的外层 expected type 单向流入 child。
  因此本阶段不会从后续使用反推 local，不会用 lambda body 选择 overload，也不会从带参
  lambda 的 body 猜参数类型；这些限制是确定性阶段边界，不是待实现的隐式行为。

### 具名函数、控制流与 `Nothing`

- 所有顶层和 member 函数先建立签名再检查任一 body，支持同文件递归。基础类型检查 只提交
  全部 TypeRef 均为本节已知类型且不需要 overload/member 选择的单态签名；泛型、nominal 或
  deferred TypeRef 的函数签名保留给 名义类型检查，不产生伪造的部分签名。
- `ImplicitUnitAbsent` 与 `ImplicitUnitBlock` 的返回类型精确为 `Unit`；显式 TypeRef 决定返回
  类型，绝不从 body 改写。表达式 body 以声明返回类型作为 expected type，不相容使用 L0084。
- `return`、`break`、`continue` 的表达式类型均为 `Nothing`。裸 `return` 只适配 `Unit`；带值
  `return e` 以最近 callable 的返回类型检查 `e`。lambda 建立独立 return 边界；文件 initializer
  等无 callable 上下文的 `return` 使用 L0086；裸 return 与非 `Unit` callable、带值 return 与
  `Unit` callable 的形态冲突使用 L0087，表达式值类型不匹配仍使用 L0084。break/continue
  target 留给 control-flow 语义检查；本节只赋 bottom type。
- 显式非 `Unit` block-body 函数若存在可到达的 body 末尾，使用 L0088，primary 为右花括号
  或 EOF 恢复点，label 指向返回 TypeRef。`if` 两分支或 control body 尾值在两侧已知时取最小
  join：同型保持原类型，任一侧 `Nothing` 取另一侧，`T` 与 `T?` 取 `T?`；否则 L0089。
  缺 `else` 的 statement-context `if` 固定 `Unit`。`when` 穷尽性和 smart cast 留给 when 类型规则；
  在此之前 `when` 的整体类型为 deferred，但其 child 仍接受局部检查。

### 诊断与阶段边界

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
同范围类型级联。本节封闭子集必须得到 known/error；每一种 deferred reason 都要精确对应
名义类型、callable、`when` 或 postfix `?` 等后续分析责任，不能用单一 `Unsupported` 垃圾桶
掩盖遗漏。

---

## 名义类型、泛型与 Interface

### 名义身份与泛型实例

- 每个源码 `value class` / `class` / `interface` / `enum class` / 具名 `object` 声明产生一个
  稳定名义身份。身份来自声明 symbol，不由名称字符串或结构相同推导；两个字段完全相同的
  class 仍是不同类型。具名 object 的类型和值共享声明来源，但仍保留不同的 type/value
  symbol。
- `TypeId` 增加 `Nominal(nominal_id, arguments)` 与 `TypeParameter(symbol_id)`。相同名义身份
  和相同有序实参规范化为同一类型；参数顺序是身份的一部分。类型参数以声明 symbol 区分，
  不按拼写合并。普通 class 是固定大小 owner/reference handle，value/enum 是内联名义值；
  精确布局和 `Copyable` 仍分别交给 Phase 4 与 条件 `Copyable` 与解构检查。
- 声明处类型参数按源码顺序编号，使用点必须提供精确数量的实参；v1 没有 raw type、默认
  类型实参、星投影、型变、隐式缺参或多余实参。泛型是 invariant：`Box<A>` 与 `Box<B>`
  只有在 `A == B` 时相同，不从接口关系推导协变/逆变。
- 无显式 bound 的类型参数等价于顶层约束 `Any`，但参数本身在单态化后有具体表示，不等于把
  值擦除成运行时 `Any`。显式单一 bound 只允许：一个可带类型实参的 interface，或编译器
  预声明的 `Copyable` / `Transferable` 能力；显式 `Any` 与省略 bound 等价。普通
  class/value/enum/object、nullable、函数
  类型和另一个类型参数不能作为 v1 上界；这种精确限制避免引入类继承、交集类型和递归
  F-bound。
- 名义类型检查 检查 interface bound；`Copyable` 满足性由 条件 `Copyable` 与解构检查 检查，`Transferable`
  满足性由 Phase 3 检查。它们在 名义类型检查 只保存为不同的能力谓词，不能伪装成普通
  interface，也不能因暂未求值而把整个名义类型降格为单一 deferred。
- `TypeEnvironment` 以外部 symbol identity 显式绑定 `Copyable` / `Transferable` 能力，方式
  与 builtin 类型绑定同样不依赖拼写；未绑定的同名外部 symbol 不是能力。源码中同名
  interface 仍只是普通 interface，不能冒充编译器能力，但可按普通 interface bound 使用。
- TypeRef 中的源码名义类型与类型参数在 名义类型检查 后必须成为 known/error；同文件限定类型
  仍不开放 nested type。多 segment TypeRef 继续只可能由后续 package/import 解析，因此在
  跨文件名称规则 前保持 `QualifiedType` deferred，不能把 `Outer.Inner` 猜成嵌套类型。

### Interface 位置与名义关系

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
  没有 `this`；其 body 内的 receiver 边界由现行
  [Companion Object 与关联成员](08-class-family-members.md#companion-object-与关联成员)规则定义。

### Callable 签名、实现与 Override

- 类型检查先为全部顶层函数和 classifier 收集 callable 签名，并为 classifier 额外收集类型
  参数、字段、enum 变体参数与 supertype，再检查图和 body。顶层/member 函数自己的类型
  参数使用独立作用域；签名中的 classifier 参数按其声明身份保存。替换是捕获规避、按 typed
  identity 进行的确定性结构替换，不能重解析源码文本。泛型函数 body 在其类型参数环境中
  检查；不因尚未实例化就把合法 `T` 当成 deferred，也不在签名收集阶段推导调用点类型实参。
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
  receiver/interface 归属与调用检查见
  [class-family 与成员规则](08-class-family-members.md)，但名义类型检查必须保存冲突
  来源，不能提前任选一个实现。

### 窄化接口委托

- `Interface by field` 只在 ordinary `class` 生效。target 必须精确指向同一主构造器的
  `val` 字段；`var`、body local、companion value、任意表达式或其他同名 symbol 均无效。
- 字段静态类型必须是具体 class/value/enum/object 或受足够 interface bound 约束的类型参数，
  且在替换 class 实参后满足被委托的完整 interface 实例。裸 interface 字段仍因没有运行时
  表示而非法；委托不隐式创建 `dyn`、代理或共享 owner。
- 委托生成的实现精确复制 interface callable 的类型参数、参数模式、参数类型和返回类型；
  不改变 `Result`、不插入 `?`，也不参与用户可见 overload。手写同键 `override` 优先。
  两个 delegate、delegate 与继承 default、或 delegate 与另一个未消歧来源提供同键实现时，
  必须显式 override；诊断保存全部源码有序来源。
- 名义类型检查只验证和记录静态转发计划；member call lowering、字段借用/移动与实际转发代码
  分别属于后续 call、Phase 3 和 Phase 4，不在此阶段生成隐藏 AST Item。

### Typed 产物、Deferred 与诊断

- typed 产物增加源码有序 `NominalId`、classifier descriptor、类型参数/上界、已替换
  interface closure、字段/变体/member 签名与委托计划。公开查询只返回 typed identity，
  内部索引使用有序 Vec/BTreeMap；图遍历必须有确定的灰/黑状态并报告第一条源码有序闭环。
  `TypeEnvironment::bind_capability` 只接受 type symbol 和封闭的 `Copyable`/`Transferable`
  identity，重复或 kind 不匹配继续作为内部环境构造错误 fail loud。
- 名义类型检查完成后，源码 nominal/type-parameter TypeRef、顶层/member 泛型函数签名和合法
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
| L0097 | 同一 callable binding scope（含顶层 package binding 与成员作用域）存在重复 callable 签名 | primary 为后出现的函数名；label 指向首个同键声明 |
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

名义图是 callable、`when`、Copyable/解构和 ownership/codegen 的共享稳定输入。调用表达式、
构造器与 member access 必须消费 typed identity，不得重解析源码名称。v0.37 保留
[Companion Object 与关联成员](08-class-family-members.md#companion-object-与关联成员)所述的关联
命名空间、无 `this` 和声明/Parser 规则；常量阶段的现行范围见
[一致性与实施边界](15-conformance-and-staging.md#const-val-的阶段交接)。

---

## TypeRef 与函数类型语法

```ebnf
type_ref          = qualified_type, [ "?" ]
                  | function_type ;

qualified_type    = Identifier, { ".", Identifier }, [ type_arguments ] ;
type_arguments    = "<", type_ref, { ",", type_ref }, ">" ;

function_type     = [ "move" ], "(",
                    [ function_type_parameter,
                      { ",", function_type_parameter } ],
                    ")", "->", type_ref ;
function_type_parameter = [ explicit_parameter_mode ], type_ref ;
explicit_parameter_mode = "own" | "borrow" | "inout" ;
```

限定类型路径只允许末段携带泛型实参；泛型实参可以递归包含任意本节 `type_ref`。限定类型
最多带一个末尾 `?`，因此 `T??` 非法。函数返回类型仍递归使用 `type_ref`，所以
`() -> T?` 唯一表示“返回 `T?` 的函数”，不表示可空函数值。现行语言不提供可空函数类型的
写法，也不新增类型分组语法来绕过该边界。v1 不支持 star projection、声明处或使用处型变；
不得把 `*`、`in T` 或 `out T` 塞入类型实参。在 `type_ref` 语法中，`move` 作为上下文关键字
仅当后继非 trivia token 是函数参数列表的 `(` 时充当前缀；其余位置遵守[普通 Identifier 通则](01-lexical.md#上下文关键字与软关键字)，例如 `move`、`move?`、`move<T>` 均可作为类型名称。
表达式位置的 `move { ... }` lambda 见[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)。

函数类型的每个参数都携带与具名函数 `value_parameter` 相同的三种**语义契约**：无标记或
显式 `borrow` 都表示 `Borrow`，显式 `own` 表示既有 `ParameterMode::Value`，显式 `inout`
表示 `Inout`。`own` 是 `Value` 的声明端拼写，不是要求调用点写 `own` 的独立第四契约。
`Value` 接收完整 owned value：结果满足 `Copyable` 时复制，
否则移动；`Borrow` 表示调用期间的共享只读借用，`Inout` 表示独占可变借用。

参数 mode 在函数类型身份中按上述语义先规范化：`(T) -> R` 与 `(borrow T) -> R` 是同一
函数类型，不能仅凭是否写出 `borrow` 形成 overload 或让 override 不匹配；`(own T) -> R`
与 `(inout T) -> R` 分别编码 `Value` 与 `Inout`，和 Borrow 函数类型不同。AST 仍保留显式
`borrow` 的真实 token / `Span`，以便 formatter 与诊断忠实反映源码，但该表面差异不进入
typed contract。

**调用点标注是否强制**是另一个独立问题，由[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)统一定义：`Value` 参数虽然
必须在声明端写 `own`，调用点仍不写 mode；向它传入 `MoveOnly` place 时无标记调用隐式
移动，传入 `Copyable` place 时交付 owned copy。`Borrow` 参数同样默认不写 mode，调用点
仍可选择写 `borrow` 强调；只有 `Inout` 参数强制要求调用点写符号 `&`（`&x`，不是关键字
`inout`）。调用点 `own x` 不属于语法。这条规则同时适用于具名函数
调用与函数类型值的调用，详见[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)“调用点自动化的设计说明”。

模式写在该参数 `type_ref` 之前，不能写在整个函数类型之前，也不能写在参数类型之后；
`move` 仍只约束闭包捕获，与任一参数模式正交。`move (inout T) -> Unit` 同时编码
move-capture 限制和可变借用参数。规范化后的参数契约、顺序、数量、参数类型、返回类型及
可选 `move` 都属于函数类型身份；v1 不提供忽略参数契约的隐式函数类型转换。函数类型不编码
具名函数的参数名称。Phase 1 对任何 call 都保留命名实参；Phase 2 解析 callee 后，只有直接
解析到具有稳定参数名的具名 callable 声明时才允许按名匹配，经普通函数值调用时必须拒绝
命名实参。模式兼容性与参数映射在 Phase 2 检查，并由该阶段标出类型层面的 place / temporary
类别；place 此刻能否移动、借用或独占访问以及实际效果在 Phase 3 检查。Phase 1 只保留真实
模式 token 及其范围。

v1 的 `type_arguments` 每一项都必须是 `type_ref`，不接受整数常量或其他值表达式。内建
`Array`、`List`、`MutableList` 精确只接受一个类型实参，因此 `Array<Int, Size>` 虽可先按
两个类型引用完成语法解析，Phase 2 仍必须因 arity 错误拒绝；`Array<Int, 4>` 则在 Phase 1
就因 `4` 不是 `type_ref` 而拒绝。该边界为[集合、索引与解构规则](12-collections-destructuring.md)所述未来 `Array<T, N>` 保留，
不能在 v1 中用普通类型参数或隐藏推导绕过。

表达式规则 实现该语法是因为 `as` / `as?` / `is` / `!is` 必须能消费类型引用。声明规则
复用同一 `type_ref`，并新增声明签名、泛型参数声明及调用点类型实参。

`call_type_arguments` 只能作为一次调用后缀的组成部分，不能单独形成 type-apply AST。它作用
于当时已经完成的整个 postfix callee，因此 `f<T>()`、`obj.f<T>()`、`(factory())<T>()` 和
`factory()<T>()` 都按同一规则成立；`f<T>` 不成立为带类型实参的值。

看到 postfix 位置的 `<` 时，parser 必须先做**无副作用试探**：从该 `<` 起若能按本节
`call_type_arguments` 完整、合法地读到匹配 `>`，且下一个**非 trivia token** 是 `(`，才提交
为 `typed_call_suffix`。这里不要求 `>` 与 `(` 字节相邻，因此 `f<T> /* comment */ ()` 合法，
而 `a < b > (c)` 也确定地按 typed call 解析；需要比较含义时必须用括号建立不同结构。
试探失败时，parser 不得留下 AST 节点、诊断或已消费状态，而应从原 `<` 按普通比较运算符
继续。因而 `f<T>`、`a < b > c` 及不完整的 `<...` 不会仅凭“看起来像泛型调用”获得专用诊断。
试探和提交均复用 `type_ref`；嵌套类型中的相邻 `>>` 继续分别关闭内外泛型，不视为 shift。
