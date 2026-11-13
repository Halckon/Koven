# SPEC-0438: `outer_borrowed_while`、`source_owner/handoff`、`map_iteration` 退出/成形/临时项及可空清理载体与源证明壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0438 |
| 标题 | `outer_borrowed_while`、`source_owner/handoff`、`map_iteration` 退出/成形/临时项及可空清理载体与源证明壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0437 |

---

## 1. 背景与动机

在 [SPEC-0437](../../archive/specs/0437-lambda-preflight-outer-while-guard-key-return-conditional-facts-ownership-primitive-local-carrier-and-source-closure-shells-retirement.md) 完成 `lambda_preflight_tests`、`outer_borrowed_while` 守卫、`key_return` 身份及 `lower_frontend` 壳子模块收敛后，`crates/lang-codegen/src/ssa/lower_frontend/` 下仍残留一批仅服务于测试对比的叶子壳模块：

1. `ssa/lower_frontend/outer_borrowed_while.rs`（712 行）：仅承载测试对比用外层借用 `for` 与内层可空 `while` 降低，可收敛入 `ssa/checked_integer_conversion_contract_tests.rs`。
2. `ssa/lower_frontend/map_iteration/source_owner/handoff.rs`（724 行）：仅承载测试对比用 Map 源所有者交接与交付证明，可收敛入 `ssa/ordered_map_storage_tests.rs`。
3. `ssa/lower_frontend/map_iteration/exits.rs`（733 行）：仅承载测试对比用 Map 迭代退出与路径恢复，可收敛入 `ssa/checked_integer_conversion_tests.rs`。
4. `ssa/lower_frontend/nullable_if_cleanup/carrier.rs`（733 行）：仅承载测试对比用可空 `if` 清理载体证明，可收敛入 `ssa/map_promotion_contract_tests.rs`。
5. `ssa/lower_frontend/nullable_borrowed_cleanup.rs`（732 行）：仅承载测试对比用可空借用清理计划与运行时校验，可收敛入 `ssa/integer_range_contract_tests.rs`。
6. `ssa/lower_frontend/map_iteration/formed.rs`（745 行）：仅承载测试对比用成形 Map 迭代证明，可收敛入 `ssa/unit_lower_control_result_tests.rs`。
7. `ssa/lower_frontend/map_iteration/temporary.rs`（749 行）：仅承载测试对比用临时 Map 迭代守卫与准入，可收敛入 `ssa/borrowed_container_lowering_tests.rs`。
8. `ssa/lower_frontend/nullable_if_cleanup/constant_presence/source.rs`（762 行）：仅承载测试对比用常量存在性源证明，可收敛入 `ssa/deinit_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/lower_frontend/outer_borrowed_while.rs` | 712 | `ssa/checked_integer_conversion_contract_tests.rs` | 830 |
| `ssa/lower_frontend/map_iteration/source_owner/handoff.rs` | 724 | `ssa/ordered_map_storage_tests.rs` | 847 |
| `ssa/lower_frontend/map_iteration/exits.rs` | 733 | `ssa/checked_integer_conversion_tests.rs` | 858 |
| `ssa/lower_frontend/nullable_if_cleanup/carrier.rs` | 733 | `ssa/map_promotion_contract_tests.rs` | 875 |
| `ssa/lower_frontend/nullable_borrowed_cleanup.rs` | 732 | `ssa/integer_range_contract_tests.rs` | 917 |
| `ssa/lower_frontend/map_iteration/formed.rs` | 745 | `ssa/unit_lower_control_result_tests.rs` | 935 |
| `ssa/lower_frontend/map_iteration/temporary.rs` | 749 | `ssa/borrowed_container_lowering_tests.rs` | 942 |
| `ssa/lower_frontend/nullable_if_cleanup/constant_presence/source.rs` | 762 | `ssa/deinit_tests.rs` | 965 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2149` 降至 `2141`。
