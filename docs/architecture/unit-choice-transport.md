# Unit Choice value 观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对受限 Choice 的 typed 准入、真实运输与拒绝边界 · **唯一真源**：owner transport 的 choice 事务与 V1 validator 生产代码及测试

基础 owner、capture、终止边观察见[Unit owner transport](unit-owner-transport.md)。本页不授予 Phase 4/native 能力。

## V1 value 投影与受限 Choice

V1 使用独立只读 String producer catalog，Definition 与 Choice 分离。旧 Complete 且全部
定义最终 named Drop 的直线观察可按需投影；有界 Choice 另由真实 planner 事务收集。
仅顶层无修饰、非泛型 Unit 函数，一至两个无模式 marker 的精确 Boolean 参数；初始 binding
必须精确 String/MoveOnly，initializer 为显式 If 或整个 If 的 Group 链。
条件仅这些参数的 Name/Group/Not/&&/|| 树，每节点非 constant-use 且精确 Boolean；完整检查两侧（§140）。
原 planner 先求纯条件，再按真实 then/else 记边；不交换输入。调用、常量、比较及其它条件形状仍拒绝。
每边允许无插值 String literal/Group（可包单尾 ControlBody），或一个精确 String/
MoveOnly 的局部 `val` 后接唯一 Name/Group 消费尾；局部 initializer 仅 literal/Group。
两边可混搭；函数体仅初始 binding 及下述有界后续，不接受其它语句或使用。

String 构造登记真定义；Group/If 完成与双边实际 tail 交付必须唯一有序。局部 initializer
与 tail 使用独立帧/计数，symbol→value 槽仅在真实 insert 后接 Binding，实际 take_named
返回 Some 才转移到 Name 并清空槽。边出口不得残留 initializer 或 named owner。
整个 If 的 Group 链沿真实 Consume 完成运输同一 Choice，post-insert 仅接完整末帧并
清空结果；保留真实 control 和双边 producer。漏层、Read、错序或重复均撤销事务（§124）。

初始 `val` 可直接精确 AfterStatement Drop，或跟至多两级不可变、精确 String/MoveOnly 的 Name/Group alias。
每次真实 take/Consume/Forward 与 post-insert 保留同一 Choice；两级时第一 alias 的值
在独立 named 槽等待第二次实际 take，结果帧在绑定后清空；仅最终 alias 精确 Drop（§138）。
失败 take、Read、错序、重复或中间清理均撤销；第三次转交和 replace 输出 alias 链仍拒绝。

初始 `var` 须紧跟 `val previous = replace(&chosen, literal/Group)` 或其整个 Group 链；
后继须不可变、精确 String/MoveOnly 的 Name/Group；单个源为 previous 或 chosen，两个须分别消费二者。
必须是真实 checked String root Replace，非投影，RHS 精确 String 且无插值；独立帧
完整 Consume 后注册真实唯一 pending，再于 checked primitive 完成和 root 更新后发 Replace。
返回旧 Choice 沿实际 call/各层 Group 按序 Consume/Forward，末帧才 post-insert 绑定 previous（§130）。
无后继或仅转交 previous 时，新 definition 在真实 CallReturn 释放；旧 Choice 按 §126/§128 释放。
仅转交 chosen 须 previous 先退休；双输出可任一顺序，各在自身 alias 声明后退休（§136）。
每段实际 take 后沿 value 帧运输/绑定，在自身声明 Drop 后清空才推进下一段；不重建身份（§132/§134/§136）。

Choice 图的 CallReturn Drop 必须承接紧邻同 call/root/incoming 的 Replace；连续 cleanup 保留
该 commit 见证，仍验证 Named→Dropped；Swap 仍仅无 Choice。公开事件 API 不变。
BranchExit、控制退出、When 等未覆盖 point 仍不准入；图守恒不等于 typed AST 准入。
手工图可验证更一般的 Pending/Replace，但生产消费者仍只限上述合同。

旧观察仍原子 Unsupported，不接收新定义/Choice；原 cleanup/Single 见证保持。新能力
绑定 observation 的 product/callable，旧 Eq 仅比旧事实，新协议须独立验收。不从事件、
cleanup 或 owner=None 重建身份；任意退出、遗漏、重复或错误交付均清空新事务。
nullable、普通调用、capture、二次 replace/任意转交、退出、真实循环/phi、通用后续消费及 Phase 4 仍未开放。
本机证据见 §120/§122/§124/§126/§128/§130/§132/§134/§136/§138/§140；原 §87/§88/§89/§101 依赖保留，D10/E2 未完成。
