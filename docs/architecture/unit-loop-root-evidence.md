# 多根入口、写入与清理证据

> **性质**：实现事实 · **状态**：current · **读取时机**：核对多根入口、私有写入及token事务证据 · **唯一真源**：drop planner root_incoming/root_writes/root_tokens及真实输入测试

本文记录[循环owner运输](unit-loop-owner-transport.md)中的私有多根证据与公开能力边界，遵循
[ADR-0038](../adr/accepted/0038-path-qualified-owner-transport.md)。

真实While入口在条件求值前另保留私有root→owner向量。采集使用当时ValueState，逐项核对
完整root frame及当前Complete observation的source、callable、catalog身份和definition
成员；任一不符整组不记录，同control失败重采集会清除旧项，函数完成清空。它不是条件后
状态、body/backedge equation或公开多根能力；已经Unsupported且清掉catalog的前缀不能
凭残留owner ID恢复资格，也不改变借用排斥或旧witness。

私有多根阶段证据另核真实condition求值结束与body正常落通：整组root/owner顺序必须
逐项等于已验证入口；body必须实际返回Ok(true)，且条件后保持标记有效，才记录正常
落通候选。Unsupported观察下旧ValueState可能残留旧ID，因此活动控制内真实求值的
Call/Assignment在早退前独立置sticky失效，后续向量相同也不能恢复资格。
这些标记不发布header/phi，不证明复杂控制流逐路径完整性；条件后保持不代表条件值为真，
正常落通候选也不是运行时可达/终止证明。Break/Continue/Return不冒充正常回边。

私有RootWriteReceipt关联活动control、已捕获root、实际checked Replace call与新producer
表达式：必须先经过非插值直接String的真实完成hook，并在同call中有唯一待转交temporary，
提交边界再消费该完成记录。不制造UnitOwnerId，不改ValueState，也不解除sticky失效。
目前不支持group/name/call RHS的producer转运；回执仅为planner遍历事实，不含完整路径身份，
不能把来自互斥分支的回执拼成运行时提交序列或完整运输/清理证明。

实际If-arm和When entry body求值（含String When特化）为producer完成与写入回执记录同一词法祖先栈，
包含source-qualified控制表达式和body索引；混合嵌套逐层保留，body返回或报错后在
传播结果前恢复。回执按真实AST核对If/When类型与arm范围，拒绝跨source、非法索引
或重复控制身份。When同entry的多个selector共享body，记录不区分匹配alternative。
String When在既有literal-candidate内核实际进入body后才记录，同entry多selector不会制造多个body身份。
它不覆盖selector判断历史、短路左侧判断历史、循环迭代或join后的历史；
空栈不是无条件执行证明，也不能据此合成逐路径运输或动态trace。

短路RHS也按实际UnitShortCircuitPlan记录同一祖先栈，&&的RHS边编号为0、||为1，
在RHS结果传播前恢复；回执重新核对计划与编号。普通与constant-control两条入口复用
同一hook，不改变既有分支清理。普通模式保守遍历，即使文本为false && RHS也可能
记录RHS事实；constant-control的Never不进入hook。它是planner求值边，不是运行时
可达证明，不保留左操作数判断历史，也不证明每条路径的root运输完整性。

祖先作用域退出必须匹配进入时的source-qualified控制和arm；重复进入、跨source或
失配退出会sticky拒绝并清除当前私有root证据，后续配对退出不能恢复资格。被拒绝期间
不再采集入口、producer或写入回执；仅新的事务生命周期重置。此保护只撤销私有证据，
不改变旧求值、清理、对外产品或语义接受范围。

