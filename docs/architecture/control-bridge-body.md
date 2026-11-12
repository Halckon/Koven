# 跨控制桥接后的独立 body 消费

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改顺序control活根桥接后的Replace/返回值清理 · **唯一真源**：frontend实际事件与codegen发射许可 · [入口桥接](unit-loop-counter-audit.md)

## 合同范围

桥接的前control退出来源仍是静态整组替代，不选择某个entry/first-body token作为下一入口。
额外body产品以目标根位置作为形式Header义务：每根一次直接String Replace，返回旧值绑定
后按真实drop fact清理，正常输出来自实际String定义。具体形状识别与旧RootTokenTransaction
共享，但不创建或复活旧入口owner ID，也不恢复旧Complete观察。

实际condition完成和body-entry是必要前置。后续literal、Replace、绑定、Drop及正常尾部
必须全部发生且身份、顺序、资源集合一致；不完整只撤销body产品，已完成入口桥接不因此
升级为权限。body产品绑定next control、目标根顺序及callable，携带独立producer catalog和
现有参数化normal transport规则。静态定义不是跨运行迭代持续存在的owner实例。

## 后端消费

选择有完整normal transport的桥接后，逐step维护Ready→Replaced→Bound→Dropped状态：

- Replace发射前核实际body块、当前根槽、call身份及真实String producer；之前各step必须已Dropped。
- 发射后核同一指令位置、span、根与replacement以及返回old槽，才能接受Replaced。
- 返回绑定必须由该call初始化且对应真实drop target；实际Alias唯一发射后才进入Bound。
- Drop必须使用同一前端fact和实际绑定槽；沿原统一drop发射一次，不增加第二份清理。
- 所有step均Dropped、实际body Replace数精确且入口桥接CFG审计通过后，才记录body完成。

该链与已有counter/header/owner drop许可互斥，不重选其他产品补缺。某step失败可保留此前
局部发射记录，但不得有整组body完成记录或成功返回的lowering结果。代码生成仍使用原有
checked arithmetic与通用控制流；此处不单独证明scalar success/abort、第二control退出
或整个函数的owner完整性。

## 验证与未覆盖

直接/逆序Alias及部分live-root桥接使用真实typed-SSA、LLVM验证和零轮/多轮native。
普通测试核实际三阶段消费数及整组完成；仅CI故障注入检查缺Replace/绑定/Drop许可、错根
第二根失败、错误指令位置及重复Drop提交。本地只编译这些故障测试，不运行。

该实现是受限body的实际消费约束，不是一般跨轮归纳、动态路径trace、嵌套控制新权限，
也不宣称这些源码此前不能通过旧lowering执行。

## 第二控制的 checked tail 接线

可选counter-tail身份仅在上述normal body和真实减一/guard写回均完成后封存；保留精确
control、callable、guard/counter及四个表达式身份，不造旧入口token。后端复用已有独立
checked-expression receipt核减一指令、正常success与terminal abort边、唯一前驱以及
success→header。正常分支必须按counter写回→大于零比较→guard写回排列，只有精确四条
scalar指令且不读写活根；header条件使用实际guard槽。整body及入口审计通过后才记录tail完成。

CI-only负控覆盖abort误接header、错误回边、陈旧比较、错误counter写回、缺receipt及
success多余前驱；本地仅编译。
真实native覆盖两段非正零轮及正数多轮；不声称运行触发了overflow abort。tail本身不提供
第二control初值比较完整证明、false-exit清理许可或一般跨轮owner归纳。

## 第二控制 false-exit 分区与清理

仅完整normal body和已完成tail可附接真实false出口分区：复用drop_loop_exit新增的实际
facts及出口ValueState，每个原目标根恰好属于live或LoopExit(next_control)的named Drop，
不从AST推断清理，不造退出owner token。缺失或重复事件不发布该附加分区。

后端在既有统一Drop发射点消费独立退出permit，核精确fact、根槽、finished块、指令位置
和span，前项必须完成；发射后核唯一实际Drop并标记一次性完成，与其他Drop许可互斥。
false边必须唯一抵达finished，再跳至真实exit；finished只有分区规定的Drop，不能碰live根。
全项消费后才记录分区完成；仅live为空才记录all-dead。没有增加第二次Drop发射。

真实native覆盖两段0/2轮、第二段全死/部分live/全live三种分区。CI-only负控覆盖缺permit、
错误指令及重复提交，仅编译不执行。此分区本身不授予live根
独立suffix许可（附加合同见下节），也不是第二control初值或一般跨轮完整证明。已消费的旧fact不认领
同名变量重新初始化后的新Drop；原fact重复仍拒绝，native覆盖该代际边界。最终再次逐项
核Drop实际根槽，并用CI-only错误退出边/晚期错根负控防止发射后篡改留下完成记录。

## 第二控制 live-root suffix 消费

