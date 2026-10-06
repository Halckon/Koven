# 同步 Borrow 容器生成的 SSA / LLVM 事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 borrowed generator 或 callable 当前捕获证明时 · **唯一真源**：SSA model/verifier、LLVM container/closure 与直接测试

`ContainerGenerateBorrowed` 显式接收 owning container 类型、Int length Value 和 Shared
initializer Loan。callable 参数为 Shared Loan(Int)，返回元素 T；Unit 的已有 void 返回形成逻辑
Unit。operands、render、类型合同、dominance、active loan 和 owning result 登记已一致接入。
旧 `ContainerGenerate` 的静态 Value(Int) signature 保持，两个路径共用受检分配与生成循环。

LLVM callable 准备、内联 environment bit storage 和 index storage 位于 loop preheader，
循环只更新 index、调用 prepared callable 并写入元素槽。pointer 不创建 environment；concrete
closure 不隐式 clone/retain 或 heap allocation。Unit 零 stride 仍按逻辑索引调用。

新 generator 的捕获检查读取指令前的 callable 内容证明；只按原 ValueId 搜索构造指令不足以
处理 root 覆盖。精确 whole-root 写入强更新，未知 alias 弱更新；RootPlaceTake 读取当前内容。
CFG 参数同时运输内容和捕获 LoanId，join 合并实际入边依赖。capture-free 投影可证明为空；
未知 borrowed 投影结构化拒绝。外部 Borrow(Fn) 参数不虚构 caller-local LoanId。
内容证明与 SPEC-0278 known-clean 证明共用确定性 worklist，保留后者旧 transfer/OR join 合同。

module type/deinit 图失败时仍检查函数结构、CFG 与 dominance，跳过要求已验证类型图的
ownership/content/escape 阶段。未引用的非法 container element ID 有真实先 panic 后结构化
Err 的回归证据。新测试25项、旧消费者及独立审阅见
[切片验收收据](../development/evidence/runtime-constructor-0279/borrowed-generator/receipt.json)。

`SharedReferenceFollow` 把 active Shared Loan(SharedReference<T>) 转成 Shared Loan(T)，
读取 slot pointer 后交给既有 Read。父/祖先 loan、CFG 重绑定与 provider lineage 保持有效；
引用槽无 capture 不等于其 callable target 无 capture，两个内容证明均采用 target 的保守默认。
7 项定向及32项 borrowed 回归通过并经独立审阅，见
[共享引用读取收据](../development/evidence/runtime-constructor-0279/shared-reference-follow/receipt.json)。

这里只交付 raw SSA / LLVM 工程切片，源码构造/helper及资源 native 终点尚未验收。
ConcreteClosure RootReplace/RootSwap 保留既有拒绝；一般 BorrowEnd、
Drop、DirectCall 的 nested LoanId 生命周期仍开放，不能由新 generator 的窄域证明推导为已关闭。
