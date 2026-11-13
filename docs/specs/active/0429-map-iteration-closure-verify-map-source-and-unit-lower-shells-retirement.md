# SPEC-0429: `map_iteration`、`closure`、`verify_ownership/borrow_result`、`map_source/formation` 及 `unit_lower` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0429 |
| 标题 | `map_iteration`、`closure`、`verify_ownership/borrow_result`、`map_source/formation` 及 `unit_lower` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0428 |

---

## 1. 背景与动机

在 [SPEC-0428](0428-model-render-verify-provider-refine-and-frontend-shells-retirement.md) 完成 `model/constant`、`render/types`、`verify_ownership/map`、`provider`、`refine` 及 `lower_frontend` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批仅向父模块或同层模块暴露私有实现的薄壳文件：

1. `ssa/lower_frontend/map_iteration/carrier.rs`（473 行）：仅向 `map_iteration` 暴露 Map 迭代载体与证据校验，可收敛入 `ssa/lower_frontend/control/map_frames.rs`。
2. `ssa/lower_frontend/map_iteration/checked.rs`（474 行）：仅向 `map_iteration` 与 `lower_frontend` 暴露受检 Map 运输与嵌套祖先校验，可收敛入 `ssa/lower_frontend/map_iteration/conditional.rs`。
3. `ssa/verify/closure_content.rs`（484 行）：仅向 `verify` 与 `verify_ownership` 暴露可调用对象捕获内容数据流证明，可收敛入 `ssa/closure.rs`。
4. `ssa/verify_ownership/borrow_result.rs`（483 行）：仅向 `verify_ownership.rs` 暴露 `BorrowReturn`/`BorrowCall` 所有权校验，可收敛入 `ssa/verify_ownership/range.rs`。
5. `ssa/map_source/formation.rs`（484 行）：仅向 `map_source.rs` 暴露 Map 源形成与延续纪元解析，可收敛入 `ssa/lower_frontend/nullable_numeric_cleanup.rs`。
6. `ssa/lower_frontend/map_iteration/source_owner/provenance.rs`（489 行）：仅向 `source_owner` 与 `conditional` 暴露源 owner 溯源图校验，可收敛入 `ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs`。
7. `ssa/lower_frontend/map_iteration/temporary/cleanup.rs`（489 行）：仅向 `temporary.rs` 与 `map_iteration.rs` 暴露临时 Map 源清理证明，可收敛入 `ssa/refine/build/rc.rs`。
8. `ssa/unit_lower/control/conditional/nullable_if/heap.rs`（507 行）：仅向 `nullable_if` 暴露堆对象可空 `if` 入口准备与条件校验，可收敛入 `ssa/unit_lower/loop_control.rs`。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/carrier.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/control/map_frames.rs`。
2. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/checked.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional.rs`。
3. 退役 `crates/lang-codegen/src/ssa/verify/closure_content.rs`，将其内联至 `crates/lang-codegen/src/ssa/closure.rs`。
4. 退役 `crates/lang-codegen/src/ssa/verify_ownership/borrow_result.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/range.rs`。
5. 退役 `crates/lang-codegen/src/ssa/map_source/formation.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup.rs`。
6. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/provenance.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs`。
7. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary/cleanup.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/build/rc.rs`。
8. 退役 `crates/lang-codegen/src/ssa/unit_lower/control/conditional/nullable_if/heap.rs`，将其内联至 `crates/lang-codegen/src/ssa/unit_lower/loop_control.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/carrier.rs` | `crates/lang-codegen/src/ssa/lower_frontend/control/map_frames.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/checked.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify/closure_content.rs` | `crates/lang-codegen/src/ssa/closure.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_ownership/borrow_result.rs` | `crates/lang-codegen/src/ssa/verify_ownership/range.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/map_source/formation.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/provenance.rs` | `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/temporary/cleanup.rs` | `crates/lang-codegen/src/ssa/refine/build/rc.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/unit_lower/control/conditional/nullable_if/heap.rs` | `crates/lang-codegen/src/ssa/unit_lower/loop_control.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
