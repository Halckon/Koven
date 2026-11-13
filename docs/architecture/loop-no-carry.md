# 循环内有限 no-carry

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改循环内借用准入或关闭时 · **唯一真源**：`ownership_checking/loop_no_carry.rs`、两 checker 的 `no_carry.rs` 与 `ssa/refine/build/no_carry.rs`

## 直线生产合同

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
当前资格允许同源直接 while 及其前、中、后的直线外部 source 消费段。唯一 Borrow
String source 与一个或多个 Borrow 或 own Boolean 参数的顺序与模式来自 typed ABI，
允许其中有未使用的 Boolean。消费段只接受直接、普通、同步、单 source 实参的 Unit
调用；其它语句保留原执行/拒绝边界而无本片 receipt，不因缺新证书收紧准入。完整
入口身份与首边之后的活跃角色分别核验，数量取真实有序元素，不枚举固定拓扑。

真实 builder append 捕获 Borrow 条件的 Read、body BorrowCall、逐 consumer 和唯一 End；
own 条件不产生 Read，由实际表达式求值后的 condition-use checkpoint 捕获。
真实 terminator 与 fold 回调捕获每 peer 五个原块角色（相邻共享 exit/entry）、
正常回边、false 出口及可发生的出口折叠。
Emitted 将这些 producer 标签绑定实际 InstructionId、EntityId 及各块参数映射，不从
最终 SSA 扫描补造 producer。最后独立验证不读取 Plans/Slot/current map，而从实际
ABI 参数索引建立独立角色固定点，不合并同型 Boolean，逐 incoming 检查运输，核 body fresh Result
的准确使用和关闭，拒绝 Result 跨边、false 创建实例、漏块/漏指令/多余定义。

单函数完整核验后仅暂存；整个 unit 的原 verifier 成功后原子发布，后续函数或 verifier
失败撤回整批。核心检查不依赖观察开关，观察关闭不取消验证。封印 FE 资格归属由准入
阶段认证，独立最终图验证不冒充重新验证整个 FE catalog。

该证书证明每次执行有限 body 均创建并关闭结果、正常回边只保留需要的外部参数；它不是已
原生运行任意多轮的记录，不证明终止性，也不扩大语言准入或退休旧见证。

## 同源直接兄弟循环

`borrow_result_support/no_carry::siblings` 只为当前已封印 binding 增加同函数、同稳定
source 的直接兄弟 while 许可。当前和 peer 都独立核原 binding/source loan、准确
initializer/body 与唯一 End；从真实 Function body 的 elements 对照直接兄弟结构。
没有自身事实的 ordinary/carried binding 不能借用别人的许可；不同 root 不准入。

每个 reached while 仍独立走原 NoCarryLease 的入口、cleanup 前关闭、回边和退出
核验。实际 SSA 用不同结果实体，各自 consumer 与 End 在对应 body 内，任何边均不
运输本轮结果；两个 false 退出后仍可消费外部 source。分支/跳转/嵌套边界不变。
执行准入与证书资格分开；额外语句等较宽执行形状不自动获得组合 receipt。不同
flag、Borrow/own 混合及未使用 Boolean 的合格序列使用下面的独立 ABI 角色证明。

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

producer 在真实入口冻结完整 ABI 槽与模式，逐 peer 的条件使用只允许该声明对应的槽；
各边界独立核完整外部 bindings/borrowed 映射，旧 NoCarryLease 不被替换。Emitted 将所有 ABI 槽按原
参数顺序绑定实际实体，最终证书不读取这些 builder 槽。

最终固定点以每个 ABI index 区分角色。各 header/body 保留 source 与当前及后续
peer 需要的 flag 集合，false/exit 只保留后续需求；最后有消费段时保留 source，
没有时为空。共享 exit/entry 的
角色集合须一致。重复使用的 flag 不提前丢弃，最后使用后不强留。每个 incoming 的
实参必须是前驱块自身的唯一对应角色实体，不能用入口旧实体或另一个同型 flag 替换。
这是既有执行合同的独立证据扩展，不新增 FE 资格或循环控制流。


## 未使用 Boolean 的入口消亡

函数 entry 保留并验证全部实际 ABI 参数，未使用的 Boolean 也不能从入口
身份目录删去。entry 到首 header 的真实边仅运输 source 与有实际条件用途的 flag；
未使用的角色不得出现在任何后续块参数或边实参中，也不能成为 Read 或 End 的对象。
这表示外部参数不再需要局部运输，不是给 caller 的原 loan 伪造结束操作。

首 entry 的完整 ABI 集合与 header/body 的活跃集合独立核验；后续 peer 的共享
entry 仍严格等于前一 peer 的 exit 需求，不因本片放宽。重复 flag 的未来需求、
逐 incoming 的前驱局部实体、真实 producer 和原 verifier 后原子发布保持。


## own Boolean 的真实 Value 条件

