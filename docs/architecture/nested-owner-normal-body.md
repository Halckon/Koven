# 受限嵌套内层 normal-body

> **性质**：实现事实 · **状态**：current · **读取时机**：修改嵌套 owner body 事件或消费者时 · **唯一真源**：本文及所列代码

## 范围

SPEC-0341 经明确批准的切片接纳一个直接父/子 while 对的内层线性 body，
及完整 false 出口事实之后紧邻的一笔受限父恢复写入。两者分别使用独立的
Replace→返回 binding→实际 Drop 单次发射许可，不能互相复用。
不是入口 owner 动态 token、一般嵌套或任意轮次归纳；有限 native 输入不能代替
这些尚缺合同。普通测试另核最终 SSA 运输，事实 receipt 与消费许可仍分开。

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
内层退出和 checked abort 继续由原通用 lowering 和 SSA verifier 负责；
父恢复仅下述明确限定的单笔写入有独立许可，其他形状保持旧路径。

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
false分区本身不授权parent消费，不得据此额外发射Drop；退休根late-SSA事实见下段。
后端在既有几何仍有效的RefinedBody阶段只读核原false块实际Drop：精确root槽、
LoopExit fact、原consumed记录、span及清理顺序；不限制live根的后续旧路径操作。

生产 `refine/nested_exit_receipt.rs` 现在独立跟踪已核对退休根的实际false入口Entity
到原Drop，随straight-line fold同步块/指令偏移，终检真实边参数、指令表与块清单。
这不是owner退出消费许可。现已核完整live+retired根的false边参数映射，空drops也核全根；
live根的后续消费须由独立受限恢复产品或原路径负责，不证明动态代际。

## 父恢复边界与受限单笔消费

实际child控制栈弹出后，以当前parent身份、正常返回结果及干净live根向量记录
`parent_resume_roots()`；依赖完整false分区，缺事件/重复/错parent不给事实。
边界事实本身不是消费许可；下述独立单笔产品负责已批准的Replace→binding→Drop，
不能复用child已经消费的normal permit。

受限恢复已进一步接入独立单笔collector和non-Clone发射许可：仅child紧邻的
val/direct String Replace，真实四事件及声明clean完成后封存。后端在实际parent
frame的join块[3]消费独立阶段，[2]false清理→[3]运输须唯一且根透明；按symbol匹配。
重新核准备loan及old→binding→Drop，不复活child许可、不重复发射。其他父恢复形状
仍走旧路径；受限编译期许可不是一般循环归纳。

独立 `refine/nested_resume_receipt.rs` 随实际 straight-line fold 维护恢复块与三阶段
位置，观察实际 SSA Replace 第二结果、Alias 消除后的同一 Entity 和原 Drop。
终检 false 根参数运输及唯一旧值消费后才给出 receipt 完成；它不新增消费权限，
且发生在整个 Program 的最终 verifier 之前，不能代替 verifier 或跨轮代际归纳。

可选 `nested_resume_receipt/carry.rs` 只跟踪恢复新 Value 沿已核 checked-success/Goto
到实际 parent header 的运输。支持范围外的合法 suffix 不选中该事实，已选中的
缺项或错误运输则失败封闭；到 header 即停止，不将 abort 当回边，也不推导下一轮。
