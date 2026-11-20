# F3a：换行、前缀与后缀绑定的工具试验

> **性质**：非规范实验账本 · **状态**：局部试验完成，仅保留离线候选，未选定门禁工具 · **读取时机**：选择独立文法工具或判断冲突门槛时 · **唯一真源**：本页记录本次带真实换行 token 的工具实验及反例

基点：`main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47。
只测试 `parse_block` 的名称、减号、成员访问和分隔符，不依赖 D1–D3。
[Guide 06](../../guide/06-blocks-control-flow.md)规定二元运算符后的换行继续，能作前缀的减号在下一行可开启
新 element，`.` 则可继续前式。[Guide 04](../../guide/04-expressions-operators.md)规定后缀比前缀／二元运算绑定更紧。

## 最小实验与复现

macOS 自带 GNU Bison 2.3，默认 LALR(1)，不改项目依赖。临时 `.y` 声明 `%token IDENT NL`、
`%expect 0`，用单字符 lexer 提供名称及标点。只把 LF（包括 CRLF 的 LF）作为 `NL`；裸 CR 当空白。
lexer 不区分 `MINUS_PREFIX` 或 `NL_CONTINUATION`，也不检查前后 token。它不支持真实标识符或注释，
不替代生产 lexer。约简动作只输出 `name`、`prefix-minus`、`binary-minus` 或 `member`。
在仓库外用 `bison -v -o probe.c probe.y` 生成状态报告，再用 `cc -Wall -Wextra -Werror probe.c -o probe`
构建。逐例比对进程退出码和约简顺序。核心文法如下（省略 C 动作）：

```yacc
input: '{' nls elements '}' ;
elements: /* empty */ | expr rest ;
rest: /* empty */ | NL nls elements | ';' nls elements ;
nls: /* empty */ | nls NL ;
expr: prefix | expr '-' nls prefix ;
prefix: IDENT | '-' prefix ;
```

| 对照 | 实跑结果 |
|---|---|
| `{a-b}`、`{a-\nb}`、`{a-\r\nb}` | 接受，约简为一个二元减法 |
| `{a\n-b}`、`{a\r\n-b}`、`{a\n\n-b}`、`{a;-b}` | 接受，约简为名称及新的前缀表达式 |
| `{a\rb}`、`{a b}`、`{a-\n}` | 语法拒绝 |
| 把 `expr '-' nls prefix` 换成 `expr '-' nls expr` | Bison 退出 1，State 22 报一个 shift/reduce；注入检验有效 |

## `.` 的反例：零冲突不等于正确

1. 将 `expr '.' IDENT | expr NL nls '.' IDENT` 加到 `expr`。Bison **零冲突**；8 个孤立检查符合预期：
   同行、LF、CRLF、连续 LF 的 `a.b` 保留成员访问，`a\n-b` 和 `a\nb` 仍形成新的 element，
   `a;\n.b` 和 `a\n.` 被拒绝。但 `{a-b.c}` 先约简 `binary-minus`，再约简 `member`，即 `(a-b).c`；
   `{-a.b}` 同理误成 `(-a).b`。两例违反 Guide。不能以这些零冲突结果验收工具。
2. 把成员后缀移到正确的优先级：用 `prefix: postfix | '-' prefix`，再用
   `postfix: IDENT | postfix '.' IDENT | postfix NL nls '.' IDENT`。Bison **退出 1**；
   State 11 在 `NL` 上同时允许 shift（可能继续后缀）和 reduce（可能已经结束表达式），报一个 shift/reduce。
   这是此文法和 LALR(1) 模式中的边界，不是 Koven 文法歧义的证明。

生产基线：`cargo test --locked -p lang-frontend --test parser_block_line_continuation -- --quiet`，
**28 passed，0 failed、0 ignored、0 filtered**。其中 `structural_breaks_separate_group_and_unary_starts_with_exact_spans`
校验换行后的独立一元表达式。实验不比较完整生产 AST、错误恢复或资源合同。

**首轮结论**：Bison 2.3 暂不选定。不用 `%expect 1`、人为降优先级或让 lexer 吞换行来消掉这条验收边界。

## 第二轮：Earley 与显式前瞻对照

在仓库外临时 venv 固定 [Lark 1.2.2](https://pypi.org/project/lark/1.2.2/)，本机 Python 3.15.2。
同一份小文法保留 `ID` 和 `NL`；`NL: /\r?\n/`，忽略空格、tab 和裸 CR。生产 lexer 不参与。
Earley 使用 `lexer='basic', ambiguity='explicit'`，逐输入排查 `_ambig`，再比较有序块元素及
`name/neg/sub/member` 结构。核心写法：

```lark
start: "{" NL* elements "}"
elements: (expr rest)?
rest: (sep NL* elements)?
sep: NL | ";"
?expr: prefix | expr "-" NL* prefix -> sub
?prefix: postfix | "-" prefix -> neg
?postfix: ID -> name | postfix "." ID -> member
        | postfix NL+ "." ID -> member
```

独立的小型定向解析器也保留所有 `NL`：读取名称或上一后缀后，以临时索引跳过连续换行，
仅看到 `.` 才提交索引；否则游标仍停在换行，由块分隔规则处理。前缀和二元减法各自建树。
这个对照不用 Earley 的树或词法产物，但共享同一份小子集合同，不是生产正确性证明。

| 验收 | 结果 |
|---|---|
| 27 个手写正反例，比较有序块元素、精确绑定树或拒绝 | 两路都全过。`a-b.c` 和 `a-b\n.c` 的右操作数为 `b.c`，`-a.b` 为 `-(a.b)`，`a\n-b` 仍为两个元素 |
| 枚举由 `a - . NL ;` 组成的 0–5 token 块体，外加固定 `{ }` | 3,906 项，两路一致（178 接受，3,728 拒绝）；Earley 对已接受输入未出现 `_ambig` |
| 把二元减法右侧从 `prefix` 改成 `expr` | `a-b-c` 报告左结合和右结合两棵树；负对照明确检出输入歧义 |
| 同一未注入文法改用 Lark `lalr`、`strict=True` | 初始化拒绝，报告 `NL` 的 shift/reduce，与第一轮针对该边界的结果一致 |

**选型结论**：保留 Lark Earley 为小语料的离线参考候选；显式前瞻用作局部对照。
Earley 的 `_ambig` 是**对给定输入**的歧义证据，不是全局冲突清单。包元数据为 MIT、无核心运行依赖，
但没有完成依赖审计、跨宿主复现或增长测试，因此不加入仓库依赖、required CI 或生产 Parser。
本轮再次通过 `parser_block_line_continuation` 的 28 项现有测试；未改 Guide 或生产代码。

下一步可独立准备 F3b 的输入合同：为 LF、CRLF、裸 CR 与含换行注释明确 span 和 trivia 投影，
先锁定不依赖 D1–D3 的词法见证，再评估离线文法工具接真实 Lexer 产物时是否仍能保持独立性。