附接可选suffix只接受同scope、紧邻出口的连续dead-val Name move：实际statement开始、
move、binding、fresh AfterStatement Drop及末态全部通过共享RootSuffix状态机，覆盖每个
live根一次。只保留源root与真实binding DropFact；不按owned drops倒推来源，不制造新token。
不同于旧suffix，此入口不接受中间scalar-literal透明语句；借用println和部分清理不发布该产品。

后端以next exit的源根槽核真实initializer和Alias，再以同一fact、实际binding槽核唯一Drop，
只复用原Alias/Drop发射。每项Ready→Bound→Dropped，之前所有退出Drop和suffix项必须完成；
最终重核exit前缀恰为逐项Alias/Drop，防止错根或缺发射仍得到完成标记。后续新代际不被认领。
该许可与旧counter/body/exit权限互斥；没有产品的旧路径不因本片扩大声明。

普通native覆盖零轮/两轮、部分live、全live、逆序清理及后续同名重新初始化；CI-only测试
覆盖缺binding/drop permit、错source及重复Drop提交，本地仅编译。仍不证明一般suffix、
嵌套新产品、第二control初值完整合同或任意跨轮代际归纳。

## 第二控制 scalar entry 强制消费

可选entry合同只来自实际成功完成的counter声明及随后guard声明；AST身份仅作为预期，
缺事件、乱序、错误initializer不发布entry。该合同绑定已有normal/tail的counter与guard，
不生成owner token。counter初值限定Int Name或直接Integer literal，guard限定counter > 0。

后端在原Alias发射点逐一消费counter/guard绑定permit：核实际当前入口块、symbol、initializer、
来源槽、指令位置和span；Name必须是实际引用绑定；默认Borrow Int参数则核真实ReadLoan，
保存loan/producer位置并最终重核。literal必须是同块同来源的唯一Constant且等于源码解析值；
保存producer位置和值，最终再次核验。
第二个Alias必须使用真实counter > 0结果且counter定义在Compare之前。进入第二while前两项
必须已完成；最终重新核原Alias、无重复定义、比较依赖及header实际counter/guard槽一致。
缺permit不会降级到通用路径后仍声称完成。Alias仍只发射一次，scalar入口不借用旧owner权限。

普通native覆盖参数值及直接0/2字面量初值，结合既有checked success/abort、body和退出消费。
CI-only缺绑定、错source及晚期错误Compare只编译不在本地执行。此处约束真实初值传递与
首次guard决定，不把静态owner来源向量提升为跨轮动态owner实例证明，也未开放计算型初值、
任意控制流入口或嵌套新owner产品。

## 生产 typed-SSA 运输校验

slot-CFG桥接、body和checked tail验证成功后，封存独立RefinedBody计划，保存两control的
实际块、成对根槽及第二control正常success/abort块。此计划不复用借用receipt授权；未选择
该桥接的函数不增加合同。直线CFG合并同步重映射计划中的块，不保留已失效的块身份。

SSA发射时从真实layout初始化每块slot→Entity映射；Alias按实际来源更新映射（不要求
不存在的SSA Alias指令），RootReplace核真实旧owner/loan/replacement与两个输出，逐根
仅一次。每个实际边逐位置核目标参数对应的当前Entity；nullable view沿用原有排除规则。
第二body必须保留checked正常/abort分支，正常success回header而abort为终止，不作回边。
所有发射事件齐备、两body对应根均Replace且最终RootReplace/终结器未变，才结束该生产校验。
另保留原指令前缀并核实际指令ID清单等于前缀加观察列表，拒绝绕过hook的额外Drop；
这不重复验证所有其他指令的内部语义。

普通跨control及稀疏live根native测试现在断言生产receipt完成；嵌套旧lowering无该receipt。
CI-only错根edge实参、遗漏终结器、abort改回边和额外Drop作用于实际SSA，均要求一次命中，
只在CI运行，本地仅编译。
该校验强制连接新owner产品与实际SSA运输，不等于运行时逐轮释放次数证明，也不开放
任意嵌套新权限。完整SSA仍须经过原有verifier；发射完成标记不是绕过verifier的授权。

### 已准入桥接的旧代际实际消费

发射校验进一步核每条Drop的真实owner Entity等于slot映射，并在完成时重核实际operation。
每个所选RootReplace第二输出（退休旧值）必须由同body内恰一条实际Drop消费；Alias搬运
沿用既有映射，第一输出（新根）仍由正常边运输。额外指令清单校验保持不变。
CI-only错代际负控把退休Drop改为新根Entity，必须拒绝且不得记录完成；本地只编译。
这建立已准入静态body的退休值身份约束，不构成动态迭代次数证明，也不扩展嵌套权限。

## 第三控制入口的私有续接采集

恰三段顶层counter的第二个桥接不再尝试从第二段形式Header伪造旧式入口catalog。
采集器从第二段真实完成的normal、checked tail和false-exit分区冻结来源，保留其control、
body catalog身份及根索引。随后仅通过实际Move→Bind搬运位置，逐步核完整资源边界；
缺根、额外owner生产、实际drop或未完成绑定均使该候选失效，不复活旧Complete。

