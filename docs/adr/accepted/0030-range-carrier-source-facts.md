# ADR-0030: 连续范围 carrier 的独立交付与根来源事实

> **性质**：长期架构决定 · **状态**：accepted · **读取时机**：实现 N1a typed/ownership 阶段交接时 · **唯一真源**：本决定；语言语义见 Guide

批准依据是 2026-10-08 用户对 r3 小修后首片的实施授权与计数决定；
本记录只覆盖已批准的连续范围 carrier，实施证据由 SPEC-0289 维护。

## 状态

accepted

## 决定

View 身份由类型环境显式绑定，不能通过名称、package 或普通 source nominal 冒充。
typed callable 和调用结果用封闭来源 mode 区分普通 owned、借用既有存储与交付新
carrier；新交付不伪造 BorrowReturnContract 或 callee 局部描述符的借用地址。

Phase 3 分别发布新描述符存储、既有描述符借用和元素借用的事实。每项保持实际 source
operand、真实/继承根 origin、原调用 loan continuation、必要 metadata 依赖与结束点。
新 root-flat carrier 不依赖父 metadata；借用既有 carrier 保留该依赖。所有活跃依赖
都保护根，权限恢复由完整 end facts 证明，不用 owner ASAP 或某个结果结束替代。
single 与 source-qualified unit 使用相同合同；未知或缺失证明必须明确拒绝。

std 扩展选择需要独立的可信 source 证明、compiler-bound receiver identity 与 canonical
callable identity。用户 package 或路径不能获得此能力。2026-10-08 用户批准独立 std
extension authority 与 canonical declaration binding 的内部宿主接线：实际加载标准
资产的边界单独授权新建的 SourceId，producer 权限不能自动升级；同次分析从真实
声明绑定 compiler-bound List/View、元素类型、Borrow receiver 与 from this。名称和
路径只用于既有可见性查询，不能代替该绑定。此决定没有新增语言语义、用户扩展、
receiver-mode overload 或可见性/优先级规则。

第一阶段仅发布签名绑定、候选选择和 T 实例化；实际 receiver root origin、caller
loan continuation、权限恢复及 SSA/verifier 尚未接通。声明继续 L0164，不能发布
validated ownership/backend 交付，不根据本 ADR 推定完整扩展已安全或可用。

算法在 `.ko`，编译器原语只提供来源受检的范围描述符构造、访问及迭代。LLVM 布局与
返回 ABI 必须另有实际 SSA/verifier/native 验证；本轮前端事实不宣称完成后端实现。

## 边界与验证

完整继承 ADR-0029 的普通借用存储合同；新增 carrier 交付使用独立 mode，未改写其 ABI。
不启用 consume、通用 Clone、N1b、用户非逃逸类型、多来源或 optional carrier。
正负例须验证 identity/使用位置/诊断 Span、来源与 continuation/end，不能用 draft
Spec、类型通过或名称一致替代安全证明。故障生成/注入/校准不在本次授权。

当前规范见 [Guide](../../guide/12-collections-destructuring.md#n1a-单来源连续范围-carrier)，
普通结果前置见 [ADR-0029](0029-ordinary-borrow-result-continuation.md)。
