# Receiver 的受限生产核验

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 protocol 2 receiver receipt 或其生产接点时 · **唯一真源**：codegen receiver_receipt 实现与普通测试


已迁窄 caller 形状中，同轮 Complete2 的 catalog/action IDs、callable 与 source-plan
实例现绑定到真实 receiver producer。进入原 owner_loan 前取得参数 binding 的 owner
Slot，原 push 分别核 RootPlace、Shared Begin、receiver DirectCall、同 loan End 及原
NamedDrop；每个计划唯一消费原 drop index，漏阶段、错身份和重复闭合为结构化内部错误。

稳定 RStmt 标记区分 P2 Receiver、P4 Consumer 与 [P3 Named](named-loan-production-receipts.md)，各用独立计划/receipt；标记随
合块移动，Alias 不生成 SSA 指令。原 append_instruction 处核原参数 owner、place、
caller-local loan 的冻结实体及五条实际 InstructionId；callee 使用独立 entry loan。
Int/Boolean 方法结果为普通 Value，Unit 无结果。唯一允许的显式实参是送入 own Int 形参
的普通位置 Int 字面量（结果也须为 Int）；私有核验绑定实际 ROp Slot 与紧邻该调用的 SSA
常量定义及实参 Value。协议版本、三个动作、五个生命周期指令及三个实体均不增加。
这些形态不增加 owner Call 授权或动作。P4、B Drop、
selected-owner Call 的既有生产责任和授权顺序不变。

off、Partial、Unsupported、缺席及窄形状之外不选 P2 计划，仍走既有路由/cleanup；
不以覆盖表改变语言准入，不发布 absence 证明。rooted/memory 两布局保留零实参 Int/Unit
和同分析 P2/P4/B/Call sibling 核验；零实参 Boolean 及单 Int 字面量各有实际 on/off、
SSA/LLVM 对照和逐分配身份一次释放的定向 native 验证。人工破损守卫本机只编译；
这些定向证据不代表旧 oracle 退休或完整借用流验收。

范围与验收见[SPEC-0341](../specs/active/0341-shared-lowering-core.md#已批准的-protocol-2-receiver-生产核验2026-11-10)；路由前置见[有界 caller](receiver-borrows.md#有界-owned-参数-caller-的-refine-路由)。

独立的有界借用参数转发路由复用入口 Shared loan，从未拥有或释放该对象，不产生 Protocol 2
receipt。实际借用起止和 owner 释放仍归外层调用者；此处不能套用 owned receiver 的三动作证明。


本地候选的同 owner 双调用是独立路由，依赖两个既有调用及其精确 End、唯一末次
NamedDrop，不依赖可选观察。原样观察第一段 Partial、末次段局部 Complete2；末次不证明
第一条借用，整组不产生后端 receipt。两种 Unit 布局分别核对实际 ROp、SSA、LLVM 与
按分配身份唯一释放，不增加或迁移单调用 receipt 的生产职责。
