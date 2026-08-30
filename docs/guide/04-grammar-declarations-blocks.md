# Koven 语言设计规范 · 语法规范（二）：声明与 Block

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第四部分 §7–8），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。现行语法版本：v0.27；
> §11.1 含由 v0.32 引入并在现行 v0.33 沿用的名称绑定解释；§13.5–§13.7 分别同步未启用的 v0.34 receiver、v0.36
> const expression 与 v0.37 iteration 候选，均不改变现行 Parser。
> 保留原节号 §7–8 以维持既有 SPEC 引用不变；共享的表达式/类型引用基础见
> [03-grammar-core.md](./03-grammar-core.md)，调用参数/lambda/解构见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)。

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
                      "(", [ spec_0008_value_parameter,
                              { ",", spec_0008_value_parameter } ], ")",
                      ":", type_ref, [ expression_body ] ;
expression_body      = "=", expression ;
spec_0008_value_parameter = Identifier, ":", type_ref ;

type_parameter_list  = "<", type_parameter,
                       { ",", type_parameter }, ">" ;
type_parameter       = Identifier, [ ":", type_ref ] ;
```

三个 `spec_0008_` 名称只记录 SPEC-0008 已完成的独立入口、简单声明与函数声明子集及其历史
验收边界；实施 SPEC-0012 前，其 `value_parameter` 历史实现仍只接受无标记形态。它们不是
v0.10 中与第 8 节并列的第二套现行入口或函数语法。完整
`standalone_declaration`、`simple_declaration` 与 `function_declaration` 的唯一产生式以
第 8 节为准。SPEC-0008 的历史函数子集要求显式返回标注；SPEC-0011 只迁移其中无体和
block-body 分支的“缺失标注”边界，表达式体分支仍保持显式标注要求。

### 声明形态与分阶段边界

- 普通不可变、可变与常量声明分别以 `val`、`var`、固定的 `const val` 开始；不存在
  `const x = 1` 或 `const var x = 1`。三者都必须有普通 `Identifier` 名称和 `=` 初始化式，
  可以省略类型标注；省略时由 Phase 2 推导。`const val` 初始化式是否可在编译期求值也由
  Phase 2 检查，parser 不按表达式内容提前判定。
- `fun` 只声明具名函数。泛型参数表若存在，位于 `fun` 与函数名之间；参数可以在
  `name: type_ref` 前写一个 `own` / `borrow` / `inout` mode。无标记和显式 `borrow` 都表示
  `Borrow`，显式 `own` 映射既有 `ParameterMode::Value`，显式 `inout` 表示 `Inout`。
  `own` 没有恢复成独立第四契约，也不能出现在调用实参；详见[01-design-decisions.md](./01-design-decisions.md)第 5 节与
  [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节。模式只能在名称之前出现一次，不能写成
  `name: borrow T`。
  SPEC-0008 已完成的历史子集要求函数返回类型显式写成 `: type_ref`；
  SPEC-0011 后，无体函数和 block-body 函数可以省略该标注，省略时精确表示 `Unit`，而
  `= expression` 形式仍必须显式标注。在 SPEC-0009 后，现行独立声明入口按第 8 节接受
  block body。无体函数是否允许由将其放入顶层、接口或其他容器的后续上下文检查。任何
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
  pattern。这里约束的是 declaration shape：SPEC-0010 后 initializer / expression body 可包含
  lambda expression，SPEC-0012 后其中的 call 可包含命名 / 模式实参；不得继续用本条把合法
  子表达式拒绝。局部 `val` 解构只由 SPEC-0013 的 statement dispatch 提交，不改写本独立入口。
- 本节只把参数模式接入当前已有的具名函数声明入口，不借此提前定义构造器、class-family
  成员或 interface 容器。后续 class-family Parser 若适用 guide 未再改变 callable contract，
  其具名函数与构造器参数必须复用同一 `value_parameter` 和 marker 表示；容器合法性仍由对应
  Parser / Phase 2 Spec 检查，不能另造按名称分流的参数语法。
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

SPEC-0008 在复用[03-grammar-core.md](./03-grammar-core.md)第 5 节既有类别外，至少区分下列稳定含义；对应 Spec 分配不与现有目录冲突
的 `L` 码、固定消息和精确主 `Span`：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected declaration | 独立入口首个非 trivia token 不是 `val`、`var`、`const` 或 `fun` 时，消费一个非法起始 token；EOF 处形成空 error root |
| expected `val` after `const` | `const` 后不是 `val` 时在下一个 token（EOF 时为空位置）报告；若下一个 token 是 `Identifier`，不消费并把它继续作为常量名，若是 `var` 则只消费该错误 marker 后继续期待名称，若是 `:`、`=` 或 EOF 则不越过该边界，其他 token 只消费一个后继续期待名称 |
| expected declaration name | 引导词（常量为完整 `const val`）或 `fun` 泛型参数表后缺普通名称时，在 `:`、`=`、`(` 或 EOF 前形成名称 error，不把这些边界冒充名称 |
| expected parameter name | 参数或泛型参数位置缺名称时，恢复到当前层 `:`、`,`、`>`、`)` 或 EOF，不跨嵌套 TypeRef delimiter |
| duplicate parameter mode | value parameter 已消费首个模式后连续出现第二个及以后模式时，每个多余 mode 各发一次本类别，主 `Span` 精确覆盖该多余 token、固定消息为 `duplicate parameter mode`；保留首个 marker，消费多余 mode 后继续期待同一参数名称。SPEC-0012 为本类别分配 `L0039`；函数类型参数复用完全相同的类别与恢复 |
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
  继续沿用[03-grammar-core.md](./03-grammar-core.md)第 5 节的 closer 诊断抑制，不另造 parser 级联。
- 对一段含 `k` 个 lexeme 的恢复，每个 lexeme 至多检查和消费一次，每个 delimiter / owner
  只压栈、弹栈一次；terminal event 按 source offset 预索引并用单调 event cursor 读取。
  因而单段恢复必须是 `O(k)` 时间、`O(d)` 嵌套栈空间，不得从每个 token 重扫诊断、回看
  opener、重启 lexer/parser 或反复切片源码。lexeme / terminal-owner 关系若违反已验证不变量，
  属于 Parser 内部错误，不得降级为用户语法诊断。

上述 list 类别只作用于已提交解析的声明侧 `type_parameter_list` 与 `value_parameter` list。
失败的 `call_type_arguments` 仍须按[03-grammar-core.md](./03-grammar-core.md)第 3 节无副作用回退，不能借这些类别遗留专用 parser
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
value_parameter           = [ explicit_parameter_mode ], Identifier,
                            ":", type_ref ;
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
第 7 节已实现的声明子结构。`local_destructuring_statement` 的唯一产生式见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节，并只在
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
- v0.16 的 `;` 只分隔完整文件的顶层声明，换行和注释在 block 内仍是 trivia；因此 block
  element 不由分号、LF、CRLF 或注释终止。parser 先按适用产生式消费一个**最大合法
  element**：局部声明的 initializer 和
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

SPEC-0009 复用 Lexer 诊断、[03-grammar-core.md](./03-grammar-core.md)第 5 节 expression 诊断、第 7 节变量声明诊断和 expected closing
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
  正在等待 primary 的 initializer 起点必须按[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节提交 lambda，只有一个 element 已经完整结束
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
  为边界；block 内没有分号或换行分隔可供猜测整个未来结构的结束位置，因此不得按行跳过，也不得越过
  当前 owner `}`。遗留 token 随后按允许的最大合法 element 规则解析；可能产生的独立错误
  必须各有真实根因，不能为同一未消费 token 重复发诊断。
- 每次循环要么消费至少一个 raw lexeme，要么在 `}` / EOF 结束；诊断顺序按源码位置稳定。
  对一段 block 输入，每个 lexeme 在 block dispatch 中至多前进一次，嵌套 parser 只处理自己
  拥有的范围，整体保持 `O(n)` 时间和 `O(d)` owner / delimiter 栈空间，不从每个 element
  重启 lexer 或扫描到 block 起点。

缺 block `}` 复用 expected closing delimiter；Lexer 已诊断的未终止 owner 根因继续按[03-grammar-core.md](./03-grammar-core.md)第 5 节
抑制同义 closer 诊断。独立 block 后仍有 token 复用 unexpected trailing token。跨顶层声明、
跨成员和完整文件的同步仍属于 SPEC-0014，不能在本入口把下一个声明关键字当作隐式 EOF。

### Staging、验收与后续拆分

SPEC-0009 的最小验收必须包括：

- pass：空 / 单 element / 多 element block、只靠 token 结构相邻且任意插入 trivia 的 element、
  嵌套空与多层 block、局部 `val` / `var`、expression statement，以及具名函数的无体 / 表达式
  体 / block body 三形态；
- fail：缺 `{` / `}`、不完整局部声明、当前 owner `}` 前缺 initializer、`const val` / 局部
  `fun` / 控制流 / class-family 等 unsupported element、分号作为 unsupported block element、block 用作 expression
  或 lambda；断言稳定错误码、UTF-8 字节 `Span`、error statement 及恢复后的 element 顺序；
- owner 与复杂度：嵌套 block、string / interpolation 中的大括号和 terminal Lexer error 不得
  提前关闭 block；长 element 序列与长错误序列用检查计数或等价白盒证据锁定单调 `O(n)`；
- regression：复跑 SPEC-0007 expression / TypeRef 和 SPEC-0008 declaration / typed-call 全部
  测试，继续锁定表达式体 `=` 的精确 `Span`，并运行 workspace 基线、同步 Architecture。
  验收不以完整文件、控制流、class-family、lambda、名称解析或类型正确性为成功条件。

后续按单一 Goal 拆分：SPEC-0010 至 SPEC-0013 分别实现[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节四项能力，SPEC-0014 再组合
届时已有节点并提供完整文件、声明边界、跨声明恢复与级联抑制；它不是 Phase 1 全部语法的
终点。control-flow 由本文件 §12 补齐；class-family 与接口委托由 v0.20 的本文件 §13
定义，并已分别由 SPEC-0017 与 SPEC-0064 实施。

## 10. SPEC-0014 完整文件、声明分隔与跨声明恢复

```ebnf
source_file = trivia*,
              [ simple_declaration,
                { declaration_separator, simple_declaration },
                [ trivia*, ";" ] ],
              trivia*, EOF ;

