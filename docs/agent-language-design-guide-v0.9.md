# AGENT 开发指导文档：Koven 语言设计规范 v0.9

> 本文档是给开发 Agent 的现行权威规范，取代
> [`agent-language-design-guide-v0.8.md`](./agent-language-design-guide-v0.8.md)，并以 v0.8
> 为完整基线；除下方 v0.9 变更记录明确修改的条款外，保留 v0.8 已确定语义。
> 本版只补齐 lambda、具名函数省略返回标注时的隐式 `Unit`、命名 / 模式实参与局部 `val`
> 解构的 Phase 1 语法契约，并重排尚未物化的路线图编号；不提前定义 `if` / `when` /
> `super` / loop、class-family、完整文件或跨声明恢复。
> 语法设计原则：
> **尽量贴近 Kotlin 命名与语法习惯**，内存模型为 Rust 式简化所有权/借用，编译器用
> Rust 实现，LLVM 后端。v0.8 及更早资料如与本文档冲突，以本文档为准。

> 版本说明：v0.8、v0.7、v0.6、v0.5、v0.4 与 v0.3 为保留的历史版本；旧版本不接收 v0.9
> 语义修改。
> 更早的 v0.2 guide、旧技术栈/
> 语言规格以及下文提到的审计报告尚未随当前仓库归档，仅作为历史来源，不参与现行规范
> 优先级。

> 文档记号：示意代码和签名中的 `{ ... }`、`(...)`、`error(...)` 等省略号表示未展开内容，
> 不是 Koven 源码 token；第三部分明确规定 v1 不支持 `...` 运算符。

## 本版（v0.9）变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 封闭 lambda literal 的上下文判定、参数、body 值、AST、Span 与 owner 恢复 | 🔴 语义与语法补全 |
| 2 | 允许具名函数在无体或 block body 形态省略返回标注，并把省略语义固定为 `Unit`；表达式体仍必须显式标注 | 🔴 语义与语法变更 |
| 3 | 把调用实参改为 typed argument，定义命名与 `own` / `inout` / `borrow` 的唯一组合顺序 | 🟡 AST 契约补全 |
| 4 | 只新增 block / lambda body 内局部 `val` 解构，排除 `var`、`const`、占位、嵌套 pattern 与类型标注 | 🟡 分阶段边界补全 |
| 5 | 将四类能力拆为 SPEC-0010 至 SPEC-0013，并整体顺延所有尚未物化的后续候选编号 | 🟡 路线图治理 |

## v0.8 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义独立 block 入口、block element 序列及不依赖分号或换行的结构边界 | 🔴 语义补全 |
| 2 | 封闭 SPEC-0009 的 block element 为局部 `val` / `var`、表达式与 nested block，不提前接受局部常量、局部函数、控制流或 class-family | 🔴 分阶段边界补全 |
| 3 | 明确 block 本身不产生值、没有尾表达式特例，表达式 element 结果按 `Unit` 语境丢弃 | 🔴 语义补全 |
| 4 | 固定具名函数无体、表达式体和 block body 三种互斥形态，并为 block body 定义 AST 引用与合成 `Span` | 🔴 语义补全 |
| 5 | 定义 block element 的局部恢复、owner delimiter、稳定诊断类别及单调前进约束 | 🟡 诊断边界补全 |
| 6 | 将 SPEC-0009 收窄为单一 block Goal；控制流和 class-family 延后至新 guide 与独立 Spec，SPEC-0011 只组合当时已实现节点 | 🟡 路线图重排 |

## v0.7 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 定义仅消费一个 `val` / `var` / `const val` / `fun` 的独立声明入口，不把换行或 trivia 当作声明终止符 | 🔴 语义补全 |
| 2 | 固定简单变量声明的名称、可选类型标注和必需初始化式；常量声明使用固定的 `const val` 前缀 | 🔴 语义补全 |
| 3 | 固定具名函数的泛型参数、普通参数、显式返回类型及可选表达式体，并把 block body 明确延后至 SPEC-0009 | 🔴 语义补全 |
| 4 | 泛型参数仅支持可选的单一内联上界；不接受 trailing comma、型变、默认类型实参、多上界或 `where` | 🔴 语义补全 |
| 5 | 定义 trivia 不敏感、先无副作用试探再提交的调用点类型实参判定及 postfix 归属 | 🔴 歧义消除 |
| 6 | 定义声明、参数、泛型参数、调用点类型实参和 typed call 的合成 `Span` | 🟡 AST 契约补全 |
| 7 | 补齐独立声明的最小诊断与局部恢复，继续把完整文件和跨声明同步留给 SPEC-0011 | 🟡 诊断边界补全 |
| 8 | 明确 SPEC-0008 不接受可见性及其他声明修饰符、类成员上下文、解构、模式实参或 block body | 🟡 分阶段边界补全 |
| 9 | 将依赖 block / statement 载体的 SPEC-0010 明确置于 SPEC-0009 之后，SPEC-0011 只负责最终组合与跨声明恢复 | 🟡 路线图门禁补全 |
| 10 | 统一所有声明级 consume-to-current-level 恢复的 delimiter、字符串、插值与 Lexer terminal-owner 规则，并固定单调单遍复杂度 | 🟡 恢复边界补全 |
| 11 | 为继续构造的声明名与参数名定义 present / missing / error 三态 marker，禁止用虚构名称 token 或范围表达恢复 | 🟡 AST 契约补全 |

## v0.6 历史变更记录

| # | 变更 | 类型 |
|---|---|---|
| 1 | 给出可执行的 primary、postfix、prefix、binary 与 assignment 表达式语法，并明确独立表达式入口和插值 stop token | 🔴 语义补全 |
| 2 | 将 v1 中缀调用封闭为 `to`，并分别定义区间、成员关系、比较、相等组的不结合约束 | 🔴 语义补全 |
| 3 | 定义限定路径、递归泛型、单层可空与函数类型组成的无歧义最小 `type_ref` 语法；可空函数类型暂不表达 | 🔴 语义补全 |
| 4 | 明确基本调用仅接受位置实参，索引恰好接受一个表达式，并把命名 / 模式实参与 use-site 类型实参延后 | 🟡 分阶段边界补全 |
| 5 | 定义无 trivia 相邻的不支持运算符组合、Lexer 错误 token 的 parser 消费规则及最小局部恢复类别 | 🟡 诊断与恢复边界补全 |
| 6 | 定义各表达式 AST 节点的合成 `Span` 规则 | 🟡 AST 契约补全 |
| 7 | 将 Phase 1 拆为 SPEC-0007 至 SPEC-0011 的依赖顺序，并同步 Lexer 已完成事实 | 🟡 路线图同步 |
| 8 | 封闭 `Array<T>`、`List<T>`、`MutableList<T>` 的长度可变性、唯一所有权和单一连续缓冲区表示 | 🔴 标准库契约补全 |
| 9 | 明确顺序容器元素按具体类型内联，禁止逐元素自动 `Box`、small-buffer optimization 和运行时双表示 | 🔴 布局契约补全 |
| 10 | 将顺序容器索引定义为内建元素 place，不建立无法表达该语义的普通 `Indexable.get/set`，移除 `getOrNull`，并补齐不可复制元素的读取、借用、替换和析构规则 | 🔴 所有权语义补全 |
| 11 | 区分显式装箱、ABI 间接传递与可选分配消除，且不把优化结果提升为语言保证 | 🟡 实现边界补全 |
| 12 | v1 固定使用单类型实参 `Array<T>`，为未来编译期长度类型 `Array<T, N>` 保留扩展边界 | 🟡 未来兼容边界 |
| 13 | 要求内联值具有目标可表示的有限布局，并用目标相关 warning 管理大栈帧和大型隐式复制 | 🟡 布局与诊断补全 |
| 14 | 不在尚未定义通用 key 等价关系的前提下臆造 `Map` 所有权语义；保留 v0.5 表面契约并将可实施契约延后到新 guide | 🟡 分阶段边界补全 |

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
- **使用规则由能力决定**：满足 `Copyable` 的值在赋值、返回或传给取得所有权的参数时可以
  隐式复制，原值仍可使用；不满足 `Copyable` 时，同样的位置转移所有权，原值随后不可使用。
  调用实参仍须遵守 `own` / `borrow` / `inout` 的专用语法，不因类型可复制而省略参数模式。
  因此 `consume(own x)` 对 `Copyable` 的 `x` 交付一个 owned copy，对不可复制的 `x` 则移动
  原值。
- 本文出现的“取得所有权的参数”只指本 guide 或后续标准库 Spec 已明确给出该契约的预声明
  操作；不能从普通 `name: type_ref` 声明或函数名推断。用户函数的 callee-side 模式、函数类型
  编码及调用匹配仍受第四部分第 9 节规定的后续 guide 门禁约束。
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

## 8. 顺序容器、内存表示与索引语义（Array / List / MutableList）

本节把语言契约与实现层次分开。`Array`、`List`、`MutableList` 及本节列出的核心构造、
`size` 和索引操作是编译器预声明的语言原语，不是 Phase 5 再声明的同名 `.ko` API。各阶段
职责唯一如下：

- Phase 1 只建立普通 call / member / index AST，不按名称硬编码容器语义；
- Phase 2 识别预声明符号、容器类型、元素类型、可变性与 element-place 类别，不判定移动、
  复制或借用有效期；
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
读取期间访问 header，不消费、复制或转移容器 owner；`size` 不能作为赋值或 `inout`
目标。`capacity` 是 `MutableList` 的实现不变量，v1 不因此自动暴露同名公共属性。

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
- `Array<T>(own size, borrow initializer)` 与 `List<T>(own size, borrow initializer)` 是已有 place
  实参的完整调用形式：`size` 的类型为 `Int`，initializer 的类型为 `(Int) -> T`；
  随后按 `0` 到 `size - 1` 的升序，以每个索引恰好调用一次。字面量或其他临时表达式按下文
  规则可不写参数模式；
- 空的 `MutableList<T>()` 配合取得元素所有权的 `add` 等 Phase 5 API，从未知数量的运行时
  数据源逐步构造动态容器。

这些拼写是编译器预声明、不可被用户重载的核心构造操作，不是依赖尚未定义声明侧
`vararg own T` 规则的普通函数。Parser 仍把它们解析为普通调用 AST；名称解析确认预声明符号后，
类型检查才建立专用的 typed construction 节点。因此 Phase 1 不按名称硬编码语义，也不提前
推导元素类型。

列表式构造按源码从左到右对每个元素表达式求值一次，并把完整结果直接初始化进缓冲区。临时
表达式的结果直接交付；从已有 place 交付元素必须显式写成调用实参模式，例如
`listOf(own endpoint)`。`Copyable` place 交付 owned copy，其他 place 发生移动。运行时长度
构造遵守同一规则：已有 place 必须写 `Array<T>(own size, borrow initializer)`；数字或闭包
字面量等临时表达式可以直接交付，编译器在本次调用期内建立临时值并借用。构造先对 `size`
求值一次并拒绝负值，再对 initializer 求值一次。随后以目标地址宽度受检计算
`size * stride(T)` 并在需要物理字节时取得完整缓冲区；确定性大小溢出或分配失败发生在任何
initializer 调用之前，但已经完成的 initializer 表达式求值副作用不回滚。分配成功后在整个
同步调用期间共享借用来自已有 place 的 initializer，并按索引升序调用；构造器不消费该
initializer，临时函数值在调用结束后按普通 ASAP 规则析构。每次返回的完整 `T` 直接交付对应
槽位。
`MutableList.add` 取得元素所有权，因此已有 place 必须写 `list.add(own element)`，临时值可直接
交付。所有形式都不得隐式 clone、retain 或装箱。

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
开放自定义索引能力必须等待 place 返回与生命周期语法由后续 guide 定义。

普通值读取以及调用实参中的 `own` / `borrow` / `inout` 索引先按源码顺序对 receiver 和
index 各求值一次，紧接着做边界检查，再形成绑定到 receiver 有效期的元素 place。
`container[i] = value` 是唯一例外，它不在 RHS 之前建立元素 place 或做边界检查，而是唯一遵循
下文的替换顺序：

- `T: Copyable` 时，普通值读取，或在取得所有权的调用实参中写
  `consume(own container[i])`，都会从 place 取得 owned copy；
- `T` 不满足 `Copyable` 时，普通 owned 读取和作为调用实参的 `own container[i]` 都必须产生
  所有权诊断，
  不得移出元素、留下未初始化洞，也不得自动改为 `Box<T>`；
