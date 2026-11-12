# SPEC-0445: `loan_region_ssa_tests` 下临时借用、绑定凭证、父子探针、多临时乘积及 `range_lowering_tests/view_source` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0445 |
| 标题 | `loan_region_ssa_tests` 下临时借用、绑定凭证、父子探针、多临时乘积及 `range_lowering_tests/view_source` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0444 |

---

## 1. 背景与动机

在 [SPEC-0444](0444-loan-observer-receiver-named-temporary-and-finite-literal-tests-shells-retirement.md) 完成 `loan_region_ssa_tests` 首批 8 个薄壳文件收敛后，同域及 `range_lowering_tests` 下仍残留一批 138–338 行的薄壳测试文件：

1. `ssa/temporary_loan_route_tests.rs`（138 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
2. `ssa/temporary_loan_receipt_tests.rs`（190 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
3. `ssa/binding_causal_links_tests.rs`（243 行）：收敛入同域宿主 `ssa/parent_child_coverage_tests.rs`。
4. `ssa/binding_coverage_tests.rs`（213 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
5. `ssa/binding_receipt_tests.rs`（256 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
6. `ssa/parent_child_lifecycle_probe_tests.rs`（176 行）：收敛入同域宿主 `ssa/parent_child_causal_tests.rs`。
7. `ssa/multi_temporary_loan_products_tests.rs`（338 行）：收敛入同域宿主 `ssa/parent_child_drop_dependency_tests.rs`。
8. `ssa/range_lowering_tests/view_source.rs`（298 行）：收敛入宿主 `ssa/parent_child_drop_dependency_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/temporary_loan_route_tests.rs` | 138 | `ssa/parent_child_coverage_tests.rs` | 475 |
| `ssa/temporary_loan_receipt_tests.rs` | 190 | `ssa/parent_child_coverage_tests.rs` | 668 |
| `ssa/binding_causal_links_tests.rs` | 243 | `ssa/parent_child_coverage_tests.rs` | 914 |
| `ssa/binding_coverage_tests.rs` | 213 | `ssa/parent_child_causal_tests.rs` | 531 |
| `ssa/binding_receipt_tests.rs` | 256 | `ssa/parent_child_causal_tests.rs` | 790 |
| `ssa/parent_child_lifecycle_probe_tests.rs` | 176 | `ssa/parent_child_causal_tests.rs` | 969 |
| `ssa/multi_temporary_loan_products_tests.rs` | 338 | `ssa/parent_child_drop_dependency_tests.rs` | 590 |
| `ssa/range_lowering_tests/view_source.rs` | 298 | `ssa/parent_child_drop_dependency_tests.rs` | 891 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2093` 降至 `2085`。
