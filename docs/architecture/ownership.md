# 所有权事实与数据流

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 move、loan、capture、drop 或 validated ownership gate 时 · **唯一真源**：`lang-frontend` 所有权代码与测试

## 分析入口与产物

`ownership_checking::check_ownership` 提供单文件入口；
`check_compilation_unit_ownership` 消费 validated typed unit 并返回 source-qualified
`CompilationUnitOwnership`。两条路径共享所有权语义，但使用各自产物的 ID，不能混用。

公开 unit 入口委托私有 `compilation_unit/analysis.rs` driver；contracts/capture/dataflow 与错误
清理顺序不变。目前公开 ownership 入口仍只接受基础 validated typed capability。

私有 driver 已消费 unit typed constant use，跳过常量 initializer 与读取路径的 namespace/
declaration flow；String 二元操作数按每次读取的 temporary 生成析构事实。此路径的 recovery
保留常量来源标记，基础 `validate` 拒绝发布 owned capability。尚未发布 unit constant
专用公开 ownership 入口。私有 materialization plan 保留 typed use descriptor 和 inline/String
temporary 类别，只由主 traversal 登记；按 source-qualified expression 去重、排序，错误或
deferred 时整体不可用。初始化器依赖、return/Abort 后读取不登记，动态分支保留原 expression
位置；双 source 的 11 类型互相读取与输入顺序稳定性由私有 driver 测试覆盖。尚未发布专用
owned capability，完整控制流清理仍由 SPEC-0226 验收。

unit 常量的 Group 只透传值：loan、Value delivery 和 temporary drop 的 owner 统一指向
内部实际常量读取，drop 来源 Span 同步归一；原实参 identity、调用和清理位置保留。普通
literal/variable/其他 temporary 不通过该常量归一路径。

unit 入口重新核对 source inputs、names、types 和 `TypeEnvironment` 的 owner identity。只有无所有权
诊断且不存在阻塞 deferred fact 时，`validate` 才产生 `ValidatedCompilationUnitOwnership`，供
codegen 使用。

## Place、binding 与 loan

binding 能力由声明和参数 mode 决定：Owned、Shared 或 Exclusive。place 由 root symbol、field path
和 container element identity 组成；冲突判断基于 place 重叠，不依赖源码字符串。

Borrow 与 Inout 调用分别建立 shared/exclusive loan。loan 在调用或 capture 生命周期的确定边界结束；
派生 field/reborrow loan 活跃时不能提前结束父 loan。字段赋值同时检查可变性、receiver 能力和 active
loan，普通源码违规形成所有权诊断。

## Value delivery 与构造

每个已选择 call argument 和 receiver 都归一化为 source-qualified contract。Value delivery 明确区分：

- Copy：源类型满足 `Copyable`，源 binding 保持可用；
- Move：消费 named/root place，后续使用被拒绝；
- Temporary：消费本次求值产生的 owner。

构造、container、Rc、assignment 和 return 复用同一 delivery 模型。求值若以 `Nothing` 提前终止，
只提交已实际执行前缀的 effect。任一所有权错误会清空 executable loan/delivery/drop facts，避免后端
消费部分成功状态。

`!!` 的操作数消费不受外层实参 Borrow mode 改写。单文件入口读取 typed assertion descriptor，
Copyable inner 使用 Read，MoveOnly inner 使用 Consume；unit 入口复用按 Copyability 分流的消费检查。
两条路径的 liveness/drop 同步整体转移，借用提取结果时由 assertion temporary 在 CallReturn 析构。
单文件 `NonNullAssertionOwnershipPlan` 保留 typed descriptor 的唯一求值身份和来源 place，
分别发布 non-null Copy/Consume 转移与封闭 null Abort 效果；失败边没有 take、正常后继或 unwind
cleanup。不可达 assertion 不登记，任意所有权诊断清空全部计划，发布顺序按 AST identity 固定。
`CompilationUnitOwnership` 提供 source-qualified `UnitNonNullAssertionOwnershipPlan` 与按 expression
查询的接口，直接消费 unit typed descriptor。赋值失败回滚本次提取计划，任意 ownership 错误清空
全部计划；缺失或不一致的 validated descriptor 返回内部阶段错误。两条路径的结果仍由普通
drop facts 管理；这不代表 Phase 4 lowering 已接入。

单文件与 unit 在 loop 正常回边和 continue 边检查 moved owner 是否仍在下一轮 live set 中，
重复消费产生 L0131；break/return 不参与回边检查。显式 `loop` 的 header 从空集求固定点，
退出后的使用经 break 边传播，允许每轮先重新赋值的 owner。普通 whole-root 赋值无需读取
已移动旧值，成功后恢复 binding；字段、元素、复合赋值和有效 loan 仍执行原有访问检查。

## Capture、Transferability 与 drop

closure capture 按 lambda identity 和 source binding 发布。普通 closure 借用 capture；`move` closure
按 Copy/Move 获取 owner。跨线程 effect 检查完整环境的 `Transferability`，不从语法形状猜测。