- 在调用实参中，`use(borrow container[i])` 对三种顺序容器都合法；
  `mutate(inout container[i])` 只对 `Array<T>` 和 `MutableList<T>` 合法；
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
`Copyable` 元素，或把非 `Copyable` 元素 place 作为 `borrow` / `inout` 调用实参。未来若引入
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
| 内存分配器 | v1 直接用系统分配器（libc `malloc`/`free`）；顺序容器的单一表示与可选分配消除见第 8 节；自定义 allocator trait 列为 v2+，引入时不得静默改变既有容器语义 |
| 诊断错误码 | 参考 rustc 的 `E0382` 风格，从 v1 起给每类编译错误分配稳定错误码（如 `L0001`） |
| 编辑器语法高亮 | LSP 之外单独提供 TextMate/Tree-sitter 语法文件 |
| 自举计划 | 标准库从第一天用目标语言自身写；编译器本身"自举"列为长期目标（v4+） |
| 数组/集合字面量 | `arrayOf(1, 2, 3)`、`listOf(1, 2, 3)`；`mapOf("a" to 1)` 的表面拼写沿用 v0.5，但实施须等待第 8 节要求的新 guide 封闭 Map 契约 |
| 中缀函数（infix） | v1 表达式语法中的中缀调用集合精确封闭为软词 `to`；`infix` 自身只是普通标识符，后续仅可在标准库声明上下文解释，用户代码不开放自定义 `infix fun` |

## 11. 解构声明与 `componentN()` 约定

```kotlin
value class Pair<A, B>(val first: A, val second: B)

fun <T> channel(): Pair<Sender<T>, Receiver<T>> { ... }

val (sender, receiver) = channel<Int>()
```

该示例说明解构一旦被相应语法上下文接纳后的类型与所有权语义，不表示本版已经开放顶层
解构。v0.9 的 Phase 1 只由 SPEC-0013 接纳 block / lambda body 内的局部 `val` 解构；独立
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

> `get` / `set` 自 v0.2 起就不是软关键字，lexer 始终把它们作为普通
> `Identifier`。第一部分第 8 节的顺序容器索引是预声明原语，parser 不查找这两个
> 名称或任何用户 operator 声明来建立 `[]` AST；本版也不因这两个普通名称
> 开放自定义索引能力。

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

# 第四部分：表达式、类型引用、声明与 block 的可执行语法

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
                   | string_expression
                   | lambda_expression ;

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
和未来保留字不符合该终结符。Phase 1 的 primary **仅**包含上表项目。`lambda_expression` 见
第 9 节；`if`、`when` 和 `super` 继续延后。SPEC-0007 的历史子集不含 lambda，不能用当前
产生式反写其已完成验收事实。

独立入口必须在跳过首尾 trivia 后消费到唯一 EOF，不能以“已得到一个表达式”为由忽略后续
token。字符串插值递归调用同一表达式 parser，但以当前层的 `InterpolationEnd` 代替 EOF
作为 stop token；它不得吃掉结束插值的 `}`。嵌套的括号、索引、调用和字符串仍各自消费
自己的闭合 delimiter。

## 2. Postfix、调用、索引与函数引用

```ebnf
postfix_expression = primary_expression, { postfix_suffix } ;

postfix_suffix = ".", Identifier
               | "?.", Identifier
               | typed_call_suffix
               | call_suffix
               | index_suffix
               | "!!"
               | "::", Identifier ;

call_suffix       = "(", [ call_argument,
                            { ",", call_argument } ], ")" ;
typed_call_suffix = call_type_arguments, call_suffix ;
call_type_arguments = "<", type_ref, { ",", type_ref }, ">" ;
index_suffix      = "[", expression, "]" ;
```

所有 suffix 同属最高优先级，从左到右逐个包裹 receiver；例如 `a!!.b(c)[i]::ref!!` 是一条
确定的左结合链，`!!` 可重复。`.`、`?.` 和有 receiver 的 `::` 后必须是普通
`Identifier`，不能用硬关键字或 reserved-word token 冒充名称。无 receiver 的 `::name`
属于 primary，有 receiver 的 `value::name` 属于 postfix。两类引用在此阶段只产生 AST，名称
解析、可见性与 callable 类型均属后续阶段。`e!!` 的既有语义不变，仍脱糖为
`e ?: error("Non-null assertion failed")`。

`call_argument` 在本版中的完整产生式见第 9 节；basic call 与 typed call 复用同一实参语法，允许空
参数列表但不允许 trailing comma。SPEC-0007 完成时只支持位置实参并以 L0016 拒绝命名 / 模式
实参；实施 SPEC-0012 后，本节四种合法组合迁移为 typed argument，显式分组的
`f((a = b))` 仍是位置 assignment expression。

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
`() -> T?` 唯一表示“返回 `T?` 的函数”，不表示可空函数值。本版暂不提供可空函数类型的
写法，也不新增类型分组语法来绕过该边界。v1 不支持 star projection、声明处或使用处型变；
不得把 `*`、`in T` 或 `out T` 塞入类型实参。在 `type_ref` 语法中，`move` 只可作为函数
类型前缀；表达式位置的 `move { ... }` lambda 见本部分第 9 节。

v1 的 `type_arguments` 每一项都必须是 `type_ref`，不接受整数常量或其他值表达式。内建
`Array`、`List`、`MutableList` 精确只接受一个类型实参，因此 `Array<Int, Size>` 虽可先按
两个类型引用完成语法解析，Phase 2 仍必须因 arity 错误拒绝；`Array<Int, 4>` 则在 Phase 1
就因 `4` 不是 `type_ref` 而拒绝。该边界为第一部分第 8 节所述未来 `Array<T, N>` 保留，
不能在 v1 中用普通类型参数或隐藏推导绕过。

SPEC-0007 实现该语法是因为 `as` / `as?` / `is` / `!is` 必须能消费类型引用。SPEC-0008
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

`own` / `inout` / `borrow` 不属于通用 prefix 层级；它们只属于 SPEC-0012 的调用实参专用
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
| unsupported argument form | SPEC-0007 的历史类别；SPEC-0012 后生产 parser 不再产生 L0016，错误码目录因已发布而保留但不得复用或改变含义；仍非法的实参形态使用第 9 节专用类别 |

这里仅要求表达式内部的最小、确定性恢复，并保证每次错误都消费输入或抵达明确 stop token；
完整文件、跨声明同步和“单个语法错误后继续解析后续声明”的策略归 SPEC-0014。

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

## 7. SPEC-0008 独立声明入口

SPEC-0008 只增加一次解析一个简单声明的入口，并复用前述 `expression` 与 `type_ref`：

```ebnf
spec_0008_standalone_declaration
                   = trivia*, spec_0008_simple_declaration, trivia*, EOF ;

spec_0008_simple_declaration
                   = variable_declaration
                   | constant_declaration
                   | spec_0008_function_declaration ;

variable_declaration = ( "val" | "var" ), Identifier,
                       [ type_annotation ], "=", expression ;
constant_declaration = "const", "val", Identifier,
                       [ type_annotation ], "=", expression ;
type_annotation      = ":", type_ref ;

spec_0008_function_declaration
                    = "fun", [ type_parameter_list ], Identifier,
                      "(", [ value_parameter,
                              { ",", value_parameter } ], ")",
                      ":", type_ref, [ expression_body ] ;
expression_body      = "=", expression ;
value_parameter      = Identifier, ":", type_ref ;

type_parameter_list  = "<", type_parameter,
                       { ",", type_parameter }, ">" ;
type_parameter       = Identifier, [ ":", type_ref ] ;
```

三个 `spec_0008_` 名称只记录 SPEC-0008 已完成的独立入口、简单声明与函数声明子集及其历史
验收边界；它们不是 v0.9 中与第 8 节并列的第二套现行入口或函数语法。完整
`standalone_declaration`、`simple_declaration` 与 `function_declaration` 的唯一产生式以
第 8 节为准。SPEC-0008 的历史函数子集要求显式返回标注；SPEC-0011 只迁移其中无体和
block-body 分支的“缺失标注”边界，表达式体分支仍保持显式标注要求。

### 声明形态与分阶段边界

- 普通不可变、可变与常量声明分别以 `val`、`var`、固定的 `const val` 开始；不存在
  `const x = 1` 或 `const var x = 1`。三者都必须有普通 `Identifier` 名称和 `=` 初始化式，
  可以省略类型标注；省略时由 Phase 2 推导。`const val` 初始化式是否可在编译期求值也由
  Phase 2 检查，parser 不按表达式内容提前判定。
- `fun` 只声明具名函数。泛型参数表若存在，位于 `fun` 与函数名之间；参数必须是
  `name: type_ref`。SPEC-0008 已完成的历史子集要求函数返回类型显式写成 `: type_ref`；
  SPEC-0011 后，无体函数和 block-body 函数可以省略该标注，省略时精确表示 `Unit`，而
  `= expression` 形式仍必须显式标注。在 SPEC-0009 后，现行独立声明入口按第 8 节接受
  block body。无体函数是否允许由将其放入顶层、接口或其他容器的后续上下文检查。任何
  分支都不恢复函数级返回类型推导。
- 已出现的泛型参数表至少包含一个元素；函数参数列表可以是空列表。两类列表一旦包含元素，
  都不接受空项、缺失逗号或 trailing comma。函数参数不接受默认值、解构、`vararg`、`val` /
  `var`、`own` / `inout` / `borrow` 或其他模式；这些 token 不能被 parser 静默忽略。v1 的
  声明侧 `vararg` 契约仍未定义。
- 类型参数可无上界，也可用单个 `: type_ref` 指定一个内联上界。v1 不接受多上界、默认类型
  实参、`where`、star projection 或声明处 / 使用处型变；重复名称、上界合法性及默认上界
  `Any` 属于 Phase 2。变量与常量声明不接受类型参数表。
- 除 `const val` 这个不可拆分的固定声明前缀外，本入口不接受 `public` / `internal` /
  `private`、`extern`、`operator`、`override` 或软词 `infix` 等修饰符，也不定义其顺序。它不
  解析 extension receiver、匿名函数声明、class-family 成员上下文、控制流或声明自身的解构
  pattern。这里约束的是 declaration shape：SPEC-0010 后 initializer / expression body 可包含
  lambda expression，SPEC-0012 后其中的 call 可包含命名 / 模式实参；不得继续用本条把合法
  子表达式拒绝。局部 `val` 解构只由 SPEC-0013 的 statement dispatch 提交，不改写本独立入口。
- `{ ... }` block body 不属于 SPEC-0008。在该已完成的历史子集 / 实现中，
  `fun f(): Unit { ... }` 先得到无体函数声明，再因 `{` 成为尾随 token 而失败，不能把大括号
  内容保存为 opaque 文本或假装已解析。**SPEC-0009 首次定义并实现 block、block 内
  statement 序列以及函数 block body**；实施该 Spec 后，现行独立声明入口改按
  第 8 节接受 block body。SPEC-0014 不再发明另一套 block 语法，只组合此前已完成的结构并
  增加完整文件与跨声明恢复。
- 独立入口只以 EOF 结束。换行、注释和其他 trivia 不终止声明；`val a = 1\nval b = 2`
  不能作为一个独立声明成功。item / statement 的结构归属由第 8 节随 SPEC-0009 首次确定；
  SPEC-0014 只负责把已有节点组合为完整文件、定义声明分隔及跨声明同步。

### 声明 AST 与 `Span`

独立入口返回一个带 `SourceId` 的索引式 declaration root。声明中的 initializer / expression
body 引用现有 expression ID；显式类型标注、显式返回标注、参数类型、泛型上界和调用点
类型实参都引用现有 TypeRef ID，不得把源码片段或解析后的类型名称复制成另一套无 `Span`
字符串模型。省略返回标注必须用下文 `FunctionForm` 这类同时封闭返回来源与 body 的状态表示，
不能伪造 `Unit` TypeRef、冒充存在的 `:`，也不能丢失“显式 `: Unit`”与“省略标注”的源码
差异。

凡错误恢复后仍继续构造的声明、type parameter 或 value parameter，名称字段必须使用以下
三态 marker 或可证明等价的表示，而不是让调用方从任意 `Span` 猜测状态：

- `Present(Span)`：只覆盖实际消费的普通 `Identifier`；
- `Missing(Span)`：没有消费名称 token，保存当前 stop / 候选 token 起点的空 `Span`；
- `Error(Span)`：消费了不能作为名称的 poison 或其他错误 token，只覆盖实际消费的非空区域。

若恢复直接把整个 item 降为 `Item::Error` 而不再构造具体声明 payload，可以不另存名称
marker；但该 error item 仍只能覆盖实际消费区域。任何分支都不得伪造 identifier token、拼写
或非空范围。

