# Parser 算法与资源合同

> **性质**：现行编译器工程合同 · **状态**：current · **读取时机**：修改 lambda header 预索引或声明、block 恢复资源边界时 · **唯一真源**：本页所列算法与复杂度约束

本页约束实现策略与资源边界，不替代 Guide 中的严格前缀接受条件、诊断、stop 归属或
Phase 边界。合同生效不表示实现已完成；实际实现及验证入口见[前端架构](../architecture/source-and-syntax.md)。

## Lambda Header 试探 DFA

试探 DFA 只跳过 trivia；遇到任何 delimiter / string opener 或其他不属于普通
Identifier、参数逗号、最终 `->` 的 token 时立即永久判为 no-header，不能进入 nested owner 后
继续搜索箭头。全流索引仍负责维护共享 delimiter / lexical-owner 栈。试探不分配 AST、不发
诊断、不改变 cursor。

## Lambda Header 共享预索引

下文“上述严格前缀”指 [Guide 的 Lambda literal](../guide/07-calls-lambdas-closures.md#lambda-literal)。

Header 识别不得从每个 `{` 向前或向后独立扫描。parser 构造时必须在整个 lexeme / terminal
event 流上做一次 `O(n)` 预索引：共享 delimiter / lexical-owner 栈，并只让每个 `{` owner 的
小型 DFA 从其紧随的首个非 trivia token 开始识别上述严格前缀；首个不匹配 token 立即把该
owner 永久记为 no-header，后续箭头不再考虑。DFA 成功时记录参数 token 与 `->` raw index，
正式 parser 以 opener raw index 做 `O(1)` 查询。也可采用完全等价的共享 memo，但每个 raw
lexeme 在所有 header trial 中合计只能访问常数次。

## 声明恢复资源约束

- 对一段含 `k` 个 lexeme 的恢复，每个 lexeme 至多检查和消费一次，每个 delimiter / owner
  只压栈、弹栈一次；terminal event 按 source offset 预索引并用单调 event cursor 读取。
  因而单段恢复必须是 `O(k)` 时间、`O(d)` 嵌套栈空间，不得从每个 token 重扫诊断、回看
  opener、重启 lexer/parser 或反复切片源码。

## Block Dispatch 资源约束

  对一段 block 输入，每个 lexeme 在 block dispatch 中至多前进一次，嵌套 parser 只处理自己
  拥有的范围，整体保持 `O(n)` 时间和 `O(d)` owner / delimiter 栈空间，不从每个 element
  重启 lexer 或扫描到 block 起点。
