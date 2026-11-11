# 多根 false 出口后的受限消费前缀

> **性质**：实现事实 · **状态**：current · **读取时机**：核对多根活根后缀消费范围 · **唯一真源**：drop planner root_suffix及真实源码测试

本文记录活根离开循环后的实际消费基础，承接[多根证据](unit-loop-root-evidence.md)。
这不是一般后缀合流、函数完成、运行时 trace 或 LLVM phi 的证明。

## 当前范围

仅当受限正常循环已有真实 false 出口分区且至少一根 live 时启动私有 tracker。
下一段同 scope depth 的连续语句必须全为 `val target = root`，RHS 是解析到剩余根的
直接 Name，target 由真实声明 liveness 判定声明后不再使用。控制节点、嵌套 block、
分支、额外声明、延迟清理及目的 binding 继续存活均保守拒绝。

每次声明须依次经过实际 moved_owner、bound_owner、去重后新增 drop hook 和声明完成。
移动核 source root/expression，绑定核 target，drop 核 target/AfterStatement；声明后资源
状态必须只含其余根且无附属资源。缺任一事件、顺序错误或剩余状态不匹配使整组失败。

tracker 不使用遗留 ValueState owner ID 推导退出时的新代际。完成事实只记录源 root 与
真实 alias drop；公开 `condition_false_suffix_retirements` 对同质完整 Entry/Backedge
来源作替换。每个初始 live root 恰消费一次，出口已退休根不可再次消费。

## 发布边界

必须全部 live roots 完成才发布，部分前缀不授予资格。封存 manifest 保留原顺序及关联，
公开一致性检查与原出口 live 分区核对。旧 all-dead、旧 owner 产品门禁和 owner 状态不变。
已完成后可忽略后续语句，只表示这个有限消费前缀成立，不表示后续函数路径或一般合流安全。

## 验证

真实源码覆盖部分/全部 live、逆序消费，以及额外语句、延迟 alias 清理、分支的拒绝。
observer 开关等价保持。CI-only 缺 move/bind/drop/完成事件负控仅编译，执行留给远端 CI。

## 已复用的实际 SSA / LLVM 路径

现有通用 lowering 已能把上述受限多根源码生成真实 SSA header 参数和 LLVM phi；不再
另写一套重复的 phi 算法，也不放宽入口或退休旧 witness。真实二根/三根、正逆 Replace
顺序、全死/部分live/全live 的矩阵，将前端 call/root/input 定义逐项对照 SSA：入口边
接原 String、正常回边接对应 RootReplace 新结果、body只消费对应 detached旧值，false
边传当前header值并在出口/有限后缀恰drop一次。LLVM另核实际双前驱String phi数量。

三根逆序的native运行只调用 `run(false)`，验证零轮出口与后缀可执行。当前纯Name且
body不改guard的形状不能在true轮后自然终止，因此不声称该native测试执行过回边。
这些测试复用原lowering和旧权限；新的边界产品尚未成为独立替代旧witness的下游授权。

## 真实回边 native 与新产品的范围隔离

现有通用 lowering 另验证二/三根、0/1/3轮、basic/constant 两种产品的12次native运行。
计数器每轮递减；每根先把当前String拼接一个x，逆序Replace后打印返回旧值，再打印
最终根。期望输出包含entry、entryx、entryxx及最终entryxxx，区分真实内容代际和根关联，
实际执行过正常回边，不再仅观察静态phi。输出成功仍不代替释放计数或无泄漏证明。
此组会借用打印旧结果，且条件比较/计数器写入超出新受限多根产品，所以同时断言新
`multi_root_boundary_v1` 不发布。成功不能充当新产品扩大接纳或退休旧witness的依据。
受限产品到SSA的精确关联矩阵则单独扩展为basic/constant两种来源，共24个组合。

## 新产品关联实际 lowering 的生产审计回执

`multi_root_receipt` 现于实际 While hook 记录 CFG 几何和 root slots，在 lowering 完成时
核新产品的实际guard loan-read/value槽、call/root/producer联合身份、正常回边、原始输入、旧值alias/drop以及false和
后缀消费；drop必须对应已消费的原frontend fact。审计通过才记录完成，CI-only覆盖缺
几何/guard、错root绑定及缺drop回执，普通矩阵检查真实完成事件。

