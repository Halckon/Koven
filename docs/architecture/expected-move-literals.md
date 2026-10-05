# Expected move literal 的前端事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 lambda expected gate 或临时 capture 清理时 · **唯一真源**：`lang-frontend` 两 checker 与 ownership/drop planner、直接测试

两入口 lambda expected gate 已按启用的 v0.41 对齐：普通 Function 上下文中的 move literal
保留该 expected canonical identity；AST 的 move 身份继续交给 ownership。strong move、已定型
named Function 的身份、arity 和参数 mode 仍有精确 L0084 反例。直接覆盖位于
`type_callable/expected_move_literals` 与 `multifile_type_checking/lambdas`。

两 ownership 入口继续按捕获事实发布 Owned/Move；返回环境交付给 caller，不在 factory
内重复释放源参数。局部及临时 callback 的 Shared 参数 loan 保持到自身调用返回，最终
ASAP drop 释放 capture。普通 borrowed closure 返回、从 Borrow 来源 move capture、捕获后
重复消费源仍实际产生 L0137/L0138/L0131；错误原子清空可执行 facts。

unit 的临时 closure 收尾按 capture facts 逆序发布 Owned+Move 的 Captured drop，随后才
发布 Temporary drop；Copy capture 不析构，Shared capture 按逆序结束 loan。正常 CallReturn
与放弃调用的 ControlTransfer 复用同一入口，Abort 无清理。两 String 和 Copy Int 捕获、
提前 return 及 iteration 退出的 ordered actions/flat facts 一致性已由前端 intent tests 验证。

独立复核及六个完整相关 suite 的482项实际结果见
[验收收据](../development/evidence/runtime-constructor-0279/frontend/receipt.json)。
这些事实不证明源码 runtime constructor/helper 的具体 callable ABI 或 native 释放已经交付。
