# SPEC-0380：`deinit` 析构体、多层字段借用、接口委托、无状态 `object` 与 `Map.withValue` 迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `deinit` 析构体与资源类、多层 `class` 字段读取与借用（含字段 `String.clone()`）、复合 `value class`/`Box<T>` 载荷、MoveOnly 值接收者（`own fun` 及 `this` 借用与返回）、接口 `by` 委托、无状态 `object` 单例、`Map.withValue` 以及 Copyable 复合类型 `Inout` 参数细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0380` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认从最新 `main` 继续推进 E3 下一批工作 |
| 基线 | `main` `4491f1a4` |
| 分支 | `feature/spec-0380-refine-nullable-when-box-map-consume` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0379](0379-refine-reassignment-and-move-enum-when.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 `deinit` 析构体及带 `deinit` 的资源 `class`、多层 `class` 字段读取与借用（`this.first.name`、`g.first.name`、`g.second.code` 及字段路径上的 `String.clone()`）、含复合存储字段的 `value class` 与 `Box<T>` 载荷、MoveOnly 类型上的 `Value` 接收者成员调用（`own fun`、`this` 借用与 `this` 返回）、临时 `Borrow`/`Value` 接收者、接口 `by` 委托与 `StaticSelf` 默认方法特化（含条件 `Value` 接收者交付与析构）、无状态 `object` 单例、`Map.withValue` / `MutableMap.withValue`、内联 `Inout` 接收者（`value class` 与 `enum class` 的 `inout fun`、`InlineFieldReplace` 及调用点写回）以及一级 `class` 字段 `replace(&holder.field, value)`（`HeapFieldExchange`）与任意 Copyable `storage_type` 的 `Inout` 参数与 `&local` 实参迁入所有权细化层（`ssa::refine`）。

## 2. 实施内容

1. **复合 `value class` / `Box<T>` 载荷、`Map.withValue`、内联 `Inout` 接收者与一级字段 `replace(&holder.field, value)`（`refine/body.rs`、`refine/emit.rs`、`refine/subset.rs`、`refine/subset/value.rs`、`refine/subset/heap.rs`、`refine/subset/inout.rs`、`refine/subset/field.rs`、`refine/subset/container.rs`、`refine/build/value.rs`、`refine/build/inout.rs`、`refine/build/field.rs`、`refine/build/container.rs`）**：
   - 将 `value_class_seen`、`move_value_class_seen` 与 `box_payload_seen` 扩展为支持任意 `storage_type_seen` 字段（含顺序容器、`String`、嵌套名义类型，带递归环检测），并支持 `Box<Entry>?`、`Box<Owned>?` 等复合载荷。
   - 新增 `ROp::MapWithValue` 支持 `Map.withValue(key, action)` 与 `MutableMap.withValue(key, action)` 借用回调查询。
   - 将 `ParameterMode::Inout` 参数与 `&local` 实参扩展至任意 Copyable `storage_type`（含 `IntRange`、`LongRange`、Copyable `value class`、Copyable `enum class`）；支持 `value class` 与 `enum class` 的内联 `Inout` 接收者（`ROp::InlineFieldReplace`、`Inout` `value class` 字段读取的 `SharedReborrow -> SharedFieldLoan` 以及调用后经 `ReadPlace` / `RootPlaceTake` 写回局部根绑定）；新增 `ROp::HeapPayloadPlace` 与 `ROp::HeapFieldExchange` 支持一级非泛型 `class` 字段 `replace(&holder.field, value)` 原子置换。
2. **`deinit` 析构体、资源 `class`、MoveOnly `Value`/`Borrow` 接收者、接口 `by` 委托、`StaticSelf` 条件 `Value` 接收者与无状态 `object`（`refine/subset.rs`、`refine/subset/drops.rs`、`refine/subset/receiver.rs`、`refine/build.rs`、`refine/build/place.rs`、`refine/build/receiver.rs`、`refine/build/expression.rs`）**：
   - 识别 `Item::Deinit` 析构体及声明了 `deinit()` 的 `class`，将其路由至 `ssa::refine`。
   - 放开 `ParameterMode::Value` 与 `ParameterMode::Borrow` 接收者到 `box_enum_type`、`move_value_class`、`class` 与无状态 `object`（`NominalKind::Object`，经零字段 `ROp::AggregateConstruct` 按需物化）；支持 `UnitDropTarget::This(owner)` 释放、`Value` `this` 临时借出给 `Borrow` 成员调用、`this` 按值返回及临时 `Borrow`/`Value` 接收者。
   - 支持接口 `by` 委托链（`site.delegation()`），在调用前按序生成 `ROp::SharedHeapFieldLoan` 并在调用后逆序发出 `ROp::BorrowEnd`；支持 `StaticSelf` 默认 `own fun` 的条件接收者交付（`UnitConditionalReceiverDeliveryFact`）与按 `preceding_drops` 位置交错的条件接收者释放（`UnitConditionalReceiverDropFact`）及非对称分支接收者归一释放。
3. **多层 `class` 字段读取、多层字段借用实参及字段 `String.clone()`（`refine/subset/heap.rs`、`refine/subset/callable.rs`、`refine/subset/scalar_text.rs`、`refine/build/heap.rs`、`refine/build/callable.rs`、`refine/build/scalar_text.rs`）**：
   - 新增 `Checker::field_loan_receiver` 与 `Builder::field_projection_loan`，递归解析单层或多层 `class` 字段投影链（如 `this.first.name`、`g.first.name`、`g.second.code`），按层发射 `ROp::SharedHeapFieldLoan` 并在消费后按逆序发射 `ROp::BorrowEnd`。
   - `projection` 支持多层 `class` 上的 Copyable 叶子字段读取（经叶子 `SharedHeapFieldLoan` 执行 `ROp::ReadLoan`）；`borrowed_string_field` / `borrowed_string_field_loan` 支持多层 MoveOnly 字段借出给 `Borrow` 调用实参；`string_operation` 支持在单层或多层字段投影上直接执行 `.clone()`（`ROp::StringClone`）。

## 3. 验收标准

- [x] `refine_subset_tests`（3/3）与 `refine_tests`（含 `deinit_nested_fields_box_payloads_and_move_receivers_lower_in_refine`）全部通过。
- [x] `unit_lower_receiver_tests`（46/46）、`unit_field_replace_tests`（13/13）、`unit_resource_deinit`（13/13）、`unit_lower_enum_tests`（15/15）、`unit_field_borrow_tests` 与 `unit_string_clone_tests` 全部保持通过。
- [x] `rustfmt`、`scripts/check_docs.py` 与单文件 `< 1000` 行上限验证通过。

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

- 具名验收：`deinit_nested_fields_box_payloads_and_move_receivers_lower_in_refine`、`unit_resource_deinit`、`unit_lower_receiver_tests`
- 本 Spec 的验收标准在文档中是未勾选的 `[ ]` 列表；本次按其中具名测试与模块级要求实际执行后结账，未修改原验收条目文本。历史已 squash，改动无法单独归因到本 Spec 的提交。
