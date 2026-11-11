# 多根 false 出口后的受限消费前缀

> **性质**：实现事实 · **状态**：current · **读取时机**：核对多根活根后缀消费范围 · **唯一真源**：drop planner root_suffix及真实源码测试

本文记录活根离开循环后的实际消费基础，承接[多根证据](unit-loop-root-evidence.md)。
这不是一般后缀合流、函数完成、运行时 trace 或 LLVM phi 的证明。

## 当前范围

仅当受限正常循环已有真实 false 出口分区且至少一根 live 时启动私有 tracker。
下一段同 scope depth 的连续语句必须全为 `val target = root`，RHS 是解析到剩余根的
直接 Name，target 由真实声明 liveness 判定声明后不再使用。控制节点、嵌套 block、
分支、额外声明、延迟清理及目的 binding 继续存活均保守拒绝。

每次声明须依次经过实际 moved_owner、bound_owner、去重后新增 drop hook 和声明完成。
移动核 source root/expression，绑定核 target，drop 核 target/AfterStatement；声明后资源
状态必须只含其余根且无附属资源。缺任一事件、顺序错误或剩余状态不匹配使整组失败。

tracker 不使用遗留 ValueState owner ID 推导退出时的新代际。完成事实只记录源 root 与
真实 alias drop；公开 `condition_false_suffix_retirements` 对同质完整 Entry/Backedge
来源作替换。每个初始 live root 恰消费一次，出口已退休根不可再次消费。

## 发布边界

必须全部 live roots 完成才发布，部分前缀不授予资格。封存 manifest 保留原顺序及关联，
公开一致性检查与原出口 live 分区核对。旧 all-dead、旧 owner 产品门禁和 owner 状态不变。
已完成后可忽略后续语句，只表示这个有限消费前缀成立，不表示后续函数路径或一般合流安全。

## 验证

真实源码覆盖部分/全部 live、逆序消费，以及额外语句、延迟 alias 清理、分支的拒绝。
observer 开关等价保持。CI-only 缺 move/bind/drop/完成事件负控仅编译，执行留给远端 CI。

## 已复用的实际 SSA / LLVM 路径

现有通用 lowering 已能把上述受限多根源码生成真实 SSA header 参数和 LLVM phi；不再
另写一套重复的 phi 算法，也不放宽入口或退休旧 witness。真实二根/三根、正逆 Replace
顺序、全死/部分live/全live 的矩阵，将前端 call/root/input 定义逐项对照 SSA：入口边
接原 String、正常回边接对应 RootReplace 新结果、body只消费对应 detached旧值，false
边传当前header值并在出口/有限后缀恰drop一次。LLVM另核实际双前驱String phi数量。

三根逆序的native运行只调用 `run(false)`，验证零轮出口与后缀可执行。当前纯Name且
body不改guard的形状不能在true轮后自然终止，因此不声称该native测试执行过回边。
这些测试复用原lowering和旧权限；新的边界产品尚未成为独立替代旧witness的下游授权。

## 真实回边 native 与新产品的范围隔离

现有通用 lowering 另验证二/三根、0/1/3轮、basic/constant 两种产品的12次native运行。
计数器每轮递减；每根先把当前String拼接一个x，逆序Replace后打印返回旧值，再打印
最终根。期望输出包含entry、entryx、entryxx及最终entryxxx，区分真实内容代际和根关联，
实际执行过正常回边，不再仅观察静态phi。输出成功仍不代替释放计数或无泄漏证明。
此组会借用打印旧结果，且条件比较/计数器写入超出新受限多根产品，所以同时断言新
`multi_root_boundary_v1` 不发布。成功不能充当新产品扩大接纳或退休旧witness的依据。
受限产品到SSA的精确关联矩阵则单独扩展为basic/constant两种来源，共24个组合。