变更根正常落通的第一片私有证据仅限：Boolean Name条件、至少两根、body恰为每根一次
直接String Replace的val声明，且实际body返回Ok(true)。逐语句对齐完成回执并核对每根
仍在body状态中，按入口根顺序保留root→producer表达式向量。重复/遗漏根、分支、额外
语句及Break/Continue/Return均不产生该向量。此向量不使用残留旧owner ID充当新owner，
不恢复unchanged-root证明，不是header/phi、跨迭代等式或运行时回边到达证明；尚未证明
代际旧值退休与逐路径运输闭合，不能用于替代旧witness；受限结果清理另见下文。

该受限normal向量另可关联Replace返回旧值的局部binding及真实drop fact：binding必须
已离开body状态，所有已产生facts中恰有一个Named(binding)，且位于该val声明的
AfterStatement。缺失或重复清理事实拒绝整组清理证据。这证明现有planner对返回值
binding的清理，不是旧root代际owner的退休证明，也不证明跨迭代运输或替代旧witness。

清理证据保持声明遍历顺序，producer向量保持入口根顺序；二者必须按root关联，不能按下标配对。

### 代际退休的公开边界与剩余缺口（真实多根 normal fixture）

实际两根均Replace且返回值均清理的输入，私有producer/cleanup向量都可完整记录，但
公开observation仍Unsupported、definitions/events为空，各value/loop能力getter均无产物。
主链的限制不是缺少drop：`define_owner`只在Complete观察中创建身份；`commit_owner_primitive`
在记录私有回执后检查active，Unsupported时不会更新root owner，也不会交付previous token。
因此不能仅凭entry owner或ValueState残留ID证明某轮Replace取出的旧代际，更不能从Named drop
倒造原catalog定义。真实回归明确锁定“私有两组证据齐全仍不发布”的边界。

按ADR-0038继续的必要接线是：在独立事务内保留原catalog引用及真实producer完成身份，
于实际Replace提交点原子关联旧root代际→返回binding与新producer→root，再让真实drop消费
该binding携带的旧代际。缺任一关联必须整组拒绝；完成这一步后仍需另证零轮入口、回边
激活/header不变量以及错误/deferred撤销。下面的私有切片只覆盖最窄catalog、提交及binding/drop关联，不代表该整体方案已完成。


最小独立token事务现仅覆盖同一受限线性body：入口保存真实旧catalog definitions及原token
引用，不重编号；首次实际String完成时建立新的独立catalog，每个新定义只能由匹配的
String完成hook产生。实际Replace提交逐项校验计划顺序、root、producer与定义身份，全部
通过才在私有root表内原子替换并附上(old,new) token关联。无分支上下文、每根一次；重复根、
额外语句、非直接RHS等均不建立事务。重分析的新catalog不可互换，公开产品门槛不变。
该关联只描述首次body遍历的静态定义运输；下面的实际binding/drop接线仍不覆盖新激活或后续迭代，不能升级为一般运行时代际退休证明。

独立token事务失效会撤销已记录的token links；body终态非正常或计划/定义未全部完成时也撤销整组links，普通非token回执不冒充成功事务。


受限旧token现在在实际bound_owner事件按计划initializer及binding消费pending交付，
再仅于真实Named drop fact首次写入时匹配原声明AfterStatement并消费该binding的旧token。
下一producer不得越过尚未交付/清理的前一结果；完整事务要求全部提交、无pending/bound、
消费数等于根数。回执记录binding及实际fact，并核旧token与对应Replace link相等；任一缺口
撤销token links和token消费记录。既有非token事实与所有公开资格仍不变。
CI-only负控覆盖首笔成功后第二笔完成/提交失效，以及跳过首笔binding/drop事件；本地仅编译。

完整受限事务另形成私有静态边界等式：保留真实入口root/token向量与正常body末root/token
向量，核对相同root顺序、旧定义成员身份及每个新定义恰出现在一个输出root；全部旧结果
已由binding/drop链消费且无悬挂交付。仅normal vector和cleanup均完整时保留，失效时清除。
它证明单次静态遍历的catalog守恒，是后续header义务的输入材料，不把entry token等同于
normal token，不表示下一轮复用这些token，也不是运行时固定点或多轮执行证明。
零轮/回边合并、每次激活的新实例身份及下一轮不变量仍须独立构建与核验。
边界等式只读取事务开始时保存且不可变的root→入口token映射，不接受调用者重组的入口向量；catalog成员相同不能代替逐根身份对应。

