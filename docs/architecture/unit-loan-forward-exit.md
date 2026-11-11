# Unit wrapper 的正常 live 出口

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 wrapper 出口覆盖证明时 · **唯一真源**：`loan_forward_regions.rs` 与 `drop_planner/action_source/forward_regions.rs`

## 有界合同

单表达式体、唯一 Shared String 参数的直接声明调用可发布独立 `forward_regions()` 区域。
真实入口、`drop_all` 前全部义务、实际 handoff 句柄、退出后状态与完整未过滤 Begin→Handoff
区间共同认证局部 Complete；不以清表后的空状态替代义务证明。Group/Block 等旧形状仍不认证。

source/callable 仍 Partial，live 交付不是 End 或权限恢复，不新增生产 receipt、清理责任或旧 oracle 退休。
该区域复用[实际动作源](unit-loan-action-source.md)，旧 raw/off/Eq 投影不变；失败或 deferred 撤销新旧产品。
验收与未覆盖范围见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#单wrapper正常liveforward出口覆盖合同2026-11-11)。
