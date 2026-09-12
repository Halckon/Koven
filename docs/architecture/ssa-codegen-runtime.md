# SSA、LLVM 与 Runtime

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 unit planning、typed SSA、LLVM、ABI 或 native 产物时 · **唯一真源**：`lang-codegen` 代码与测试

## Typed SSA 模型

`lang-codegen::ssa` 自有 typed SSA，不把 LLVM 类型当作 frontend 或中间语义。`Program` 包含 module、
function、block、entity、type 和 operation；控制流通过 block parameters 与显式 edge operands 传值。
每个 entity 保留类型、定义位置、ownership 和 source origin。

验证分三层执行：

- 结构/类型验证：ID、block、dominance、operand/result、edge 和 terminator 契约；
- operation 验证：构造、投影、container、nullable、String、Rc、closure、receiver 等专用约束；
- ownership 验证：Value 消费唯一性、loan 生命周期、派生 loan、drop 与 control-flow 合并。

未经验证的 SSA 不进入 LLVM adapter。renderer 只用于确定性调试和测试，不是稳定序列化协议。

单文件 lowering 在函数参数 bindings 建立后消费 `FunctionEntry` 析构事实；MoveOnly Value 实参
交付后移除其源码 binding。通用 `when` 条件边显式传递 owned bindings，并只在匹配边消费
`WhenAlternativeMatch` 析构事实。非 nullable subjectless 逗号分支已通过 SSA 和 native 双路径测试；
该证据不扩大 nullable native 支持，也不证明 MoveOnly enum 后续类型判别的主体重绑定已闭合。

单文件 pointer-like nullable `when` 消费 typed/ownership descriptor，对 owned whole-root 和
temporary 的 class、Box、Rc 使用 `NullableBranch` proof；消费提取使用 `NullableTake`。
class/Box/Rc 的只读后复用、循环读取、消费提取与 temporary 一次求值已有 object/link/run 证据。
Borrow/Inout/field/element 与 inline nullable 不在该 lowering 范围内。调用实参之间的 CFG
显式传递 temporary owner、pending loan 和 non-null view；ControlTransfer 的 loan-end 事实在
return/break/continue cleanup 前消费，abort 直接终止。native 控制转移和分配/释放计数已覆盖三类
owner：每类 8 次分配、8 次释放；Rc 另验证原 owner 释放后 retained alias 的 payload 仍可读取。

单文件 `!!` lowering 消费 typed assertion 与 ownership plan，owned root/temporary 经
`NullableBranch` proof 和 `NullableTake` 转移，null 边直接 Abort。待交付的 Value 实参复用
linear temporary 携带与重绑定，调用前刷新参数身份。普通和 nullable if 在合流前移交 MoveOnly 结果并
消费 BranchExit drop，避免同一个 owner 同时进入结果槽和来源绑定槽；循环也在每条出口
合流前消费 LoopExit drop。仅已发布 ownership plan 的 assertion 参与类型登记。
class/Box/Rc 已有非空交付、null SIGABRT、error 名称遮蔽和 operand 一次求值的 native 证据；
每类计数验证 1 次分配、1 次释放，Rc 显式 share 另验证 1 次 retain、2 次 release，以及原 owner
释放后别名仍可读取。Box 仅验证 owner 交付与释放，未据此证明 payload projection。
此入口仍拒绝 inline nullable 与非 owned 提取；unit lowering 不由这些单文件证据证明。

