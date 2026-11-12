# 受限嵌套内层 normal-body

> **性质**：实现事实 · **状态**：current · **读取时机**：修改嵌套 owner body 事件或消费者时 · **唯一真源**：本文及所列代码

## 范围

SPEC-0341 经明确批准的切片仅接纳一个直接父/子 while 对的内层线性 body。
它给出形式 header 根上的 Replace→返回 binding→实际 Drop 单次消费权限；
不是入口 owner token、内层退出证明、外层恢复许可或任意轮次归纳。
有限 native 输入的输出也不能代替这些尚缺合同。
普通测试另核现有通用 lowering 的内层 header false-edge→外层 Replace SSA 运输；
这是旧路径实证，不是新 owner 产品的出口/恢复许可。

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
中 owner 的定义与使用位置，拒绝额外读取、消费或重复定义。临时 place/loan 另按
全函数精确位置核唯一产生/消费，并禁止 terminator 逃逸，不限制 owner 的旧路径跨块运输。
结束时再按原 FE step 及实际 call/producer 核 Replace 第二结果→唯一 Alias binding→
原事实 Drop，整函数检查 old/binding 唯一定义和消费且不从 terminator 逃逸；不只依赖
已推进的 stage。普通测试核最终 typed-SSA 旧值恰一次同块后置 Drop，Alias 会被消除。
没有另发一次 Drop，也没有向 whole-loop 或 late-SSA group 授予新权限。
内层退出、checked abort 及外层恢复继续由原通用 lowering 和 SSA verifier 负责。

## 验证位置

前端 `nested.rs::tests`：真实输入、未覆盖形状、跨 analysis/混合产品与完成事件负控。
后端 `owner_counter_boundary_tests.rs`：外/内零、一、二轮组合与新权限完成标记；
`owner_nested_body_tests.rs`：缺 permit、重复提交和实际 parent frame 损坏负控。
故障注入名称含 `ci_only`，只在 CI 执行；本地编译不等于负控已通过。
验收实际结果记入 SPEC-0341，不以此页测试位置清单宣称测试已运行。

## 可选内层 false 清理事实（不授权恢复）

批准的事实采集在实际 `drop_loop_exit` 完成之后读取清理后 state 与新追加 facts，
仅为同一 child/condition、已完成 normal 事件记录 `false_exit_partition()`。
返回 live 根和精确 LoopExit drops，完整覆盖且互斥；干净资源检查复用 boundary_values。
重复或缺事件不给可选事实，但不撤销原 normal-body 权限。
这是 planner false 清理点的静态分区，不是入口/回边 token 的实际代际合流证明；
没有 parent 恢复消费者，仍不得据此额外发射 Drop；退休根late-SSA事实见下段。
后端在既有几何仍有效的RefinedBody阶段只读核原false块实际Drop：精确root槽、
LoopExit fact、原consumed记录、span及清理顺序；不限制live根的后续旧路径操作。

生产 `refine/nested_exit_receipt.rs` 现在独立跟踪已核对退休根的实际false入口Entity
到原Drop，随straight-line fold同步块/指令偏移，终检真实边参数、指令表与块清单。
这不是owner退出消费许可；空drops只核几何，live根和parent恢复仍由原路径负责。
