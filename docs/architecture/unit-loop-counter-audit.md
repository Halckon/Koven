# 多根 counter 尾部的非授权实际审计

> **性质**：实现事实 · **状态**：current · **读取时机**：核对受限计数循环的发射与数据依赖 · **唯一真源**：counter_tail evidence及counter_tail_receipt实现

承接[多根消费与私有计数合同](unit-loop-root-suffix.md)。本文不是新的owner许可，
也不把既有lowering执行成功称为新multi-root boundary支持任意轮数。

## 前端输入

只为已有精确私有前缀和完整root退休方程提供额外审计证据：本地Int counter、Boolean
Name guard初始化为同counter大于0，body中每根直接String Replace并立即退休旧值，
末尾为counter减1写回、同counter大于0写回guard。记录真实source-qualified初始化、
赋值和两个RHS身份、原producer catalog及实际root Replace/drop事实。

该记录还封存原入口catalog、每根entry→first-body输出token全向量、normal steps及实际
false-exit活根/退休分区和完成的suffix事实。内部复用既有私有manifest校验，不把boundary
对象暴露给counter消费者；缺少完整suffix时仍保留None，不能据分区虚构消费完成。

该记录不暴露header/exit能力；旧multi-root boundary仍拒绝counter。getter要求原analysis
身份、同control的Unsupported Syntax及空legacy事件，不允许与其他产品混用。
失败或deferred分析撤销证据。不能借该getter把Unsupported改为Complete。

## 受限发射消费合同

已选counter先为每根真实入口Alias取得一次性permit：从原catalog追溯literal或own参数
及其同块alias链，核实际发射后保存root槽；while开始前要求全入口向量与实际bindings一致。
入口permit不替代参数模式/typed检查，不把借用参数当作拥有者，不新增Alias指令。

每个精确RootReplace call在实际发射前必须获得私有一次性permit：
核当前body块、root槽及原catalog对应的真实producer槽；发射后核同一指令位置、call span、
root/replacement和返回旧值，再提交该项。所有项最终必须恰好提交一次。范围外维持通用
lowering；这不是借用旧单根plan，也不从“没有receipt”推断所选范围允许跳过消费。

这是局部发射消费合同，不是新的公开跨轮owner产品。typed/inout及通用drop权限仍保留，
返回旧值binding及其实际drop已加入同一强制链：Ready→Replaced(old)→Bound(binding)→Dropped。
绑定匹配精确call与返回binding，drop匹配原始完整fact与绑定槽；每步均先授权、后核实际
指令再提交，后根替换要求前根完成退休，最终不得留半完成项。只消费既有通用drop指令，
不重复发射清理，并拒绝与旧owner/header drop receipt重叠。完整false分区与必要suffix还建立独立的同控制退出链：死根drop以及活根Name→binding→drop
均核原始fact、当前finished/exit块和实际指令，再按证据顺序单次提交；最终全组必须完成。
缺分区或suffix不全时整个退出链不选中，不能把一部分消费冒充完整退出。后续同root的新
生命周期按不同fact保留通用清理，不被已完成的退出链认领。CI-only覆盖缺失permit、错来源、重复提交；
本地只编译，普通native矩阵核真实每根消费记录。

## 实际后端消费

独立plan仅选择同callable的唯一审计记录，不创建旧单根header plan。

1. 在真实while发射时捕获entry/header/body/finished/exit和counter/guard/root槽。
2. 复用实际checked-expression receipt，要求指定RHS的Subtract、左操作数为counter、
   右操作数为唯一常量1，失败边仍到终结abort，成功边单独保存。
3. success内必须是该减法结果写counter，再以更新后的counter与唯一常量0做Greater，
   最后将结果写guard。核两个实际assignment span、比较RHS身份及严格先后顺序。
4. header只读实际guard并分支body/finished；正常回边来自success，不能来自abort。
   初始counter定义必须先于比较，初始guard也必须消费同counter大于0的实际比较结果。
5. 每根真实Replace的root/producer/call必须在同一指令对应；旧值alias与实际已消费drop
   均在减法之前。success不能读取或修改root及detached旧值，header不得改写它们。

额外完整boundary审计只接纳可解析的literal入口与完整false分区/必要suffix：按root身份
连接entry向量与body顺序，追溯真实入口alias链；false分支核finished→exit及唯一前驱，
逐根匹配已消费drop，活根核真实suffix alias→drop，并按整个回执向量验证消费先后。
参数来源入口、缺分区或未完成suffix不给完整receipt，但保留旧scalar/body审计及lowering。
完整校验先产生私有单次消费结果，绑定同一evidence实例、实际几何及root槽向量；最终
发布前再次核真实checked指令、失败分支与终结abort，全部成功才发布完整receipt。
中途失败不留下counter完整或scalar完成记录。该receipt仍不授予跨轮权限，checked
abort不加入正常出口；该局部原子性不意味着整个编译流水线日志的事务回滚。
私有结果仅在当前无生产CFG变更的finalization范围消费，不是任意后续CFG修改后仍有效的证明。

counter/guard定义检查只覆盖本次入口与循环区域，不把合法退出后的重新赋值算作冲突。
现有SSA verifier和通用ownership/drop权限仍必需。本审计不取代它们，不认证退出后的
一般控制流，也不独立证明AST算符翻译、终止性或运行时owner代际归纳。