| 节点 | 合成范围 |
|---|---|
| `val` / `var` / `const val` 声明 | 从首个引导关键字起到 initializer 终；恢复时到本声明最后实际消费位置 |
| 常量声明的 `val` marker | 正常时精确覆盖 `val` token；缺失时保存 missing / error marker，不为未消费的缺失 token 合成虚构 keyword 或非空 `Span` |
| `fun` 声明 | 从 `fun` 起；有表达式体或 block body 时到 body 终；显式无体时到返回 `type_ref` 终；隐式 `Unit` 无体时到真实 `)` 终；恢复时到最后实际消费位置 |
| 声明 / 参数名称 marker | present 精确覆盖 `Identifier`；missing 是 stop / 候选 token 起点的空范围；error 只覆盖实际消费区域 |
| type annotation | 从 `:` 起到 `type_ref` 终；若不单建节点，该范围仍由声明字段的 `Span` 保留 |
| value parameter | 从参数名起到参数 `type_ref` 终；恢复时到逗号、`)` 或本参数最后实际消费位置 |
| type parameter | 从参数名起；有上界时到 bound `type_ref` 终，否则到名称终 |
| type parameter list | 从 `<` 起到匹配 `>` 终；若在候选函数名处恢复缺失 `>`，则止于最后实际消费的类型参数，不纳入函数名或 `(` |
| call type arguments | 从 `<` 起到匹配 `>` 终；各实参继续使用自身 TypeRef `Span` |
| typed call | 从 callee 起点至调用 `)` 终；恢复时至该 suffix 最后实际消费位置 |

所有合成范围继续使用同一源中的 UTF-8 字节半开区间，不为缺失 token 伪造位置。失败的 typed
call 试探不是 AST 构造过程，因此不能分配残留节点或改变后续 ID 的确定性。常量声明缺
`val` 时，若恢复未消费 token，marker 使用当前候选名称 / stop 起点的空 `Span`；若为恢复
消费了 `var` 或其他错误 token，error marker 只覆盖实际消费区域。声明整体仍从真实 `const`
起到真实 initializer / 最后消费位置，绝不把缺失的 `val` 计入一个虚构范围。

### 最小诊断与局部恢复

SPEC-0008 在复用第 5 节既有类别外，至少区分下列稳定含义；对应 Spec 分配不与现有目录冲突
的 `L` 码、固定消息和精确主 `Span`：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected declaration | 独立入口首个非 trivia token 不是 `val`、`var`、`const` 或 `fun` 时，消费一个非法起始 token；EOF 处形成空 error root |
| expected `val` after `const` | `const` 后不是 `val` 时在下一个 token（EOF 时为空位置）报告；若下一个 token 是 `Identifier`，不消费并把它继续作为常量名，若是 `var` 则只消费该错误 marker 后继续期待名称，若是 `:`、`=` 或 EOF 则不越过该边界，其他 token 只消费一个后继续期待名称 |
| expected declaration name | 引导词（常量为完整 `const val`）或 `fun` 泛型参数表后缺普通名称时，在 `:`、`=`、`(` 或 EOF 前形成名称 error，不把这些边界冒充名称 |
| expected parameter name | 参数或泛型参数位置缺名称时，恢复到当前层 `:`、`,`、`>`、`)` 或 EOF，不跨嵌套 TypeRef delimiter |
| expected parameter type separator | value parameter 名称后缺 `:` 时在当前 token 报告（EOF 时为空位置）。若它可开始 `type_ref`，不消费并按插入 `:` 继续解析；若是 `=`，形成空 TypeRef error，再按 unsupported parameter default 的同步方式消费默认值区域但不追加第二条诊断；若是当前层 `,`、`)`、外层 `{`、EOF 或调用方 stop，则不消费并形成空 TypeRef error；其余情况至少消费一个 token，并继续消费到当前层 `,` / `)`、外层 `{`、EOF 或调用方 stop，形成覆盖实际消费区域的 TypeRef error。所有分支均抑制同根因的 expected type reference 与 list 诊断 |
| expected list element | declaration 的 type-parameter list 在 `>` 前没有元素时主 `Span` 取该 `>` 且不消费；type / value parameter 位置直接出现 leading / repeated `,` 时主 `Span` 覆盖并只消费该逗号；随后均从下一项或当前层闭合符继续 |
| expected list separator | 一个完整 type / value parameter 后，下一个 token 可开始同类参数但中间没有 `,` 时，在该 token 报告且不消费，把它继续作为下一项；其他非法 token 恢复到当前层 `,`、`>`、`)` 或 EOF |
| unsupported trailing comma | type / value parameter 的 `,` 后下一个非 trivia token 是当前层 `>` / `)` 时，仅消费并覆盖该逗号，闭合符仍由所属列表消费；空 value-parameter list `()` 本身合法，不属于此类 |
| expected generic closing delimiter | 至少完成一个 type parameter 后，若当前 token 是可作函数名的 `Identifier` 且下一非 trivia token 是 `(`，唯一解释为缺失 `>`：复用 expected closing delimiter 诊断，主 `Span` 是候选名称起点的空位置；不消费候选名称或 `(`，结束 type-parameter list 并让外层从该名称继续；该规则优先于“缺逗号”恢复 |
| unsupported parameter default | 已完整解析 `name: type_ref` 后出现 `=` 时，从 `=` 起按下方统一 owner-aware 扫描规则消费默认值错误区域，直到声明当前层 `,`、`)` 或 EOF 前停止并保留该 delimiter；嵌套 `()` / `[]` / `{}`、string 或 interpolation 内的逗号和右括号不是同步点。即使 `=` 后没有表达式也至少消费 `=`，且不追加 expected expression、expected list separator 或 trailing-token 诊断 |
| expected initializer | 简单值声明缺 `=` 时，若当前 token 可开始 `expression`，不消费并按插入 `=` 继续解析 initializer；若已到 EOF 或调用方声明 stop，则不消费并建立空 Expression error；其余情况至少消费一个 token，再同步消费到 EOF / 调用方声明 stop，建立只覆盖实际消费区域的 Expression error，且不为该区域追加 expected expression 或 unexpected trailing token。已有 `=` 但缺表达式时复用 expected expression error node |
| expected explicit return type | SPEC-0008 的历史边界：函数参数列表后缺 `:` 时，若当前 token 可开始 `type_ref`，不消费并按插入 `:` 继续解析；若是 `=`、`{`、EOF 或调用方 stop，则不消费并形成空 TypeRef error；其余情况至少消费一个 token，并同步到 `=`、`{`、EOF 或调用方 stop，形成覆盖实际消费区域的 TypeRef error。SPEC-0011 后只保留两类用途：`=` 前省略标注仍发本诊断并构造显式 Error TypeRef；明显 `type_ref` 起点前缺 `:` 仍按插入分隔符恢复。`{`、EOF 或调用方无体 stop 直接提交 `ImplicitUnit`，不再发本诊断；其他普通 token 先结束隐式无体函数，再由调用方 trailing / boundary 恢复拥有。已有 `:` 但缺类型继续复用 expected type reference |

#### 声明级 consume-to-current-level 的统一扫描规则

本节所有写成“继续消费”“恢复到”“同步到当前层 delimiter / stop”或“消费错误区域”的声明
恢复，只要可能跨过一个以上 raw lexeme，都必须使用同一所有权感知扫描规则。这包括缺失的
声明 / 参数名称、参数类型分隔符的兜底、列表空项与缺分隔符、unsupported parameter
default、initializer 缺 `=` 的兜底、返回类型分隔符兜底及独立声明尾随输入；只消费一个已
确定错误 token 的分支也可以调用它，但不得让每个诊断分支各写一套仅统计括号的扫描器。

- 每次调用必须记录进入时的 owner baseline，并由调用方给出它仍拥有的 **hard closing stop**
  集合：value-parameter list 至少给出 `)`，type-parameter list 给出 `>`；若恢复入口位于既有
  string / interpolation owner 内，则相应 `StringEnd` / `InterpolationEnd` 也属于 hard closing
  stop；具体调用方拥有的其他 closing delimiter 同样显式传入。EOF 始终是 hard stop。hard
  closing stop 的所有权来自调用上下文，不能靠扫描器看到某个 closer 后猜测。
- 扫描从当前 raw lexeme cursor 单调向前；trivia 被跳过作语法判断但仍只经过一次。维护本次
  恢复局部打开的 delimiter stack，`(` / `[` / `{` 分别压入自己的 closer。处理普通 closer
  时，若它匹配 stack 顶，先消费该 closer 并弹栈；若不匹配且当前没有比 owner baseline 更深
  的活动 owner，但它属于调用方给出的 hard closing stop，则即使 delimiter stack 非空也必须
  **保留该 closer 并立即停止**。此时所有未闭合的局部 delimiter 随当前恢复错误区结束，既不
  把 hard closer 纳入 error `Span`，也不越过它寻找内层 closer。例如局部 `[` 后遇参数表的
  `)`，以及局部 `(` 后遇 type-parameter list 的 `>`，都必须保留外层 closer。
- `,`、声明边界等非 closer 同步点属于 **soft stop**；只有 delimiter stack 没有局部 frame，
  且 owner stack 回到本次调用 baseline 时才保留并停止。嵌套 delimiter 或本次扫描打开的
  string / interpolation 内出现的同形 token 一律属于错误区域，不得冒充调用方边界。
- 另维护按真实嵌套顺序排列的 owner stack。`StringStart` 压入本次扫描打开的 string owner，
  匹配 `StringEnd` 时先消费并只关闭该栈顶 string；该 string 内的 `InterpolationStart` 压入
  interpolation owner，匹配 `InterpolationEnd` 时先消费并只关闭该栈顶 interpolation。插值
  中的嵌套字符串及其插值继续按相同规则压栈。只有本次扫描打开的 owner closer 才由扫描器
  消费；owner baseline 所属的 `StringEnd` / `InterpolationEnd` 是上条所述 hard closing stop，
  即使局部 delimiter 尚未闭合也须保留给 owner 调用方。`InterpolationEnd` 不能冒充普通 `}`。
- 每个 owner frame 记录进入时的 delimiter-stack 深度。正常 `StringEnd` /
  `InterpolationEnd` 或下述 Lexer terminal recovery 关闭该 owner 时，未匹配且由该 owner
  内部打开的 delimiter 随 owner 一同结束，不能泄漏到父 owner 或声明层；进入 owner 之前的
  delimiter 仍保留。
- Parser 必须从既有 lexeme / 诊断流预先关联 `L0004`–`L0006` 与其实际 opener owner：
  `L0004` 只在规范记录的未终止字符串边界关闭对应的最内层 string；`L0005` 只在其 EOF
  边界关闭对应的最内层 interpolation；`L0006` 只有在反斜杠后直接是 CR / LF / EOF、因而
  终止当前字符串的形态才关闭对应 string，普通可继续的非法转义不关闭 owner。terminal
  event 到达时其 owner 必须是当前栈顶；它只弹出自己拥有的 frame 并恢复到该 frame 的
  delimiter 基线，不得顺带弹出父 string / interpolation，也不得把 parent owner 内后续 token
  算入已终止 owner 的局部范围。事件若指向非栈顶 owner，属于 lexeme / recovery 关联不变量
  破坏，不能通过越过子 owner 来“修复”。
- 应在处理恢复边界后的下一个 lexeme 前应用 terminal event。若内层字符串在 LF 前由
  `L0004` 结束，扫描随后仍处于它的父 interpolation / string；因此后续逗号或 `)` 只有等到
  所有剩余 owner 正常或终止退出后才可能成为声明 stop。EOF 处已有 `L0004`–`L0006` 根因时
  继续沿用第 5 节的 closer 诊断抑制，不另造 parser 级联。
- 对一段含 `k` 个 lexeme 的恢复，每个 lexeme 至多检查和消费一次，每个 delimiter / owner
  只压栈、弹栈一次；terminal event 按 source offset 预索引并用单调 event cursor 读取。
  因而单段恢复必须是 `O(k)` 时间、`O(d)` 嵌套栈空间，不得从每个 token 重扫诊断、回看
  opener、重启 lexer/parser 或反复切片源码。lexeme / terminal-owner 关系若违反已验证不变量，
  属于 Parser 内部错误，不得降级为用户语法诊断。

上述 list 类别只作用于已提交解析的声明侧 `type_parameter_list` 与 `value_parameter` list。
失败的 `call_type_arguments` 仍须按第 3 节无副作用回退，不能借这些类别遗留专用 parser
诊断。参数、泛型参数和 TypeRef 的其他缺失闭合符继续复用 expected closing delimiter。
Lexer 已诊断的 invalid / reserved-word token 仍只消费并放 error node，不在同一 `Span` 重复
parser 诊断。每条恢复路径必须消费输入或抵达明确 delimiter / EOF；本入口不得把换行当同步
点，也不得扫描到下一声明关键字后假称恢复成功。SPEC-0014 只增加完整文件组合、声明分隔、
跨声明同步与级联抑制，不重新定义 SPEC-0008/0009 的节点内部恢复。

