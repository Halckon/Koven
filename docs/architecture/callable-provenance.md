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
[收据](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/runtime-constructor-0279/callable-provenance/receipt.json)。
这些来源事实不证明 capture loan 仍活跃，也不代表 source helper ABI、LLVM/native 或
SPEC-0279 的 PR/main 验收已经完成。

## 借用返回调用的辅助闭包实参

本地候选 `36f088c7` 仅修复普通直达非泛型 BorrowCall：两项位置实参，
第一项是 Int 借用返回的来源，第二项是普通 Name 局部闭包。第二项须有同次分析的
精确 Shared 根事实，实际闭包只有一个 Shared capture，且 `() -> Int` ABI
与已定的 callable view 一致。先对真实闭包 owner 建 Shared loan，再用
`CallableViewOf` 生成 view loan；view 不是值、Place 或可写借用，不放宽 verifier。

原样用例中内层调用先结束 view、再结束它的闭包 loan，之后才释放闭包。
从原结果 loan 读值后，依原顺序结束结果与来源 loan，再释放原 Boxed；
未另造捕获或返回来源事实，其他实参形态和 Range 路径不随之改动。
Mac/Linux 默认与带观察开关构建的定向各 39 项通过。双布局核实际 SSA、
LLVM 与原生的同分配唯一释放。原用例的 `observe` 是空函数，native 结果只证明
执行与释放，不是对包装后的 Int 做外部输出验收。