这是附加核对，不替代既有lowering权限或witness。选择同entry block的String经单次消费alias链
到root且出口消费已封闭的产品；未获跨块证明或附加读取的入口仍可走原路径。
根定义检查限于被证明的消费前缀，允许完成后合法重初始化；最终SSA verifier继续负责
全函数每代所有权消费。新回执不证明所有动态轮次或释放计数，不放宽产品接纳范围。

## 旧依赖替代前的逐项合同审计

| 依赖 | 当前职责 | 本切片是否替代 |
|---|---|---|
| 新boundary及normal产品 | 原始catalog、控制/root/call身份与整组转移 | 已用于附加审计，不是独立入口许可 |
| Entry/Backedge来源替换 | 零轮与回边两种header来源的同一消费合同 | 两种来源均在生产核对，不能只检查Backedge |
| 原frontend drop facts及consumed集合 | 清理位置与实际消费凭据 | 保留；不能把必要drop事实统称旧witness后删除 |
| 通用slot CFG→SSA构建 | 实际参数和边操作数 | 复用，不重复制造phi |
| SSA verifier及LLVM | 每代消费、类型、真实后端产物 | 保留，附加回执不能取代 |
| 旧lowering入口权限 | 当前接纳和失败边界 | 未退休；尚无新回执独立授权切换 |

生产审计现要求实际entry为直达header的Goto，同时将完整Entry向量和完整Backedge向量
分别交给前端合同，核normal退休、false出口live/retired分区和完整suffix消费一致。
比较通过各自catalog来源解析到同一boundary slot再替换，绝不跨catalog裸比较definition编号；
保留实际消费顺序。这补齐零轮来源的消费合同必要条件，不构成运行时跨轮归纳或旧权限退休。

新多根plan选择后、任何body构建前，显式拒绝与replace-break、open-return或header-return
旧单根plan共存。真实矩阵确认这些旧plan不参与本产品；CI-only冲突负控仅远端执行。
因此本次没有可据此删除的同合同旧witness：不能将原本未激活的旧plan宣称为已退休，
也不能跳过通用入口或drop/SSA验证。该互斥只封闭混用不同合同的权限路径；实际CFG不符合
可选审计范围时，仍保留原有合法lowering行为。

## 实际 alias 链入口消费

新多根产品现在能对接同一entry block内真实的String→多级val移动→root初始化，
不再只覆盖直接字面量初始化。逐槽追踪唯一实际定义，要求每一级alias严格晚于来源，
每个中间槽只有这一次消费，终点必须为catalog指定String。While选择和完成阶段都核此链。
alias链有额外读取或不同路径定义时不扩大资格，保留通用lowering。

真实源码覆盖1/2/4级移动与全死/部分live/全live退出，共9组SSA、LLVM及零轮native验证；
额外读取负例仍可原路径native执行。它扩展的是新产品对应的实际消费形状，未新增运行时
轮次证明，也不改变前端产品或退休旧权限。

## 受限跨块入口消费

真实产品现可消费初始化后单层纯标量If/else前缀：从While入口逆查所有CFG前驱路径，
每条必须回到同一catalog初始化块；中间块、terminator和初始化后的尾部不得触碰root，
绕过来源、重新定义或循环均不授予资格。原String必须匹配
真实catalog。选择与完成阶段都检查，不以单条路径代替全前驱覆盖。

实测单层标量diamond两个选择值，要求前端产品存在、两个真实跨块root回执、完成审计，
再经SSA verifier、LLVM和零轮native。包含读取root的前缀由前端拒绝新产品，旧lowering
仍可执行。嵌套If与带打印前缀的探索未获得相同产品，不能据通用lowering宣称支持；
本切片不扩大前端门禁，也不覆盖一般嵌套控制或运行时跨轮证明。

## 跨块alias移动的同一定义消费

在实际消费点逆向寻找slot定义，遇消费前的读取/使用即拒绝；跨前驱时所有路径必须
到达同一(block,index)定义，不把各分支不同定义拼成phi。继续沿实际Alias逐步回溯，
直到catalog指定String。分别保留root初始化块和producer块，原有真实literal核对与
root定义检查不再错误假定二者同块。每次slot查询独立缓存已完成结果，避免连续diamond
造成指数重复遍历；active栈仍拒绝环。

真实seed在分支前、moved/root在分支后的源码要求产品存在和跨块完成回执；
1/12个连续标量diamond、两个选择值均验证SSA/LLVM/native零轮。此为既有前端产品
的实际代码消费扩展，不构造新语言许可，也不证明循环body的跨轮归纳。