第三段不把第一段producer当作其运行时实例；normal-body投影范围见下文，退出权限仍不公开。
普通三段native基线覆盖零轮和正常回边组合及逆序重绑定；不完整链仍走原通用lowering。
第三段现在复用独立BodyPlan及真实producer→Replace→返回binding→Drop事件机：condition
完成与body进入必须按同一control顺序发生；入口只有完整continuation才能提供根位置，
新的body catalog独立分配，不挪用前驱producer。normal、checked tail和false-exit分区继续
各自以实际事件完成为前提，未支持body形状不产生完整结果。
第三段scalar入口现接实际有序声明完成；完整根转移前也保留初始化事件，不用AST形状
代替实际完成。恰三段的整链先验证全部成员及跨段来源，成功才一次性取走，内部证据
保存在observation而非测试旁路；缺入口、body、tail、退出或终段清理均不产生部分链。
中间live根由续接转移，终段必须全死或有真实完整suffix。内部链与其他能力互斥，绑定
原analysis/callable和首次Syntax拒绝点，deferred时撤销，旧Complete不恢复。
所有旧产品的直接getter也独立排斥内部整链，不能只依赖发布前一致性检查；
派生getter通过父getter继承该互斥。CI-only将真实已有效的旧产品与真实整链混合，
要求一致性失败且旧getter无可读权限，本地只编译。
整链现提供 `control_chain_bodies_v1()` 借用投影，只暴露第二/第三成员的
control、condition、body、原顺序根和真实normal规则/catalog；成员字段私有，不造入口token。
getter先核observation整体身份、一致性和互斥；deferred、外来identity或混合旧能力均不投影。
消费者必须整组选择并完成两个成员的Replace→binding→Drop，不允许单成员降级。
成员另以 `false_exit_partition()` 借用真实implicit-false分区：live根与原顺序LoopExit
清理facts直接来自第二段原bridge及第三段封存exit，不按AST猜测、不复制重造fact。
本接口不提供scalar入口或SSA权限；checked尾部须核真实边，公开不等于整循环完成。
嵌套循环的外层恢复与路径合流仍不在本片范围内。

### 后端按 control 的 normal-body 整组消费

完整三段内部链投影第二、第三段的真实 normal-body、implicit-false 与 checked 尾部组，不公开入口token、
整链SSA权限。后端一次选择整组，逐control登记实际body块与根槽；调用、返回
binding和Drop仍复用原发射点与一次性permit，每张permit增加control身份并在提交时重核
实际块、指令位置、span及原阶段。第三body登记前第二body必须全部退休完毕，所有成员
均完成才记录整组完成；组内匹配失败不退回旧单bridge消费者，也不重复发射Drop。

普通三段native保留零轮/两轮组合和逆序root映射，封闭终段时另外要求两成员各两根的三步
消费及整组完成。CI-only错误control和最后成员缺Drop许可要求拒绝且无整组完成，本地
只编译。该合同是normal指令发射约束；尚未运输每个成员的晚期SSA receipt，不声称入口、
跨段边参或复杂退出的整链授权与任意跨轮归纳已经完成。

implicit-false清理现在复用原单次Drop发射点，permit带control并在commit重新选择对应
finished块；只认原fact与根槽，不能以另一个control或新生命周期的同名root代替。
第三control开始前第二段原分区必须全部消费。最终逐段重核header假边、finished/exit
唯一前驱、原序Drop数量和目标，拒绝活根被消费或额外Drop；所有成员通过后才记整组完成。
普通native新增三根到两根的稀疏链，实际覆盖中段一根退休与末段两根退休，并运行零轮/两轮。
CI-only错control与缺退出许可必须恰一次命中、拒绝且不发布组完成；本地仅编译。
这扩展implicit-false分区，晚期SSA合同仍未整组迁移。

组成员的 `checked_tail()` 提供原完成counter/guard及四个有序赋值点，不制造旧入口catalog。
后端在每个真实while登记时快照entry块和scalar槽，复用单bridge的实际checked校验：唯一
subtraction receipt、减数1、body双分支、terminal abort、success写回counter→比较→guard→header，
并核success不消费根。两成员的body、退出与tail全部通过后才发布组完成。
CI-only删除最后成员receipt要求整组拒绝且无任何完成记录，本地仅编译；普通稀疏/全根native
要求两tail完成并运行零轮/两轮。本片仍不授权scalar初始化、不证明跨轮代际归纳或晚期SSA。

最终成员现在通过 `false_exit_suffix()` 借用原完成的live-root后缀事实；中段保持None，
不能把向下一control的运输当作终段清理。绑定与Drop沿原发射点使用control限定的一次permit，
绑定核真实源root槽，Drop核原fact；提交时重选control/exit并核指令位置/span/阶段。
只有原有body与退出分区已完成才可消费，终检要求每条Alias→Drop均完成后才允许组完成。
普通稀疏/全根native覆盖最终两根的绑定与Drop各两次；CI-only缺binding、缺Drop、错源和
重复Drop必须恰一次命中并撤销整组，本地仅编译。scalar初值与整链晚期SSA仍未迁移。
