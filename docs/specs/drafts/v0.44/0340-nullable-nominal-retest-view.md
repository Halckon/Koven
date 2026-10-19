# SPEC-0340: 名义类型 nullable retest view 的普通读取

> **性质**：具体普通前置草案 · **状态**：draft · **读取时机**：恢复本片固定普通源码时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0340` |
| 前置 Spec | [SPEC-0331有限范围对账](../../../archive/specs/0331-map-borrowed-iteration.md)；从实际合并main另建分支，当前未实施 |
| 语言规范 | [Guide06控制流](../../../guide/06-blocks-control-flow.md)、[Guide07调用](../../../guide/07-calls-lambdas-closures.md)、[Guide10所有权](../../../guide/10-ownership-borrowing-drop.md)；不新增语义 |
| 阻塞项 | 本片普通lowering/ABI实施计划尚未批准；草案仅保留精确失败和硬成功目标 |

## 1. 原输入与实际归因

两入口无For均在use(node)实际node读取处停止：Single lower Unsupported、Unit lower MissingFact；For保留同普通前置与Single外层If最早挡点。

固定P01–P24的96条观察全收齐并在独立审计中逐项核原source/span；collector Rust0通过/1失败、
exit101，无native。本片只承接下表的已知普通前置，不把其余33条provider独有失败转交后继。
原For与无For源码、独立按语言行为写定的完整golden均保持；不改成必拒绝oracle。

| 原输入 | source SHA-256 | golden SHA-256 |
|---|---|---|
| [P23-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P23-control.ko) | `69e271f7f3a234411fe46bfe03645bfd539dd8be6374e650c2ac9f28d01d74f7` | `19f0fbeffa7bf292230b0f1bab0df4e1a8f3c29d54c22dce0e727b36a549f23c` |
| [P23-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P23-for.ko) | `8c83592e620ad882318b94ae890367bf0fbe42a1ef3f77da6e3f75e4f333147d` | `6d37e7d0d67828690e02a4aec939a8dfef89adca843fec08f6bc0d6ad4f44190` |

## 2. 有界实施与验收

先闭合非Map的nullable nominal当前view/owner与同次retest证明；nonnull分支只读原node，null分支不伪造payload，不扩条件借用或String? Map ABI。

- [ ] 先将原普通无For控制恢复为真实成功，按同次公开typed/ownership身份和actual SSA证明；MissingFact不得当语言拒绝。
- [ ] 完整原输入×双入口共4项都成为SSA verifier/LLVM和严格native成功目标；已有另一入口的成功作为回归，不删原失败输入。
- [ ] For仍须单独核实际source/birth/current owner、完整incoming、所有受保护用途和唯一清理；普通前置修好不自动把旧For记绿。
- [ ] 每项exit0、完整固定golden、空stderr；独立审查、工程门和精确新头双宿主CI分别验收，校准/故障/sanitizer对照只在CI。

不扩大Field/Inout、动态interface、其他Map/nullable ABI或D10；不是任意Unsupported的统一豁免。
仓外原收据 `f-scope-births-v1.json`、`f-frozen-scope-attribution-v1.json` 和
`final-f-frozen-scope-attribution-independent-review.json`保留实际阶段/位置/全部输入身份。
