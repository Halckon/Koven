# SPEC-0191：instance receiver 与静态委托 lowering

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-191` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 无；当前仍等待前置 SPEC-0180/0181 完成后的独立批准 |
| 前置 Spec | SPEC-0034、0035、0038、0039、0177、0184、0195 `done`；SPEC-0180/0181 待完成 |
| 前置 ADR | [ADR-0016](../adr/0016-interprocedural-borrow-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0008、0009 |
| 阻塞项 | SPEC-0180/0181 `done` |
| 影响范围 | `lang-codegen` callable SSA/frontend lowering/LLVM/member native tests；Architecture/Roadmap |
| 语言语义变更 | 否；lower 已验证 receiver facts |

## 1. Goal

完成后，class/value/enum/object/interface 的静态分发 member/default/override 与 Borrow-only
delegate 调用可经 verified SSA、LLVM、object/link/run 执行，receiver mode 与 frontend
所有权效果一致且不生成 vtable、proxy、隐式 copy/retain 或额外 allocation。

## 2. 范围与需求

- callable SSA signature 把隐藏 receiver置于第一参数；Value 使用既有 value/owner ABI，Borrow/
  Inout 使用 ADR-0016 shared/exclusive loan pointer ABI，显式参数顺序随后保持不变。
- member instance key 使用静态 callable target、owner type arguments、callable type arguments；
  从 receiver typed/ownership descriptor lower，不按名称或 AST owner 重新选择。
- lower member body 的 `this`、field place、Borrow/Inout load/store 与 Value owner/drop；receiver
  求值一次，loan/copy/move 在第一个 argument 前发生并与 frontend end/drop facts对应。
- Inout ordinary-class receiver传递现有 handle storage 的 exclusive loan；callee load 同一 handle
  后只修改 payload field。SSA/verifier 与 LLVM 测试拒绝 receiver-binding store、handle 替换或
  把 payload pointer 误当成另一套公开 ABI；inline receiver 继续借用实际 inline storage。
- default/override/`super<I>` 只生成静态 direct call；interface `Self` 在单态化时替换为具体
  owner，不产生 runtime interface value。
- 无状态 object receiver 按静态唯一 value identity lower，可使用临时 ZST addressization 满足
  Borrow ABI；不生成 singleton allocation、全局初始化、guard 或退出析构。
- Borrow-only delegation 生成确定性静态 thunk 或等价直接转发；只投影 delegate field 和转交
  既有 loan/arguments，不生成隐藏 AST、retain、proxy allocation或新 owner。
- SSA/verifier 拒绝 receiver mode/type/loan kind、instance key、ownership plan 与 callee signature
  不一致；verified-before-LLVM 不变。
- 真实 source→object→link→run 覆盖多种 nominal/generic receiver、drop 与调用顺序；DWARF
  仍指向用户 member body，compiler thunk 不伪造源码声明。

## 3. 非目标

- 不实现动态 dispatch/vtable、dyn value、FFI method ABI、bound callable reference或extension。
- 不实现 safe-call、nullable assertion/when receiver、borrow-return或 iterator/provider runtime。
- 不增加非 Borrow interface delegation、公开符号稳定 ABI或跨 compilation-unit member ABI。

## 4. 验收标准

- [ ] SSA signature/DirectCall receiver mode 与 verifier 正反矩阵通过；本 Spec 不为已排除的
  bound method value 虚构 `CallableInvoke` receiver source path。
- [ ] class/value/enum/object、generic owner+method、default/override/`super<I>` 静态实例运行正确。
- [ ] Borrow/Inout LLVM pointer ABI、Value owner ABI、receiver-before-arguments 与一次求值被 IR/运行锁定。
- [ ] class Inout val-handle native mutation 保持 handle identity；verifier/LLVM 反例拒绝重绑
  receiver、写回另一 handle或使用 payload-only 私有 calling convention。
- [ ] Borrow delegate 与手写转发结果/loan/drop 一致，无 vtable/proxy/retain/额外 allocation。
- [ ] MoveOnly Value receiver 唯一消费、Borrow/Inout 不消费，正常/提前退出 drop 精确。
- [ ] 受影响 `lang-codegen`/CLI 窄测及 workspace Layer 2 静态门禁通过，Architecture/Roadmap/Spec 同步。

## 5. 技术方案与边界

扩展现有 callable signature/instance planner 和 frontend lowerer，而不是建立 method-only IR；
receiver 只是带显式 delivery identity 的第一个 operand。复用 ADR-0016 的 addressization/loan
parameter、ADR-0008 nominal ABI 和 ADR-0009 closure 环境。delegate thunk 使用编译器内部稳定
FunctionId，不形成源码 `DeclarationId` 或用户可见 stack frame。

## 6. 实施计划

1. [ ] 扩展 SSA callable/operation/verifier receiver → 验证：model/render 正反矩阵。
2. [ ] 接 frontend member body/call 与 LLVM ABI → 验证：nominal/generic native tests。
3. [ ] 接 default/override/super/delegate 静态转发 → 验证：运行、drop、无动态设施。
4. [ ] 同步 Architecture/Spec并运行 workspace基线。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver callable SSA/verifier 与 frontend/LLVM native 闭环 | `feat(codegen): lower member receivers (SPEC-0191)` |

## 8. 未决问题

- 无；iteration、nullable member forms 与跨 unit ABI 分别保持独立门禁。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | 既有 callable SSA 只遍历显式参数；ADR-0016 已为未来 receiver 固定 pointer ABI |
| 2026-08-26 候选闭合审计 | 通过 | 移除无 bound method source path 的 CallableInvoke receiver 验收，只保留 DirectCall |
| 2026-08-31 重基审计 | 通过 | v0.34 已重基到完整 v0.33；本 Spec 继续等待 SPEC-0180/0181 与独立批准 |
