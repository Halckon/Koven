# SPEC-0206：非空断言 Copy/Consume 所有权

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-206` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0028、0029 `done`；SPEC-0205 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` ownership/drop facts/tests、unit assertion typed descriptor；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 ownership 契约 |

## 2. Goal

完成后，ownership checker 消费 SPEC-0205 descriptor，为 `!!` 发布 Copyable copy 或 MoveOnly
whole-root consumption、non-null inner transfer、null Abort edge 与精确 drop facts。

## 3. 范围与需求

- Copyable inner 复制且 place 可继续使用；MoveOnly inner 只允许整体消费 owned root/temporary。
- MoveOnly Borrow/Inout、field、element extraction 分别使用 L0133、L0132、L0136；active loan
  使用 L0135。Copyable inner 从相同 source copy 合法；成功 MoveOnly continuation 上原 binding
  moved，后续使用 L0131。
- non-null edge凭 proof consume nullable并把 inner obligation交付结果；null edge不伪造 take/
  consume，直接进入 0205 的 diverging Abort effect，无正常 successor 或 unwind cleanup。
- drop/liveness 与 nested return/call/assignment 保持 ASAP 和一次求值。

## 4. 非目标

- 不生成 SSA/LLVM；不提供 place-preserving borrow unwrap、borrow-return 或 inline nullable ABI。

## 5. 验收标准

- [ ] Copyable place 保留、MoveOnly root/temporary move 与 use-after-move 矩阵通过。
- [ ] MoveOnly Borrow/Inout/field/element/active loan 与重复使用分别产生稳定
  L0133/L0132/L0136/L0135/L0131 与精确 Span/labels。
- [ ] null Abort、non-null transfer、nested control-flow/drop facts 无泄漏或双析构。
- [ ] validated ownership plan、determinism 与现有 ownership/drop suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

替换当前把 `NonNullAssert` operand 一律当 `Read` 的临时处理，复用 place、Value delivery、move、
loan 与 drop planner；frontend facts不引用 SSA operation。

## 7. 实施计划

1. [ ] 建立 assertion extraction/abort ownership facts → 验证：model 正反测试。
2. [ ] 接 checker、liveness、drop planner → 验证：move/loan/drop 矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | ownership facts、测试与完成文档 | `feat(frontend): own non-null extraction (SPEC-0206)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

按[分层验收](../../development/testing.md)复用有效证据。下表为本 Spec 的统一验收入口；
不因文档勾选重复运行 Cargo，不运行 frontend 全量测试。

| 验收项 | 测试目标 / 过滤器 | 实际结果 |
|---|---|---|
| Copyable 源保留；MoveOnly 来源、loan、Span、结果 drop | `ownership_checking non_null_assertion` | 最近运行 12 passed；包括循环正反例 |
| unit 来源矩阵、descriptor identity、Abort/transfer、错误清空和 nested/assignment drop | `multifile_ownership_checking unit_non_null_assertion` | 本轮 unit 全套 61 passed，含五项 assertion 测试 |
| 可达循环重复消费；break/return、每轮初始化和重新赋值 | 两端 `non_null_assertion` 循环测试 | single/unit 最小 while red 均实证；修复后两端全部正反例通过 |
| ownership、nullable、container、closure 共享契约 | `ownership_checking`、`multifile_ownership_checking`、`ownership_nullable_when`、`ownership_containers`、`ownership_closures` | 28 + 61 + 26 + 12 + 13 = 140 passed；0 failed/ignored/filtered |
| 多文件 typed descriptor 来源、顺序、overload trial rollback | `multifile_type_checking` | 100 passed / 3 既有 failed；新增 5 项通过，基线见下 |
| 公开 descriptor / ownership API 下游编译 | `cargo check --workspace --all-targets` | `a2909e9` 状态通过，9m03s；循环修复未改变公开 API |
| lint | frontend lib 与受影响定向 tests | lib 与上述五个 ownership suites 定向通过，仅允许既有 `filter_map_bool_then` |
| 格式与文档结构 | `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`git diff --check` | 通过；336 Markdown，结构检查不替代语义审查 |

### 实现和独立复核

- `6f8b7dc` 接入 Copy/Consume，修复独立审查发现的 Copyable 字段 shared-loan 误判。
- `3da360f` 发布单文件 `NonNullAssertionOwnershipPlan`；`446321c` 发布 source-qualified typed
  descriptor；`a2909e9` 发布 unit ownership plan。失败边固定 Abort，无 take 或 unwind cleanup；
  validated identity、trial/assignment rollback、全局诊断清空和稳定发布由对应测试覆盖。
- 完整复核发现循环回边重复消费缺少 L0131，单文件和 unit 最小 while 测试均取得有效 red。
  修复检查 next/continue 上 moved 与下一轮 live 的交集，排除 break/return；显式 loop 空 header
  避免每轮先补回的合法程序误报。已有 condition/body 错误时不追加回边级联诊断。
- unit 正例进一步发现 whole-root 赋值先检查旧值 available，导致移动后无法恢复。已仅对根
  Mutation 跳过旧值读取；仍检查 mutability/capture/loan，复合赋值先 Read，失败仍回滚。
- 上述循环及赋值修复均已独立静态复核，未发现新增阻塞；实际运行结论以表中结果为准。
  `for` 的 owned binder delivery 仍属既有类型 deferred 边界，本次未提前新增该语义。

### 基线失败与覆盖边界

`multifile_type_checking` 的三个失败在独立导出的 `3da360f` 基线同样出现（95 passed / 3 failed），
当前新增五项通过，失败集合未变：

- `deferred_explicit_constructor_type_arguments_publish_no_construction_fact`：MissingDeclarationSymbol。
- `cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`：既有期望多一个 L0112。
- `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary`：既有期望多一个 L0084。

严格 frontend all-targets clippy 曾被既有 `multifile_ownership_checking` 的
`filter_map_bool_then` 阻断；typed suite 的严格定向 clippy 曾被既有 `obfuscated_if_else` 阻断。
两者仅在各自定向命令中显式允许该 lint 后通过，不能记录为严格 all-targets 通过。

未运行 frontend 全量测试；Phase 4 SSA/LLVM/native 不属于本 Spec，由 SPEC-0207 承接。
本轮 140 项共享回归、定向 lint 和独立复核已结束；下一步逐项核对第 5 节并迁移完成状态。
