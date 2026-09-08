# Koven v0.35：表达式与运算符

> **性质**：规范性语言规范 · **状态**：current（v0.35） · **读取时机**：实现或评审表达式 Parser、优先级和运算语义时 · **唯一真源**：本页

本页是现行 Koven v0.35 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 整数溢出与除零

- **v1 采用 checked 语义**：`+`、`-`、`*` 在结果超出目标类型可表示范围时触发 `error()`
  （abort），与[顺序容器](12-collections-destructuring.md)分配大小溢出的“溢出即终止进程”规则保持
  一致，不做静默环绕（wrapping）。
- **整数除法 `/` 和取余 `%` 在除数为零时同样触发 `error()`**；`Int.MIN_VALUE / -1`（结果超出
  可表示范围）按上一条溢出规则同样触发 `error()`。
- **浮点（`Float`/`Double`）不适用本节的 checked 语义**，按 IEEE 754 语义处理：除以零产生
  `Infinity`/`-Infinity`/`NaN`，不触发 `error()`，不视为程序错误。
- 编译器可以在证明操作数范围后，把 checked 检查优化为 as-if 等价的更快代码路径，但不能
  把它优化成静默环绕——这与[顺序容器](12-collections-destructuring.md)“编译器可以按 as-if 规则优化分配，但不能改变
  可观察语义”的原则一致。
- 无符号类型（`UByte`/`UShort`/`UInt`/`ULong`）的溢出同样按 checked 语义处理，不做隐式
  环绕。
- v1 不提供显式的 wrapping / saturating / checked 变体方法（如 Rust 的 `wrapping_add` /
  `saturating_add`）；如果后续证明性能敏感场景确实需要环绕语义作为显式操作，应作为标准
  库方法在 Phase 5 之后按需引入，不修改默认 `+`/`-`/`*` 的 checked 语义。

具体 codegen 可以选择任何保持上述可观察结果的实现；后端策略不改变源语言规则。

## Primary 表达式

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
[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)；control-flow primary 见
[block 与控制流规则](06-blocks-control-flow.md)。三类 primary 均进入同一 expression AST，不形成
互相覆盖的并行语法。

`object` 只存在于声明语法，不是 primary expression；v1 不接受匿名内部类或
`object { ... }` expression。lambda 只产生函数类型值，不能借此匿名实现多方法 interface。

独立入口必须在跳过首尾 trivia 后消费到唯一 EOF，不能以“已得到一个表达式”为由忽略后续
token。字符串插值递归调用同一表达式 parser，但以当前层的 `InterpolationEnd` 代替 EOF
作为 stop token；它不得吃掉结束插值的 `}`。嵌套的括号、索引、调用和字符串仍各自消费
自己的闭合 delimiter。

## Postfix、调用、索引与函数引用

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
其 `Result<T, E>` 与最近 callable 约束见[空安全与错误值规则](09-nullability-errors.md)；Phase 1 不做类型或上下文拒绝。
Lexer 的 `?`、`?.`、`?:` 是三个最长匹配 token，parser 不拆分后两者。因而 `result?` 是
propagate，`result?.member` 是 safe member，`result ?: fallback` 是 Elvis；`result??` 是
两个左结合 propagate 节点，最终类型是否合法由 Phase 2 判断。传播后立即做普通成员访问时
统一写 `(result?).member`；不得用空白敏感的 token 重解释把 `result?.member` 当作传播。

`call_argument` 的完整产生式见[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)；basic call 与 typed call 复用同一实参语法，允许空
参数列表但不允许 trailing comma。命名 / 模式实参使用同一 typed argument 结构；显式分组的
`f((a = b))` 仍是位置 assignment expression。

索引后缀恰好包含一个表达式：不允许 `a[]`、`a[x, y]` 或 trailing comma。`arr[1..3]`
语法上是以 range 表达式为唯一 key 的普通索引；它不产生切片 AST，类型及 v2 切片边界见
[集合、索引与解构规则](12-collections-destructuring.md)。

## 运算符层级与结合性

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
lhs 合法性由类型检查验证。调用参数对未加括号的 `Identifier = ...` 另有[调用参数规则](07-calls-lambdas-closures.md)的上下文
保留规则。

`own` / `borrow` / `inout` 不属于通用 prefix 层级；它们只在
[TypeRef 与函数类型语法](03-types-generics.md#typeref-与函数类型语法)、
[声明语法](05-declarations-callables.md#声明语法)和
[Receiver Grammar](08-class-family-members.md#receiver-grammar)明确给出的位置出现。`own`
只映射声明端 `Value`，不得据此接受调用点 `own expression`。调用实参
专用入口使用符号 `&` 表达 `Inout` 调用点标注，`inout` 关键字不出现在调用
实参位置（`borrow` 关键字仍可在调用实参位置可选出现，语义与省略标注相同）；`&` 同样
不属于通用 prefix 层级，只在[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)给出的调用实参入口
合法，v1 不提供其他表达式位置的 `&` 用法，详见该节说明。无 trivia 相邻的 `++`、`--`、
`<<`、`>>` 和 `...` 必须由 parser 整体识别并报告
unsupported operator，不能把它们接受为两次 prefix / binary 或 range 加 dot。若组成字符间
有 trivia，则按各自独立合法 token 和本节普通语法处理，最终是否成立由该 token 序列决定。
这条组合检查只在**表达式运算符位置**生效；`type_ref` 递归解析时，相邻的两个 `>` 可以分别
关闭内外两层泛型实参，例如 `Outer<Inner<T>>` 必须合法，不能被误报为 shift 运算符。

`|`、`^`、`~`、`<<`、`>>` 没有现行位运算语义；`&` 也只在调用实参入口表示 `Inout`，
不是通用一元或二元运算符。位运算属于后续版本范围。
