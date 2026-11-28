# SPEC-0521：F6 第二十一片 `declaration` 常量恢复、函数返回分派、参数模式前瞻与值参数守卫双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十一片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0520](0520-f6-borrow-result-and-head-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0520](0520-f6-borrow-result-and-head-machine-proof.md)，
对 [declaration.rs](../../../crates/lang-frontend/src/parser/engine/declaration.rs) 中的 `const` 缺失 `val` 四路恢复分派、函数返回形式五路分派、参数模式前瞻尾判定、值参数类型位置三路分派与默认实参防重报守卫、以及声明尾部诊断守卫交付 **Lean 4（模型定理：5 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [declaration.rs](../../../crates/lang-frontend/src/parser/engine/declaration.rs) 中提炼纯栈值内核（`ConstValRecoveryAction`、`const_val_recovery_action`、`FunctionFormDispatch`、`function_form_dispatch`、`is_parameter_mode_follow_kind`、`ValueParamTypeAction`、`value_param_type_action`、`value_param_default_emits_diagnostic` 与 `declaration_tail_emits_unexpected_trailing`），接入 `parse_constant_declaration`、`parse_function_form`、`current_is_parameter_mode`、`parse_value_parameters` 与 `consume_declaration_tail`，严格保留原有短路求值（`:`/`=`/`{` 短路 `can_start_type_ref`，非模式关键字短路 `peek(1)`，EOF 与非隐式函数短路尾毒化判定）与解析行为零变化。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod declaration_kani;` 与 `#[cfg(test)] mod declaration_tests;`，并在 [declaration_tests.rs](../../../crates/lang-frontend/src/parser/engine/declaration_tests.rs) 补充 3 个单元测试。
2. **轨道 1（Lean 4 模型定理：5 条 `complete finite domain`）**：
   - [formal/parser/f6_declaration/F6Declaration.lean](../../../formal/parser/f6_declaration/F6Declaration.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条有限域模型定理（D1–D5，`proved (model, complete finite domain)`）：
     - `constValRecoveryAction_spec`（D1：`const` 缺失 `val` 时的四路恢复分派充要条件）
     - `functionFormDispatch_spec`（D2：函数返回形式五路分派充要条件，`:`/`=`/`{` 严格优先于 `canStartTypeRef`）
     - `parameterModeFollow_spec`（D3：`NamedParameter` 仅接受标识符、`FunctionType` 接受标识符或 `(`）
     - `valueParamTypeAction_and_defaultGuard_spec`（D4：值参数类型位置三路分派充要条件，以及 `fun f(x = 1)` 防重报默认实参诊断精确守卫）
     - `declarationTailEmits_spec`（D5：声明尾部仅在 EOF 或隐式 Unit 函数尾毒化时抑制 `UNEXPECTED_TRAILING_TOKEN`）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [declaration_kani.rs](../../../crates/lang-frontend/src/parser/engine/declaration_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_const_val_recovery_and_function_form_dispatch`：直接调用生产 `const_val_recovery_action` 与 `function_form_dispatch`，通过单源自动计数 `#[deny(unreachable_patterns)]` 宏覆盖全部生产 `LexemeKind` 变体与独立预期分类。
     - `verify_parameter_mode_follow_and_value_param_guards`：直接调用生产 `is_parameter_mode_follow_kind`、`value_param_type_action` 与 `value_param_default_emits_diagnostic`，穷举全部 `ParameterModeContext`、`LexemeKind` 与布尔组合。
     - `verify_declaration_tail_guard_exhaustive`：直接调用生产 `declaration_tail_emits_unexpected_trailing`，穷举全部 `LexemeKind` 与布尔组合。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_declaration/**`、`crates/lang-frontend/src/parser/engine/declaration.rs` 与 `crates/lang-frontend/src/parser/engine/declaration_kani.rs`，统一校验全部 **21 份 F6 Lean 4 证明工程（137 条定理）与 56 个 Kani harness**。

## 2. 检查点

- [x] `F6Declaration.lean` 零告警构建与 5 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_const_val_recovery_and_function_form_dispatch`、`verify_parameter_mode_follow_and_value_param_guards` 与 `verify_declaration_tail_guard_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`declaration_tests`（3/3）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（21 片 / 137 定理 / 56 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
