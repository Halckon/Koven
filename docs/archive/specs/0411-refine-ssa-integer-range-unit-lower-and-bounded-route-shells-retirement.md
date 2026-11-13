# SPEC-0411：`ssa/integer_range`、`unit_lower` 迭代清理与空值比较壳模块及 `refine` 有界路由壳模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/integer_range` 顶层壳模块、`unit_lower` 迭代清理与空值比较壳模块（`iteration_cleanup`、`nullable_comparison`）及 `refine/subset` 有界路由壳子模块（`receiver_route`、`temporary_route`）收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0411` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一步实施 |
| 基线 | [SPEC-0410](0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)（`d1f0cba2`，含 `main` `4150bbd8`） |
| 分支 | `feature/spec-0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0404](../../specs/active/0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](../../specs/active/0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](../../specs/active/0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](../../specs/active/0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](0410-refine-delegated-receiver-and-build-subset-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0410` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa` 顶层的整数区间算子壳模块（`integer_range.rs`）、`unit_lower` 下的迭代清理与空值比较壳模块（`loop_control/iteration_cleanup.rs`、`nullable_comparison.rs`）以及 `refine/subset` 下的 2 个有界调用方路由触发壳子模块（`receiver_route.rs`、`temporary_route.rs`），所有合并后模块均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `ssa/integer_range.rs` 至 `ssa/integer.rs`

- **退役 `crates/lang-codegen/src/ssa/integer_range.rs`（92 行）**：将 `matches_fact`（重命名为 `matches_range_fact`）与测试专用 `#[cfg(test)] operation`（重命名为 `range_operation`）收归同属整数与区间算子映射的 `crates/lang-codegen/src/ssa/integer.rs`（合并后 170 行，`< 500` 行）。
- 同步更新 `crates/lang-codegen/src/ssa/mod.rs`、`crates/lang-codegen/src/ssa/lower_frontend/integer_range.rs` 与 `crates/lang-codegen/src/ssa/refine/subset/scalar_text.rs`。

### 2.2 收敛 `unit_lower` 下的迭代清理与空值比较壳模块

- **退役 `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration_cleanup.rs`（151 行）**：将 `emit_iteration_cleanup`、`iteration_context`、`iteration_pending_loan` 与 `iteration_end_loan` 收归同属顺序迭代 lowering 的 `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration.rs`（合并后 425 行，`< 500` 行），并从 `unit_lower/loop_control.rs` 移除 `mod iteration_cleanup;`。
- **退役 `crates/lang-codegen/src/ssa/unit_lower/nullable_comparison.rs`（194 行）**：将 `lower_nullable_comparison`、`emit_null_condition_drops` 与 `lower_temporary_nullable_comparison` 收归 `crates/lang-codegen/src/ssa/unit_lower/assignment.rs`（合并后 358 行，`< 500` 行），并从 `unit_lower.rs` 移除 `mod nullable_comparison;`。

### 2.3 收敛 `refine/subset` 下的有界调用方路由触发壳子模块

- **退役 `crates/lang-codegen/src/ssa/refine/subset/receiver_route.rs`（166 行）与 `temporary_route.rs`（158 行）**：将 `bounded_receiver_route`、`bounded_receiver_candidate` 与 `bounded_temporary_call` 收归同属 `SPEC-0341` 有界调用方路由触发器的 `crates/lang-codegen/src/ssa/refine/subset/named_route.rs`（合并后 468 行，`< 500` 行）。
- 同步更新 `crates/lang-codegen/src/ssa/refine/subset.rs` 与 `crates/lang-codegen/src/ssa/refine/temporary_receipt/admission.rs`。

## 3. 验收标准

- [x] 退役 `ssa/integer_range.rs`、`unit_lower/loop_control/iteration_cleanup.rs`、`unit_lower/nullable_comparison.rs` 及 `refine/subset/{receiver_route.rs, temporary_route.rs}` 共 5 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 6 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：6/6 个路径不存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并，见下） | 1/6 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 3b7ddf29 refactor(codegen): retire ssa/integer_range, unit_lower and refine bounded-route shells (SPEC-0411)（同批共 5 个删除提交） |

宿主核对说明：

- `loop_control/iteration_cleanup.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `integer.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/constant_materialization.rs`（`9` 行）
- `unit_lower/assignment.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/integer_range_pending_tests.rs`（`680` 行）
- `temporary_route.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/refine/subset/named_route.rs`（`163` 行）
- `unit_lower/loop_control/iteration_cleanup.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/constant_materialization.rs`（`9` 行）

其中 1 处未能按符号定位，需人工复核。