## 验证范围

完整出口另有0/1/全部活根×0/2轮native矩阵、参数根回退和不完整suffix回归。

真实2/3根、Int最小值/-1/0/1/2/5初值矩阵中，Boolean Name guard形式要求新审计完成；直接比较
while条件仍没有该证据。所有样例继续要求旧multi-root boundary缺席，并验证SSA、
LLVM和native输出。另有循环后counter/guard重赋值、inout counter不启用新审计但
保持旧lowering可用的native回归。

CI-only负控颠倒初始定义与比较顺序，或破坏counter写回、比较来源、guard写回或将success回边改到abort；前端另有
deferred、foreign analysis、伪Complete撤销负控。本地只编译，执行结果留远端。

## 当前权限缺口

当前记录只证明本次slot CFG与FE事实对应，并未产生跨轮owner权限：

| 合同 | 当前证据 | 尚未覆盖 |
|---|---|---|
| 初值 | 本地counter先定义、同值比较、guard写入、entry到header | 已封存原入口向量与scalar身份，但尚未授予入口权限 |
| 正常迭代 | Replace旧值退休后减1，写回再比较，success返回header | 未签发新代际向量或对每轮消费做归纳 |
| 溢出 | checked receipt验证真实失败分支和终结abort | 不把“预计不可溢出”作为删除abort或授权回边的依据 |
| 零轮与退出 | 非正初值真实native跳过body；现有通用drop继续工作 | 已封存静态false分区/完成suffix，尚非零轮/多轮退出权限 |

按既有[Int checked语义](../guide/04-expressions-operators.md#整数溢出与除零)，
正counter减1在数学上可表示，非正初值不进入此精确循环；这只是受限合同的推理，
不是编译器已经持有的范围证明。最小Int初值验证零轮退出，不能称为实际走过overflow边。
abort是进程终止，不得合成为正常header incoming；根的正常最终清理也不能挪到abort上
以冒充正常退出证明。当前保留checked指令、独立失败边和所有旧权限要求。

## 多control采集边界

静态候选发现按函数顶层的顺序while分段，只记录紧邻declaration-only分段且符合精确counter尾部的
control/initializer身份。候选清单不含owner ID，不代表入口或循环运行已经发生。
中间有普通语句、沿用前段guard/counter或尾部不匹配者不在候选集合；整组拒绝只指已发现
的精确候选，不声称发现函数内所有counter或嵌套循环。候选外后续控制仍可能使真实
方程失效；单候选不是无条件发布承诺。
真实while入口调用后，只有原catalog实际capture或独立第二段入口核验成功才从Discovered进入Entered；缺失
catalog或重复进入转Rejected。单候选还需真实完成方程才可Completed；多个候选不能
从失败批次选首个成功项作为单产品，只允许下述完整组发布。

真实双counter回归可观察首项入口及normal方程；旧observation首次Unsupported仍清空
其definitions/events，第二项仍不走旧入口capture，不恢复Complete或注入ValueState owner。

现在另有私有第二段catalog：仅两个精确顶层候选、两者之间全为声明时建立采集范围；
在真实首声明入口核实owned、borrow、pending、closure等完整边界为空才启动。
实际String完成分配独立namespace的dense token；实际Name move和binding转移该token，
实际named drop入口移除它并记录生命周期事件。死前缀producer保留在catalog但不在活根向量；
token不来自清理、AST枚举、旧observation恢复或上一循环first-body猜测。
第二while之前逐语句核对完整实际根集合与pending为空后冻结入口快照；前段活根跨入、
未知token、事件缺失或顺序不符均拒绝。Drop事件尚非绑定最终fact的退休receipt。
独立入口现在可按control一次性接入既有RootEvidence，且必须核对全部实际legacy owner为None；
不把独立token写回ValueState。条件完成只认可发现时的同一纯Name guard及完整未变根形状。
随后复用真实body-entry、literal、Replace、返回binding、最终Drop fact与counter尾部完成hooks；
第二段可形成私有entry/first-body方程及实际false-exit活根/清理分区。重复根、滞后清理及
不在精确线性合同内的条件路径不产生该方程。前缀Drop日志与body最终Drop fact的证据等级不同。
这些是编译遍历中的实际阶段事件，不是多次运行迭代trace；独立catalog identity也不是
原observation的analysis identity，不放宽旧getter的Arc检查。

`counter_batch_evidence_v1`现在只读发布精确两成员闭合组：第二while必须是函数最后语句，
每个真实退出均须完整空边界和全根dead分区。构建时固定原analysis与第二catalog各自identity；
实际退出按control顺序封存，各成员保留自己的命名空间，不拼成假单catalog。缺失、重复、
乱序或任一成员失败立即清空全组；最终seal核对控制、成员身份、callable、根及body catalog
独立性。deferred清除整组，旧单counter及其他loop getter不投影该组。
这是原子审计证据发布，不是后端权限迁移：现有codegen仍只选择单counter Plan，双counter
继续走通用lowering，尚无批次receipt或多Plan强制消费。嵌套loop、跨段活根运输也未启用。
