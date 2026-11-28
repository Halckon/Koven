# SPEC-0524：F6 第二十四片 `class` 分类器策略、成员上下文规则、名称/枚举边界与非法前缀双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十四片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0523](0523-f6-expression-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0523](0523-f6-expression-machine-proof.md)，
对 [class.rs](../../../crates/lang-frontend/src/parser/engine/class.rs)（纯内核位于 [class/kernel.rs](../../../crates/lang-frontend/src/parser/engine/class/kernel.rs)）中的分类器构造器/类体/类型参数策略、成员上下文（`ClassMemberContext`）互补与接收者策略、接口函数可见性与重复 `companion` 守卫、分类器名称边界与枚举体停止集合、以及构造器字段/枚举变体参数非法前缀交付 **Lean 4（模型定理：5 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [class/kernel.rs](../../../crates/lang-frontend/src/parser/engine/class/kernel.rs) 中提炼纯栈值内核（`ClassifierKindTag`、`classifier_kind_tag`、`classifier_supports_constructor`、`classifier_requires_constructor`、`classifier_requires_body`、`classifier_forbids_type_parameters`、`is_classifier_name_boundary_kind`、`is_class_field_invalid_prefix_kind`、`is_enum_variant_param_invalid_prefix_kind`、`member_receiver_policy`、`member_visibility_invalid_for_fun`、`member_companion_is_unsupported` 与 `is_enum_body_stop_kind`），直接接入 [class.rs](../../../crates/lang-frontend/src/parser/engine/class.rs) 的 `parse_classifier_declaration`、`parse_classifier_name`、`parse_class_field`、`parse_classifier_member`、`parse_enum_body` 与 `parse_enum_variant_parameter`，严格保留原有短路求值（非模式标识符短路 `peek(1)`）与解析行为零变化，且 `class.rs` 物理行数保持低于 1000 行门禁。
   - 在 [class.rs](../../../crates/lang-frontend/src/parser/engine/class.rs) 中挂载 `mod kernel;`、`#[cfg(kani)] mod kernel_kani;` 与 `#[cfg(test)] mod kernel_tests;`，并在 [class/kernel_tests.rs](../../../crates/lang-frontend/src/parser/engine/class/kernel_tests.rs) 补充 4 个单元测试（含生产入口 `parse_declaration` 对 `value class` 缺失构造器、`object` 泛型参数、`enum class` 缺失类体、`interface` 私有函数与重复 `companion` 的端到端验证）。
2. **轨道 1（Lean 4 模型定理：5 条 `complete finite domain`）**：
   - [formal/parser/f6_class/F6Class.lean](../../../formal/parser/f6_class/F6Class.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条有限域模型定理（C1–C5，`proved (model, complete finite domain)`）：
     - `classifierKindPolicy_spec`（C1：5 种分类器的构造器/类体/类型参数策略充要条件，且 `requiresConstructor → supportsConstructor`）
     - `classMemberContext_policy_spec`（C2：6 种成员上下文的 `allowsCompanion ↔ ¬allowsConstant` 互补律、`allowsDeinit → allowsOverride ∧ allowsCompanion` 以及 `memberReceiverPolicy`）
     - `memberModifierAndCompanionGuards_spec`（C3：接口函数禁止 `internal`/`private` 与重复/不支持 `companion` 精确守卫）
     - `classifierNameBoundary_and_enumBodyStop_spec`（C4：分类器名称边界与枚举体停止集合精确充要条件及包含关系）
     - `fieldAndEnumParamInvalidPrefix_spec`（C5：构造器字段与枚举变体参数非法前缀精确充要条件）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [class/kernel_kani.rs](../../../crates/lang-frontend/src/parser/engine/class/kernel_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_classifier_kind_and_member_context_policies_exhaustive`：全量覆盖生产 `ClassifierKindTag`、`ClassMemberContext` 与布尔组合。
     - `verify_classifier_name_boundary_and_enum_body_stop_exhaustive`：全量覆盖生产 `LexemeKind` 与包含关系。
     - `verify_class_field_and_enum_param_invalid_prefix_exhaustive`：全量覆盖生产 `LexemeKind × bool × Option<LexemeKind>`。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_class/**`、`crates/lang-frontend/src/parser/engine/class/kernel.rs` 与 `crates/lang-frontend/src/parser/engine/class/kernel_kani.rs`，统一校验全部 **24 份 F6 Lean 4 证明工程（152 条定理）与 65 个 Kani harness**。

## 2. 检查点

- [x] `F6Class.lean` 零告警构建与 5 条定理核心公理白名单本机通过（零外部公理）
- [x] `verify_classifier_kind_and_member_context_policies_exhaustive`、`verify_classifier_name_boundary_and_enum_body_stop_exhaustive` 与 `verify_class_field_and_enum_param_invalid_prefix_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`class::kernel_tests`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（24 片 / 152 定理 / 65 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
