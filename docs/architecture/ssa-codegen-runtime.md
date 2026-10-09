# SSA、LLVM 与 Runtime

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 unit planning、typed SSA、LLVM、ABI 或 native 产物时 · **唯一真源**：`lang-codegen` 代码与测试 · [Receiver 两阶段借用](receiver-borrows.md)

## Typed SSA 模型

`lang-codegen::ssa` 自有 typed SSA，不把 LLVM 类型当作 frontend 或中间语义。`Program` 包含 module、
function、block、entity、type 和 operation；控制流通过 block parameters 与显式 edge operands 传值。
每个 entity 保留类型、定义位置、ownership 和 source origin；[root 原子置换](root-ownership-primitives.md)记录 replace / swap 的 commit、verifier 与 native 边界。[一级字段 replace](direct-field-replace.md)使用独立 field facts 与交换操作，已有有界本地和双平台验收。

验证分三层执行：

- 结构/类型验证：ID、block、dominance、operand/result、edge 和 terminator 契约；
- operation 验证：构造、投影、container、nullable、String、Rc、closure、receiver 等专用约束；
- ownership 验证：Value 消费唯一性、loan 生命周期、派生 loan、drop、control-flow 合并及[borrowed closure owned escape](borrowed-closure-escape.md)。

未经验证的 SSA 不进入 LLVM adapter。renderer 只用于确定性调试和测试，不是稳定序列化协议。整数位级执行见[专页](integer-operations.md)。