编译单元入口也消费 source-qualified assertion descriptor 与 ownership plan，按同一
`NullableBranch/Take` 合同提取 owned class/Box/Rc。待交付实参、借用、调用接收者、闭包
callable、构造字段和容器元素在求值后续 `!!` 时显式携带并重绑定；同一实体只传递一次。
该入口已有跨文件 SSA、pointer operand matrix 的 verified LLVM 与 class/Box/Rc native
正反例证据：显式源码 error 正常调用，null assertion 仍 SIGABRT，非空 operand 只求值一次。
普通 if/Boolean when 的 nullable 结果作为 `!!` operand 已通过 SSA/LLVM 验证；
pending Value/Loan 也在普通 if、Boolean/subjectless when、短路条件和 checked 算术边上传递，
合流仅复用所有入边都保持相同别名关系的参数槽。前序 owned Value、owned root 的借用与
借用参数搭配后续控制流 assertion operand 已通过 SSA/LLVM 验证。前序借用 temporary
同时携带 loan 与 owner，分支合流保留表达式到 owner 的映射，调用返回后消费既有 temporary
drop facts；分支正常出口要求保留入口 temporary 集合，终止出口不产生正常清理。
unit native 计数覆盖前序 Rc Borrow temporary 与七类控制流 operand、两个 flag 输入：14 次调用
共 28 次分配与 28 次释放，并核对 live pointer 身份。它验证调用交付与 owner 释放，不包含
借用 Rc 参数的 payload projection；该路径仍为现有 unsupported 边界。

实现入口：`crates/lang-codegen/src/ssa/model.rs`、`verify*.rs` 和 `render.rs`。

SSA 使用独立 `Char` 类型与 `Char(u32)` 常量；verifier 只接受 Unicode scalar，并拒绝把它与
UInt32 常量/类型互换。Char 为 Copyable，支持相等/不等而不进入整数算术或排序契约；LLVM
映射和目标布局使用 i32。`char_constant_tests` 覆盖 scalar 边界、错误类型、Copy/call 返回类型、
非恒定参数比较和确定 LLVM 输出。

单文件 lowering 消费同一分析的 validated constants/materializations，逐 use 核对 descriptor，
Boolean/整数/Char 生成 typed constant，String bytes 进入普通 `StringLiteral` temporary owner；
String 二元运算的常量名也通过该物化入口，复用现有 borrow/drop 事实。声明 initializer 不进入
lowering，未使用的 const/object roots 不产生函数或初始化代码。五类关联命名空间、八种整数
精确宽度、Boolean/Char 和重复 String use 已通过 SSA/LLVM 定向测试；native 冒烟验证了 object
整数、class companion Char 返回与比较、String concat/equality/println/return 的 UTF-8 stdout。
native 验收另覆盖六种 namespace × 十种 scalar/Char 类型、参数化 argv entry 和重复 object 字节。
String 计数夹具动态验证 9 literal + 2 concat owner 共 11 次 drop；两个 concat buffer 各分配/释放
一次，free 逐指针核对 live allocation。literal owner 本身不分配 heap，唯一消费由 SSA verifier
与动态计数共同证明。常量分析错配与非法 Byte 值在 object 落盘前拒绝。跨文件常量已有 crate 内专用标量 lowering 入口：逐 use 核对同一 typed/owned
descriptor 后生成精确 Boolean/整数/Char；unit storage 保留独立 Char 类型。
String use 已生成普通 `StringLiteral` temporary，复用 loan/transfer/drop；String 二元中的
常量 Name/Member 不读取声明 binding。专用 Group Value delivery 的 source 归一到已发布
物化 use，原 call/argument identity 保留。专用短路按 source-qualified 计划消费 Always/Never/Conditional，
校验 operand 与 branch identity；LHS 退出直接传播，动态分支消费对应 BranchExit 并保留 skip
后继。基础入口原有短路能力边界保持。String 插值按现行 guide 确定性拒绝，
包括含常量 use 的插值；完整退出矩阵尚未接通。
公开 `emit_native_constant_unit_object` 接受专用 typed/owned capability，按身份校验、共享
process entry shape、SSA lowering 的顺序检查，再复用 sibling object 原子发布。基础入口
仍只接受基础 capability。跨文件 String concat/println、argv 入口形状、正逆 inputs 与重复
object 字节已有 native 证据；非法 entry、插值及分析错配在写出前拒绝并保留既有目标。

专用普通同步调用的求值帧记录新建 loan 的 pending 槽位、前缀起点与循环深度；CFG 重绑定后
仍按槽位读取实际 LoanId。return/break/continue 先逆序结束退出帧的新建 loan，再截断 pending
槽并消费 frontend drop；复用传入 loan 不结束，Abort 不展开。Value 前缀在提交前保留具体
实参/常量 use 对应的 temporary，正常提交后移除，避免提前退出缺 owner 或重复清理。
receiver/function-value 调用的参数控制退出仍保留 guard，外层 temporary 内求值循环仍受
既有 loop lowering 限制；这些组合不由当前前缀测试证明。

