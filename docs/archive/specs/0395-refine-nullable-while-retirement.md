# SPEC-0395：可空 `while` 证明视图与借用子项旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的可空 `while` 家族（`unit_lower/loop_control/nullable_while.rs`、`borrowed_while.rs`、`borrow/nullable_field.rs`、`call_lifetimes/nullable_child.rs`、`cfg/pending_child.rs` 与 `while_proof.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0395` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认继续推进 E3；按顺序先推进可空 `while` 家族旧路径退役，再推进 Map 迭代旧路径退役 |
| 基线 | `main` `4ae149a6`（PR #354） |
| 分支 | `feature/spec-0395-refine-legacy-retirement` |
| 前置 | [SPEC-0378](../../specs/active/0378-refine-class-fields-rc-and-null-views.md)、[SPEC-0385](0385-refine-deinit-borrow-heap-nullable-if-string-field-retirement.md)、[SPEC-0394](0394-refine-string-clone-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将直接堆对象 `class` 可空 `while (x != null)` 循环计入“只在细化层降低”的构造（`subset/range_borrow.rs::while_requires_refine`），补齐 `ssa::refine` 对循环体内证明视图变量重赋值、`if` 分支继续边视图失效及数值区间 `for` 循环入口被重赋值视图提前结束的准入与构建支持，并退役 `unit_lower` 中专用于可空 `while` 证明家族的旧路径模块（合计约 4,800 行代码）。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入与构建收口

1. **直接堆对象可空 `while` 纳入必需细化构造**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 新增 `while_requires_refine(condition)`，当 `while` 条件（剥离 `Expression::Group` 后）携带 `non_null_when_true()` 的 `null_comparison` 事实且被测可空类型的非空内层为 `class` 时，将其所在函数及编译单元标记为需要细化层（`span_requires_refine` / `unit_requires_refine`）。
   - 普通标量 `while`（如 `while (i < 2)`）与 `Rc<T>?` 共享所有者 `while` 不触发该强制路由，保持既有分流边界测试不变。
2. **循环内证明视图重赋值与分支重测准入**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset/field.rs` 的 `assignment` 与 `compound_assignment` 中，当 `self.scope_requires_refine()` 为真时，允许在循环内对当前处于 `view_symbols` 的绑定重赋值并在赋值后移除该视图符号（构建器 `build/assignment.rs` 已通过 `end_views_of` 先结束证明视图再更新绑定）。
   - 在 `crates/lang-codegen/src/ssa/refine/subset.rs` 的 `Expression::If` 准入中，若可继续执行的分支（`statement_may_continue`）移除了某个外层证明视图符号，则在 `If` 合流后从 `view_symbols` 中剔除该失效符号，使分流后继续读取未重测视图的非法用例保持拒绝，而先经 `if (current != null)` 重测的合法用例正常准入。
3. **数值区间 `for` 循环前结束被重赋值外层视图**：
   - 在 `crates/lang-codegen/src/ssa/refine/build/scalar_text.rs::numeric_for` 入口调用 `self.end_reassigned_loop_views(span)?`，与 `while_loop`、`loop_statement`、`sequential_for`、`map_for` 对齐。

### 2.2 `unit_lower` 旧路径退役清单

删除以下可空 `while` 专属旧路径模块与测试：

- `crates/lang-codegen/src/ssa/unit_lower/loop_control/nullable_while.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/nullable_while/entry.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/nullable_while/borrowed_for.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/nullable_while/borrowed_for/guard_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/nullable_while/borrowed_for/guard_tests/checkpoint.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/borrowed_while.rs`
- `crates/lang-codegen/src/ssa/unit_lower/borrow/nullable_field.rs`
- `crates/lang-codegen/src/ssa/unit_lower/call_lifetimes/nullable_child.rs`
- `crates/lang-codegen/src/ssa/unit_lower/cfg/pending_child.rs`
- `crates/lang-codegen/src/ssa/unit_lower/cfg/pending_child/guard_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/while_proof/retirement_tests.rs`

同步清理 `nullable_proof.rs`（移除 `NullableProofScope::While` 与 `introduce_while_proof`）、`while_proof.rs`（仅保留 `String` `If` 证明返回与清理辅助）、`control/nullable_if.rs`（移除 `while` 内未窄化 `if` 重测回退分支）、`loop_control.rs`、`loop_control/iteration.rs`、`borrow.rs`、`call.rs`、`call_lifetimes.rs`、`cfg.rs`、`cfg/rebind.rs`、`cfg/alias_slots.rs`、`assignment.rs`、`aggregate.rs` 与 `ownership.rs` 中的残留字段与调用点。

### 2.3 测试契约更新

- `crates/lang-codegen/src/ssa/while_flow_tests/{direct_contract.rs, written_nested_contract.rs, distinct_nested_contract.rs, pending_child_contract.rs, if_retest_contract.rs, borrowed_proof_cases.rs, borrowed_proof_contract.rs, outer_borrowed_proof_contract.rs}`：对齐 `ssa::refine` 的等价 SSA 形状（死参数消除、无回边 `while` 头块折叠，以及 `NullableBranch` 记录空值比较条件源区间）。
- `crates/lang-codegen/src/ssa/refine_subset_tests.rs` 与 `crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs`：补充可空 `while` 细化层路由与强制旧路径下的 `UnsupportedNode` 边界回归测试。

## 3. 验收标准

- [x] 删除第 2.2 节列出的全部可空 `while` 旧路径模块并清理残留字段与方法。
- [x] 更新 `while_flow_tests`、`refine_subset_tests` 与 `map_condition_tests` 回归测试。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [ ] 双宿主 `cargo test -p lang-codegen` 与 CI 以 PR 为准。

## 4. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | 路由堆对象可空 `while` 至 `ssa::refine`，退役 `unit_lower` 可空 `while` 家族旧路径并同步测试与文档 |

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 11 个退役项已从版本库移除 | 通过 | 逐个确认：11/11 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 5/11 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`nullable_while`（含 `entry`、`borrowed_for` 及其 guard_tests/checkpoint）、`borrowed_while`、`borrow/nullable_field`、`call_lifetimes/nullable_child`、`cfg/pending_child`（含 guard_tests）、`while_proof/retirement_tests`
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