## 8. SPEC-0009 block、statement 序列与函数 block body

SPEC-0009 首次建立 statement AST 类别，但只交付 block 这个单一 Goal。它复用第 7 节的
简单变量声明和既有 expression，不借 block 载体顺带引入尚未定义的语法：

```ebnf
standalone_block          = trivia*, block, trivia*, EOF ;
standalone_declaration    = trivia*, simple_declaration, trivia*, EOF ;

simple_declaration        = variable_declaration
                          | constant_declaration
                          | function_declaration ;

block                     = "{", { block_element }, "}" ;
block_element             = local_variable_statement
                          | expression_statement
                          | nested_block_statement ;

local_variable_statement  = ordinary_local_variable_statement
                          | local_destructuring_statement ;
ordinary_local_variable_statement
                          = ( "val" | "var" ), Identifier,
                            [ type_annotation ], "=", expression ;
expression_statement      = expression ;
nested_block_statement    = block ;

function_declaration      = "fun", [ type_parameter_list ], Identifier,
                            "(", [ value_parameter,
                                    { ",", value_parameter } ], ")",
                            function_return_and_body ;
function_return_and_body  = ":", type_ref, function_body
                          | implicit_unit_body ;
function_body             = /* empty */
                          | "=", expression
                          | block ;
implicit_unit_body        = /* empty */
                          | block ;
```

产生式中的 `{ block_element }` 重复不是“任意相邻的两个 element 产生式都可自行切开”的
词法规则；每次重复必须满足下文唯一的最大 element / 显式结构 stop 谓词。尤其普通
expression-start 不是重复边界，所以 `{ x y }` 不能由该 EBNF 拆成两个 expression element。

这里的 `/* empty */` 是 EBNF 记号，不是要求源码包含注释。独立 block 入口一次只解析一个
block 并要求 EOF；函数 block body 由本节完整 `standalone_declaration` 入口解析，并复用
第 7 节已实现的声明子结构。`local_destructuring_statement` 的唯一产生式见第 9 节，并只在
SPEC-0013 后进入该聚合；SPEC-0009 的历史子集仅包含
`ordinary_local_variable_statement`。两种独立入口都不组合完整文件，也不以换行寻找下一
顶层声明。函数后缀使用互斥产生式，而不是两个独立 optional 字段，以免误接受省略返回标注
的表达式体。

### Block element、结构边界与值语义

- block element 的集合精确封闭为局部 `val` / `var`、既有 `expression` 和嵌套 block。
  `const val`、局部 `fun`、`return` / `break` / `continue`、`if` / `when` / `super`、`for` /
  `while` / `loop` 以及 `value class` / `class` / `interface` / `enum class` / `object` /
  `companion object` 都不属于本节。关键字已经存在不等于可以作为 block element；parser 必须
  准确拒绝，不能将未实现结构保存为 opaque token 或错误地当作 identifier expression。
- Koven v1 没有源码分号，换行和注释始终是 trivia；因此 element 不由分号、LF、CRLF 或
  注释终止。parser 先按适用产生式消费一个**最大合法 element**：局部声明的 initializer 和
  expression statement 都使用既有 Pratt expression。当前 owner 的 `}` 始终是 hard stop；
  `val`、`var` 和本节 unsupported element 引导关键字在最大 expression 已完整且不在任何
  expression owner / delimiter 内时是结构 stop，留给 block dispatch 开始下一局部声明或
  unsupported element。`{` 只有在左侧最大 expression 已完整、parser 不再等待 operand 时才是
  下一 nested block 的 soft stop；正在等待 primary 时，SPEC-0010 后必须把 `{` 解析为 lambda。
  例如 `{ val x = 1 val y = 2 }`
  与把两个声明写在多行的版本具有同一 AST；
  `{ x - y }` 因 `-` 能继续当前表达式而只有一项。连续两个普通 expression-start token 之间
  若没有上述显式结构 stop，则**不能**仅凭 trivia 或“第二个 token 也能开始表达式”推断为
  两项。普通 Identifier、字面量、`this`、`null`、`true`、`false`、`::` 或 prefix opener
  都不是 element stop；既有 expression 的 trailing-token 恢复消费余下非法区域，
  `{ x y }` 因而不是两条合法 expression statement。语法边界完全由 token 结构决定，增删
  trivia 不得改变 element 数量或归属。
- `{}` 是合法空 block；`{{}}` 是包含一个 nested block statement 的合法 block。block dispatch
  在 element 起点直接看到 `{` 时提交 nested block；只有 expression parser 正在等待 primary
  时，同一个 token 才按 SPEC-0010 提交 lambda。每个 `{`
  都建立独立 block owner，由对应层的 `}` 关闭。block 不是 expression primary；`f({})`、
  `val x = {}` 或把 block 用在二元运算符任一侧，不能在 SPEC-0009 中作为 block expression
  成功。lambda 字面量由 SPEC-0010 定义，不能用该未来语义反向解释本节大括号。
- block 是顺序执行容器，静态类型为 `Unit`，**不产生值，也没有尾表达式特例**。包括最后
  一项在内的 expression statement 结果均按 `Unit` 语境丢弃；`{ 1 }` 不是值为 `1` 的
  expression。函数 block body 的返回路径与显式 `return` 语义等待控制流 guide 和对应 Spec；
  Phase 1 parser 只保存结构，不因声明返回类型不是 `Unit` 而拒绝 block body。
- 每个 block 建立词法作用域。局部 `val` / `var` 从其声明完成后开始对同一 block 的后续
  element 及其嵌套 block 可见，不在自身 initializer 中可见，也不在声明前可见；嵌套 block
  中声明的名称离开该 block 后不可见。重名、shadowing、引用解析和 use-before-declaration
  的具体诊断属于 Phase 2；parser 必须保留 element 顺序，不能为了未来解析重排声明。

### 函数 body 三形态

每个具名函数声明恰好具有以下三种互斥 body 形态之一；返回标注是否可省略由形态共同决定：

1. **无体**：显式 `: type_ref` 后，或直接在参数列表 `)` 后，到独立入口 EOF / 调用方 stop；
   省略时返回类型固定为 `Unit`；
2. **表达式体**：显式 `: type_ref` 后为 `=` 与一个既有 expression；此形态禁止省略返回标注；
3. **block body**：显式 `: type_ref` 后，或直接在参数列表 `)` 后，为一个本节 `block`；省略时
   返回类型固定为 `Unit`。

`=` 已提交表达式体后不能再把随后的 `{` 改判为 block body；`{` 已提交 block body 后也不能
追加 `= expression`。`fun f() = expression` 必须产生 expected explicit return type，不能把
表达式结果静默丢弃到 `Unit`。无体函数在何种顶层或成员上下文合法、block body 是否满足
显式返回类型，以及所有路径是否返回，均是后续容器 / Phase 2 规则，Phase 1 不猜测。省略
返回标注只产生固定 `Unit`，不恢复函数级返回类型推导，也不从被覆盖声明继承返回类型。

### Statement AST、body 表示与 `Span`

SPEC-0009 把具体 AST 扩展为有 payload 的 statement table；所有 block element 都以有序
`StatementId` 保存，不把 element 混入 expression table，也不创建无 `Span` 的源码字符串：

- `Statement::Block { elements: Vec<StatementId> }` 表示独立、嵌套或函数体 block；
- `Statement::LocalVariable { declaration: ItemId }` 引用按第 7 节字段契约构造的 `val` /
  `var` item，禁止引用 `const val` 或 `fun` item；
- `Statement::Expression { expression: ExpressionId }` 引用既有 expression；
- `Statement::Error` 只覆盖本次实际消费的错误区域。

实现可采用可证明同样保持 typed ID、顺序与下述范围的等价枚举命名，但不能把 block 降为
`Vec<ExpressionId>`。函数 item 必须用一个合并的封闭 sum type 同时保存返回标注来源与 body，
至少等价于 `ImplicitUnitAbsent | ImplicitUnitBlock(StatementId) | Explicit {
colon_span: Span, type_ref: TypeRefId, body: FunctionBody }`；其中显式分支的 `FunctionBody` 才可为
`Absent | Expression { equals_span: Span, expression: ExpressionId } | Block(StatementId)`。
表达式体必须继续精确保存 SPEC-0008 已规定的真实 `=` token `Span`。不得把返回标注和 body
暴露为可独立构造的字段，不能制造 `ImplicitUnit + Expression`、“双 body”或“半个显式标注”
状态，也不得为隐式 `Unit` 伪造 TypeRef / `:` Span。独立 block 入口返回带 `SourceId` 的
statement root。

| 节点 | 合成范围 |
|---|---|
| 完整 block / `Statement::Block` | 从真实 `{` 起至匹配 `}` 终；缺 `}` 时到该 block 最后实际消费位置，空 block 仍覆盖真实 `{}` |
| local-variable statement | 与其引用的第 7 节变量声明范围完全相同 |
| expression statement | 与其引用的 expression 范围完全相同 |
| error statement | 只覆盖实际消费的错误区域；没有消费 token 时只能是在 owner closer / EOF 的空范围，且不得插入会使循环停滞的零宽 error element |
| 隐式 `Unit` 返回标注 | 不单独拥有虚构源码范围；无体 `fun` item 到真实 `)` 终，block-body item 继续到 block 终 |
| 显式返回标注 | 从真实（或缺分隔符恢复时的空）`:` Span 起到真实 / Error TypeRef 终；显式 `: Unit` 不折叠为隐式状态 |
| expression function body 的 `equals_span` | 精确覆盖真实 `=` token；恢复不得伪造或扩张该范围 |
| 有 block body 的 `fun` item | 从真实 `fun` 起至 body block 的真实 `}` 终；缺 `}` 时至 body 最后实际消费位置 |

所有范围继续是同一 `SourceId` 的 UTF-8 字节半开区间。嵌套 block 的范围不得吞入父 block 的
`}`；缺失 closer 不得用 EOF 之外的虚构字符扩展范围。

### Block 局部诊断、恢复与 owner 边界

SPEC-0009 复用 Lexer 诊断、第 5 节 expression 诊断、第 7 节变量声明诊断和 expected closing
delimiter，并至少增加下列稳定错误类别；具体 `L` 码和固定消息由 Spec 从注册表下一空位
分配：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected block | 独立 block 入口缺 `{` 时，诊断主 `Span` 只覆盖当前普通 token；恢复把从该 token 到 EOF 的全部剩余 lexeme（包括其间 trivia）消费为唯一 Error statement，其节点 `Span` 从当前 token 起到最后实际消费的非 trivia / invalid lexeme 终，不再追加同根因 trailing-token 诊断。EOF 的诊断与 Error root 均为空范围。若首 token 已有 Lexer invalid / reserved 根因，则不发本诊断，但仍把从该 poison token 到 EOF 的全部余量消费为同样范围的唯一 Error root。任何分支都不伪造 opener。函数参数列表后可选返回标注之后的 `{` 是 block body 的唯一提交信号；没有 `{` 时按无体函数或独立入口 trailing token 处理，不凭期待的形态追加本诊断 |
| expected block element | 当前非 trivia token 既不能开始允许的 block element，也没有更具体的 Lexer / unsupported 类别时，至少消费一个 token 形成 error statement，再从下一 element 候选或 owner `}` / EOF 继续 |
| unsupported block element | 当前层以本节明确延后的关键字或 `const val` 开始时，主 `Span` 覆盖该引导 token（`const val` 可覆盖固定前缀）；恢复至少消费引导部分且必须前进，不把后续内容伪装为已支持结构 |

恢复必须遵守以下 owner 契约：

- 当前 block 的 `}` 和 EOF 是 hard stop，只能由当前 block 正常闭合路径消费 `}`；expression、
  局部声明或 error-element 恢复都必须把它保留给 block owner。嵌套 block 调用建立子 owner，
  子 parser 正常消费自己的 `}` 后父 parser 才继续。同形 `}` 总是关闭当前最内层 block，
  parser 不能猜测它“本来属于父 block”；因此 `{{}` 中唯一的 `}` 关闭内层 block，外层在
  EOF 报缺 closer。子 block 的普通恢复不得越过其下一 block-level `}`，所有未被真实 `}`
  关闭的剩余 owner 最终各在 EOF / 异类调用方 hard stop 按由内到外的确定顺序结束。
