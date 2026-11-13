# SPEC-0419：`lower_frontend` 绑定、非空断言、Map 结果/资源析构与 `map_iteration` 壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/lower_frontend` 局部绑定、非空断言、Copyable Map 结果、资源析构、常量存在性证明及 `map_iteration` 嵌套/后续使用/局部条件壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0419` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续一次推进多批 |
| 基线 | [SPEC-0418](0418-refine-receipt-subset-and-frontend-shells-retirement.md)（`8977b378`） |
| 分支 | `feature/spec-0419-refine-lower-frontend-and-map-iteration-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0414](0414-refine-model-constant-and-lower-frontend-shells-retirement.md)、[SPEC-0415](0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement.md)、[SPEC-0416](0416-refine-lowering-support-map-source-and-iteration-shells-retirement.md)、[SPEC-0417](0417-refine-verify-render-and-build-shells-retirement.md)、[SPEC-0418](0418-refine-receipt-subset-and-frontend-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0418` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa/lower_frontend` 下的 8 个单职责壳子模块（`binding.rs`、`non_null_assertion.rs`、`map_result.rs`、`resource_deinit.rs`、`constant_presence.rs`、`map_iteration/nested.rs`、`map_iteration/source_owner/future.rs`、`map_iteration/local/conditions.rs`），所有合并后目标模块均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `lower_frontend` 顶层绑定、断言、Map 结果、资源析构与存在性证明壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/binding.rs`（166 行）**：将局部绑定与赋值右值交付辅助内联收归 `crates/lang-codegen/src/ssa/lower_frontend/declarations.rs`（合并后 `< 500` 行），并从 `lower_frontend.rs` 移除 `mod binding;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/non_null_assertion.rs`（175 行）**：将 `lower_non_null_assertion` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/nullable_comparison.rs`（合并后 `< 500` 行），并从 `lower_frontend.rs` 移除 `mod non_null_assertion;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_result.rs`（155 行）**：将 Copyable Map 查询结果包裹、解包与比较辅助内联收归 `crates/lang-codegen/src/ssa/lower_frontend/string_clone.rs`（合并后 `< 500` 行），并从 `lower_frontend.rs` 移除 `mod map_result;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/resource_deinit.rs`（190 行）**：将 `resource_deinit::{DeinitPlan, declare, validate_type, validate_map_value_type}` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/aggregate.rs`（合并后 `< 500` 行），并在 `lower_frontend.rs` 中通过 `pub(super) use aggregate::resource_deinit;` 保持引用路径透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/constant_presence.rs`（214 行）**：将 `constant_presence::{constant_drop, prove_graph}` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup.rs`（合并后 `< 500` 行），并在 `lower_frontend.rs` 中通过 `pub(super) use nullable_if_cleanup::constant_presence;` 保持引用路径透明。

### 2.2 收敛 `lower_frontend/map_iteration` 下的嵌套、后续使用与局部条件壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/nested.rs`（213 行）**：将 `nested::{Nesting, nesting, incoming, transport}` 及父级校验方法内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/checked.rs`（合并后 `< 500` 行），并在 `map_iteration.rs` 中通过 `use checked::nested;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/future.rs`（112 行）**：将 `future::qualify` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/delivery.rs`（合并后 `< 500` 行），并在 `source_owner.rs` 中通过 `use delivery::future;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local/conditions.rs`（210 行）**：将 `conditions::{Expected, path, check}` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local_runtime.rs`（合并后 `< 500` 行），并在 `local.rs` 中通过 `use super::local_runtime::conditions;` 保持内部引用透明。

## 3. 验收标准

- [x] 退役 `lower_frontend/{binding.rs, non_null_assertion.rs, map_result.rs, resource_deinit.rs, constant_presence.rs}` 及 `lower_frontend/map_iteration/{nested.rs, source_owner/future.rs, local/conditions.rs}` 共 8 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 部分宿主被后续 Spec 再次合并 | 1/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 2e003c05 refactor(codegen): retire lower_frontend binding, assertion, map_result, deinit and map_iteration shells (SPEC-0419)（同批共 8 个删除提交） |

宿主核对说明：

- `lower_frontend/declarations.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/unit_lower.rs`（`703` 行）
- `lower_frontend/nullable_comparison.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/integer_range_pending_tests.rs`（`517` 行）
- `lower_frontend/string_clone.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/map_unit_consumer_test_support.rs`（`278` 行）
- `lower_frontend/aggregate.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/owned_map_borrow_boundary_tests.rs`（`221` 行）
- `lower_frontend/map_iteration/checked.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup.rs`（`518` 行）
- `lower_frontend/map_iteration/source_owner/delivery.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/map_promotion_contract_tests.rs`（`529` 行）
- `lower_frontend/map_iteration/local_runtime.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/native.rs`（`478` 行）
