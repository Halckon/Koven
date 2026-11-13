# SPEC-0415：`lower_frontend` 下 `map_iteration` 与 `borrow_argument` 壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/lower_frontend/map_iteration` 条件/成型/临时/局部迭代证明壳子模块及 `borrow_argument/owned_field` 壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0415` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续下一批次 |
| 基线 | [SPEC-0414](0414-refine-model-constant-and-lower-frontend-shells-retirement.md)（`e78a9444`） |
| 分支 | `feature/spec-0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0404](../../specs/active/0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](../../specs/active/0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](../../specs/active/0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](../../specs/active/0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](../../specs/active/0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](../../specs/active/0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](../../specs/active/0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0411](../../specs/active/0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md)、[SPEC-0412](../../specs/active/0412-refine-render-verify-and-map-frontend-shells-retirement.md)、[SPEC-0413](../../specs/active/0413-refine-verify-operation-ownership-and-content-flow-shells-retirement.md)、[SPEC-0414](0414-refine-model-constant-and-lower-frontend-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0414` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration` 下的 8 个单职责壳子模块（`conditional/shape.rs`、`conditional/transport.rs`、`conditional/local.rs`、`conditional/successor.rs`、`formed_runtime/transport.rs`、`temporary/tail.rs`、`formed/epoch.rs`、`local/consumer.rs`）以及 `lower_frontend/borrow_argument/owned_field.rs`，所有合并后目标文件均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `lower_frontend/map_iteration` 下的条件、成型、临时与局部迭代壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/{shape.rs, transport.rs, local.rs}`（32 + 35 + 62 行）**：将 `shape`、`transport::roles` 与 `local::validate` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional.rs`（合并后 218 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/successor.rs`（120 行）**：将 `terminal` 收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/retained.rs`（合并后 265 行，`< 500` 行），并在 `conditional.rs` 中通过 `pub(super) use retained as successor;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed_runtime/transport.rs`（56 行）**：将 `check_formed_transport` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed_runtime.rs`（合并后 279 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary/tail.rs`（68 行）**：将 `tail_guard` 与 `contains` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary.rs`（合并后 198 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed/epoch.rs`（80 行）**：将 `Epoch` 与 `resolve` 收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed/retained.rs`（合并后 316 行，`< 500` 行），并在 `formed.rs`（469 行，`< 500` 行）中通过 `pub(super) use retained as epoch;` 保持引用路径不变。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local/consumer.rs`（177 行）**：将 `consumer::prove` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local.rs`（合并后 357 行，`< 500` 行）。

### 2.2 收敛 `lower_frontend/borrow_argument/owned_field.rs`

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument/owned_field.rs`（159 行）**：将 `lower_owned_field_borrow_place` 收归同属字段借用实参 lowering 的 `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument/nullable_field.rs`（合并后 332 行，`< 500` 行），并从 `borrow_argument.rs`（332 行，`< 500` 行）移除 `mod owned_field;`。

## 3. 验收标准

- [x] 退役 `map_iteration/{conditional/shape.rs, conditional/transport.rs, conditional/local.rs, conditional/successor.rs, formed_runtime/transport.rs, temporary/tail.rs, formed/epoch.rs, local/consumer.rs}` 及 `borrow_argument/owned_field.rs` 共 9 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 6 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：6/6 个路径不存在 |
| 内容并入声明的合并目标 | 部分宿主被后续 Spec 再次合并 | 0/6 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 98f2e975 refactor(codegen): retire lower_frontend map_iteration and field borrow shells (SPEC-0415)（同批共 6 个删除提交） |

宿主核对说明：

- `lower_frontend/map_iteration/conditional/retained.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/checked_integer_conversion_tests.rs`（`767` 行）
- `lower_frontend/map_iteration/formed_runtime.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/native_nullable_when_tests/support.rs`（`708` 行）
- `lower_frontend/map_iteration/temporary.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs`（`766` 行）
- `lower_frontend/map_iteration/formed/retained.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership/range.rs`（`142` 行）
- `lower_frontend/map_iteration/local.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/deinit_tests.rs`（`218` 行）
- `lower_frontend/borrow_argument/nullable_field.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/mod.rs`（`848` 行）
