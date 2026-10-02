# 整数位运算链路

> **性质**：当前实现事实 · **状态**：current · **读取时机**：核对整数位运算与取反的阶段支持时 · **唯一真源**：frontend/codegen 代码与测试

## 整数具名位运算与取反

两条常量路径共享 `constant_value.rs` 的六种整数位运算值内核与资格判断，保留精确 builtin
宽度及符号。所有位级结果先截到目标宽度再恢复有符号解释；算术的 checked 失败仍独立处理。
`bitwise_constants` 两入口各覆盖 236 个边界计算，并验证跨文件依赖和输入顺序确定性。

`TypedFile` / `CompilationUnitTypes` 的 `integer_operations` 发布 `inv()` 的 receiver、调用、
类型及 Invert 身份。源码普通同名方法继续按原 callable 选择；内建不接受类型实参或参数。
事实参与 trial 回滚，错误分析不发布，按稳定 expression identity 排序；unit 两种验证能力都
核对 source/type/category 关系。`inv()` 仍是 call，Guide05 的 const 白名单不包含它。

ownership traversal、liveness 与 drop planner 沿既有 Copyable Read 路径处理整数 receiver，
因此保留借用冲突和命名父 owner 清理。临时对象字段仍可能处于既有 MemberReceiver deferred
边界；本切片不扩大该投影合同。后端消费见 [SSA/LLVM](ssa-codegen-runtime.md)。

## 整数位级执行

`IntegerBitwise` 与 `IntegerNot` 是独立 typed SSA operation；verifier 只接受精确同类型的
8/16/32/64 位整数 operand/result。两条 lowering 入口消费已定型的二元 operator，`inv()`
消费 frontend Invert descriptor 并核验 Call/Member receiver 的 AST 身份，不根据成员拼写猜测。

LLVM shift 先对 count 与 W-1 相与；signed shr 使用算术右移，unsigned shr 与 ushr 使用
逻辑右移，shl 不附加 nsw/nuw/exact。原有 checked 算术仍使用独立控制流与失败路径。
测试位置为 `bitwise_lowering_tests`、`bitwise_operation_tests` 和两条 native bitwise suites。

## 验证与既有边界

验收账本见 [SPEC-0240](../archive/specs/0240-integer-bitwise-execution.md)。该切片不改变 const call 白名单、同类型操作数约束或 checked 算术。
单文件普通整数索引 value lowering 与 source member call 尚未支持，裸索引/普通方法与 inv 的对照负例固定这些边界；unit 入口保留借用/命名 List 元素取反及普通源码 inv 方法正例。

两 native 入口各以 304 个二元、40 个取反边界检查结果，六组 eager operand 验证顺序与一次求值；48 组三方对照核验 const/runtime 与独立十进制 oracle。Guide Litmus12 原样提取并通过两 native 入口运行。
