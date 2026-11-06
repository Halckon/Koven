# SPEC-0387：`String.clone()`、装箱构造、`value class` 字段借用与通用 `when` 旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `String.clone()`（`unit_lower/string_clone.rs`）、`Box(v)` 构造（`unit_lower/aggregate.rs`）、`value class` 字段借用（`unit_lower/field_borrow.rs`）与通用 `when` 控制流（`unit_lower/control.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0387` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0386 基线上继续推进 E3 下一个切片 |
| 基线 | `4972ecd7`（SPEC-0386） |
| 分支 | `feature/spec-0387-refine-field-borrow-string-clone-container-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0356](../../archive/specs/0356-refine-when-family.md)、[SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0377](0377-refine-box-map.md)、[SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md)、[SPEC-0386](0386-refine-inout-delegation-object-destructuring-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0356](../../archive/specs/0356-refine-when-family.md)、[SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0377](0377-refine-box-map.md) 与 [SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md) 已将 `String.clone()`、`Box<T>` 构造、`value class` 字段借用及 `when` 家族迁入 `ssa::refine` 的基础上，补齐 `ssa::refine` 对 `value class` 字段/`Rc<String>` `.value`/顺序容器下标接收者上的 `String.clone()` 及非返回来源 `Borrow` `value class` 参数 MoveOnly 字段借用的准入与构建，并彻底退役 `unit_lower` 中已被细化层完全取代的 legacy 实现模块与分支（合计清理约 903 行旧路径代码，使 `unit_lower/control.rs` 从 844 行降至 396 行）：

1. **补齐 `ssa::refine` 对扩展 `String.clone()` 接收者与 `value class` 字段借用的准入与构建**：
   - 在 `refine/subset/scalar_text.rs` 与 `refine/build/scalar_text.rs` 的 `string_operation` 中新增对 `value class` 字段投影（经 `ROp::SharedFieldLoan`）、`Rc<String>` `.value` 载荷（`borrowed_rc_payload` / `borrowed_rc_payload_loan`）及顺序容器下标 `xs[i]`（`borrowed_container_element` / `borrowed_container_element_loan`）接收者上 `String.clone()` 的支持；
   - 在 `refine/subset/callable.rs` 的 `borrowed_string_field` 中新增 `borrowed_value_parameter_field`（非返回来源的 `Borrow` `value class` 参数 MoveOnly 字段），并将字段借用实参标记为 `self.migrated |= ok`；
   - 在 `refine/subset/range_borrow.rs` 的 `expr_requires_refine` 中纳入 `typed.string_operation(id).is_some()`，使含 `String.clone()` 且带借用返回的编译单元默认路由至 `ssa::refine`。
2. **退役 `unit_lower/string_clone.rs` 与 `IntrinsicBox` 聚合构造旧路径**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/string_clone.rs`（226 行），仅将借用返回路径仍需的 `clone_field_projection` 与 `clone_field_loan` 迁入 `unit_lower/borrow_result.rs`，并在 `unit_lower.rs` 对 `string_operation` 显式返回 `UnsupportedNode`；
   - 在 `unit_lower.rs` 与 `unit_lower/aggregate.rs`（`lower_aggregate_construction` 及 `validate_aggregate_construction_identity`）中退役 `UnitConstructionTarget::IntrinsicBox` 分支，显式返回 `UnsupportedNode`。
3. **退役 `unit_lower/field_borrow.rs` 的 `value class` 字段借用与 `unit_lower/control.rs` 的通用 `when` 链**：
   - 移除 `unit_lower/field_borrow.rs` 中的 `read_borrowed_inline_field`、`borrowed_inline_parameter`、`field_is_result_source` 及 `lower_borrowed_field_loan` 的 `NominalKind::ValueClass` 分支（约 176 行），并在 `unit_lower/aggregate.rs` 移除 `read_borrowed_inline_field` 调用；
   - 移除 `unit_lower/control.rs` 中的 `BooleanWhenArm`、`BooleanWhenArms`、`lower_exhaustive_boolean_arm`、`lower_when_chain` 与 `lower_when_condition`（约 476 行，使 `unit_lower/control.rs` 从 844 行降至 396 行），仅保留 `nullable_when` 入口及供 `nullable_when` 内部嵌套双分支布尔字面量 `when` 复用的 `boolean_literal_two_arms`（委托至 `lower_conditional`），其余 `Expression::When` 在 `unit_lower` 显式返回 `UnsupportedNode`。

## 2. 边界与不变量

- `unit_lower/field_borrow.rs` 中普通 `class` 的 `lower_owned_field_borrow_place` 与 `lower_owned_shared_field_place` 仍保留，供实参同时包含借用 `class` 字段与控制流退出（如 `read(holder.text, if (flag) { return } else { ... })`）的回退路径使用。
- 所有合法 `String.clone()`、`Box(payload)` 构造、`value class` 字段借用与通用 `when` 表达式经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode`。

## 3. 验收标准

- [x] 补齐 `crates/lang-codegen/src/ssa/refine/{subset,build}/{scalar_text.rs,callable.rs}` 与 `refine/subset/range_borrow.rs`，删除 `crates/lang-codegen/src/ssa/unit_lower/string_clone.rs`，并精简 `crates/lang-codegen/src/ssa/unit_lower/{aggregate.rs,field_borrow.rs,control.rs,borrow_result.rs}` 与 `unit_lower.rs`（净减约 903 行 legacy 代码，`unit_lower/control.rs` 从 844 行降至 396 行）。
- [x] 新增 `ssa::refine_subset_tests::string_clone_value_field_borrow_box_and_when_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_string_clone_box_value_field_borrow_and_when_boundary_tests` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。
