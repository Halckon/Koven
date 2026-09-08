# Koven 语言设计规范 · 语法规范（一）：表达式与类型引用基础

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第四部分 §1–6），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。内容版本：v0.27。
> 原第四部分体量过大，本次拆分为三份，均保留原节号以维持既有 SPEC 引用与 Span 表述
> 不变：本文档（§1–6）覆盖 primary/postfix/`type_ref`/运算符优先级/Lexer 错误交接/AST
> `Span` 规则，是后续两份的共享基础；[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)（§7–8）覆盖
> SPEC-0008、SPEC-0009；[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)（§9）覆盖 SPEC-0010 至 SPEC-0013。

本文档（原第四部分）是 Phase 1 parser 的可执行边界。产生式中的终结符引用
[02-lexical-spec.md](./02-lexical-spec.md)的 token 分类；parser
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
                   | lambda_expression
                   | if_expression | when_expression | jump_expression
                   | super_expression ;

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
[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节；control-flow primary 见
[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)第 12 节。SPEC-0007 的历史子集不含 lambda 或 control-flow，不能用当前
产生式反写其已完成验收事实。

v0.20 的 `object` 只存在于声明语法，不是 primary expression；v1 不接受匿名内部类或
`object { ... }` expression。lambda 只产生函数类型值，不能借此匿名实现多方法 interface。

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
               | "?"
               | "::", Identifier ;

call_suffix       = "(", [ call_argument,
                            { ",", call_argument } ], ")" ;
typed_call_suffix = call_type_arguments, call_suffix ;
call_type_arguments = "<", type_ref, { ",", type_ref }, ">" ;
index_suffix      = "[", expression, "]" ;
```

所有 suffix 同属最高优先级，从左到右逐个包裹 receiver；例如 `a!!.b(c)[i]::ref!!` 是一条
确定的左结合链，`!!` 与 `?` 均可重复。`.`、`?.` 和有 receiver 的 `::` 后必须是普通
`Identifier`，不能用硬关键字或 reserved-word token 冒充名称。无 receiver 的 `::name`
属于 primary，有 receiver 的 `value::name` 属于 postfix。两类引用在此阶段只产生 AST，名称
解析、可见性与 callable 类型均属后续阶段。`e!!` 的既有语义不变，仍脱糖为
`e ?: error("Non-null assertion failed")`。`e?` 建立 `Propagate { value, question_span }`，
其 `Result<T, E>` 与最近 callable 约束见[01-design-decisions.md](./01-design-decisions.md)第 19 节；Phase 1 不做类型或上下文拒绝。
Lexer 的 `?`、`?.`、`?:` 是三个最长匹配 token，parser 不拆分后两者。因而 `result?` 是
propagate，`result?.member` 是 safe member，`result ?: fallback` 是 Elvis；`result??` 是
两个左结合 propagate 节点，最终类型是否合法由 Phase 2 判断。传播后立即做普通成员访问时
统一写 `(result?).member`；不得用空白敏感的 token 重解释把 `result?.member` 当作传播。

`call_argument` 在本版中的完整产生式见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节；basic call 与 typed call 复用同一实参语法，允许空
参数列表但不允许 trailing comma。SPEC-0007 完成时只支持位置实参并以 L0016 拒绝命名 / 模式
实参；实施 SPEC-0012 后，本节四种合法组合迁移为 typed argument，显式分组的
`f((a = b))` 仍是位置 assignment expression。

索引后缀恰好包含一个表达式：不允许 `a[]`、`a[x, y]` 或 trailing comma。`arr[1..3]`
语法上是以 range 表达式为唯一 key 的普通索引；它不产生切片 AST，类型及 v2 切片边界见
[01-design-decisions.md](./01-design-decisions.md)第 8 节。

## 3. `type_ref` 最小语法

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
`() -> T?` 唯一表示“返回 `T?` 的函数”，不表示可空函数值。本版暂不提供可空函数类型的
写法，也不新增类型分组语法来绕过该边界。v1 不支持 star projection、声明处或使用处型变；
不得把 `*`、`in T` 或 `out T` 塞入类型实参。在 `type_ref` 语法中，`move` 只可作为函数
类型前缀；表达式位置的 `move { ... }` lambda 见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节。

函数类型的每个参数都携带与具名函数 `value_parameter` 相同的三种**语义契约**：无标记或
显式 `borrow` 都表示 `Borrow`，显式 `own` 表示既有 `ParameterMode::Value`，显式 `inout`
表示 `Inout`。v0.26 恢复的是 `Value` 的声明端 `own` 拼写，不是 v0.10/v0.11 那个还要求
调用点写 `own` 的独立第四契约。`Value` 接收完整 owned value：结果满足 `Copyable` 时复制，
否则移动；`Borrow` 表示调用期间的共享只读借用，`Inout` 表示独占可变借用。

参数 mode 在函数类型身份中按上述语义先规范化：`(T) -> R` 与 `(borrow T) -> R` 是同一
函数类型，不能仅凭是否写出 `borrow` 形成 overload 或让 override 不匹配；`(own T) -> R`
与 `(inout T) -> R` 分别编码 `Value` 与 `Inout`，和 Borrow 函数类型不同。AST 仍保留显式
`borrow` 的真实 token / `Span`，以便 formatter 与诊断忠实反映源码，但该表面差异不进入
typed contract。

**调用点标注是否强制**是另一个独立问题，由[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节统一定义：`Value` 参数虽然
必须在声明端写 `own`，调用点仍不写 mode；向它传入 `MoveOnly` place 时无标记调用隐式
移动，传入 `Copyable` place 时交付 owned copy。`Borrow` 参数同样默认不写 mode，调用点
仍可选择写 `borrow` 强调；只有 `Inout` 参数强制要求调用点写符号 `&`（`&x`，不是关键字
`inout`）。调用点 `own x` 不属于语法。这条规则同时适用于具名函数
调用与函数类型值的调用，详见[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节“调用点自动化的设计说明”。

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
就因 `4` 不是 `type_ref` 而拒绝。该边界为[01-design-decisions.md](./01-design-decisions.md)第 8 节所述未来 `Array<T, N>` 保留，
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
| 1 | `.` `?.` `()` `[]` postfix `!!`、postfix `?`、bound `::name` | 左结合，可连续 |
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

`own` / `borrow` / `inout` 不属于通用 prefix 层级；它们只在第 3、7、8 节明确给出的函数
类型参数与具名值参数声明位置出现。v0.26 的 `own` 只映射声明端 `Value`，不得据此接受
调用点 `own expression`。SPEC-0012 调用实参
专用入口自 v0.14 起改用符号 `&` 表达 `Inout` 调用点标注，`inout` 关键字不再出现在调用
实参位置（`borrow` 关键字仍可在调用实参位置可选出现，语义与省略标注相同）；`&` 同样
不属于通用 prefix 层级，只在[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节给出的调用实参入口
合法，v1 不提供其他表达式位置的 `&` 用法，详见该节说明。无 trivia 相邻的 `++`、`--`、
`<<`、`>>` 和 `...` 必须由 parser 整体识别并报告
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
| unsupported argument form | SPEC-0007 的历史类别；SPEC-0012 后生产 parser 不再产生 L0016，错误码目录因已发布而保留但不得复用或改变含义；仍非法的实参形态使用[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节专用类别 |

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
| postfix `?` | receiver 起点至真实 `?` 终点；保存 `question_span`，不反查或扩张既有 child |
| string | 开始引号起点至结束引号终点；恢复时至该字符串最后消费位置 |
| interpolation | `${` 起点至匹配 `}` 终点；恢复时至该插值最后消费位置 |
| error | 覆盖本次实际消费的错误区域；只有位于 stop token / EOF 且没有可消费 token 时可以为空 |

TypeRef 节点遵循以下唯一合成规则：

| 节点 | 合成范围 |
|---|---|
| qualified type | 从首段 `Identifier` 起，到末段 `Identifier` 终；若有 type arguments，则改为到匹配 `>` 终；若再有 nullable `?`，最终到该 `?` 终 |
| function type | 若有 `move`，从 `move` 起，否则从 `(` 起；到 return `type_ref` 终；参数模式已包含在各参数完整范围内 |
| function parameter | 若有模式，从真实 `own` / `borrow` / `inout` 起，否则从参数 `type_ref` 起；到参数 `type_ref` 终 |
| type arguments | 从 `<` 起到匹配 `>` 终；若实现不为它单建节点，该范围仍完整纳入所属 qualified type |
| TypeRef error | 与通用 error 相同：覆盖实际消费的错误区域；只有位于 stop token / delimiter / EOF 且没有可消费 token 时可以为空 |

函数类型 AST 的 `parameters` 唯一改为源码顺序的 `Vec<FunctionTypeParameter>`；每项至少保存
`span`、`mode_marker: Option<ParameterModeMarker>` 与唯一 `type_ref: TypeRefId`。marker 缺失
表示 `Borrow`；显式 marker 使用封闭的
`ParameterModeMarker::{Own(Span), Borrow(Span), Inout(Span)}`，不得用多个独立 `Option`
制造“有模式无 Span”或“有 Span 无模式”的半状态。语义层唯一映射为
`None | Some(Borrow) -> ParameterMode::Borrow`、`Some(Own) -> ParameterMode::Value`、
`Some(Inout) -> ParameterMode::Inout`。参数名不进入函数类型 AST；类型身份比较规范化后的
语义 mode，而不是比较 `None` 与显式 `Borrow` 的源码形态。

所有空 Error TypeRef / Expression 都位于下一 non-trivia boundary token 的起点（EOF 则为
EOF offset），但**零宽插入范围不扩大父节点**。父函数类型参数、value parameter 或
CallArgument 的结束位置只取本构造最后实际消费的非 trivia / invalid token 终点；若没有
这样的 token，父节点才可与空 error 同为空范围。因而 `(borrow /*c*/ ,) -> R` 的空 TypeRef
位于逗号起点，而参数 Span 仍只到真实 `borrow` 终；`fun f(borrow x: /*c*/ ,)` 只到真实 `:` 终，
`f(name = /*c*/ )` 的 argument 只到真实 `=` 终。trivia 本身既不扩张父 Span，也不被伪装成
错误 token；child 的零宽插入点可以位于 parent 半开范围终点之后，这是恢复元数据，不表示
父节点消费了中间 trivia 或 boundary。

缺失泛型 `>`、函数参数 `)`、`->` 后返回类型或其他 TypeRef delimiter 时，恢复节点止于本构造
最后实际消费位置，不得越过外层 stop token 或 delimiter。所有表达式与 TypeRef 合成范围都
不得用不存在的 token 伪造超出已消费输入的坐标。

函数类型参数模式后的恢复复用现有 expected type reference：`own` / `borrow` / `inout` 后若遇当前参数 `,`、所属
`)`、调用方 TypeRef stop 或 EOF，则在该边界建立空 TypeRef error 并保留边界；遇其他不能
开始 `type_ref` 的 token 时消费到同一组边界前，Error 只覆盖实际消费区域。连续第二个模式
不是新的参数，而是当前参数的错误区域：保留首个模式，消费后续连续模式，并对每个多余
token 发 `duplicate parameter mode`；主 `Span` 精确覆盖该多余 mode token，固定消息为
`duplicate parameter mode`。该类别由 SPEC-0012 在现有 L0032 后分配 `L0039`，与调用实参
自己的 duplicate-mode 类别不同。本版不为函数类型引入空项、缺 separator 或 trailing
comma 的新接受形式，既有 TypeRef list 恢复继续适用。所有路径单调前进，单个函数类型保持
`O(n)`。

调用点类型实参的 strict trial 必须同步识别这里扩展后的 `function_type_parameter`；合法的
`f<(borrow T) -> R>()`、`f<(own T) -> R>()`、嵌套泛型中的模式函数类型以及对应失败候选，都必须继续满足第 3 节
既有的无副作用、Match / NoMatch 预算传播与整根 `O(n)` 预索引约束。不得在正式 TypeRef parser
接受参数模式后，让 trial 仍按旧 `Vec<TypeRef>` 语法误回退为比较，也不得为每个 `<` 重新扫描。
