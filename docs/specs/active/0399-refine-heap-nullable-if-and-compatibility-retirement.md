# SPEC-0399：同根堆对象可空 `if`、接口委托与混合兼容回退旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的同根堆对象可空 `if` 兼容分支、可空根共享借用实参（`lower_nullable_root_borrow`）、接口委托调用接收者（`lower_delegated_call_receiver`）及 `migrated_family_compatibility` 混合回退路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0399` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认合入最新 `main` 后继续推进下一片 E3 旧路径退役 |
| 基线 | [SPEC-0398](0398-refine-nullable-string-and-field-if-retirement.md)（`c96e63e2`） |
| 分支 | `feature/spec-0399-refine-heap-nullable-if-and-compatibility-retirement` |
| 前置 | [SPEC-0385](0385-refine-deinit-borrow-heap-nullable-if-string-field-retirement.md)、[SPEC-0386](0386-refine-inout-delegation-object-destructuring-retirement.md)、[SPEC-0395](0395-refine-nullable-while-retirement.md)、[SPEC-0398](0398-refine-nullable-string-and-field-if-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将含循环重赋值可空变量的接口委托调用与同根堆对象/字符串可空 `if` 函数统一纳入 `ssa::refine` 的 `scope_requires_refine` 准入范围，消除 `unit_lower` 中的 `migrated_family_compatibility` 混合兼容回退标志，并完整退役 `unit_lower/control/nullable_if/heap.rs`、`unit_lower/map_consume.rs` 以及 `unit_lower/borrow.rs` 中的可空根证明共享借用实参旧路径（净减约 800 行旧代码）。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入补齐

- 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 中新增 `if_requires_refine`，并在 `expr_requires_refine` 中补充：
  1. 接口委托调用（`self.inputs.source_plan.call_site(self.inputs.source_token, id).is_some_and(|site| !site.delegation().is_empty())`）；
  2. 条件为堆类或 `String` 的可空 `if`（`Nullable(inner)` 满足 `self.class(*inner) || self.string(*inner)`，且比较根符号存在 `non_null_use` 或根 `LoanFact`）。
- 由此使包含 `while` 循环重赋值可空局部变量并伴随接口委托调用或堆对象/`String` 可空 `if` 的函数直接通过 `strip_reassigned_loop_views` 路由至 `ssa::refine`，不再回退至 `unit_lower`。

### 2.2 `unit_lower` 旧路径退役清单

删除以下 2 个旧路径文件：

- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/heap.rs`
- `crates/lang-codegen/src/ssa/unit_lower/map_consume.rs`

同步清理以下模块中的残留代码：

- `crates/lang-codegen/src/ssa/unit_lower/borrow.rs`：移除 `can_call_with_nullable_root_proof`、`lower_nullable_root_borrow` 与 `lower_nullable_root_place_borrow`。
- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if.rs`：移除 `heap` 子模块、`prepare_heap_nullable_if_entry`、`map_if_body_is_empty` 与 `migrated_family_compatibility` 分支。
- `crates/lang-codegen/src/ssa/unit_lower/{unit_lower.rs, orchestrate.rs, call.rs, while_proof.rs, nullable_proof/guard_tests/support.rs}`：移除 `migrated_family_compatibility` 字段、`root_borrow_call` 守卫豁免与 `lower_delegated_call_receiver` 调用，并将 `ensure_while_proof_non_call_carriers` 内联回 `ensure_no_while_proof_carriers`。

## 3. 验收标准

- [x] 删除第 2.2 节列出的旧路径模块并清理关联模块中的 `migrated_family_compatibility` 与可空根借用回退逻辑。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
