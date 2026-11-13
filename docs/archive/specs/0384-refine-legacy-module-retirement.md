# SPEC-0384：枚举、非空断言与可变列表变更旧路径退役及细化层准入补齐

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的非泛型 Copyable `enum` 默认路由、MoveOnly `enum` Copyable 载荷投影、`for` 借用 `String` `when` 主语、单分支完备布尔 `when`、`when` 临时操作数 `!!`、`if (x == null)` `else` 分支非空视图，以及退休旧路径模块（`enum_lower.rs`、`control/when_subject.rs`、`non_null_assertion.rs` 与 `container.rs` 6 项 `MutableList` 变更方法）时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0384` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在最新 `main`（PR #300 与 PR #307 合并后）基线上继续推进 E3 后续开发 |
| 基线 | `main` `58479df5` |
| 分支 | `feature/spec-0384-refine-legacy-module-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md)、[SPEC-0383](0383-refine-final-legacy-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将非泛型 Copyable `enum` 的函数签名、构造、字段投影与 `this` 表达式全量纳入 `ssa::refine` 默认路由，补齐 MoveOnly `enum` 的 Copyable 非标量载荷投影（如 `Result<String, TextError>` 的 `TextError` 载荷）、`for` 借用 / `borrow val` 绑定的 `String` `when` 主语、单分支完备布尔 `when`（`when (flag) { true, false -> ... }`）、`(when (...) { ... })!!` 非空断言临时操作数、`if (x == null)` 的 `else` 分支非空证明视图以及含控制流退出的扩展方法借用接收者，并彻底退役已 100% 由细化层接管的旧 `unit_lower` 模块（`enum_lower.rs` 410 行、`control/when_subject.rs` 137 行、`non_null_assertion.rs` 215 行）及 `unit_lower/container.rs` 中的 6 个 `MutableList` 变更方法（共清理约 1,030 行冗余旧路径代码）。

## 2. 实施内容

1. **`ssa::refine` 准入与构建能力补齐（`refine/subset.rs`、`subset/tagged.rs`、`subset/when.rs`、`subset/heap.rs`、`subset/callable.rs`、`subset/range_borrow.rs`、`build/when.rs`、`build/condition.rs`）**：
   - **非泛型 Copyable `enum` 全量默认路由**：在 `subset.rs`（`signature_types`）与 `subset/tagged.rs`（`this_expression`、`enum_construction`、`enum_projection`）中统一对 Copyable `enum_type` 置位 `self.migrated = true`；在 `enum_projection` 中接纳任意 Copyable `storage_type` 载荷（如嵌套 Copyable `enum` `TextError`）。
   - **借用 `String` `when` 主语与单分支完备布尔 `when`**：`subset/when.rs` 的 `string_when_subject` 接纳 `self.for_borrowed.contains(&symbol)`（覆盖顺序容器/`RangeView`/`Map` `for` 借用绑定与局部 `borrow val` 绑定）；`subset/when.rs` 与 `build/when.rs` 新增单分支完备布尔 `when`（`true, false -> body`）支持，求值主语后直接降级分支体并消费 `BranchExit`。
   - **`when` 临时操作数 `!!` 与作用域细化识别**：`subset/heap.rs` 的 `non_null_assertion` 将 `Expression::When` 纳入合法 `Temporary` 来源；`subset/range_borrow.rs` 的 `expr_requires_refine` 纳入 `typed.non_null_assertion(id).is_some()`，使包含 `!!` 的作用域支持 `MutableMap<K, V>?` 移除结果的消费式断言与扩展方法控制流前缀。
   - **`if (x == null)` 的 `else` 分支非空证明视图**：`subset.rs` 在 `Expression::If` 的 `else` 分支通过 `note_else_view` 为顶层（含括号）`!fact.non_null_when_true()` 空值比较激活 `view_symbols`；`build/condition.rs` 在 `control_span.is_some()` 且 `!drops_owner` 时将证明视图挂入 `on_non_null`（即 `when_false`），由 `if_expression` 进入 `else` 块时激活并在离开或合流时自动发 `BorrowEnd`。
2. **旧路径模块退役与 `UnsupportedNode` 边界收敛**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/enum_lower.rs`（410 行）、`crates/lang-codegen/src/ssa/unit_lower/control/when_subject.rs`（137 行）与 `crates/lang-codegen/src/ssa/unit_lower/non_null_assertion.rs`（215 行）。
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/container.rs` 中已完全由 `ssa::refine` 承担的 6 个 `MutableList` 变更方法（`lower_container_append`、`lower_container_clear`、`lower_container_remove_at`、`lower_container_remove_first`、`lower_container_remove_last`、`lower_container_insert_at`），并将对应分发分支收敛为 `UnsupportedNode`。
   - 清理 `unit_lower/aggregate.rs`、`unit_lower/control.rs`、`unit_lower/field_borrow.rs` 与 `unit_lower/nullable_proof.rs` 中的旧枚举与非空断言死分支，保留非布尔 `when` 条件表达式的精确拒绝 Span。

## 3. 验收标准

- [x] `ssa::` 全量单元测试（1,864 项，含新增的 `enum_construction_for_borrowed_string_when_and_else_views_route_to_refine` 与 `retired_enum_when_non_null_assertion_and_mutable_list_boundary_tests`）100% 通过。
- [x] `llvm::` 全量单元测试与 `native_nullable_when_tests` 100% 通过。
- [x] 手写 Rust 文件尺寸护栏保持在 1000 行软上限内。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 3 个退役项已从版本库移除 | 通过 | 逐个确认：3/3 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 1/3 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`unit_lower/enum_lower.rs`、`unit_lower/control/when_subject.rs`、`unit_lower/non_null_assertion.rs`
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
