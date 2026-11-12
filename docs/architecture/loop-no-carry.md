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
首证书仅限函数体唯一 while、两个 Borrow source/flag 参数，参数顺序来自 typed ABI。
own Boolean、兄弟循环或其它前后语句保留原有执行/拒绝边界，不因缺新证书收紧准入。

真实 builder append 捕获 header 条件读取、body BorrowCall、逐 consumer 和唯一 End；
真实 terminator 与 fold 回调捕获五个原块、正常回边、false 出口及可发生的出口折叠。
Emitted 将这些 producer 标签绑定实际 InstructionId、EntityId 及各块参数映射，不从
最终 SSA 扫描补造 producer。最后独立验证不读取 Plans/Slot/current map，而从实际
ABI 参数建立 Source/Flag 角色固定点，逐 incoming 检查运输，核 body fresh Result
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
此为兄弟执行准入，不扩独立 fresh-loop receipt 的 whole-function 单 while 资格。
