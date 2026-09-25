# v0.37 分阶段实施

> **性质**：分阶段 Spec 索引 · **状态**：guide 已启用 / typed plan 与 SSA primitive 已完成 / ownership 实施中 · **读取时机**：实施 v0.37 借用式迭代时 · **唯一真源**：各 Spec

2026-09-19 用户明确启用 v0.37，temporary source 纳入首轮 native；ADR-0023 已 accepted。
按依赖顺序推进，SPEC-0179/0212 已完成归档，SPEC-0211 仍在 active，SPEC-0182 保持 draft：

- [SPEC-0179](../../../archive/specs/0179-sequential-iteration-typed-plan.md)：typed iteration plan（done）
- [SPEC-0211](../../active/0211-sequential-iteration-ownership.md)：iteration ownership（in-progress）
- [SPEC-0212](../../../archive/specs/0212-borrowed-sequential-iteration-ssa.md)：SSA provider primitive（done）
- [SPEC-0182](0182-sequential-for-lowering.md)：完整 `for` lowering

现行语义见[顺序迭代 §37](../../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider)；
内部 provider 边界见[ADR-0023](../../../adr/accepted/0023-borrowed-sequential-iteration-provider.md)。

## 启用后的实施切片

以下排列实施顺序；每次只启动一个依赖完备的切片，完整前置仍以各 Spec 为准。
临时 source 的求值一次、延寿及全部退出清理是首轮 native 必须完成的验收。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0179 | typed iteration plan |
| Phase 3 | 0211 ← 0179 | 循环借用及退出清理 |
| Phase 4 primitive | 0212 | 独立手工 SSA provider 验证 |
| Phase 4 集成 | 0182 ← 0179/0211/0212 | 真实 for 到 native 的完整路径 |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。
