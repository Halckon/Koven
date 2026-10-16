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

## Checked 整数转换（SPEC-0327）

`integer_operations` 另发布 CheckedConvert 与独立 receiver/result type；完整8×8及identity
复用真实调用/AST身份，const、非整数与用户同名分派边界保留。两个lower入口生成
`CheckedIntegerConvert(operand)` 的目标整数/Boolean双结果，沿各自checked CFG保留
owner/loan/pending operand运输并把failed送Abort。verifier不接受任意位宽或错误结果型。
LLVM从原source按符号扩展/零扩展到内部i128，先比较目标闭界再生成截断暂值，失败值
不交付；i128不进入语言类型或ABI。Linux云single/project成功native已覆盖64对交集边界、
receiver一次及有限资源控制；PR113实施CI双host另核7个畸形SSA exact与2个越界Abort exact，
各Abort exact固定9例由循环断言验收，不承诺stderr或unwind/drop。
准确实跑head、历史失败与归档CI/main待验见[SPEC-0327](../archive/specs/0327-checked-integer-conversions.md)。

## 数值 Range 普通值（SPEC-0328）

环境绑定的 IntRange/LongRange 通过私有 TypeId 注册表映射为 Copyable 三字段 aggregate，
端点仅signed32/64；不复用借用RangeView、不按显示名称识别。构造、包含、相等三种SSA运算
验证注册身份与真实operand/result，LLVM半开在nonempty块才计算bound−1，以ready PHI续接；
闭MAX不加一，序列相等区分空与有效端点。普通局部/参数返回/字段/List/Map值沿通用aggregate ABI。
有限Linux单文件/项目native已验证动态边界、源码求值顺序及MapResult found/missing真实消费；
requireValue仍须显式borrow val再复制，复杂全函数borrow CFG与一般Range?仍保留结构化边界。
数值 Range for 的 owned 快照/终值先判 CFG 与有界 native 已在 [SPEC-0329](../archive/specs/0329-integer-range-iteration.md) 按 PR115 实现 CI 有界验收；
值 ABI 的本地/CI 分账见 [SPEC-0328](../archive/specs/0328-integer-range-values.md)。
