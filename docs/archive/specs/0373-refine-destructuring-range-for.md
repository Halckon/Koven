# SPEC-0373：局部解构与整数区间 `for` 循环迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的局部解构与整数区间 `for` 循环细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0373` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 块与控制流](../../guide/06-blocks-control-flow.md)、[集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认按推荐优先级依次完成 E3 后续切片 |
| 基线 | `feature/spec-0372-refine-generic-class-enum` `53f961b5` |
| 分支 | `feature/spec-0373-refine-destructuring-range-for` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0372](0372-refine-generic-class-enum.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 `value class`（含 Copyable 与 MoveOnly 标量/`String`/普通 `class` 字段）的局部解构声明（`val (a, b) = expr`）以及 `IntRange`/`LongRange` 的升序 `for` 循环（`for (i in lo..hi)` / `for (i in span)`，含 `break`、`continue`、`return` 与 `LoopExit` 清理）迁入 `ssa::refine`，使已迁入家族内的解构与数值区间迭代统一由所有权细化层构建。

## 2. 实施内容

1. **细化层 IR 与发射（`refine/body.rs`、`refine/emit.rs`）**：
   - 新增 `ROp::Binary`（用于非空升序区间 `current < last` 步进的不溢出加法）、`ROp::AggregateCopyExplode` 与 `ROp::AggregateExplode`，并在 `emit.rs` 中映射为对应 SSA `Operation`。
2. **局部解构与 MoveOnly `value class`（`refine/subset.rs`、`refine/subset/value.rs`、`refine/build.rs`、`refine/build/value.rs`）**：
   - 新增 `Checker::move_value_class` 与 `local_destructuring`，支持 `DestructuringMode::Copy`（Copyable `value class`）与 `DestructuringMode::Consume`（MoveOnly `value class`，构造消费 `ConstructionRootKind::Inline`，解构交付初始化表达式并绑定各分量槽位）。
3. **整数区间 `for` 循环（`refine/subset.rs`、`refine/subset/scalar_text.rs`、`refine/build/scalar_text.rs`）**：
   - 在 `type_demands` 中为 `IntRange`/`LongRange` 自动登记端点类型 `Int`/`Long`。
   - 新增 `Checker::numeric_for` 与 `Builder::numeric_for`：在循环入口一次性求值区间快照并投影 `first`/`last`/`empty`，构建 `header`、`continuation`（比较 `current == last`）、`increment`（`current + 1` 回边）、`exhausted`（消费 `LoopExit` 析构）与 `exit` 块。

## 3. 验收标准

- [x] `refine_tests::destructuring_and_numeric_for_lower_in_refine` 验证 Copyable/MoveOnly `value class` 局部解构与含 `continue`/`break` 的 `IntRange` `for` 循环经 `ssa::refine` lowering 并通过原生计数执行验证。
- [x] `unit_lower_aggregate_tests::lowers_compilation_unit_copyable_and_move_only_destructuring` 与 `numeric_iteration_tests`（5/5）全部通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 1 个退役项已从版本库移除 | 通过 | 逐个确认：1/1 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 1/1 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- lower_in_refine` = `14 passed / 0 failed`（覆盖 0370–0381 各片的具名迁移回归）；模块级验收（`ssa::refine_`、`ssa::unit_` 等）由完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` 覆盖 |

核对说明：

- 具名验收：`destructuring_and_numeric_for_lower_in_refine`、`unit_lower_aggregate_tests::lowers_compilation_unit_copyable_and_move_only_destructuring`
- 本 Spec 的验收标准在文档中是未勾选的 `[ ]` 列表；本次按其中具名测试与模块级要求实际执行后结账，未修改原验收条目文本。历史已 squash，改动无法单独归因到本 Spec 的提交。
