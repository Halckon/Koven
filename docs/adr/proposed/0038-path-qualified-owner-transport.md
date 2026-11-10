# ADR-0038：按控制边封闭动态 owner 运输与合流

> **性质**：架构决策提案 · **状态**：proposed · **读取时机**：设计 D10/E2 的变更 owner、循环入边及共同后继事实时 · **唯一真源**：本提案；语言规则仍以 Guide 为准

## 状态

proposed

## 接受依据

不适用。2026-11-10 用户要求先起草 D10/E2 剩余工作所需 ADR；起草不是接受或实施授权。
下列方案与首片选择均待确认，不修改任何 accepted ADR、现行 Guide 或已有观察产品。

## 背景

基线 main `3fe1920a6735b71f5a330ea8a8549371615fce88` 已包含 #390。现有
[Choice](../../architecture/unit-choice-transport.md)、[闭合 While](../../architecture/unit-loop-owner-transport.md)、
[不变输入](../../architecture/unit-invariant-loop-owner.md)分别证明不同范围，不能将 getter 的并集当成完整运输。
不变输入协议依赖所有边界持有同一实际 owner，旧闭合 V1 则要求 body 定义全消费。

本地 ADR-0037 已于提交 `03851b2a6fe8695059e2cfa88880759ed019c6ff` 接受，实施末头
`ef5ded79e4350873be019bc4c178106025af3dec` 尚未纳入本基线。它仅证明一次根 Replace 后
立即 Break 的独立 P/Q catalogs、四次真实快照及两条出口；不提供共同 exit、phi 或后缀。
本 ADR 复用而不复制该成果，若以后采纳首片，先纳入其已审实现并重新验证组合头。

真正缺失的是跨不同来源及不同控制边的统一义务：哪个 incoming 持有哪个值，哪个路径已
终止，合流后的消费如何只作用于被选择的实际 owner。继续按每个组合放宽旧 validator
会制造新的平行合同。静态 producer 也不能代表同一构造点跨循环形成的多个运行实例。

## 决策提议

### 1. Phase 3 发布路径化运输合同，Phase 4 继续使用既定 refine/SSA

推荐由 Unit ownership 保留真实转换点产生的 owner 输入、输出、移动、替换和清理事实，
在独立封闭产品中记录控制边及合流；不产出 LLVM 类型，也不在 frontend 重建 refine IR。
[ADR-0032](../accepted/0032-ownership-refinement-layer.md) 的 refine 与
[ADR-0006](../accepted/0006-typed-ssa-block-parameters.md) 的 block parameters 不变。

每个边端点绑定同轮 analysis、source、callable、真实 control/body/jump identity 及其边角色。
Entry、正常回边、Continue、Break、ConditionFalse、Return 与 Abort 不可互换；非继续边不
参加正常 join。真实 source-qualified 身份不能退化为 Span、symbol 名字或裸整数相等。

### 2. 保留定义身份，Choice/phi 是运输结果而不是新 producer

引用 owner 必须携带原 catalog 的不透明身份与 token；跨 catalog 建立显式、私有验证的
引用，不能把 P/Q 重新编号到假 catalog、复制 Arc 或用 cleanup 倒造定义。运行激活中的
实例与静态 producer 分层，按边运输当前值；不要求为每个可能轮次静态展开图。

一次 join 有精确且完整的可达 incoming 表，每项携带前驱 edge 和该路径的实际 owner
位置。互斥选择保留相关性，不能把多个根的选项笛卡尔相乘，也不能将不同 owner 求交后丢弃。
同一 owner 的纯转交不新建析构义务；不同 owner 的 Choice 只选择现有义务，不 clone/retain。
只有全部可达继续路径均给出已初始化且类型一致的值，才能向后继发布单一运输结果。
Moved/Uninitialized/Absent 与活 owner 不得靠补 drop、隐式 copy 或虚构 incoming 调和。

### 3. 区分边界证据、开放片段与完整消费证明

开放片段显式声明 live inputs/live outputs；validator 同时核输入归属、实际创建、每条路径
线性运输、终止清理以及输出仍存活。它不冒充旧 V1 的“全部定义最终 Dropped”。
边界产品只证明端点，完整产品另证明从真实 prefix 经控制边到实际 consumer 的闭合关系。
没有 consumer 的两出口快照不得转成完整共同后继能力。

Return payload 先交付再清理，Break 到对应 loop exit，Continue 到对应 header；现有
[Guide 10](../../guide/10-ownership-borrowing-drop.md) 的 ASAP、资源词法逆序与 Abort 无 unwind
不变。borrow/capture 保护存在时，须联合相应已验证生命周期，不能单凭 owner 图授权 drop。

### 4. 两出口 Choice 的首片选项，尚未选择

0037 已有两出口，接下来是否增加共同后继能力，以及是否限定唯一尾句，须独立确认：