declaration_separator = trivia_with_line_break
                      | trivia*, ";", trivia* ;
```

- 空文件合法。文件产物按源码顺序保存零个或多个既有 `ItemId`；`AstFile` 已是文件级容器，
  不增加虚构的根 Item。SPEC-0014 只组合现有 `val`、`var`、`const val`、`fun`，不接纳
  `package` / `import`、control-flow、class-family 或其他尚未定义的顶层产生式。v0.17 的
  SPEC-0015 在本节既有完整文件契约上增量加入文件头，不改写 SPEC-0014 的历史边界。
- `trivia_with_line_break` 是至少包含一个实际 LF / CRLF 的非空 trivia 序列；已终止 block
  comment 内的 LF / CRLF 同样计入，只有 space、tab 或不含换行的注释不构成分隔。声明之间
  必须存在该换行分隔或一个 `;`；因此同一行多个声明必须写 `;`。分隔区域可同时包含换行和
  一个 `;`，最后一个声明后允许一个可选 `;`；文件开头或连续的 `;` 不产生空声明，按非法
  顶层区域恢复。`const val` 仍由同一 constant declaration 消费，不拆成两个声明。
- 上述四个 starter 与 `;` 只在 delimiter stack 为空且 lexical owner 回到文件 baseline 时
  构成恢复用 **soft declaration boundary**。括号、方括号、大括号、string 或 interpolation
  内的同形 token 属于当前声明或错误区，不能提前开始下一声明；EOF 是唯一无条件 hard
  boundary。恢复边界不等于合法分隔：两个同行 starter 之间缺少 `;` 时仍保留后一声明，
  但必须产生 L0047 `expected declaration separator`，主 `Span` 精确覆盖后一声明 starter。
- 文件入口遇到 `val (`、`var (`、`const val (` 继续使用 L0043 和 `Item::Error`，但错误区
  在下一 soft declaration boundary 前结束。其他不能开始既有声明的普通顶层 token 使用
  L0017 `expected declaration`，一次错误区只发一条该诊断并建立覆盖实际消费区的
  `Item::Error`；Lexer 已诊断的 invalid / reserved token 只建立 Error Item，不重复分类。
- 独立声明入口继续以 EOF 为唯一声明 stop，并保留 L0013 `unexpected trailing token` 行为；
  文件入口不得把后续合法声明报告为 trailing token，也不得修改 SPEC-0008–0013 节点内部
  的诊断含义。内部恢复抵达文件 soft boundary 时保留该 starter，交还文件循环。
- 每轮文件循环必须消费一个声明或一个非空错误区，或者抵达 EOF。Lexer terminal owner
  事件、异形 closer 与局部 delimiter 沿用第 7、8 节 owner-aware 规则；同一 Lexer / Parser
  根因不得产生文件级级联。诊断按现有全序确定性合并。
- 对含 `n` 个 lexeme 的文件，文件 dispatch 与跨声明恢复合计必须是 `O(n)` 时间、`O(d)`
  owner / delimiter 栈空间；starter、terminal event 与错误区不得从每个声明重新扫描全文件。

v0.16 的换行 / `;` 规则只改变 `source_file` 顶层声明序列，不把换行或分号提升为通用
expression / block statement separator，也不改变独立声明入口。该增量已由 SPEC-0062 实现
并验收。

SPEC-0014 新增 `parse_file(&SourceMap, &LexedFile) -> ParsedFile`，其中 `ParsedFile` 暴露同源
`SyntaxAst`、有序根 `ItemId` 切片及合并诊断。现有 `parse_expression`、`parse_declaration`、
`parse_block` 的公共行为与返回类型保持不变。

## 11. SPEC-0015 `package` 与 Kotlin 风格 `import` 文件头

```ebnf
source_file = trivia*,
              [ package_directive ],
              { import_directive },
              [ simple_declaration,
                { declaration_separator, simple_declaration },
                [ trivia*, ";" ] ],
              trivia*, EOF ;

package_directive = "package", trivia+, qualified_name ;

import_directive = "import", trivia+, import_target,
                   [ trivia+, "as", trivia+, Identifier ] ;

import_target = qualified_name
              | qualified_name, ".", "*" ;

qualified_name = Identifier, { ".", Identifier } ;

file_header_separator = trivia_with_line_break
                      | trivia*, ";", trivia* ;
```

- 每个文件最多有一个可选 `package` directive。它必须是文件首个非 trivia 构造；省略时
  文件处于默认 package。零个或多个 `import` 必须紧随可选 `package`，并位于任何普通声明
  之前。只有 exact import 可写 `as Identifier`；wildcard import 的 `*` 必须是最后一个
  segment，不能起别名。
- 名称采用一个或多个点分 ASCII Identifier segment。不得有前导点、尾随点、空 segment，
  也不得以硬关键字或 reserved word 冒充 segment。所有 import 都是绝对名称；v1 不提供
  相对 import。Koven 不接受 Rust 风格的 `mod`、`use`、`::`、`self` / `super` / `crate`
  路径或花括号分组导入。
- 两个文件头构造之间、最后一个文件头与首个声明之间，必须出现实际 LF / CRLF 或 `;`。
  最后一个构造若直接抵达 EOF 可省略分隔符。换行判定与第 10 节一致，已终止 block comment
  内换行计入，space、tab 或无换行注释不计；同一行继续写下一个文件构造必须显式写 `;`。
  这项规则不改变 block、expression 或独立声明入口。
- `ParsedFile` 内嵌保存 `Option<PackageDirective>` 与源码有序的 `Vec<ImportDirective>`；普通
  声明继续只通过 `roots: Vec<ItemId>` 暴露。directive、点分 segment、wildcard 和 alias
  都保留真实 `Span`，不创建虚构 Item 或第五张 AST table。Parser 只保存源码结构，不解析
  文件系统路径、source root、package identity、名称绑定、可见性、重复或 wildcard 展开。
- `package` 重复或出现在 import / 声明之后使用 L0051 `misplaced package directive`；声明
  后的 `import` 使用 L0052 `misplaced import directive`。缺 package 名、缺 import target、
  缺 alias 分别使用 L0048–L0050；文件头间缺换行 / `;` 使用 L0053；wildcard import 后写
  alias 使用 L0054。主 `Span` 覆盖首个能确定类别的真实 token；EOF 缺失使用同位置空 Span。
- 文件头 starter 只在 delimiter stack 为空且 lexical owner 回到文件 baseline 时作为 soft
  boundary。恢复必须保留下一合法文件头或声明 starter，不能把 nested string、interpolation
  或 delimiter 内的 `package` / `import` 提升到文件级；一次根因不产生重复文件级诊断。
  文件 dispatch、header 解析和跨构造恢复合计保持 `O(n)` 时间与单调游标。

SPEC-0015 只交付 Phase 1 Lexer / Parser / AST / 诊断与恢复。package 到 source root / 文件的
映射、多文件名称解析、exact / wildcard import 的绑定与冲突规则属于 SPEC-0025；相关长期
映射必须先由 package ADR 决定，不能从当前文件名或相对路径静默推导语义。上述 Phase 1
增量已由 SPEC-0015 实现并验收。

### 11.1 v0.32 绑定解释

本小节不改变上述 grammar、AST 或 Parser 恢复。v0.32 已明确启用；以下解释约束
SPEC-0025：

- exact import 只能绑定可见的顶层类型、顶层值或同 package 同名函数 overload set；alias
  只创建当前文件的本地绑定名；
- wildcard target 必须是 package，只按实际名称查询暴露该 package 的可见顶层声明，不递归
  子 package、不导入 member、不 re-export；
- 同 target/same alias 的 exact 重复是幂等的；不同 exact target 的同名绑定冲突；exact 优先
  于 wildcard，多个 wildcard 的歧义只在实际查询该名称时报告；
- 同文件/同 package 自动可见声明与 exact import 同名时报告冲突；跨 package 的函数不会因
  多个 import 自动组成 overload set；类型和值命名空间分别判定；
- import 和 package 限定名使用绝对路径与最长 package 前缀；package 不是表达式值或运行时
  receiver。完整身份、可见性、诊断和阶段边界见 `01-design-decisions.md` §32。

**v0.32 纯勘误（2026-08-27）**：exact import 的终端严格限于上述顶层声明/函数组；enum case
与 object/companion member 只能在导入/限定顶层 Type 后继续以 `Type.member` 选择，不能写
`import p.Type.member`。该澄清只拆开 §32.4 原先混写的 import 与普通静态限定名，不改变
§32.3、本小节及 v0.32 变更记录已经确定的顶层 import 语义。

## 12. SPEC-0016 control-flow、jump 与 `super`

```ebnf
if_expression = "if", "(", expression, ")", control_body,
                [ "else", (if_expression | control_body) ] ;

when_expression = "when", [ "(", expression, ")" ], "{",
                  { when_entry, when_separator }, "}" ;
when_entry = (when_condition, { ",", when_condition } | "else"),
             "->", control_body ;
when_condition = expression | "is", type_ref | "!is", type_ref
               | "in", expression | "!in", expression ;
when_separator = trivia_with_line_break | trivia*, ";", trivia* ;

while_statement = "while", "(", expression, ")", block ;
for_statement = "for", "(", for_binding, "in", expression, ")", block ;
for_binding = Identifier | "(", for_binding_name,
              { ",", for_binding_name }, ")" ;
for_binding_name = Identifier | "_" ;
loop_statement = "loop", block ;

jump_expression = "return", [ expression ] | "break" | "continue" ;
super_expression = "super", "<", type_ref, ">", ".", Identifier ;
control_body = expression | control_block ;
control_block = "{", control_element*, "}" ;
```

### 12.1 `if` 的 statement / value context

- `if` 是表达式，但缺 `else` 的形态只能作为 block、lambda body 或 control block 中一个
  **完整且最外层的 expression statement**。`val x = if (...) ...`、赋值右侧、实参、运算符
  操作数、`return` 值、lambda 尾值以及任何嵌套 value context 都必须有 `else`；缺失时使用
  L0057 `expected else branch`，在 then body 结束位置产生空主 `Span`，AST 仍保留
  `else = None` 供恢复与工具使用。
- statement context 中缺 `else` 的 `if` 结果固定为 `Unit`。同时存在两条分支时，整体值由
  Phase 2 对两个分支尾值求公共类型；`Nothing` 继续作为 bottom type。Koven 在这一点与
  Kotlin 的位置规则一致，不采用“所有 `if` 都强制 `else`”的更严格变体。
- `else if` 右侧直接嵌套另一个 `if_expression`。条件必须位于 `()`；缺条件用 L0055，缺
  `)` 复用 L0010。缺 then / else body 使用 L0056。普通 block 仍固定为 `Unit`；只有由
  `if` / `when` 明确拥有的 `control_block` 才把最后一个 expression element 作为尾值，空
  control block 或以非表达式 element 结束时值为 `Unit`。

### 12.2 `when`

- 同时支持 `when (subject)` 与无 subject 的 `when { ... }`。有 subject 时，普通表达式条件
  表示与 subject 做相等比较；`is` / `!is` 是类型测试，`in` / `!in` 是包含测试。无 subject
  时每个普通条件必须在 Phase 2 为 `Boolean`，且不接受省略左操作数的四种 subject 条件。
- 同一 entry 的多个条件用 `,` 分组。`else` 必须是唯一条件并位于最后一个 entry；其顺序、
  重复与穷尽性由 Phase 2 检查。`enum class` / Boolean 等已证明穷尽的 value-context `when`
  可省略 `else`；其他 value-context `when` 必须穷尽。Phase 1 保存全部条件与 entry 顺序，
  不伪造类型结论。
- entry 之间必须有实际换行或 `;`；同一行多个 entry 必须写 `;`。缺 entry 用 L0058，缺
  `->` 用 L0059，同行缺分隔用 L0065。entry body 使用 `control_body`，owner-aware 恢复保留
  下一 entry、`else` 或 `}`，不能把嵌套 delimiter/string/interpolation 内的箭头或分隔提升。

### 12.3 loops 与迭代契约

- `while`、`for`、`loop` 是 statement，不进入 Pratt 运算符表；三者 body 都必须是普通
  `{ ... }` block，缺 body 使用 L0060。`while` 条件与 `for` source 只解析表达式，类型约束
  留给 Phase 2。
- `for` 接受一个名称或完整解构 binding；解构中的 `_` 表示丢弃该分量。缺 binding 用
  L0061，缺 `in` 用 L0062。source 只求值一次，语义等价于取得一次 `source.iterator()`，
  再反复调用 `hasNext()` / `next()`；名称绑定、`Iterable<T>` / `Iterator<T>` 的名义检查和
  解构类型检查分别属于 Phase 2/5，不阻塞 Phase 1 保存源码结构。
- `break` / `continue` 只允许控制最近的词法 enclosing loop；不得越过 lambda 或具名函数
  边界。Phase 1 建立 jump AST，Phase 2 负责上下文诊断。v1 不提供 loop label。

### 12.4 `return` 边界

- lambda 与具名函数都是 callable boundary。裸 `return` 永远退出最近的 callable：lambda
  内退出 lambda，具名函数体内退出具名函数；它绝不穿过 lambda 非局部返回外层函数。
- `return expression` 的 expression 必须与当前 callable 返回类型兼容；裸 `return` 返回
  `Unit`。换行结束一个无值 `return`，因此跨行返回值必须显式分组。`return`、`break`、
  `continue` 都是 `Nothing` 类型的 jump expression，可出现在 Elvis 等 value context。
- v1 不定义 `label@`、`return@label`、隐式调用名标签或 inline 函数的非局部返回例外；这些
  拼写必须按现有非法/尾随 token 规则拒绝，不得从 Kotlin 经验补齐。Phase 1 只保存 jump 与
  可选值；目标解析和返回类型检查属于 Phase 2。

### 12.5 `super` 与 Phase 1 AST / 诊断

- `super<Interface>.member` 是 primary/postfix receiver，后续可继续普通 call / member 链。
  `super` 不接受裸用法、`super.member`、`super<T>` 无成员或 Rust 风格路径。缺接口类型使用
  L0063，缺 `.` 使用 L0064，缺成员名使用既有 L0011。它只表示接口默认方法冲突消歧义；
  接口归属、override 与冲突检查属于 SPEC-0017/Phase 2。
- AST 增加 `If`、`When`、`Return`、`Break`、`Continue`、`SuperMember` expression payload，
  以及 `While`、`For`、`Loop` statement payload；control body 以 statement ID 连接并保留
  entry、binding、关键字、分隔符和 delimiter 的真实 `Span`。错误恢复建立显式 Error
  child，不伪造不存在的 token 范围。
- L0055–L0065 的稳定含义依次为 expected condition、expected control body、expected else
  branch、expected when entry、expected when arrow、expected loop body、expected for
  binding、expected `in`、expected super interface、expected super member separator、
  expected when entry separator。Parser 保持单调游标；每个控制结构及恢复对其拥有区域为
  `O(n)`，不得从每个 entry 或 element 重新扫描整个文件。

SPEC-0016 只交付 Phase 1 Parser / AST / 诊断、恢复与 fixture。Boolean 条件、分支公共类型、
`Nothing`、jump target、`when` 穷尽性 / smart cast、迭代协议绑定和接口默认方法检查属于
Phase 2；不得为让 Parser 通过而提前伪造这些语义。

## 13. SPEC-0017 class-family 与后续接口委托

v0.20 封闭 `value class` / `class` / `interface` / `enum class` / 具名 `object` 与
`companion object` 的 v1 表面语法。以下产生式中的普通 token 间允许 trivia；需要实际换行
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

`type_parameter_list`、`function_declaration`、`constant_declaration` 与 `type_ref` 分别复用
第 7 节及[03-grammar-core.md](./03-grammar-core.md)已有结构；不得为成员复制第二套 callable 参数、返回标注或 TypeRef AST。

### 13.1 声明头、构造器字段与修饰符

- `value class` 必须有一个非空主构造器字段列表；普通 `class` 可以省略构造器，等价于可调用
  的空构造器，也可以显式写空 `()`。两者的主构造器参数都必须以 `val` / `var` 声明存储字段，
  不接受未存储参数、callable 的 `own` / `borrow` / `inout` marker、默认值、`vararg` 或 trailing comma。
- 主构造器 `val` / `var` 直接建立存储字段，因此是 v0.26 的专用天然-owned 形态：构造调用向
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
  `own` 是成员声明的修饰符位置；成员函数参数列表内的 `own parameter: T` 仍按第 7 节合法。
- `class`、`value class`、`enum class` 与具名 `object` 的 supertype list 在 Phase 1 保存
  TypeRef 源码顺序；interface 的同形列表表示父接口。Phase 2 必须证明每项都是接口，Koven
  不支持 class implementation inheritance、构造器调用、`by` 之外的 delegation specifier
  或 Kotlin `Base(...)` 基类初始化。

### 13.2 body、enum 变体与成员边界

- class/value class body 只含实例函数和至多一个 companion；interface 只含抽象/默认函数和
  至多一个 companion；enum 在变体区后只含共享实例函数和至多一个 companion；具名 object
  只含实例函数与 `const val`。所有 body 都拒绝普通 `val` / `var`、嵌套 class-family、局部
  类型、getter/setter、constructor/init 和匿名 object。
- 具名 object 同时引入一个名义类型和唯一值；它没有运行时字段或初始化状态，但实例函数可
  执行普通 v1 代码、使用 `this` 并实现接口。companion 与它不同：companion 是纯关联命名
  空间，不能作为值、没有 `this`、不实现接口，也不接受名称。
- companion 可出现在 class/value/enum/interface 中，body 只接受 `const val` 与关联函数。
  `const val` initializer 的编译期求值和允许类型由 Phase 2 检查；关联函数体不受编译期求值
  限制。interface companion 常量以 `Interface.CONSTANT` 访问，不被实现类型继承或 override。
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

### 13.3 接口委托的独立实施边界

`by` 在 Lexer 中始终是普通 Identifier，只在 ordinary class 的 supertype entry 中按拼写
提交 `delegation_clause`。完整契约如下：

- target 必须是同一主构造器的不可变 `val` 字段名称；不接受 `var`、任意表达式或未存储参数；
- delegate 字段持有具体名义类型，Phase 2 必须证明该类型静态满足目标 interface；裸
  interface、`dyn`、反射代理或运行时查找不属于 v1；
- 自动转发完整保持原成员的显式参数模式、类型、返回类型与 `Result` 契约，不插入隐式 `?`
  或异常层；现行 v0.33 尚未定义 instance receiver mode，未启用的 v0.34 §34.3 候选拟把自动
  转发收窄为 Borrow receiver；手写 override 优先，多来源同签名冲突必须显式 override；
- delegate field 的移动、借用和析构与普通 owned field 相同，不获得隐藏共享或生命周期；
- `val/var property by expression` 属性委托明确不支持。

SPEC-0017 只实现不带 `delegation_clause` 的 class-family Parser；SPEC-0064 已增量实现该
子句。生产 Parser 只在 ordinary class supertype entry 中提交 `by Identifier`；其他上下文
或任意 delegate expression 仍以 unsupported class-family form 定向拒绝。

### 13.4 Phase 1 AST、`Span`、诊断与恢复

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
- SPEC-0017 分配 L0066–L0077：expected `class` keyword、expected classifier name、expected
  constructor field、expected constructor separator、expected supertype、expected member、
  expected member separator、expected enum variant、expected enum variant separator、
  expected enum member delimiter、invalid declaration modifier、unsupported class-family form。
  缺通用 `:` / TypeRef / `)` / `}` 继续复用既有稳定类别，不改变旧错误码含义。SPEC-0064
  分配 L0078 `expected delegation target`，覆盖缺失或非法目标 Identifier。
- 文件 soft boundary 增量识别 class-family starter 及其单一 visibility 前缀；body recovery
  只在当前 brace/string/interpolation owner 回到 member baseline 后识别 member/variant starter。
  nested delimiter 内同形 token 不提升。恢复抵达外层 `}`、下一顶层声明或 EOF 时保留 owner
  closer/boundary；Lexer poison 不重复分类，每轮必须消费非空错误区或抵达 stop。
- enum 的 Identifier variant starter、普通 member starter、visibility 前缀与 contextual `by`
  使用有限状态试探；失败试探不分配 AST、不发诊断、不移动正式游标。一个含 n 个 lexeme、
  d 层 delimiter 的 class-family 声明解析与恢复必须为 `O(n)` 时间、`O(d)` owner 空间，不从
  每个 member/variant/type parameter 回扫整个声明。

### 13.5 v0.34 receiver grammar 候选（未启用）

本小节与设计正文 §34 同为候选，不改变现行 v0.33 Parser。只有后续 guide 先明确 v0.34 如何
重基并取代 v0.33、且用户明确启用后，以下
产生式才取代本节开头的 `method_modifiers` 及对应 instance member 产生式：

```ebnf
method_receiver_mode = "borrow" | "inout" | "own" ;
method_modifiers = [ visibility_modifier ], [ "override" ],
                   [ method_receiver_mode ] ;

