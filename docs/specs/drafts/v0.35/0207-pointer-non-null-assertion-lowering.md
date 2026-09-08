# SPEC-0207：pointer-like 非空断言 lowering

> **性质**：draft Spec · **状态**：draft（等待前置 Spec） · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-207` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.35 空安全](../../../guide/09-nullability-errors.md)与[阶段边界](../../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0034、0039、0184、0196 `done`；SPEC-0205/0206 待完成 |
| 前置 ADR | ADR-0017 `accepted` |
| 阻塞项 | SPEC-0205/0206 `done` |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
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

- [ ] native 覆盖同名 error 遮蔽时 !! 仍 abort，显式 error 调用仍选择源码声明。

- [ ] SSA/verifier 覆盖合法 take、伪/跨 owner proof、重复 take/drop 与 null-edge direct Abort。
- [ ] class/Box/Rc 非空结果与 null 进程终止 native 测试通过，operand副作用只发生一次。
- [ ] Rc retain/release、Box/class free 与 moved binding 后续行为正确，无额外 tag/allocation。
- [ ] Borrow/Inout/field/element 与 inline nullable 继续确定性 unsupported；既有 nullable if/when
  的受影响契约回归。
- [ ] Architecture 同步。

## 6. 技术方案与边界

在 frontend lowerer增加 descriptor-driven `NonNullAssert` 分派，复用既有 nullable SSA/LLVM 与
Abort primitive，不新增 parallel unwrap operation 或后端 AST 模式匹配。

## 7. 实施计划

1. [ ] 接 extraction facts 到 NullableTake/CFG → 验证：lowering/verifier 窄测试。
2. [ ] 接 LLVM/native class/Box/Rc → 验证：IR 与真实进程正反测试。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA/LLVM/native 与完成文档 | `feat(codegen): lower non-null assertions (SPEC-0207)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | ADR-0017 已定义 NullableTake，frontend lowerer当前仍确定性拒绝 `NonNullAssert` |
