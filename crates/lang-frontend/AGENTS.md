# AGENTS.md — lang-frontend

本 crate 负责 LLVM 无关的语言前端。根 [AGENTS.md](../../AGENTS.md) 继续适用。

## 按任务读取

| 修改内容 | 必读文档（最多四份） |
|---|---|
| Source/Span、Lexer | [词法](../../docs/guide/01-lexical.md)、[前端架构](../../docs/architecture/source-and-syntax.md)、[诊断](../../docs/development/diagnostics.md) |
| 表达式/声明/控制流 Parser | [guide 索引](../../docs/guide/README.md)、[前端架构](../../docs/architecture/source-and-syntax.md)；从索引只追加一个目标语法页 |
| 名称与类型 | [名称规则](../../docs/guide/02-names-files-packages.md)、[类型规则](../../docs/guide/03-types-generics.md)、[实现事实](../../docs/architecture/names-and-types.md) |
| move / loan / drop | [所有权](../../docs/guide/10-ownership-borrowing-drop.md)、[所有权架构](../../docs/architecture/ownership.md) |
| capture / closure | [调用与 Closure](../../docs/guide/07-calls-lambdas-closures.md)、[所有权](../../docs/guide/10-ownership-borrowing-drop.md)、[所有权架构](../../docs/architecture/ownership.md) |
| container / destructuring | [集合与解构](../../docs/guide/12-collections-destructuring.md)、[所有权](../../docs/guide/10-ownership-borrowing-drop.md)、[所有权架构](../../docs/architecture/ownership.md) |
| formatter/diagnostic | [工具架构](../../docs/architecture/tooling.md)、[诊断规范](../../docs/development/diagnostics.md) |

## 边界与验证

- AST 使用索引 ID 并保留 `Span`；阶段返回明确产物和诊断，不写 LLVM 类型或隐式全局状态。
- Pratt 优先级只有一个实现真源；Parser 恢复不得伪造 AST 或丢失 Lexer 诊断身份。
- typed/ownership facts 必须稳定、可查询并支持 trial rollback，后端不得重新解释 AST。
- 最近测试位于 `crates/lang-frontend/tests/`；先运行对应 `lexer`、`parser_*`、`name_resolution`、
  `*_type_*` 或 `ownership_*` 套件。修改通用状态机、Span、AST、诊断或 harness 时按高风险门禁升级。