class_member = method_modifiers, function_declaration
             | declaration_modifiers, companion_object ;
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
  的语言限制由 SPEC-0180 检查，Parser 不从 classifier kind 提前做 Phase 2 判断。
- 本候选不改变普通 callable 参数、调用点 argument marker、function type、extension receiver、
  callable reference 或 safe-call grammar；formatter 与 grammar bridge 必须保存原 token，不能
  把省略形式重写成显式 `borrow`。

### 13.6 v0.36 const expression 解释（候选，未启用）

本小节不新增 Parser 产生式：`constant_declaration` 的 initializer 继续解析普通 `expression`，
由候选设计正文 §36 和 SPEC-0026 在 Phase 2 选择封闭子集。只有用户明确启用 v0.36 后以下
解释才生效；现行版本仍为 v0.32。

- 合法子集为 Boolean/整数/Char/plain String literal、group、const reference（含
  `Object.CONST`/`Type.CONST`）、`+`/`-`/`!`、整数算术/比较/相等、Boolean `&&`/`||` 与
  String concat/equality。Parser 不根据 declaration context 构造第二套 expression AST。
- call、constructor、lambda、assignment、control expression、nullable/postfix、range/`to`、
  container/index 与 interpolation 均保持可恢复的普通 AST，由 Phase 2 在首个非法子表达式
  产生 L0156；不得在 Parser 阶段改报 L0009/L0015 或吞掉后续 sibling member。
