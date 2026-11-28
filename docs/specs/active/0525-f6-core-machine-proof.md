# SPEC-0525：F6 第二十五片 `core` 递归守卫、逆向有效词法扫描与上下文传播双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十五片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0524](0524-f6-class-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0524](0524-f6-class-machine-proof.md)，
对 [core.rs](../../../crates/lang-frontend/src/parser/engine/core.rs) 中的递归深度守卫（`try_enter_recursion_depth`）、逆向非 trivia 词法终偏移扫描（`previous_significant_end_with`）、`LambdaBody`/`ControlBody` 尾表达式上下文传播（`lambda_body_element_is_statement`、`control_body_element_is_statement`）、值位置 `if` 缺失 `else` 诊断守卫（`if_missing_else_emits_diagnostic`）、表达式尾部块边界与未预期尾随词法守卫（`expression_tail_is_block_boundary`、`expression_tail_emits_unexpected_trailing`）以及边界空 span / 类型恢复边界命中判定（`boundary_uses_empty_span`、`type_recovery_boundary_hit`）交付 **Lean 4（模型定理：3 条 `unbounded` + 2 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [core.rs](../../../crates/lang-frontend/src/parser/engine/core.rs) 中提炼纯栈值内核（`try_enter_recursion_depth`、`previous_significant_end_with`、`lambda_body_element_is_statement`、`control_body_element_is_statement`、`if_missing_else_emits_diagnostic`、`expression_tail_is_block_boundary`、`expression_tail_emits_unexpected_trailing`、`boundary_uses_empty_span` 与 `type_recovery_boundary_hit`），接入 `enter_recursion`、`previous_significant_end`、`validate_context_work`、`consume_expression_tail`、`boundary_span` 与 `at_type_recovery_boundary`，严格保留原有短路求值（不引入额外 `peek()` 或边界查询）与解析行为零变化。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod core_kani;` 与 `#[cfg(test)] mod core_tests;`，并在 [core_tests.rs](../../../crates/lang-frontend/src/parser/engine/core_tests.rs) 补充 4 个单元测试（含生产入口 `parse` / `parse_block` 对值位置与语句位置 `if` 缺失 `else` 以及表达式尾部未预期词法单元的端到端验证）。
2. **轨道 1（Lean 4 模型定理：3 条 `unbounded` + 2 条 `complete finite domain`）**：
   - [formal/parser/f6_core/F6Core.lean](../../../formal/parser/f6_core/F6Core.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（C1–C5）：
     - 无界模型定理（3 条，`proved (model, unbounded)`）：
       - `tryEnterRecursionDepth_spec`（C1：任意自然数深度与上限下，拒绝当且仅当 `depth ≥ maxDepth`，放行当且仅当 `depth < maxDepth` 且推进至 `depth + 1`）
       - `previousSignificantEndRev_spec`（C2：任意长度词法前缀末尾追加非 trivia 返回其终偏移、追加 trivia 保持不变、全为 trivia 返回 0）
       - `bodyElementIsStatement_spec`（C3：任意长度与下标下，`LambdaBody` 与 `ControlBody` 仅在末元素为表达式语句且值需求成立时进入值上下文）
     - 有限域模型定理（2 条，`proved (model, complete finite domain)`）：
       - `ifElse_and_expressionTail_spec`（C4：`if` 缺失 `else` 诊断守卫与表达式尾部未预期尾随词法单元判定充要条件）
       - `boundarySpan_and_typeRecoveryBoundary_spec`（C5：边界空 span 与类型恢复边界 6 路标志充要条件与短路无关性）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [core_kani.rs](../../../crates/lang-frontend/src/parser/engine/core_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_recursion_depth_and_body_context_propagation`：直接调用生产 `try_enter_recursion_depth`、`lambda_body_element_is_statement` 与 `control_body_element_is_statement`。
     - `verify_previous_significant_end_len_6`：直接调用生产共享逆向扫描函数 `previous_significant_end_with`（`#[kani::unwind(8)]`），以独立正向遍历与单源 `#[deny(unreachable_patterns)]` 全生产 `LexemeKind` 枚举覆盖核对长度 `0..=6` 任意输入。
     - `verify_if_else_tail_and_recovery_boundary_guards_exhaustive`：全量覆盖 `if_missing_else_emits_diagnostic`、`expression_tail_is_block_boundary`、`expression_tail_emits_unexpected_trailing`、`boundary_uses_empty_span` 与 `type_recovery_boundary_hit`。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_core/**`、`crates/lang-frontend/src/parser/engine/core.rs` 与 `crates/lang-frontend/src/parser/engine/core_kani.rs`，统一校验全部 **25 份 F6 Lean 4 证明工程（157 条定理）与 68 个 Kani harness**。

## 2. 检查点

- [x] `F6Core.lean` 零告警构建与 5 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_recursion_depth_and_body_context_propagation`、`verify_previous_significant_end_len_6` 与 `verify_if_else_tail_and_recovery_boundary_guards_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`core_tests`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（25 片 / 157 定理 / 68 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
