# v0.36 阻塞草案

> **性质**：draft Spec 索引 · **状态**：blocked by unapproved guide · **读取时机**：评审 v0.36 关联常量提案时 · **唯一真源**：本目录 Spec

现行语义仍是 v0.35；以下草案均不可实施：

- [SPEC-0026](0026-associated-constant-evaluation.md)：单文件选择与求值
- [SPEC-0208](0208-constant-materialization-ownership.md)：所有权与重新物化
- [SPEC-0209](0209-associated-constant-lowering.md)：单文件 native lowering
- [SPEC-0210](0210-multifile-associated-constants.md)：跨文件集成

共同语义候选见 [v0.36 proposal](../../../proposals/v0.36-associated-constants.md)。

## 启用后的实施切片

以下只排列实施顺序，不改变各 Spec 的批准状态或完整依赖。先完成对现行 v0.35 的重基、
明确取代关系并取得 guide 启用；有 ADR 前置时还须 accepted。每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0026 | 单文件常量选择、求值与错误诊断 |
| Phase 3 | 0208 ← 0026 | 重新物化的 ownership facts |
| Phase 4 | 0209 ← 0026/0208 | 单文件常量 native 行为 |
| Phase 2 跨文件 | 0210 ← 0026 | 跨文件常量 typed facts；不交付 ownership/native |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。

跨文件 ownership/native 仍需独立后继 Spec，不能用 SPEC-0210 的完成状态代替。
