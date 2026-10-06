# Callable 实例与冻结路由

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 helper 实例身份、来源运输或 source call 路由时 · **唯一真源**：`lang-codegen` 的 instance planner 代码与测试

## 身份与输入

两入口 planner 消费同一已验证链的 typed call 与[ownership 来源事实](callable-provenance.md)。
single 私有调用方先执行既有输入校验，unit 从已验证 facts/view 进入；源码 ID 相等不能代替
分析身份。planner 不按 Fn 签名猜测环境，也不重新执行名称、类型或所有权分析。

source key 保留原 target、类型实参以及 unit 的 StaticSelf/deinit 身份，增加按声明参数位置
递增排列的稀疏 callback 槽，本片仅接受同步 Borrow Fn 参数。Lambda 身份包含具体 source owner 与实际表达式 ID；已选定
ordinary named function 使用其 source 身份。Parameter 转发复用原 token，环境运行期值、
LoanId 和 SSA/LLVM function ID 不进入 key。Deferred/Unknown 来源仍被结构化拒绝。

私有共享 arena 使用两个有序 key map 与一个 append-only record Vec；source/callable token
只能引用先前已登记、种类正确的 record。key 不递归嵌套，完全相同的 key 先去重再扩展。
generic、unit StaticSelf 与 callback specialization 共用既有 1024 source 实例预算，一项实例只计费一次。

## 路由与边界

plan 保留 `(source token, call expression)` 到完整 callee key 的只读路由。unit 路由还保留
原 resolver 的 delegation 产物，driver 将整个 source plan 移交给 source 函数及 lambda thunk；
ordinary static call 直接查询该路由。lambda 内调用继承所属 source owner，hidden deinit
拥有自己的 source token。两入口 source callback ABI 的消费见[源码 ABI](callable-source-abi.md)。

pointer factory 的摘要按具体 factory source memo，核对声明及调用位置的具体 Fn 类型，
无捕获返回 lambda 归属 factory source；named function 依赖进入原 worklist。
普通非 Fn 数据参数保持原规划路径，不因 callback discovery 增加 concrete layout 门禁。
捕获 Fn 的环境缺少本片 concrete ABI，在 capture 引用位置返回 UnsupportedNode。

实现位于 `ssa/lowering_support/callable_instances.rs`、`lower_frontend/instances/` 与
`unit_plan/callable_instances.rs`。本页描述 planner 事实；source runtime constructor、helper
closure 参数/返回 ABI 与native事实见[源码ABI](callable-source-abi.md)，逐项验收及实际
PR/main交付见[0279账本](../development/evidence/runtime-constructor-0279/delivery.json)。