反向 liveness 产生 source-qualified drop plan，覆盖参数、local、temporary、replaced value、container
element、constructor root 和 closure environment。drop point 包括最后使用后、replacement 前、
branch/loop/control transfer、return 和 callable 退出；复合 owner 按逆构造顺序析构。

单文件 `when` 的条件沿未匹配路径依次检查。逗号 alternatives 合入同一 body 前，
`WhenAlternativeMatch` 标记仅部分匹配路径仍持有的 owner 析构，避免状态交集丢失析构义务。
nullable 条件路径消费 typed 剩余域；单文件产物提供 nullable plan 查询，包含 body-entry view、
Copy/Consume extraction 和分支 drop 关联。proof 随 State 失效及合流，任何所有权诊断清空计划。
extraction 区分 entry body 与 condition alternative；Nothing 调用没有正常出口。未消费 temporary
subject 在正常、return、break/continue 边清理，abort 不展开析构。

调用前缀已建立的 loan 与借用 temporary 随 ValueState 分支传递，保持较早实参 owner 活到调用结束。
`LoanEndFact` 区分 CallReturn 与放弃尚未调用前缀的 ControlTransfer；同边先结束 loan，再执行 drop。
这些前端事实不等于 native 支持：当前单文件 lowering 仅消费正常 CallReturn 的 loan end，
ControlTransfer 结束事实还需 Phase 4 接入。

编译单元 drop planner 按调用身份在 ValueState 保存前序借用的具名 owner，使后续实参中的
分支与嵌套调用不会提前析构它；正常 CallReturn 只解除当前调用的保护。字符串插值中的
Nothing 传播无正常出口，callable 退出时清除未提交调用的保护，避免阻止最终 owner 清理。

unit 调用的 Borrow temporary 与尚未提交的 MoveOnly Value 实参也保存在 ValueState，随分支
保留。正常提交时 Value 不再清理，Borrow 在 CallReturn 逆序清理；return 和离开调用前缀
所在循环的 break/continue 先清理后建 local，再逆序清理前缀 owner，最后清理旧 local。
内层循环跳转保留外层调用 owner，Abort 不展开清理；return operand 自身不继续时也不生成
return cleanup。此为 Phase 3 事实，unit native 对新增 pending Value operand 的接线仍待
SPEC-0227 验收。

## 实现与测试位置

单文件实现位于 `crates/lang-frontend/src/ownership_checking/`，unit 实现位于其
`compilation_unit/` 子树。对应覆盖位于 `ownership_checking`、`ownership_structural`、
`ownership_containers`、`ownership_closures` 与 `multifile_ownership_checking` integration suites。

单文件调用前缀同时保存尚未交付的 MoveOnly Value 实参义务。正常调用提交时移除义务，
不发布 CallReturn drop；return 或离开当前实参求值范围的 break/continue 发布对应
`Temporary(argument)` ControlTransfer drop，内层循环跳转保留外层待调用义务。Nothing/Abort
没有正常 cleanup。该状态与 borrowed temporary 共用既有逆序、作用域和 loop-depth 清理。

单文件常量读取直接消费 Phase 2 `ValidatedConstants` use descriptor，跳过常量声明 initializer
的运行时 flow；常量名称没有 place/root，限定读取不求值 object/companion receiver。String use
沿既有 temporary loan/drop 路径处理，二元运算的各次读取分别逆序析构。scalar/Char Value
交付和 String return 不产生常量声明 owner，closure capture 不包含常量或关联命名空间。
`OwnershipCheckedFile::constant_materializations` 发布 `ValidatedConstantMaterializations`，包含
按 expression identity 查询的 `ConstantMaterializationPlan`，区分 InlineCopy 和 StringTemporary。
计划保留 typed descriptor 的值、类型与读取身份；只记录运行时遍历到的 use，不记录 initializer
依赖或不可达尾句。typed constants 缺失、所有权诊断或 deferred 均阻止能力发布，`matches`
核对同一次 typed analysis identity。局部 initializer 保留终止流，无出口 loop 阻止后续 drop 规划。
这些行为由 `ownership_constants` 覆盖，不代表 compilation-unit 或 Phase 4 支持。


String 常量的 Group 只透传读取：Borrow loan 和 temporary drop 的 owner/value-origin 归一到
物化 descriptor 的叶表达式，call argument 与 drop point 仍保留原语法身份。通用二元左 operand、
String interpolation 及专用 String operand 的终止状态向外传播，阻止后续读取及正常 drop 规划。


String binary 的已完成左 temporary 和各插值输入进入现有 ValueState 清理栈，跨后续操作数的
return/break/continue 逆序释放，Abort 不展开。正常 binary 仍在 AfterBinaryOperands 按右左清理；
插值在 AfterExpression(外层 String) 仅清理本次输入 temporary，不释放外层结果或仍 live 的 named owner。
