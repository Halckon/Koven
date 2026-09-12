# v0.36 分阶段实施

> **性质**：draft Spec 索引 · **状态**：按阶段排队 / 依赖未完备 · **读取时机**：推进 v0.36 关联常量时 · **唯一真源**：本目录 Spec

v0.36 已于 2026-09-12 启用；以下按依赖顺序实施，未满足前置者保持 draft：

- [SPEC-0208](../../active/0208-constant-materialization-ownership.md)：所有权与重新物化，已批准
- [SPEC-0209](0209-associated-constant-lowering.md)：单文件 native lowering
- [SPEC-0210](0210-multifile-associated-constants.md)：跨文件集成

现行语义见 [v0.36 常量规则](../../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)。

## 启用后的实施切片

v0.36 已完整继承并取代 v0.35；以下排列实施顺序，不省略各 Spec 的完整依赖。
有 ADR 前置时须 accepted；每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0026（done） | 单文件常量选择、求值与错误诊断 |
| Phase 3 | 0208 ← 0026 | 重新物化的 ownership facts |
| Phase 4 | 0209 ← 0026/0208 | 单文件常量 native 行为 |
| Phase 2 跨文件 | 0210 ← 0026 | 跨文件常量 typed facts；不交付 ownership/native |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。

跨文件 ownership/native 仍需独立后继 Spec，不能用 SPEC-0210 的完成状态代替。

已完成 Phase 2 的历史证据仅在追溯时读取：[SPEC-0026](../../../archive/specs/0026-associated-constant-evaluation.md)。
