# SPEC-0440: `rc_nullable_while_identity_queries`、`same_guard`、`map_loop/operand_unit_identity`、`owner_consumer_test_support` 及 `loop_control`/`instances` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0440 |
| 标题 | `rc_nullable_while_identity_queries`、`same_guard`、`map_loop/operand_unit_identity`、`owner_consumer_test_support` 及 `loop_control`/`instances` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0439 |

---

## 1. 背景与动机

在 [SPEC-0439](../../archive/specs/0439-numeric-iteration-map-consume-map-iteration-resource-verify-source-and-presence-identity-shells-retirement.md) 完成 `numeric_iteration`、`map_consume`、`map_iteration` 根/资源/校验/源及 `rc_nullable_while_presence_identity` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批测试支撑与测试对比用壳子模块：

1. `ssa/rc_nullable_while_identity_queries.rs`（609 行）：仅承载可空 `Rc` `while` 身份查询测试支撑，可收敛入 `ssa/lower_frontend_map_return_tests.rs`。
2. `ssa/lower_frontend_map_same_guard_ssa_test_support.rs`（626 行）：仅承载同守卫 SSA 测试断言支撑，可收敛入 `ssa/lower_frontend_map_continue_tests.rs`。
3. `ssa/lower_frontend_map_loop_unit_identity.rs`（713 行）：仅承载 `lower_frontend_map_loop_tests` 的 Unit 身份校验，可内联回父模块 `ssa/lower_frontend_map_loop_tests.rs`。
4. `ssa/lower_frontend/loop_control.rs`（728 行）：仅承载测试对比用旧前端循环控制入口，可收敛入 `ssa/lower_frontend_map_break_tests.rs`。
5. `ssa/lower_frontend_map_operand_unit_identity.rs`（737 行）：仅承载操作数最后使用 Unit 身份校验，可收敛入 `ssa/constant_lowering_tests.rs`。
6. `ssa/refine/build/owner_consumer_test_support.rs`（740 行）：仅承载细化层 owner consumer 测试故障注入与检查点，可收敛入 `ssa/boxed_signature_storage_tests.rs`。
7. `ssa/lower_frontend_map_same_guard_unit_test_support.rs`（869 行）：仅承载同守卫 Unit 测试支撑，可收敛入 `ssa/unit_receiver_two_phase_control_tests.rs`。
8. `ssa/lower_frontend/instances.rs`（910 行）：仅承载测试对比用旧前端泛型实例与可调用规划，可收敛入 `ssa/multi_temporary_loan_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/rc_nullable_while_identity_queries.rs` | 609 | `ssa/lower_frontend_map_return_tests.rs` | 726 |
| `ssa/lower_frontend_map_same_guard_ssa_test_support.rs` | 626 | `ssa/lower_frontend_map_continue_tests.rs` | 774 |
| `ssa/lower_frontend_map_loop_unit_identity.rs` | 713 | `ssa/lower_frontend_map_loop_tests.rs` | 955 |
| `ssa/lower_frontend/loop_control.rs` | 728 | `ssa/lower_frontend_map_break_tests.rs` | 924 |
| `ssa/lower_frontend_map_operand_unit_identity.rs` | 737 | `ssa/constant_lowering_tests.rs` | 879 |
| `ssa/refine/build/owner_consumer_test_support.rs` | 740 | `ssa/boxed_signature_storage_tests.rs` | 925 |
| `ssa/lower_frontend_map_same_guard_unit_test_support.rs` | 869 | `ssa/unit_receiver_two_phase_control_tests.rs` | 943 |
| `ssa/lower_frontend/instances.rs` | 910 | `ssa/multi_temporary_loan_tests.rs` | 984 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2133` 降至 `2125`。
