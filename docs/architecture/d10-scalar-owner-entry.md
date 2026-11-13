# D10 标量分支入口

> **性质**：实现合同 · **状态**：current（远端故障验收待完成） · **读取时机**：修改父循环入口基例时 · **唯一真源**：本文及 scalar_entry 模块

只选择 own/literal String 根初始化后、父 while 前的单个 if/else。条件为 Boolean 参数（own 或默认模式），两臂各一次已核同源空体函数调用，函数为 own Int→Unit、实参为整数 literal。
不支持借用模式 Int callee、concat、一般调用、根读写或额外嵌套/复杂退出。

FE `UnitOwnerScalarEntryV1` 必须来自真实条件、两臂调用和清理、join、parent entry
全部完成；同一完整 `(root, owner)` 向量保持，既有 nested entry owners 与之等值。
纯效果形状判定可复用 E2，但事实与权限独立，不能复用 tail-break receipt。

后端封存实际 split/condition/join 角色和已核 callee。初始函数入口核 own 参数/literal
唯一来源，逐边核完整向量；两臂实际仅 scalar Constant 和指定 DirectCall。每个 arm、
join 的 root Value 必须具有精确 BlockParameter 定义、唯一参数归属、无操作消费或
其他边逃逸。entry 无前驱、arm 唯一前驱、join 恰两前驱，排除任意后续重入。
join 保持为父 header 的初始前驱，接续原 child/parent recurrence、false 与独立 abort。

CI-only 六类变异直接要求 scalar receipt 拒绝：arm偷用、join逃逸、参数定义错误、
根交换、join重入及错误callee。本地只编译列举，正常执行不代替归纳证明。

普通组合回归同时要求入口、初始基例、父组合与 false 分区收据：own/literal 两种
初始来源分别覆盖仅保留一根与全部清理。该检查补齐 diamond 入口与既有清理分区的
交叉覆盖，不增加生产权限，也不替代 CI-only 破坏性反例。
