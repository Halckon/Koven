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
激活/header不变量以及错误/deferred撤销。下面的首个私有切片只覆盖最窄catalog/提交关联，不代表该整体方案已完成。


最小独立token事务现仅覆盖同一受限线性body：入口保存真实旧catalog definitions及原token
引用，不重编号；首次实际String完成时建立新的独立catalog，每个新定义只能由匹配的
String完成hook产生。实际Replace提交逐项校验计划顺序、root、producer与定义身份，全部
通过才在私有root表内原子替换并附上(old,new) token关联。无分支上下文、每根一次；重复根、
额外语句、非直接RHS等均不建立事务。重分析的新catalog不可互换，公开产品门槛不变。
该关联只描述首次body遍历的静态定义运输，尚未把旧token绑定到返回binding并由真实drop
消费，也未覆盖新激活/后续迭代；返回binding清理证据不能自行升级为代际退休证明。

独立token事务失效会撤销已记录的token links；body终态非正常或计划/定义未全部完成时也撤销整组links，普通非token回执不冒充成功事务。
