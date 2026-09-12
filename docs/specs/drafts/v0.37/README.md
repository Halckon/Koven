# v0.37 阻塞草案

> **性质**：draft Spec 索引 · **状态**：blocked by unapproved guide · **读取时机**：评审 v0.37 借用式迭代提案时 · **唯一真源**：本目录 Spec

现行语义仍是 v0.36；以下草案均不可实施：

- [SPEC-0179](0179-sequential-iteration-typed-plan.md)：typed iteration plan
- [SPEC-0211](0211-sequential-iteration-ownership.md)：iteration ownership
- [SPEC-0212](0212-borrowed-sequential-iteration-ssa.md)：SSA provider primitive
- [SPEC-0182](0182-sequential-for-lowering.md)：完整 `for` lowering

共同语义候选见 [v0.37 proposal](../../../proposals/v0.37-sequential-iteration.md)；长期 provider 方案仍是
[proposed ADR-0023](../../../adr/proposed/0023-borrowed-sequential-iteration-provider.md)。

## 启用后的实施切片

以下只排列实施顺序，不改变各 Spec 的批准状态或完整依赖。先完成对现行 v0.36 的重基、
明确取代关系并取得 guide 启用；有 ADR 前置时还须 accepted。每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0179 | typed iteration plan |
| Phase 3 | 0211 ← 0179 | 循环借用及退出清理 |
| Phase 4 primitive | 0212 | 独立手工 SSA provider 验证 |
| Phase 4 集成 | 0182 ← 0179/0211/0212 | 真实 for 到 native 的完整路径 |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。