- `Object.CONST`/`Type.CONST` 沿用普通 member expression 的 source shape；type/value target、
  visibility和 companion scope 是 typed selection，不以大小写或名称文本在 Parser 中猜测。
- 本候选不改变 `const val` 的合法声明位置、modifier 顺序、member separator、Span 或 owner-aware
  recovery；formatter、TextMate 与 Tree-sitter 无新增 token/grammar 工作。

### 13.7 v0.37 `for` provider 解释（候选，未启用）

本小节不新增 Parser 产生式：`for_statement` / `for_binding`、L0061/L0062、block body 和最近
词法 jump target 继续使用 §12.3 的现有 AST。只有用户明确启用 v0.37 后，以下 Phase 2/3
解释才生效；现行版本仍为 v0.32。

- v0.18 的 `source.iterator()` / `hasNext()` / `next()` 文字不产生 call/member AST，也不触发
  普通名称或 receiver 选择；它在 v0.37 中由 compiler-bound `AcquireProvider` / `HasNext` /
  `NextPlace` / `FinishProvider` 抽象步骤取代。
- `for (name in source)` 与 `for (_ in source)` 的 source 只求值一次；名称是 loop-scoped Borrow
  binding，`_` 不创建 symbol。Parser 不按 source 拼写、大小写或同名 `iterator` 方法猜 provider。
- `for ((a, _, b) in source)` 保留源码顺序和全部 marker；Phase 2 只对 intrinsic sequential
  container 的 concrete value-class element 建立 borrowed field projections。非结构 element
  使用 L0160，字段数不匹配继续使用 L0118；Parser 不改报 L0042 或丢失 sibling statement。
- formatter、TextMate 与 Tree-sitter 无新增 token/grammar 工作；本候选不把 `borrow`、`own`、
  `&` 或 consuming marker 加入 `for_binding`。

SPEC-0017 只交付 Phase 1 Parser/AST/诊断、恢复、复杂度与真实 file fixture；名称重复、
visibility、接口归属、override、enum 穷尽性、object/companion 常量求值和类型/所有权规则
分别属于 Phase 2/3。它不得因语法已保存而把这些后续检查伪装成已实现事实。

---
