# v0.35 阻塞草案

> **性质**：draft Spec 索引 · **状态**：等待前置 Spec · **读取时机**：推进 v0.35 nullable 实施链时 · **唯一真源**：本目录 Spec

现行 v0.35 已启用。0202/0203 已完成，0204/0205 已迁移至 active；本目录两份草案等待前置实现完成：

- [SPEC-0202](../../../archive/specs/0202-nullable-when-flow-facts.md)：nullable `when` typed facts
- [SPEC-0203](../../../archive/specs/0203-nullable-when-ownership.md)：nullable `when` ownership
- [SPEC-0204](../../active/0204-pointer-nullable-when-lowering.md)：pointer nullable `when` lowering
- [SPEC-0205](../../active/0205-non-null-assertion-facts.md)：`!!` typed facts
- [SPEC-0206](0206-non-null-assertion-ownership.md)：`!!` ownership
- [SPEC-0207](0207-pointer-non-null-assertion-lowering.md)：pointer `!!` lowering

现行语义见 [空安全](../../../guide/09-nullability-errors.md)与[阶段边界](../../../guide/15-conformance-and-staging.md)。

## 启用后的实施切片

以下只排列实施顺序，不改变各 Spec 的批准状态或完整依赖。v0.35 已继承并取代 v0.34；
前置 Spec 须 done，有 ADR 前置时还须 accepted。每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0202、0205 | when flow 与 !! extraction typed facts |
| Phase 3 | 0203 ← 0202；0206 ← 0205 | move/loan/drop 与 null 分支生命周期 |
| Phase 4 | 0204 ← 0202/0203；0207 ← 0205/0206 | SSA/LLVM 与 native null/非 null 路径 |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。
