# ADR-0036：独占闭包捕获使用持久根槽

> **性质**：架构决策记录 · **状态**：proposed · **读取时机**：接通独占闭包捕获的 SSA、LLVM 或析构时 · **唯一真源**：本 ADR

## 状态

proposed

## 接受依据

待接受。语言行为由 [Guide 07](../../guide/07-calls-lambdas-closures.md#mutating-closure)
规定。本提案记录现行规则进入 SSA 和 LLVM 所需的内部表示。

## 背景

[SPEC-0392 §49–50](../../specs/active/0392-compiler-layered-review.md#50-r3-moveonly-捕获根替换)
发布独占捕获及具体 MoveOnly 根替换的 Phase 3 事实。
[ADR-0009](../accepted/0009-concrete-closure-internal-abi.md) 的环境只承载 Shared 和 Owned。
现有 RootPlace 把一个 SSA 值初始化到临时地址，原 SSA 值继续可用。若仅让 thunk 经此地址写入，
外层绑定在下一次读取时可能仍取旧值。环境释放也必须匹配前端实际 loan 结束边，尤其是
进入一个不再使用闭包的分支时。

## 决策

1. **持久根槽。** 从已验证的独占捕获描述符确定源绑定。每个动态绑定实例初始化一个 typed SSA
   槽，在生命周期内统一由该槽处理读取、替换、借用和最终析构。独立的 Copyable 读取结果
   是读取时的快照。槽地址在活跃 loan 期间保持同一身份，跨 CFG 经显式参数传递，循环内
   动态实例按各自生命周期初始化和结束。函数帧内可复用已经结束的槽，回边不得为同一
   绑定反复累积栈空间。由目标 DataLayout 决定大小和对齐，沿用栈内存及既有 concrete
   environment，无额外堆分配或运行时标签。
2. **环境权限。** 新的 Exclusive 槽身份与 Shared、Owned 区分。其物理字段保存源槽指针。
   构造消费一个活跃的独占 loan 的使用权，绑定该 loan 到唯一环境和唯一槽。构造后调用者
   不得读取、重借、写入、显式结束或把该 loan 再作为 inout 交付。环境销毁结束 loan，
   源槽和其中的值继续由原 owner 管理。源槽在 loan 结束前不得被读取、替换、转移或析构。
3. **Thunk 入口。** 隐藏环境参数继续只读，thunk 通过专用投影从验证过的具体环境入口取得
   对应槽的独占访问。核对 thunk、布局、字段和 target 身份。普通 Shared 引用、任意聚合
   投影或裸调用无法构造此权限。同一调用内的派生访问必须先结束，才能执行写入或归还权限。
   调用和同步 callable view 沿用现有借用逃逸约束。无法证明同一环境的独占调用不重叠时，
   保持拒绝。Abort 结束执行，沿用无 unwind 的约定。
4. **提交与退出。** Copyable 根在 RHS 正常求值后经槽写入，外层随后的读取从同一槽获取
   当前值。MoveOnly 根只按匹配的 ReplacedCapture 事实提交，完整 RHS 正常完成后读取
   槽中的当前旧值、按类型析构，再写入新值。该操作保留已初始化槽和捕获 loan，直至原
   前端释放点。提前退出不得触发替换。分支入口需要独立的已验证清理事实，不能将
   BranchExit 重新解释为入口。缺事实、来源冲突及仍待验证的路径保持可诊断的拒绝。

## 替代方案

- **每次调用结束后复制回原绑定。** 调用与 loan 结束可能相隔多次调用或跨分支。过早恢复
  允许外部访问仍在借用的源。延迟到最后释放再统一写回，则每条控制边都必须同步所有
  SSA 绑定快照，MoveOnly 根还要处理旧值已在 thunk 内析构的期间。
- **共享字段直接升级权限。** Shared 投影的来源证明不足以授予独占访问，也无法区分同一
  环境内的多个独占槽。
- **新增堆环境或擦除调度。** 当前源槽与 concrete environment 足以承载已证明的同步路径，
  无需改变 ADR-0009 的分配和 callable 身份合同。

## 后果

- SSA verifier 需验证槽初始化状态、唯一环境持有、权限投影和跨边依赖。LLVM 只消费已验证
  的地址和析构顺序，现有 Share/Own 环境表示继续适用。
- 首批原生验收包括别名持有、同步 helper 连续调用、最终源读取和无调用分支入口释放。SSA
  负例覆盖重复接管 loan、伪造入口、捕获 loan 的外部操作以及提前释放仍活跃的子借用。
- MoveOnly 验收覆盖嵌套和连续替换、提前退出、旧值析构顺序及最终一次根清理。需同一候选
  SHA 的 Apple Silicon macOS 与 x86_64 Linux 实跑，再纳入全链路复审。

## 关联

- 相关 Spec：[SPEC-0392](../../specs/active/0392-compiler-layered-review.md)
- 拟局部扩展：[ADR-0009](../accepted/0009-concrete-closure-internal-abi.md) 的 capture 槽类别与
  [ADR-0034](../accepted/0034-refine-closures-and-callable-views.md) 的 thunk 接入
- 取代的 ADR：无
- 被以下 ADR 取代：无
