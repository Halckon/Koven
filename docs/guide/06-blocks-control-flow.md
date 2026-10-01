# Koven v0.38：Block 与控制流

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审 block、if、when、loop 与 jump 时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## Block 与函数 Body

block 是 statement 的顺序容器，复用声明、表达式和本页控制流产生式：

```ebnf
standalone_block          = trivia*, block, trivia*, EOF ;
standalone_declaration    = trivia*, simple_declaration, trivia*, EOF ;

simple_declaration        = variable_declaration
                          | constant_declaration
                          | function_declaration ;

block                     = "{", { block_element }, "}" ;
block_element             = local_variable_statement
                          | expression_statement
                          | nested_block_statement
                          | while_statement
                          | for_statement
                          | loop_statement ;

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

产生式中的 `{ block_element }` 重复由换行（LF / CRLF）或显式分号 `;` 分隔；在不存在换行续行条件时，行末换行自然开启下一个 element。在同一行内连续书写多个 element 时必须显式使用 `;` 分隔。

这里的 `/* empty */` 是 EBNF 记号，不是要求源码包含注释。独立 block 入口一次只解析一个
block 并要求 EOF；函数 block body 由本节完整 `standalone_declaration` 入口解析，并复用
声明子结构。`local_destructuring_statement` 的唯一产生式见[集合、索引与解构规则](12-collections-destructuring.md)。
两种独立入口都不组合完整文件，也不以换行寻找下一
顶层声明。函数后缀使用互斥产生式，而不是两个独立 optional 字段，以免误接受省略返回标注
的表达式体。

### Block Element、结构边界与值语义

- block element 精确包括局部 `val` / `var`、expression（含 `if` / `when` / jump / `super`）、
  嵌套 block 和 `while` / `for` / `loop`。`const val`、局部 `fun` 与 class-family 声明不是
  block element；parser 必须定向拒绝，不能保存为 opaque token 或当作 identifier expression。
- block element 之间使用**显式分号 `;`** 或**语法换行（LF / CRLF）**进行分隔。换行默认作为
  当前 element 的终止边界，并允许下一个 expression-start token 开启新的 expression statement。
  因此，分属不同行的 `println(a)` 与 `println(b)` 是两条合法、连续的 expression statement。
- **续行规则（Line Continuation）**：换行在以下情形中被视为连续 trivia，不终止当前表达式或声明：
  1. 行末停在未完成的二元运算符（如 `+`、`-`、`*`、`/`、`%`、`&&`、`||`、`to`、`and`、`or`、`xor`、
     `shl`、`shr`、`ushr`、`as`、`as?`、`is`、`!is`、`in`、`!in`、`..`、`..<`）、复合赋值操作符、
     赋值号 `=`、逗号 `,`、类型标注前缀 `:` 或未闭合的分隔符（`(`、`[`、`${`）；
  2. 下一行首个非 trivia token 只能作为中缀/后缀延续当前表达式（例如 `.`、`?.`、`?:`、`else`、
     中缀关键字）。
  在满足上述续行条件时，表达式跨多行完整解析；不满足续行条件时，行末换行结束当前 element。
- `;` 在块内是合法的显式语句分隔符。同一行内书写多条语句时必须用 `;` 分隔（如 `{ val x = 1; val y = 2 }`
  或 `{ foo(); bar() }`）；行末的 `;` 也是合法的可选终止符。若同一行内连续出现两个普通
  expression-start token 且中间既无换行也无分号（如 `{ x y }`），parser 报告缺少语句分隔符诊断，
  而不是无说明地当作尾随未知 token。
- parser 先按适用产生式消费当前 element。当前 block owner 的 `}` 始终是 hard stop；
  在没有续行的情况下，换行、`;`、`val`、`var` 以及循环引导关键字均作为当前 element 的边界，
  将控制权归还给 block dispatch 开始下一局部声明或语句。`{` 只有在左侧最大 expression 已完整、
  parser 不再等待 operand 时才是下一 nested block 的 soft stop；正在等待 primary 时，必须按
  [Lambda 规则](07-calls-lambdas-closures.md#lambda-literal)把 `{` 解析为 lambda。
- `{}` 是合法空 block；`{{}}` 是包含一个 nested block statement 的合法 block。block dispatch
  在 element 起点直接看到 `{` 时提交 nested block；只有 expression parser 正在等待 primary
  时，同一个 token 才按 [Lambda 规则](07-calls-lambdas-closures.md#lambda-literal)提交 lambda。每个 `{`
  都建立独立 block owner，由对应层的 `}` 关闭。block 不是 expression primary；`f({})`、
  `val x = {}` 或把 block 用在二元运算符任一侧，不能作为 block expression 成功。lambda
  字面量由[Lambda 规则](07-calls-lambdas-closures.md#lambda-literal)定义，不能反向改变本节
  大括号的归属。
- block 是顺序执行容器，静态类型为 `Unit`，**不产生值，也没有尾表达式特例**。包括最后
  一项在内的 expression statement 结果均按 `Unit` 语境丢弃；`{ 1 }` 不是值为 `1` 的
  expression。函数的显式 `return` 与路径检查使用本页后续规则；block 本身仍固定为 `Unit`。
- 每个 block 建立词法作用域。局部 `val` / `var` 从其声明完成后开始对同一 block 的后续
  element 及其嵌套 block 可见，不在自身 initializer 中可见，也不在声明前可见；嵌套 block
  中声明的名称离开该 block 后不可见。重名、shadowing、引用解析和 use-before-declaration
  的具体诊断属于名称与类型检查；parser 必须保留 element 顺序，不能重排声明。

### 函数 Body 三形态

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

### Statement AST、Body 表示与 `Span`

block AST 使用有 payload 的 statement table；所有 block element 都以有序
`StatementId` 保存，不把 element 混入 expression table，也不创建无 `Span` 的源码字符串：

- `Statement::Block { elements: Vec<StatementId> }` 表示独立、嵌套或函数体 block；
- `Statement::LocalVariable { declaration: ItemId }` 引用按[声明规则](05-declarations-callables.md)构造的 `val` /
  `var` item，禁止引用 `const val` 或 `fun` item；
- `Statement::Expression { expression: ExpressionId }` 引用既有 expression；
- `Statement::Error` 只覆盖本次实际消费的错误区域。

实现可采用可证明同样保持 typed ID、顺序与下述范围的等价枚举命名，但不能把 block 降为
`Vec<ExpressionId>`。函数 item 必须用一个合并的封闭 sum type 同时保存返回标注来源与 body，
至少等价于 `ImplicitUnitAbsent | ImplicitUnitBlock(StatementId) | Explicit {
colon_span: Span, type_ref: TypeRefId, body: FunctionBody }`；其中显式分支的 `FunctionBody` 才可为
`Absent | Expression { equals_span: Span, expression: ExpressionId } | Block(StatementId)`。
表达式体必须继续精确保存[声明规则](05-declarations-callables.md)规定的真实 `=` token `Span`。
不得把返回标注和 body
暴露为可独立构造的字段，不能制造 `ImplicitUnit + Expression`、“双 body”或“半个显式标注”
状态，也不得为隐式 `Unit` 伪造 TypeRef / `:` Span。独立 block 入口返回带 `SourceId` 的
statement root。

| 节点 | 合成范围 |
|---|---|
| 完整 block / `Statement::Block` | 从真实 `{` 起至匹配 `}` 终；缺 `}` 时到该 block 最后实际消费位置，空 block 仍覆盖真实 `{}` |
| local-variable statement | 与其引用的[变量声明](05-declarations-callables.md)范围完全相同 |
| expression statement | 与其引用的 expression 范围完全相同 |
| error statement | 只覆盖实际消费的错误区域；没有消费 token 时只能是在 owner closer / EOF 的空范围，且不得插入会使循环停滞的零宽 error element |
| 隐式 `Unit` 返回标注 | 不单独拥有虚构源码范围；无体 `fun` item 到真实 `)` 终，block-body item 继续到 block 终 |
| 显式返回标注 | 从真实（或缺分隔符恢复时的空）`:` Span 起到真实 / Error TypeRef 终；显式 `: Unit` 不折叠为隐式状态 |
| expression function body 的 `equals_span` | 精确覆盖真实 `=` token；恢复不得伪造或扩张该范围 |
| 有 block body 的 `fun` item | 从真实 `fun` 起至 body block 的真实 `}` 终；缺 `}` 时至 body 最后实际消费位置 |

所有范围继续是同一 `SourceId` 的 UTF-8 字节半开区间。嵌套 block 的范围不得吞入父 block 的
`}`；缺失 closer 不得用 EOF 之外的虚构字符扩展范围。

### Block 诊断、恢复与 Owner 边界

block 规则复用 Lexer 诊断、[表达式规则](04-expressions-operators.md)诊断、声明诊断和 expected closing
delimiter，并至少增加下列稳定错误类别；具体 `L` 码和固定消息由诊断注册表统一维护：

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
  element 已完整结束之后，是下一 element 的结构边界。对 `{`，正在等待 primary 的 initializer
  起点必须按[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)提交 lambda；只有一个 element 已经完整结束
  且 parser 不再等待 operand 时，顶层 `{` 才作为下一 nested block 的结构边界。若在 owner
  `}` 前没有可消费 expression，同样复用 expected expression，不再追加同根因的 expected
  block element；局部 `val` / `var`
  的名称、类型、`=` 与 initializer 恢复继续复用[声明规则](05-declarations-callables.md)类别，但 stop 集合增加这些 element
  边界和当前 block 的 `}`。普通 Identifier 或字面量仍能开始 initializer，不能仅因它也可能
  开始下一 expression statement 就提前停止。
- Lexer invalid / reserved token 已有诊断时，parser 消费它并形成 error statement，不在相同
  `Span` 追加 block 诊断。string / interpolation 内的 `{`、`}` 和 element 起始 token 归
  lexical owner；恢复必须复用[声明恢复规则](05-declarations-callables.md)预索引的 `L0004`–`L0006` terminal-owner 关系，不能把
  `InterpolationEnd` 当作 block closer，也不能在每个 block error 处重扫 Lexer 诊断。
- unsupported / expected element 的最小恢复以“消费确定的错误引导 token 或错误 token”
  为边界；block 内没有分号或换行分隔可供猜测整个未来结构的结束位置，因此不得按行跳过，也不得越过
  当前 owner `}`。遗留 token 随后按允许的最大合法 element 规则解析；可能产生的独立错误
  必须各有真实根因，不能为同一未消费 token 重复发诊断。
- 每次循环要么消费至少一个 raw lexeme，要么在 `}` / EOF 结束；诊断顺序按源码位置稳定。
  对一段 block 输入，每个 lexeme 在 block dispatch 中至多前进一次，嵌套 parser 只处理自己
  拥有的范围，整体保持 `O(n)` 时间和 `O(d)` owner / delimiter 栈空间，不从每个 element
  重启 lexer 或扫描到 block 起点。

缺 block `}` 复用 expected closing delimiter；Lexer 已诊断的未终止 owner 根因继续按[词法规则](01-lexical.md)
抑制同义 closer 诊断。独立 block 后仍有 token 复用 unexpected trailing token。跨顶层声明、
跨成员和完整文件的同步仍属于
[完整文件规则](02-names-files-packages.md#完整文件与声明分隔)，不能在本入口把下一个声明
关键字当作隐式 EOF。

## 控制流语法与语义

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
control_element = block_element ;
```

