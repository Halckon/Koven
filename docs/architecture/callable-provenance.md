# Callable 来源事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：消费 callable 来源或 pointer factory return 时 · **唯一真源**：frontend ownership 产物与对应测试

两入口的既有 sealed ownership 产物提供只读 `callable_origins` 和
`pointer_callable_returns`；事实沿原 typed/ownership 身份链发布，不增加独立验证凭据。
来源区分 Lambda、真实 Function entry 参数、已唯一选定的普通 source function 与
已完成 pointer factory 的调用结果；capture 运行时值不参与来源身份。
单文件 deferred function reference 保持 deferred，不能凭 Function 类型补出已知来源。

私有来源图在一次分析内共享 arena，绑定、分支与循环通过有限方程合流。
while 条件读取预先建立的 header，正常回边和 continue 更新 header，break 仅进入出口；
for source 不重复求值。worklist 中每个节点最多上升两次，反向边增量传播，图求解为
O(V+E)；此界限不代表整个 ownership 分析或查询接口的复杂度。

返回摘要仅接受已完成 source body 的唯一正常交付，且来源为无捕获 lambda 或已选定普通
source function。保留实际 return operand（包括 Group）、Span 与声明 Function identity。
Abort、lambda 内层返回、多个正常返回、未知或 capturing 来源不能形成该摘要。
所有 body 完成后才验证 FactoryResult，避免 caller-first 顺序改变事实；P2/P3 错误清空
两个公开表，loan、capture、drop 与已有 deferred 门禁仍由原 checker 决定。

本片的实现、有效红测、复审修复及验收 raw 见
[收据](../development/evidence/runtime-constructor-0279/callable-provenance/receipt.json)。
这些来源事实不证明 capture loan 仍活跃，也不代表 source helper ABI、LLVM/native 或
SPEC-0279 的 PR/main 验收已经完成。
