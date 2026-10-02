# 用户 deinit 与资源生命周期

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改用户析构、资源分类或 drop glue 时 · **唯一真源**：frontend resource/drop planner 与 codegen deinit 代码和测试

语言规则见 [Guide08](../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)与
[Guide10](../guide/10-ownership-borrowing-drop.md#双轨析构策略纯内存与资源类型)。这里记录实现边界，
不以类型接受或 IR 存在代替运行证明。

## 类型事实

单文件 `NominalDescriptor::deinit` 与 unit `UnitNominalSignature::deinit` 返回私有字段的可信
描述符：声明 owner、item、body、receiver type 与固定 Borrow mode；unit AST 身份含 source-unit。
`has_deinit` 由描述符派生，析构不进入可供源码选择的普通 callable 集合。

`TypedFile` / `CompilationUnitTypes::is_resource_type` 返回三值分类。字段、enum payload、nullable、
标准容器递归承接资源义务；未证明的开放类型/closure capture 返回 None。具体泛型通过字段持有的
类型参数依赖摘要求有限固定点，再组合实参；不创建 TypeId、不展开不断增长的泛型实例。
只在最终 typed 产物内缓存摘要，checker trial 不参与缓存；phantom 参数不引入不存在的资源字段。
分类支持不意味着所有对应类型已可 native 执行。

## 所有权与时机

两条 drop planner 将 ASAP 路径和强制 scope/控制退出路径分离。已证明纯内存 owner 保持原来
的 last-use 清理；资源保留到声明 scope 退出。未使用的 owned 资源参数也保留至函数退出，Borrow
参数没有 owned drop。移动转移唯一义务，替换先完成 RHS，再清理旧值；unit 另保存声明 Span，
重绑后仍按声明逆序清理。return/break/continue 复用现行退出作用域，Abort 不展开。

closure 类型本身不暴露 captures；planner 使用已检查 capture 事实判别已知环境。owned resource
capture 不能被当作纯内存；shared capture 不承接被借用资源的析构义务。纯内存 closure 的既有
ASAP 与 loan 结束顺序继续有回归证据。开放的 opaque closure 仍不能据此宣称资源分类已闭合。

资源条件运输有明确边界：

- single 条件移动的 surviving owner 仍发布词法出口 conditional fact；现有 backend 不会执行该
  可缺席 owner，因此带 exact value-origin 的 Unsupported 阻断 native，并保留已有输出文件
- unit 分支不对称移动 outer resource 发 `ResourceLifetime` deferred，并原子清空可执行 drop facts
- 两入口循环内消费外层 resource 的旧循环运输未闭合，同样 deferred；loop 内层资源与未移动外层
  guard 的 break/continue 清理可执行

这不是允许分支末提前释放资源，也不增加 NLL 或新的借用语义。

## SSA 与 LLVM

SSA `Module::set_deinit` 关联具体 HeapOwner 和隐藏函数；verifier 重新检查同一 module 的精确 owner、
唯一 Shared Loan receiver、恰一个入口参数及无返回值。关联不能指向普通 owned/exclusive receiver
或不同 class。调试 renderer 以稳定 type/function ID 展示关联。

单文件为 typed descriptor 创建隐藏 body，并将仅在 body 中使用的泛型顶层函数纳入实例规划；
unit planner 从 reachable storage layouts 求隐藏 body 与普通调用的固定点，跨文件与输入排列保持
确定性。基础及 constant-enabled unit 共享 body lowering，后者消费既有常量物化计划。

LLVM 先声明全部函数，再定义 drop glue。HeapOwner 析构为句柄建立局部 slot，以既有 Borrow ABI
调用用户 body；正常返回后继续现有 aggregate 字段逆序 drop，最后 free 实例。body 中的字段仍
可读取；Abort 直接终止，不新增 unwind。没有额外 retain、copy、clone 或堆分配。

两入口 body 的 Shared `this` 接收者只允许读取与同步共享借用。unit 的 this 字段链核对 typed
projection 和 ownership loan target，派生 loan 按创建逆序结束并参与 pending-call 控制退出；
单文件只支持当前已验证的直接字段调用借用与 nested Copyable 字段读取。

## 与 owned root 原语组合

`replace` / `swap` 仍只运输完整 owner，不在 commit 中执行 deinit。资源根在提交后继续服从
词法义务；replace 的旧 owner 由返回值交付，swap 的两个新 owner 仍按各变量声明顺序逆序释放。
与 [root 原语](root-ownership-primitives.md)的交叉 native 检查正常提交、旧值返回/借用、
pending 控制退出与 Abort，保留既有纯内存类型和字段/index/Inout 参数边界。

## Native 范围与未交付项

已接入普通、具体、非泛型 class，包括未声明 deinit 但持有普通资源字段的 class。字段可以是
已有纯内存表示或普通资源 class。用户 body 的 Unit 正常结束/return 和 Abort 使用现有控制流。
测试以真实 object/link/run 的 stdout 验证变量、body、字段与动作相对顺序，使用 allocator 插桩逐
pointer 核对 owner 唯一释放；插桩只存在于测试，不成为运行时 ABI。

尚不支持 resource-bearing generic、nullable、Box/Rc/顺序容器、value/enum wrapper、interface
继承及 closure 路径的 native 交付，均显式拒绝。类型分类广于此 native 范围。single 仍保留普通
instance member 调用和 nested field chain 作为 Borrow 实参的原有限制，不因 deinit 顺带扩展。

证据入口：frontend `resource_deinit_type_facts`、`ownership_resource_deinit`；codegen
`deinit_tests`、single/unit deinit lowering 与 native 测试。当前验收记录见
[SPEC-0245](../archive/specs/0245-resource-deinit.md)，不代表整个语言/全部 frontend 完成。
