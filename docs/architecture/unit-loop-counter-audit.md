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

已选counter范围的每个精确RootReplace call在实际发射前必须获得私有一次性permit：
核当前body块、root槽及原catalog对应的真实producer槽；发射后核同一指令位置、call span、
root/replacement和返回旧值，再提交该项。所有项最终必须恰好提交一次。范围外维持通用
lowering；这不是借用旧单根plan，也不从“没有receipt”推断所选范围允许跳过消费。

这是局部发射消费合同，不是新的公开跨轮owner产品。typed/inout及通用drop权限仍保留，
binding/drop尚未迁移为该permit链的一部分。CI-only覆盖缺失permit、错root、重复提交；
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
