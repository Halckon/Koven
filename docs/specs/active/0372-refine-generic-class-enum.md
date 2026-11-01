# SPEC-0372：泛型 `class` 与泛型 Copyable `enum class` 迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的泛型 `class` 与泛型 Copyable `enum class` 细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0372` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 类型与泛型](../../guide/03-types-generics.md)、[类族与成员](../../guide/08-class-family-members.md) |
| 批准依据 | 用户确认继续推进 E3 所有权细化层下一片 |
| 基线 | `feature/spec-0371-refine-integer-range-text` `ac1004be` |
| 分支 | `feature/spec-0372-refine-generic-class-enum` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0371](0371-refine-integer-range-text.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将具体实例化后字段为标量或 `String` 的泛型 `class`（含构造、字段读取、字段置换 `owner.f = v`、`Borrow`/`Inout` 参数与 receiver 调用、可空 `Class<T>?` 视图与 `!!`）以及具体实例化后 payload 全为标量的泛型 Copyable `enum class`（含 case 构造、`when` 与表达式 `is`/`!is` 测试、窄化 payload 投影及 enum 方法 receiver）迁入 `ssa::refine`，消除细化层对非泛型标量/`String` 名义类型的限制。

## 2. 实施内容

1. **泛型 `class` 具体化准入与分流（`refine/subset.rs`、`refine/subset/heap.rs`）**：
   - 放开 `Checker::class_signature` 与 `class_fields` 的非泛型限制，消费 `resolve_nominal_runtime_field_types(typed, ty, nominal, &arguments)` 解析具体实例字段类型。
   - 在 `subset/heap.rs` 的 `construction` 与 `projection` 中校验具体化后的 `instance().type_arguments()`，并在触及带类型实参的泛型 `class` 时标记 `migrated = true`。
2. **泛型 Copyable `enum class` 具体化准入与构建（`refine/subset.rs`、`refine/subset/tagged.rs`、`refine/build/tagged.rs`）**：
   - 在 `subset.rs` 的 `type_demands` 中对 `UnitTypeKind::EnumCase { root, .. }` 先提取 `root` 再具体化，保证窄化表达式的根枚举类型进入 SSA 类型表。
   - 在 `subset/tagged.rs` 中放开 `enum_signature` 与 `enum_construction` 的非泛型限制，对 `case_of_type` 与 `enum_projection` 的 `EnumCase` 根类型做 `self.c(...)` 具体化，并在触及泛型 `enum class` 时标记 `migrated = true`。
   - 在 `build/tagged.rs` 的 `type_test`、`is_enum_projection` 与 `enum_projection` 中对 `EnumCase` 的 `root` 做具体化匹配。

## 3. 验收标准

- [x] `refine_tests::generic_classes_and_enums_lower_in_refine` 验证泛型 `class`（构造、方法读取、`inout` 方法修改、直接字段读取与置换）及泛型 Copyable `enum class`（case 构造、方法内 `this is Case`、`when` `is` 分支与多字段窄化 payload 读取）全部经 `ssa::refine` lowering 并通过原生计数执行验证。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