- expression statement 和局部 initializer 调用既有 expression parser 时显式加入当前 `}`
  作为 hard stop。`val`、`var` 及本节列出的 unsupported element 引导关键字若出现在一个
  element 已完整结束之后，是下一 element 的结构边界。对 `{`，SPEC-0009 的历史入口在必需
  initializer 起点曾复用 expected expression 并把它保留给 block dispatch；SPEC-0010 生效后，
  正在等待 primary 的 initializer 起点必须按第 9 节提交 lambda，只有一个 element 已经完整结束
  且 parser 不再等待 operand 时，顶层 `{` 才作为下一 nested block 的结构边界。若在 owner
  `}` 前没有可消费 expression，同样复用 expected expression，不再追加同根因的 expected
  block element；局部 `val` / `var`
  的名称、类型、`=` 与 initializer 恢复继续复用第 7 节类别，但 stop 集合增加这些 element
  边界和当前 block 的 `}`。普通 Identifier 或字面量仍能开始 initializer，不能仅因它也可能
  开始下一 expression statement 就提前停止。
- Lexer invalid / reserved token 已有诊断时，parser 消费它并形成 error statement，不在相同
  `Span` 追加 block 诊断。string / interpolation 内的 `{`、`}` 和 element 起始 token 归
  lexical owner；恢复必须复用第 7 节预索引的 `L0004`–`L0006` terminal-owner 关系，不能把
  `InterpolationEnd` 当作 block closer，也不能在每个 block error 处重扫 Lexer 诊断。
- unsupported / expected element 的最小恢复以“消费确定的错误引导 token 或错误 token”
  为边界；没有分号或换行可供猜测整个未来结构的结束位置，因此不得按行跳过，也不得越过
  当前 owner `}`。遗留 token 随后按允许的最大合法 element 规则解析；可能产生的独立错误
  必须各有真实根因，不能为同一未消费 token 重复发诊断。
- 每次循环要么消费至少一个 raw lexeme，要么在 `}` / EOF 结束；诊断顺序按源码位置稳定。
  对一段 block 输入，每个 lexeme 在 block dispatch 中至多前进一次，嵌套 parser 只处理自己
  拥有的范围，整体保持 `O(n)` 时间和 `O(d)` owner / delimiter 栈空间，不从每个 element
  重启 lexer 或扫描到 block 起点。

缺 block `}` 复用 expected closing delimiter；Lexer 已诊断的未终止 owner 根因继续按第 5 节
抑制同义 closer 诊断。独立 block 后仍有 token 复用 unexpected trailing token。跨顶层声明、
跨成员和完整文件的同步仍属于 SPEC-0014，不能在本入口把下一个声明关键字当作隐式 EOF。

### Staging、验收与后续拆分

SPEC-0009 的最小验收必须包括：

- pass：空 / 单 element / 多 element block、只靠 token 结构相邻且任意插入 trivia 的 element、
  嵌套空与多层 block、局部 `val` / `var`、expression statement，以及具名函数的无体 / 表达式
  体 / block body 三形态；
- fail：缺 `{` / `}`、不完整局部声明、当前 owner `}` 前缺 initializer、`const val` / 局部
  `fun` / 控制流 / class-family 等 unsupported element、分号 Lexer error、block 用作 expression
  或 lambda；断言稳定错误码、UTF-8 字节 `Span`、error statement 及恢复后的 element 顺序；
- owner 与复杂度：嵌套 block、string / interpolation 中的大括号和 terminal Lexer error 不得
  提前关闭 block；长 element 序列与长错误序列用检查计数或等价白盒证据锁定单调 `O(n)`；
- regression：复跑 SPEC-0007 expression / TypeRef 和 SPEC-0008 declaration / typed-call 全部
  测试，继续锁定表达式体 `=` 的精确 `Span`，并运行 workspace 基线、同步 Architecture。
  验收不以完整文件、控制流、class-family、lambda、名称解析或类型正确性为成功条件。

后续按单一 Goal 拆分：SPEC-0010 至 SPEC-0013 分别实现第 9 节四项能力，SPEC-0014 再组合
届时已有节点并提供完整文件、声明边界、跨声明恢复与级联抑制；它不是 Phase 1 全部语法的
终点。control-flow 与 class-family 分别由 SPEC-0016、0017 的后续 guide 补齐。

---

## 9. SPEC-0010 至 SPEC-0013：lambda、隐式 `Unit`、typed call argument 与局部 `val` 解构

四项能力复用既有 expression、statement 与声明基础，但不是一个实现 Goal。SPEC-0010 只交付
lambda，SPEC-0011 只交付具名函数隐式 `Unit` 返回标注，SPEC-0012 只交付命名 / 模式实参，
SPEC-0013 只交付局部 `val` 解构；每项均须独立验收和提交，后项不得反向扩大前项。

lambda、typed argument 与解构三项结构 parser 都必须接收调用方的 hard stop 集合，并把自身
真实 closer 作为新增 owner。恢复按以下固定优先级处理边界，不能用“所有局部 owner 退出后
才看 hard stop”的笼统规则代替：

1. 当前 token 匹配局部 delimiter / lexical owner 栈顶 closer 时，先消费并 pop；
2. 否则遇 EOF 或与栈顶异形的调用方 hard closer（`)`、`]`、`}`、`InterpolationEnd` 等）时，
   无论局部 owner 是否闭合都立即停止并保留该 token；局部未闭合随当前 error region 结束，
   不得为了寻找自己的 closer 吞掉调用方边界，例如 `f({ [x )` 中 `)` 必须留给 call owner；
3. 同形 `}` 同时可关闭最内层 lambda / block owner 时，最内层 owner 优先消费。例如只有一个
   `}` 的嵌套 lambda / block 输入先关闭 lambda，外层 block 随后报告缺 closer；
4. 逗号、下一 argument / element 候选等 soft stop 只有在局部 delimiter 与 lexical owner 回到
   进入恢复时的 baseline 后才生效。

三项结构恢复统一复用第 7、8 节在 parser 构造时一次建立的 terminal-owner event index；遇到
Lexer `L0004`–`L0006` 的开始、恢复结束及抑制事件时按 owner 栈推进，不在每个 lambda、实参
或解构错误处重扫诊断。

### Lambda literal（SPEC-0010）

```ebnf
lambda_expression  = [ "move" ], "{", lambda_body, "}" ;
lambda_body        = [ lambda_header ], { lambda_element } ;
lambda_header      = [ lambda_parameter,
                       { ",", lambda_parameter } ], "->" ;
lambda_parameter   = Identifier ;
lambda_element     = local_variable_statement
                   | expression_statement
                   | nested_block_statement ;
```

Lambda 是 expression primary，可以继续接受 call、member、index 等 postfix。expression 已经
要求一个 primary 时，`{` 提交 lambda；block dispatch 在 element 起点直接看到 `{` 时仍提交
nested block。因而 `val f = { x }` 的 initializer 是零参数 lambda，body 尾值为 `x`；`{ x }`
作为外层 block 的直接 element 则是 nested block。需要在 statement 位置强制表达 lambda 时可写
`({ x })`。`move { ... }` 只能是 lambda。trailing lambda（`f { ... }`）不属于本节，调用仍须
写 `f({ ... })`。

第 8 节的 `{` soft element stop 因此是 parser-state-sensitive 的：已有完整左表达式且没有运算符
要求右 operand 时，顶层 `{` 留给下一 nested-block element；initializer 起点、prefix / binary
右 operand、grouped expression 或 call argument 等正在等待 primary 的位置则必须让 `{` 进入
lambda parser。`x { y }` 是 expression statement `x` 后接 nested block，`x + { y }` 的右侧则
是 lambda。该判定只依赖语法状态，不依赖 trivia、名称或推测类型。

`->` 前允许零个或多个逗号分隔的普通 Identifier；`{ -> e }` 是显式零参数形式。参数不接受
类型、默认值、`val` / `var`、模式、解构或 trailing comma。Header 只能从 `{` 后第一个非
trivia token 起严格匹配完整前缀 `[ Identifier { "," Identifier } ] "->"`；只有整个前缀
成功才提交。任一 token 不匹配就以零状态失败，并从 `{` 后按零参数 body 解析，不得继续搜索
后方任意顶层 `->`。因此 `{ value as () -> Int }` 中函数类型的箭头绝不会反向把 `value as ()`
误判为 lambda 参数，`{ x y -> z }` 也不是可恢复 header，而是带非法 body token 的零参数
lambda。试探 DFA 只跳过 trivia；遇到任何 delimiter / string opener 或其他不属于普通
Identifier、参数逗号、最终 `->` 的 token 时立即永久判为 no-header，不能进入 nested owner 后
继续搜索箭头。全流索引仍负责维护共享 delimiter / lexical-owner 栈。试探不分配 AST、不发
诊断、不改变 cursor。

Body 复用第 8 节的三种 element 和最大 element / 显式 stop 规则，但使用独立 lambda-body
payload，不能复用静态类型固定为 `Unit` 的 `Statement::Block`。若最后一个 element 是
expression statement，该 expression 是 lambda 的尾值；空 body 或最后一项为局部声明 / nested
block 时尾值为 `Unit`。这里不创造隐式 statement separator：普通 expression-start 仍不是
局部声明 initializer 的 stop，所以 `{ val x = 1 x }` 必须作为 initializer 尾随输入报错，不能
把 `x` 改判为第二项 tail expression。当前阶段若要使用普通 tail expression，它必须是 body
首项，或位于一个已有真实 `}` 结束的 nested block 之后；不承诺“任意局部声明序列 + tail
expression”。Phase 1 只保存该结构；参数类型、捕获、返回类型与 `move` 合法性由 Phase 2 / 3
检查。`return` 等控制流仍不属于本节。

lambda body 在最大 expression 已完整、没有子语法等待 token，且 delimiter / lexical owner
回到 body baseline 时，额外把顶层 `,` 与 `->` 作为 body-dispatch soft stop。它们只把控制权
交回当前 lambda body，不得泄漏成外围 call 的 argument separator。call、group、function type
或其他 nested owner 内的 `,` / `->` 不受影响，因此 `{ value as () -> Int }` 仍是单个完整
尾表达式。

AST 至少等价保存 `move_span: Option<Span>`、有序参数名称 Span、`arrow_span: Option<Span>`、
有序 `StatementId` body 和可判定的 tail expression。完整 lambda Span 从真实 `move`（若存在）
或 `{` 起至匹配 `}` 终；缺 `}` 时止于最后实际消费位置。由于 header 只在严格完整匹配后
提交，参数均为真实 Identifier，不存在 missing / error 参数 marker；header Span 从首参数
（零参数时从 `->`）至 `->` 终。body element 沿用第 8 节范围，不为缺失 token 伪造非空 Span。

`arrow_span == None` 精确表示没有 header，此时参数必须为空；参数非空时必须存在真实
`arrow_span`，而“参数为空且有真实 `arrow_span`”唯一表示 `{ -> ... }`。lambda body ID 必须
指向 `Statement::LambdaBody`；该 variant 不能直接成为 Block / LambdaBody 的 element，也不能
成为孤儿。strict probe 失败时参数为空、arrow 为 `None`，失败 token 只能进入 body 的
Expression / Error statement。

SPEC-0010 从 L0031 开始分配自身专用的 expected lambda body element 与 unsupported lambda
body form 类别，不复用声明列表的 L0024–L0026。前者只用于 `}` / 调用方 hard stop 之前真实
存在且不能开始任何合法 element 的普通 token。后者由 body dispatch 发出：无论 strict header
是否成功，当前 lambda-body baseline 的 `,` / `->`，以及 `return`、局部 `fun`、`const val`、
控制流或 class-family 等本阶段明确延后的 element introducer，都使用 unsupported lambda body
form；普通 token 每次精确消费一个，`const val` 可消费固定前缀，并形成同范围 Error statement。
`L0030 unsupported block element` 只用于普通 `Statement::Block`，不能在 LambdaBody 复用；
`move` 未后接 `{` 仍按既有 expected-expression 根因处理。`{}` 与 `{ -> }` 都是合法空 body，
绝不发 body 诊断。缺 `}` 复用通用 expected closing delimiter。body 恢复保留当前 lambda 的
`}`；nested block、调用、索引、字符串和插值各消费自己的 closer。同形 `}` 必须先关闭当前
最内层 lambda / block，不能越过未闭合 lambda 交给父 block；只有局部栈顶为 `)`、`]` 等异形
frame 时，调用方 `}` 才作为 hard closer 被保留。

