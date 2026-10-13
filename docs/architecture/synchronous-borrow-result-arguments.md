# 普通借用结果的同步 Borrow 实参

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改普通借用结果的同步调用消费时 · **唯一真源**：frontend/codegen源码及正常source测试

具名普通函数可在直线caller中通过 `observe(view(source))` 消费已支持的普通借用结果。
outer返回Unit或既有owned结果；inner沿多参数Borrow/Value、具名wrapper、具体泛型、字段
返回及透明Group合同。语言依据为[Guide10](../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)，
参数/结果ABI继续由ADR-0016/0029规定。

## 前端来源与终止事实

`BorrowResultFacts::call_uses()`发布`BorrowCallUseFact`：规范inner调用、outer调用、原始
argument identity、原`BorrowSourceLoan`、canonical来源lease、既有parent与outer CallReturn。
Group只归一inner调用，原outer operand仍保留；没有伪造binding symbol或payload owner。

两入口按源码顺序检查参数；inner来源Shared active loan在成功返回后转由outer call持有，
再发布outer operand的canonical LoanFact。该lease用于重叠权限；lowering必须用返回的真实
payload Loan，不把source对象地址当payload。后续operand的own move或exclusive重叠仍冲突。
错误来源/owned交付、诊断或deferred沿现有原子发布门清空call-use事实。

single把原source loan从inner PendingCall移入outer帧，保持原call/argument identity，
在outer返回发布显式LoanEndFact；同帧普通operand/result先于continued祖先发布。
其`loans_ending_at`仍限定原call，codegen另核source的精确终止事实。unit保留自身按call枚举
的loan查询，在inner清理前将canonical owner交给outer pending borrow，防止ASAP提前drop。
两入口不把结果当MoveOnly temporary，也不改变Resource词法析构策略。

## SSA 与普通 native

single普通DirectCall在局部Vec中运输`lower_result_call`返回的新建source祖先；outer先结束
参数/result loan，再结束这些祖先，最后消费owner drop。unit复用既有argument_loans/
created_loans调用帧，按每条依赖链子先父的偏序清理。独立链可交错收尾；相关owner必须等
全部关联loan结束才drop。已存在的Borrow entry或borrow val parent不在本次created链中。

`BorrowCall`结果直接作为`DirectCall`的Loan operand，不创建owned payload或隐式copy/retain。
既有verifier验证活性、target、唯一来源及依赖；LLVM继续交付原target-storage pointer，
本片未增加公共SSA operation、verifier豁免或LLVM ABI。

源码测试覆盖single与真正跨文件unit，包括named反排的同类型Resource实参值/析构顺序、
source与payload类型不同的实际字段loan、两层wrapper、同root多结果、ASAP最后使用、Resource
词法析构、Borrow entry与已存在借用作来源/辅助参数。验证读取正常source产生的原样Program，
随后使用原版LLVM/object/link/run；实际命令和运行范围见[SPEC-0307](../specs/active/0307-synchronous-borrow-result-arguments.md)。

## 保持的边界

outer借用结果继续绑定/返回、temporary来源、actual receiver/Inout来源、间接/内建特殊
调用、跨线程、CFG/loop与含Nothing的outer调用前缀仍无本片延续证明。后序错误调用/return
不能发布一个并不存在的outer正常Return终止。owned参数（含Int）、普通val、owned返回
继续拒绝直接借用结果；显式borrow val的Copyable读取仍按已有独立规则处理。

实现入口为两套`borrow_call_use.rs`、`borrow_result.rs`、pending-call/drop-planner接线及
single`call_lifetimes.rs`/unit`borrow.rs`；普通多参数内部返回见[既有快照](multi-parameter-borrow-results.md)。
本片不表示完整M2B、任意表达式借用、receiver/Inout或完整NLL已实现。
