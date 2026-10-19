# SPEC-0339: Unit 值实参与待决调用求值

> **性质**：具体普通前置草案 · **状态**：draft · **读取时机**：恢复本片固定普通源码时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0339` |
| 前置 Spec | [SPEC-0331有限范围对账](../../../archive/specs/0331-map-borrowed-iteration.md)；从实际合并main另建分支，当前未实施 |
| 语言规范 | [Guide06控制流](../../../guide/06-blocks-control-flow.md)、[Guide07调用](../../../guide/07-calls-lambdas-closures.md)、[Guide10所有权](../../../guide/10-ownership-borrowing-drop.md)；不新增语义 |
| 阻塞项 | 本片普通lowering/ABI实施计划尚未批准；草案仅保留精确失败和硬成功目标 |

## 1. 原输入与实际归因

Single无For停if(flag){observe(source)}else{}实参，Unit无For停marker:Unit参数ABI，均lower Unsupported；For最早停点分别为Single外层If/Unit同参数ABI。

固定P01–P24的96条观察全收齐并在独立审计中逐项核原source/span；collector Rust0通过/1失败、
exit101，无native。本片只承接下表的已知普通前置，不把其余33条provider独有失败转交后继。
原For与无For源码、独立按语言行为写定的完整golden均保持；不改成必拒绝oracle。

| 原输入 | source SHA-256 | golden SHA-256 |
|---|---|---|
| [P22-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P22-control.ko) | `d67252efb3b49f6dfbafc7c85cc27351373444be0fc67ca7e6e06a368ab619e3` | `2efe9f629701d4718e9718f0e0c96a3a68403ac779083b12a75c72994725d2c5` |
| [P22-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P22-for.ko) | `3ff13ceef3536e08f325c6083a6110ac15e46e3cfc131a8070e3a21058014194` | `4908172930e6d5594da75e1ac16950d654d2c409a3e643e17c0eed7fd9c7f097` |

## 2. 有界实施与验收

前一Borrow实参应在后续Unit实参求值期间有效，条件分支、调用结果与loan cleanup顺序精确；不把语法block误作可返回值的expression。

- [ ] 先将原普通无For控制恢复为真实成功，按同次公开typed/ownership身份和actual SSA证明；MissingFact不得当语言拒绝。
- [ ] 完整原输入×双入口共4项都成为SSA verifier/LLVM和严格native成功目标；已有另一入口的成功作为回归，不删原失败输入。
- [ ] For仍须单独核实际source/birth/current owner、完整incoming、所有受保护用途和唯一清理；普通前置修好不自动把旧For记绿。
- [ ] 每项exit0、完整固定golden、空stderr；独立审查、工程门和精确新头双宿主CI分别验收，校准/故障/sanitizer对照只在CI。

不扩大Field/Inout、动态interface、其他Map/nullable ABI或D10；不是任意Unsupported的统一豁免。
仓外原收据 `f-scope-births-v1.json`、`f-frozen-scope-attribution-v1.json` 和
`final-f-frozen-scope-attribution-independent-review.json`保留实际阶段/位置/全部输入身份。
