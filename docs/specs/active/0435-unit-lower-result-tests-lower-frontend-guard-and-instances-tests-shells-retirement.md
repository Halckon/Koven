# SPEC-0435: `unit_lower/control/result_tests`、`item_operand_ssa_test_support` 及 `lower_frontend` 守卫与实例测试壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0435 |
| 标题 | `unit_lower/control/result_tests`、`item_operand_ssa_test_support` 及 `lower_frontend` 守卫与实例测试壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0434 |

---

## 1. 背景与动机

在 [SPEC-0434](0434-verify-operation-verify-ownership-and-unit-lower-closure-tests-shells-retirement.md) 完成 `verify_operation/map/tests`、`verify_ownership` 字段与生命周期测试及 `unit_lower` 闭包/类型原点测试壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批 `unit_lower/control` 及 `lower_frontend` 的孤立单文件测试与守卫壳模块：

1. `ssa/unit_lower/control/result_tests.rs`（428 行）：仅包含控制流上下文空值结果交付测试，可收敛入同目录 `ssa/unit_lower/control/conditional.rs`。
2. `ssa/lower_frontend_map_item_operand_ssa_test_support.rs`（305 行）：仅包含 Map 条目返回操作数 SSA 结构断言辅助，可收敛入同父测试支撑 `ssa/lower_frontend_map_same_guard_unit_test_support.rs`。
3. `ssa/lower_frontend/map/guard_tests.rs`（244 行）：仅包含 Map 条件读取与清理守卫测试，可收敛入宿主 `ssa/statement_core.rs`。
4. `ssa/lower_frontend/non_null_proof/guard_tests.rs`（246 行）：仅包含非空证明私有守卫测试，可收敛入 `ssa/expression_core.rs`。
5. `ssa/lower_frontend/instances/canonical_callable_tests.rs`（98 行）：仅包含借用可调用实例规范化测试，可收敛入 `ssa/map_source.rs`。
6. `ssa/lower_frontend/instances/callable_route_tests.rs`（228 行）：仅包含完整键可调用路由测试，可收敛入 `ssa/lower_frontend/control/carried.rs`。
7. `ssa/lower_frontend/nullable_if_cleanup/tests.rs`（299 行）：仅包含可空 `if` 清理直接私有守卫测试，可收敛入 `ssa/unit_lower/ownership.rs`。
8. `ssa/lower_frontend/map_consume/nullable/guard_tests.rs`（406 行）：仅包含可空 `Map.consume` 检查点守卫测试，可收敛入 `ssa/lower_frontend/field_replace.rs`。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/unit_lower/control/result_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/control/conditional.rs`。
2. 退役 `crates/lang-codegen/src/ssa/lower_frontend_map_item_operand_ssa_test_support.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend_map_same_guard_unit_test_support.rs`。
3. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map/guard_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/statement_core.rs`。
4. 退役 `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof/guard_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/expression_core.rs`。
5. 退役 `crates/lang-codegen/src/ssa/lower_frontend/instances/canonical_callable_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/map_source.rs`。
6. 退役 `crates/lang-codegen/src/ssa/lower_frontend/instances/callable_route_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs`。
7. 退役 `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/ownership.rs`。
8. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable/guard_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/field_replace.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/unit_lower/control/result_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/control/conditional.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend_map_item_operand_ssa_test_support.rs` | `crates/lang-codegen/src/ssa/lower_frontend_map_same_guard_unit_test_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map/guard_tests.rs` | `crates/lang-codegen/src/ssa/statement_core.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof/guard_tests.rs` | `crates/lang-codegen/src/ssa/expression_core.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/instances/canonical_callable_tests.rs` | `crates/lang-codegen/src/ssa/map_source.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/instances/callable_route_tests.rs` | `crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/ownership.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable/guard_tests.rs` | `crates/lang-codegen/src/ssa/lower_frontend/field_replace.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
