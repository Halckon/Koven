# Koven Compiler Contracts

> **性质**：编译器工程合同入口 · **状态**：current · **读取时机**：实现或评审 Parser 内部表示、算法与资源约束时 · **唯一真源**：本索引导航的合同页面

<!-- current-compiler-contracts -->

本目录是现行编译器工程合同的唯一 current 入口。它承接从 Guide 分离的内部表示、算法和
复杂度要求；[Language Reference / Guide](../guide/README.md) 仍是语言语义、可观察诊断和
强制 Phase 边界的唯一规范真源，工程合同不得改写或扩展其授权。

`current` 表示合同现行有效，不表示代码已实现；实现事实和验证入口由
[Architecture](../architecture/source-and-syntax.md) 记录。一次变更的范围与验收仍属于
[Specs](../specs/README.md)。本目录不接收未启用的候选语义。

## 按任务读取

| 任务 | 必读合同 | 语言规则入口 |
|---|---|---|
| 声明、statement 与 lambda 内部表示 | [Parser AST](parser-ast.md) | [Guide 索引](../guide/README.md)，只追加一个目标语法页 |
| Lambda header DFA、预索引与恢复复杂度 | [Parser 算法与资源](parser-algorithms.md) | [调用与 Lambda](../guide/07-calls-lambdas-closures.md)、[声明](../guide/05-declarations-callables.md)或[Block 与控制流](../guide/06-blocks-control-flow.md)，按任务选一页 |

单次任务最多五份必读文档，不因本入口要求预读全部 Guide。需要核对代码事实时再打开
[前端架构](../architecture/source-and-syntax.md)。

## 渐进迁移边界

首片仅包含上述明确内部合同。尚与 grammar、诊断或阶段规则交织的 AST/恢复文字仍保留在
Guide，不以删除实现噪声为由改写语义；具体保留范围与验收见
[SPEC-0233](../archive/specs/0233-parser-compiler-contracts.md)。

后续迁移必须逐段登记来源与唯一新归属，正文保全，仅机械修正相对链接；Guide 以最小链接
转交已迁合同，不再保留该段正文。历史迁移证据从 [Archive](../archive/README.md) 按需查阅。
