# SPEC-0335: Map Borrow 结果的 caller continuation 证明

> **性质**：证明前置草案 · **状态**：draft · **读取时机**：闭合普通确定借用结果到provider的caller证明时 · **唯一真源**：本片待设计目标；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P3-0335` |
| 前置 Spec | [SPEC-0331范围对账](../../../archive/specs/0331-map-borrowed-iteration.md)；不由0291或D10承接 |
| 合同 | [Guide10普通确定借用结果](../../../guide/10-ownership-borrowing-drop.md)、[ADR0029](../../../adr/accepted/0029-ordinary-borrow-result-continuation.md)、[Guide37.4](../../../guide/12-collections-destructuring.md#374-irphase-交接与非目标) |
| 阻塞项 | caller来源/continuation/终止点证明与provider交接设计未闭合；本草案不授权猜测origin或绕过L0164 |
| 语言语义变更 | 不启用条件借用或新借用类型；普通确定借用结果已启用，不得误写成整体未启用 |

## 1. 原始入口与阶段

[原g10-borrow-return.ko](../../../../crates/lang-codegen/tests/fixtures/map-provider-prerequisites/g10-borrow-return.ko)
SHA-256 `892e1756b33db5bed118e0781b52c12f18f108217e54df652076bc80e7d27fb0`，逐字来自0331原83源，未改为owned包装或其他golden。

`a25bc6be`下single/unit均为ownership L0164：caller borrow result continuation is not yet proven；
未进入SSA/LLVM/native。该诊断是缺完整origin/end-proof的受限路径，不等于无条件禁止语言中的普通借用结果。

## 2. 后继证明与成功标准

- [ ] 明确唯一from来源、实参与结果loan、每个子loan、别名与全部未来使用，发布公开可校验caller continuation事实。
- [ ] CallReturn只结束无关实参loan；结果/子loan在实际终止边结束后恢复父权限，最后按原owner轨清理。
- [ ] 原源两入口成为硬成功目标；另有noFor原call、活跃结果下move/drop/变异拒绝和终止后恢复正控。
- [ ] 原同分析身份经完整validator、SSA全incoming/all protected uses与LLVM，随后真实native，不用getter互证或输出替代origin。
- [ ] 独立审查、精确双宿主CI；缺失/损坏事实、故障/校准仅CI，不把Internal当语言限制。

不启用maybe/条件借用、可存储借用、borrow-return/place-return provider扩展协议或新Guide；
证明方案批准与实施分支准备完成前保持draft，不将此源计入0331已支持的owned/Borrow参数/temporary矩阵。