源码 own Boolean 在 typed ABI 中是 Value 模式，入口来自 bindings；Borrow 条件
来自 borrowed，实体是 Shared Loan。两种模式按参数原 index 保持各自身份和类型，
不能只凭同为 Boolean 互换。source 始终是 Borrow String，不由本片改为 owned。

Name 表达式的实际求值完成后、原 Branch terminate 前，condition-use checkpoint
核精确 UnitExpressionId、span、header、当前阶段和真实返回槽，并且只消费一次。
Borrow 使用已有 Read append 的结果；own 直接使用预先冻结的 Value 槽，没有
Condition 指令或新 Read。每阶段检查全部参数在 bindings/borrowed 中的独占归属、
实际槽类型、dead 状态及完整映射，没有新增临时贷款或结果来源残留。

Emitted 保存真实 checkpoint 完成证据及原 terminator 映射。独立 validator 要求
own header 无指令，Branch 条件等于该 header 自己的唯一条件角色 Value；Borrow
仍验证实际 Read。每条 incoming 同时核局部实体、角色和 EntityType，不接受入口旧
Value、另一同型条件或模式伪换。重复用途与 unused own Boolean 沿原活性规则运输
或消亡，不制造 Drop/End，不扩大循环执行范围，也不证明条件可变或循环终止。


## 前、中、后的外部 source 消费段

有序 peer 对应 n+1 个动态消费段，空段与多调用段均保留原顺序。准入逐调用核 typed
直接声明、Unit 正常返回、Borrow String 实参以及原 LoanFact/参数合同的精确
call、argument、source place 和模式。named 实参、非 Unit、特殊调用、receiver、
Nothing、借用返回或其它 source 不进入本片证书；合法非候选仍走原执行路径。

实际参数绑定完成后先冻结完整 ABI，首段不借首个 BorrowCall 倒推 source。每调用
依次经历实际 operand 身份核验、DirectCall append 捕获、原正常 statement 完成与
清理后的封口。完整外部映射必须保持，前一 loop 真正关闭后才允许下一段。外部
参数直接复用原 source loan，没有新建 loan 的指令，也不伪造 Begin/End。

Emitted 保存实际段块与指令身份；中段可共享前 exit/后 entry，按原序合并而非覆盖
指令清单。detached 验证每调用的实参恰为当前块的 source 实体，拒入口旧实体或
body fresh result 替代。尾段要求 source 经 false/finished/exit 运输至调用后 Return；
不再一律要求末出口为空。全指令覆盖、真实 fold 重放及原 verifier 后整批发布保持。

## CI 故障用例的精确执行入口

`scripts/borrow_fault_tests.json` 枚举 12 个 frontend 与 24 个 codegen 真正 ignored
用例（其中 fresh receipt 16 个、既有 loop receipt 8 个）。CI 的 Targeted Tests
在普通集成测试后调用 `scripts/check_borrow_faults.py`，先核 ignored 列表与清单精确
一致且非空，再逐项 exact 执行并校验实际通过数量。ownership checking 与 refine
范围新增或删除 ignored 必须同步清单；永久忽略的 debugserver 不在范围内。

本地只允许 `--list-only` 编译枚举和脚本的模拟单测，不执行这些故障。普通 cargo
test 不会运行 ignored；入口接线不等于远端已通过，远端结果须另核实际 CI 日志。

## 跨函数发布的普通编排验收

普通测试从真实入口调用两个候选函数，记录两份不同 caller 的 pending，并在原
whole-unit verifier 前观察 published 仍为空；原验证通过后才整体发布。另一路使
后函数遇到既有不支持的字符串插值，必须先有前函数的实际 pending，再核精确失败
span、原 verifier 未运行及整批撤回，随后独立 capture 可成功。两路覆盖 rooted / memory
输入与观察开关，不人工注入错误或伪造 receipt。新增旁听仅为 cfg(test)，不改变
生产准入、原 verifier、drain/truncate 或提交顺序；CI-only 人工故障仍只在远端执行。

CI 清单另登记真实最终 verifier 失败的原子性用例：仅测试专用、一次性 hook 在两份
真实 pending 已生成后移除该 caller 的实际入口 terminator，随后仍调用原 verifier。
用例要求验证确实失败、两份 pending 撤回且 published 为空，并在下一次正常 capture
验证恢复。hook 未武装时无操作，RAII 清除武装；不改生产路径或只读诊断捕获器。
该项本地仅编译和列举，行为是否通过须以远端精确执行日志为准，不能将编译成功
表述为故障验收通过。

既有三 peer 的 CI-only 缺证负控逐项移除每个 peer 的全部指令证据、原边、最后
Return、最终捕获边及每个 fold，并分别重复每个 fold。每次只改独立克隆的 receipt，
原 SSA 保持不变，原样本先通过验证；这是冻结证据完整性测试，不冒称真实 producer
漏回调注入。沿用原精确 FQN，清单仍为 36 项，本地仅编译列举，远端行为待验证。

## 受限双臂前端闭合事实

见[双臂 no-carry](loop-arm-no-carry.md)，只记录前端闭合事实，不代替上述直线后端合同。
