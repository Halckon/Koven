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

这证明原样合法输入在现有后端的兼容性，不证明后端消费新 B 产品：当前 production
codegen 没有调用 `replace_break_observation_v1()` 或 `suffix_consumer_v1()`；测试查询
只核 Phase 3 产品存在，不替代真实下游接线。没有修改 production lowering、扩大首片
合同、退休 Single oracle 或验证 runtime 释放次数。精确验收仍见 SPEC-0341。
