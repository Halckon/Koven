# SPEC-0444: `loan_region_ssa_tests` 下观察器、接收者、具名借用、临时借用兄弟及有限字面量测试壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0444 |
| 标题 | `loan_region_ssa_tests` 下观察器、接收者、具名借用、临时借用兄弟及有限字面量测试壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0443 |

---

## 1. 背景与动机

在 [SPEC-0443](0443-owned-source-return-range-short-circuit-local-arguments-nested-and-temporary-exits-shells-retirement.md) 完成 `owned_source_return`、`range_lowering_tests`、`local_arguments`、`nested` 及 `temporary_exits` 子壳模块收敛后，`crates/lang-codegen/src/ssa/loan_region_ssa_tests.rs` 下仍挂载一批 90–150 行的薄壳测试文件：

1. `ssa/loan_observer_off_tests.rs`（109 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
2. `ssa/receiver_route_tests.rs`（136 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
3. `ssa/receiver_receipt_tests.rs`（137 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
4. `ssa/named_loan_route_tests.rs`（117 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
5. `ssa/named_loan_receipt_tests.rs`（147 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
6. `ssa/named_loan_sibling_tests.rs`（113 行）：收敛入同域宿主 `ssa/loan_receipt_tests.rs`。
7. `ssa/temporary_loan_sibling_tests.rs`（130 行）：直接内联回父模块 `ssa/loan_region_ssa_tests.rs`。
8. `ssa/finite_literal_tests.rs`（94 行）：直接内联回父模块 `ssa/loan_region_ssa_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/loan_observer_off_tests.rs` | 109 | `ssa/loan_receipt_tests.rs` | 266 |
| `ssa/receiver_route_tests.rs` | 136 | `ssa/loan_receipt_tests.rs` | 405 |
| `ssa/receiver_receipt_tests.rs` | 137 | `ssa/loan_receipt_tests.rs` | 545 |
| `ssa/named_loan_route_tests.rs` | 117 | `ssa/loan_receipt_tests.rs` | 665 |
| `ssa/named_loan_receipt_tests.rs` | 147 | `ssa/loan_receipt_tests.rs` | 815 |
| `ssa/named_loan_sibling_tests.rs` | 113 | `ssa/loan_receipt_tests.rs` | 931 |
| `ssa/temporary_loan_sibling_tests.rs` | 130 | `ssa/loan_region_ssa_tests.rs` | 886 |
| `ssa/finite_literal_tests.rs` | 94 | `ssa/loan_region_ssa_tests.rs` | 980 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2101` 降至 `2093`。
