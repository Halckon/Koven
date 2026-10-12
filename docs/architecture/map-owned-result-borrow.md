# owned Map结果的非消费借用

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改窄化 owned remove 的 Borrow 或 nullable view storage 时 · **唯一真源**：代码与测试；验收账本见 [SPEC-0299](../specs/active/0299-owned-map-result-borrow.md)

[SPEC-0299](../specs/active/0299-owned-map-result-borrow.md) 的首片支持具名 whole-root 删除结果，
在直接非空 if 分支（含透明括号、`== null` 的 else）同步只读调用。single 保留既有
NullableBranch 分流，Group 只透明定位 typed descriptor，cleanup 仍从原完整 condition 回放。
unit 核验 call/argument、Shared/end-span、non-null-use、symbol、声明/窄化类型、注册表示与
实际 SSA owner；沿 block incoming edges 必须追至同一个 MapRemove 结果 index 1。
每次调用借入时使用既有 NullableBranch proof，缺失边为既定 Abort，不用 MapResultUnwrap、
NullableTake 或隐式 clone。binding、loan、capture、result-source 与 pending operands 完整运输；
专用 Shared proof 后缀单独校验，旧 CFG rebind 的 arity 不放松。调用按原 loan fact 结束 view，
后续仍可再次 Borrow、显式 `!!` 消费或按原最后使用点清理；Resource 仍在词法结束点 deinit。

unit 新分流只接非嵌套直接表达式语句的 Owned-result 调用，普通 Unit 结果属于这一模式。
Borrow/Carrier 返回、loop 和特殊 receiver 的 key/index/element 内嵌调用是明确后继，
不是本片已完成内容；位置 gate 不改变旧 single 或 Copyable 路径。原 Map/key 可以结束或变异，
被删除结果的 loan 依赖实际独立 owner，不能误绑容器槽位。

LLVM aggregate payload 继续用 entry stack slot 提供非 owning view。HeapOwner pointer-niche
view 现在同样交付**存放 handle 的 slot 地址**，callee load handle 后投影，修复旧 single plain
Resource 虽通过 SSA/verifier 却读出空字段的 native 错误。每个静态 proof 独立 entry slot，
每次分支执行写入当前 handle；不增加 owning copy、retain 或 drop。SharedOwner/Rc 的旧
retain/payload view 消费裸 handle，本片保持该分流，未统一 ABI 或 runtime。
普通源回归覆盖两次 Borrow 后 `!!`、String/inline 最后使用、Resource 词法 deinit、missing、
同 call 两 Borrow 和 prefix 运输、single 既有 loop 重入，以及非 Map class/Box/Rc。
stdout 只证明字段/析构事件，完整泄漏或内存安全不由本片声称。


非Map class字段回归保留两借用后`!!`；删去后续消费的另一个一般nullable候选仍在lowering
返回`InvalidSsa`，未在本片定位/修复。已通过的class/Box/Rc回归不代表一般nullable最后使用已闭合。