Header 识别不得从每个 `{` 向前或向后独立扫描。parser 构造时必须在整个 lexeme / terminal
event 流上做一次 `O(n)` 预索引：共享 delimiter / lexical-owner 栈，并只让每个 `{` owner 的
小型 DFA 从其紧随的首个非 trivia token 开始识别上述严格前缀；首个不匹配 token 立即把该
owner 永久记为 no-header，后续箭头不再考虑。DFA 成功时记录参数 token 与 `->` raw index，
正式 parser 以 opener raw index 做 `O(1)` 查询。也可采用完全等价的共享 memo，但每个 raw
lexeme 在所有 header trial 中合计只能访问常数次。缺 lambda `}` 时，正式 parser 在最早的
调用方 hard stop 停止，即使预索引的词法范围延伸得更远也不得越界。每轮要么消费 lexeme，
要么在自身 `}`、调用方 hard stop 或 EOF
结束，整体 `O(n)`、owner 栈 `O(d)`。

### 具名函数隐式 `Unit` 返回标注（SPEC-0011）

具名 `fun` 的参数列表闭合后按以下互斥顺序提交：真实 `:` 提交 `FunctionForm::Explicit` 并
继续解析任意 body 形态；直接 `{` 提交 `FunctionForm::ImplicitUnitBlock`；直接到独立入口
EOF 或调用方无体 stop 提交 `FunctionForm::ImplicitUnitAbsent`；直接 `=` 则仍发既有 expected
explicit return type，构造显式 Error TypeRef 后保留并解析 expression body。若当前位置明显
可开始 `type_ref` 但缺 `:`，继续使用既有缺分隔符恢复并构造显式标注；已有 `:` 但缺类型仍
使用 expected type reference。参数表后的 Lexer invalid / reserved token 或 segmented
string / interpolation poison 若没有真实 `:`，不得被猜成显式返回类型：parser 先提交
`ImplicitUnitAbsent`，再由独立声明 / 未来容器的 trailing owner 消费该 poison；只保留既有
Lexer 根因，不在同一 `Span` 追加 expected explicit return type 或 trailing-token 诊断。
terminal Lexer 根因抵达 EOF 时同样提交 `ImplicitUnitAbsent`，不得派生 Parser 诊断。

函数 item 的返回来源与 body 必须是同一个封闭状态，至少等价于：

```text
FunctionForm::ImplicitUnitAbsent
FunctionForm::ImplicitUnitBlock(StatementId)
FunctionForm::Explicit {
    colon_span: Span,
    type_ref: TypeRefId,
    body: FunctionBody,
}
```

两个 implicit variant 都不拥有虚构 TypeRef 或 `:` Span；Phase 2 把它们解析为内建 `Unit`。
显式 `: Unit` 保持 `Explicit`，以便工具和诊断忠实反映源码。`FunctionBody` 只嵌在显式分支，
因而类型上不能构造 implicit expression body。省略标注的无体 item Span 到真实 `)` 终，block
body item Span 到 block 终。此 Spec 不改变 lambda、函数类型或构造器，不从 body 推导返回
类型，也不放宽 `fun f() = expression`。

### Typed call argument（SPEC-0012）

```ebnf
call_suffix       = "(", [ call_argument,
                            { ",", call_argument } ], ")" ;
call_argument     = [ named_argument_prefix ], [ argument_mode ], expression ;
named_argument_prefix = Identifier, "=" ;
argument_mode     = "own" | "inout" | "borrow" ;
```

唯一源码顺序是“可选名称、可选模式、表达式”：`f(e)`、`f(name = e)`、`f(own e)` 与
`f(name = own e)` 在 Phase 1 均可形成语法 AST。模式不是通用一元运算符；只有 call argument
入口可消费。模式后
直接出现顶层 `Identifier =` 是错误的逆序组合；显式分组的 `own (x = y)` 仍是以 assignment
expression 为 operand 的模式实参。空列表合法，trailing comma 继续非法。Parser 保留源码
顺序，但重复名称、位置实参与命名实参的混排规则、参数匹配以及 operand 是否为合法 place
分别留给 Phase 2 / 3。

本版只封闭调用点的 Phase 1 语法和 AST，**不把它误写成已经存在的 callee-side 参数模式
契约**。第 7 节的 `value_parameter` 仍不接受 `own` / `borrow` / `inout`，现有函数类型也不编码
这三种模式；因此“成功解析”不等于模式与被调函数匹配。进入 Phase 3 的 SPEC-0029 前，后续
guide 必须一次定义：用户函数如何声明或获得 owned / shared-borrow / mutable-borrow 参数契约、
函数类型是否编码该契约、临时值省略调用点模式的精确规则，以及预声明 API 是否允许例外。
在该门禁解除前，不得按函数名、参数类型或调用点拼写猜测 callee 契约，也不得声称三态调用
已通过所有权检查。由于该选择会反向影响 CallArgument 的长期含义，SPEC-0012 在门禁解除前
只能保持未物化候选，不得批准或实施；这不阻塞彼此独立的 SPEC-0010、0011。

`Expression::Call` 的 `arguments` 字段唯一改为 `Vec<CallArgument>`，不得再建
`CallArgumentId`、第五张 AST table 或同时保留旧 `Vec<ExpressionId>`。`CallArgument` 是内嵌
payload，至少保存完整 `span`、`name_span: Option<Span>`、`equals_span: Option<Span>`、可选
模式枚举及其真实 token Span、唯一 `value: ExpressionId`。完整实参从名称或模式（存在时）
否则 operand 起，到 operand 终；恢复时只到最后实际消费位置。call 与 typed call 的既有合成
Span 不变。错误 operand 只覆盖实际消费区域，或在 call / 调用方 hard stop 处为空范围。

SPEC-0012 从 SPEC-0010 末码之后至少分配 expected argument value、expected argument separator、
unsupported argument empty element、unsupported argument trailing comma、invalid argument mode
ordering 与 duplicate argument mode 六个专用稳定类别；不得复用声明列表 L0024–L0026，`)`
缺失只复用通用 expected closing delimiter。恢复分支精确如下：

| 分支 | 诊断、消费与 AST |
|---|---|
| 初始位置直接 `)` | 合法空列表；不消费 `)` 之外的 token，不创建 argument |
| 初始 / separator 后直接 `,` | unsupported argument empty element 覆盖该逗号；消费逗号并追加一个在逗号起点为空的 Error value argument，再继续下一项；若随后直接 `)`，不为同一空项追加第二条诊断 |
| 已有完整项后的 `,` 紧接 `)` | unsupported argument trailing comma 覆盖并消费逗号；追加一个在 `)` 起点为空的 Error value argument，保留 `)` 给 call owner；本分支优先于一般 separator 消费 |
| 已提交 `Identifier =` 或首个 mode 后遇顶层 `,` / `)` / 调用方 hard stop / EOF | expected argument value 取该边界的空 Span；不消费边界，追加保留已消费 name / mode marker 的 Error value argument；不再追加同根因 expected expression |
| 连续第二个及以后 mode | 每个多余 mode 各发 duplicate argument mode 并消费；AST 只保存首个 mode，argument Span 仍覆盖所有已消费错误 marker 后的最终 value |
| mode 后直接出现顶层 `Identifier =` | invalid argument mode ordering 主 Span 覆盖 `=`；消费该 Identifier 与 `=` 作为错误区域，不保存 name marker，再从其后解析唯一 value |
| 实参起点，或已提交 name / mode 后遇不能开始 expression 的其他普通 token | expected argument value 覆盖首个非法 token；owner-aware 消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，追加覆盖实际错误区的 Error value argument；Lexer poison 已有根因时只消费并建 Error value，不重复诊断 |
| 完整 value 后直接出现可开始下一 argument 的 token | expected argument separator 取该 token 起点空 Span且不消费；结束前一项并把该 token 作为下一项起点。该 soft boundary 只在 operand 已完整时生效，不能截断正在等待右 operand 的表达式 |
| 完整 value 后出现其他非法 token | expected argument separator 覆盖首个非法 token；按统一 owner 扫描消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，不把错误区附会为下一实参 |
| 缺所属 `)` | 在自身 nested owner 全部退出后的最早调用方 hard stop / EOF 发 expected closing delimiter 并停止；保留非 EOF 调用方 boundary |

所有扫描跟踪局部 `()` / `[]` / lambda `{}`、string / interpolation owner；顶层逗号只由 call
list 消费，所属 `)` 只由 call owner 消费，不能退化为按行同步。Lexer poison / terminal event
已表达根因时，只建立 Error value 并抑制同 Span 或同 owner closer 的 parser 级联。SPEC-0007
的 L0016 “unsupported argument form”历史负例在本 Spec 后迁移为正例；生产 parser 自此不再
产生 L0016，catalog 仅为已发布兼容性保留，不能复用其编号或含义。所有仍非法的实参形态均
落入上表六个专用类别或通用 expected closing delimiter。每个 raw lexeme 只由所属 argument
或 list owner 前进一次，单次 call 保持 `O(n)`。

### 局部 `val` 解构（SPEC-0013）

```ebnf
local_destructuring_statement
                       = "val", "(", destructuring_binding,
                         { ",", destructuring_binding }, ")",
                         "=", expression ;
destructuring_binding  = Identifier ;
```

仅当 block / lambda body dispatch 已消费 `val` 后看到 `(` 才提交解构。绑定列表至少一项，
只接受普通 Identifier；即使 Lexer 把源码拼写恰好为 `_` 的 token 归入 Identifier，解构 parser
也必须按原始 source slice 将它拒绝。`var`、`const val`、`_`、嵌套 pattern、binding 类型标注、
整个 pattern 的类型标注与 trailing comma 均不支持。重复名称和分量数量是否完整由 Phase 2
按源类型诊断。initializer 在 AST 中只出现一个 ExpressionId，从而锁定“右值只求值一次”；
复制式或消费式语义仍由 Phase 2 / 3 决定。

AST 唯一新增以下 statement variant；不得用等价 pattern table、`Item::Variable`、多个可选
variant 或第五张 AST table 代替：

```text
Statement::LocalDestructuring {
    val_span: Span,
    left_paren_span: Span,
    bindings: Vec<NameMarker>,
    right_paren_span: Option<Span>,
    equals_span: Option<Span>,
    initializer: ExpressionId,
}
```

两个 `Option<Span>` 只有真实 token 存在时为 `Some`；恢复不伪造 delimiter。statement 整体
Span 从 `val` 起到 initializer 或最后实际消费 token 终；pattern 的可观察范围从 `(` 起，正常
到 `)` 终，缺 closer 时到最后一个实际消费 binding / error token 终。每个 binding marker
遵守 present / missing / error 范围规则。

SPEC-0013 从前一 Spec 末码之后分配专用的 expected destructuring binding、expected
destructuring separator、unsupported destructuring form、unsupported destructuring context、
unsupported destructuring trailing comma、expected destructuring initializer separator 与
expected destructuring initializer；不得复用声明列表 L0024–L0026。恢复分支精确如下：

