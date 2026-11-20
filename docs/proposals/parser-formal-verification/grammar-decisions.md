# F1：Parser 文法冲突裁决卡（D1–D3）

> **性质**：非规范裁决提案 · **状态**：D1–D3 已裁决；Guide 修订草案未启用 · **读取时机**：同步 Guide 或将受影响规则纳入机器文法前 · **唯一真源**：本页维护候选选择及裁决见证，不定义现行语义

基点 `6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，规范 Guide v0.47。
这里的推荐是为了解除正文冲突，不以当前实现倒推权威。下列预期只在对应规则获批并同步 Guide 后生效。
本轮只记录裁决，不修改语言规范或启用新规则。

| 编号 | 推荐 A | B 的含义与代价 | 状态 |
|---|---|---|---|
| D1 | 保留 `<T>` 后的同行无括号尾 lambda，统一提交条件 | 泛型调用必须先有 `(...)`；删除 §07 的无括号 typed 表面形式，改变既有接受面 | 按 Kotlin 的尾 lambda 附着对齐，含已开始调用后缀的跨行情况 |
| D2 | 允许 lambda 中的本地 `return`，移除矛盾的 unsupported 描述 | 禁用 lambda 的本地 `return`；需重定 §03/06/07、诊断与生产准入 | 选 A：lambda 是最近的 callable 时，`return` 退出该 lambda |
| D3 | 非前缀位置的 `move` 仍是 Identifier；保持前缀的非 trivia 判定 | 在表达式 primary 位置保留 `move`、缺 `{` 就拒绝；原先名称／调用路径会变 | 选 A |

## D1：类型实参后接无括号尾 lambda

**冲突**：[Guide 03「TypeRef 与函数类型语法」](../../guide/03-types-generics.md#typeref-与函数类型语法)
要求严格 `<...>` 后下一个非 trivia token 只能是 `(` 才提交；
[Guide 04「Postfix、调用、索引与函数引用」](../../guide/04-expressions-operators.md#postfix调用索引与函数引用)
的 `typed_call_suffix` 也只引用括号 `call_suffix`。
[Guide 07「尾 lambda 调用糖」](../../guide/07-calls-lambdas-closures.md#尾-lambda-调用糖)
则明确给出 `consume<Int> { item -> item }` 的等价调用和同行限制。

**推荐 A**：严格、无副作用地识别完整 `<...>`；后继有效 token 为 `(`，或后继为 `{` 且从 `>` 到
`{` 的 trivia gap 不含换行，才提交。语境已经终止的 block element 不重新附着。调用保留现有规范化语义：
无 `(...)` 时建立一个 Call，已有 `(...)` 时追加为该 Call 的最后一个普通实参；不附加 type-apply AST。
失败从原 `<` 回到比较路径，不留试探的 TypeRef、诊断或已消费状态。不要把「不提交 typed call」等同于
「在所有外层入口下均拒绝」，比较、lambda operand 和 element recovery 仍由原入口判定。

**备选 B**：只允许 `(` 提交，仍允许 `f<T>() { ... }`；`f<T> { ... }` 不走 typed call。
这不能只改例子，需要同步删除 §07 的现行形式，处理已有源码和实现变更，并先启用对应 Guide 版本。

**A 的验收案例**（未标注时为独立 expression 入口）：

| 源码或条件 | 应核对 |
|---|---|
| `f<T>()`、`f<T> /* c */ ()`、`f<T>\n()`、`a < b > (c)` | 保留括号形式的既有提交条件；只有 `{` 受此处同行限制；最后一例仍是 typed call，不是比较链 |
| `f<T> { x -> x }`、`f<A<B>> /* 同行 */ {}` | 一次 Call，严格 TypeRef，lambda 为唯一普通实参，匹配的 `>>` 分别闭合 |
| `f<T>() {}` | 一次 Call，lambda 追加到已建立的实参表 |
| `f<T>\n{}`、CRLF、含换行块注释 | 不走 typed-call 提交；保留原入口的比较／错误行为，typed trial 不分配 TypeRef |
| `f<T>`、`a < b > c`、`f<T,> {}`、缺 `>` | 不是 typed call；试探无副作用。拒绝和诊断由后续正常语法路径给出，不固定成单一诊断 |
| block 中 `f\n{}` | 无泛型对照：名称 expression element 后接 nested block，不能把这个结论机械套给 `f<T>\n{}` |

**当前实现证据**：`parser/trial.rs` 为 `(` 或 `{` 建候选；`engine/postfix.rs` 在提交分配 TypeRef
之前用 `typed_call_suffix_continues` 门控 `{` 的换行。`parser_trailing_lambda` 已断言同一 Call 的实参及跨行
不提交。尚需逐项补上未被现行测试覆盖的嵌套/注释/入口组合。
批准 A 后同步 03 的试探正文、04 的 EBNF/谓词说明、07 的统一引用。04 中涉及尾 lambda 的表达式说明
应回链同一规则，不复制多个不一致的允许条件。现有规范同时要求与允许不相容的表面形式；是否仅属澄清，
需维护者明确确认；若视作语义变化，先启用新 Guide，不在 v0.47 上默默改变接受集。

### 选定方向与 Guide 草案

维护者指定 D1 参照 Kotlin，而非直接选择 A 或 B。
[Kotlin 规范文法](https://github.com/Kotlin/kotlin-spec/blob/0f762a2314a9304e4b3fc386b1aceef1d56c7e4c/grammar/src/main/antlr/KotlinParser.g4)
允许省略值实参括号，且 `annotatedLambda` 的 `NL*` 允许换行。
[Kotlin 生产 Parser 源码](https://github.com/JetBrains/kotlin/blob/445745fcddeb23e605547c2600a81472ef225c29/compiler/psi/parser/src/org/jetbrains/kotlin/parsing/KotlinExpressionParsing.java#L399-L498)
也在泛型实参列表成功后调用 `parseCallWithClosure`；其直接 `{` 路径没有与 Koven 相同的
`>` 到 `{` 同行门控。该代码对后续 `(` 反而有换行检查；成功读取类型实参也不要求出现值实参或尾 lambda
才构造 PSI Call。PSI 形状不代表该源码一定能通过 Kotlin 的语义检查。

维护者确认按 Kotlin 对齐，并要求先准备 Guide 修订、不启用。最终范围及固定版本编译／运行矩阵见
[Guide 修订草案](guide-amendment-draft.md)；本节上方 A/B 保留为原候选，不是 D1 最终条文。

## D2：Lambda 内的本地 `return`

**冲突**：[Guide 03](../../guide/03-types-generics.md)和
[Guide 06「return 边界」](../../guide/06-blocks-control-flow.md#return-边界)
都明确 lambda 是独立 callable；[Guide 07「Lambda literal」](../../guide/07-calls-lambdas-closures.md#lambda-literal)
正文也允许 `return` 作为 body element，却又在 L0032 的延后清单列 `return`。

**推荐 A**：允许无标签的本地 `return`。Phase 1 保存 jump 及可选值；Phase 2 确定最近 callable 和
检查返回类型。嵌套普通 block 不建立 callable；嵌套 lambda 建立新边界。LF、CRLF、带换行注释均不把
下一 element 吞为返回值；跨行返回值从同行显式分组开始。拒绝标签及非局部返回，不增加 inline 例外。
只修 `return` 的矛盾描述；其余 L0032 列表若发现实现/条文偏差，另行登记，不借此一并启用。

**备选 B**：lambda callable 不接受本地 `return`（包括经 nested 普通 block 的路径），保留尾值；
这将推翻多个章节的显式规则，要求一个新 Guide 与分阶段诊断合同，而不只是改 L0032 清单。

**A 的验收案例**：

| 源码及入口 | 应核对 |
|---|---|
| expression `{ return }`、`{ -> return 1 }` | 无 L0032；分别保存空值及整数值；不是 Phase 1 类型兼容结论 |
| expression `{ { return 1 } }`、`{ ({ return 1 }) }` | 前者内层为 nested block，后者内层为独立 lambda；Phase 2 分别验证最近 callable |
| expression `{ return\n1 }`、`{ return /*\n*/ 1 }` | 裸 return 加后继 element，不把 `1` 同时挂到 Return；显式分组允许跨行操作数 |
| expression `{ return@label }`、`{ const val x = 1 }` | 前者非法，不承认标签；后者仍走 unsupported body form；两者不因允许 return 被放行 |
| call 中 lambda 返回后还有下一实参或外层 caller stop | 保留当前 body/下一实参和真实 closer；不把 malformed 用例的恢复等同于接受 |

**当前实现证据**：`engine/expression.rs` 识别 `Keyword::Return`；`parser_return_control` 已验证 lambda 内的
`return when` 和嵌套 block 的 `return if`。批准 A 后改 §07 的 `return` 条目，保留 §06 为边界来源，
用现有 Parser 正反测试锁定语法。若验收声称「目标一定是最近 callable」，应另选 Phase 2 的正反例，不能只靠 AST。
在只删除过时诊断描述并保全以上正文语义时，建议按 v0.47 澄清；若选择更改允许范围则走新 Guide。

## D3：非 lambda 前缀位置的 `move`

**冲突**：[Guide 01](../../guide/01-lexical.md#上下文关键字与软关键字)将 `move` 保留为
Identifier，仅在 lambda 前缀或函数类型前缀赋特殊意义，Guide 03 明确其普通类型名用法。
[Guide 07「Lambda literal」](../../guide/07-calls-lambdas-closures.md#lambda-literal)
却有不限定入口的「`move` 未后接 `{` 仍按既有 expected-expression 根因处理」。

**推荐 A**：表达式等待 primary 时，只有 `move` 之后下一个非 trivia token 是 `{` 才提交 move lambda；
保留该前缀的 trivia 判定，包括换行。此处是 lambda 内部前缀，不是 D1 的 callee 到尾 lambda 的同行连接。
其他表达式位置沿普通 Identifier 或成员名规则，真正非法的后继 token 由所在入口诊断。
在 TypeRef 中仍只在后继非 trivia `(` 时走 move 函数类型的前缀路径；不改变该路径的后续严格性或恢复。

**备选 B**：在表达式 primary 把 `move` 视为专用前缀，缺 `{` 时拒绝；成员名和 TypeRef 是否继续普通化
须显式保留。此举将使 `move` 作为普通值或独立 callee 的习惯写法失效，需要新的 Guide 规则和兼容评估。
若只希望改变 `move` 与 `{` 间的换行效果，也是一项独立语义选择，不应夹带在诊断文字修订里。

**A 的验收案例**：

| 源码及入口 | 应核对 |
|---|---|
| expression `move`、`move()`、`player.move()`；declaration `val x = move` | 普通 Name／普通 Call／member Call，无 move 捕获 marker；词法一律是 Identifier |
| expression `move {}`、`move /* c */ {}`、`move\n{}` | 在等待 primary 的位置形成 move Lambda，保存真实 move Span；补精确 LF/CRLF/注释组合见证 |
| expression `move x`、`move(` | 按普通名称后的尾部／不完整调用路径处理，不假造 lambda 或 expected-expression 的 move 专用承诺 |
| expression 内 TypeRef：`x as move`、`x as move () -> R` | 第一例为名称，第二例为特殊类型前缀；不复用表达式的 `{` 门控 |

**当前实现证据**：`engine/expression.rs` 以 `current_identifier_is("move")` 与下一有效 `{` 区分前缀；
`parser_expression` 已覆盖 `player.move()`、`move + 3`，`parser_lambda` 已覆盖同行注释和失败的 `move x`，
`parser_contextual_type_ref` 锁定 TypeRef 分界。批准 A 后改 §07 诊断段并回链 §01 和 §03；
保持普通标识符与前缀规则时建议按 v0.47 澄清。若维护者希望更改换行或普通名称接纳，先启用新 Guide。

## 核对记录与放行条件

本轮运行现有测试，未新增候选 golden，也未修改 Guide、Parser 或 Compiler Contracts：

```bash
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_trailing_lambda --test parser_lambda --test parser_return_control \
  --test parser_contextual_type_ref --test parser_expression -- --quiet
```

`6cc03907` / macOS AArch64 / Rust 1.96.0：5 个目标 **100 passed，0 failed、0 ignored、0 filtered**。
这是当前实现的见证，不代表上述所有建议案例已实跑，不验证 Phase 2 的返回边界，也不替代语言裁决。

D1–D3 已裁决，见[未启用的 Guide 修订草案](guide-amendment-draft.md)。
启用前不将草案规则纳入机器文法 golden。
