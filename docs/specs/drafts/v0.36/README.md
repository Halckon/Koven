# v0.36 分阶段实施

> **性质**：阶段路由 · **状态**：单文件与跨文件 typed 完成 / ownership 实施中 · **读取时机**：推进 v0.36 关联常量时 · **唯一真源**：各阶段 Spec

v0.36 已于 2026-09-12 启用；以下按依赖顺序实施，未满足前置者保持 draft：

- [SPEC-0226](../../active/0226-unit-constant-materialization-ownership.md)：跨文件 ownership，in-progress。
- [SPEC-0227](0227-unit-constant-native-lowering.md)：跨文件 native，draft，等待 0226。

现行语义见 [v0.36 常量规则](../../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)。

## 启用后的实施切片

v0.36 已完整继承并取代 v0.35；以下排列实施顺序，不省略各 Spec 的完整依赖。
有 ADR 前置时须 accepted；每次推进一个依赖完备的 Spec。

| 切片 | Spec / 组内前置 | 交付证据 |
|---|---|---|
| Phase 2 | 0026（done） | 单文件常量选择、求值与错误诊断 |
| Phase 3 | 0208（done）← 0026 | 重新物化的 ownership facts |
| Phase 4 | 0209（done）← 0026/0208 | 单文件常量 native 行为 |
| Phase 2 跨文件 | 0210（done）← 0026 | 跨文件常量 typed facts；不交付 ownership/native |
| Phase 3 跨文件 | 0226（in-progress）← 0198/0199/0208/0209/0210 | 独立 owned capability 与物化/capture/drop facts |
| Phase 4 跨文件 | 0227（draft）← 0198/0199/0208/0209/0210/0226 | 专用 native 入口与运行验收 |

验收按 [分层测试规则](../../../development/testing.md)选择直接行为、共享契约与必要下游。
开始实施前把每项验收映射到实际 suite/过滤器；各阶段复用仍有效的证据，不重复运行 frontend
全量测试。Phase 2/3 证据不能替代 Phase 4 native 结果。

跨文件 ownership 由 SPEC-0226 实施；native 由 SPEC-0227 承接，并等待完整 ownership 产物。

已完成单文件 Phase 2/3/4 的历史证据仅在追溯时读取：[SPEC-0026](../../../archive/specs/0026-associated-constant-evaluation.md)、[SPEC-0208](../../../archive/specs/0208-constant-materialization-ownership.md)、[SPEC-0209](../../../archive/specs/0209-associated-constant-lowering.md)。

跨文件 typed 历史证据：[SPEC-0210](../../../archive/specs/0210-multifile-associated-constants.md)。