静态header候选现将入口与首次body末向量封装为两种整组前驱：`ZeroBody`保留原catalog
引用，`FirstBody`保留独立producer catalog；按control身份选择，不允许逐根挑选或外部
改写向量。逐根旧新catalog必须不同，同一候选clone保持原定义身份。真实两控制输入验证
不能把第一控制的候选用于第二控制。这只是零轮与首次body边界的备选定义域，选择API
不代表该边实际可达，也不支持把FirstBody解释为任意后续轮次；未生成激活序号、动态实例
或公开phi产品。跨轮归纳与路径完整性仍未实现。

每个header候选还保留按入口root排列的静态转移槽：input root、实际完成的String producer、
消费该root原token的真实drop fact。事务保存drop hook实际消费的(old,fact)，建槽时按root
查声明索引，再核原token、返回binding、AfterStatement与对应新定义；不得用声明下标直接
配入口根。槽仅通过同control的整组只读查询取得，clone不产生新身份。
该有限签名为下一步归纳提供“输入槽退休/producer输出”材料，不实例化第二次激活、不重放
drop，也不把静态producer定义当各运行轮次共享的owner。公开能力及旧见证保持不变。

header候选还须通过真实body末的状态形状门禁：复用既有boundary_values，要求借用、
临时值、替换、capture及迭代等辅助状态无悬挂，活root集合与入口根集合精确一致且无重复。
只核symbol集合，不使用Unsupported后可能残留的owner ID；代际关联仍来自独立token事务。
形状失败仅拒绝header候选，不改旧planner行为或把普通回执升级为能力。此为后续激活
归纳所需前置之一，仍未证明运行时header合并及任意轮次不变量。

候选现在携带真实While condition表达式、body语句及Name解析到的Boolean binding，
它们必须与control同source，guard binding不得与任何String root重合。身份在事务开始
直接由AST/reference/type读取并只读保留，非由文本名称或Span相等推断。真实参数guard与
局部val/var guard均覆盖，验证保留的是实际引用而非默认参数。该接线只标明以后重复header
求值的真实位置，不预测条件结果、不声明guard在任意代码下不变，也不生成运行时激活。

独立token事务另要求真实planner事件回执：condition表达式成功求值后保存其身份，仅在
普通条件分析允许进入body的实际分支入口记录(control,condition,body)。开始token事务时
须与实际While的condition/body精确匹配，并通过已核条件保持、无污染与资源形状检查；
静态AST结构本身不能替代进入事件。CI-only遗漏入口hook负控保留普通写回执，但无token
links及header候选。此事件指首次planner遍历，不代表运行时条件为真或发生了第几次激活。

整组incoming现可反向匹配已有静态前驱：必须同control，root顺序及每个原catalog token
全部相等；仅shape相同不足。真实独立两次分析的normal快照不可互用，CI-only另覆盖逐根
拼接entry/new、交换及缺根。该匹配只识别已经保存的两个静态样本，不接受或推断新激活的
实例，即使新激活具有相同root/producer形状也不能冒用静态token作为跨轮证明。

测试层新增有限正常前缀激活模型，由真实sealed entry/normal及消费槽提供输入，模型实例
单独携带私有模型作用域和出生轮次；不新造UnitOwnerId，也不能进入公开产品。正序/逆序
写根分别检查0/1/2/8次正常前缀：读完旧组后再整体替换、每个旧实例仅退休一次、当前实例
未退休、新一轮实例与同producer旧轮次不同。该模型不执行Koven，不代表条件真值、实际
运行次数或退出，只验证模型的有限自洽性；跨轮生产实现与归纳仍待完成。
