# Receiver 的受限生产核验

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 protocol 2 receiver receipt 或其生产接点时 · **唯一真源**：codegen receiver_receipt 实现与普通测试


已迁窄 caller 形状中，同轮 Complete2 的 catalog/action IDs、callable 与 source-plan
实例现绑定到真实 receiver producer。进入原 owner_loan 前取得参数 binding 的 owner
Slot，原 push 分别核 RootPlace、Shared Begin、receiver DirectCall、同 loan End 及原
NamedDrop；每个计划唯一消费原 drop index，漏阶段、错身份和重复闭合为结构化内部错误。

稳定 RStmt 标记显式区分 P2 Receiver 与 P4 Consumer，各用独立计划/receipt；标记随
合块移动，Alias 不生成 SSA 指令。原 append_instruction 处核原参数 owner、place、
caller-local loan 的冻结实体及五条实际 InstructionId；callee 使用独立 entry loan。
Int 方法结果为普通 Value，Unit 无结果，不增加 owner Call 授权或动作。P4、B Drop、
selected-owner Call 的既有生产责任和授权顺序不变。

off、Partial、Unsupported、缺席及窄形状之外不选 P2 计划，仍走既有路由/cleanup；
不以覆盖表改变语言准入，不发布 absence 证明。Basic rooted/memory 的 Int/Unit 真
on/off、原精确生命周期及同分析 P2/P4/B/Call sibling 证据保留；人工破损守卫本机
只编译，不把核验接线视为旧 oracle 退休、完整借用流或 native 资源计数验收。

范围与验收见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#已批准的-protocol-2-receiver-生产核验2026-11-10)；路由前置见[有界 caller](receiver-borrows.md#有界-owned-参数-caller-的-refine-路由)。
