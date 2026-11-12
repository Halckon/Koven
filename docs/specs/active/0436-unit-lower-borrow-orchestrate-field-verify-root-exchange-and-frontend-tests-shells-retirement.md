# SPEC-0436: `unit_lower` 借用/编排/字段可空测试、`verify_ownership/root_exchange_tests` 及 `lower_frontend` 实例/闭包/待定调用/借用清理测试壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0436 |
| 标题 | `unit_lower` 借用/编排/字段可空测试、`verify_ownership/root_exchange_tests` 及 `lower_frontend` 实例/闭包/待定调用/借用清理测试壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0435 |

---

## 1. 背景与动机

在 [SPEC-0435](0435-unit-lower-result-tests-lower-frontend-guard-and-instances-tests-shells-retirement.md) 完成 `unit_lower/control/result_tests`、`item_operand_ssa_test_support` 及 `lower_frontend` 守卫与实例测试壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批仅向父模块暴露私有测试套件的薄壳文件：

1. `ssa/unit_lower/borrow/nullable_temporary_tests.rs`（474 行）：仅承载 `unit_lower/borrow` 可空临时借用单测，可收敛入 `ssa/unit_lower_borrow_tests.rs`。
2. `ssa/unit_lower/orchestrate/registration_tests.rs`（536 行）：仅承载 `unit_lower/orchestrate` 实例注册契约单测，可收敛入 `ssa/unit_lower_type_plan_tests.rs`。
3. `ssa/lower_frontend/instances/callable_plan_tests.rs`（539 行）：仅承载 `lower_frontend/instances` 可调用规划身份单测与 Fixture，可收敛入 `ssa/unit_llvm_tests.rs`。
4. `ssa/lower_frontend/source_closure/tests.rs`（560 行）：仅承载 `lower_frontend/source_closure` 源闭包单测，可收敛入 `ssa/refine_closure_tests.rs`。
5. `ssa/lower_frontend/pending_call/guard_tests.rs`（568 行）：仅承载 `pending_call` 清理守卫单测，可收敛入 `ssa/call_lifetimes_tests.rs`。
6. `ssa/lower_frontend/nullable_borrowed_cleanup/runtime/tests.rs`（608 行）：仅承载 `nullable_borrowed_cleanup/runtime` 守卫单测，可收敛入 `ssa/owned_null_cleanup_tests.rs`。
7. `ssa/unit_lower/control/conditional/nullable_if/field_tests.rs`（636 行）：仅承载 `nullable_if` 字段证明单测，可收敛入 `ssa/unit_lower_nullable_neighbor_tests.rs`。
8. `ssa/verify_ownership/root_exchange_tests.rs`（652 行）：仅承载 `verify_ownership` 根交换校验单测，可收敛入 `ssa/verify_loan_mutate_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/unit_lower/borrow/nullable_temporary_tests.rs` | 474 | `ssa/unit_lower_borrow_tests.rs` | 713 |
| `ssa/unit_lower/orchestrate/registration_tests.rs` | 536 | `ssa/unit_lower_type_plan_tests.rs` | 713 |
| `ssa/lower_frontend/instances/callable_plan_tests.rs` | 539 | `ssa/unit_llvm_tests.rs` | 685 |
| `ssa/lower_frontend/source_closure/tests.rs` | 560 | `ssa/refine_closure_tests.rs` | 795 |
| `ssa/lower_frontend/pending_call/guard_tests.rs` | 568 | `ssa/call_lifetimes_tests.rs` | 650 |
| `ssa/lower_frontend/nullable_borrowed_cleanup/runtime/tests.rs` | 608 | `ssa/owned_null_cleanup_tests.rs` | 733 |
| `ssa/unit_lower/control/conditional/nullable_if/field_tests.rs` | 636 | `ssa/unit_lower_nullable_neighbor_tests.rs` | 743 |
| `ssa/verify_ownership/root_exchange_tests.rs` | 652 | `ssa/verify_loan_mutate_tests.rs` | 749 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2165` 降至 `2157`。
