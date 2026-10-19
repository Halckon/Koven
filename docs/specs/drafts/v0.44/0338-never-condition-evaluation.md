# SPEC-0338: Never 条件的普通求值与 provider 不获取

> **性质**：具体普通前置草案 · **状态**：draft · **读取时机**：恢复本片固定普通源码时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0338` |
| 前置 Spec | [SPEC-0331有限范围对账](../../../archive/specs/0331-map-borrowed-iteration.md)；从实际合并main另建分支，当前未实施 |
| 语言规范 | [Guide06控制流](../../../guide/06-blocks-control-flow.md)、[Guide07调用](../../../guide/07-calls-lambdas-closures.md)、[Guide10所有权](../../../guide/10-ownership-borrowing-drop.md)；不新增语义 |
| 阻塞项 | 本片普通lowering/ABI实施计划尚未批准；草案仅保留精确失败和硬成功目标 |

## 1. 原输入与实际归因

两入口无For均在if条件的return表达式lower Unsupported；含For在未Acquire的完整For拒绝。原前端unacquired成功合同保留。

固定P01–P24的96条观察全收齐并在独立审计中逐项核原source/span；collector Rust0通过/1失败、
exit101，无native。本片只承接下表的已知普通前置，不把其余33条provider独有失败转交后继。
原For与无For源码、独立按语言行为写定的完整golden均保持；不改成必拒绝oracle。

| 原输入 | source SHA-256 | golden SHA-256 |
|---|---|---|
| [P01-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P01-control.ko) | `10f7a94c8a872127bfd829b8cdb0762aad73920d2394fe3329370045640d5e9a` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| [P01-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P01-for.ko) | `c1502e6f10f243d16f22e9ee80945b1905240bc683c5f767c121e02ed7c88637` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

## 2. 有界实施与验收

先普通求值return/不可继续条件；不制造Bool、分支求值、Map allocation或已获取provider。实际无获取必须由真实阶段事实区分损坏缺plan。

- [ ] 先将原普通无For控制恢复为真实成功，按同次公开typed/ownership身份和actual SSA证明；MissingFact不得当语言拒绝。
- [ ] 完整原输入×双入口共4项都成为SSA verifier/LLVM和严格native成功目标；已有另一入口的成功作为回归，不删原失败输入。
- [ ] P01须独立核精确无获取事实与普通return交付：不生成provider操作、Map allocation或未出生source owner的清理；不制造incoming或唯一Drop。普通前置修好不自动把旧For记绿。
- [ ] 每项exit0、完整固定golden、空stderr；独立审查、工程门和精确新头双宿主CI分别验收，校准/故障/sanitizer对照只在CI。

不扩大Field/Inout、动态interface、其他Map/nullable ABI或D10；不是任意Unsupported的统一豁免。
仓外原收据 `f-scope-births-v1.json`、`f-frozen-scope-attribution-v1.json` 和
`final-f-frozen-scope-attribution-independent-review.json`保留实际阶段/位置/全部输入身份。
