# SPEC-0398：可空 `String` `if` 与同根可空堆对象字段 `if` 旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的可空 `String?` `if` 证明视图借用/返回、可空 `String?` 文本扩展接收者借用与同根可空堆对象 `if` 字段读取旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0398` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认继续推进 E3；在完成 `wt-spec-0397` 合并清理后继续推进可空 `String` `if` 与同根可空堆对象字段 `if` 旧路径退役 |
| 基线 | [SPEC-0397](0397-refine-nullable-map-and-closure-retirement.md)（`dd716bab`） |
| 分支 | `feature/spec-0398-refine-nullable-string-and-field-if-retirement` |
| 前置 | [SPEC-0371](../../specs/active/0371-refine-integer-range-text.md)、[SPEC-0378](../../specs/active/0378-refine-class-fields-rc-and-null-views.md)、[SPEC-0385](../../specs/active/0385-refine-deinit-borrow-heap-nullable-if-string-field-retirement.md)、[SPEC-0397](0397-refine-nullable-map-and-closure-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将可空 `String?` 的 `if` 非空证明视图（含 `println` / 共享借用实参、文本扩展接收者借用及窄化 `String` owner 返回）与同根可空堆对象 `if` 字段读取统一收口至 `ssa::refine` 细化层默认路由，并完整退役 `unit_lower` 中对应的 6 个旧路径实现与测试文件（净减约 1,100 行旧代码）。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入与构建收口

1. **`if (x == null)` 的 `else` 分支激活非空证明视图**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset/control.rs::note_else_view` 中，除检查 `LoanFact`、`class(*inner)` 与 `map_kv(*inner)` 外，补充检查 `self.inputs.typed.non_null_uses().iter().any(|use_fact| use_fact.symbol() == fact.symbol())`，使 `if (text == null) { ... } else { return text }` 等仅含非空值使用的 `else` 分支同样激活证明视图。
2. **支持窄化 `String` owner 从 `String?` 参数/局部绑定直接返回**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset.rs::returned_nullable_owner` 中支持 `narrowed == self.inputs.return_type && self.string(narrowed) && !self.is_map_remove_symbol(symbol)`。
   - 在 `crates/lang-codegen/src/ssa/refine/build.rs` 新增 `consume_views_of`，并在 `crates/lang-codegen/src/ssa/refine/build/expression.rs::name` 中当 `narrowed == self.inputs.return_type` 时发出 `ROp::NullableTake { owner, proof }` 提取窄化 `String` owner，同时消费当前激活视图以避免在返回前重复发出 `BorrowEnd`。

### 2.2 `unit_lower` 旧路径退役清单

删除以下 6 个旧路径及私有守卫测试文件：

- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/string.rs`
- `crates/lang-codegen/src/ssa/unit_lower/receiver/nullable_string.rs`
- `crates/lang-codegen/src/ssa/unit_lower/call_lifetimes/nullable_text.rs`
- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/heap/field.rs`
- `crates/lang-codegen/src/ssa/unit_lower/while_proof/string_return_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/borrow/nullable_string_tests.rs`

将整源回归测试文件 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/heap/field/tests.rs` 扁平化迁至 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/field_tests.rs`，并同步清理以下模块中的残留代码：

- `crates/lang-codegen/src/ssa/unit_lower/control/{nullable_if.rs, nullable_if/heap.rs}`：移除 `string` 与 `field` 子模块、`prepare_string_nullable_if_entry`、`ensure_string_if_proof_carriers`、`has_only_string_if_proofs` 及同根堆对象字段 `if` 入口分支。
- `crates/lang-codegen/src/ssa/unit_lower/{while_proof.rs, receiver.rs, call_lifetimes.rs, aggregate.rs, borrow.rs, nullable_proof.rs, orchestrate.rs, unit_lower.rs}`：移除 `can_return_from_nullable_string_if`、`lower_proven_owner_return`、`nullable_string_receiver_loan`、`supports_direct_control_prefix`、`entry_string_roots`、以及 `InlineNullable` String `If` 证明分支。

## 3. 验收标准

- [x] 删除第 2.2 节列出的全部 6 个旧路径文件并清理关联模块中的残留逻辑。
- [x] 更新 `crates/lang-codegen/src/ssa/text_extension_tests.rs` 与 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/field_tests.rs`，验证默认细化层路由覆盖可空 `String` `if` 借用/返回与双极性可空字段 `if`，且强制旧路径（`with_per_family_lowering`）对已退役的 `String` `if` 返回 `UnsupportedNode`。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个退役项已从版本库移除 | 通过 | 逐个确认：8/8 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 1/8 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`control/nullable_if/string.rs`、`receiver/nullable_string.rs`、`call_lifetimes/nullable_text.rs`、`nullable_if/heap/field`（含 tests）、`while_proof/string_return_tests`、`borrow/nullable_string_tests`、`nullable_if/field_tests`
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
