# SPEC-0378：复合 class 字段、通用 Rc 负载与智能窄化接收者迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的非标量 `class` 字段、通用 `Rc<T>` 负载与 `Rc<T>?` 证明视图、MoveOnly 值空值测试、智能窄化接收者方法调用以及无 `else` 的双分支布尔字面量 `when` 细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0378` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认整理 E3 剩余范围后继续推进下一片 |
| 基线 | `feature/spec-0377-refine-box-map` `5212a9bc` |
| 分支 | `feature/spec-0378-refine-class-fields-rc-null` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0377](0377-refine-box-map.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将携带复合存储类型字段（容器、`Rc`、`Box`、`value class`、`enum class`、非递归 `class`）的 `class`、通用 `Rc<T>` 负载（含 `String`、`value class` 等）与 `Rc<T>?` 证明视图、非条件位置的 MoveOnly 空值比较（`val present = s != null`）、智能窄化可空接收者的成员调用（`if (n != null) { n.get() }`）以及无 `else` 的完备布尔字面量 `when`（`when (b) { true -> ...; false -> ... }`）迁入所有权细化层（`ssa::refine`）。

## 2. 实施内容

1. **复合 `class` 字段准入、读取、替换与借用（`refine/subset.rs`、`refine/subset/heap.rs`、`refine/subset/field.rs`、`refine/subset/callable.rs`、`refine/build/callable.rs`）**：
   - 将 `Checker::class` 扩展为带活跃集防环的 `class_seen`，支持任意非递归 `storage_type` 字段（如 `class Bag(val items: List<Int>)`），并在 `push_demand` 中递归收集字段类型需求。
   - `projection` 支持任意 Copyable `storage_type` 字段读取；`field_assignment` 支持任意 `storage_type` 字段替换（`owner.f = v`）；`borrowed_string_field_loan` 推广为支持任意 MoveOnly `storage_type` 字段经 `ROp::SharedHeapFieldLoan` 出借给 `Borrow` 调用实参。
   - 在 `Checker::call` 中校验 `site.delegation().is_empty()`，使接口委托调用保持既有路径。
2. **通用 `Rc<T>` 负载、`Rc<T>?` 证明视图与借用负载实参（`refine/subset/rc.rs`、`refine/build/rc.rs`、`refine/build/heap.rs`）**：
   - `rc_payload_seen` 接纳任意非递归 `storage_type` 负载（如 `Rc<String>`、`Rc<Data>`），并将 `Rc<T>` 纳入 `view_inner_seen`，支持 `Rc<T>?` 的空值分支证明视图、`.share()`、`.value`、`.value.f` 及 `!!` 解包。
   - 新增 `borrowed_rc_payload` / `borrowed_rc_payload_loan`，支持 MoveOnly `owner.value` 作为 `Borrow` 调用实参直接从 `SharedPayloadPlace` 建立共享借用。
3. **非条件位置 MoveOnly 空值比较与智能窄化接收者调用（`refine/subset.rs`、`refine/subset/receiver.rs`、`refine/build/condition.rs`、`refine/build/place.rs`、`refine/build/expression.rs`）**：
   - 在 `Checker` 中以 `view_symbols` 精确追踪 `if`/`while` 真分支内激活的 MoveOnly 证明视图；当 `views == false`（如 `val present = s != null`、`!(s != null)`、`s != null || ...`）时允许 MoveOnly 空值比较并在 `non_null_block` 立即发射 `ROp::BorrowEnd` 结束证明视图。
   - `owner_type` 与 `owner_loan` 支持智能窄化后的 `class` 具名绑定直接复用其活跃证明视图作为 `Borrow` 成员调用接收者，并在调用返回后消费接收者的 `AfterExpression` 清理事实。
4. **完备双分支布尔字面量 `when`（`refine/subset/when.rs`、`refine/build/when.rs`）**：
   - 对覆盖 `true` 与 `false` 两个字面量分支且无 `else` 的布尔 `when`（`when (b) { true -> ...; false -> ... }` 或反序），直接发射基于 `subject` 的双向 `RTerm::Branch` 并按原条目下标消费 `BranchExit` 清理事实。

## 3. 验收标准

- [x] `refine_subset_tests`（3/3）与 `refine_tests`（含 `class_fields_rc_and_smart_cast_receivers_lower_in_refine`）全部通过。
- [x] `unit_lower_rc_tests`、`lower_frontend_rc_unit_comparison_tests`、`lower_frontend_nullable_unit_comparison_tests`、`unit_lower_control_result_tests` 与 `unit_field_borrow_tests` 全部保持通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
