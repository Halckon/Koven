# SPEC-0334: String nullable 表示与 Map 组合 ABI 前置

> **性质**：设计前置草案 · **状态**：draft · **读取时机**：规划inline/tagged String nullable ABI时 · **唯一真源**：本片未实施目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0334` |
| 前置 Spec | [0331范围对账](../../active/0331-map-borrowed-iteration.md)；不是0291的一般ABI扩展 |
| 语言规范 | [Guide15 nullable实施边界](../../../guide/15-conformance-and-staging.md)已要求inline/tagged nullable独立ABI ADR与后继Spec |
| 阻塞项 | 独立SSA/LLVM表示、参数/返回/存储与Drop ABI设计尚未批准；本草案不授权实施或扩大Guide |
| 当前状态 | 只登记原12源及具体设计/验收义务，未宣称native支持 |

## 1. 固定原源与真实失败

以下fixture逐字来自0331原83源诊断，保留文件字节与SHA，不以新包装替换源码。
`a25bc6be`下12源×双入口24观察均在ordinary参数/返回/存储lower报Unsupported，原83另70源双入口LLVM成功，剩1源caller Borrow证明前置见[0335](0335-map-borrow-caller-proof.md)。
诊断Rust真实0通过/1失败/0忽略、exit101，无native；这些24不是0331新引入的provider能力拒绝。

| 原源 | SHA-256 |
|---|---|
| [g6-nullable-string-normal](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-normal.ko) | `9e8746bf7d3bde5dfbdfcf86eb6865b4b5bec77834a7f7c82e9420fe24ce9040` |
| [g6-nullable-string-continue](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-continue.ko) | `f37af83268fae7435995cb5880b880e5988b317934923e6e35bf14df66b5a6e3` |
| [g6-nullable-string-break](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-break.ko) | `ca8b9873b5463528d3195cc560a655a8cab6bf4be609ef06a362bc0dfcec7b8d` |
| [g6-nullable-string-return](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-return.ko) | `8727ef031ef07e9cccc56ca2d864c6255ce61b89e47af091d6cb30486319e44f` |
| [g7-resource-return-nullable-string](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g7-resource-return-nullable-string.ko) | `8c33101396b0e57b4df36af986d2c528b49e40c04139e5f76e932843d1c7d38f` |
| [control-nullable-string-storage](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/control-nullable-string-storage.ko) | `575747a86c610fb6616b1cc50bf407eccadde04582bc78f8c94862456d31fb1f` |
| [control-nullable-string-param](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/control-nullable-string-param.ko) | `dca52434f909f052a2c3edd21aa9ea40d4af625919138b9ff2779eaf02b6328e` |
| [control-nullable-string-return](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/control-nullable-string-return.ko) | `17b9c4eedb60d57fa8d40cdde0266414471e1721711d8e55e9057fb82f0e5526` |
| [g6-nullable-string-param-only-normal](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-param-only-normal.ko) | `097b697c3d1be07b917e4a4f1c5effa2bb6e997ff73b0fef67796ffe6806e93b` |
| [g6-nullable-string-param-only-continue](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-param-only-continue.ko) | `71a6d1d4aaaa9e5220a8b5bb1cba04a6d605187645c06193c2178cb5652b40cd` |
| [g6-nullable-string-param-only-break](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-param-only-break.ko) | `d5ab793ae845e5841200c5a6a47fa7e2e7618ecf71a6b82f0d27d3bad09628dc` |
| [g6-nullable-string-presence](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g6-nullable-string-presence.ko) | `097aaf3be801c867107c89935becb3140d1ed92cd6d39147c097322552cb7513` |

## 2. 设计门与后续验收

- [ ] 先决定String?及Map payload的IR-local表示、布局、nullable分支/读取、调用参数/返回、move/drop合同；不套pointer-like owner-only分支。
- [ ] 批准独立ABI ADR后从实际main另开Spec分支；原12源×双入口24项成为硬成功目标，另核不含For对照。
- [ ] 同分析type/category、payload/origin/owner、全部incoming与清理身份；SSA/LLVM、native完整golden/退出/空stderr分别验收。
- [ ] Resource Return的nullable结果先交付再清理；普通native不冒称无泄漏，sanitizer故障/校准仅CI。

不实现新nullable表面语义、safe call/Elvis/条件借用、Field/Inout provider或D10；未满足设计门保持draft。
