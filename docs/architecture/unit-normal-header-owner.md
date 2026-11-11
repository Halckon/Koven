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
