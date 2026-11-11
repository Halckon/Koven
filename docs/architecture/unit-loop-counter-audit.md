# 多根 counter 尾部的非授权实际审计

> **性质**：实现事实 · **状态**：current · **读取时机**：核对受限计数循环的发射与数据依赖 · **唯一真源**：counter_tail evidence及counter_tail_receipt实现

承接[多根消费与私有计数合同](unit-loop-root-suffix.md)。本文不是新的owner许可，
也不把既有lowering执行成功称为新multi-root boundary支持任意轮数。

## 前端输入

只为已有精确私有前缀和完整root退休方程提供额外审计证据：本地Int counter、Boolean
Name guard初始化为同counter大于0，body中每根直接String Replace并立即退休旧值，
末尾为counter减1写回、同counter大于0写回guard。记录真实source-qualified初始化、
赋值和两个RHS身份、原producer catalog及实际root Replace/drop事实。

该记录不暴露header/exit能力；旧multi-root boundary仍拒绝counter。getter要求原analysis
身份、同control的Unsupported Syntax及空legacy事件，不允许与其他产品混用。
失败或deferred分析撤销证据。不能借该getter把Unsupported改为Complete。

## 实际后端消费

独立plan仅选择同callable的唯一审计记录，不创建旧单根header plan。

1. 在真实while发射时捕获entry/header/body/finished/exit和counter/guard/root槽。
2. 复用实际checked-expression receipt，要求指定RHS的Subtract、左操作数为counter、
   右操作数为唯一常量1，失败边仍到终结abort，成功边单独保存。
3. success内必须是该减法结果写counter，再以更新后的counter与唯一常量0做Greater，
   最后将结果写guard。核两个实际assignment span、比较RHS身份及严格先后顺序。
4. header只读实际guard并分支body/finished；正常回边来自success，不能来自abort。
   初始guard也必须消费同counter大于0的实际比较结果。
5. 每根真实Replace的root/producer/call必须在同一指令对应；旧值alias与实际已消费drop
   均在减法之前。success不能读取或修改root及detached旧值，header不得改写它们。

counter/guard定义检查只覆盖本次入口与循环区域，不把合法退出后的重新赋值算作冲突。
现有SSA verifier和通用ownership/drop权限仍必需。本审计不取代它们，不认证退出后的
一般控制流，也不独立证明AST算符翻译、终止性或运行时owner代际归纳。

## 验证范围

真实2/3根、0/1/2/5轮矩阵中，Boolean Name guard形式要求新审计完成；直接比较
while条件仍没有该证据。所有样例继续要求旧multi-root boundary缺席，并验证SSA、
LLVM和native输出。另有循环后counter/guard重赋值、inout counter不启用新审计但
保持旧lowering可用的native回归。

CI-only负控破坏counter写回、比较来源、guard写回或将success回边改到abort；前端另有
deferred、foreign analysis、伪Complete撤销负控。本地只编译，执行结果留远端。
