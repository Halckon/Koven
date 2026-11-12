# 循环内有限 no-carry

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改循环内借用准入或关闭时 · **唯一真源**：`ownership_checking/loop_no_carry.rs`、两 checker 的 `no_carry.rs` 与 `ssa/refine/build/no_carry.rs`

## 有限生产合同

函数 body 直接 while，条件为 Boolean 值参数的直接 Name；source 为稳定外部 Borrow
String 参数。body 为一条直接借用声明，随后非空的直线同步 Unit consumer 调用序列。
每轮结果不逃逸，全部暂借返回，最后 consumer 后实际 End，再经过正常回边。
consumer 数量取实际序列长度，不枚举固定拓扑或新增人为语言数量上限。

分支、跳转、嵌套、别名、body 内额外声明、多根、局部 owned source、常量/调用条件
均不在本片范围，保留 L0164。多个兄弟循环各自独立核验，不将其合为多根循环。
用户批准合同见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## 两 checker 的真实关闭

非 cfg 核心 Frame 以真实条件后状态为外部义务基线，由真实 body scope 认领不可复用票据。
原 borrow binding 检查只给当前精确 binding/initializer 放行；实际创建后才记录 acquire。
每 consumer 必须有真实 call/operand 暂借移除及正常返回，最后使用的实际移除和原 End
才生成关闭票据。scope cleanup **之前**检查全部贷款关闭和完整外部义务恢复，正常
pre-merge 再消费一次性完成票据。零轮 false 边沿用原基线，不伪造 body 实例。

file-local 与 compilation-unit 都执行同样协议，observer-off 不关闭检查。
资源快照自然拆入各 checker 的 `resource_state.rs`，仅归一化不含资源的空缓存。
Unit 私有构造的 `UnitLoopNoCarryFact` 同时绑定真实 source/callable/body/initializer、
逐 consumer 及唯一原 End；最终聚合再次匹配原 binding/End，失败撤回资格并诊断。
能力属于同一 ownership 实例，不能从等值重分析结果移植。

## 后端执行与验证

CG 只接受同实例私有能力及原事实的完整一致关系。真实 source 槽在入口存在，fresh
binding 尚不存在；实际 body lowering 后、任何 loop leave 清理前，借用槽和源贷款映射
必须恢复，pending call loans 为空，source 未失效。条件后、false 边和出口同样复核。
原 SSA verifier 保留。此处 `NoCarryLease` 是执行边界检查，不冒充独立生命周期 receipt。

普通测试检查实际 BorrowCall、逐 consumer 与唯一结果 BorrowEnd 均在 body 内，End
先于正常回边，结果不随回边传递；遍历 false 可达路径确认没有本轮借用操作，最后返回。
外部 source 参数不被本轮 End。rooted/memory、observer on/off 均验证实际 LLVM 输出。
原生 false 执行后仍可消费 source，输出 kept；固定 true 参数下本合同自然无限循环，
没有伪造可终止的“一轮/多轮”原生样例，也不宣称终止性。

## 研究证据与剩余范围

旧普通 frame 研究仍捕获 acquire、逐 consumer 正常返回及原 End，并在 cleanup 前
验证义务恢复。合法新片自然产生普通 frame 观察，但它本身不是循环归纳证书。
旧 carried-loan、空 body、外部 consumer-body 观察仍是各自的研究合同；这些观察不
授予 fresh 准入。实际生产准入仅由上述非 cfg 协议提供。

更一般控制流、跨回边存活及多根组合尚未完成；独立 receipt 的更窄范围见下节。
旧 raw/off/oracle 见证保留，不能以本片较窄合同替代。CI-only 故障组只编译，不本地执行。

## 独立 fresh-loop receipt

`refine/fresh_loop_receipt` 为独立协议，不套用旧 carried-loop 的 entry acquire/exit End。
当前资格限函数体完全由同源直接 while 构成，唯一 Borrow String source 与一个或多个
Borrow Boolean 参数的顺序来自 typed ABI，允许其中有未使用的 Boolean。完整入口
身份与首边之后的活跃角色分别核验；own Boolean 或其它前后语句仍保留原执行/拒绝
边界而无本片 receipt，不因缺新证书收紧准入。数量取真实有序元素，不枚举固定拓扑。

