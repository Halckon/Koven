# F3c：真实词法流接入参考表达式文法

> **性质**：非规范试验账本 · **状态**：首个有界切片已实跑，全表达式映射未完成 · **读取时机**：扩展独立文法或设计 F4a 对照时 · **唯一真源**：本页维护本切片的 Guide 映射与真实词法流验收结果

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`、Guide v0.47。接续
[词法输入合同](lexical-input-baseline.md)和[工具试验](grammar-tool-probe.md)。不含 D1–D3。
试验只消费整个 `{ … }`，检查至唯一 EOF；不将未覆盖项计入 G1。

## 1. 切片映射

以下 ID 只标识本试验规则，不是新增生产语法。先从 Guide 写出有序块元素与绑定树预期，不从生产 Parser 回填。

| ID | Guide 来源 | 参考文法规则与代表性见证 |
|---|---|---|
| E-ATOM | [04 Primary](../../guide/04-expressions-operators.md#primary-表达式) | `ID` 只产生名称；名称拼写取真实 token 的原 UTF-8 字节切片 |
| E-MEMBER | [04 Postfix](../../guide/04-expressions-operators.md#postfix调用索引与函数引用) + [06 边界](../../guide/06-blocks-control-flow.md#block-element结构边界与值语义) | `postfix DOT ID` 或 `postfix NL+ DOT ID`；`a/*雪\r\n*/.b` 为一次成员访问 |
| E-NEG/SUB | [04 运算符层级](../../guide/04-expressions-operators.md#运算符层级与结合性) + 06 边界 | 后缀优先于前缀减号，前缀优先于二元减法；`a-b.c` 为 `a-(b.c)`；`a-\nb` 的换行在二元运算符之后 |
| B-SEP | 06 边界 | 普通块元素以 `NL` 或 `SEMI` 分隔；`a\n-b` 为两个元素，`a;\n.b` 不得跨分号续行 |

非目标：其他运算符（包括非结合约束）、更多后缀、具名中缀、所有权及上下文关键字、完整嵌套 owner、诊断恢复、
资源上界。离线拒绝不是生产诊断代码。目标仍是独立文法参考，不是生产 Parser 重写。

## 2. 流交接与故障注入

仓库外 Rust 探针只调用公开 `lex`。对词法错误和超出 token 白名单的输入分别给出 `lexical-error`、
`out-of-scope`，不把其包装成文法拒绝。干净输入输出按原序排列的 `(kind, byte_start, byte_end, owner_index,
owner_start, owner_end)`，不合成前缀/续行标签。Python 3.15.2 下用 Lark 1.2.2 的自定义输入适配器
直接交付这些 token 给 Earley（`ambiguity='explicit'`）；不二次扫描原始换行、不调用 Parser。
接受前检查词法流最后只有一个 EOF；参考文法形如 `LBRACE NL* elements RBRACE EOF`。
词法 Span 与 owner 由独立手填观测检查；构建的结构只保留名称、减法、前缀及成员绑定。

`6cc03907` / macOS AArch64 实跑 **16 个输入**，均符合手填预期：11 接受（含 CRLF、多换行块注释及裸 CR
注释内容），2 个文法拒绝（分号后孤立 `.`、块后仍有 token），2 个词法拒收，1 个范围外。
接受用例中未出现 `_ambig`。代表性字节断言：`{a/*雪\r\n*/.b}` 的 `NL=[7,9)`，owner 为第 2 个
lexeme（零基），原注释 `[2,11)`。若同一注释含 LF 和 CRLF，两事件保留顺序并仍引用同一 owner。

| 人为错误 | 实测结果 / 检出来源 |
|---|---|
| 漏发 `{a\n-b}` 的 `NL` | 变成一个二元减法，违反预期两元素；结构检出 |
| 将 `{a/*雪\r*/-b}` 注释中的裸 CR 多发为 `NL` | 变成两个元素，违反预期二元减法；结构检出 |
| 漏发 `{a/*雪\r\n*/.b}` 的 `NL` | 树仍为成员访问，结构不可见；流验收因缺失事件失败 |
| 将同一 CRLF 的 Span 从 `[7,9)` 截成 `[7,8)` | 树仍为成员访问；流验收因 Span/拼写不符失败 |

因此后续差分不能只比较 AST，也不能只比较 token 数量。必须并列验证原始事件、Span 与绑定结构。
共用生产 Lexer 的正反例不能发现双方继承的词法缺陷；这部分由独立 lexical witnesses 保持。

## 3. 验证与后续门槛

```bash
cargo test --locked -p lang-frontend --test parser_block_line_continuation --test parser_expression -- --quiet
```

现有生产目标 **85 passed，0 failed、0 ignored、0 filtered**；这是已有行为回归，不是同语料的
生产/参考结构 DiffTest。探针未进入仓库，未新增依赖或远端 CI，也未启用 Guide 草案。

[F4a 普通块差分原型](independent-block-diff.md)已在同一批输入上比较生产 `parse_block` 与参考文法，
并增加有界模板及位置平移对照。持久化须独立 Spec；不把这个子集泛化为全表达式文法或全局无歧义结论。
