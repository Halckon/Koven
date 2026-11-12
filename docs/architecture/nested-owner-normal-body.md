# 受限嵌套内层 normal-body

> **性质**：实现事实 · **状态**：current · **读取时机**：修改嵌套 owner body 事件或消费者时 · **唯一真源**：本文及所列代码

## 范围

SPEC-0341 经明确批准的切片仅接纳一个直接父/子 while 对的内层线性 body。
它给出形式 header 根上的 Replace→返回 binding→实际 Drop 单次消费权限；
不是入口 owner token、内层退出证明、外层恢复许可或任意轮次归纳。
有限 native 输入的输出也不能代替这些尚缺合同。

## 前端事件

`dataflow/drop_planner/owner_transport/loops/nested.rs` 在实际 ControlStack 进入
深度 2 时冻结 parent/child、condition/body、analysis/callable 和干净的根 symbol 帧。
实际条件完成及 body-entry 再核相同边界，随后共享 `counter_segment::Body` 收集
真实 literal、Replace、返回 binding、Drop 与尾部完成事件。AST BodyPlan 仅决定
精确形状，不替代成功事件。额外 child、更深嵌套、错误分支资源或不完整事件不发布。

`UnitOwnerNestedNormalBodyV1` 绑定同一 analysis/callable，producer catalog 独立，
与旧 Complete、bridge、counter 和 chain 产品互斥。Unsupported 观察仍只记录旧路径
不支持的外层点；新 normal-body 不将旧观察复活为 Complete。deferred 发布清除产品。

## 后端消费

`build/control_bridge/nested_body.rs` 从公开产品选择唯一 plan。实际 while lowering
必须处于产品指定的直接 parent frame，捕获同一 roots slots 和 child blocks，
且不存在活动 view、调用/结果 loan 或短期 range。共享 normal-body selector 与
bridge/group 互斥；原 Replace、Alias、Drop 发射消费同一非 Clone permit，依次从
Ready→Replaced→Bound→Dropped，每一步绑定 block、指令位置、span 和真实 slot。

结束检查全部阶段完成、原 body Replace 数量、header→body/false 分支，以及每根
唯一且有序的 RootPlace→Exclusive BorrowBegin→对应 Replace 准备链；检查整个 body
中 owner/place/loan 的定义与使用位置，拒绝额外读取、消费或重复定义。没有另发一次 Drop，也没有向 whole-loop 或 late-SSA group 授予新权限。
内层退出、checked abort 及外层恢复继续由原通用 lowering 和 SSA verifier 负责。

## 验证位置

前端 `nested.rs::tests`：真实输入、未覆盖形状、跨 analysis/混合产品与完成事件负控。
后端 `owner_counter_boundary_tests.rs`：外/内零、一、二轮组合与新权限完成标记；
`owner_nested_body_tests.rs`：缺 permit、重复提交和实际 parent frame 损坏负控。
故障注入名称含 `ci_only`，只在 CI 执行；本地编译不等于负控已通过。
验收实际结果记入 SPEC-0341，不以此页测试位置清单宣称测试已运行。
