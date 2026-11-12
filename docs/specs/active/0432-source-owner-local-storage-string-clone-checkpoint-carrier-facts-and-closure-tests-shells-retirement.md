# SPEC-0432: `source_owner`、`local_storage`、`string_clone`、`checkpoint`、`carrier`、`closure/runtime_tests`、`nullable_numeric_cleanup/tests` 及 `orchestrate/registration_tests` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0432 |
| 标题 | `source_owner`、`local_storage`、`string_clone`、`checkpoint`、`carrier`、`closure/runtime_tests`、`nullable_numeric_cleanup/tests` 及 `orchestrate/registration_tests` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0431 |

---

## 1. 背景与动机

在 [SPEC-0431](0431-loop-control-field-replace-range-identity-nullable-nominal-and-borrow-shells-retirement.md) 完成 `map_source_unit_contract`、`range`、`map_iteration/local/identity`、`nullable_when`、`nominal`、`loop_control/nullable_while`、`borrow_result` 及 `borrow_argument` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批可并入现有测试支撑与宿主模块的薄壳文件：

1. `ssa/lower_frontend/map_iteration/source_owner.rs`（595 行）：仅向 `map_iteration` 暴露 Map 源 owner 证明，可收敛入 `ssa/tests.rs`。
2. `ssa/lower_frontend/local_storage.rs`（667 行）：仅向 `lower_frontend` 暴露局部存储规划与整数辅助，可收敛入 `ssa/unit_lower_test_support.rs`。
3. `ssa/lower_frontend/string_clone.rs`（668 行）：仅向 `lower_frontend` 暴露字符串克隆、字段借用与声明规划，可收敛入 `ssa/map_unit_consumer_test_support.rs`。
4. `ssa/lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs`（671 行）：仅向 `nullable_map_guard_support` 暴露可空 Map 守卫检查点恢复，可收敛入 `ssa/refine/build/owner_consumer_test_support.rs`。
5. `ssa/lower_frontend/nullable_borrowed_cleanup/carrier.rs`（672 行）：仅向 `nullable_borrowed_cleanup` 暴露只读 CFG 列与源证明，可收敛入 `ssa/borrow_memory_test_support.rs`。
6. `ssa/unit_lower/closure/runtime_tests.rs`（532 行）：仅包含编译单元闭包运行时回调测试，可收敛入 `ssa/types.rs` 的测试专用内联模块。
7. `ssa/lower_frontend/nullable_numeric_cleanup/tests.rs`（461 行）：仅包含数值循环可空清理守卫测试，可收敛入 `ssa/lower_frontend/loop_control/iteration.rs`。
8. `ssa/lower_frontend/orchestrate/registration_tests.rs`（408 行）：仅包含前端编排登记测试，可收敛入 `ssa/unit_lower/nullable_when/source.rs` 的测试专用内联模块。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner.rs`，将其内联至 `crates/lang-codegen/src/ssa/tests.rs`。
2. 退役 `crates/lang-codegen/src/ssa/lower_frontend/local_storage.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower_test_support.rs`。
3. 退役 `crates/lang-codegen/src/ssa/lower_frontend/string_clone.rs`，将其内联至 `crates/lang-codegen/src/ssa/map_unit_consumer_test_support.rs`。
4. 退役 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/build/owner_consumer_test_support.rs`。
5. 退役 `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/carrier.rs`，将其内联至 `crates/lang-codegen/src/ssa/borrow_memory_test_support.rs`。
6. 退役 `crates/lang-codegen/src/ssa/unit_lower/closure/runtime_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/types.rs`。
7. 退役 `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup/tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/loop_control/iteration.rs`。
8. 退役 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/registration_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/nullable_when/source.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner.rs` | `crates/lang-codegen/src/ssa/tests.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/local_storage.rs` | `crates/lang-codegen/src/ssa/unit_lower_test_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/string_clone.rs` | `crates/lang-codegen/src/ssa/map_unit_consumer_test_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs` | `crates/lang-codegen/src/ssa/refine/build/owner_consumer_test_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/carrier.rs` | `crates/lang-codegen/src/ssa/borrow_memory_test_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/closure/runtime_tests.rs` | `crates/lang-codegen/src/ssa/types.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup/tests.rs` | `crates/lang-codegen/src/ssa/lower_frontend/loop_control/iteration.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/registration_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/nullable_when/source.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
