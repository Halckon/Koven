# SPEC-0447: `unit_lower_receiver_tests`、`sequential_for_lowering_tests`、`nullable_operation_tests`、`unit_enum_regression_tests` 及 `lowering_entry_contract_tests` 子壳模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0447 |
| 标题 | `unit_lower_receiver_tests`、`sequential_for_lowering_tests`、`nullable_operation_tests`、`unit_enum_regression_tests` 及 `lowering_entry_contract_tests` 子壳模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0446 |

---

## 1. 背景与动机

在 [SPEC-0446](../../archive/specs/0446-range-local-control-assertions-support-comparison-boundary-and-receiver-shells-retirement.md) 完成 `range_lowering_tests` 局部控制流与接收者测试壳模块收敛后，`crates/lang-codegen/src/ssa/` 的多个测试子目录下仍残留一批可直接内联回直接父模块的薄壳文件：

1. `ssa/unit_lower_receiver_tests/borrowing.rs`（430 行）：直接内联回 `ssa/unit_lower_receiver_tests.rs`。
2. `ssa/unit_lower_receiver_tests/delegation.rs`（256 行）：直接内联回 `ssa/unit_lower_receiver_tests.rs`。
3. `ssa/unit_lower_receiver_tests/representation.rs`（151 行）：直接内联回 `ssa/unit_lower_receiver_tests.rs`。
4. `ssa/sequential_for_lowering_tests/provider_lifetime_tests.rs`（127 行）：直接内联回 `ssa/sequential_for_lowering_tests.rs`。
5. `ssa/sequential_for_lowering_tests/unit_storage_boundary_tests.rs`（117 行）：直接内联回 `ssa/sequential_for_lowering_tests.rs`。
6. `ssa/nullable_operation_tests/capture_proof_tests.rs`（228 行）：直接内联回 `ssa/nullable_operation_tests.rs`。
7. `ssa/unit_enum_regression_tests/queries.rs`（372 行）：直接内联回 `ssa/unit_enum_regression_tests.rs`。
8. `ssa/lowering_entry_contract_tests/products.rs`（281 行）：直接内联回 `ssa/lowering_entry_contract_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/unit_lower_receiver_tests/borrowing.rs` | 430 | `ssa/unit_lower_receiver_tests.rs` | 464 |
| `ssa/unit_lower_receiver_tests/delegation.rs` | 256 | `ssa/unit_lower_receiver_tests.rs` | 720 |
| `ssa/unit_lower_receiver_tests/representation.rs` | 151 | `ssa/unit_lower_receiver_tests.rs` | 871 |
| `ssa/sequential_for_lowering_tests/provider_lifetime_tests.rs` | 127 | `ssa/sequential_for_lowering_tests.rs` | 680 |
| `ssa/sequential_for_lowering_tests/unit_storage_boundary_tests.rs` | 117 | `ssa/sequential_for_lowering_tests.rs` | 797 |
| `ssa/nullable_operation_tests/capture_proof_tests.rs` | 228 | `ssa/nullable_operation_tests.rs` | 824 |
| `ssa/unit_enum_regression_tests/queries.rs` | 372 | `ssa/unit_enum_regression_tests.rs` | 728 |
| `ssa/lowering_entry_contract_tests/products.rs` | 281 | `ssa/lowering_entry_contract_tests.rs` | 988 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2077` 降至 `2069`。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 通过 | 8/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- loan_region_ssa range_lowering receipt_ parent_child binding_lifecycle` = `390 passed / 0 failed`` |
