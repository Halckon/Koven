# Unit 根替换后 Break 的 owner 边界

> **性质**：实现事实 · **状态**：current · **读取时机**：消费一次根替换后 Break 的独立 owner 出口时 · **唯一真源**：owner transport 的 replace_break 收集器、封闭产品与 validator

合同为 [ADR-0037](../adr/accepted/0037-replace-break-owner-boundaries.md)，验收见
[SPEC-0341 §180](../specs/active/0341-shared-lowering-core.md#180-一次根替换后-break-的协议提案)。
这是与[闭合 body](unit-loop-owner-transport.md)和[不变输入](unit-invariant-loop-owner.md)
互斥的独立能力；[ADR-0038 B](../adr/accepted/0038-path-qualified-owner-transport.md)
增加唯一尾句的两入边消费，不是完整循环运输、通用 header/exit phi 或一般后缀。

## 实际准入

普通非泛型顶层 Unit 函数只有一个无 mode marker 的精确 Boolean 参数，函数体依次为
外层 var 字符串字面量、While、直接 val 移出该根。While 条件是该参数的 Name；body
恰为 val 接收 checked root Replace 的旧值、一个 bare Break。三个绑定精确为非 nullable
String，两个字面量没有插值；名称与文本内容不参与准入身份判断。

Replace 仅支持普通 Name callee、无显式类型实参、两个无命名前缀的实参。第一个是
带 Inout marker 的直接根 Name，第二个是无模式标记的字面量；实际 ownership primitive
必须为 String Replace，只有该完整根，两个 descriptor operand 与真实 AST 实参顺序完全相同。

Group/Not、额外参数、别名输入、额外 body 语句、var 旧值绑定、nullable、字段/index、
opaque call、正常回边、Continue、条件分支、嵌套或后续使用能力均不准入。限制只撤销
本观察产品，不新增语言诊断或改变既有成功源码。函数末尾直接移出用于保留真实 liveness，
其普通 cleanup 保持不变；只有实际路径 carrier 完整经过该尾句，才另发布封闭消费证明。

## 实际 producer 与四个读点

`prepare_owner_loop_input` 在 While 的旧 Unsupported 标记前保存真实 prefix；必须完整
通过旧观察一致性，恰有 Result(P)、Binding(root,P) 与一个实际 String producer。
仅该根非空，原状态的 origin 等于声明 marker、scope 为函数 body 的一层。

实际 condition 返回后独立读取 entry。复用原 While 对 body 的一次遍历及 fresh 私有
observer；condition state 保留 P，body 的真实 RHS String hook 产生另一个 catalog 的 Q。
原 pending、Replace、Result、Binding、ASAP Drop hook 依次发出固定六事件，不改任何 hook
语义。两个 token 即使局部 index 都为0，也只能通过所属 catalog 的完整身份解析。

真实 Break 按原顺序完成 pending loan、内层 owner、iteration cleanup 后，读取 held=Q，
并封存实际 kind/expression 与活动 body identity。最近 loop、scope、准确 tail 与一次性
completed 检查仍由原跳转路径负责；AST 存在 Break 不能独自提供完成证明。

body 实际返回后再次读取 Q，并要求真实 `Ok(false)`、同一 completed/collector identity。
恢复原 observer、Choice 与 branch depth 后才传播错误或考虑发表。原 condition state
完成真实 LoopExit cleanup 后独立读取 P，只代表 condition-false 路径，不是公共合流状态。

每次读点复用穷尽 ValueState 字段的边界检查：仅 held 可存在，其余 lambda/this、owner、
借用/运输、pending、iteration、replacement、closure 槽为空。P→Q 仅改变真实 token 和
primitive commit 写入的 origin，symbol、声明、scope 不变；两份 P 状态完全相同，两份 Q
状态完全相同。没有把快照回灌普通 planner 或通过改写 liveness 获得新观察。

## 独立产品与原子发表

`replace_break_observation_v1()` 返回 `UnitOwnerReplaceBreakObservationV1`，只读查询
control IDs、两个 catalog、entry/post-Break/body-completion/condition-false 四状态、
真实六事件及 EnterBody/BreakExit/ConditionFalseExit 三条结构边。它没有共同 exit、
closed body transport、通用 phi 或 runtime generation。`suffix_consumer_v1()` 只返回下面
严格 B 的完整尾句消费；原双出口 getter 不把缺少 consumer 的边界冒充完整消费。

私有 validator 同时核两 catalog 的完整 identity、来源、callable、稠密定义，实际 prefix、
六事件、边界元数据、完成身份与准确 Break，以及三条边。body raw 允许引用外来 P，但只
通过本片专用完整校验；旧 `from_legacy`、全消费 V1 与不变输入相等规则完全不放宽。

外部不能构造产品或状态。旧 legacy observation 仍在原 While 点 Syntax Unsupported，
events/definitions 清空；新能力与所有旧循环/Choice getter 互斥。任意 source 失败整体
撤销观察，任意 deferred 清除本能力。旧观察相等和 on/off 产物相等仅证明兼容性，
不能代替本能力自己的 catalog、事件和四边界证据。

这些是静态控制边与实际分析转换证明，不等于 runtime 实例、析构次数或 native 能力。
一般动态循环、多个 owner、header phi 和一般后缀仍未实现。

## 唯一尾句的真实状态交接

双出口验证后，原 condition state 的 P 身份与实际完成的 body state 中移出的 Q 共同进入
`ValueState` 的严格 B carrier。Q 不从快照倒造；原 catalog/token 不重编号。单 owner 字段
保留旧 planner 兼容投影，不将该 P 当作 Q，也不向旧 hook 重放或补造 Q 事件。

唯一 Name 的真实 take 把 carrier 移交给本次 `EvaluatedOwner`，真实声明 bind 后存回
`ValueState`，实际 ASAP Drop 完成后消耗 carrier。三个实际转换按序记录 Take/Bind/Drop，
表示一个按边选择的值消费一次，不是同时消费两个值。公开 incoming 固定为 condition-false P、
Break Q；每项保留完整状态身份，匹配原两出口和两个 catalog。

consumer 独立匹配准入时保留的 suffix initializer、statement、retired 身份，核三个动作
恰一次和准确顺序。carrier 不进入普通 branch merge，原边界检查拒绝其泄漏；不能推广
通用 phi、额外尾句、alias、borrow/capture。新 consumer 嵌入原 replace 产品，共享全局
source-error/deferred 撤销。实际源、双入口、跨 source/重分析和旧产物 on/off 检查见 Spec。

## 原样源码的既有下游证据

独立普通测试 `owner_replace_break_downstream_tests` 保留上述原样 B 函数，另用入口
分别调用 `run(false)` 与 `run(true)`，从严格真实 Unit 产品执行既有 lowering。
SSA 结构断言确认原 P 分别进入 true/false 边、唯一 RootReplace 取实际 Q，Break 的新
root 与 false 的原 root 进入同一 exit 参数；body 只 Drop detached 旧值，尾句只 Drop
选中参数，并独立通过 SSA verifier 与 LLVM verifier。调用源码存在不等于 native 已运行。

上述旧 oracle 继续独立保留，不能单独证明新 B 产品的生产消费。

## 严格 B 的 Phase 4 单次消费

`ssa/refine/build/owner_consumer.rs` 在发射前从当前 owned Unit 与 callable 选择封闭 B
产品，按各自 catalog 的完整 opaque identity 将 P/Q 绑定到原 String producer。
While 的 condition-false 与真实 Break 钩子按角色记录 CFG 端点；Take 前核两入边精确覆盖，
不 zip 前驱顺序，不造 phi、owner definition 或 AST ownership 事实。

Name Take 从计划选中槽交付，实际 Alias Bind 发前授权、发后记账；后缀 Drop 必须取得
已完成 Take/Bind 的 receipt，且匹配唯一旧 cleanup index、point、target 与 binding slot。
旧 `emit_drops` 仍是唯一物理发射器，成功发射后才消费 receipt；函数结束同时核新计划
完整性与旧 `check_all_drops_consumed`。原 RootReplace/Inout 与 body detached P 清理不变。
无可选 B 产品的合法输入仍用原路径，已选计划的任何失败不回退。

实际 root/Take/Bind/Drop 顺序与物理 Drop receipt 有普通见证；原样 SSA/LLVM、双 Unit、
同字面量、重分析身份、合法非候选及普通 native false/true 已在固定代码头验收。
普通运行不证明释放计数；故障负控只编译，正式双宿主 CI、计数和 sanitizer 未运行。
精确输入、命令和剩余范围见 SPEC-0341；不退休旧 oracle，不扩通用 phi/capture/Resource。

## 与 loan 动作源的组合边界

owner 与 loan 观察独立开关；同一已检查输入的四种组合移除对应观察后，完整旧 planner
产物及顺序保持相等。严格 B 与 temporary、named、receiver、result 及同步 consumer loan
的兄弟函数共同存在，不表示有界 flat loan region 已支持 B 的循环，也不保证这种形状隔离永久成立。
组合断言核真实 P/Q incoming、AST 限定的 Take/Bind/Drop；独立原 B oracle 保留。

真实独立 source 的错误在 raw/public 均撤销 owner 观察；真实 deferred 保留旧诊断性
观察，但在 raw 返回前已剥离 replace-break、closed-loop 和 invariant-loop 能力。
该处复用原公开发布策略，公开构造仍保留幂等兜底；loan 新观察与既有相关投影全局撤销。
这修复旧 raw 边界缺口，不改变语言准入或后端执行责任。组合验收见 SPEC-0341。

同步 consumer 的组合正例还解析实际 source Begin、transfer、consumer Begin 与 retirement
的同 catalog 公开句柄并核严格顺序；独立 source 错误/deferred 后这些依赖和旧 transport
一并撤销。依赖仍是 Partial 静态记录，不授予一般 parent 权限恢复或动态唯一消费能力。

protocol4 的 flat-state 准入显式拒绝活跃 `replace_consumer`，与现有 loop/replacement/
pending/capture 门并列。当前 B 的 var/while/尾句绑定与 protocol4 的 val literal/调用尾部
语法不相交；这一事实不是省略状态门的理由，也不构成普通源码缺陷红测。合法兄弟函数
组合另核区域完整五动作和正常退休后同点根 Drop；双 End 保存在有序 retirement transaction
中，不虚构额外动作或把整个 source 升级为 Complete。


严格 B 的 Take 另核 condition-false 角色对应 While 构图的真实 `finished` 块，不只核前驱集合。
这是无普通源码反例的防御补齐，不是新形状或能力 red→green。跨 analysis sealed view错配、
缺/错/互换入边以及真实String Drop调用计数/LLVM校准已有CI-only测试准备；本地只编译，
正式执行仍待。计数仅针对literal P/Q的析构次数及零heap分配，不证明Resource或一般泄漏。
