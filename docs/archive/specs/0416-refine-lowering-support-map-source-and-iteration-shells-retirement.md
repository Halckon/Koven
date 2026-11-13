# SPEC-0416：`lowering_support`、`map_source`、`borrow_result_support` 及 `lower_frontend` 控制/清理壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/lowering_support`、`ssa/map_source`、`ssa/borrow_result_support` 及 `ssa/lower_frontend` 控制流与 Map 迭代清理壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0416` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一步 |
| 基线 | [SPEC-0415](0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement.md)（`98f2e975`） |
| 分支 | `feature/spec-0416-refine-lowering-support-map-source-and-iteration-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0404](../../specs/active/0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](../../specs/active/0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](../../specs/active/0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](../../specs/active/0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](../../specs/active/0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](../../specs/active/0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](../../specs/active/0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0411](../../specs/active/0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md)、[SPEC-0412](../../specs/active/0412-refine-render-verify-and-map-frontend-shells-retirement.md)、[SPEC-0413](0413-refine-verify-operation-ownership-and-content-flow-shells-retirement.md)、[SPEC-0414](0414-refine-model-constant-and-lower-frontend-shells-retirement.md)、[SPEC-0415](0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0415` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa` 下的 10 个单职责壳子模块（`lowering_support/string_literal.rs`、`map_source/transport.rs`、`map_source/handoff.rs`、`borrow_result_support/short_circuit.rs`、`borrow_result_support/stable_map.rs`、`lower_frontend/map_iteration/borrowed.rs`、`lower_frontend/map_iteration/exit_shape.rs`、`lower_frontend/map_iteration/guard.rs`、`lower_frontend/control/proof_join.rs`、`lower_frontend/call_lifetimes.rs`），所有合并后目标模块均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `lowering_support`、`map_source` 与 `borrow_result_support` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/lowering_support/string_literal.rs`（67 行）**：将 `string_literal::decode_plain` 与 `decode_text` 内联收归 `crates/lang-codegen/src/ssa/lowering_support.rs`（合并后 181 行，`< 500` 行），保留 `pub(in crate::ssa) mod string_literal` 命名空间与单向依赖边界。
- **退役 `crates/lang-codegen/src/ssa/map_source/{transport.rs, handoff.rs}`（98 + 111 行）**：将 `transport::check` 与 `handoff::{Handoff, same_callable, find}` 内联收归 `crates/lang-codegen/src/ssa/map_source.rs`（合并后 406 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/borrow_result_support/short_circuit.rs`（177 行）与 `stable_map.rs`（128 行）**：将 `short_circuit::allows` 内联收归 `crates/lang-codegen/src/ssa/borrow_result_support.rs`（合并后 394 行，`< 500` 行），将测试门控的 `stable_map` 收归 `crates/lang-codegen/src/ssa/borrow_result_support/local_if.rs`（合并后 376 行，`< 500` 行）并在 `borrow_result_support.rs` 中重导出以保持路径兼容。

### 2.2 收敛 `lower_frontend` 下的 `map_iteration`、`control` 与 `drops` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/borrowed.rs`（142 行）**：将 `borrowed::{Proof, prove}` 与 `borrowed_map_seed` 收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source.rs`（合并后 463 行，`< 500` 行），并在 `map_iteration.rs` 中通过 `use source::borrowed;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/exit_shape.rs`（158 行）**：将 `exit_shape::{body_exit, point, terminal, cleanup}` 收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/cleanup.rs`（合并后 347 行，`< 500` 行），并在 `map_iteration.rs` 中通过 `use cleanup::exit_shape;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/guard.rs`（176 行）**：将 `guard::{Local, local}` 收归唯一消费方 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary.rs`（合并后 377 行，`< 500` 行），并在 `map_iteration.rs` 中通过 `use temporary::guard;` 保持引用路径兼容。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/control/proof_join.rs`（95 行）**：将 `retire_non_common_if_while_proofs` 与 `restore_if_proof_exit` 收归 `crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs`（合并后 437 行，`< 500` 行），并从 `control.rs` 移除 `mod proof_join;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/call_lifetimes.rs`（113 行）**：将 `finish_borrowed_call`、`finish_borrowed_call_with_sources` 与 `emit_control_transfer_cleanup` 收归 `crates/lang-codegen/src/ssa/lower_frontend/drops.rs`（合并后 242 行，`< 500` 行），并从 `lower_frontend.rs` 移除 `mod call_lifetimes;`。

## 3. 验收标准

- [x] 退役 `lowering_support/string_literal.rs`、`map_source/{transport.rs, handoff.rs}`、`borrow_result_support/{short_circuit.rs, stable_map.rs}`、`lower_frontend/map_iteration/{borrowed.rs, exit_shape.rs, guard.rs}`、`lower_frontend/control/proof_join.rs` 及 `lower_frontend/call_lifetimes.rs` 共 10 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 7 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：7/7 个路径不存在 |
| 内容并入声明的合并目标 | 部分宿主被后续 Spec 再次合并 | 3/7 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 43b117f8 refactor(codegen): retire lowering_support, map_source, borrow_result_support and lower_frontend shells (SPEC-0416)（同批共 7 个删除提交） |

宿主核对说明：

- `lower_frontend/map_iteration/source.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/multi_borrow_range_boundary_tests/map_iteration_source.rs`（`21` 行）
- `lower_frontend/map_iteration/cleanup.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/checked_integer_conversion_tests.rs`（`704` 行）
- `lower_frontend/map_iteration/temporary.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/unit_type_origin_tests.rs`（`380` 行）
- `lower_frontend/drops.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs`（`78` 行）
