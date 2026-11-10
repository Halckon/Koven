# 有界临时 String 借用的生产核验

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 P1 literal caller 路由或 receipt 时 · **唯一真源**：codegen temporary_route/temporary_receipt 与普通测试

无参数、非泛型 Unit caller 的单一直线 Declaration Borrow 调用，实参为一个或恰好两个 raw
Text-only String literal，正常 owned Int/Unit 结果时可独立进入 refine。完整 subset
仍必须通过，typed/ownership 触发不看 coverage/observer；callee 与非目标旧路由独立。
Complete-empty、三个及更多literal仍不在此片；一个/两个实参的typed参数位置须构成
完整精确置换，两个命名literal可重排，空字符串内容合法。不是所有Complete1形状都已迁移或消费。

该形状只消费同轮非空 Complete1 三或六原action，每个operand核唯一旧loan/end/temporary Drop、
完整argument contract及source token/callable/call/callee身份。原decode_plain后以
实际expression ID激活formation，原StringLiteral push捕获owner Slot；adapt后核仍是
该Slot且原temporary表映射一致。原RootPlace/Shared Begin/DirectCall/End/TemporaryDrop
唯一发动作，核验不重放清理或按Span寻找匹配。

RStmt的Temporary标记与Receiver/Named/Consumer显式互斥，合块时随语句移动，
Alias无SSA指令。一份call-level计划保独立operand状态，实际append处核逐个形成、
RootPlace、Begin按源码argument_index；每个operand独立保存经contract核过的parameter_index，
唯一共享Call按完整参数槽向量核验，再核源码逆序End和原发布顺序Drop。禁止排序operand、
按bytes配对或从Call向量逆推出cleanup；构造与SSA两阶段分别核完整映射。
单literal六InstructionId/三实体，双literal十一InstructionId/六实体；同字节临时仍不合并。
每个owner/place/loan按实际形成冻结，后续角色同时核稳定实体及当前Slot映射；
callee entryloan独立，Int结果1/Unit0。漏形成、重复、错身份或漏结束为结构化内部错误。
私有LoanReceipts仅容纳四独立emission tracker，四家族计划、角色与finish仍分别核验。
owner B/Call/Return的授权与唯一cleanup顺序不变。

off/Partial/Unsupported/缺席/非目标形状不选P1计划，不发布absence或全callable覆盖。
Basic rooted/memory的单/双literal、Int/Unit/相同字节/空字符串真实重分析off，原off产品直接lower；比较完整
旧facts副本及SSA/LLVM。同analysis P1/P2/P3/P4/B/Call/Return普通邻控保身份分离。
人工损坏/foreign检查仅编译，未执行；不代表Constant off/native/资源计数或旧oracle退休。

两次独立能力红绿、精确输入和限制见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#2026-11-10-单临时-string-p1-路由与生产核验)。

双literal的独立route红绿与receipt红绿、实际producer顺序和排除边界见
[SPEC-0341增量合同](../specs/active/0341-shared-lowering-core.md#2026-11-10-双临时-string-p1-路由与生产核验合同)。

命名双literal的独立route/receipt红绿及source序与parameter序对照见
[SPEC-0341映射合同](../specs/active/0341-shared-lowering-core.md#已批准的双-literal-命名参数映射增量2026-11-10)。