### `if` 的 Statement/Value Context

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

### `when`

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

### Loop 与迭代契约

- `while`、`for`、`loop` 是 statement，不进入 Pratt 运算符表；三者 body 都必须是普通
  `{ ... }` block，缺 body 使用 L0060。`while` 条件与 `for` source 只解析表达式，类型约束
  留给 Phase 2。
- `for` 接受一个名称或完整解构 binding，单名称 `_` 与解构中的 `_` 均为 discard。缺 binding
  用 L0061，缺 `in` 用 L0062。Parser 保留源码顺序和完整 marker，只保存 source、binding 与 body。
  provider、Borrow binding 和借用式 value-class projection 由
  [顺序迭代规则](12-collections-destructuring.md#37-借用式顺序容器迭代-provider)定义；
  不新增 call/member AST，不按 `iterator()` / `hasNext()` / `next()` 拼写猜测协议。
  `for_binding` 不增加 `borrow`、`own`、`&` 或 consuming marker。
- `break` / `continue` 只允许控制最近的词法 enclosing loop；不得越过 lambda 或具名函数
  边界。Phase 1 建立 jump AST，Phase 2 负责上下文诊断。v1 不提供 loop label。

### `return` 边界

- lambda 与具名函数都是 callable boundary。裸 `return` 永远退出最近的 callable：lambda
  内退出 lambda，具名函数体内退出具名函数；它绝不穿过 lambda 非局部返回外层函数。
- `return expression` 的 expression 必须与当前 callable 返回类型兼容；裸 `return` 返回
  `Unit`。换行结束一个无值 `return`，因此跨行返回值必须显式分组。`return`、`break`、
  `continue` 都是 `Nothing` 类型的 jump expression，可出现在 Elvis 等 value context。
- v1 不定义 `label@`、`return@label`、隐式调用名标签或 inline 函数的非局部返回例外；这些
  拼写必须按现有非法/尾随 token 规则拒绝，不得从 Kotlin 经验补齐。Phase 1 只保存 jump 与
  可选值；目标解析和返回类型检查属于 Phase 2。

### `super` 与静态调用结构

- `super<Interface>.member` 是 primary/postfix receiver，后续可继续普通 call / member 链。
  `super` 不接受裸用法、`super.member`、`super<T>` 无成员或 Rust 风格路径。缺接口类型使用
  L0063，缺 `.` 使用 L0064，缺成员名使用既有 L0011。它只表示接口默认方法冲突消歧义；
  接口归属、override 与冲突检查属于
  [class-family 与成员规则](08-class-family-members.md)和类型检查。
- AST 增加 `If`、`When`、`Return`、`Break`、`Continue`、`SuperMember` expression payload，
  以及 `While`、`For`、`Loop` statement payload；control body 以 statement ID 连接并保留
  entry、binding、关键字、分隔符和 delimiter 的真实 `Span`。错误恢复建立显式 Error
  child，不伪造不存在的 token 范围。
- L0055–L0065 的稳定含义依次为 expected condition、expected control body、expected else
  branch、expected when entry、expected when arrow、expected loop body、expected for
  binding、expected `in`、expected super interface、expected super member separator、
  expected when entry separator。Parser 保持单调游标；每个控制结构及恢复对其拥有区域为
  `O(n)`，不得从每个 entry 或 element 重新扫描整个文件。

Parser 只保存控制结构、顺序和恢复节点；Boolean 条件、分支公共类型、`Nothing`、jump target、
`when` 穷尽性 / smart cast、迭代协议绑定和接口默认方法由类型及后续语义检查完成，不得从
AST 形状提前伪造结论。

---

## `when` 穷尽性与 Smart Cast

### Enum Case 的双重身份

- `enum class E { C(...), D }` 中每个 case 同时声明同拼写的值构造器和嵌套 case type；二者
  共享一个 `EnumCaseId`，分别进入 `E` 的值/类型命名空间。case type 不是可独立实现
  interface 的普通 classifier，也不能出现在 supertype、泛型实参或公开签名中；只允许作为
  `is` / `!is` 的目标和 smart-cast 后的内部流类型。其 runtime 公共类型始终是 `E<...>`；
  其他显式 TypeRef 位置使用 L0114，而不是把 case type 当作 root enum 的别名。
- enum 本体作用域内可写短名 `C`；外部源码必须写限定名 `E.C`。同一限定拼写在值位置表示
  case value/constructor，在 `is` / `!is` 的目标位置表示 case type。名称阶段解析完整限定链，
  不允许把“首段已解析、尾段 deferred”伪装为成功；跨 package 的前缀展开仍后置 跨文件名称规则。
- case payload 字段属于对应 case type。enum 自身方法内的裸 `radius` 是“隐式 `this` 的
  case payload 候选”，名称阶段保留候选而不提前报 unresolved；只有当前流事实唯一证明
  `this` 为声明该字段的 case 时才能取其类型。`this.radius` 遵循相同规则。没有该事实、多个
  case 同名字段无法唯一选择或在 enum 外裸用时产生 L0113。
- 普通 class/value class/object/interface 不因此获得继承或 runtime tag；v1 的 `is` 不提供
  任意 RTTI，也不能用 interface、类型参数或 `Any` 对未知具体类型作动态探测。

### 类型测试与流事实

- 合法 `e is T` / `e !is T` 的结果固定为 `Boolean`。v1 的有效测试关系仅包括：同一 enum
  root 与其 case、同一已知 nominal 的 nullable/non-null 分离，以及已经静态相同的具体类型；
  interface、类型参数、无关 nominal 和需要运行时泛型反射的测试使用 L0106。`as` / `as?`
  仍不属于 when 类型规则。
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

### `when` 条件、覆盖域与重复

- subjectless `when` 的普通条件必须是 `Boolean`；类型/包含条件仍为语法错误恢复产物，类型
  阶段使用 L0107。subjectful 普通条件按 `subject == condition` 检查可比较性；`in` / `!in`
  的协议选择由[class-family 与调用规则](08-class-family-members.md)处理，在此保持专用
  deferred，不据此证明穷尽。
- 编译器只对有限且封闭的域证明无 `else` 穷尽：`Boolean` 的 `{true,false}`、enum root 的全部
  case，以及它们的 nullable 形式（额外包含 `null`）。泛型类型参数、普通 class、整数、
  String、interface、`Any` 和 subjectless predicate 集合都不是封闭域。
- enum case 的正 `is` 覆盖该 case，`!is` 覆盖当前有限域的补集；`null`、Boolean literal 和
  enum 无 payload case 的等值条件可贡献单点覆盖。一个条件对当前剩余域不增加覆盖时产生
  L0110；poisoned/未知条件不参与覆盖，也不制造后续重复诊断。
- `else` 最多一次且必须是最后一个 entry；重复使用 L0108，非末尾使用 L0109。即使前面已
  穷尽，显式末尾 `else` 仍允许，作为未来兼容兜底，不报冗余。

### Value/Statement Context 与分支类型

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

### 诊断

| 错误码 | 含义 | 主范围与关联信息 |
|---|---|---|
| L0106 | `is` / `!is` 目标不可运行时判定或与被测类型无合法关系 | primary 为运算符；label 指向目标 TypeRef |
| L0107 | `when` 条件形态或类型与有/无 subject 规则不匹配 | primary 为条件；label 指向 subject |
| L0108 | 同一 `when` 出现多个 `else` | primary 为后出现的 `else`；label 指向第一个 |
| L0109 | `else` 不是最后一个 entry | primary 为 `else`；label 指向后续首 entry |
| L0110 | 有限域中的条件不增加任何新覆盖 | primary 为该条件；label 指向首次覆盖来源 |
| L0111 | value-context `when` 未覆盖封闭域或无法证明穷尽 | primary 为 `when`；labels 按声明顺序列出遗漏 case，非封闭域建议添加 `else` |
| L0112 | 无 expected type 的可达分支无法形成合法公共类型 | primary 为后出现分支尾值；label 指向首个冲突分支 |
| L0113 | enum case payload 在当前流事实下不可唯一访问 | primary 为字段名称；labels 指向候选 case 声明 |
| L0114 | enum case type 出现在 `is` / `!is` 目标之外的显式 TypeRef 位置 | primary 为 case TypeRef；label 指向 root enum 声明 |

L0106/L0107 已使条件 poisoned 后，不追加同条件的 L0110；L0108/L0109 不阻止仍可确定的
entry body 类型检查；L0111 只产生一条并聚合遗漏项。when 类型规则 不顺带实现一般 member/call
选择、`as`、包含协议、所有权或跨文件 sealed hierarchy。
