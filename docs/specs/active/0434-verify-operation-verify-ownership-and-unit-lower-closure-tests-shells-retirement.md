# SPEC-0434: `verify_operation/map/tests`、`verify_ownership` 字段与生命周期测试及 `unit_lower` 闭包/类型原点测试壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0434 |
| 标题 | `verify_operation/map/tests`、`verify_ownership` 字段与生命周期测试及 `unit_lower` 闭包/类型原点测试壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0433 |

---

## 1. 背景与动机

在 [SPEC-0433](../../archive/specs/0433-refine-receipt-ci-tests-verify-closure-escape-and-verify-operation-map-tests-shells-retirement.md) 完成 `refine` 五类借用凭证 `ci_tests`、`verify/closure_escape/types/graph_tests` 及 `verify_operation/map` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批 `verify_operation/map`、`verify_ownership` 及 `unit_lower` 的孤立单文件测试壳模块：

1. `ssa/verify_operation/map/tests.rs`（212 行）：仅包含 Map 操作局部静态契约测试，可收敛入 `ssa/unit_plan/concrete_types.rs`。
2. `ssa/verify_ownership/direct_field_tests.rs`（177 行）：仅包含直接字段借用投影证明测试，可收敛入 `ssa/unit_plan/recipe_validation.rs`。
3. `ssa/verify_ownership/provider_lifetime_tests.rs`（223 行）：仅包含迭代 provider 生命周期校验测试，可收敛入 `ssa/unit_lower/cfg/rebind.rs`。
4. `ssa/verify_ownership/provider_entry_projection_tests.rs`（469 行）：仅包含迭代 provider 入口投影状态守卫测试，可收敛入 `ssa/unit_lower/loop_control/iteration.rs`。
5. `ssa/unit_lower/closure/layout_tests.rs`（69 行）：仅包含共享捕获存储布局测试，可收敛入宿主 `ssa/unit_lower/closure.rs`。
6. `ssa/unit_lower/closure/generic_runtime_tests.rs`（97 行）：仅包含泛型闭包运行时资源返回测试，可收敛入 `ssa/unit_lower/aggregate.rs`。
7. `ssa/unit_lower/closure/runtime_slots_tests.rs`（157 行）：仅包含闭包运行时外部借用槽保持测试，可收敛入 `ssa/unit_lower/receiver.rs`。
8. `ssa/unit_lower/type_origin_cache_tests.rs`（272 行）：仅包含类型原点缓存顺序测试，可收敛入 `ssa/unit_lower/constant.rs`。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/verify_operation/map/tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_plan/concrete_types.rs`。
2. 退役 `crates/lang-codegen/src/ssa/verify_ownership/direct_field_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_plan/recipe_validation.rs`。
3. 退役 `crates/lang-codegen/src/ssa/verify_ownership/provider_lifetime_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/cfg/rebind.rs`。
4. 退役 `crates/lang-codegen/src/ssa/verify_ownership/provider_entry_projection_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration.rs`。
5. 退役 `crates/lang-codegen/src/ssa/unit_lower/closure/layout_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/closure.rs`。
6. 退役 `crates/lang-codegen/src/ssa/unit_lower/closure/generic_runtime_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/aggregate.rs`。
7. 退役 `crates/lang-codegen/src/ssa/unit_lower/closure/runtime_slots_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/receiver.rs`。
8. 退役 `crates/lang-codegen/src/ssa/unit_lower/type_origin_cache_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/constant.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/verify_operation/map/tests.rs` | `crates/lang-codegen/src/ssa/unit_plan/concrete_types.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_ownership/direct_field_tests.rs` | `crates/lang-codegen/src/ssa/unit_plan/recipe_validation.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_ownership/provider_lifetime_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/cfg/rebind.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_ownership/provider_entry_projection_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/closure/layout_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/closure.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/closure/generic_runtime_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/aggregate.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/closure/runtime_slots_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/receiver.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/type_origin_cache_tests.rs` | `crates/lang-codegen/src/ssa/unit_lower/constant.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
