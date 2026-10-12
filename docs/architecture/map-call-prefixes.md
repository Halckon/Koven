# Map 调用前缀与 key return

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Map operand 控制流或清理时 · **唯一真源**：两入口 Map lowering 与直接测试

`lower_frontend/map.rs` 与 `unit_lower/map.rs` 消费现行 typed/ownership Map 合同。
`get`、`contains`、`remove` 的 key 支持 mixed、反向mixed、双return与直接return；
`put` 的 key/value return 退出保持既有支持。完全退出传播 Diverged，不生成目标 Map 操作或后缀；
有正常出口的 mixed 只在该出口执行一次操作。

## Receiver 与 CFG 身份

single 把真实 receiver 的 source `(call, argument)` cleanup identity 与 SSA owner/loan
运输分开。直接读取owner或复用loan的Map操作登记空cleanup标记，不制造SSA loan；
return消费已有LoanEnd事实，正常路径撤销标记再执行操作与CallReturn清理。
owned receiver 使用既有temporary carrier；Borrow receiver 使用其binding已有的CFG
carrier，不把调用者的loan当成本次新建loan终止。put共用此登记机制。

unit 将receiver保存在pending operand槽，在key的实际正常出口取回重绑定身份；
key完全退出时截断本次前缀并向上传播Diverged。receiver owner仍按源drop facts清理。
两入口不从操作名或具体K/V类型推导清理动作。

透明Group只用于定位named Borrow operand，避免把借用写成move；temporary/control
继续lower原始最外层expression，登记其temporary alias并消费AfterExpression与
CallReturn事实。清理时沿用同一owner的alias退休规则，不留下已释放的载体。

## 已验证与边界

直接SSA测试核对真实receiver call/argument、reservation与return drop，检查完全退出
时目标get/contains/remove指令和后缀调用为零，并生成未改写LLVM。普通native覆盖String key复用、
Resource前缀/Map值、get/contains借用参数、temporary/grouped receiver一次求值与清理，
以及存在零值、缺失和无Map对照。实际命令与结果见[0297账本](../archive/specs/0297-map-key-return-cleanup.md)。

get仍只查询Copyable V；remove仍要求Inout权限。Resource没有因此成为Hashable key。
Borrow receiver原生正例为MutableMap<String, Int>参数；temporary/grouped receiver只覆盖
get/contains。Resource值覆盖owned contains/remove，不声称以上维度的完整笛卡尔积。
本片不扩大普通调用、break/continue、Elvis、withValue/requireValue或nullable V三态；
普通输出仅证明所列源码的结果与deinit顺序，不声称逐指针计数或完整内存证明。
