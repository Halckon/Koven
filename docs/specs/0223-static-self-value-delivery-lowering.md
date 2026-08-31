# SPEC-0223：StaticSelf Value receiver 条件交付 lowering

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-223` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 2026-09-01 持续 Goal；SPEC-0222 已完成并提交 validated conditional delivery fact |
| 前置 Spec | SPEC-0191、SPEC-0222 `done` |
| 前置 ADR | ADR-0016 `accepted` |
| 关联 Spec | SPEC-0181、0191 |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` receiver SSA/frontend lowering/LLVM/native tests；Architecture/Roadmap |
| 语言语义变更 | 否；只消费 SPEC-0222 的 validated facts |

## 1. Goal

完成后，Value interface default 中的显式 `this` delivery 与隐式 Value receiver call 可按 SPEC-0222
事实 lower 到 verified SSA/LLVM/native，MoveOnly owner 恰好转交一次，Copyable specialization 可重复使用。

## 2. 范围与需求

- lowerer 只消费 source-qualified conditional delivery fact，并交叉核对 current receiver、target、
  `StaticSelf` template 与 origin，再由 concrete specialization 的 copyability 决定 Copy/Move Value ABI。
- MoveOnly receiver 在 DirectCall 前从当前 receiver state 唯一 take/deliver，禁止同一路径再插入 receiver drop；
  Copyable receiver 生成既有 copy delivery。
- 正常/提前 return 与 CFG merge 必须保持唯一 owner identity；verifier 拒绝重复 consume/drop。
- LLVM 复用既有 aggregate/heap-owner Value ABI，不新增 receiver-only runtime ABI、allocation 或 retain。

## 3. 非目标

- 不在 codegen 推断缺失的 delivery/drop fact，也不修改 frontend ownership。
- 不开放非 Borrow delegation、dynamic dispatch、bound method、nullable member form 或部分移动。

## 4. 验收标准

- [x] SSA/verifier 正反矩阵锁定 MoveOnly take/deliver、Copyable copy 与 receiver-first 顺序。
- [x] 显式 `this` delivery、隐式 Value receiver call及正常/提前 return 均经 native link/run。
- [x] 缺失、重复或 identity 不匹配的 SPEC-0222 fact 在 LLVM 前 fail loud 并保留 Span。
- [x] receiver 与 native 职责窄测、workspace library check/clippy、fmt/diff 通过；未跑 frontend 全量测试。
- [x] Architecture/Roadmap/Spec 同步。

## 5. 技术方案与边界

复用 SPEC-0191 的 receiver-first DirectCall、current receiver state 与 Value owner/drop operation；新增逻辑
只负责把 SPEC-0222 的互斥 delivery 决策映射到现有 operation，不建立第二套 method IR。

## 6. 实施计划

1. [x] 消费 SPEC-0222 facts 并建立 verifier 红测 → 验证：缺 fact 不可进入 LLVM。
2. [x] lower CFG delivery/drop 互斥与 LLVM Value ABI → 验证：SSA/LLVM 正反矩阵。
3. [x] 接 native 正常/提前 return 并同步文档 → 验证：真实运行与精简静态门禁。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | StaticSelf Value delivery SSA/LLVM/native | `feat(codegen): lower static-self value delivery (SPEC-0223)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-codegen ssa::unit_lower_receiver_tests:: --locked --offline -- --test-threads=1` | 通过，45/45 | 含 MoveOnly/Copyable、显式/隐式/继承/`super`、正常/提前 return、非对称合流及发散 sibling/while 回归 |
| `cargo test -p lang-codegen native::unit_tests::value_interface_default_move_only_and_copyable_specializations_link_and_run --locked --offline -- --test-threads=1` | 通过，1/1 | 真实 object/link/run，stdout 为 `value-default` |
| `cargo check --workspace --lib --locked --offline` | 通过 | 精简 workspace library 静态门禁 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 未启用 frontend 全量测试 |
| `cargo fmt --all -- --check` | 通过 |  |
| `git diff --check` | 通过 |  |