String 二元操作的左 view 以 pending 槽位跨越右侧 CFG，运算时读取重绑定后的 owner。
操作数退出向上传播；正常路径仍消费 `AfterBinaryOperands`，控制退出沿用 frontend drop
事实，Abort 不展开。嵌套二元操作只移除自己的 pending 槽，保留外层调用前缀。


## Compilation-unit planning 与 lowering

`ssa::unit_plan` 从显式 entry 对 validated typed/ownership unit 做 reachability 和单态化，使用稳定的
callable/type instance key。planner 消费 frontend 已选择的 declaration、member、effective interface
implementation 和 delegation route，不重新按名称或 shape 选择。

`ssa::unit_lower` 按 source-qualified facts 生成一个 SSA program。当前路径覆盖：

- 标量、短路、`if`/`when`/loop/control result；
- top-level、member、generic callable 与 receiver-first direct call；
- ordinary/value class、enum、Box、字段投影和字段 replacement；
- 顺序容器构造、element place/read/replace；
- closure environment、capture 与 callable thunk；
- String owner/operation、Rc retain/release/payload loan；
- pointer-like nullable handle；
- source-qualified drop、loan、Value delivery 和 multi-file entry。

lowerer 只接受它能证明的 concrete layout 和 runtime recipe。缺少 frontend fact、身份不一致或不支持的
concrete 表示返回带 source origin 的 typed error，而不是生成猜测性 IR。

基础入口仍接受 validated typed/owned 并核对完整身份链；入口之后的私有 lowering driver 与
内部 planner 只读取同一轮 `CompilationUnitTypes` / `CompilationUnitOwnership`，复用已有算法。
此拆分未提供新的公开 capability 转换；crate 内常量标量入口独立校验身份后复用该 driver，
公开常量 native 入口尚未接通。

实现入口：`crates/lang-codegen/src/ssa/unit_plan.rs`、`unit_lower.rs` 及对应子模块。

## LLVM 与 ABI 边界

`lang-codegen::llvm` 是 LLVM 细节的唯一边界。type map 将已验证 SSA type 映射为 target type；layout
预检在构造 LLVM 复合类型前检查 size、alignment、stride 和算术溢出。

当前 ABI 要点：

- Borrow/Inout 以 pointer-like loan operand 传递，Value 按具体表示传递；instance receiver 位于显式
  argument 之前。
- ordinary class、Box、Rc 和 String 使用明确的 owner/runtime 表示；value class 和 enum 使用 concrete
  aggregate/tagged 表示。
- drop glue 由 concrete type 递归生成；Rc retain/release、String allocation/free、container buffer、
  abort 和 stdout 通过集中 runtime helper 发出。
- nullable handle 使用 null niche 和条件 drop；不把所有 nullable 类型统一强制成 pointer。
- 一个 compilation unit 生成一个 LLVM module，并保留多 source DWARF 行映射。

实现位于 `crates/lang-codegen/src/llvm/`；runtime helper 集中在 `llvm/runtime.rs` 与
`llvm/runtime/` 子模块。

## Native object 与发布

`native::emit_native_object` 处理单文件产物；`emit_native_unit_object` 处理 validated
compilation unit。两者接收显式 entry 与输出路径，生成 sibling temporary object，成功后原子替换目标。
backend、link 或 commit 失败由 RAII 清理临时文件并保留旧目标。

CLI 的 linker/runner 是外围编排，不进入 SSA 或 LLVM 语义。标准库源码也作为普通 frontend input
进入同一 validated 链。

## 测试覆盖位置

SSA/lowering 覆盖位于 `unit_lower_*_tests.rs`、`*_operation_tests.rs` 和 LLVM 模块测试；native
公共路径另由 `lang-cli` integration tests 覆盖。命令与扩大范围规则见
[开发测试指南](../development/testing.md)。