| 选项 | 范围与实际证明 | 取舍 |
|---|---|---|
| A：仅保持独立出口 | P/Q 分别可查询，不提供 Choice 或后缀 | 不增加协议，也不能关闭共同消费缺口 |
| B：唯一实际尾句消费 | 原有严格结构不变；`val retired = held` 真正 take、bind、最终 Drop，与两个互斥 incoming 关联 | 推荐首个验证切片；范围小且能证明共同消费，但不是语言限制或通用后缀 |
| C：通用后缀立即接入 | 任意已准入 move/replace/call/return 后继均消费共同结果 | 可复用性高，但须一次验证所有新 consumer、loan 和失败事务 |

推荐 B 只作为可逆的观察准入，不把唯一尾句写进 Guide，不把尾句存在误当完成证据。
其形式仍为 0037 的固定 Boolean 参数、String root、一次 Replace 后裸 Break；不顺手放开
Group/Not、更多参数或 alias。条件 false 路径实际持 P，Break 路径实际持 Q；共同 consumer
必须由有 incoming 关联的真实取值事实证明。若原 planner 只读旧 condition state 的 P，
不得据该单次读取宣称已消费 Q；须先设计真实路径状态/结果交接，不能回填或重放造事实。
仅展示两条出口时无须增加一个“伪消费”来满足 validator。B/C 未选定前不实施。

### 5. 循环推广采用有限 CFG 合同，不以有界轨迹代替归纳证明

推荐在进入一般循环前，统一 header 的零轮入口、各正常/Continue 回边和 exit 的全部
Break/condition-false incoming。每次激活读取旧来源后再并行提交目标，缺席目标显式清空；
replace 必须先完整形成 RHS，再退休/交付旧值及提交新值，RHS 提前退出不提交。

每条回边须重新满足 header 的初始化、owner 和 loan 不变量；静态定义在新激活中产生新
实例，旧实例由真实 capture/运输关系保存或消费。有限回放只作测试，不证明所有轮次。
嵌套/顺序循环、多个根、nullable、Resource、不同条件形状和 For/Loop 分批纳入 Spec；
未获覆盖证明的形状保持现有能力边界，不得借本 ADR 宣布已完成或新增语言拒绝。

### 6. 事务、下游与退休

所有数据在同轮私有 builder 中产生，只有来源、控制覆盖、运输守恒和生命周期联合验证后
才封印；任一错误、deferred、未完成事务须撤销公开及 raw 候选数据。迁移期旧观察契约保持。
具体 API 命名、是否保留旧只读投影及拆分批次由 Spec 决定，不新建永久并行 producer。

refine 消费已验证 incoming/动作一次，SSA verifier 独立检查每条边的值/loan 使用；不从 AST
重推 owner，不执行新旧两套 drop。未完成下游接线前产品仍是观察能力，不能退役对应
Single witness 或把普通运行成功当成动态无泄漏证明。

## 替代方案

- 持续新增各组合专用 getter/validator：局部实施便宜，但难证明完备覆盖和统一消费；不作为终态。
- 把所有边界折成同一 token：丢失 P/Q 与跨轮实例，不可接受。
- 在 codegen 重新做 liveness/owner 合流：违反 frontend 事实权威及 ADR-0032；不采用。
- 先实现通用 runtime owner registry/引用计数：本合同可符号化运输，暂无引入新运行时机制的依据。

## 后果、依赖与验收

收益是共同描述 changing owner 和多出口消费，避免继续按组合扩张；代价是需要开放片段
验证、路径相关性和循环归纳检查，不能仅扩充一个枚举。

- 首片 B 依赖 0037 正式纳入并验证；其已有代码不得重新开发。§170 的不变 join suffix
  是独立脏树，保全等待自身闭环，不以本 ADR 覆盖或宣称已完成。
- capture 实例依赖 [0039](0039-dynamic-capture-instance-provenance.md)；loan 保护和消费顺序
  与 [0040](0040-complete-loan-lifecycle-evidence.md)接合。无 capture/loan 的首片可先独立批准。
- 验收须保留原源及旧 oracle：双 Unit 入口、跨 source/重分析、P/Q 同 index 不同 catalog、
  零轮/一轮出口、真实 consumer 恰一次，observer on/off 完整旧产物一致、错误/deferred 原子撤销。
- 推广验收增加两轮及 0/1/2/7 轮实例回放、各回边/出口组合、RHS 提前 Return、多个根
  相关性、嵌套控制；固定参数不变的循环不得伪装成每轮可随机选择的 TT/TF/FT/FF 源码。
- 下游另列 SSA/LLVM/native 与计数资源 oracle；故障/破损/计数校准按仓库规则仅在 CI。
  未运行项逐一列明，最终以精确交付头双宿主 CI 及独立复审验收。

## 关联

- 相关 Spec：[SPEC-0341](../../specs/active/0341-shared-lowering-core.md)，接受后另定实施切片，不自动改为完成。
- 复用的本地 ADR：0037，未在本基线收录，完整身份见背景；不在本提案复制或重新接受。
- 取代的 ADR：无；拟新增 Phase 3 运输合同，不改写 0006/0032 的 SSA/refine 决定。
- 被以下 ADR 取代：无。
