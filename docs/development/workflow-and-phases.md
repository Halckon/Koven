# 工作流与 Phase

> **性质**：工程流程 · **状态**：current · **读取时机**：界定任务、创建 Spec/ADR 或准备交付时 · **唯一真源**：本页

## 固定流程

1. 读取现行 guide、作用域 AGENTS、相关接口、直接调用方和测试。
2. 写明所属 Phase、假设、非目标、成功标准和最小修改范围。
3. 行为变化先建立能失败的测试，再实施最小改动。
4. 按影响面运行窄测试、共享契约和必要下游检查。
5. 同步 Architecture、Spec 状态和实际验证记录。
6. 交付结果、检查命令、跳过项及不确定性；用户要求提交时保持单一 Goal 边界。

## Phase 职责

| Phase | 交付重点 |
|---|---|
| 0 | Cargo workspace、Source/Span、索引式 AST、诊断和 fixture 骨架 |
| 1 | Lexer、Parser 和错误恢复 |
| 2 | 名称、类型、smart cast、穷尽性、Copyable 与 typed facts |
| 3 | 所有权、借用、capture、Transferable 与析构计划 |
| 4 | typed SSA、LLVM、native、runtime ABI 与调试信息 |
| 5 | 使用 Koven 源码实现的最小标准库 |
| 6 | project、CLI、LSP、formatter 和编辑器 grammar |

跨 Phase 任务只实施依赖完备且已授权的部分。

## 何时需要文档门禁

- 新功能、可观察行为或诊断契约变化：先有 approved Spec。
- workspace、IR、ABI、LLVM/runtime 策略或长期开发模式变化：先有 accepted ADR。
- 语言语义、标准库契约或强制 Phase 边界变化：先形成新 guide 并由用户明确启用。
- 纯文案、链接、无语义的文档重组和带回归测试的局部 bug 修复通常不新建 Spec。

Spec/ADR 的目录和状态规则见 [docs/AGENTS.md](../AGENTS.md)。