| 分支 | 诊断、消费与 AST |
|---|---|
| local `val (` | 唯一提交 `Statement::LocalDestructuring`，后续错误仍保留该 variant |
| block / lambda body 的 `var (` 或 `const val (` | unsupported destructuring form 分别覆盖真实 `var` 或 `const val` 前缀；用 owner-aware 扫描消费本错误 element，形成 `Statement::Error`，不得构造 LocalDestructuring |
| 独立声明入口或未来文件顶层的 `val (` / `var (` / `const val (` | unsupported destructuring context 主 Span 覆盖真实 `(`；当前独立入口 owner-aware 消费到 EOF 并形成 `Item::Error`，SPEC-0014 文件入口则保留下一顶层声明 boundary |
| `(` 后直接 `)` | expected destructuring binding 取 `)` 起点空 Span，追加 Missing marker 并保留 `)`；空 pattern 不成为合法形式 |
| 期待 binding 时直接 `,` | expected destructuring binding 覆盖并消费逗号，追加位于逗号起点的 Missing marker，再继续下一项 |
| 期待 binding 时遇 `=`、element boundary、调用方 hard stop 或 EOF | expected destructuring binding 取边界空 Span，追加 Missing marker且不消费边界；随后按缺 `)` 分支继续，但不重复 binding 诊断 |
| binding 源码拼写 `_` | unsupported destructuring form 覆盖并消费该 token，追加同范围 Error marker |
| nested `(`、binding 后 `:` 或其他 unsupported binding form | unsupported destructuring form 覆盖首个引导 token；跟踪局部 owner 消费到当前 pattern 顶层 `,` / `)` 前，追加覆盖实际错误区的 Error marker |
| 完整 binding 后直接出现下一普通 Identifier | expected destructuring separator 取该 token 起点空 Span且不消费；结束前一项并从同 token 解析下一 binding |
| 完整 binding 后直接出现 `=` | 只按缺 `)` 分支处理，不追加 expected destructuring separator；保留 `=` 给 initializer |
| 完整 binding 后出现其他非法 token | expected destructuring separator 覆盖首 token；owner-aware 消费到顶层 `,` / `)` / `=` 或调用方 hard stop 前 |
| 完整 binding 后的逗号紧接 `)` | unsupported destructuring trailing comma 覆盖并消费逗号，不追加 missing binding，保留 `)`；本分支优先于一般逗号消费。若逗号本身已作为空 binding 报错，随后 `)` 不再追加同根因诊断 |
| 缺 `)` 但当前为 `=` | expected closing delimiter 取 `=` 起点空 Span；`right_paren_span = None`，保留 `=` 并继续 initializer |
| 缺 `)` 且遇调用方 element boundary / hard stop / EOF | expected closing delimiter 取边界空 Span并保留非 EOF boundary；随后以同一根因构造缺失 initializer，不追加 separator 级联 |
| `)` 后缺 `=`，当前 token 可开始 expression（SPEC-0010 后包括 `{`） | expected destructuring initializer separator 取当前起点空 Span；`equals_span = None`，不消费并继续解析唯一 initializer |
| `)` 后缺 `=`，当前为 element boundary / hard stop / EOF | expected destructuring initializer separator 取边界空 Span；不消费边界，建立同位置空 Error initializer，并抑制 expected destructuring initializer |
| `)` 后缺 `=` 且为其他非法 token | expected destructuring initializer separator 覆盖首 token；owner-aware 消费错误区到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer，不再追加 initializer 诊断 |
| 已消费真实 `=` 后直接遇 element boundary / hard stop / EOF | expected destructuring initializer 取边界空 Span，不消费边界，建立同位置空 Error initializer |
| 已消费真实 `=` 后为普通 expression-start | 用现行 expression parser 解析唯一 initializer；完整 element 后的结构 stop 与调用方 `}` 均保留 |
| 已消费真实 `=` 后为其他普通非法 token | expected destructuring initializer 覆盖首 token；owner-aware 消费到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer；Lexer poison 已有根因时只建 Error initializer |

pattern 级逗号只由 pattern list 消费，真实 `)` 只由 pattern owner 消费；initializer 恢复继续
使用 block / lambda 的结构 stop 与调用方 hard stop。nested delimiter 和 lexical owner 内同形
token 不作同步点，Lexer poison / terminal 根因抑制同 Span 与同 closer 级联。每轮单调前进，
单个 pattern 与 initializer 合计 `O(n)`。

### 分阶段验收与路线图重排

每个 Spec 都必须包含 pass / fail / recovery、精确 UTF-8 Span、多 SourceMap identity、AST ID
与诊断顺序确定性、terminal Lexer owner、深嵌套与长正确 / 错误序列复杂度证据；复跑所有
已完成且适用于本次改动的历史 Spec 回归、fixture harness 与 workspace 基线，并同步
Architecture。独立分支的草案编号顺序不构成未完成前一 Spec 的实现依赖。
验收不以名称、类型、捕获或所有权正确性为条件。

| Spec | 最小 pass | 最小 fail / recovery |
|---|---|---|
| 0010 | 空 / 显式零参数 / 多参数、`move`、initializer / 普通 call argument / grouped statement、嵌套 lambda、body 首项或真实 nested-block closer 后的尾 expression | 严格 header 前缀 lookalike 按零参数 body 恢复、顶层 `,` / `->`、局部声明后普通 tail 被拒绝、缺 `}`、lambda 与 nested block 同 token 串的上下文对照；空 body 不是 fail |
| 0011 | 无体 / 空或非空 block body 的隐式 `Unit`，以及三种形态的显式返回标注 | 表达式体省略标注继续发 expected explicit return type；缺 `:` 与缺 TypeRef 恢复仍区分，AST 不伪造 `Unit` TypeRef / `:` Span |
| 0012 | 位置、命名、模式、命名加模式四形态，basic / typed / member / chained call，nested lambda / delimiter operand | 缺命名值、缺模式 operand、逆序或重复模式、空项 / trailing comma / 缺 `)`，并保留 outer owner closer |
| 0013 | 单 / 多 binding、复杂 RHS、与前后 element 相邻、block 与 lambda body 内嵌套 | 空 binding、缺 separator / `)` / `=` / initializer，`var` / `const` / `_` / nested / typed pattern 均按稳定类别拒绝 |

SPEC-0009 中 `f({})`、`val x = {}` 等“block 不可作 expression”的历史负例在 SPEC-0010 后
迁移为 expression-context lambda 正例；直接 block dispatch 的 `{}` 仍是 nested block。
SPEC-0007 的 trailing lambda 负例继续成立，不能用本次迁移批量接受其他 golden 变化。

0001–0009 的历史实体文件和编号保持不变；0010、0011 已有实体 Spec，0010 已按站立授权
进入实施，0011 保持排队中的 `draft`。其余候选尚未物化，本版按下表使用唯一编号，禁止保留
新旧编号别名：

| 新编号 | Goal / 旧候选映射 |
|---|---|
| 0010 | lambda literal |
| 0011 | 具名函数省略返回标注时固定为 `Unit` |
| 0012 | 命名 / 模式实参 |
| 0013 | 局部 `val` 解构 |
| 0014 | 完整文件、声明分隔与跨声明恢复（旧 0011） |
| 0015 | `module` / `import`（旧 0012） |
| 0016 | control-flow Parser（原待编号） |
| 0017 | class-family Parser（原待编号） |
| 0018–0026 | Phase 2 旧 0013–0021，逐项 `+5` |
| 0027–0032 | Phase 3 旧 0022–0027，逐项 `+5` |
| 0033–0041 | Phase 4 旧 0028–0036，逐项 `+5` |
| 0042–0051 | Phase 5 旧 0037–0046，逐项 `+5` |
| 0052–0061 | Phase 6 旧 0047–0056，逐项 `+5` |

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
- [x] **SPEC-0007**：实现第四部分独立表达式入口、全部运算符层级、`type_ref`、仅位置实参的
      basic call、单表达式索引、局部恢复及表达式 AST `Span`
- [x] **SPEC-0008**：实现第四部分第 7 节的独立 `val` / `var` / `const val` / `fun` 声明、单一
      内联泛型上界与签名 `type_ref`，以及按第 2、3 节无副作用试探规则成立的调用点类型实参
- [x] **SPEC-0009**：只实现第四部分第 8 节的独立 block、局部 `val` / `var` 与 expression
      statement 序列、嵌套 block，以及具名函数 block body
- [ ] **SPEC-0010（前置：SPEC-0009 `done`）**：只实现第 9 节 lambda literal
- [ ] **SPEC-0011（前置：SPEC-0009 `done`）**：只实现具名函数无体 / block body 省略返回
      标注时固定为 `Unit`；表达式体仍要求显式标注
- [ ] **SPEC-0012（前置：SPEC-0010 `done`；决策门禁：后续 guide 封闭 callee-side 参数模式）**：
      只实现 typed call argument、命名实参与 `own` / `inout` / `borrow` 模式实参
- [ ] **SPEC-0013（前置：SPEC-0012 `done`）**：只实现 block / lambda body 内局部 `val` 解构
- [ ] **SPEC-0014（前置：SPEC-0011、0013 `done`）**：只把 SPEC-0007 至 SPEC-0013 的既有
      节点组合为完整文件，并实现声明
      分隔、跨声明同步与级联抑制；单个语法错误不得导致整个文件解析中断，但本 Spec 不以
      尚未定义的控制流或 class-family 范例为验收条件
- [ ] **SPEC-0015**：解析 `module` / `import`；先由后续 guide 定义语法
- [ ] **SPEC-0016 / SPEC-0017**：由后续 guide 分别定义并实现 control-flow 与 class-family；
      不得把这些结构塞回 SPEC-0009 至 SPEC-0014

前两项 Lexer 工作以及 SPEC-0007 至 SPEC-0009 已完成；SPEC-0010 至 SPEC-0017 是本版规定
的后续 Parser 边界。未勾选状态不表示已经批准或已有代码；各 Spec 必须按实际依赖顺序独立
验收和提交。后续阶段使用第 9 节的新编号映射。
`type_ref` 的 Phase 1 反例必须拒绝含值实参的 `Array<Int, 4>`；`Array<Int, Size>` 的两个实参
都可先解析为类型引用，内建 `Array` 的 arity 则由 Phase 2 判断，Parser 不提前做名称或类型
判定。

**Lexer 验收标准**：穷举验证全部硬 / 软 / 未来保留字及前后缀边界；正例覆盖 ASCII
标识符、Unicode `Char` / `String` 内容、全部字面量、`${...}` 嵌套插值、LF / CRLF、两类
注释、全部固定符号和最长匹配；反例逐类覆盖上表八种错误，并断言稳定错误码、精确字节
`Span`、恢复后的后续 token 及确定性顺序。数字测试必须锁定 `1..2` / `1..<2` 与
`1e3` / `0x10` / `1L` / `1_0` 的边界。空文件只产生一个 EOF，所有非 EOF lexeme
非空、不重叠并覆盖全部输入字节；真实 pass / fail `.ko` fixture 必须被 Cargo test target
枚举，零用例必须失败。

**SPEC-0008 历史 Parser 验收标准**：独立正例至少覆盖带 / 不带类型标注的 `val`、`var`、
`const val`，无体和表达式体 `fun`，空 / 多参数，空缺 / 单个 / 多个泛型参数、递归单一上界
及函数类型签名；反例覆盖 `const` 后缺 `val`、缺名称 / `:` / `=` / initializer / 显式返回
类型、缺 separator 后直接出现 expression / type 起始、`=` / `{` / EOF 与其他非法 token、
不支持的 parameter default、声明列表空项 / 缺逗号 / trailing comma、泛型参数表缺 `>` 后跟
候选函数名与 `(`、多上界、`where`、声明修饰符、block body 与两个连续声明。这里把 block
body 列为反例只记录 SPEC-0008 完成时的测试边界；实施 SPEC-0009 后，按第 8 节
把该用例改为正例，不能继续用历史验收覆盖现行语法。typed call
必须同时覆盖
`f<T>()`、成员及调用链 callee、嵌套 `>>`、`>` 与 `(` 间 trivia，以及失败试探回退为比较的
相邻反例；测试必须证明失败试探不遗留诊断 / AST 节点且 ID 和诊断顺序确定。所有反例断言
稳定错误码和关键 UTF-8 字节 `Span`。名称恢复必须分别锁定 present / missing / error marker
及其非虚构范围。所有 consume-to-current-level 路径至少以 unsupported parameter default
覆盖 string / interpolation 内的 `,`、`)`、嵌套 string / interpolation、`L0004` 只结束内层
string 后继续处于父 owner、terminal `L0006` 及 EOF `L0005` 不越 owner 的用例，并证明只在
所有 owner 退出后才识别声明层 stop、没有 parser 级联。hard closing stop 还须覆盖局部 `[` 未
闭合便遇到外层参数表 `)`、局部 `(` 未闭合便遇到外层 type-parameter `>`，断言外层 closer
保留且错误范围在其之前结束；以 balanced nested `[...]` / `(...)` 后再遇外层 closer 作对照，
断言匹配的局部 closer 先被消费而外层 closer 才停止扫描。长错误区域还须锁定单调单遍
`O(k)` 扫描不发生二次回看。验收同时复跑 SPEC-0007 表达式与 TypeRef 回归。该验收不以
完整文件、类成员、block 语句、跨声明恢复或 Phase 2 名称 / 类型正确性为成功条件。

其中“缺显式返回类型”同样只记录 SPEC-0008 完成时的历史边界。SPEC-0011 只把参数列表后
直接到 EOF / 调用方无体 stop 或 `{` 的分支迁移为隐式 `Unit` 正例；参数列表后直接 `=` 仍是
反例并继续产生 expected explicit return type，不能批量迁移表达式体负例。

