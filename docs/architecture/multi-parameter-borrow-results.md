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

`type_borrow_result` 三项原签名与实参映射测试使用 MemoryDocument/Rooted 双入口，
保留可空/inout/this 来源正例、L0162 拒绝以及 Parameter(1)/named argument 映射断言。
这些 raw 类型事实不额外授予 ownership 或 native 能力。
`ownership_borrow_result` 的四项原消费者也覆盖双入口：wrapper/local scope 恢复、实际
foreign/temporary 来源拒绝、借用存活期间禁止 source move，以及普通 val 不隐式拥有借用结果。
`ownership_borrow_origins` 五项消费者在双入口保留 direct/projected/nullable/wrapper 的
实际来源数量、未证明路径与错误交付的原子拒绝、精确 primary/来源 label 断言。
`ownership_borrow_last_use` 全八项原测试覆盖双入口，保留父子逆序/独立结果的精确
source-qualified endpoint、未来使用/Map失效冲突、call-return 边界与保守 scope endpoint。
`ownership_borrow_continuation` 的 capture/conditional/nested-call/temporary/loop 五项拒绝
消费者也覆盖双入口，错误时 bindings/ends 不发布；独立 Single loan-end witnesses 保留。

普通直接调用的 Unit loan-end 是有界观测切片：Owned-return 根实参/普通 String 的
成功前缀已有固定事件与唯一 owner-drop 对照；后续真实 ownership 错误或 deferred
会撤销事件能力。它不包含借用返回来源交接，不能替代 continuation 的 Single loan-end
stream；该差异对照在对应消费者中仍需保留。

`borrow_call_handoffs()` 额外发布直接 borrow-return initializer 到显式 borrow val 的
有界正常返回观察：真实 pending source token 捕获后，只在 binding 正常提交时发布，
保留 source-qualified loan/binding/CallReturn 身份。首片无 receiver/parent，来源限当前
存活的 formed-owned 根，实际 loan tokens 限 Shared 根；Value 辅助参数并未整体排除。
错误/deferred 会撤销该能力；不提供辅助 loan 结束或完整 cleanup 流，旧 Single 对照保留。
其余五项普通 Unit continuation 消费者也覆盖双入口：nested/alias 权限恢复、live-result
冲突、scope/return 父子清理、真实诊断 span 与 nullable/generic/copyable binding；
三项依赖独立 loan-end stream 的 Single 对照仍完整保留，其 Unit 半部同样覆盖两入口，
并校验 source/key 身份；parser-only 拒绝不变，Unit 仍未发布独立 loan-end stream。

`ownership_nested_borrow_arguments` 的 direct/grouped、named/mixed/multiple-chain 与
generic/field/wrapper/parent 三项成功消费者覆盖双入口，保留独立 Single loan-end
及逐项 call-use 合同；另外两项精确顺序/owner-drop 成功 witness 也覆盖双入口，
保持 grouped/named 身份、parent 存活与 owner 在 outer-call-return 唯一 drop 的断言。

`borrow_call_prefix_ends()` 从实际 Return unwind 的 EndCallLoan 动作发布借用来源结束
切片；ValueState 记录真正进入的 direct initializer 与候选source argument身份，实际
unwind时核已求值token、原operand place、仍存活Shared owned根并记录action下标。
因此不再要求最终binding证明，可覆盖无正常出口的直接initializer，但不表示结果建立；
正常handoff、结果终止和完整cleanup流不能互相替代，原Single对照继续保留。