顺序容器的 `ContainerLength` 接受 Value 或 active shared Loan；verifier 拒绝 exclusive、失效和错误 target 的 loan。source `.size`与unit String读取见[接入边界](unit-container-storage.md#source-size与unit-string读取)。
结果是 signed i32 Koven `Int`，LLVM header 用 target `size_t`；借用读取按创建不变量转为 i32。list-form 与 runtime-length 创建共用目标位宽的 logical length 上限。
runtime-length 在 `Int` 域拒绝负数再转换；argv 从非负 i32 建立相同边界；目标 `size_t` 高位不再作有符号负数检查。
元素 place/replace 的索引只接受 signed i32 `Int`；LLVM 先在逻辑 Int 域拒绝负数和 `index >= length`。
成功边再无损转换为 target index；物理 GEP 不声明 inbounds。物理分配字节数受指针索引位宽的有符号上限约束；ZST 只检查逻辑边界且不形成 GEP。
provider builder 固定 length 快照、零 cursor 的入口运输、guard 真边 element loan 与固定步长回边；真实 `for` lowering 已接入 `lower_frontend`：消费前端 validated sequential iteration provider 与 ownership drop/loan 事实，按 preheader/header/body/continue/exit 展开无分配 CFG；body CFG 重绑定同步维护本层 source loan 的使用与 cleanup 身份，不结束外部 Borrow source。支持 named、borrowed 与 temporary 顺序容器（Array/List/MutableList）、名称/discard/value-class 借用解构及 break/continue/early return/nested 完整 cleanup 闭环，并通过 native object/link/run 运行验证；有限资源与provider边界见[顺序迭代事实与provider](finite-sequential-iteration.md)。

单文件 lowering 在函数参数 bindings 建立后消费 `FunctionEntry` 析构事实；MoveOnly Value 实参
交付后移除其源码 binding。通用 `when` 条件边显式传递 owned bindings，并只在匹配边消费
`WhenAlternativeMatch` 析构事实。非 nullable subjectless 逗号分支已通过 SSA 和 native 双路径测试；
该证据不扩大 nullable native 支持，也不证明 MoveOnly enum 后续类型判别的主体重绑定已闭合。 无 payload Copyable enum case condition 另消费 construction/ownership identity 生成 Boolean tag 比较；Guide4 单文件 native 和精确剩余边界见[Litmus4 账本](guide-conformance.md#return-控制表达式与-litmus4-的-native-边界)。

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
非恒定参数比较和确定 LLVM 输出。两入口的整数 lowering 复用[前端精确数值解码](names-and-types.md#数值字面量解码)，通过受检 i128 转换构造 SSA 常量；四条新 native 测试覆盖两入口 runtime/constant，各以 25 组新拼写对照十进制值实际 object/link/run，不扩展浮点或位运算。

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
包括含常量 use 的插值；调用前缀退出与内层循环的已支持范围见下文。
公开 `emit_native_constant_unit_object` 将专用 typed/owned 封闭为const view；新view native
按entry shape、SSA顺序检查，再复用sibling原子发布；[完整交接](pipeline-and-workspace.md#常量-owned-unit-交接)保留专用事实。基础入口
仍只接受基础 capability。跨文件 String concat/println、argv 入口形状、正逆 inputs 与重复
object 字节已有 native 证据；非法 entry、插值及分析错配在写出前拒绝并保留既有目标。
跨文件 native 矩阵另覆盖六类 namespace 的全部 11 种常量类型，以 import 和绝对路径读取，
验证比较 marker、常量依赖链及含 NUL 的 String 输出。unit equality/not-equal 支持独立 Char，
精确码点另由 SSA payload 检查；普通 Char literal、排序及算术未由该接入扩展。

专用普通同步调用的求值帧记录新建 loan 的 pending 槽位、前缀起点与循环深度；CFG 重绑定后
仍按槽位读取实际 LoanId。return/break/continue 先逆序结束退出帧的新建 loan，再截断 pending
槽并消费 frontend drop；复用传入 loan 不结束，Abort 不展开。Value 前缀在提交前保留具体
实参/常量 use 对应的 temporary，正常提交后移除，避免提前退出缺 owner 或重复清理。
专用共享 Borrow receiver 的求值帧包围普通参数帧，记录实际创建的 receiver loan 槽位。
参数 return/break/continue 先结束参数 loan，再结束 receiver loan；CFG 后使用重绑定槽，
借用传入对象的既有 loan 不由本次调用结束，Abort 不展开。命名、借用输入和临时 receiver
均有 SSA/LLVM 覆盖，嵌套调用及循环退出按现有 scope 深度处理。

显式 Value receiver 的 MoveOnly owner 在参数求值期间按 receiver expression 登记 temporary，
参数完成并重绑定后才撤销该记录；提前退出沿用 frontend drop，Abort 不展开。Copyable
receiver 不额外登记 owner，Group alias 按同一 ValueId 清除。
Copyable Inout receiver 的 inline writeback owner 与原始 Place 一起跨参数 CFG 传递；非 entry Place
通过 LLVM pointer phi 保留同一存储。return/break/continue 结束 loan 后沿原 drop facts 清理，
Abort 不展开；正常调用后才 Read 并重绑定。函数 entry 仍不接受 Place 参数。
MoveOnly inline Inout 的跨块正常写回仍被拒绝，完全 Diverged 路径不需要写回。
常量专用入口的 named function-value callee pending 帧包围参数帧，参数 return/break/continue
先移除 alias 再消费 frontend owner drop，Abort 不展开。lambda 的 Borrow storage 参数
在基础与常量入口复用 shared Loan ABI；基础入口仍拒绝参数控制退出。普通调用的非 Name callee、带捕获 borrowed closure 与超出现行 storage mapper 的 Borrow
参数仍有边界；Borrow Unit 与 Inout callback 参数保持拒绝。Map 专用同步 callback
的通用 storage 与 capture 接线见 [Map 当前边界](map-native-execution.md)。
常量专用入口允许外层 pending temporary 穿过内层 loop/while；header、回边与 break/false
出口携带完整 owner/loan/pending 状态，内层跳转仅结束本循环内的调用帧并移除其前缀槽位。
循环结束保留外层 temporary；return 清理退出帧，Abort 不展开。Copyable 命名变量与已求值
实参快照在循环入口分开携带，避免循环赋值改写旧实参；MoveOnly owner/loan alias 继续去重。
while 条件包含 CFG 时从实际条件出口分支。基础入口的 temporary 循环限制仍保留。
隐式具体 MoveOnly Value `this` 在 frontend pending 队列中保留既有 `This(owner)` target，
专用后端保留 current receiver 到实参全部完成再移交。参数 return 先清理后建实参，再清理
receiver，Abort 不展开；CFG 将 receiver 与 pending 槽的 alias 重绑定到同一实体。
隐式调用的 return/正常提交已有常量/literal native 对照，使用双 concat 分配、逐指针释放
与 String drop 计数验证 receiver 字段和前缀实参均正确处理。
条件 StaticSelf 的显式/隐式 Value receiver 同样延迟到参数完成才移交；conditional drop
按 frontend 的 `preceding_drops` 与同 point 的普通事实交错，越界位置拒绝，Copyable 跳过
receiver 析构。该索引计数普通 fact，包含由专用路径处理的 capture fact，不计数 LLVM 指令。
显式 Value receiver 的循环退出另有常量/literal、
Borrow/Value 实参对照，通过动态分配/释放、String drop 计数与循环后输出检查清理和跳转。

String 二元操作的左 view 以 pending 槽位跨越右侧 CFG，运算时读取重绑定后的 owner。
操作数退出向上传播；正常路径仍消费 `AfterBinaryOperands`，控制退出沿用 frontend drop
事实，Abort 不展开。嵌套二元操作只移除自己的 pending 槽，保留外层调用前缀。
Temporary drop 与 transfer 共享精确 origin 校验，清除同一 owner 的全部透明 Group alias，
避免后续 CFG 携带已清理 owner。跨文件 String 动态计数对照验证 11 次 drop、2 次 concat
分配及逐指针释放；pending concat 的 return 与 Abort 分别验证释放和不展开，结合 SSA
verifier 检查 owner 唯一消费。计数注入仅存在于测试 LLVM。

## Compilation-unit planning 与 lowering

`ssa::unit_plan` 从显式 entry 对 validated typed/ownership unit 做 reachability 和单态化，使用稳定的
callable/type instance key。planner 消费 frontend 已选择的 declaration、member、effective interface
implementation 和 delegation route，不重新按名称或 shape 选择；callback 槽与只读完整 call 路由见[实例规划边界](callable-instance-plans.md)。
`unit_plan` 私有子模块分别处理 call routes、recipe preflight/validation、canonical type 与 runtime layout；原 resolver 生产路径保留，[组合边界](../development/ownership-planning-milestone.md#codegen-unit-planner)及[布局验收](../development/unit-runtime-layout-migration.md)记录保全合同。

`ssa::unit_lower` 按 source-qualified facts 生成一个 SSA program。当前路径覆盖：

- 标量、短路、`if`/`when`/loop/control result；
- top-level、member、generic callable 与 receiver-first direct call；
- ordinary/value class、enum、Box、字段投影、replacement 及[有界字段 Borrow](direct-field-replace.md)；
- 顺序容器构造、element place/read/replace；
- closure environment、capture 与 callable thunk；
- String owner/operation（含 [StringClone](string-clone.md)）、Rc retain/release/payload loan；
- pointer-like nullable handle；
- source-qualified drop、loan、Value delivery 和 multi-file entry。

lowerer 只接受它能证明的 concrete layout 和 runtime recipe。缺少 frontend fact、身份不一致或不支持的
concrete 表示返回带 source origin 的 typed error，而不是生成猜测性 IR。

普通 `lower_owned_unit_with_entry` 消费[封闭 view](pipeline-and-workspace.md#普通-owned-unit-交接)，直接进入原私有 driver/planner，不再建立交接 index。
旧 lower/planner adapter 保留原签名与受限可见性，各经一次 factory；私有 driver 只读取同链 types/ownership facts。
两种 view 错误仍映射原 lowering kind 与 None span；[中立支撑](lowering-support.md)保留原错误路径，不新增公开 SSA/planner API。
常量 lowering 独立校验身份后共享原 driver；公开常量 native 仍消费专用 capability，不转换为普通 view。

实现入口：`crates/lang-codegen/src/ssa/unit_plan.rs`、`unit_lower.rs` 及对应子模块。

## LLVM 与 ABI 边界

`lang-codegen::llvm` 是 LLVM 细节的唯一边界。type map 将已验证 SSA type 映射为 target type；layout
预检在构造 LLVM 复合类型前检查 size、alignment、stride 和算术溢出。

当前 ABI 要点：

- Borrow/Inout 以 pointer-like loan operand 传递，Value 按具体表示传递；instance receiver 位于显式
  argument 之前。
- ordinary class、Box、Rc 和 String 使用明确的 owner/runtime 表示；value class 和 enum 使用 concrete
  aggregate/tagged 表示。HeapOwner payload 已接受 tagged union；单文件 Box 延迟定义 payload，与 unit 路径一样打断非 nullable enum 递归。两入口的具体非泛型 Box enum 已验证构造/运输/递归释放：简单 cases 2 次、四层树与 inline root 共 45 次分配/释放，逐指针计数；不证明解引用、generic 或 nullable 递归。
- drop glue 由 concrete type 递归生成；Rc retain/release、String allocation/free、container buffer、
  abort 和 stdout 通过集中 runtime helper 发出。
- nullable handle 使用 null niche 和条件 drop；不把所有 nullable 类型统一强制成 pointer。
- 一个 compilation unit 生成一个 LLVM module，并保留多 source DWARF 行映射。

[Unit 容器存储](unit-container-storage.md)记录单文件有界接入与 void ABI 隔离。实现位于 `crates/lang-codegen/src/llvm/`；runtime helper 集中在 `llvm/runtime.rs` 与
`llvm/runtime/` 子模块。

## Native object 与发布

`emit_native_object` 处理单文件；`emit_native_constant_unit_object` 消费专用常量 capability。
普通 `emit_native_owned_unit_object` 消费封闭 view；旧八参入口先执行用户 Into，再 factory 并转接。
普通 unit 的校验、reserve 与 emission 顺序见[交接流水线](pipeline-and-workspace.md#普通-owned-unit-交接)；不据此改写单文件或 const 入口。

普通 unit 的 sibling reserve/commit/Drop 算法不变，失败由 RAII 清理 object 临时文件并保留旧目标。
CLI 的 linker/runner 与链接产物清理留宿主层；标准库源码仍作为普通 frontend input 进入同一 validated 链。

## 测试覆盖位置

SSA/lowering 覆盖位于 `unit_lower_*_tests.rs`、`*_operation_tests.rs` 和 LLVM 模块测试；receiver的46项由原私有cfg(test)入口加载七个领域模块，见[搬迁验收](../development/codegen-receiver-test-migration.md)。
planner的48项保留私有cfg(test)入口与七域组织，见[plan验收](../development/codegen-plan-test-migration.md)；native公共路径另由 `lang-cli` integration tests 覆盖，命令与扩大范围见[开发测试指南](../development/testing.md)。
