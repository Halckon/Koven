# SPEC-0526：F6 第二十六片 `boundary` 起始集层级、`NameContext` 停止一致性与修饰符/恢复映射双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十六片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0525](0525-f6-core-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0525](0525-f6-core-machine-proof.md)，
对 [boundary.rs](../../../crates/lang-frontend/src/parser/engine/boundary.rs) 中此前未被 [SPEC-0508](../../archive/specs/0508-f6-stops-lattice-machine-proof.md) 覆盖的核心纯逻辑——词法起始集包含与互斥层级、`NameContext::is_stop_kind` 与 `NameContext::recovery_stops` 双向一致性、`declaration_modifier_start` 三路可选偏移最小值、`DeclarationStops::keeps_unproved_angle_comma` 以及 `TypeStops::supertype_element_recovery` 归属映射——交付 **Lean 4（模型定理：1 条 `unbounded` + 4 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核共享与零行为变化**：
   - 在 [boundary.rs](../../../crates/lang-frontend/src/parser/engine/boundary.rs) 中将 `NameContext::is_stop` 委托给共享纯函数 `NameContext::is_stop_kind(self, kind: LexemeKind)`，将 `DeclarationStops::contains_hard`/`contains_soft` 委托给 `contains_hard_kind`/`contains_soft_kind`，并将 `declaration_modifier_start` 改用纯栈值辅助函数 `opt_min_usize`（零堆分配、零迭代分配），解析行为与公开 Rust API 零变化。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod boundary_kani;` 与 `#[cfg(test)] mod boundary_tests;`，并在 [boundary_tests.rs](../../../crates/lang-frontend/src/parser/engine/boundary_tests.rs) 补充 4 个单元测试（含生产入口 `parse_declaration` 对修饰符顺序与 `NameContext` 缺失名称恢复的端到端验证）。
2. **轨道 1（Lean 4 模型定理：1 条 `unbounded` + 4 条 `complete finite domain`）**：
   - [formal/parser/f6_boundary/F6Boundary.lean](../../../formal/parser/f6_boundary/F6Boundary.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（B1–B5）：
     - 有限域模型定理（4 条，`proved (model, complete finite domain)`）：
       - `starterHierarchy_and_disjointness_spec`（B1：分类器/简单声明/文件构造/类字段/类成员起始集包含链及与控制表达式起始集严格互斥）
       - `nameContext_isStop_iff_recoveryStops_spec`（B2：全部 4 种 `NameContext` 下 `is_stop` 与 `recovery_stops` 精确等价）
       - `keepsUnprovedAngleComma_spec`（B4：未证明尖括号逗号保留守卫充要条件与委托恢复无条件保留）
       - `supertypeElementRecovery_owner_and_flags_spec`（B5：基类型元素恢复恒含基础停止集、合并 `LEFT_BRACE | CLASS_BODY` 且按 `ClassifierHeaderOwner` 互斥设置 `CLASS_FILE` / `CLASS_MEMBER`）
     - 无界模型定理（1 条，`proved (model, unbounded)`）：
       - `declarationModifierStart_spec`（B3：任意三路 `Option Nat` 修饰符起点下，`none` 当且仅当三路皆空，否则精确等于所有非空偏移的最小值且必取自其中之一）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [boundary_kani.rs](../../../crates/lang-frontend/src/parser/engine/boundary_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_boundary_starter_hierarchy_and_disjointness_exhaustive`：通过单源 `#[deny(unreachable_patterns)]` 覆盖全部生产 `LexemeKind` 变体，用独立预期集合核对全部 6 个起始集谓词及其包含/互斥定律。
     - `verify_name_context_stop_and_recovery_alignment_exhaustive`：全量覆盖 4 种 `NameContext` × 全部生产 `LexemeKind`，直接验证 `NameContext::is_stop_kind` 与 `recovery_stops()` 命中完全一致。
     - `verify_modifier_start_min_and_angle_comma_and_supertype_recovery`：经由生产 `declaration_modifier_start` 入口验证三路可选修饰符起点最小值，并穷尽验证 `DeclarationStops::keeps_unproved_angle_comma` 与 `TypeStops::supertype_element_recovery`。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_boundary/**` 与 `crates/lang-frontend/src/parser/engine/boundary_kani.rs`，统一校验全部 **26 份 F6 Lean 4 证明工程（162 条定理）与 71 个 Kani harness**。

## 2. 检查点

- [x] `F6Boundary.lean` 零告警构建与 5 条定理核心公理白名单本机通过（仅依赖 `propext`、`Classical.choice`、`Quot.sound`）
- [x] `verify_boundary_starter_hierarchy_and_disjointness_exhaustive`、`verify_name_context_stop_and_recovery_alignment_exhaustive` 与 `verify_modifier_start_min_and_angle_comma_and_supertype_recovery` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`boundary_tests`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（26 片 / 162 定理 / 71 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
