# SPEC-0386：`Inout` 接收者、接口委托、无状态 `object` 与局部解构旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `Inout` 接收者方法体与字段替换（`unit_lower/assignment.rs`、`unit_lower/aggregate.rs`、`unit_lower/receiver.rs`、`unit_lower/orchestrate.rs`）、接口 `by` 委托调用（`lower_delegated_call_receiver`）、无状态 `object` 单例接收者（`lower_stateless_object_receiver`）与局部解构（`lower_destructuring`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0386` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0385 基线上继续推进 E3 下一个切片 |
| 基线 | `09edda45`（SPEC-0385） |
| 分支 | `feature/spec-0386-refine-inout-delegation-object-destructuring-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0358](../../archive/specs/0358-refine-assignment-inout.md)、[SPEC-0373](0373-refine-destructuring-range-for.md)、[SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md)、[SPEC-0385](0385-refine-deinit-borrow-heap-nullable-if-string-field-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0358](../../archive/specs/0358-refine-assignment-inout.md)、[SPEC-0373](0373-refine-destructuring-range-for.md) 与 [SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md) 已将 `Inout` 接收者方法体（`inout fun`）、接口 `by` 委托链、无状态 `object` 单例及 `value class` 局部解构迁入 `ssa::refine` 的基础上，补齐 `ssa::refine` 括号接收者与具名委托/可变接收者调用准入，并彻底退役 `unit_lower` 中已被细化层完全取代的 legacy 实现分支（合计再清理约 558 行旧路径代码，使 `unit_lower/receiver.rs` 从 992 行降至 693 行）：

1. **补齐 `ssa::refine` 接收者准入与括号解包**：
   - 在 `refine/subset/receiver.rs` 的 `owner_type` 与 `refine/build/place.rs` 的 `owner_loan` 中对 `Owner::Expression` / `OwnerRef::Expression` 执行括号解包（`ungrouped`），使 `(this).field = v` 等括号包裹的接收者字段赋值与访问完整纳入 `ssa::refine`；
   - 在 `refine/subset/receiver.rs` 的 `receiver_call` 中将具名 `Place` 上的 `ParameterMode::Inout` 接收者调用及非空 `site.delegation()` 接口委托调用标记为 `self.migrated |= ok`，确保所有常规 `Inout` 成员调用与 `by` 委托调用默认路由至 `ssa::refine`。
2. **退役 `unit_lower` 中的 `Inout` 接收者方法体与字段替换分支**：
   - 在 `unit_lower/orchestrate.rs` 中将 `plan.receiver.filter(|r| r.mode == ParameterMode::Inout)` 与 `plan.inout_parameter` 一并作为细化层专属签名，在 legacy 回退入口直接返回 `UnsupportedNode`；
   - 移除 `unit_lower/assignment.rs` 中的 `lower_current_receiver_field_assignment`、`validate_field_replacement_drop` 与 `CurrentFieldOperation`（约 160 行）；
   - 移除 `unit_lower/aggregate.rs` 中 `current_receiver_field` 与 `lower_this_value_field` 的 `ParameterMode::Inout` / `LoanKind::Exclusive` 分支，以及 `unit_lower/receiver.rs` 中 `lower_existing_this_receiver`、`lower_this_borrow_argument` 与 `begin_shared_reborrow` 的 `Inout` `this` 分支。
3. **退役 `unit_lower/receiver.rs`、`unit_lower/call.rs` 与 `unit_lower/aggregate.rs` 中的接口委托、无状态 `object` 接收者与局部解构分支**：
   - 移除 `unit_lower/receiver.rs` 中的 `lower_delegated_call_receiver`、`lower_stateless_object_receiver` 与 `stateless_object_declaration`（约 193 行），并在 `unit_lower/call.rs` 对非空 `resolved.delegation()` 显式返回 `UnsupportedNode`；
   - 移除 `unit_lower/aggregate.rs` 中的 `lower_destructuring`（约 40 行），并在 `unit_lower.rs` 对 `Statement::LocalDestructuring` 显式返回 `UnsupportedNode`。

## 2. 边界与不变量

- `unit_lower/receiver.rs` 与 `unit_lower/call.rs` 中调用方侧的 `ExclusiveLoan` 预留、激活与 `ReceiverWriteback` 仍保留，供含实参控制流转移（如 `host.update(p.TEXT, if (flag) { return } else { ... })`）且携带借用常量临时量的 `unit_constant_tests` 回退路径使用。
- 所有合法 `inout fun` 方法体、接口 `by` 委托调用、无状态 `object` 接收者调用与 `val (a, b) = ...` 局部解构经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode` / `MissingFact`。

## 3. 验收标准

- [x] 补齐 `crates/lang-codegen/src/ssa/refine/subset/receiver.rs` 与 `crates/lang-codegen/src/ssa/refine/build/place.rs`，并精简 `crates/lang-codegen/src/ssa/unit_lower/{receiver.rs,call.rs,assignment.rs,aggregate.rs,orchestrate.rs}` 与 `unit_lower.rs`（净减约 558 行 legacy 代码，`unit_lower/receiver.rs` 从 992 行降至 693 行）。
- [x] 新增 `ssa::refine_subset_tests::inout_receiver_delegation_object_and_destructuring_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_inout_receiver_delegation_object_and_destructuring_boundary_tests` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。
