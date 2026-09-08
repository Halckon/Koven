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

实现入口：`crates/lang-codegen/src/ssa/model.rs`、`verify*.rs` 和 `render.rs`。

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
