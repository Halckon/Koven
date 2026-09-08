# SPEC-0202：nullable `when` 剩余域 typed facts

> **性质**：实施 Spec · **状态**：done · **读取时机**：实施或评审 v0.35 对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-202` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.35 空安全](../../guide/09-nullability-errors.md)与[阶段边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-09-08 用户明确启用 v0.35，按持续推进 Goal 分阶段实施；依赖未完成者保持 draft |
| 前置 Spec | SPEC-0019、0021 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` type checking flow model/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 typed flow 契约 |

## 2. Goal

完成后，type checker 为 nullable `when` 发布稳定 subject、逐 entry 剩余域、alternative 事实
交集与 body non-null typed plan，后续阶段无需重新解释 AST 条件顺序。

## 3. 范围与需求

- subject 精确检查一次；只有稳定 subject binding 可获得源码可见 smart-cast identity，temporary
  仍保留内部单次求值 identity。
- explicit `null` condition 发布 match-null/fallthrough-non-null edge；后续 entry 与最终 `else`
  接收前序 condition 的剩余域。
- comma alternatives 分别检查，body 只保留所有可达 match edge 的事实交集；与既有 enum/
  Boolean coverage、branch join、trial rollback 组合。
- descriptor 以封闭 category 区分 owned root、Borrow/Inout root、ordinary field、container
  element 与 temporary；发布 facts 而不把 flow plan 写回 AST，诊断继续复用 L0108–L0112。

## 4. 非目标

- 不检查 move/loan/drop，不生成 SSA/LLVM；不实现 `!!`、Elvis、safe call 或 inline nullable ABI。

## 5. 验收标准

- [x] field/element proof 仅绑定内部单次求值 identity；后续重新读取同一表达式不自动收窄。

- [x] nullable enum/Boolean 的 null-first、null-last、else 与多 entry 剩余域正反矩阵通过。
- [x] `Node?`/`Box<V>?`/`Rc<T>?` 的 `null`+`else` 覆盖 owned root、temporary、Borrow/Inout 与
  field/element category；plan 明确标识 ADR-0017 owner-native eligible subset。
- [x] mixed comma alternatives 不获得错误 non-null fact；全 non-null alternatives 获得共同 fact。
- [x] temporary 单次求值、stable binding、mutation/call invalidation、branch join 与 trial rollback 稳定。
- [x] descriptor identity/Span/顺序确定，既有 L0108–L0112 与 type suite 回归。
- [x] Architecture 与实现事实同步。

## 6. 技术方案与边界

扩展现有 `when` flow checker 与 typed product，复用 closed-domain、flow key 和 trial snapshot；
不在 ownership/codegen 中复制 coverage 算法。

## 7. 实施计划

1. [x] 建立 nullable remaining-domain descriptor → 验证：纯 model/rollback 测试。
2. [x] 接入 `when` entry/alternative/body flow → 验证：type-checking 正反矩阵。
3. [x] 同步验收与 Architecture → 验证：按[分层验收](../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、测试与完成文档 | `feat(frontend): plan nullable when flow (SPEC-0202)` |

## 9. 未决问题

- 无语义未决项；R1–R3 已随 v0.35 启用，前置依赖见元数据。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | 现有 type checker 有 nullable coverage，但未发布 null fallthrough/else remaining-domain plan |

| `cargo test -p lang-frontend --test type_checking nullable_when_ -- --list` | 命中 2 项 | 仅定向目标 |
| `cargo test -p lang-frontend --test type_checking nullable_when_` | 1 通过、1 预期失败；51 filtered、0 ignored | 2026-09-08：else 的 x 仍为 Int?，L0084；mixed alternatives 的 L0084/x Span 反例通过；生产实现尚未修改 |

回归设计已独立审查：Parser/名称检查先通过，失败来自目标类型契约；随后完成生产修复、typed descriptor、
trial rollback 和 category/identity 矩阵；最终证据见下表。

### 本次实现与定向证据（2026-09-08）

本次沿 SPEC-0019/0021 的单文件 `TypedFile` 链实施；compilation-unit 尚无等价 descriptor，
不据此宣布跨文件或 native 支持。无 LLVM、所有权或依赖变更。

| 验收项 | 直接测试（`type_checking` 目标） |
|---|---|
| field/element 身份与 native eligibility | `nullable_when_does_not_smart_cast_repeated_field_read`、`nullable_when_subject_categories_bound_native_eligibility` |
| nullable enum/Boolean 顺序与剩余域 | `nullable_when_remaining_domain_covers_boolean_and_enum_orders`、`nullable_when_remaining_single_case_keeps_payload_refinement` |
| mixed alternatives 与共同非空事实 | `nullable_when_mixed_alternatives_do_not_narrow_body`、上述顺序矩阵 |
| subject 一次求值、计划稳定 | `nullable_when_plans_have_stable_identity_and_single_subject_evaluation`、`nullable_when_domains_encode_null_match_and_non_null_fallthrough` |
| mutation/call/capture 失效与分支隔离 | `nullable_when_condition_mutation_invalidates_source_proof`、`nullable_when_inout_condition_call_invalidates_subject_binding`、`nullable_when_captured_mutable_binding_is_not_a_stable_source`、`nullable_when_body_mutation_does_not_leak_to_unselected_entry` |
| trial rollback | `nullable_when_failed_overload_trials_do_not_publish_plans`、`nullable_when_successful_overload_trial_publishes_only_selected_plan` |
| 相邻 flow 与诊断 | 两项 `nullable_when_identity_does_not_interfere_*`；完整 `type_checking` 中既有 when/type-test/诊断测试 |

独立审查发现并修复两项回归：identity marker 干扰后继/嵌套 if 的 refinement；剩余域单 enum
case 未保留 payload refinement。两者均先记录实际失败，再修复并通过回归，修复已复审。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test type_checking` | 67 通过；0 filtered/ignored | 包含 16 项 nullable_when 测试 |
| `cargo test -p lang-frontend --test type_checking --test type_callable` | callable 18 通过、1 失败 | Cargo 在 callable 失败后未运行 type_checking；后者已单独运行 |
| 基线 `ad02ae0`：`cargo test -p lang-frontend --test type_callable ambiguous_and_failed_overload_lambda_trials_leak_no_candidate_facts -- --exact` | 相同失败；18 filtered | 既有断言期望 symbol_type 为 None，实际 Error TypeId；不修改无关行为或把此项称为通过 |

未运行 frontend 全量测试。验证基线使用临时源码快照和同一 target；恢复当前源码后强制重编译
frontend，避免复用基线库产物。最终检查如下。

| 最终检查 | 结果 | 范围 |
|---|---|---|
| `cargo fmt --all -- --check` | 通过 | 格式 |
| `cargo clippy -p lang-frontend --lib --test type_checking --test type_callable -- -D warnings` | 通过 | 受影响库与测试目标 |
| `cargo check --workspace --all-targets` | 通过；7m31s | 跨 crate 编译兼容；不执行测试 |
| `python3 scripts/check_docs.py`、`git diff --check` | 通过 | 状态迁移后重新检查 |

完成范围是单文件 Phase 2 合同。已知 callable 基线失败保持明确记录，不将其作为本 Spec 引入的
回归，也不宣称相关套件全绿。后续由 SPEC-0203 消费计划检查所有权；本 Spec 不发布 owner/drop。
