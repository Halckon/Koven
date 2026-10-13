# 普通多参数借用返回

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改多参数普通Borrow结果时 · **唯一真源**：lang-codegen源码与正常source测试

多参数调用按源码顺序求值、typed argument mapping 排列 ABI 参数，来源再与真实
BorrowSourceLoan 的 argument identity 对齐。unit call frame 保留每实参新建 loan 的 pending
槽位来源，只有来源组延续到结果；不能把 loans_ending_at 的全部枚举当成非来源终止事实。
两入口先结束全部无关新建 loan，再消费 AfterExpression/CallReturn drop；同 owner 多次
Borrow 不合并实参身份，复用入口或外部 borrow val loan 不会由内部调用结束。BorrowCall verifier检查所有
Loan operands的活性，结果alias与依赖仍只连接唯一source。16组普通源码已通过两入口
SSA/原版LLVM及object/link/run，覆盖来源首/中/末、命名反排、同root/不同root、wrapper、
Resource临时与字段、Copyable和MoveOnly泛型；有界账本见
[SPEC-0305](../archive/specs/0305-multi-parameter-borrow-result.md)。

普通借用包装调用的实参不接通CFG运输：除原控制流门外，实参子树中的checked负号和
非空断言明确拒绝，并核对实参lowering未新增caller block。辅助实参的即时range producer、
具体callable storage仍留后继；已有稳定View metadata不随之禁用。此片不改变LLVM ABI，
不证明完整M2B、receiver/Inout来源、复杂CFG或完整NLL。

实现入口是两套`borrow_result.rs`、unit `call.rs`、`verify_borrow_result.rs`与
`verify_ownership/borrow_result.rs`。模型的BorrowCall/source与BorrowReturn身份保持，
LLVM adapter继续使用既有多个Loan pointer参数和单一storage pointer结果。

语言合同见[Guide10](../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)，
本片不修改现行语义或ADR。普通结果的基础storage及词法/last-use规则见
[原实现快照](map-native-execution.md#普通借用返回与-caller-continuation)。
