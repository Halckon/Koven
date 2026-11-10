# 单临时 String 借用的生产核验

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 P1 literal caller 路由或 receipt 时 · **唯一真源**：codegen temporary_route/temporary_receipt 与普通测试

无参数、非泛型 Unit caller 的单一直线 Declaration Borrow 调用，唯一实参为 raw
Text-only String literal，正常 owned Int/Unit 结果时可独立进入 refine。完整 subset
仍必须通过，typed/ownership 触发不看 coverage/observer；callee 与非目标旧路由独立。
Complete-empty 零action与多个临时借用不在此片，空字符串内容仍是合法单literal。

该形状只消费同轮非空 Complete1 三原action，核唯一旧loan/end/temporary Drop、
完整argument contract及source token/callable/call/callee身份。原decode_plain后以
实际expression ID激活formation，原StringLiteral push捕获owner Slot；adapt后核仍是
该Slot且原temporary表映射一致。原RootPlace/Shared Begin/DirectCall/End/TemporaryDrop
唯一发动作，核验不重放清理或按Span寻找匹配。

RStmt的Temporary标记与Receiver/Named/Consumer显式互斥，合块时随语句移动，
Alias无SSA指令。实际append处核六角色与冻结owner/place/loan实体、六InstructionId；
callee entryloan独立，Int结果1/Unit0。漏形成、重复、错身份或漏结束为结构化内部错误。
私有LoanReceipts仅容纳四独立emission tracker，四家族计划、角色与finish仍分别核验。
owner B/Call/Return的授权与唯一cleanup顺序不变。

off/Partial/Unsupported/缺席/非目标形状不选P1计划，不发布absence或全callable覆盖。
Basic rooted/memory的Int/Unit/空字符串真实重分析off，原off产品直接lower；比较完整
旧facts副本及SSA/LLVM。同analysis P1/P2/P3/P4/B/Call/Return普通邻控保身份分离。
人工损坏/foreign检查仅编译，未执行；不代表Constant off/native/资源计数或旧oracle退休。

两次独立能力红绿、精确输入和限制见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#2026-11-10-单临时-string-p1-路由与生产核验)。
