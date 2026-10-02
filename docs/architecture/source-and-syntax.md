# Source、Lexer、Parser 与 AST

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Source、Lexer、Parser、AST 或恢复逻辑时 · **唯一真源**：`lang-frontend` 代码与测试

## Source 与 Span

`source::SourceMap` 拥有不可变 UTF-8 源码、用户可见名称和预计算行起点。`SourceId` 是 map-local
身份；`Span` 是同一 source 上的 `[start, end)` 半开字节范围，只能通过受检 API 创建。

- 重复 source 名称、foreign `SourceId`、逆序范围、越界和非 UTF-8 边界返回具体 `SourceError`。
- 行列只在展示边界派生；行为覆盖 LF、CRLF、空文件、EOF 和 Unicode scalar 列。
- Lexer、Parser、诊断和 LSP 共用 SourceMap 的切片与位置换算，不各自维护行表。

实现入口是 `crates/lang-frontend/src/source.rs`；对应覆盖位于 `source_span` integration suite。

## 确定性 Lexer

`lexer::lex(&SourceMap, SourceId)` 返回 `LexedFile`。产物保存 source identity、有序 lexeme、唯一 EOF
和已排序诊断；除 EOF 外的 lexeme 按顺序无重叠地覆盖完整输入。

Lexer 负责现行关键字与保留字、标识符、数字后缀、`Char`、单行 String/interpolation、comment、
trivia 和固定符号最长匹配。它只分类拼写，不解析数值范围，也不决定语法位置、类型或所有权。
字符串插值通过显式模式栈扫描；非法输入必须前进并形成结构化诊断，不能在用户输入路径 panic。

实现入口是 `crates/lang-frontend/src/lexer/`；对应覆盖位于 `lexer`、`lexer_boundary_matrix` 和
`lexer_stress_matrix` integration suites。

## Parser 入口与模块

语言接受条件见 [Guide](../guide/README.md)，已分离的内部表示与资源要求见
[Compiler Contracts](../compiler-specs/README.md)；下文只记录代码与测试的当前事实。

`parser::parse_expression`、`parse_declaration`、`parse_block` 和 `parse_file` 都接收同一
`SourceMap + LexedFile`，并共享 `SyntaxAst`、诊断顺序和资源边界。`parser/mod.rs` 是公开门面：

- `syntax.rs` 定义具体 AST payload；
- `output.rs` 定义四类解析产物；
- `engine.rs` 持有唯一 cursor、AST、诊断和恢复状态；
- `engine/` 子模块按 file、declaration、class、block、expression、postfix、operator、TypeRef 和
  recovery 等语法职责拆分；
- `trial.rs` 与 `lambda_trial.rs` 只做无副作用预索引，正式分支成功后才提交 AST。

TypeRef 的上下文头部由正式解析与 strict call trial 保持一致：`move` 仅在后继 `(` 时是
函数类型前缀；函数类型参数模式可先于普通或嵌套函数 TypeRef，具名参数仍必须先有名称。
单独的 own/borrow/inout/move 类型名称不被无条件消费。重复参数模式的恢复只保留首 marker，
失败 typed-call 候选不提交 TypeRef；共享线性预算与原有递归上限保持。

调用实参的显式模式只识别 `&`；`borrow(x)`、`borrow (x)`、member/typed call 与尾 lambda
按普通表达式解析，`borrow` 不再由后继 token 推断成调用 marker。旧 `f(borrow x)` 沿
L0034 separator 恢复，在 `x` 起点报告空 Span，保留两个值与后继实参的源码身份。
声明与函数类型的 Borrow 模式不变；对应直接证据为 `parser_call_argument`。

Pratt binding power 只有一个实现来源。Parser 保存参数 marker、调用实参、receiver、尾 lambda、
隐式 `it` anchor、control-flow、class-family、package/import、解构和错误传播等源码结构；名称映射、
类型选择和所有权检查留给后续阶段。

错误恢复遵守 lexical owner、delimiter 和声明/block 边界；恢复必须单调前进。递归语法在隔离 worker
栈和固定递归预算内执行，资源边界以内部错误返回。

Block 与 lambda body 的 dispatch 每轮复用当前 lexeme 判断 closer、hard stop 和分号，避免
在同一 trivia 区域重复扫描；分号不生成 statement，caller hard closer 保留给外层 owner。

普通/control/nested block 的顶层 Pratt/postfix 在完整左表达式之后遇换行的 `(`、`+`、`-`
时归还 block dispatch；局部 initializer 复用该边界。group/call/index 内的 block soft stop
已清除；for header 只在真实 opener 存在时清软 stop，因此未闭合 delimiter 内继续解析。
typed-call 试探、独立 expression 与顶层
initializer 保持既有入口行为。lambda 顶层 body 保留独立 owner，内嵌普通 block 使用自己的
换行边界。同行缺分隔符诊断和 lambda 顶层换行仍不属于当前已闭合实现。

`return` 的同行操作数起点允许既有 control primary（含 `if` / `when`），不把 block 的
下一 element soft stop 误用为缺值判断；已开始解析操作数后仍传递原 stops。换行、分号、
`else`、caller delimiter 与 EOF 保留原 owner 边界，缺 else 的 returned if 仍为 value-context
L0057。相关结构、范围和资源回归见 `parser_return_control` 与 parser 内部预算测试。

Parser 覆盖按语法领域位于 `crates/lang-frontend/tests/parser_*.rs`，matrix suites 覆盖恢复和资源
边界。测试选择规则见[开发测试指南](../development/testing.md)。

## 索引式 AST

`ast::AstFile<Item, Statement, Expression, TypeRef>` 拥有四张按插入顺序增长的 typed table。
`ItemId`、`StatementId`、`ExpressionId` 和 `TypeRefId` 不能互换，也没有公开裸下标构造或 unchecked
lookup。

- 每个 `AstNode<T>` 持有 payload 与 `Span`；跨 source 插入在修改 table 前失败。
- table 只追加，不删除或重排；已分配 ID 在该 AST owner 内稳定。
- 节点间只通过 typed ID 连接；源码拼写通过 SourceMap 回查。
- 名称、类型、所有权和 codegen 事实保存在独立产物中，不写回 AST。

实现入口是 `crates/lang-frontend/src/ast.rs` 与 `parser/syntax.rs`；对应覆盖位于 `indexed_ast`
integration suite。
