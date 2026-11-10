# 普通参数借用的受限生产核验

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 protocol 3 caller 路由或 receipt 时 · **唯一真源**：codegen named_route/named_receipt 与普通测试

唯一显式 own String 参数、非泛型 Unit caller 的单一直线普通 Declaration 调用可独立
进入 refine。实参必须是原参数 raw Name，参数模式 Borrow、无 receiver、正常 owned
Int/Unit 结果；typed 与旧 ownership 的 Shared loan、CallReturn End/NamedDrop 守门。
完整 subset 仍须通过，触发不读取 coverage/observer，不强迁 callee，不反禁旧 trigger。
临时值、投影、控制、替换、borrow-result、额外参数/调用和泛型不由此触发。

同轮 Complete3 在该窄形状中选择独立计划，核 catalog 原 IDs、完整 loan/argument
contract、唯一原 End/Drop、source token 到 callable/caller/callee 实例。真实普通 Borrow
producer 在 adapt 后绑定原 owner Slot，随后原 RootPlace/Shared Begin/DirectCall 的
arguments=[loan]、同 loan End、NamedDrop 逐步闭合；唯一 producer 不增加任何动作。
RStmt 的 Named 标记与 Receiver/Consumer 分家，随合块移动，Alias 不产指令。
实际 append_instruction 后冻结参数 owner/place/caller loan 与五个 InstructionId；
callee entry loan 保持独立，遗漏、重复、错身份返回结构化内部错误，不靠 Span 后扫。

off/Partial/Unsupported/缺席及非目标形状不生成 P3 receipt，不据此发布 absence。
普通 Int/Unit 的 Basic rooted/memory 入口真实重新分析 off，比较完整旧 facts 副本及
SSA/LLVM；off 产品本身不被清表或修改。四个非目标来源保留旧路由，同分析
P2/P3/P4/B/Call/Return 普通 sibling 核身份和授权分离。故障负控本机仅编译；
不代表 native、Constant off、任意 Borrow 形状或旧 oracle 已退休。

两阶段独立红绿与输入证据见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#2026-11-10-普通参数-p3-caller-路由与生产核验)。
