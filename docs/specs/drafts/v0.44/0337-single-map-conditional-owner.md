# SPEC-0337: Single Map 条件结果与重绑 owner 运输

> **性质**：具体普通前置草案 · **状态**：draft · **读取时机**：恢复本片固定普通源码时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0337` |
| 前置 Spec | [SPEC-0331有限范围对账](../../../archive/specs/0331-map-borrowed-iteration.md)；从实际合并main另建分支，当前未实施 |
| 语言规范 | [Guide06控制流](../../../guide/06-blocks-control-flow.md)、[Guide07调用](../../../guide/07-calls-lambdas-closures.md)、[Guide10所有权](../../../guide/10-ownership-borrowing-drop.md)；不新增语义 |
| 阻塞项 | 本片普通lowering/ABI实施计划尚未批准；草案仅保留精确失败和硬成功目标 |

## 1. 原输入与实际归因

Single四个无For控制均在source/selected本地根lower Unsupported；Unit四无For成功、四For属于0331的真实形成缺口，不能转交本片。

固定P01–P24的96条观察全收齐并在独立审计中逐项核原source/span；collector Rust0通过/1失败、
exit101，无native。本片只承接下表的已知普通前置，不把其余33条provider独有失败转交后继。
原For与无For源码、独立按语言行为写定的完整golden均保持；不改成必拒绝oracle。

| 原输入 | source SHA-256 | golden SHA-256 |
|---|---|---|
| [P03-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P03-control.ko) | `ad7d29e1b5e71eba43cdbb6d33c024c607236d87941c1ebbbe43c086eaaa1703` | `8796b585ec5847d549d65625f6b37d7d93f5bd827caec06de72504aa92ed8d21` |
| [P03-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P03-for.ko) | `07dedeef22bf97636f950bfb03bddc53275eb7005a554ed476afa1c51b5237dd` | `e4b321ffb6cd143f10cf5853178aadf1d00fdb441be3720f03a14f8994beac18` |
| [P04-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P04-control.ko) | `c159ad2a00ada2a31bf92d8168d5b15f170324d7b1b97b8f6a3e1d07cdce3631` | `13ea947c233de9d51387483bd23aabd0ee9f71fdfd733e98497a97cdcbcd171e` |
| [P04-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P04-for.ko) | `f7048db560c8f6e17af27871a30a2a7caaf07a346b704f177b02aeb5c49e9775` | `3c7c9f1aeac14c0b257c19685385cc7358a462f656c803c1229605571964aa79` |
| [P07-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P07-control.ko) | `1993d68faeba99d19f91d04f042d7e2a1668796bb59c31b86b09fa06e8aae92c` | `83eb89fcb20162bda6054c24f601c5a902bd58720a7fd082b3352f6508c7eb16` |
| [P07-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P07-for.ko) | `0b0004c340607c906f5758dd550a4522172bf776eb4f9a78393aa5aec844f68d` | `8b1677f36fc14ed48810ab248d603390c0de1bb6b499c088ec1bb30fc29d9180` |
| [P08-control](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P08-control.ko) | `da644a59ac3d8bc4f77a46ba7f435cd24a924aa8b4e3aef3fdf56afd648bae88` | `ff0d5f62f9c9ed68efc3f7648adcd1b703af314422aaba715814b6e54d8e445f` |
| [P08-for](../../../../crates/lang-codegen/tests/fixtures/map-provider-scope-0331/P08-for.ko) | `ba688ae3759151c6002b55599ffcc4ecfe5ac3c4a105f5fcee89700acc5e5f40` | `4fe11f3c90a4e133a4d75f15bc27a9001e14587a09b3a206d7cab92ec1cc9a5e` |

## 2. 有界实施与验收

原If值必须只执行实际selected branch；条件replacement须保old/new owner与真实写入路径，不能仅按同type或根名匹配。

- [ ] 先将原普通无For控制恢复为真实成功，按同次公开typed/ownership身份和actual SSA证明；MissingFact不得当语言拒绝。
- [ ] 完整原输入×双入口共16项都成为SSA verifier/LLVM和严格native成功目标；已有另一入口的成功作为回归，不删原失败输入。
- [ ] For仍须单独核实际source/birth/current owner、完整incoming、所有受保护用途和唯一清理；普通前置修好不自动把旧For记绿。
- [ ] 每项exit0、完整固定golden、空stderr；独立审查、工程门和精确新头双宿主CI分别验收，校准/故障/sanitizer对照只在CI。

不扩大Field/Inout、动态interface、其他Map/nullable ABI或D10；不是任意Unsupported的统一豁免。
仓外原收据 `f-scope-births-v1.json`、`f-frozen-scope-attribution-v1.json` 和
`final-f-frozen-scope-attribution-independent-review.json`保留实际阶段/位置/全部输入身份。
