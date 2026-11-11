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