**Phase 1 聚合 Parser 验收标准（不归 SPEC-0014 单独承担）**：在 SPEC-0014 以及后续控制流、
class-family 等独立 Parser Spec 全部完成后，能完整解析以下代码为 AST，语法错误有准确的
行列号定位。该范例还依赖 lambda、命名实参、`when` 和 class-family，不能作为提前扩大
SPEC-0009 至 SPEC-0014 范围的理由：

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
    val points: List<Point> = listOf(Point(x = 1, y = 2), Point(x = 3, y = 4))
    val first = points[0]
    val double: (Int) -> Int = { x -> x * 2 }
    println(double(first.x))
}
```

## Phase 2：类型检查（不含所有权/借用）

- [ ] 局部类型推导（`val`/`var`）
- [ ] 函数签名类型检查（显式返回标注、隐式 `Unit` 与 `Nothing`）
- [ ] 接口/`enum class` 变体的类型检查，`when` 穷尽性检查
- [ ] **智能类型转换（smart cast）**：`is`/`when` 分支内的类型收窄及其失效规则（变量在收窄后被重新赋值则收窄失效）
- [ ] 泛型单态化的类型层面准备（类型替换，不接编译期计算）
- [ ] `Nothing` 类型的 bottom-type 特殊处理
- [ ] 计算 `value class` 的条件 `Copyable`：允许不可复制字段，按实际字段类型和泛型实参递归
      推导；支持把预声明的 `Copyable` 用作泛型上界，但不接受用户手动实现、覆盖或同名冒充
- [ ] 检查内联类型结构有限；拒绝未经过 `class`、`Box` 或动态容器等固定大小 handle 打断的
      直接 / 间接递归内联环
- [ ] 对内建 `Box<T>` 执行 type-kind 检查：只接受 `value class` 类型实参，拒绝普通 `class`
- [ ] 检查字段投影的使用模式：可复制字段可读出 owned copy，不可复制字段只允许投影借用，
      禁止把普通字段读取标记为所有权移出
- [ ] 为 `value class` 建立有序结构分量并支持解构类型检查；右值只求值一次，类型结果标记为
      复制式或消费式解构；不可复制类型的消费式解构必须覆盖全部分量
- [ ] 按第一部分第 8 节识别 `Array<T>`、`List<T>`、`MutableList<T>` 的精确单类型实参、
      长度 / 可变性角色和非 `Copyable` 独占 owner 能力；拒绝内建容器 arity 错误
- [ ] 检查顺序容器元素的 storable type 条件：保留单态化后的具体元素类型，不擦除为 `Any`，
      不把裸 interface 当作 v1 `dyn` 表示，也不隐式改写成 `Box<T>`
- [ ] 识别封闭的列表式 / 运行时长度构造操作并推导元素类型；把顺序容器索引结果标记为
      element place，按容器类型检查索引 key、place 可变性和赋值左侧合法性
- [ ] 顺序容器索引能力不进入用户可见 `Indexable` / `MutableIndexable` interface 或泛型上界；普通
      `.get(...)` / `.set(...)` 成员调用不得绕过内建 `[]` place 规则

Map 不是 Phase 2 的本版实施项。在后续 guide 定义 key 等价性与所有权契约前，类型检查器
不得自行加入 `V : Copyable`、key 借用、`put` 或下标赋值特例。

**验收标准**：能对 Phase 1 能解析的全部语法结构做类型检查，类型错误有清晰的错误码和
定位；`when (this) { is Circle -> radius }` 这类智能类型转换场景能正确通过类型检查；包含
不可复制字段的 `value class` 以及 `Pair<Sender<Int>, Receiver<Int>>` 均是合法类型，而
`Pair<Int, Int>` 被推导为 `Copyable`；`<T : Copyable>` 可以满足要求该上界的调用或类型
约束，未约束的 `<T>` 不能。未约束 `T` 的普通所有权转移仍然合法，不应在 Phase 2 因缺少
`Copyable` 报错；其移动后使用由 Phase 3 判断。`Box<Node>` 必须产生类型诊断。
`listOf(Point(...))` 必须保留为 `List<Point>`，不得推导为 `List<Box<Point>>`；运行时
`size: Int` 可以用 `Array<Point>(own size, borrow initializer)` 形成容器，而
`Array<Int, Size>` 必须产生内建类型 arity 诊断。不可复制元素可以形成合法顺序容器类型，
索引节点保留 place 类别，具体读取和借用合法性由 Phase 3 判断；`List<Any>`、裸 interface 元素和
`list.get(0)` 必须被拒绝。直接
递归或经多个 `value class` 形成的无限内联布局必须报错，经 `Box` 或动态容器打断的递归布局
必须合法。本 Phase 不以 Map 正反例作为验收，也不将任何 Map 所有权策略固化到 typed AST。

## Phase 3：所有权 / 借用检查

- [ ] 实现简化版单一所有者 + ASAP 析构（不做完整 NLL）
- [ ] 在后续 guide 先封闭 callee-side 参数模式契约后，检查调用点 `borrow` / `inout` / `own`
      与该契约匹配；不得仅凭 Phase 1 AST 猜测模式
- [ ] 移动后使用（use-after-move）检测
- [ ] 按类型能力区分复制与移动：`Copyable value class` 可以复制；非 `Copyable value class`
      与普通 `class` 转交所有权后都禁止再次使用
- [ ] 检查消费式解构：不可复制聚合解构后源值不可用，所有分量作为一个所有权动作转移
- [ ] 拒绝通过普通字段访问或单独 `componentN()` 移出不可复制分量，不建立部分移动状态
- [ ] 移动顺序容器时转移唯一缓冲区 owner，拒绝再次使用源容器；构造时按 `Copyable`
      能力处理已有 place：列表式构造必须要求显式 `own`，并复制或移动元素，不插入 clone、
      retain 或 `Box`；临时表达式和 initializer 返回值直接交付
- [ ] 检查顺序容器 element place：可复制元素可读出 owned copy；不可复制元素只可借用，
      禁止部分移出；`inout` 仅适用于 `Array` / `MutableList`
- [ ] 跟踪元素借用与 `MutableList` 扩容、缩容、删除、替换、重排的冲突；按第 8 节固定的提交
      顺序检查元素替换；所有正常构造、移动、替换、扩容和析构路径上，每个资源恰好析构一次
- [ ] `move (...) -> T` 函数类型的检查：验证传给此类参数的闭包字面量必须带 `move` 前缀，且闭包体内不能捕获任何借用语义的外部变量
- [ ] `Shareable`/`Transferable` 标记 trait 的检查：跨线程 API（`thread` 等）传递的值类型必须满足对应约束

**验收标准**：能正确拒绝典型的“移动后使用”和“重复可变借用”错误用例；复制
`Pair<Int, Int>` 后源值仍可用，复制 `Pair<Sender<Int>, Receiver<Int>>` 被拒绝，后者消费式
解构后再次使用源值也被拒绝；遗漏任一分量的消费式解构、`own pair.first` 这类不可复制
字段部分移动均被拒绝；能正确拒绝“把借用捕获的普通闭包传给 `thread()`”这类用例（必须
报错要求改用 `move { ... }`）。泛型 `<T>` 的 owned 转移后再次使用源值被拒绝，而
`<T : Copyable>` 的同类操作交付 owned copy，源值仍可用。还必须覆盖 `List<Endpoint>` 的
构造、整体移动和元素借用：`listOf(endpoint)` 因已有 place 缺少 `own` 被拒绝，
`listOf(own endpoint)` 移动元素；移动 List 后再次使用源 owner、把 `own list[i]` 用作调用
实参来移出不可复制元素、元素借用存续期间触发 `MutableList` 重分配都必须报错。显式
`List<Box<Endpoint>>` 继续按 `Box` 所有权检查，不获得特殊规则。
Map 所有权检查不在本版 Phase 3 范围内，必须等待第 8 节要求的后续 guide。

## Phase 4：LLVM 代码生成

- [ ] 自建 SSA IR，从 AST/类型检查结果 lower 到该 IR
- [ ] IR 到 LLVM IR 的映射（用 `inkwell`）
- [ ] `value class`（内联布局）vs `class`（堆分配）的 codegen 差异实现；布局策略与
      `Copyable` 能力保持正交
- [ ] 生成复制/移动/消费式解构：复制只用于 `Copyable` 类型，非可复制内联字段转移后不
      重复析构
- [ ] `Copyable` 复制不调用 retain / clone glue，也不为被复制值生成唯一析构义务
- [ ] 在 typed SSA 中保留顺序容器 owner、构造、length、checked-index、place load / borrow /
      store、relocation 和 drop 基元；owned SSA 值在每条正常退出路径恰好消费或析构一次
- [ ] 为单态化元素生成 size / alignment / stride，以系统堆基线生成固定大小 owner header、
      单个连续缓冲区、受检分配大小和先检查后寻址的索引；不生成逐元素 `Box`
- [ ] 在构造 LLVM 类型前拒绝目标 DataLayout 中的 size / alignment / stride 溢出和超过目标
      可表示对象大小的聚合，返回结构化用户诊断而不是 LLVM 错误或编译器崩溃
- [ ] 大型聚合与容器 header 的 ABI 间接传递不得 lower 为隐式 `Box`，也不得仅因参数或返回
      约定产生堆分配
- [ ] 验证标准顺序容器不存在 small-buffer storage-kind tag、短 / 长双表示或按优化级别改变的
      静态类型
- [ ] 构造和替换保持第 8 节的求值 / 提交 / 析构顺序；正常析构按元素逆序后释放缓冲区，
      ZST 仍按逻辑 `size` 执行 drop；abort 路径不生成异常展开或部分构造 cleanup
- [ ] 在目标布局确定后估算静态栈帧和实际仍存在的隐式大值复制，以对应 Spec 分配的稳定
      warning code 报告目标相关阈值超限，不因 warning 自动改变类型或表示
- [ ] **闭包环境捕获的 codegen**：捕获环境结构体的内存布局设计，`move` 闭包与默认借用闭包在捕获方式上的差异实现，无捕获场景下降级为裸函数指针
- [ ] 析构函数插入（对应 Phase 3 的 ASAP 析构点）
- [ ] `error()` 编译为 abort 语义（不生成栈展开代码）
- [ ] DWARF 调试信息生成

**验收标准**：能编译并运行第二部分示例代码，产出正确结果的可执行文件；带副作用的解构
右值只执行一次，消费式解构后的每个不可复制字段恰好析构一次，不可复制 `value class`
移入 `Box` 后源存储不再析构，可复制 `Point` 通过 `own` 装箱后源值仍有效，复制
`Pair<Int, Int>` 不调用 glue 且源值仍有效；能用 `gdb`（Linux）或 `lldb`（macOS，现代
macOS 工具链下 `gdb` 需要额外签名权限，`lldb` 摩擦更小，两者都基于 DWARF）设断点单步
调试。IR / runtime 基元验收还必须证明：运行时长度的 `Array<Point>` 使用一个连续堆缓冲区，
`List<Point>` 不为各元素单独分配；大型 `Point` 可以 ABI 间接传递，但不得出现隐式 `Box`
allocation；非 `Copyable` 元素在替换、移动、重分配和正常析构路径上恰好 drop 一次，ZST 的
drop 次数仍等于逻辑长度。越界检查必须发生在地址计算前，负长度、分配大小溢出与 OOM 走
abort 且不生成异常展开。大栈帧 / 大型隐式复制测试必须锁定 warning code 和关键 `Span`，并
证明 warning 不会改变程序的静态类型。分配消除不属于本 Phase 的正确性验收；启用时必须由
独立 Phase 4+ 优化 Spec 与基线结果做差分验证。

## Phase 5：最小标准库（用目标语言自身编写）

- [ ] 在预声明的 `Array`、`List`、`MutableList` 及 Phase 4 基元之上，用目标语言实现
      `MutableList` 增删等普通集合方法与算法；不在 `.ko` 中重新声明 `arrayOf`、`listOf`、
      `mutableListOf`、运行时长度构造、`size` 或 `[]`，也不重新实现容器 header
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
使用源值、`Box<Node>` 的 compile-fail 和 `Rc<T>` 不被误判为 `Copyable`。顺序容器测试还
必须覆盖基础标量、可复制 / 不可复制 `value class`、普通 `class` handle 和显式 `Box` 元素：
`List<Point>` 不产生逐元素分配，`List<Box<Point>>` 只因显式 `Box` 产生间接分配；运行时读取
`n` 后，`Array<Point>(own n, borrow initializer)` /
`List<Point>(own n, borrow initializer)` 必须得到长度 `n`，按索引升序各调用 initializer 一次，
负长度稳定 abort。`Array<Endpoint>` 替换元素必须按提交顺序且
旧值析构一次，`MutableList<Endpoint>` 多次扩容后值保持正确且每个资源最终只释放一次。空容器、
零大小元素、单元素容器、分配大小溢出、移动后使用、显式 `own` 构造和不可复制元素索引借用
都必须有正反例；顺序容器 `.get` / `.set` 与 `getOrNull` 均不得作为隐藏的特殊入口出现。
Map 不是本版 Phase 5 验收项；不得为让测试通过而将本版未定义的 key 等价性或
所有权策略固化在标准库中。

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
