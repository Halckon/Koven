# ADR-0037：一次根替换后 Break 的独立 owner 出口

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：实现或消费循环内根替换后 Break 的 owner 观察时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-11-10，用户在明确询问“接受这个限定范围的 ADR-0037，并据此实施”后回复“同意”。
接受范围只包括一次 root Replace 后立即 Break 的双 catalog 身份与两条独立出口；
不开放一般 phi、共同出口或后续消费。此前设计范围确认与此次正式接受分开记录。
本次按 [ADR-0001](0001-record-architecture-decisions.md) 完成 accepted 迁移与库存同步；
实施及实际验收由 [SPEC-0341 §180](../../specs/active/0341-shared-lowering-core.md#180-一次根替换后-break-的协议提案)维护。

## 背景

现有闭合 While 产品要求 body 中所有实际 producer 已消费；不变输入产品则要求输入 owner
在所有已观察边界完全相同。将外层 root 从 P 替换为 Q 后立即 Break 不满足任一合同。
扩大旧 validator 会掩盖未完成的控制边证明，尤其不能把 condition-false 的 P 当成 Break 的 Q。

[Guide 10](../../guide/10-ownership-borrowing-drop.md#原地置换原子原语replace-与-swap)
已规定 replace 的原子交换、旧值返回和保持完整初始化；其 ASAP 与 jump 规则仍是语言真源。
本提案只观察现有合法语义，不改变 drop、借用、控制流、诊断、SSA、LLVM 或 runtime 行为。

## 决策

### 首片边界

只准入普通非泛型顶层 Unit 函数：一个无 mode marker 的值模式 Boolean 参数；函数体依次为一个外层
`var held = "outer"`、一个 While、一个直接 `val retired = held`。While 条件为该参数
的 Name；body 恰为 `val previous = replace(&held, "next")` 与一个 bare Break。
两个字符串均为无插值字面量，replace 必须匹配现有已检查的 String root primitive descriptor。
held、previous、retired 的类型均须精确为非 nullable String。显式与隐式 Unit 共用同一规则。
变量名和字面量内容仅为示意，准入核实际 symbol、类型与结构关系，不按源码文本匹配。
尾部绑定只确保 held 的真实 liveness，不作为新后缀能力。

首片不接受 Group/Not、第二参数、别名输入、多个 owner、额外 body 语句、字段/index、
opaque call、分支、Continue、正常回边或嵌套 loop；这些限制只限制观察，不增加语言诊断。

### 两个实际 catalog，不重写 token

输入 catalog 恰含 prefix 实际构造的 P；body catalog 恰含 fresh body sink 实际构造的 Q。
两个 catalog 属于同一 callable/source 和同轮根分析，但拥有不同的私有 catalog 身份。
P、Q 必须各自在原 catalog 解析；即使二者局部 index 都为零，也不能按 index 混同。
不得复制 P 到 body catalog、重新编号、替换 Arc，或由 AST、cleanup、replace 结果造 producer。

输入 prefix 必须 Complete 且通过旧一致性校验，并实际发出 Result(P)、Binding(held,P)。
body 的开放事件不送入旧 closed V1 validator，不放宽旧 catalog 的 `from_legacy` 工厂。
专门 validator 只为本有界见证核对实际 definitions 与下列精确六事件：

1. RHS 字面量 Result(Q)。
2. 同一 checked call/argument 的 Pending(Q)。
3. 实际 Replace(held, previous=P, incoming=Q)。
4. 同一 replace call 的 Result(P)。
5. previous 声明的 Binding(P)。
6. previous 实际声明完成后的 ASAP Drop(P)。

所有 expression、statement、symbol、drop point 都保留 source-qualified 身份。六事件
证明 P 的单次运输与消费、Q 安装；它们本身不证明 Break 已完成。

### 四次独立实际快照，两条互斥出口

复用现有 planner 执行，不新增模拟遍历或改变其 ValueState。分别读取：

- condition 完成后的 entry：held=P。
- 实际 Break 的 pending loan、内层 owner 和 iteration cleanup 完成后：held=Q。
- body 实际返回 `Ok(false)` 后：held=Q。
- 原 condition state 完成实际 LoopExit cleanup 后：held=P，表示 condition-false 出口。

每次读取穷尽所有 ValueState 字段，仅一个 held 可非空；其余 owner、borrow、pending、
closure 与 iteration 槽必须为空。entry 与 condition-false 完整状态相同；post-Break 与
body-completion 完整状态相同。P→Q 只允许 owner 与实际 primitive commit 更新的 origin
改变，symbol、declaration、scope 保持不变。快照不能相互复制以代替执行边界读取。

完成封印核对真实 Break expression、最近 loop、scope、fresh body sink 身份、一次性
completed 及实际 body 返回。只发表 EnterBody、BreakExit、ConditionFalseExit 三条结构边。
不把 condition-false 状态称为共同 exit，不回灌 body state，不生成 backedge 或合流值。

### 封闭产品与发表

使用独立只读产品，不复用旧 invariant binding 的“token 仅在 input catalog 解析”合同。
最窄生产构造权限留在 compilation-unit 内；直接消费者是同轮 observation 发表检查及
公开 ownership 分析的独立契约测试。外部只读 getter 可查询控制点、两个 catalog、四个
精确边界和实际六事件，不提供构造器、共同 exit、closed body transport、phi 或 suffix getter。

旧 closed/invariant/Choice 与新能力互斥；全部旧 validator、支持范围与 oracle 保留。
legacy observation 仍为 Unsupported 且没有残留 events/definitions。关闭 observer 不改变
任何原 diagnostics/cleanup 产物；新能力须另行验证，不能以旧 observation 相等替代证明。
任意 source 的失败或 deferred 都撤销本片完整产品；private sink 在传播错误前恢复。

## 替代方案

- 放宽旧 closed/invariant validator：拒绝。其全消费与输入不变合同不适用于开放输出。
- 直接令外层 var 通过不变输入准入：不能证明真实 P→Q，不解决本片能力缺口。
- 首片建立通用 open transport、header phi 或 runtime generation：推迟。该函数最多激活
  一次 body；一般回边与合流的状态、实例和消费证明不在本次已批准范围。
- 将两条出口折成一个 token：拒绝。两个互斥路径分别持有 P 或 Q，缺少共同合流证明。

## 后果与验收要求

收益：在不改变语言及旧 cleanup 的前提下，首次封闭一个真实改变外层 owner 的循环边界。
代价：新增独立产品、身份校验和原子撤销接线；不得因此宣称一般循环 owner 已完成。

正式实施须先保存真实源码能力红测，再实现并复绿。普通证据逐项检查两 catalog 身份、
完整六事件、四边界来源、旧 owner 恰一次 Drop、新 owner 未提前 Drop、两个公开分析入口、
跨 source/重分析身份、合法非候选和全局失败/deferred 撤销；disabled 观察与原产物完整比较。
破坏模型、伪造身份和注入负控仅按既有 CI 规则执行。本片不以静态 producer 等于 runtime
实例，不以普通 ownership 检查通过等于 native 释放次数；未运行检查分别登记。

## 关联

- 相关 Spec：[SPEC-0341 §180](../../specs/active/0341-shared-lowering-core.md#180-一次根替换后-break-的协议提案)
- 当前事实：[闭合循环观察](../../architecture/unit-loop-owner-transport.md)、[不变输入](../../architecture/unit-invariant-loop-owner.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
