# ADR-0040：有覆盖证明的 loan 生命周期与统一清理顺序

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：设计完整 Unit loan 结束流、来源延续和 absence/顺序见证退休时 · **唯一真源**：本 ADR；Borrow 语义与 ABI 沿用既有决定

## 状态

accepted

## 接受依据

2026-11-10 用户询问推荐方案后，对“动态 owner 和普通借用并行，随后动态 capture；
owner 先做有界真实 P/Q consumer，借用建立完整生命周期证据”回复：
“可以，先按这个来，后续有问题再review”。据此接受本 ADR 的推荐方向。
首片范围与验证由 SPEC-0341 的“已批准 owner/loan 并行首片”限定；接受不表示实现完成，
不接受 ADR-0025/0036，不启用 Guide 之外的语言能力。动态 capture 的后续实施另按
ADR-0039 的依赖与审议处理，不因本次接受而声称其 ABI 或阶段映射已经解决。

## 背景

main `3fe1920a` 已有直接调用、Exclusive prefix、receiver、String clone 结束观察，以及
同步 consumer、显式 binding、wrapper return 的 source token 交接。`loan.rs` 明确说明
`end_span` 不是实际结束点；`preceding_call_loan_ends` 只计部分动作，不含 receiver/result/drop；
forward 是来源转交而非 end。各表的空集合因此不能证明整个函数“没有遗漏 loan end”。

在途 temporary/clone/concat/Rc/parameter reborrow 分支延伸的是已有实际事件观察，属于
实现增量，应复用而不重写。真正长期缺口是：如何证明已建立的每一条 loan 在每条路径上
有合法后继/终止，以及如何把 result、source、receiver、capture/provider 和 owner drop
放在可复核的同点顺序中，使 Single 的完整 absence/唯一结束 oracle 可以退休。

## 决策

### 1. 单一真实动作来源，有限 CFG 生命周期产品

推荐在 Unit 的实际建立、激活、交接和结束位置保存原 token 身份，统一汇入有路径归属的
生命周期产品。旧只读切片可在迁移期成为该产品的投影；不得新增另一套推理 loan 的算法。
第一步可以先增量补观察，但“Complete”只在覆盖认证器验证完整范围后发布。

身份绑定同轮 analysis/source/callable、实际 call frame/operand、target/place/projection、
模式及 parent。嵌套调用、同点多个 receiver、同源兄弟和不同动态激活不得仅凭 Span 或
symbol 相等合并。callee entry loan 与 caller loan 保持各自本地身份，通过已验证参数/
结果来源映射连接，不直接跨函数复用 LoanId。Borrow/Inout 参数不产生 payload owner。

事件区分 Begin、ReceiverReserve/Activate、Reborrow、ResultHandoff、Binding/Forward
Transfer 与 End；名字仅为合同术语，不规定新的 public Rust API 或语法。实际正常完成、
提前 Return/Break/Continue 和 Abort 边分别建模；未求值 operand 没有 Begin，也没有 End。

### 2. 结果与来源组成可验证的依赖关系

普通借用结果继续严格只有 Guide 允许的唯一非 owning 来源。结果 loan、parent/source、
同步 consumer、借用 binding 与 wrapper return 是不同角色；handoff 不结束来源，不复制
其义务。多个子 loan/别名都结束前父权限不能恢复，普通调用期 loan 不因 callee last-use
缩短；Inout 来源的 Shared result 结束前仍限制父权限。

callee 的每条正常 borrowed return 证明同一声明来源的实际投影/延续；caller 根据已验证
签名和实参 mapping 建立本地关系。wrapper 转发交付 live obligation，不能把返回点算成
source End。结果转交与 owner Value delivery 的身份不可互换。

只在已证明的有限控制流上运输上述关系。跨边不能把来自不同 root 的 Shared loan 合成
语言禁止的条件借用值；同一合法来源的控制路径交接也必须有完整证明。受限 last-use
仍从别名、投影、子 loan 和未来使用事实验证，不以 owner ASAP 或新 coverage 标志代替。

### 3. 路径内统一清理序列，不是全程序时间线

推荐每个真实 source-qualified cleanup 点/控制边具有唯一的有序动作段，身份还包含
所属 callable、路径及必要的 frame/激活关系。同点出现多个真实执行片段时保留段身份，
不按 Span 合并。序列包含相关 call/result/receiver/capture/provider end、权限恢复及
owner/drop 动作，只记录 planner 已完成/发布的真实顺序，不事后排序拼接各旧表。

同一段内可比较 End 与 Drop 的先后；互斥分支各有自己的段，循环保存模板和边关系，不
编造跨轮全局时钟或线性“运行日志”。元数据的稳定顺序不等于动态执行顺序。
依赖检查要求 child/result 先于其 parent 恢复权限、所有有效 loan 先于 source drop；
Resource 逆声明、temporary 逆求值和 replace 提交仍服从 Guide，不由本 ADR 重排。

同一来源在多个表出现时共享一个动作身份，投影不能再执行一次。后端仅消费一个获验证
的动作版本；迁移每一族时同步撤掉旧执行责任，不能新旧 End/Drop 双跑。

### 4. Complete、部分覆盖与失败必须可区分

推荐覆盖声明显式记录 callable、控制区域、动作家族和协议版本，并由独立 validator
验证该范围内所有真实 acquisition/交接及每条可达边。不要把“未触发 observer 拒绝”当成
完备证明，也不能通过删 unsupported 样例或忽略未登记家族使计数归零。

