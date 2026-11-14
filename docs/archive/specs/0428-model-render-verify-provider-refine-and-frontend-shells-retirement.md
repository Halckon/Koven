# SPEC-0428: `model/constant`、`render/types`、`verify_ownership/map`、`provider`、`refine` 及 `lower_frontend` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0428 |
| 标题 | `model/constant`、`render/types`、`verify_ownership/map`、`provider`、`refine` 及 `lower_frontend` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
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
5. `ssa/unit_lower/borrow_result.rs`（540 行）：仅向 `unit_lower` 暴露借用结果声明与绑定校验，可收敛入 `ssa/provider.rs` 的内联 `borrow_result` 子模块。
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
5. 退役 `crates/lang-codegen/src/ssa/unit_lower/borrow_result.rs`，将其内联至 `crates/lang-codegen/src/ssa/provider.rs`。
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
| `crates/lang-codegen/src/ssa/unit_lower/borrow_result.rs` | `crates/lang-codegen/src/ssa/provider.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/build/owner_consumer/replacement.rs` | `crates/lang-codegen/src/ssa/refine/build/scalar_text.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/subset/inout.rs` | `crates/lang-codegen/src/ssa/refine/subset/heap.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/provider.rs` | `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件已从版本库物理删除 | 通过 | 提交 `430a0de2` 上逐个确认不存在：`model/constant.rs`、`render/types.rs`、`verify_ownership/map.rs`、`lower_frontend/map_iteration/local_runtime.rs`、`unit_lower/borrow_result.rs`、`refine/build/owner_consumer/replacement.rs`、`refine/subset/inout.rs`、`lower_frontend/outer_borrowed_while/provider.rs` |
| 合并目标承接原内容 | 通过 | `ScalarConstant` 在 `ssa/model/operation.rs:692`；`verify_map_ownership` 在 `ssa/verify_ownership/aliases.rs:777`；`local_runtime` 在 `ssa/lower_frontend/nullable_if_cleanup.rs:438`；`outer_borrowed_provider` 在 `ssa/lower_frontend/non_null_proof.rs:460`；`owner_return` 在 `ssa/refine/build/scalar_text.rs`；inout 论证在 `ssa/refine/subset/heap.rs`；`unit_lower/borrow_result` 现为 `ssa/provider/borrow_result.rs`（宿主仍是 `provider`） |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20`（等价于 `--base main`）退出 0；8 个宿主实测 888 / 989 / 982 / 992 / 457 / 538 / 983 / 988 行 |
| 手写 Rust 文件数净减 8 | 受限通过 | 整体提交删除 387 个文件、新增 137 个（净 -250）。23 个 Spec 已 squash 为单一提交 `430a0de2`，**无法再从历史中分离本 Spec 的 -8**；文件删除逐项已核实，未做逐 Spec 计数分离 |
| `scripts/gen_spec_dag.py` 通过 | 通过 | 在 `430a0de2` 运行后生成的 `docs/specs/` 与 `docs/archive/specs/` 视图无差异（`git status` 稳定） |
| `scripts/check_docs.py` 通过 | 通过 | `documentation check passed (941 Markdown files)`；CI `Docs & Spec Structural Gate` 亦为 pass |
| `cargo test -p lang-codegen --lib` 编译与单测通过 | 通过 | 默认配置全量 `3055 passed / 0 failed / 1 ignored`；`loan-observation-test-support` 全量 `3117 passed / 0 failed / 1 ignored`；本 Spec 直接相关域定向 `cargo test -p lang-codegen --lib -- verify_ownership nullable_if_cleanup outer_borrowed inout constant_ scalar_constant` 为 `173 passed / 0 failed` |

未覆盖项：上述定向为默认配置；本 Spec 不涉及 native/CLI/LSP 行为，未单独跑这些门禁，交由 CI 的 `Targeted Tests` 双平台执行。
