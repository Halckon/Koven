# SPEC-0410：`unit_lower` 接口委托兼容死分支退役、`refine` 构建/准入/凭证壳模块收敛与 `nullable_when` 保留边界核定

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的接口委托兼容死分支（`lower_delegated_call_receiver`）退役、`refine/build`、`refine/subset` 与 `refine/*_receipt` 剩余壳子模块收敛，以及核定非 `String` `nullable_when` 与纯标量函数保留边界时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0410` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认从 `main` 合入最新改动后继续推进非 `String` `nullable_when` 与默认路由专项收口 |
| 基线 | [SPEC-0409](0409-refine-deinit-and-build-receipt-shells-retirement.md)（PR #415 合入 `main` `4150bbd8`） |
| 分支 | `feature/spec-0410-refine-nullable-when-and-default-routing` |
| 前置 | [SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0399](0399-refine-heap-nullable-if-and-compatibility-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](0409-refine-deinit-and-build-receipt-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在合入 `main`（PR #415 `SPEC-0409` `4150bbd8`）的基础上，完成接口委托兼容死分支（`unit_lower/field_borrow.rs::lower_delegated_call_receiver` 与 `unit_lower/call.rs` 兼容分支）退役，将 `refine/build`、`refine/subset` 与 `refine/*_receipt` 下的 7 个小型单入口壳子模块（`build/place.rs`、`build/statements.rs`、`subset/loop_route.rs`、`subset/drops.rs`、`receiver_receipt/admission.rs`、`loan_receipt/admission.rs`、`named_receipt/admission.rs`）收归职责内聚的父模块或相邻模块，并正式核定非 `String` `nullable_when` 与纯标量函数在 E3 白盒测试契约下的保留边界。

## 2. 范围与设计

### 2.1 退役 `unit_lower` 接口委托兼容死分支

- 自 `SPEC-0399` 将接口委托调用（`!site.delegation().is_empty()`）纳入 `refine/subset/range_borrow.rs::expr_requires_refine` 后，所有可达接口委托调用均已路由至 `ssa::refine`，且没有任何含 `nullable_when` 的兼容函数使用接口委托。
- 从 `crates/lang-codegen/src/ssa/unit_lower/field_borrow.rs` 移除不可达的 `lower_delegated_call_receiver`（57 行）及未使用的 `UnitCallReceiverOrigin` 导入；在 `crates/lang-codegen/src/ssa/unit_lower/call.rs` 中对非空 `resolved.delegation()` 直接返回 `LoweringErrorKind::UnsupportedNode`。

### 2.2 收敛 `refine/build`、`refine/subset` 与 `refine/*_receipt` 下的 7 个壳子模块

- **退役 `crates/lang-codegen/src/ssa/refine/build/place.rs`（161 行）**：将 `OwnerRef`、`OwnerLoan`、`owner_loan`、`lend`、`shared_operand` 与 `end_owner_loan` 收归同属字段与 owner 借用构建的 `crates/lang-codegen/src/ssa/refine/build/field.rs`（合并后 352 行，`< 500` 行），并同步更新 `build/{receiver.rs, heap.rs, rc.rs}` 导入路径。
- **退役 `crates/lang-codegen/src/ssa/refine/build/statements.rs`（97 行）**：将语句级借用凭证分发入口 `push`、`value`、`finish_loan_receipts` 与 `authorize_loan_drops` 收归 `crates/lang-codegen/src/ssa/refine/build/borrow_binding.rs`（合并后 382 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/refine/subset/loop_route.rs`（124 行）**：将 `LoopRoute`、`accepts`、`check_while` 与 `scope_has_while_loop` 收归同属控制流准入规则的 `crates/lang-codegen/src/ssa/refine/subset/control.rs`（合并后 327 行，`< 500` 行），并更新 `refine/subset.rs`。
- **退役 `crates/lang-codegen/src/ssa/refine/subset/drops.rs`（147 行）**：将 `drops_supported`、`pending_owner` 与 `fact_in_function` 收归同属局部绑定与字段替换准入校验的 `crates/lang-codegen/src/ssa/refine/subset/field.rs`（合并后 404 行，`< 500` 行），并更新 `refine/subset.rs`。
- **退役 `crates/lang-codegen/src/ssa/refine/{receiver_receipt, loan_receipt, named_receipt}/admission.rs`（139 / 142 / 179 行）**：将单函数 `Plans::new` 准入实现分别收归父模块 `receiver_receipt.rs`（合并后 303 行）、`loan_receipt.rs`（合并后 345 行）与 `named_receipt.rs`（合并后 348 行），全部严格控制在 `< 500` 行内。

### 2.3 非 `String` `nullable_when` 与全函数默认路由的白盒契约边界核定

经全量审计，以下两项边界由现有 2,100+ 行白盒 SSA 结构测试与路由矩阵测试显式锁定（对应 [SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md) §2.2 与 [SPEC-0399](0399-refine-heap-nullable-if-and-compatibility-retirement.md) §2.2）：

1. **非 `String` `nullable_when`（`unit_lower/nullable_when`）**：
   - `crates/lang-codegen/src/native_nullable_when_tests/`（`native_nullable_when_tests.rs`、`support.rs`、`replacement_if.rs`、`generation.rs`、`sources.rs`）直接白盒断言 `unit_lower` 生成的特定 CFG 拓扑与 `Origin::Source` 归属：
     - `support.rs::loop_stop` 断言 `while` 头部的 `when_false` 跳向单独的 `Origin::Source(statement)` 中间块再经 `TerminatorKind::Branch` 跳向 `break` 汇合块（而 `ssa::refine::emit::merge_straight_lines` 会折叠单前驱直通块）；
     - `replacement_if.rs::if_birth` 断言 `when` 分支内嵌套 `if` 的 `NullableBranch` 终结符携带 `Origin::Source(control.span)` 且分支块分别携带 `Origin::Source(control.null)` / `Origin::Source(control.present)`（而 `ssa::refine` 遵循 `SPEC-0350` 将 `NullableBranch` 归因于比较表达式且基本块携带函数级 `Origin`）；
     - `sources.rs::NEGATIVES` 锁定 12 项 `"lower"` 阶段 `UnsupportedNode` 精确字节跨度，且 `refine_subset_tests.rs:116` 与 `refine_tests/map_condition_tests.rs:351` 显式断言可空主语 `when` 保持 `!routed`。
2. **纯标量/短回路/基础循环函数的 `!routed` 白盒契约**：
   - `unit_lower_scalar_tests.rs`、`unit_lower_straight_line_tests.rs`、`unit_lower_loop_tests.rs`、`unit_lower_short_circuit_tests.rs`、`unit_lower_borrow_tests.rs`、`refine_subset_tests.rs`、`refine_subset_tests/temporary_sequential_for.rs` 及 `refine_tests/{container_tests.rs, map_condition_tests.rs}` 显式断言纯标量/基础循环函数保持 `!routed`，并白盒检查 `unit_lower` 特有的死借用参数携带、独立 `while` preheader 及 `[result, carried...]` 块参数顺序。

## 3. 验收标准

- [x] 退役 `unit_lower/field_borrow.rs::lower_delegated_call_receiver` 与 `refine` 下的 7 个壳子模块（`build/place.rs`、`build/statements.rs`、`subset/loop_route.rs`、`subset/drops.rs`、`receiver_receipt/admission.rs`、`loan_receipt/admission.rs`、`named_receipt/admission.rs`）。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个退役项已从版本库移除 | 通过 | 逐个确认：8/8 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 5/8 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |

核对说明：

- `ssa/triggered-by-0410` 的 3 个 `*/admission.rs` 逐个确认已删除（receiver/loan/named receipt）
- 宿主 `refine/subset/control.rs` 随后被 SPEC-0417 再次合并，其 `LoopRoute` 逻辑现位于 `ssa/refine/subset/` 下的控制流准入模块
- `unit_lower/call.rs`、`refine/build/borrow_binding.rs`、`refine/subset/field.rs`、三个 receipt 宿主文件均存在