- Complete：对应已声明范围的每个建立/延续义务均在每条路径上闭合，或在明确出口合法交付。
- Unsupported/Partial：普通合法源码尚未覆盖；没有结果或空表不能作 absence 证据。
- Invalid/Deferred：原分析不允许发布成功产品，全部 raw 与公开新表撤销；错误诊断保持原义。

上述是候选状态含义，具体枚举及是否把 Partial/Unsupported 分开由实施合同固定。
Complete 不一定表示事件数大于零：合法零事件须有覆盖了全部相关控制/调用的正面证明。
也不表示所有 loan 都 End：正常 borrowed return 可以显式移交，Abort 没有 unwind；
验证器必须区分“合法转交/终止执行”与“丢失结束”。

任一 source 失败/deferred 时遵守现有 compilation-unit 的全局原子撤销边界；不允许仅
公开包装器隐藏 raw 候选泄漏。各 private frame/sink 在错误传播前恢复，不能污染 sibling。

### 5. 与 owner/capture 及后端的边界

目标为与 [0038](0038-path-qualified-owner-transport.md) 的 owner 边和
[0039](../proposed/0039-dynamic-capture-instance-provenance.md) 的实例/槽建立显式引用，不合并成
无类型大 context。普通直线调用可以先形成自己的完整覆盖，不必等待动态 capture ABI。
若声称整个 callable 完整，则所有真实涉及的家族必须覆盖，不能只跳过 capture/provider。

refine 仍消费 validated 事实，SSA verifier 继续独立校验 Begin/End/overlap 与 edge transfer；
LLVM 继续 ADR-0016/0029 的地址 ABI，不增加 first-class reference 或借用计数 runtime。
旧 Single 的精确身份、完整顺序、缺席和资源 oracle 在相应 Unit 能力真实接线前保留。

## 已接受的推荐选择

| 决策 | 推荐 | 其他选择及限制 |
|---|---|---|
| 统一顺序来源 | 同一真实 cleanup 动作源，旧接口作只读投影 | 事后 merge 多表会遗漏未公开动作和路径身份，不推荐 |
| 覆盖粒度 | 显式可验证区域/家族，另有整 callable 完整声明 | 仅全局布尔简单但妨碍增量；无覆盖的空表不能证明 absence |
| 首批 | 复用现有完成分支，闭合普通 direct/result/receiver 的顺序与撤销 | 不先写完整 NLL，也不等待所有 closure native ABI |
| retirement | 每项旧 oracle 对应可重复的 Unit 同强度证据才删 | 仅 grep 无调用或两模式都接受不够 |

本次接受表中的推荐列。拟纳入的 Inout-forward、parent binding、provider、projected/nullable 与
嵌套结果范围由具体 Spec 和 Guide 准入共同决定，不能由“通用图支持”自动启用。

## 替代方案

- 继续按每种返回值/receiver 新增独立结束列表：能交付小片，但不能作为完整生命周期终态。
- 由源码 span/end_span 或 drop 反推 End：无法区分来源交接、未求值实参与 Abort，不采用。
- 在 codegen 重做借用分析或用隐式 retain 兜底：违反事实权威及既有 ABI/语言规则，不采用。
- 直接移植 Single 整套状态机并双跑：延长双实现，只有原源/oracle 可复用，生产责任必须唯一。

## 后果与验收

收益是可证明顺序与真正 absence，具备删除旧 witness/producer 的基础；代价是需对现有
raw 表、全局失败门与下游消费逐族对账，不能只增加若干 End 事件。

- 覆盖正常返回、later operand Return/Break/Continue、嵌套调用、命名参数求值顺序、
  Two-Phase receiver、Shared/Exclusive reborrow、temporary owner 与同源多个独立 loan。
- 覆盖 source→result→binding/consumer/wrapper、parent/child/sibling、投影/provider、
  Inout 来源 Shared result；来源 End、结果 End、权限恢复和 Drop 分别核真实身份及顺序。
- 正面验证 Complete-empty 与合法 handoff；反面对照 Unsupported-empty，未求值实参和
  不可达后缀不得造事件，Abort 不展开。含 Abort/注入的测试按仓库 CI-only 约束执行。
- 非空合法前缀后追加独立 source 错误/deferred，直接验证 raw/公开全撤销及 sibling 隔离。
- 独立 validator 检查重复/漏 End、错 parent/source、外来分析、错控制边、End-before-child
  与 Drop-before-End；破损/注入负控只在 CI，普通源码测试不替代这些守卫。
- 双 Unit 入口保留原诊断全文及旧生命周期 oracle；Phase 3、SSA、native、释放计数分账，
  精确组合头双宿主 CI 与独立复审后才认领相应退休；本次接受没有运行或通过这些验收。

## 关联

- 相关 Spec：[SPEC-0341](../../specs/active/0341-shared-lowering-core.md)。
- 当前事实：[所有权](../../architecture/ownership.md)、[receiver](../../architecture/receiver-borrows.md)、
  [同步结果](../../architecture/synchronous-borrow-result-arguments.md)。
- 取代的 ADR：无；延续 0016/0029，接合 0038/0039，不改写语言、参数/结果 ABI。
- 被以下 ADR 取代：无。
