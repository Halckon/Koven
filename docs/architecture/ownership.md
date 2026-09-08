# 所有权事实与数据流

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 move、loan、capture、drop 或 validated ownership gate 时 · **唯一真源**：`lang-frontend` 所有权代码与测试

## 分析入口与产物

`ownership_checking::check_ownership` 提供单文件入口；
`check_compilation_unit_ownership` 消费 validated typed unit 并返回 source-qualified
`CompilationUnitOwnership`。两条路径共享所有权语义，但使用各自产物的 ID，不能混用。

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

## 实现与测试位置

单文件实现位于 `crates/lang-frontend/src/ownership_checking/`，unit 实现位于其
`compilation_unit/` 子树。对应覆盖位于 `ownership_checking`、`ownership_structural`、
`ownership_containers`、`ownership_closures` 与 `multifile_ownership_checking` integration suites。
