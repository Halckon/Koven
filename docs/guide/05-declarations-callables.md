# Koven v0.39：声明与 Callable

> **性质**：规范性语言规范 · **状态**：current（v0.39） · **读取时机**：实现或评审声明、函数签名、参数与返回契约时 · **唯一真源**：本页

本页是现行 Koven v0.39 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## Callable 与函数值

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
- `ParamTypes` 的每一项使用[类型与泛型规则](03-types-generics.md)与[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)的 callable 参数契约：无标记或显式 `borrow` 都是共享借用 `Borrow`；显式 `own`
  映射既有 `ParameterMode::Value`（`Copyable` 则复制、否则移动）；显式 `inout` 是独占可变
  借用。具名函数参数使用同一契约，`(T) -> R` 与 `(borrow T) -> R` 规范化为同一函数类型，
  `(own T) -> R` 则是不同的 Value contract。**调用点是否需要
  书写 `borrow` 由编译器按 callee 已声明的契约自动判定；只有 `Inout` 契约仍要求调用点
  显式标注，但调用点的拼写是符号 `&` 而不是关键字 `inout`**（`&x` 而非 `inout x`），
  Value 调用也保持无 marker：声明写 `own`，调用仍写 `consume(x)`，不写 `consume(own x)`。
  详见[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)的完整规则与设计说明。
- 单态化为具体闭包结构体（捕获环境 + 函数指针），无捕获的函数值直接是裸函数指针，零成本抽象。
- 函数类型可以带 `move` 前缀（`move (ParamTypes) -> ReturnType`），表示只接受不含任何借用捕获的闭包，详见[调用与 Closure](07-calls-lambdas-closures.md)。
- `::globalFunction` 与 `obj::method` 在 Phase 1 只建立未绑定 / 绑定 callable reference AST；
  名称是否存在、可见性、重载选择和最终 callable 类型都由后续名称解析与类型检查判定。

## 函数返回类型

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


### 函数形态与 AST

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

