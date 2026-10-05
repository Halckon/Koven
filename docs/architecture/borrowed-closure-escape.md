# Borrowed closure 的 owned escape 检查

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 shared-capture 环境的 SSA 交付或值证明时 · **唯一真源**：`lang-codegen/src/ssa/verify/closure_escape` 及直接测试

模块类型/deinit 与函数原有结构、类型、dominance、ownership 验证成功后，独立 guard 检查
Return、Value 调用、aggregate/tagged/heap/shared/container 存储与 replacement、projected-place
Mutate 的 owned 交付。错误使用现有 OperationContract，location 和 Origin 指向真实消费点。
局部 ClosureConstruct、Drop、同步 Borrow 与 RootPlace 局部 Mutate 不一律拒绝。

类型缓存用迭代图遍历沿实际 owned 存储传播 may-contain：Shared capture 是种子，Owned capture、
字段、owner payload、nullable inner 和 container element 是存储边。FunctionPointer 签名与
SharedReference target 是叶子；类型判定不证明当前值实际有捕获。

按程序点的内容证明覆盖 Value、Place、Loan，保留空容器、null、inactive tagged variant 及其
已知无捕获包装/CFG 运输；合法 Loan retain 从当前借用内容读取证明，未知 entry loan 不一律 clean。
写入更新 owner 当前内容；RootReplace/Swap 分别运输当前旧值、替换值及交换后的两份内容。
局部 root 强更新复用精确 owner/place 配对；inline FieldPlace 还须证明沿同一root的字段地址路径。
heap/shared/container/tagged payload投影不能凭alias overlap视作被替换的新root内容。
未知 may-contain 值缺少无捕获证明时在 raw SSA owned 交付被保守拒绝，不新增 Source L0137 规则。

直接测试先确认旧 verifier 接受合法夹具，再记录真实拒绝测试失败；独立审发现的 Loan retain
及预创建inline字段证明误拒已真实复现并修复。新 guard 已通过41个逃逸正反例，类型分类通过4个类型图测试。
完整消费者及交付验证以 Spec 验收账本为准。
此防线不传播嵌套 capture 的 LoanId 生命周期，相关局部清理问题仍未封闭；也不代表
Array/List runtime-length 源码构造器已经接入。
