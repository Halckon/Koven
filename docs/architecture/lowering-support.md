# 中立 lowering 支撑

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 lowering 错误归属、String decoder 或依赖边界时 · **唯一真源**：`lang-codegen::ssa::lowering_support` 与对应测试

私有 `ssa::lowering_support` 承接原 `LoweringError/Kind`、`Some(span)` 错误构造与
`string_literal::decode_plain`。single/unit adapters与unit planner保留原本地调用名，
`ssa`根及single adapter错误类型路径继续re-export；decoder算法逐字移动，driver和validation顺序不变。
这两个driver原本已经共用decoder，本片只修正职责归属，不新增String、常量求值或receiver能力。

[有界配对合同](../archive/specs/0255-neutral-lowering-support.md)使用逻辑source key、Span、
StringOwner结构和definition→use关系，不比较裸TypeId。单文件Borrow String receiver调用仍
UnsupportedNode，unit保留原支持；const与basic能力不混用。实际源码词法依赖护栏禁止unit回依赖
single adapter与IR core反向读取support；它不是完整Rust依赖图，也不代表P4全部收敛。

## nullable 最后读取与 cleanup

两入口 scalar nullable comparison 先按 typed null descriptor 和已登记的 aggregate/pointer
表示读取 presence，再消费原 operand 的最后使用 drop。String 与 MoveOnly inline 的单文件
Boolean 判空、unit 最后 if 判空已有普通源码 SSA/native 证据；借用 aggregate 继续经过
既有 Copyable 门。单文件 owning if 的 NullableBranch、资源词法和后续消费事实保持。

两 frontend 对 nullable temporary 与 null literal 的比较，在实际 operand 完成后发布
AfterBinaryOperands 结果事实；Group 归一叶和 origin，借用结果与未完成值不产生 owning drop。
两入口以精确结果事实和已登记的 aggregate/NullableHandle 表示读取 presence，再清理临时
owner；Resource temporary 在 if body 前析构，具名 Resource 的词法清理不变。
