# Callable 源码 ABI 与 borrowed runtime construction

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 source callable 具体签名、thunk、runtime initializer 或同步清理时 · **唯一真源**：`lang-codegen` 的 source ABI 代码与测试

## 已接入的来源链

两入口消费[冻结实例计划](callable-instance-plans.md)的具体 source owner、callback 槽与调用路由。
source 函数、无捕获指针和 concrete closure 的参数/返回签名分别声明，再生成 source body 与
thunk body；同签名的不同 lambda 不共用环境布局。capture 字段按已发布 mode 与只读具体类型
映射，运行期环境值不进入 instance key，也不在 frontend 类型 arena 新增类型。

single 的布局/形成/签名位于 `lower_frontend/source_closure/`；unit 的具体 ABI 位于
`unit_lower/callable_abi.rs` 和 `unit_lower/closure/`。unit 将选定 runtime initializer token
冻结在 plan 中，仅选定 lambda 的 Resource/deinit 需求进入已有 nominal worklist。
正常无捕获 factory 返回沿 source memo 查找，不按 Fn 类型猜测返回来源。

single 当前 bare named expression 仍可能是 Deferred(OverloadSelection/Call)，没有 sealed
来源；runtime initializer 在实际 operand Span 拒绝它。单文件 pointer 正控使用无捕获
lambda 与已验证 factory。unit 对确有 canonical Function、sealed selected KnownFunction
及匹配名称 target 的直接具名 initializer 形成 typed address；alias 沿已有 binding/fact 路径。
无 LoanFact 的 direct source address 使用同步调用帧的 owner/loan 槽，结束 loan 后 Drop address。
缺失类型/来源事实不能由 AST 名称或候选 overload 补齐。lambda layout 同样只取 Phase 3
实际发布的 Lambda origin，return 后不可运行的 lambda 不产生 thunk；可运行 nested
lambda 仍是明确拒绝边界。具体构造 storage 使用结构化 descriptor demand 与相应
ownership construction/loan 计划，类型元信息不当作 runtime layout demand。

## 求值、借用与清理

runtime constructor 消费 frontend descriptor：size 求值一次后建立第一 Shared Loan，读取
Int 快照并在显式 Abort CFG 拒负；之后求 initializer 一次、建立或复用 Shared Loan，再执行
`ContainerGenerateBorrowed`。正常 CallReturn 结束参数 loan，并按 frontend facts 清理 temporary
initializer/capture；命名 callback 保留供后续调用，Abort 不展开清理。

generator 的 LLVM callable 与 index storage 在 preheader 准备，循环按逻辑索引调用；
Unit void 仍每个索引一次。Shared capture follow 及其父环境 loan 通过 CFG carrier 同步
重绑定；普通 when 的未匹配分支单独保存和恢复 capture map，不能复用兄弟分支的 LoanId。
SSA closure Drop 已释放的 capture dependency 不再重复 BorrowEnd。

unit 普通 Borrow Resource 参数与 closure capture 读取 Copyable 字段时使用 active parent
heap loan。String 等 MoveOnly 字段借用必须有单字段 LoanFact，核对 typed projection、root、
field symbol、field index 与 target 后建立 `SharedHeapFieldLoan`；child 结束而 parent 保留。
不通过 whole-owner Read 替代字段视图，既有 owned-local 字段路径保留。

## 验证边界

两入口各144个 Int/String/Resource/Unit × Array/List × 三环境 × named/temporary × 0/1/3
源码组合已通过 SSA/LLVM。两入口各144个普通 native 组合及各48个 generic helper
组合（显式与推断 T，single 同文件、unit 跨文件）已实际 object/link/run；Resource trace
另各六个用例验证升序 callback、逆序 element drop 与 CallReturn temporary Owned
environment 清理。两入口各六个正常求值/语言级拒负用例核对 size→factory→callback 顺序；
公共 CLI 与全部交付闭环仍由 active SPEC-0279 的实际账本逐项记录，不从源码通过外推。

helper 另各48个普通 native 用例覆盖 `Array<Int>`、`List<Int>`、Int 字段 value class 和
泛型 `Holder<Int>`，组合两种外容器、三环境及显式/推断 T；核内层长度、实际末元素值、
外层长度与回调次数。证据见[元素扩展收据](../development/evidence/runtime-constructor-0279/storable-helpers/receipt.json)，
不外推所有可存储 T，也不把无析构日志解释为零 Drop 或 allocation/free。

普通 unit 入口的 while 与未提交 temporary call operand 仍受既有 control-prefix 门禁；
只读迭代/常量 view 的既有能力不外推为任意 CFG 支持。Source/native 原始失败、夹具/缓存
错误与通过结果见[分层收据](../development/evidence/runtime-constructor-0279/source-abi/receipt.json)。
single 静态容器预登记仍沿 typed descriptor，缺少通用已求值容器事实；不可达静态
unsupported element storage 的边界未在本片扩展（原1aef99b已同样预登记静态 descriptor）。
本地安全验证、故障注入/校准以及逐指针 allocation/free 在本轮未执行。
