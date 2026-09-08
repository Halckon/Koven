# SPEC-0205：非空断言 extraction typed facts

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-205` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0019、0022、0067 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` type checking model/checker/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 typed extraction 契约 |

## 2. Goal

完成后，type checker 为每个有效 `e!!` 发布 operand identity/category、nullable/inner 类型、
Copyable 条件、一次求值的 extraction descriptor 与 compiler-bound assertion Abort effect。

## 3. 范围与需求

- 保持现有 `T? -> T` 与 L0085；descriptor 精确关联 assertion、operand、operator Span 与类型。
- 保存 operand 的 place/temporary 及 binding category，标明 Copyable-copy 或 MoveOnly extraction
  候选，但不在 Phase 2 决定 loan/move/drop 成功。
- Abort effect 是由 `NonNullAssert` AST identity 直接产生的封闭 compiler-bound fact，不执行
  源码名称查找、不生成普通 `CallDescriptor`，也不受同名 `error` 声明遮蔽。
- descriptor 参加 overload/lambda trial snapshot/rollback，顺序确定且不写回 AST。

## 4. 非目标

- 不执行所有权检查，不生成 abort/SSA/LLVM；不实现 nullable borrow unwrap 或其他 control form。

## 5. 验收标准

- [x] 同名 error 声明不改变 assertion Abort identity；显式 error 调用仍按普通名称解析。

- [x] Copyable/MoveOnly、owned/Borrow/Inout root、temporary/field/element operand descriptor 矩阵通过。
- [x] assertion Abort effect 唯一、可回滚且不依赖 `error` 名称或普通 callable selection。
- [x] 非 nullable/Error/Deferred、nested postfix 与单次求值 Span/identity 稳定。
- [x] overload trial 完整回滚，L0085 与既有 type suite 回归。
- [x] Architecture 与实现事实同步。

## 6. 技术方案与边界

在现有 `Expression::NonNullAssert` type path 发布专用 descriptor，复用 `ExpressionCategory`、
place facts 和条件 `Copyable` 查询；ownership 阶段是唯一消费该候选并决定合法性的阶段。

## 7. 实施计划

1. [x] 增加 descriptor/API/trial snapshot → 验证：model/rollback 测试。
2. [x] 接 type checker 与 category 矩阵 → 验证：type-checking 窄测试。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、测试与完成文档 | `feat(frontend): describe non-null extraction (SPEC-0205)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | `!!` 已类型检查但没有 extraction descriptor，ownership 当前把 operand 当普通 Read |

### 本次验收映射

| 验收契约 | 实际目标 / 过滤器 | 结果 |
|---|---|---|
| 单次 operand 身份、operator Span、类型与无 synthetic call | `type_checking non_null_assertion_describes_one_evaluation_without_a_synthetic_call` | 实施前编译失败：缺少 assertion API；实施后通过 |
| Copyable/MoveOnly 与 6 类来源矩阵 | `type_checking non_null_assertion_records_extraction_candidates_without_deciding_ownership` | 通过（8 项定向测试合计） |
| 同名 error 与 Abort 独立 | `type_checking non_null_assertion_abort_is_independent_of_shadowed_error` | 通过（8 项定向测试合计） |
| overload trial 回滚与唯一发布 | `type_checking non_null_assertion_failed_trials_roll_back_and_selected_trial_is_unique` | 通过（8 项定向测试合计） |
| nested postfix 与 L0085 Span | `type_checking non_null_assertion_invalid_nested_postfix_keeps_only_valid_inner_fact` | 通过（8 项定向测试合计） |
| 条件 Copyable、Error/Deferred recovery | `type_checking non_null_assertion_preserves_conditional_copyability` / `non_null_assertion_error_and_deferred_operands_do_not_publish_extraction` | 通过（8 项定向测试合计） |
| 共享状态回归与下游 | `type_checking`、`type_copyability`；workspace check、frontend clippy | 类型套件通过 75 + 8 项；定向 clippy 通过，all-targets 被既有告警阻塞，workspace check 通过 |

2026-09-08：`cargo test -p lang-frontend --test type_checking non_null_assertion -- --nocapture`
8 passed / 0 failed，覆盖全部失败候选回滚后保留既有 assertion；独立只读审查及补测复核无新增问题。
`cargo fmt --all -- --check` 与 `git diff --check` 通过。共享契约与静态检查结果见下。

共享契约：`cargo test -p lang-frontend --test type_checking --test type_copyability --no-fail-fast`
通过，分别 75 / 8 项，0 failed、0 ignored。此证据覆盖新增 8 项及既有类型/条件 Copyable 回归；
未运行 frontend 全量套件。`cargo check --workspace --all-targets` 通过（11m 05s）。

静态检查：`cargo clippy -p lang-frontend --all-targets -- -D warnings` 失败，
既有 `tests/multifile_ownership_checking.rs:1006` 的 `filter_map_bool_then` 告警
（提交 `4c6d44c1`，本阶段未修改该文件）。缩小到本次实现和直接契约的
`cargo clippy -p lang-frontend --lib --test type_checking --test type_copyability -- -D warnings`
通过；不将定向结果称为 all-targets 通过。