返回来源与 body 的封闭表示见[函数 AST 工程合同](../compiler-specs/parser-ast.md#函数返回来源与-body-封闭表示)。

两个 implicit variant 都不拥有虚构 TypeRef 或 `:` Span；Phase 2 把它们解析为内建 `Unit`。
显式 `: Unit` 保持 `Explicit`，以便工具和诊断忠实反映源码。`FunctionBody` 只嵌在显式分支，
因而类型上不能构造 implicit expression body。省略标注的无体 item Span 到真实 `)` 终，block
body item Span 到 block 终。本页不改变 lambda、函数类型或构造器，不从 body 推导返回
类型，也不放宽 `fun f() = expression`。

## 声明语法

独立声明入口一次解析一个简单声明，并复用现行 `expression`、`type_ref` 与 `block`：

```ebnf
standalone_declaration = trivia*, simple_declaration, trivia*, EOF ;

simple_declaration = variable_declaration
                   | constant_declaration
                   | function_declaration ;

variable_declaration = ( "val" | "var" ), Identifier,
                       [ type_annotation ], "=", expression ;
constant_declaration = "const", "val", Identifier,
                       [ type_annotation ], "=", expression ;
type_annotation      = ":", type_ref ;

function_declaration = "fun", [ type_parameter_list ], Identifier,
                       "(", [ value_parameter,
                               { ",", value_parameter } ], ")",
                       [ ":", type_ref ],
                       [ "=", expression | block ] ;

value_parameter = [ parameter_mode ], Identifier, ":", type_ref ;
parameter_mode = "own" | "borrow" | "inout" ;

type_parameter_list  = "<", type_parameter,
                       { ",", type_parameter }, ">" ;
type_parameter       = Identifier, [ ":", type_ref ] ;
```

若函数使用 `= expression`，返回类型标注必须存在；无体或 block body 函数可省略标注，省略时
精确返回 `Unit`。这些形态共用上面唯一一套 `function_declaration`，不另设历史兼容产生式。

### 声明形态与分阶段边界

- 普通不可变、可变与常量声明分别以 `val`、`var`、固定的 `const val` 开始；不存在
  `const x = 1` 或 `const var x = 1`。三者都必须有普通 `Identifier` 名称和 `=` 初始化式，
  parser 接受省略类型标注并保存完整 initializer。普通 `val` / `var` 的省略标注由 Phase 2
  推导；`const val` 的 Phase 2 规则见[关联常量与封闭求值](#36-无运行时存储的关联常量与封闭求值)。parser 不按表达式
  内容提前判定。
- `fun` 只声明具名函数。泛型参数表若存在，位于 `fun` 与函数名之间；参数可以在
  `name: type_ref` 前写一个 `own` / `borrow` / `inout` mode。无标记和显式 `borrow` 都表示
  `Borrow`，显式 `own` 映射既有 `ParameterMode::Value`，显式 `inout` 表示 `Inout`。
  `own` 没有恢复成独立第四契约，也不能出现在调用实参；详见[class-family 与成员规则](08-class-family-members.md)与
  [调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)。模式只能在名称之前出现一次，不能写成
  `name: borrow T`。
  无体函数和 block-body 函数可以省略返回标注，省略时精确表示 `Unit`；
  `= expression` 形式仍必须显式标注。无体函数是否允许由将其放入顶层、接口或其他容器的上下文检查。任何
  分支都不恢复函数级返回类型推导。
- 已出现的泛型参数表至少包含一个元素；函数参数列表可以是空列表。两类列表一旦包含元素，
  都不接受空项、缺失逗号或 trailing comma。函数参数除上述三个显式 mode 外仍不接受默认值、
  解构、`vararg`、`val` / `var` 或其他模式。这里的无标记 `Borrow` 契约不是“默认参数值”；
  v1 继续完全不支持默认参数值和 `vararg`。
- 类型参数可无上界，也可用单个 `: type_ref` 指定一个内联上界。v1 不接受多上界、默认类型
  实参、`where`、star projection 或声明处 / 使用处型变；重复名称、上界合法性及默认上界
  `Any` 属于 Phase 2。变量与常量声明不接受类型参数表。
- 除 `const val` 这个不可拆分的固定声明前缀外，本入口不接受 `public` / `internal` /
  `private`、`extern`、`operator`、`override` 或软词 `infix` 等修饰符，也不定义其顺序。它不
  解析 extension receiver、匿名函数声明、class-family 成员上下文、控制流或声明自身的解构
  pattern。这里约束的是 declaration shape：initializer / expression body 可包含
  [lambda expression](07-calls-lambdas-closures.md#lambda-literal)，其中的 call 可包含
  [命名 / 模式实参](07-calls-lambdas-closures.md#typed-call-argument)；不得继续用本条把合法
  子表达式拒绝。局部 `val` 解构只由
  [局部解构语法](12-collections-destructuring.md#局部-val-解构语法)的 statement dispatch 提交，
  不改写本独立入口。
- 本节只定义具名函数声明入口，不定义构造器或容器合法性。class-family 成员复用同一
  `value_parameter` 与 marker 表示；容器合法性由对应 Parser 与类型检查器检查，不能另造按名称
  分流的参数语法。
- `{ ... }` 使用[block 与控制流规则](06-blocks-control-flow.md)的同一 block 语法；声明入口不得
  把 body 保存为 opaque 文本。完整文件只组合声明并提供跨声明恢复，不定义第二套 block。
- 独立入口只以 EOF 结束。换行、注释和其他 trivia 不终止声明；`val a = 1\nval b = 2`
  不能作为一个独立声明成功。item / statement 的结构归属由[Block 规则](06-blocks-control-flow.md)确定；
  [完整文件规则](02-names-files-packages.md#完整文件与声明分隔)只负责把已有节点组合为完整文件、
  定义声明分隔及跨声明同步。

### 声明 AST 与 `Span`

索引式 declaration root 与节点引用见[声明 AST 工程合同](../compiler-specs/parser-ast.md#声明-root-与索引引用)。

`ValueParameter` 唯一增加 `mode_marker: Option<ParameterModeMarker>`；使用与函数类型参数
相同的封闭 marker（`Own` / `Borrow` / `Inout` 三项），不增加新的参数 AST table。marker
缺失与显式 `Borrow` 都规范化为 `ParameterMode::Borrow`，显式 `Own` 映射
`ParameterMode::Value`，显式 `Inout` 映射 `ParameterMode::Inout`。参数名、`:` 与 TypeRef
的既有字段不变；恢复出的 missing / error name 也不得丢失此前已消费的合法首个 mode marker。

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
| value parameter | 若有显式模式，从真实 mode token 起，否则从参数名起；到参数 `type_ref` 终；恢复时只到本参数最后实际消费位置，空 TypeRef 插入点不把逗号、`)` 或其前 trivia 纳入范围 |
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

声明规则在复用[词法规则](01-lexical.md)既有类别外，至少区分下列稳定含义；`L` 码、固定消息和精确主
`Span` 由诊断目录统一维护：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected declaration | 独立入口首个非 trivia token 不是 `val`、`var`、`const` 或 `fun` 时，消费一个非法起始 token；EOF 处形成空 error root |
| expected `val` after `const` | `const` 后不是 `val` 时在下一个 token（EOF 时为空位置）报告；若下一个 token 是 `Identifier`，不消费并把它继续作为常量名，若是 `var` 则只消费该错误 marker 后继续期待名称，若是 `:`、`=` 或 EOF 则不越过该边界，其他 token 只消费一个后继续期待名称 |
| expected declaration name | 引导词（常量为完整 `const val`）或 `fun` 泛型参数表后缺普通名称时，在 `:`、`=`、`(` 或 EOF 前形成名称 error，不把这些边界冒充名称 |
| expected parameter name | 参数或泛型参数位置缺名称时，恢复到当前层 `:`、`,`、`>`、`)` 或 EOF，不跨嵌套 TypeRef delimiter |
| duplicate parameter mode | value parameter 已消费首个模式后连续出现第二个及以后模式时，每个多余 mode 各发一次本类别，主 `Span` 精确覆盖该多余 token、固定消息为 `duplicate parameter mode`；保留首个 marker，消费多余 mode 后继续期待同一参数名称。本类别使用 `L0039`；函数类型参数复用完全相同的类别与恢复 |
| expected parameter type separator | value parameter 名称后缺 `:` 时在当前 token 报告（EOF 时为空位置）。若它可开始 `type_ref`，不消费并按插入 `:` 继续解析；若是 `=`，形成空 TypeRef error，再按 unsupported parameter default 的同步方式消费默认值区域但不追加第二条诊断；若是当前层 `,`、`)`、外层 `{`、EOF 或调用方 stop，则不消费并形成空 TypeRef error；其余情况至少消费一个 token，并继续消费到当前层 `,` / `)`、外层 `{`、EOF 或调用方 stop，形成覆盖实际消费区域的 TypeRef error。所有分支均抑制同根因的 expected type reference 与 list 诊断 |
| expected list element | declaration 的 type-parameter list 在 `>` 前没有元素时主 `Span` 取该 `>` 且不消费；type / value parameter 位置直接出现 leading / repeated `,` 时主 `Span` 覆盖并只消费该逗号；随后均从下一项或当前层闭合符继续 |
| expected list separator | 一个完整 type / value parameter 后，下一个 token 可开始同类参数但中间没有 `,` 时，在该 token 报告且不消费，把它继续作为下一项；其他非法 token 恢复到当前层 `,`、`>`、`)` 或 EOF |
| unsupported trailing comma | type / value parameter 的 `,` 后下一个非 trivia token 是当前层 `>` / `)` 时，仅消费并覆盖该逗号，闭合符仍由所属列表消费；空 value-parameter list `()` 本身合法，不属于此类 |
| expected generic closing delimiter | 至少完成一个 type parameter 后，若当前 token 是可作函数名的 `Identifier` 且下一非 trivia token 是 `(`，唯一解释为缺失 `>`：复用 expected closing delimiter 诊断，主 `Span` 是候选名称起点的空位置；不消费候选名称或 `(`，结束 type-parameter list 并让外层从该名称继续；该规则优先于“缺逗号”恢复 |
| unsupported parameter default | 已完整解析 `name: type_ref` 后出现 `=` 时，从 `=` 起按下方统一 owner-aware 扫描规则消费默认值错误区域，直到声明当前层 `,`、`)` 或 EOF 前停止并保留该 delimiter；嵌套 `()` / `[]` / `{}`、string 或 interpolation 内的逗号和右括号不是同步点。即使 `=` 后没有表达式也至少消费 `=`，且不追加 expected expression、expected list separator 或 trailing-token 诊断 |
| expected initializer | 简单值声明缺 `=` 时，若当前 token 可开始 `expression`，不消费并按插入 `=` 继续解析 initializer；若已到 EOF 或调用方声明 stop，则不消费并建立空 Expression error；其余情况至少消费一个 token，再同步消费到 EOF / 调用方声明 stop，建立只覆盖实际消费区域的 Expression error，且不为该区域追加 expected expression 或 unexpected trailing token。已有 `=` 但缺表达式时复用 expected expression error node |
| expected explicit return type | 函数参数列表后缺 `:` 时，若当前 token 可开始 `type_ref`，不消费并按插入 `:` 继续解析；若是 `=`、`{`、EOF 或调用方 stop，则不消费并形成空 TypeRef error；其余情况至少消费一个 token，并同步到 `=`、`{`、EOF 或调用方 stop，形成覆盖实际消费区域的 TypeRef error。`=` 前省略标注仍发本诊断并构造显式 Error TypeRef；明显 `type_ref` 起点前缺 `:` 仍按插入分隔符恢复。`{`、EOF 或调用方无体 stop 直接提交 `ImplicitUnit`，不再发本诊断；其他普通 token 先结束隐式无体函数，再由调用方 trailing / boundary 恢复拥有。已有 `:` 但缺类型继续复用 expected type reference |

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
  继续沿用[词法规则](01-lexical.md)的 closer 诊断抑制，不另造 parser 级联。
- 恢复的扫描、terminal event 索引与复杂度见[声明恢复资源合同](../compiler-specs/parser-algorithms.md#声明恢复资源约束)。
  lexeme / terminal-owner 关系若违反已验证不变量，
  属于 Parser 内部错误，不得降级为用户语法诊断。

上述 list 类别只作用于已提交解析的声明侧 `type_parameter_list` 与 `value_parameter` list。
失败的 `call_type_arguments` 仍须按[类型与泛型规则](03-types-generics.md)无副作用回退，不能借这些类别遗留专用 parser
诊断。参数、泛型参数和 TypeRef 的其他缺失闭合符继续复用 expected closing delimiter。
Lexer 已诊断的 invalid / reserved-word token 仍只消费并放 error node，不在同一 `Span` 重复
parser 诊断。每条恢复路径必须消费输入或抵达明确 delimiter / EOF；本入口不得把换行当同步
点，也不得扫描到下一声明关键字后假称恢复成功。
[完整文件规则](02-names-files-packages.md#完整文件与声明分隔)只增加完整文件组合、声明分隔、
跨声明同步与级联抑制，不重新定义 声明规则/0009 的节点内部恢复。

## 36. 无运行时存储的关联常量与封闭求值

本节在完整继承 v0.35 的基础上启用；不引入通用 CTFE、runtime global、singleton 初始化或
object instance receiver。`constant_declaration` initializer 仍解析普通 expression，Phase 2
选择下述封闭子集；call、constructor、lambda、assignment、control、nullable/postfix、range/`to`、
container/index 与 interpolation 保留可恢复 AST，由 Phase 2 报告 L0156，Parser 不改报
L0009/L0015 或吞掉后续 sibling member。`Object.CONST`/`Type.CONST` 沿用 member expression；
type/value target、visibility 与 companion scope 均由 typed selection 决定。
声明位置、modifier 顺序、separator、Span 与 owner-aware recovery 不变；formatter 和编辑器
语法无新增 token/grammar 工作。

### 36.1 关联命名空间与 target 选择

- 顶层 `const val`、具名 object body 的 `const val` 及 class/value class/enum class/interface
  companion 的 `const val` 是 compile-time declaration identity。companion 仍是匿名关联命名
  空间，不产生 `Type.Companion` 值；常量声明本身没有运行时地址、owner、init guard 或 drop。
- 同文件 `Object.CONST` 与 `Type.CONST` 直接选择关联常量 symbol。具名 object 同时具有
  type/value identity 时，该形态优先解释为 object declaration 的关联常量；它不据此获得
  `Object.instanceMethod()` 能力，普通 object instance call 仍遵循既有 receiver 规则。
- companion 与 instance member scope 不互相注入。companion initializer 不能使用 `this`、实例
  field/member 或 enclosing classifier type parameter；关联泛型函数只使用自身声明的类型参数，
  但关联函数的选择/lowering 不属于常量实施链。
- `private` associated const 仅在 declaring classifier 内可见；interface companion 常量不被
  实现类型继承或 override。不存在 target/member 使用 L0080；非法 companion context 与越界
  访问分别使用 L0153/L0154。

### 36.2 const 类型、值与重新物化

- v1 首轮 const 类型封闭为 `Boolean`、`Byte`/`Short`/`Int`/`Long`、`UByte`/`UShort`/`UInt`/
  `ULong`、`Char` 与 `String`。整数值按声明类型的精确 width/signedness 规范化，`Char` 是 Unicode
  scalar，`String` 是 compiler-owned UTF-8 bytes。其他 builtin、nullable、function、nominal、
  enum、value class、Box/Rc/容器或类型参数使用 L0155；Float/Double 等待后继扩展。
- const declaration 不是普通 variable owner。const initializer 内的依赖只读取 compiler value，
  不产生 runtime value；每个运行时 use 才内联或重新物化：Boolean/整数/Char 直接产生
  Copyable value；String use 从同一 UTF-8 bytes 新建一个普通 String literal temporary owner，
  按[所有权规则](10-ownership-borrowing-drop.md)的 Value/Borrow/return/ASAP drop 规则处理。两个运行时 use 不共享 String owner，也不
  隐式 Rc/retain。
- constant identity 不进入 closure capture environment、loan graph 或 runtime reachability root。
  引用 object/companion 常量不捕获 object/Type，也不产生 singleton 地址或退出析构。

### 36.3 封闭 const expression 与求值失败

- initializer 只接受上述类型的 literal、group、其他 const reference（含 `Type.CONST`）、prefix
  `+`/`-`/`!`、整数 `+ - * / %` 与比较/相等、Boolean `&&`/`||`、String `+`/相等。所有 operand
  仍先按普通 Phase 2 类型规则检查；本列表只决定通过类型检查后是否可在编译期求值。
- 普通 `val`/参数/field/`this`、call、constructor、lambda、assignment、`if`/`when`、Elvis、
  safe call、`!!`、postfix `?`、range/`to`、container/index 与 String interpolation 均不是 const
  expression。编译器不执行“看起来纯”的用户函数；首个非法子表达式使用 L0156。
- 所有常量先收集再建立稳定 dependency graph，因此同文件前向引用合法。self/mutual cycle 每个
  strongly connected component 产生一个 L0157；primary/labels 按稳定 declaration key 与源码
  Span 排序，不依赖 HashMap 或输入遍历顺序。已有 unresolved/Error dependency 不追加 cycle/
  evaluation 级联。
- dependency graph 按语法引用建立：`&&`/`||` 的两个 operand 都先检查类型与 const-expression
  资格，RHS 中的 const reference 即使运行时会短路也形成 edge 并参与 SCC/L0157。实际值求值
  保留 short-circuit；未求值 RHS 不产生 L0158。因此 `false && (1 / 0 == 0)` 不报 L0158，但
  short-circuit RHS 中的非法表达式仍报 L0156，RHS 形成的常量环仍报 L0157。
- 整数 `+/-/*` overflow、`/`/`%` 除零及 signed MIN/-1 在编译期产生 L0158，不生成运行时
  checked operation 或 Abort。literal representability、operand/type mismatch 继续分别使用
  L0090、L0085、L0084，不以 L0158 覆盖既有诊断。

### 36.4 import、分阶段交接与非目标

- 按[名称与 import 规则](02-names-files-packages.md)，exact import 的终端只能是
  可见顶层类型、顶层值或同 package 函数 overload set；enum case、companion/object member
  都不是 import target。v0.37 不改变该既有边界；`import p.Type.CONST` 使用 L0148，应写
  `import p.Type` 后使用 `Type.CONST`，或使用绝对 `p.Type.CONST`。wildcard 同样不导入 member。
- 单文件 Phase 2 发布 associated target、typed ConstValue、依赖图与 use descriptor；不得
  将跨文件事实伪装成单文件结果。Phase 3 消费这些 facts，发布 scalar inline 与 String temporary
  materialization 的 ownership/liveness/drop/capture facts；Phase 4 再消费已验证产物。
- compilation-unit Phase 2 使用稳定 DeclarationId 与 visibility/import facts 复用同一 evaluator，
  发布 const-enabled typed capability；既有基础 validated unit 不因此自动取得常量能力。
  跨文件 ownership/native 必须通过独立后继 Spec 显式消费该 capability 与物化事实，不能通过
  改写已完成基础阶段的验收含义提前接线。

本节不实现 associated function 调用、object instance method、用户可观察常量地址、runtime
global/init、序列化 constant object、跨 compilation-unit ABI 或通用 CTFE VM。Boolean、整数和
String 可复用现有 lowering；Char 必须新增独立的 IR-local Char type/constant contract，由 verifier
验证 Unicode scalar，并映射为 LLVM `i32`，不得擦除成 `UInt32`。这不新增 runtime/global ABI，
仍可复用 ADR-0008 的标量直接传递规则，因此不需要新 ADR；若未来执行用户函数、引入持久
global/init 或稳定跨 object 常量 ABI，则必须另行 guide/ADR。
