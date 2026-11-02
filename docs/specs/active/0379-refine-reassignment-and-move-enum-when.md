# SPEC-0379：MoveOnly 局部可变重赋值与 MoveOnly 枚举 `when` 迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 MoveOnly 局部 `var` 重赋值、通用 MoveOnly `enum class` 构造与 `when` 分支判别/载荷借用细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0379` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认合入 `main`、修复 E3 审查问题后继续推进 E3 下一步工作 |
| 基线 | `feature/spec-0378-refine-class-fields-rc-null` `dd474036` |
| 分支 | `feature/spec-0379-refine-reassignment-nullable-move-when` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0378](0378-refine-class-fields-rc-and-null-views.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 MoveOnly 局部可变绑定重赋值（如 `var s: String? = null; s = "a"`、`var cur: String = a; cur = b`）以及携带任意受支持 `storage_type` 负载的 MoveOnly `enum class`（含非泛型与泛型实例的构造、`is`/`!is` 判别、`when (subject)` 分支匹配、标量载荷投影与 MoveOnly 载荷借用实参）迁入所有权细化层（`ssa::refine`）。

## 2. 实施内容

1. **MoveOnly 局部可变绑定重赋值（`refine/subset.rs`、`refine/build/expression.rs`）**：
   - `Checker::assignment` 放开局部 `var` 绑定的类型准入为任意 `storage_type`（含 `String`、`String?`、容器、`Rc<T>`、MoveOnly `enum class` 等），并在重赋值后清除该符号在 `view_symbols` 中的窄化证明视图状态。
   - `Builder::assignment` 在写入新值前调用 `end_views_of(symbol)` 结束该符号已激活的证明视图，消费前端 planner 在赋值表达式右侧求值完毕、覆写旧值前发布的 `AfterExpression(value)`（即重赋覆写前旧值析构）清理事实（若旧值仍存活），并通过 `self.dead.remove(&symbol)` 重新激活该槽位的生命周期追踪，使新写入的 MoveOnly owner 在后续退出或覆写点被正常释放。
2. **通用 MoveOnly `enum class` 准入与标量载荷投影（`refine/subset/tagged.rs`、`refine/build/tagged.rs`）**：
   - 将 `box_enum_type_seen` 扩展为接纳任意载荷满足 `storage_type_seen` 的 MoveOnly `enum class`（如 `enum class Tag { Named(n: String), Empty }`、`enum class Reply<T> { Full(value: T, code: Int), Empty }`），保留 `active` 集合防递归死循环。
   - 允许在 `is Case` 窄化后对 MoveOnly `enum class` 具名绑定读取 Copyable 标量载荷字段（如 `item.code`），经 `ROp::TaggedPayloadPlace` 与 `ROp::SharedFieldLoan` 读取。
3. **MoveOnly `enum class` 的 `when` subject 判别与 MoveOnly 载荷借用（`refine/subset/when.rs`、`refine/build/when.rs`、`refine/subset/callable.rs`、`refine/build/callable.rs`）**：
   - `Checker::when_expression` 与 `when_condition` 新增 `move_enum_when_subject` 准入，允许具名 MoveOnly `enum class` 作为 `when` 的 subject 执行 `is`/`!is` 分支匹配。
   - 当 `when (subject)` 分支体不再引用 `subject` 时，前端在 `UnitDropPoint::AfterExpression(subject)` 发布 `subject` 的清理事实；`Builder::when_subject` 在发射 `AfterExpression(subject)` 清理前先提取 `ROp::TaggedDiscriminant` 保存为 `WhenSubject::Tag(Slot)`，供后续分支比较使用。
   - 语句位置的完备 `when`（`expression_falls_through(control) == Some(false)`）在无 `else` 时的落空块直接发射 `RTerm::Abort { kind: UnmatchedWhen }`，避免为已由各分支消费完毕的 MoveOnly owner 合成不可达的后继坠落边。
   - `borrowed_string_field` 与 `borrowed_string_field_loan` 扩展支持已窄化为具体 case 的 MoveOnly `enum class` 具名绑定，经 `ROp::TaggedPayloadPlace` -> `ROp::FieldPlace` -> `ROp::BorrowBegin` 将 MoveOnly 载荷字段（如 `Reply.Full.value`）借出给 `Borrow` 调用实参。

## 3. 验收标准

- [x] `refine_subset_tests`（3/3）与 `refine_tests`（含 `move_only_reassignment_and_move_enum_when_lower_in_refine`）全部通过。
- [x] `unit_lower_enum_tests`、`unit_field_borrow_tests`、`lower_frontend_nullable_unit_comparison_tests` 与 `unit_lower_control_result_tests` 全部保持通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
