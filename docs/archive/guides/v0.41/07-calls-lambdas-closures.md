# Koven v0.41：调用、Lambda 与 Closure

> **性质**：规范性语言规范 · **状态**：current（v0.41） · **读取时机**：实现或评审调用匹配、lambda、capture 与 overload trial 时 · **唯一真源**：本页

本页是现行 Koven v0.41 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## Closure Capture 与跨线程转移

### Capture 身份与普通闭包

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

### Move Closure

`move { ... }` 对每个自由 binding 建立 owned capture：静态满足 `Copyable` 时复制 snapshot，
否则必须从当前 owned、available binding 移动，lambda 形成后源 binding 不可再使用。Borrow /
Inout binding 的 MoveOnly 值不能形成 owned capture，产生 L0138；Copyable 值可从其 shared read
复制为 owned snapshot。local `var` 捕获的是形成时 snapshot，捕获名称在 v1 closure 内仍不可
赋值，避免引入独立的 mutable closure receiver 模型。

move closure 可以绑定、return、写入字段或交给 Value 参数。v1 的 move closure 不直接捕获
显式/隐式 `this` 或字段；需要先把所需字段复制/移动到 local，再捕获该 local，否则产生 L0138。
这是现行限制；普通 shared `this` capture 不受影响。

### 捕获方式的选择

两种闭包不是强弱或性能关系，而是两份不同的所有权契约。选择取决于两件事：闭包的 loan 是否
会活过当前作用域或调用，以及捕获值是否跨越线程边界。

普通闭包用于**立即消费**：callee 在同步调用内用完闭包，loan 不超出当前 callable。

```kotlin
fun applyTwice(f: (Int) -> Int, x: Int): Int = f(f(x))

fun demo(): Int {
    val scale = 3
    return applyTwice({ n -> n * scale }, 7)   // 只读捕获；scale 仍可继续使用
}
```

顺序容器运行时长度构造的 `initializer` 参数契约是 `(Int) -> T`，同样只接受立即消费的闭包：

```kotlin
fun makeSquares(): List<Int> = List<Int>(4, { index -> index * index })
```

普通闭包形成 shared loan 后，源 owner 在该 loan 存活期间不能被移动或析构（L0135）；捕获名称
在 lambda 内也不能赋值或按值移出 MoveOnly 值。它仍可绑定与移动，但**不能** return、写入
字段、交给 `Value` 参数或被 escaping move closure 捕获；这些位置产生 L0137。

```kotlin
// 错误：借用捕获随闭包逃出 defining callable，L0137
fun makeMatcher(label: String): () -> Boolean = { label == "report" }

// 正确：需要逃逸时改用 move；捕获源须是 owning binding，因此参数声明为 own
fun makeMatcher(own label: String): () -> Boolean = move { label == "report" }
```

move closure 只用于必须把捕获值所有权搬离当前位置的场合：逃逸、跨线程，或把独占资源交给
延迟执行。捕获 `Copyable` 值时 move 只产生快照，与普通闭包在可观察语义上没有区别；无捕获的
普通 lambda 没有 loan，可按普通函数值使用。因此 move 不应作为默认写法。

函数类型的 `move` 前缀表达的是对该函数值不含借用捕获的约束：跨线程入口的
`move (...) -> T` 参数以此限制实参，而不是给闭包值本身打标签。

| 场合 | 捕获方式 | 关键约束 |
|---|---|---|
| 同步调用、立即消费 | 普通闭包 | 源变量保持可用；loan 结束后才可移动 / 析构源 |
| 闭包逃逸（return、写字段、交给 `Value` 参数） | move | 普通闭包逃逸产生 L0137 |
| 跨线程入口（如 `thread`、`Sender.send`） | move | 入口参数契约为 `move (...) -> T`；每个 owned capture 须满足 `Transferable`，否则 L0139 |
| 捕获 `String`、顺序容器等 MoveOnly 值并逃逸 | move | 捕获源须是 owned、available binding，否则 L0138 |

### `Transferable` 与跨线程 Effect

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

### 产物、析构与诊断

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

## 泛型 Callable 实例化与 Overload-Lambda 隔离