真实 builder append 捕获 header 条件读取、body BorrowCall、逐 consumer 和唯一 End；
真实 terminator 与 fold 回调捕获每 peer 五个原块角色（相邻共享 exit/entry）、
正常回边、false 出口及可发生的出口折叠。
Emitted 将这些 producer 标签绑定实际 InstructionId、EntityId 及各块参数映射，不从
最终 SSA 扫描补造 producer。最后独立验证不读取 Plans/Slot/current map，而从实际
ABI 参数索引建立独立角色固定点，不合并同型 Boolean，逐 incoming 检查运输，核 body fresh Result
的准确使用和关闭，拒绝 Result 跨边、false 创建实例、漏块/漏指令/多余定义。

单函数完整核验后仅暂存；整个 unit 的原 verifier 成功后原子发布，后续函数或 verifier
失败撤回整批。核心检查不依赖观察开关，观察关闭不取消验证。封印 FE 资格归属由准入
阶段认证，独立最终图验证不冒充重新验证整个 FE catalog。

该证书证明每次执行有限 body 均创建并关闭结果、正常回边只保留外部贷款；它不是已
原生运行任意多轮的记录，不证明终止性，也不扩大语言准入或退休旧见证。

## 同源直接兄弟循环

`borrow_result_support/no_carry::siblings` 只为当前已封印 binding 增加同函数、同稳定
source 的直接兄弟 while 许可。当前和 peer 都独立核原 binding/source loan、准确
initializer/body 与唯一 End；从真实 Function body 的 elements 对照直接兄弟结构。
没有自身事实的 ordinary/carried binding 不能借用别人的许可；不同 root 不准入。

每个 reached while 仍独立走原 NoCarryLease 的入口、cleanup 前关闭、回边和退出
核验。实际 SSA 用不同结果实体，各自 consumer 与 End 在对应 body 内，任何边均不
运输本轮结果；两个 false 退出后仍可消费外部 source。分支/跳转/嵌套边界不变。
执行准入与证书资格分开；own Boolean 或额外语句等较宽执行形状不自动获得组合
receipt。不同 flag 及未使用 Borrow Boolean 的合格序列使用下面的独立 ABI 角色证明。

## 动态组合证据

合格兄弟共享一个整函数 receipt，内部是有序 peer 证据，而非多个已发布的单loop
receipt。实际入口按 StatementId 激活下一 peer，必须在前一 peer 真正关闭并恢复后，
以其 exit 作为下一 entry。每 peer 标签、原 fact 独占身份及 fresh 结果独立保存。

整函数记录实际 term/fold：中间 peer 无 Return，只有最后出口返回；fold 先消费真实
唯一前驱 Goto，再重写剩余边及共享 exit/entry，不推测最终图以前发生过何种折叠。
最终一次核 whole-function 全集、每 peer 的原身份和顺序、各自 consumer/End，以及
共享外部参数逐入边的准确局部实体。任何一条边都不得携出任一 fresh 结果。
任一 peer 或最终 verifier 失败均撤回整组；仍不声称已运行任意非零多轮或证明终止。

## 多条件参数的活性运输

producer 在真实入口冻结完整 ABI 槽，逐 peer 的 Read 只允许该声明条件对应的槽；
各边界独立核完整外部映射，旧 NoCarryLease 不被替换。Emitted 将所有 ABI 槽按原
参数顺序绑定实际实体，最终证书不读取这些 builder 槽。

最终固定点以每个 ABI index 区分角色。各 header/body 保留 source 与当前及后续
peer 需要的 flag 集合，false/exit 只保留后续需求，最后退出为空；共享 exit/entry 的
角色集合须一致。重复使用的 flag 不提前丢弃，最后使用后不强留。每个 incoming 的
实参必须是前驱块自身的唯一对应角色实体，不能用入口旧实体或另一个同型 flag 替换。
这是既有执行合同的独立证据扩展，不新增 FE 资格或循环控制流。


## 未使用 Boolean 的入口消亡

函数 entry 保留并验证全部实际 Borrow ABI 参数，未使用的 Boolean 也不能从入口
身份目录删去。entry 到首 header 的真实边仅运输 source 与有实际条件用途的 flag；
未使用的角色不得出现在任何后续块参数或边实参中，也不能成为 Read 或 End 的对象。
这表示外部参数不再需要局部运输，不是给 caller 的原 loan 伪造结束操作。

首 entry 的完整 ABI 集合与 header/body 的活跃集合独立核验；后续 peer 的共享
entry 仍严格等于前一 peer 的 exit 需求，不因本片放宽。重复 flag 的未来需求、
逐 incoming 的前驱局部实体、真实 producer 和原 verifier 后原子发布保持。
