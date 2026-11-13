# SPEC-0413：`verify_operation`、`verify_ownership` 及 `verify/content_flow` 壳模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/verify_operation` 算子与迭代契约壳模块、`ssa/verify_ownership` 提供者入口投影与区间根源壳模块及 `ssa/verify/content_flow` 壳模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0413` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一步实施 |
| 基线 | [SPEC-0412](0412-refine-render-verify-and-map-frontend-shells-retirement.md)（`4d478f58`） |
| 分支 | `feature/spec-0413-refine-verify-operation-ownership-and-content-flow-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0411](0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md)、[SPEC-0412](0412-refine-render-verify-and-map-frontend-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0412` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa/verify_operation` 下的 5 个单职责契约壳模块（`constant_contract.rs`、`text.rs`、`borrowed_generate.rs`、`integer_range.rs`、`map/iteration.rs`）、`ssa/verify_ownership` 下的 2 个辅助壳模块（`provider_entry_projection.rs`、`range/source.rs`）以及 `ssa/verify/content_flow.rs`，所有合并后模块均严格控制在 `< 500` 行内且遵守 IR 方向性依赖边界。

## 2. 范围与设计

### 2.1 收敛 `ssa/verify_operation` 下的契约壳模块

- **退役 `crates/lang-codegen/src/ssa/verify_operation/{constant_contract.rs, text.rs, borrowed_generate.rs, integer_range.rs}`（22 + 27 + 48 + 56 行）**：将 `constant_contract`、`shared_byte_array`、`borrowed_generate_contract` 以及 `range_construct`、`range_contains`、`range_equal` 统一收归 `crates/lang-codegen/src/ssa/verify_operation/integer.rs`（合并后 163 行，`< 500` 行），并从 `verify_operation.rs` 移除上述 4 个子模块声明。
- **退役 `crates/lang-codegen/src/ssa/verify_operation/map/iteration.rs`（80 行）**：将 `verify_map_iteration`、`iteration_source_types`、`is_cursor` 与 `cursor_type` 内联收归 `crates/lang-codegen/src/ssa/verify_operation/map.rs`（合并后 281 行，`< 500` 行），并移除 `mod iteration;`。

### 2.2 收敛 `ssa/verify_ownership` 与 `ssa/verify` 下的辅助壳模块

- **退役 `crates/lang-codegen/src/ssa/verify_ownership/provider_entry_projection.rs`（74 行）**：将 `EntryProjection` 内联入唯一调用方 `crates/lang-codegen/src/ssa/verify_ownership/provider_lifetime.rs`（合并后 354 行，`< 500` 行），保留 `#[cfg(test)] #[path = "provider_entry_projection_tests.rs"] mod projection_tests;` 测试挂载。
- **退役 `crates/lang-codegen/src/ssa/verify_ownership/range/source.rs`（102 行）**：将 `is_view`、`root`、`capability` 与 `resolve` 收归 `crates/lang-codegen/src/ssa/verify_ownership/range/pairs.rs`（合并后 205 行，`< 500` 行），并在 `verify_ownership/range.rs` 中通过 `pub(super) use pairs as source;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/verify/content_flow.rs`（72 行）**：将 `block_entries` 收归 `crates/lang-codegen/src/ssa/verify/module.rs`（合并后 184 行，`< 500` 行），并在 `verify.rs` 中通过 `pub(in crate::ssa::verify) use module as content_flow;` 保持内容流证明调用路径不变。

## 3. 验收标准

- [x] 退役 `verify_operation/{constant_contract.rs, text.rs, borrowed_generate.rs, integer_range.rs, map/iteration.rs}`、`verify_ownership/{provider_entry_projection.rs, range/source.rs}` 及 `verify/content_flow.rs` 共 8 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 14 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：14/14 个路径不存在 |
| 内容并入声明的合并目标 | 部分未定位 | 0/14 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 0d3a5635 refactor(codegen): retire verify_operation, verify_ownership and content_flow shells (SPEC-0413)（同批共 8 个删除提交） |

宿主核对说明：

- `text.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `verify_operation/integer.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership/root_exchange.rs`（`465` 行）
- `verify_operation/map.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership/root_exchange.rs`（`836` 行）
- `verify_ownership/provider_lifetime.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/native_ordered_map_tests/runner.rs`（`25` 行）
- `verify_ownership/range/pairs.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership/range.rs`（`110` 行）
- `verify/module.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_types.rs`（`700` 行）
- `verify_ownership/provider_entry_projection.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核

其中 2 处未能按符号定位，需人工复核。
