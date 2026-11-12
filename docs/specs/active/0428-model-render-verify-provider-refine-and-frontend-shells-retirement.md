# SPEC-0428: `model/constant`、`render/types`、`verify_ownership/map`、`provider`、`refine` 及 `lower_frontend` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0428 |
| 标题 | `model/constant`、`render/types`、`verify_ownership/map`、`provider`、`refine` 及 `lower_frontend` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0427 |

---

## 1. 背景与动机

在 [SPEC-0427](0427-map-iteration-nullable-borrowed-cleanup-refine-and-frontend-shells-retirement.md) 完成 `map_iteration`、`nullable_borrowed_cleanup`、`refine` 及 `lower_frontend` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批仅向父模块或同层模块暴露私有实现的薄壳文件：

1. `ssa/model/constant.rs`（11 行）：仅包含 `ScalarConstant` 枚举定义，完全可并入 `ssa/model/operation.rs`。
2. `ssa/render/types.rs`（222 行）：仅向 `ssa/render.rs` 暴露 SSA 类型渲染辅助，完全可收敛入 `ssa/verify_types.rs` 的内联 `render_types` 子模块。
3. `ssa/verify_ownership/map.rs`（372 行）：仅向 `ssa/verify_ownership.rs` 暴露 `verify_map_ownership` 与 `reborrow` 子模块，完全可收敛入 `ssa/verify_ownership/aliases.rs`。
4. `ssa/lower_frontend/map_iteration/local_runtime.rs`（549 行）：仅向 `map_iteration` 与 `lower_frontend` 暴露局部 Map 迭代载体与双选择器条件校验，可收敛入 `ssa/lower_frontend/nullable_if_cleanup.rs`。
5. `ssa/provider.rs`（450 行）：仅提供顺序 provider 的固定 SSA 构造辅助，可收敛入 `ssa/unit_lower/borrow_result.rs` 的内联 `provider` 子模块并由 `ssa/mod.rs` 统一重导出。
6. `ssa/refine/build/owner_consumer/replacement.rs`（455 行）：仅向 `ssa/refine/build/owner_consumer.rs` 暴露 `ReturnReplacementPlan` 及相关 `Builder` 方法，可收敛入 `ssa/refine/build/scalar_text.rs`。
7. `ssa/refine/subset/inout.rs`（470 行）：仅为 `refine/subset::Checker` 提供 `Inout` 与所有权原语准入方法，可与 `ssa/refine/subset/heap.rs` 合并。
8. `ssa/lower_frontend/outer_borrowed_while/provider.rs`（519 行）：仅向 `outer_borrowed_while` 暴露外层借用 `for` 与内层可空 `while` 的 provider 形状与回边证明，可收敛入 `ssa/lower_frontend/non_null_proof.rs`。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/model/constant.rs`，将其 `ScalarConstant` 定义并入 `crates/lang-codegen/src/ssa/model/operation.rs`。
2. 退役 `crates/lang-codegen/src/ssa/render/types.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_types.rs`。
3. 退役 `crates/lang-codegen/src/ssa/verify_ownership/map.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/aliases.rs`。
4. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local_runtime.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup.rs`。
5. 退役 `crates/lang-codegen/src/ssa/provider.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/borrow_result.rs`。
6. 退役 `crates/lang-codegen/src/ssa/refine/build/owner_consumer/replacement.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/build/scalar_text.rs`。
7. 退役 `crates/lang-codegen/src/ssa/refine/subset/inout.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/subset/heap.rs`。
8. 退役 `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/provider.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/model/constant.rs` | `crates/lang-codegen/src/ssa/model/operation.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/render/types.rs` | `crates/lang-codegen/src/ssa/verify_types.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_ownership/map.rs` | `crates/lang-codegen/src/ssa/verify_ownership/aliases.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local_runtime.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/provider.rs` | `crates/lang-codegen/src/ssa/unit_lower/borrow_result.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/build/owner_consumer/replacement.rs` | `crates/lang-codegen/src/ssa/refine/build/scalar_text.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/subset/inout.rs` | `crates/lang-codegen/src/ssa/refine/subset/heap.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/provider.rs` | `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
