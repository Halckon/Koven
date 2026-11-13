# SPEC-0437: `lambda_preflight_tests`、`outer_borrowed_while` 守卫、`key_return` 身份及 `lower_frontend` 条件事实/所有权原语/局部迭代/可空载体/源闭包壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0437 |
| 标题 | `lambda_preflight_tests`、`outer_borrowed_while` 守卫、`key_return` 身份及 `lower_frontend` 条件事实/所有权原语/局部迭代/可空载体/源闭包壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0436 |

---

## 1. 背景与动机

在 [SPEC-0436](../../archive/specs/0436-unit-lower-borrow-orchestrate-field-verify-root-exchange-and-frontend-tests-shells-retirement.md) 完成 `unit_lower` 借用/编排/字段可空测试、`verify_ownership/root_exchange_tests` 及 `lower_frontend` 测试壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批测试专用与旧路径对比壳模块：

1. `ssa/unit_lower/type_plan/lambda_preflight_tests.rs`（736 行）：仅承载 `unit_lower/type_plan` 闭包预检单测，可收敛入 `ssa/unit_type_origin_tests.rs`。
2. `ssa/lower_frontend/outer_borrowed_while/runtime/guard_tests.rs`（914 行）：仅承载 `outer_borrowed_while/runtime` 守卫单测，可收敛入 `ssa/lower_frontend_map_continue_source_tests.rs`。
3. `ssa/lower_frontend_map_key_return_unit_identity.rs`（632 行）：仅承载 `lower_frontend_map_key_return_tests` 的 Unit 身份校验，可内联回父模块 `ssa/lower_frontend_map_key_return_tests.rs`。
4. `ssa/lower_frontend/map_iteration/conditional/facts.rs`（676 行）：仅承载测试对比用 `lower_frontend` 条件迭代事实查询，可收敛入 `ssa/borrow_result_boundary_tests.rs`。
5. `ssa/lower_frontend/ownership_primitive.rs`（685 行）：仅承载测试对比用 `lower_frontend` 所有权原语与空值比较降低，可收敛入 `ssa/integer_range_pending_tests.rs`。
6. `ssa/lower_frontend/map_iteration/local.rs`（686 行）：仅承载测试对比用 `lower_frontend` 局部 Map 迭代证明，可收敛入 `ssa/mixed_borrow_boundary_tests.rs`。
7. `ssa/lower_frontend/nullable_numeric_cleanup/carrier.rs`（691 行）：仅承载测试对比用 `nullable_numeric_cleanup` 载体证明，可收敛入 `ssa/bitwise_operation_tests.rs`。
8. `ssa/lower_frontend/source_closure.rs`（714 行）：仅承载测试对比用 `lower_frontend` 源闭包布局与降低，可收敛入 `ssa/owned_map_borrow_boundary_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/unit_lower/type_plan/lambda_preflight_tests.rs` | 736 | `ssa/unit_type_origin_tests.rs` | 956 |
| `ssa/lower_frontend/outer_borrowed_while/runtime/guard_tests.rs` | 914 | `ssa/lower_frontend_map_continue_source_tests.rs` | 951 |
| `ssa/lower_frontend_map_key_return_unit_identity.rs` | 632 | `ssa/lower_frontend_map_key_return_tests.rs` | 874 |
| `ssa/lower_frontend/map_iteration/conditional/facts.rs` | 676 | `ssa/borrow_result_boundary_tests.rs` | 826 |
| `ssa/lower_frontend/ownership_primitive.rs` | 685 | `ssa/integer_range_pending_tests.rs` | 843 |
| `ssa/lower_frontend/map_iteration/local.rs` | 686 | `ssa/mixed_borrow_boundary_tests.rs` | 848 |
| `ssa/lower_frontend/nullable_numeric_cleanup/carrier.rs` | 691 | `ssa/bitwise_operation_tests.rs` | 872 |
| `ssa/lower_frontend/source_closure.rs` | 714 | `ssa/owned_map_borrow_boundary_tests.rs` | 898 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2157` 降至 `2149`。