### 范围与边界

v1 只为源码具名顶层函数和实例 member 函数实例化 callable 类型参数。调用可以写完整显式
类型实参 `f<A, B>(...)`，也可以完全省略并从实参推导；不接受部分类型实参、`_` 占位、默认
类型实参、`where` 约束或把未填项留给返回上下文。普通函数值没有 callable 泛型参数，class
constructor 遵守[构造规则](11-copyability-layout-construction.md#名义值与-intrinsic-box-构造)，
跨文件 overload 遵守 compilation-unit 类型规则；callable reference 与 safe call 未进入现行范围。

泛型推导保持单向和局部：只读取本次调用中已经具有确定类型的非 lambda 实参，包括具名
函数值；不从调用结果的 expected type、赋值后的使用、lambda body、未定型 lambda 参数或
另一个文件反推类型实参。没有 expected type 的数字字面量先按[类型规则](03-types-generics.md)的默认规则定型，再
参与推导。因此 `identity(1)` 推导 `T = Int`，而仅在 `factory<T>(): T` 返回位置出现的 `T`
不能从 `val value: String = factory()` 推导。

### 候选实例化与 Bound

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

### 多 Overload 候选中的 Lambda

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

### 诊断与阶段边界

| 错误码 | 候选稳定含义 | primary / 关联位置 |
|---|---|---|
| L0140 | 唯一泛型 callable 无法从合法输入得到完整一致的类型实参 | primary 为 callee；label 指向未解/冲突的类型参数声明 |
| L0141 | callable 类型实参不满足编译器 `Transferable` bound | primary 为显式实参或触发推导的 operand；label 指向 bound 声明 |

泛型 callable 实例化与 overload-lambda candidate isolation 是两个连续的类型检查责任；只有唯一
候选的完整事实可以提交。当前未形成完整 typed target 的 callable reference、safe call 等调用必须
保留精确 `DeferredReason`，不得携带伪造实例进入所有权检查或 SSA。

---

## Lambda 与调用参数

lambda、callable 参数 marker 与 typed call argument 复用同一 expression/statement/声明基础。
Parser 只保存调用边界语法；名称映射、类型候选和所有权效果由后续检查完成，不能从 AST
形状提前推断。

lambda 只实现一个函数类型调用入口；它不产生匿名 class/object，也不能实现
多方法 interface。需要多方法实现时使用具名 class；机械转发可使用[class-family 规则](08-class-family-members.md)限定的接口委托。

lambda、typed argument 与解构三项结构 parser 都必须接收调用方的 hard stop 集合，并把自身
真实 closer 作为新增 owner。恢复按以下固定优先级处理边界，不能用“所有局部 owner 退出后
才看 hard stop”的笼统规则代替：

1. 当前 token 匹配局部 delimiter / lexical owner 栈顶 closer 时，先消费并 pop；
2. 否则，只有 lexical-owner stack 已回到本次恢复入口 baseline 时，EOF 或与普通 delimiter 栈顶
   异形的调用方 hard closer（`)`、`]`、`}` 等）才立即停止并保留；它可以抢占未闭合的普通
   delimiter，这些局部 delimiter 随当前 error region 结束，例如 `f({ a[x )` 中 `)` 必须留给
   call owner。若仍处于本次扫描新开的 string / interpolation owner 内，其中的同形 token
   必须先由该 lexical owner 消费，不能越过它关闭外层 call；`StringEnd` / `InterpolationEnd`
   若正是入口 baseline owner 的真实 closer，则按[声明与 callable 规则](05-declarations-callables.md)作为调用方 hard closer 保留；
3. 同形 `}` 同时可关闭最内层 lambda / block owner 时，最内层 owner 优先消费。例如只有一个
   `}` 的嵌套 lambda / block 输入先关闭 lambda，外层 block 随后报告缺 closer；
4. 逗号、下一 argument / element 候选等 soft stop 只有在局部 delimiter 与 lexical owner 回到
   进入恢复时的 baseline 后才生效。

三项结构恢复统一复用
[声明恢复规则](05-declarations-callables.md#最小诊断与局部恢复)在 parser 构造时一次建立的
terminal-owner event index；遇到
Lexer `L0004`–`L0006` 的开始、恢复结束及抑制事件时按 owner 栈推进，不在每个 lambda、实参
或解构错误处重扫诊断。

### Lambda literal

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
`({ x })`。`move { ... }` 只能是 lambda。同行的 trailing lambda
（`f { ... }`）；跨换行仍把 `{ ... }` 留给后续 nested block element。

[block 与控制流规则](06-blocks-control-flow.md)的 `{` soft element stop 因此是 parser-state-sensitive 的：已有完整左表达式且没有运算符
要求右 operand 时，顶层 `{` 留给下一 nested-block element；initializer 起点、prefix / binary
右 operand、grouped expression 或 call argument 等正在等待 primary 的位置则必须让 `{` 进入
lambda parser。`x { y }` 是一个尾 lambda call，`x\n{ y }` 才是 expression
statement `x` 后接 nested block；`x + { y }` 的右侧仍是 lambda。该判定只依赖语法状态和
尾 lambda 的换行边界，不依赖名称或推测类型。

`->` 前允许零个或多个逗号分隔的普通 Identifier；`{ -> e }` 是显式零参数形式。参数不接受
类型、默认值、`val` / `var`、模式、解构或 trailing comma。Header 只能从 `{` 后第一个非
trivia token 起严格匹配完整前缀 `[ Identifier { "," Identifier } ] "->"`；只有整个前缀
成功才提交。任一 token 不匹配就以零状态失败，并从 `{` 后按零参数 body 解析，不得继续搜索
后方任意顶层 `->`。因此 `{ source as () -> Int }` 中函数类型的箭头绝不会反向把 `source as ()`
误判为 lambda 参数，`{ x y -> z }` 也不是可恢复 header，而是带非法 body token 的零参数
lambda。严格前缀的 DFA 与共享索引见[Parser 工程合同](../../../compiler-specs/parser-algorithms.md#lambda-header-试探-dfa)。

Body 复用[block 与控制流规则](06-blocks-control-flow.md)的三种 element 和最大 element / 显式 stop 规则，但使用独立 lambda-body
payload，不能复用静态类型固定为 `Unit` 的 `Statement::Block`。若最后一个 element 是
expression statement，该 expression 是 lambda 的尾值；空 body 或最后一项为局部声明 / nested
block 时尾值为 `Unit`。Body 内部各 element 遵循 block 的换行与分号 `;` 分隔契约：普通 expression-start
不是同一行局部声明 initializer 的隐式 stop，因此同行内无分号的 `{ val x = 1 x }` 属于语法错误；
而在换行或分号分隔下（如 `{ val x = 1 \n x }` 或 `{ val x = 1; x }`），声明之后紧随 tail expression
是完全合法且标准的写法。Parser 只保存该结构；参数类型、捕获、返回类型与 `move` 合法性由后续检查。
`return` 可作为 lambda body element，且退出最近 lambda；完整 jump 产生式、
上下文与恢复规则见[block 与控制流规则](06-blocks-control-flow.md)。

Lambda 参数源码不重复书写 `own` / `borrow` / `inout`；其契约由 Phase 2 对该 lambda 应用的
**期望函数类型**逐项提供。例如把 `{ x -> use(x) }` 检查为 `(T) -> R` 或
`(borrow T) -> R` 时，body 中的 `x` 都是共享借用参数；检查为 `(own T) -> R` 时，`x` 是
owned `Value` 参数，`Copyable` 时可复制、否则按普通移动规则使用；检查为 `(inout T) -> R`
时则获得独占可变绑定。每个重载候选必须用自身
期望函数类型独立检查 lambda，不能先默认成任一 mode 再做隐式模式转换；若无法得到唯一的参数
类型 / 契约，沿用普通 lambda 上下文类型不足或重载歧义诊断。Phase 1 的 Lambda AST 仍只保存
真实参数名 Span，不伪造 marker；typed AST 必须保存最终采用的函数参数契约。`move` 只约束
捕获，与期望函数类型的参数契约正交。capture 语义由
[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md) 唯一定义：普通 capturing lambda 建立
shared capture 且不得逃出 defining callable，`move` lambda 形成 owned capture 并可逃逸。
Parser 不计算 capture；Phase 3 必须按解析后的 symbol identity 检查。

lambda body 在最大 expression 已完整、没有子语法等待 token，且 delimiter / lexical owner
回到 body baseline 时，额外把顶层 `,` 与 `->` 作为 body-dispatch soft stop。它们只把控制权
交回当前 lambda body，不得泄漏成外围 call 的 argument separator。call、group、function type
或其他 nested owner 内的 `,` / `->` 不受影响，因此 `{ source as () -> Int }` 仍是单个完整
尾表达式。

Lambda payload 与 header 状态见[Lambda AST 工程合同](../../../compiler-specs/parser-ast.md#lambda-payload-字段)。
完整 lambda Span 从真实 `move`（若存在）
或 `{` 起至匹配 `}` 终；缺 `}` 时止于最后实际消费位置。由于 header 只在严格完整匹配后
提交，参数均为真实 Identifier，不存在 missing / error 参数 marker；header Span 从首参数
（零参数时从 `->`）至 `->` 终。body element 沿用[block 与控制流规则](06-blocks-control-flow.md)范围，不为缺失 token 伪造非空 Span。

lambda 规则 从 L0031 开始分配自身专用的 expected lambda body element 与 unsupported lambda
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

Header 的预索引与查询复杂度见[共享预索引合同](../../../compiler-specs/parser-algorithms.md#lambda-header-共享预索引)。

缺 lambda `}` 时，正式 parser 在最早的
调用方 hard stop 停止，即使预索引的词法范围延伸得更远也不得越界。每轮要么消费 lexeme，
要么在自身 `}`、调用方 hard stop 或 EOF
结束，整体 `O(n)`、owner 栈 `O(d)`。

### 尾 lambda 调用糖

尾 lambda 只是一种 call-argument 表面语法，不增加 AST 节点、参数模式或调用语义：

```kotlin
consume { item -> item }          // 等价于 consume({ item -> item })
consume(1) { item -> item }       // 等价于 consume(1, { item -> item })
consume<Int> { item -> item }     // 等价于 consume<Int>({ item -> item })
receiver.consume(1) { item -> item }
```

- `{ ... }` 必须在没有换行的 trivia gap 后紧随可调用 postfix；因此 `f {}` 是尾 lambda，
  `f\n{}` 是 expression statement `f` 后接 nested block。LF、CRLF 或含换行 comment 都形成
  该边界；尾 lambda 的附着规则与 [block element 分隔及续行规则](06-blocks-control-flow.md)
  同时适用，不能跨越已经终止的 block element。
- callee 尚无圆括号 call suffix 时，Parser 建立一个只有该 lambda 的 `Expression::Call`；已有
  `(...)` 时，把 lambda 追加为同一个 call 的最后一个普通 `CallArgument`，并把 call span 扩到
  lambda 的 `}`。不得把 `f() {}` 表示成“先调用 `f()`、再调用其结果”的第二个 call。
- 尾 lambda 的 `CallArgument` 没有 name 或显式 mode；需要命名或 `&` 标注时仍写在
  圆括号内。每个 call 最多接受一个同行尾 lambda，第二个同行的 `{ ... }` 继续形成
  trailing-input 语法错误，而不是隐式调用前一个 call 的结果。
- typed/member/chained callee 复用同一规则；失败的 `<...>` call-type-argument 试探仍必须零状态
  回退，不能仅因后方存在 `{` 就把比较表达式重解释为 typed call。
- 尾 lambda 本身不增加新的 lambda body 语义；下一节为全部无显式 header lambda 定义隐式
  `it`。label return、receiver lambda、参数 trailing comma、默认参数、`vararg` 与新的调用点
  mode 均不属于本语法。

### 无显式 header lambda 的隐式 `it`

隐式 `it` 属于 lambda，不属于尾随调用语法。因此以下三种写法采用同一参数契约：

```kotlin
consume { use(it) }
consume({ use(it) })
val transform: (borrow Item) -> Result = { use(it) }
```

- 只有 `arrow_span == None` 的无显式 header lambda 才有一个 contextual implicit-parameter
  candidate。期望函数类型恰有一个参数时，该 candidate 激活为名为 `it` 的
  `LambdaParameter`；其类型及 Borrow/Value/Inout mode 精确来自期望参数，即使 body 未读取
  `it` 也不改变 arity。
- 显式 `{ value -> ... }`、多参数 header 与显式零参数 `{ -> ... }` 都不创建隐式参数；其中
  出现的 `it` 按普通词法作用域解析。
- 无 expected function type 且 body 引用隐式 `it` 时使用 L0083，不能从 `it` 的操作、返回使用
  或 overload 候选之外反向推导参数类型。没有 `it` 引用时，既有无参 lambda 尾值推导继续
  形成 `() -> R`。
- expected function type 为零参数而 body 引用 `it`，或参数数量大于一但没有显式 header 时，
  使用 L0084 拒绝结构不匹配；不得静默选择第一个参数或虚构 tuple 参数。
- headerless lambda 内的 `it` 优先保留给 implicit candidate，不回退捕获外层同名 binding。
  若要在零参数 lambda 中读取外层 `it`，必须写 `{ -> it }`；普通 local shadowing 仍遵守
  initializer 完成后可见的既有规则。
- 名称产物必须为激活或待定的 `it` 保留稳定 lambda-local symbol identity，并以真实 `{` 作为
  synthetic declaration anchor，不伪造 Identifier Span。typed facts 发布参数类型/mode；
  ownership 把它当普通 lambda parameter 而非 capture，后端继续复用现有函数参数 ABI。
- nested headerless lambda 各自拥有独立 candidate；显式 header、group、尾 lambda 与输入文件
  顺序不得改变 symbol/type/mode/capture identity。

### Typed call argument

```ebnf
call_suffix       = "(", [ call_argument,
                            { ",", call_argument } ], ")" ;
call_argument     = [ named_argument_prefix ], [ argument_mode ], expression ;
named_argument_prefix = Identifier, "=" ;
argument_mode     = "&" ;
```

调用实参的 `argument_mode` 与声明侧 `parameter_mode` 是两套封闭语法：声明侧接受
`own` / `borrow` / `inout`，调用点只接受 `&`。声明用 `inout` 表示可变借用契约，
调用点必须用 `&` 交付可变 place；`own`、`borrow`、`inout` 都不是调用点模式标记。

唯一源码顺序是“可选名称、可选 `&`、表达式”：`f(e)`、`f(name = e)`、`f(&e)` 与
`f(name = &e)` 均可形成语法 AST。`&` 不是通用一元运算符，只有 call argument 入口可消费。
`f(borrow x)`、`f(name = borrow x)` 与 `f(own x)` 均非法；旧借用调用迁移为 `f(x)`、
`f(name = x)`。`borrow(x)`、`f(borrow(x))` 与 `f(borrow)` 中的 `borrow` 是普通 Identifier，
按普通名称/调用规则解析，不建立显式借用 marker。模式后直接出现顶层 `Identifier =`
是错误的逆序组合；显式分组的 `&(x = y)` 仍是 assignment expression 为 operand 的模式实参。
空列表合法，
trailing comma 继续非法。Parser 只保留源码顺序和真实 marker；名称映射、参数契约匹配与
operand 的 place / temporary 分类由后续检查完成，parser 不按 callee 名称或
operand 形态改变语法。

#### Callable 参数契约与调用匹配

callee 的每个值参数具有 `Value`、`Borrow` 或 `Inout` 契约。无标记或声明侧显式 `borrow`
都是 `Borrow`；声明侧显式 `own` 映射 `Value`；显式 `inout` 映射 `Inout`。`Value` 接收
完整 owned value，满足 `Copyable` 时复制，否则移动；声明必须写 `own`，调用点却始终不写
marker。`Borrow` 表示调用期间共享借用，调用点始终无 marker，编译器按 callee 已声明的
契约自动借用。`Inout` 表示调用期间独占可变借用，
调用点**必须**显式标注，但拼写是符号 `&`，不是声明侧使用的关键字 `inout`
（`mutate(&x)`，不是 `mutate(inout x)`）。默认参数**模式**
不等于默认参数**值**；v1 继续禁止参数默认值与用户声明 `vararg`。

调用点与 callee 契约的唯一兼容矩阵如下；“temporary”是本次 operand 求值新产生且没有既有
place 身份的值，group 继承内部表达式的类别。名称、成员或索引只有在 Phase 2 把它标为
place 时才属于 place；其他产生完整值的表达式均为 temporary。

| 调用点形态 | `Value`（声明端 `own`） | `Borrow`（无标记或声明端 `borrow`） | `Inout` |
|---|---|---|---|
| 无 marker、operand 为 place | 合法，按值复制或移动 | 合法，自动借用，借到同步调用结束 | 非法，缺 `&` |
| 无 marker、operand 为 temporary | 合法，直接交付 | 合法，自动借用，借到同步调用结束 | 非法，始终要求可变 place |
| `&operand` | 非法 | 非法 | 仅 operand 为可变 place 时合法 |

`Copyable` 只决定向 `Value` 交付时是复制还是移动，不改变矩阵，也不让 `Inout` place 省略 marker。
Borrow 参数接受无 marker 的 temporary；显式 `&temporary` 非法。place、可变性、Copyable、移动
与借用冲突由 Phase 3 判断，Phase 1 不据此拒绝语法。

调用点 `own operand` 不在矩阵中，因为它不是合法 `argument_mode`。对
`fun consume(own value: T)`，`consume(place)` 已由 callee contract 唯一决定 owned delivery；
若 `place` 是 `MoveOnly`，该无标记调用移动 owner 并使源 place 后续不可用。

Phase 2 先按源码顺序解析每个 argument 的类型和表达式类别，再对每个候选 callable 做映射：位置实参可以
出现在第一个命名实参之前，并依次填充尚未匹配的参数；一旦出现命名实参，后续所有实参都
必须命名。名称按大小写敏感的源码 Identifier 精确匹配，只允许直接解析到具有稳定参数名的
具名 callable；通过函数值、callable reference 结果或其他只拥有函数类型的 callee 调用时，
命名实参是类型诊断。一个参数不能被位置与名称重复填充，同一名称不能出现两次；v1 没有
默认参数值，所以成功调用必须恰好填充全部参数且没有额外实参。每个重载候选独立应用这套
映射与契约约束；无 marker 的 place 或 temporary 都可直接与 `Value` 或 `Borrow` 候选兼容，
显式 `&` 只能与 `Inout` 候选兼容。参数 mode 不参与 overload shape，不能声明只在 mode
上不同的 overload，也不存在通过调用处 `borrow` 筛选候选的语法。
若类型检查后仍有多个候选，使用统一的 overload 歧义诊断，不能通过重排求值或忽略契约
择一。这里的表达式类别只区分类型层面已经
建立的 place 与 temporary；Phase 2 不判断 place 此刻能否移动、借用或独占访问，这些动态
所有权前提仍全部属于 Phase 3。无论名称把实参映射到哪个参数，operand 始终按**源码从左到右**各求值一次，
随后才按映射交付；命名顺序不改变副作用顺序。

所有编译器预声明 callable、核心构造器与后续标准库签名都必须使用同一有序参数元数据：
参数契约、可选稳定名称及 TypeRef。列表式核心构造可使用内部“重复 `Value`（源码等价于
重复 `own`）参数”形状，仍不
向用户开放 `vararg` 声明语法；parser 始终生成普通 CallArgument，不按 `arrayOf`、`listOf`、
`MutableList.add`、`println` 等名称硬编码模式或省略例外。预声明 API 的具体签名由对应标准库
规范页面列出，但其调用必须服从上述统一匹配矩阵。

以下预声明 / 核心 API 的参数模式固定；标准库可以补充具体重载，但不得改变这些位置的契约：

| API 形状 | 有序参数契约 |
|---|---|
| `error(message)`、`println(value)` | 唯一参数为无标记 `Borrow`；`println` 可有多个具体类型重载，但契约相同 |
| `Box<T>(value)` | `Value T`，声明等价于 `own value: T` |
| `arrayOf(...)` / `listOf(...)` / `mutableListOf(...)` | 每个元素位置都是内部重复的 `Value T`，等价于重复 `own` |
| `Array<T>(size, initializer)` / `List<T>(size, initializer)` | 两个参数均为 `Borrow`；类型依次为 `Int`、`(Int) -> T`，后者的无标记 `Int` 参数同样是 `Borrow` |
| `MutableList<T>.add(value)` | `Value T`，声明端 `own` |
| `thread(task)` | `Value (move () -> Unit)`，声明端 `own` |
| `channel<T>()`、`join()`、`receive()` | 无参数 |
| `Sender<T>.send(value)` | `Value T`，声明端 `own` |

其中 `(Int) -> T` 的 `Int` 参数无 marker，因此是 `Borrow`；表格中的“声明端 `own`”只解释
规范签名，不是调用点语法。上表所有 `Value` 与 `Borrow` 参数在调用点都不写 marker。
`println` 的可打印类型集合、channel / thread 的具体返回类型及普通集合算法由对应 Phase 2 / 5
规范确定；这些留白不允许改变上表的模式或让 parser 按名称特判。

具名函数 `ValueParameter`、函数类型参数和 `CallArgument` 共用现行 AST / parser。声明 marker
为 `Own` / `Borrow` / `Inout`，missing marker 的 typed mode 是 `Borrow`；调用实参的表面
字母表只有符号 `&`。Span 各自
覆盖源码中实际出现的 token；Phase 2
实现名称、参数映射、类型与参数契约匹配并标记
类型层面的 place / temporary 类别，Phase 3 才实现具体 place 的移动能力、可变性、复制 /
移动与借用效果。Parser 不得提前声称名称、类型或所有权检查已经完成。

`Expression::Call` 的 `arguments` 字段唯一改为 `Vec<CallArgument>`，不得再建
`CallArgumentId`、第五张 AST table 或同时保留旧 `Vec<ExpressionId>`。`CallArgument` 是内嵌
payload，至少保存完整 `span`、`named_prefix: Option<NamedArgumentPrefix>`、
`mode_marker: Option<ParameterModeMarker>` 及唯一 `value: ExpressionId`；
`NamedArgumentPrefix` 封闭保存真实 `name_span` 与 `equals_span`，不能用两个独立 `Option` 构造
只有名称或只有等号的半状态。mode marker 复用
[AST `Span` 与恢复规则](15-conformance-and-staging.md#ast-span-与恢复一致性)封闭的
`ParameterModeMarker::{Own(Span), Borrow(Span), Inout(Span)}`，自身封闭真实 kind / Span；
但 `CallArgument` 的构造不变量只允许 `Inout`，`Own` / `Borrow` 只用于声明 marker；
parser 不得因表达式中出现 `own` / `borrow` Identifier 就构造相应实参 marker。**在
`CallArgument` 里，`Inout(Span)` 变体的 `Span` 覆盖的是符号 `&`
token，不是关键字 `inout` token**（value_parameter / function_type_parameter 的
`Inout(Span)` 才覆盖 `inout` 关键字）；两处共享同一枚举变体名是因为语义相同，实现读取
`Span` 时不能假设它一定是某个固定字符长度的 token。完整实参从名称或模式（存在时）
否则 operand 起，到 operand 终；恢复时只到最后实际消费位置，空 Error value 的 boundary
插入点不扩大 argument 范围。call 与 typed call 的既有合成
Span 不变。错误 operand 只覆盖实际消费区域，或在 call / 调用方 hard stop 处为空范围。

调用参数诊断在现有 L0032 后连续分配六类：`L0033 expected argument value`、
`L0034 expected argument separator`、`L0035 unsupported argument empty element`、
`L0036 unsupported argument trailing comma`、`L0037 invalid argument mode ordering` 与
`L0038 duplicate argument mode`；固定消息就是各英文类别拼写，主 `Span` 按下表。参数声明 /
函数类型的 `L0039 duplicate parameter mode` 由上节定义。这六类诊断的触发条件与恢复语义
继续按下表应用唯一的 `&` marker：`f(& &x)` 是 `duplicate argument mode`，
`f(&name = x)` 是 `invalid argument mode ordering`；`&x` 单独出现不算“重复”。`f(&&x)` 不落入这一类别——lexer 按
最长匹配把 `&&` 识别为单一 token（logical-and 的固定符号），不是两个相邻的 `&`，因此
parser 在 `argument_mode` 位置看到的是一个不匹配 `"&"` 的 `&&` token，按
`expected argument value` 处理；只有写成 `f(& &x)`（中间有 trivia）才会产生两个独立
`&` token，触发 `duplicate argument mode`。不得复用声明列表 L0024–L0026，`)`
缺失只复用通用 expected closing delimiter。恢复分支精确如下：

`f(borrow x)`、`f(own x)` 与 `f(inout x)` 中首个名称按普通 Identifier expression 解析，
后续 `x` 缺少实参 separator，按下表产生 `L0034 expected argument separator`；不得把名称
重新解释成 marker，也不得仅因普通函数或变量名是 `borrow` / `own` / `inout` 而拒绝。

| 分支 | 诊断、消费与 AST |
|---|---|
| 初始位置直接 `)` | 合法空列表；不消费 `)` 之外的 token，不创建 argument |
| 初始 / separator 后直接 `,` | unsupported argument empty element 覆盖该逗号；消费逗号并追加一个在逗号起点为空的 Error value argument，再继续下一项；若随后直接 `)`，不为同一空项追加第二条诊断 |
| 已有完整项后的 `,` 紧接 `)` | unsupported argument trailing comma 覆盖并消费逗号；追加一个在 `)` 起点为空的 Error value argument，保留 `)` 给 call owner；本分支优先于一般 separator 消费 |
| 已提交 `Identifier =` 或首个 mode 后遇顶层 `,` / `)` / 调用方 hard stop / EOF | expected argument value 取该边界的空 Span；追加保留已消费 name / mode marker 的唯一 Error value argument，不再追加同根因 expected expression。若边界为 `,`，本分支消费它作为当前缺值项的 separator；紧接 `)` 时不得再报 trailing comma 或追加第二个 Error argument；其他 hard stop / EOF 不消费 |
| 连续第二个及以后 mode | 每个多余 mode 各发 duplicate argument mode 并消费；AST 只保存首个 mode，argument Span 仍覆盖所有已消费错误 marker 后的最终 value |
| mode 后直接出现顶层 `Identifier =` | invalid argument mode ordering 主 Span 覆盖 `=`；消费该 Identifier 与 `=` 作为错误区域，不保存 name marker，再从其后解析唯一 value |
| 实参起点，或已提交 name / mode 后遇不能开始 expression 的其他普通 token | expected argument value 覆盖首个非法 token；owner-aware 消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，追加覆盖实际错误区的 Error value argument；Lexer poison 已有根因时只消费并建 Error value，不重复诊断 |
| 完整 value 后直接出现可开始下一 argument 的 token | expected argument separator 取该 token 起点空 Span且不消费；结束前一项并把该 token 作为下一项起点。该 soft boundary 只在 operand 已完整时生效，不能截断正在等待右 operand 的表达式 |
| 完整 value 后出现其他非法 token | expected argument separator 覆盖首个非法 token；按统一 owner 扫描消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，不把错误区附会为下一实参 |
| 缺所属 `)` | lexical owner 回到本 call 入口 baseline 后，在最早调用方 hard stop / EOF 发 expected closing delimiter 并停止；未闭合普通 delimiter 不阻止该 hard stop，保留非 EOF 调用方 boundary |

所有扫描跟踪局部 `()` / `[]` / lambda `{}`、string / interpolation owner；顶层逗号只由 call
list 消费，所属 `)` 只由 call owner 消费，不能退化为按行同步。Lexer poison / terminal event
已表达根因时，只建立 Error value 并抑制同 Span 或同 owner closer 的 parser 级联。L0016
`unsupported argument form` 是保留兼容码；现行 parser 不再产生，catalog 不能复用其编号或
含义。所有仍非法的实参形态均
落入上表六个专用类别或通用 expected closing delimiter。每个 raw lexeme 只由所属 argument
或 list owner 前进一次，单次 call 保持 `O(n)`。
