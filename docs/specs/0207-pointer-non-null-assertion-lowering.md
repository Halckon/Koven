# SPEC-0207：pointer-like 非空断言 lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-207` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 v0.32；候选 [v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用) |
| 批准依据 | 无；v0.35 尚未启用 |
| 前置 Spec | SPEC-0034、0039、0184、0196 `done`；SPEC-0205/0206 待完成 |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | v0.35 明确启用；SPEC-0205/0206 `done` |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Roadmap/Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 pointer-like lowering |

## 2. Goal

完成后，class/Box/Rc nullable `!!` 消费 frontend extraction/ownership facts，经
`NullableBranch/Take`、verified LLVM 和 compiler-bound Abort 运行，且 move/drop 与源码契约一致。

## 3. 范围与需求

- 只消费 0205/0206 facts；首轮 pointer-like class/Box/Rc 均为 MoveOnly，只接受 owned
  whole-root/temporary operand并消费 owner，不虚构 Copyable pointer path。
- 先以 ADR-0017 `NullableBranch` 建 proof；non-null edge执行 `NullableTake(owner, proof)`，
  null edge直接 lower 0205 compiler-bound effect 到既有 SSA Abort primitive，不查询 `error`
  名称、不生成普通 call或 unwind cleanup。
- class/Box/Rc 的 pointer null niche保持无 tag/allocation/隐式 retain；verifier先于 LLVM 拒绝
  proof/owner/drop 不匹配。

## 4. 非目标

- 不实现 inline/tagged nullable、borrow unwrap、Elvis、safe call、`as?` 或新的 Abort ABI。

## 5. 验收标准

- [ ] SSA/verifier 覆盖合法 take、伪/跨 owner proof、重复 take/drop 与 null-edge direct Abort。
- [ ] class/Box/Rc 非空结果与 null 进程终止 native 测试通过，operand副作用只发生一次。
- [ ] Rc retain/release、Box/class free 与 moved binding 后续行为正确，无额外 tag/allocation。
- [ ] Borrow/Inout/field/element 与 inline nullable 继续确定性 unsupported；既有 nullable if/when
  和 workspace 基线回归。
- [ ] Architecture/Roadmap 同步。

## 6. 技术方案与边界

在 frontend lowerer增加 descriptor-driven `NonNullAssert` 分派，复用既有 nullable SSA/LLVM 与
Abort primitive，不新增 parallel unwrap operation 或后端 AST 模式匹配。

## 7. 实施计划

1. [ ] 接 extraction facts 到 NullableTake/CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：IR 与真实进程正反测试。
3. [ ] 同步验收与 Architecture → 验证：codegen、workspace、fmt/clippy 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower non-null assertions (SPEC-0207)` |

## 9. 未决问题

- 无；inline nullable 由新 ABI ADR/后继 Spec 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017 已定义 NullableTake，frontend lowerer当前仍确定性拒绝 `NonNullAssert` |
