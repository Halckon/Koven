# SPEC-0446: `range_lowering_tests` 下局部控制流、断言、支撑、比较边界及接收者测试壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0446 |
| 标题 | `range_lowering_tests` 下局部控制流、断言、支撑、比较边界及接收者测试壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0445 |

---

## 1. 背景与动机

在 [SPEC-0445](../../archive/specs/0445-temporary-loan-binding-parent-child-products-and-range-view-source-tests-shells-retirement.md) 完成 `loan_region_ssa_tests` 及 `range_lowering_tests/view_source` 收敛后，`crates/lang-codegen/src/ssa/range_lowering_tests/` 下仍残留一批局部控制流、比较边界与接收者薄壳测试模块：

1. `ssa/range_lowering_tests/local_control_ownership.rs`（264 行）：直接内联回 `ssa/range_lowering_tests/local_control.rs`。
2. `ssa/range_lowering_tests/local_control_boundary.rs`（247 行）：直接内联回 `ssa/range_lowering_tests/local_control.rs`。
3. `ssa/range_lowering_tests/local_control_selection.rs`（217 行）：直接内联回 `ssa/range_lowering_tests/local_control.rs`。
4. `ssa/range_lowering_tests/local_comparison_boundary.rs`（88 行）：收敛入同目录宿主 `ssa/range_lowering_tests/local_short_circuit.rs`。
5. `ssa/range_lowering_tests/local_control_assertions.rs`（387 行）：收敛入同目录宿主 `ssa/range_lowering_tests/scalar_counts.rs`。
6. `ssa/range_lowering_tests/local_control_support.rs`（601 行）：收敛入同目录宿主 `ssa/range_lowering_tests/local_short_circuit_boundary.rs`。
7. `ssa/range_lowering_tests/receiver/count_break.rs`（533 行）：收敛入宿主 `ssa/lower_frontend_map_return_controls_tests.rs`。
8. `ssa/range_lowering_tests/receiver.rs`（664 行）：收敛入宿主 `ssa/unit_receiver_two_phase_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/range_lowering_tests/local_control_ownership.rs` | 264 | `ssa/range_lowering_tests/local_control.rs` | 493 |
| `ssa/range_lowering_tests/local_control_boundary.rs` | 247 | `ssa/range_lowering_tests/local_control.rs` | 740 |
| `ssa/range_lowering_tests/local_control_selection.rs` | 217 | `ssa/range_lowering_tests/local_control.rs` | 957 |
| `ssa/range_lowering_tests/local_comparison_boundary.rs` | 88 | `ssa/range_lowering_tests/local_short_circuit.rs` | 674 |
| `ssa/range_lowering_tests/local_control_assertions.rs` | 387 | `ssa/range_lowering_tests/scalar_counts.rs` | 852 |
| `ssa/range_lowering_tests/local_control_support.rs` | 601 | `ssa/range_lowering_tests/local_short_circuit_boundary.rs` | 871 |
| `ssa/range_lowering_tests/receiver/count_break.rs` | 533 | `ssa/lower_frontend_map_return_controls_tests.rs` | 778 |
| `ssa/range_lowering_tests/receiver.rs` | 664 | `ssa/unit_receiver_two_phase_tests.rs` | 905 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2085` 降至 `2077`。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 通过 | 8/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- loan_region_ssa range_lowering receipt_ parent_child binding_lifecycle` = `390 passed / 0 failed`` |
