# SPEC-0195：跨 callable Borrow 的 SSA/LLVM lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-195` |
| 所属 Phase | Phase 3/4 纵向切片 |
| 语言规范 | 现行 [`guide/01-design-decisions.md` §26、§30.2](../guide/01-design-decisions.md) |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据依赖图推进”的站立授权 |
| 前置 Spec | SPEC-0029、0034、0035、0045 `done` |
| 前置 ADR | [ADR-0016](../adr/0016-interprocedural-borrow-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0008、0015 |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` callable/loan SSA、frontend lowering、LLVM adapter，Architecture/Roadmap |
| 语言语义变更 | 否；实施既有 Borrow 参数与调用期 loan 语义 |

## 2. Goal

完成后，MoveOnly owner 可经默认 Borrow 参数同步调用而不被当成 Value delivery 消费；
`inspect(owner.value)` 可从 Rc payload place 建立 loan、完成 native call，并在返回后继续合法使用
和最终 release owner。

## 3. 范围与需求

- callable SSA signature 与 DirectCall operand 保留 Value/Borrow delivery identity。
- Borrow 实参消费 frontend `LoanFact`，按 `RootPlace`/payload place → shared loan → internal
  reference → DirectCall → BorrowEnd 的顺序 lower；不按 AST marker 重推 mode。
- callee entry 可接收并读取 Borrow binding；MoveOnly target 不产生 owned value、Copy 或 retain。
- operation/type/ownership verifier 拒绝 mode/type/loan kind 不匹配、inactive loan、call 前 move/drop、
  call 后 reference 逃逸与缺失 BorrowEnd。
- LLVM 将 internal reference 作为非空 pointer 传递；必要的 value addressization 不复制 owner，
  verified-before-LLVM 保持成立。
- 覆盖普通 class/Box/Rc payload 的 MoveOnly Borrow，确保该能力不是 Rc 名称特例。

## 4. 非目标

- 不完成一般 Inout source lowering、FFI reference ABI、dyn dispatch、异步/跨线程 borrow 或引用返回。
- 不改变 frontend ParameterMode、loan 诊断或 Borrow 默认语义。
- 不实现 nullable handle；该能力交给 SPEC-0196。

## 5. 验收标准

- [ ] SSA signature、internal reference、DirectCall operand、render 与 model verifier 正反矩阵通过。
- [ ] ownership verifier 证明 Borrow call 不消费 MoveOnly owner，且 loan 精确覆盖同步 call。
- [ ] frontend→SSA 覆盖 Copyable 与 MoveOnly Borrow；Value 参数仍精确消费。
- [ ] Rc MoveOnly payload Borrow native build/run 退出 0，call 后 owner 可 share/drop。
- [ ] LLVM IR 使用 pointer ABI，无隐式 copy/retain、无 reference 逃逸。
- [ ] Architecture、Roadmap、workspace check/Clippy/test/fmt 基线同步。

## 6. 技术方案与边界

严格实施 ADR-0016：caller-local `LoanId` 不跨函数复用，而是形成受 loan 支撑的内部 reference
operand；callee entry 接收 reference identity并显式取得只读 place。第一提交先封闭 SSA model/
verifier，第二提交接 frontend/LLVM/native，避免同时改动验证规则与后端行为而无法定位漂移。

## 7. 实施计划

1. [ ] 扩展 callable signature、internal reference operation 与 DirectCall verifier。
2. [ ] 接入 frontend loan facts、callee Borrow binding 与 ASAP drop/控制转移。
3. [ ] 接入 LLVM pointer ABI、Rc/class/Box native 正反验收。
4. [ ] 同步 Architecture、Roadmap、验证记录与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Borrow-call SSA model/render/verifier | `feat(codegen): verify borrow call operands (SPEC-0195)` |
| 2 | frontend/LLVM/native 与完成文档 | `feat(codegen): lower borrow calls to native (SPEC-0195)` |

## 9. 未决问题

- 无。Inout source lowering与 reference return 已明确排除，不阻塞本 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 前置审计 | 通过 | 0045 core `done`；ADR-0016 `accepted`；现有 DirectCall 只接受 ValueId 的缺口已由源码级 Rc 验收确认 |
