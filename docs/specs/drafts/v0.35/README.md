# v0.35 阻塞草案

> **性质**：draft Spec 索引 · **状态**：blocked by unapproved guide · **读取时机**：评审 v0.35 nullable 提案时 · **唯一真源**：本目录 Spec

现行语义仍是 v0.34；以下草案均不可实施：

- [SPEC-0202](0202-nullable-when-flow-facts.md)：nullable `when` typed facts
- [SPEC-0203](0203-nullable-when-ownership.md)：nullable `when` ownership
- [SPEC-0204](0204-pointer-nullable-when-lowering.md)：pointer nullable `when` lowering
- [SPEC-0205](0205-non-null-assertion-facts.md)：`!!` typed facts
- [SPEC-0206](0206-non-null-assertion-ownership.md)：`!!` ownership
- [SPEC-0207](0207-pointer-non-null-assertion-lowering.md)：pointer `!!` lowering

共同语义候选见 [v0.35 proposal](../../../proposals/v0.35-nullable-when-and-non-null-assertion.md)。

## 启用后的实施切片

以下只排列实施顺序，不改变各 Spec 的批准状态或完整依赖。先完成对现行 v0.34 的重基、
明确取代关系并取得 guide 启用；有 ADR 前置时还须 accepted。每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0202、0205 | when flow 与 !! extraction typed facts |
| Phase 3 | 0203 ← 0202；0206 ← 0205 | move/loan/drop 与 null 分支生命周期 |
| Phase 4 | 0204 ← 0202/0203；0207 ← 0205/0206 | SSA/LLVM 与 native null/非 null 路径 |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。
