# 受限正常回边的当前 owner

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改单根正常回边与 Return · **唯一真源**：FE normal_backedge 与 codegen normal_header_return

## 有限合同

单 String root 的真实 prefix P 和 body producer Q 保持两个独立 catalog。header 的
H 是运输来源方程，不是新 producer；每次 body 形成新激活 Q，Replace 当前 H，
绑定并 Drop detached H，然后只携 Q 回到 header。condition-false 按当前 H Return。
实际原遍历的 P 只作参数化 body 规则证据，不把它冒充所有轮次的固定值。

控制仅接两种严格形状：Boolean 局部量从唯一参数初始化，body 末赋 false；或 Int
局部量从 0 初始化，raw `index < limit`（唯一不可变 Int 参数），body 末加 1。
入边/回边初始化和单步守恒独立于有限测试次数。Int 正常 taken 边的加一可表示，
但后端仍保留原 checked-add 守卫、成功 continuation 和独立 Abort，不优化掉检查。
未准入的 Int 不进入旧 Boolean/closed/Break 协议；不扩 Continue、多个根或 capture。

## 真实消费

FE 只在同一事务的准确表达式身份、assignment/body完成及最终Return/cleanup完整时
封印；任何额外控制或资源动作不受新门禁覆盖。codegen消费同轮产品，记录真实
entry/header/body/false/exit 与 Replace/Drop，不从 AST 重建 owner。

Int increment 在原 checked-arithmetic 生产处绑定精确指令索引、结果/失败slot和
成功/Abort block。Return 再核实际Add、操作数、唯一定义、单入success/Abort及正常
success回边；Abort不参与正常join、没有新unwind。success不得额外读写H/P/Q或
old/detached。所有H相关定义仍经完整census。最终Return的实际payload收到receipt。

有限 0/1/2/7 普通运行不证明任意轮次、无泄漏或资源计数；非法构造/fault/Abort执行
保持CI-only。对应代码与精确验收在[SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## 显式 Continue 的独立封印

同两种既有控制可在body末使用唯一裸Continue。私有HeaderCore复用H守恒，但公开
NormalBackedge只接受Fallthrough，ContinueBackedge只接受真实Continue(jump)，
两者不提供Deref或互转。旧Normal判断仍不把Continue当作正常回边。

真实jump结束loan/owner/iteration清理后，记录同body catalog的独立快照；只有
原遍历返回Ok(false)、expected Continue与witness身份一致、body完成再次读到同一
状态，才构造Continue产品。目标固定当前source-qualified While，无嵌套/其他跳转。

后端在原loop_jump清理之后保存实际source/header target和语句数量，原terminate
成功之后封印一次Continue receipt。Return再核jump身份、block、Goto、source语句数
和detached Drop已消费，保留所有H定义census；缺失/重复receipt或角色错配不回退。
Int的checked-add success与Abort仍独立，Boolean直接body回边单独覆盖。

## 单根混合 Continue / Break

固定Int控制可在Replace与加一之后，以唯一 `if(index < 2)` 分出Continue与Break。
MixedBackedge独立封印split/condition/arm/jump身份；共享H核心不改变旧Normal或
Continue角色。共前缀真实Drop退休detached old，live Q进入两arm，分别保留实际
entry、jump cleanup后及arm completion快照。两臂均终止时body状态不回填，只有
真实Conditional witness与Ok(false)完成才发布；旧闭合value validator不放宽。

后端在真实If生产处封印split来源和两臂block，在实际arm入口及清理后的jump
前后封印独立角色receipts。Mixed终态另核Continue只到header、Break只到exit，
false边当前H与Break边当轮Q共同进入Return；实际SSA参数运输保留来源关联。
正常/Continue旧CFG校验路径保留，checked Add Abort不进入正常join。此能力不
代表任意If条件、多根或嵌套循环；验收和未覆盖范围仍以SPEC-0341本片账本为准。

## 独立运行时混合阈值

Mixed的split阈值还可来自当前callable另一个不可变Int参数：两个参数分别供应
while的limit及split的stop，raw Name引用与symbol、声明参数集合、callable和
source identity均由私有阈值证据保留。参数数目同时封印，不能删掉参数证据后
冒充旧literal2路径；阈值算式、别名、同一参数供两边及额外参数不在此片内。

只有双Int且最终Mixed准入成功才走此扩展，旧Normal/Continue/Boolean及带owner
参数族不扩大。CG生产不新增plan，直接复用已有Mixed实际split/arm/jump/Return
receipts；定向SSA测试追溯两个参数各自经block参数运输到真实比较与分支条件。

## 条件 Break 与隐式正常回边

双 Int 参数合同还覆盖尾部 `if(index < stop) { break }` 的无 else 形状。
独立 ConditionalBreak 产品分别封印真实 Break 清理、隐式 false 分支清理、If 的
正常继续及 body 正常完成，不构造不存在的 else body 或 Continue 身份。共享
表达式门只为已准入产品的精确控制身份开放，旧 Mixed 两臂终止门禁不扩大。

后端在真实 split、Break、if join 与 while 清理后的正常回边生产点取独立收据；
完整 incoming 分别证明 entry/正常回边到 header、condition-false/Break 到 exit。
发射阶段可合法折叠单前驱 if join；最终 SSA 的隐式 false 直接到 header，而
refine 的细粒度完成收据仍独立保留。当前只覆盖此单根有限形状，不代表一般 If。
