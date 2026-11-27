# SPEC-0512：F6 第十二片 `WhenEntryBody` / `SelectorScanState` 换行新选择器与 `if`/`else` 平衡双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十二片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@d44c8b9a`（合并同步自 `main@e339b881` 并重编号自原 0511） |
| 分支 | `feature/spec-0511-f6-when-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0510](../../archive/specs/0510-f6-lexical-owner-events-machine-proof.md)（编号 `0512` 避让主干已合入的 [SPEC-0511](0511-lsp-workspace-manifest-and-diagnostic-context.md)），
对 [when_boundary.rs](../../../crates/lang-frontend/src/parser/engine/when_boundary.rs) 中 `selector_arrow_after_group` 的换行新选择器判定与 `if`/`else` 平衡状态机内核交付 **Lean 4（无界状态与任意长度事件流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配状态机核抽取与零行为变化**：
   - 在 [when_boundary.rs](../../../crates/lang-frontend/src/parser/engine/when_boundary.rs) 中将原内联布尔/计数器逻辑提炼为纯栈值状态机核（`SelectorScanState`、`SelectorEvent`、`SelectorStep`、`is_new_selector_after_line_break`、`SelectorScanState::step`、`SelectorScanState::step_non_operand_control`），`selector_arrow_after_group` 直接调用该状态机核，保持原有短路求值与解析行为零变化，并补充针对 subject 敏感性与 `if`/`else` 平衡的单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中仅挂载 `#[cfg(kani)] mod when_boundary_kani;`。
2. **轨道 1（Lean 4 无界模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_when_boundary/F6WhenBoundary.lean](../../../formal/parser/f6_when_boundary/F6WhenBoundary.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 7 条无界定理（W1–W7）：
     - `isNewSelectorAfterLineBreak_spec`（W1：`LeftParen`/`LeftBrace`/`Plus`/`Minus` 恒真，`Is`/`In`/`BangIs`/`BangIn` 仅在 `withSubject` 为真）
     - `step_arrow_iff`（W2：`.arrow` 接受当且仅当 `needsOperand = false ∧ pendingElse = 0`）
     - `step_else_exact`（W3：`else` 仅在 `!needsOperand ∧ pendingElse > 0` 时减 1 且不发生下溢，否则拒绝）
     - `step_if_exact`（W4：`if` 仅在 `needsOperand = true` 时递增 `pendingElse` 并保持 `needsOperand = true`，否则拒绝）
     - `stepNonOperandControl_spec`（W5：非操作数控制分支在 `needsOperand = true` 返回 `none`，换行新选择器时拒绝）
     - `runPrefix_pendingElse_balance`（W6：任意长度可继续事件前缀上精确满足 `finalState.pendingElse + countElse pre = state.pendingElse + countIf pre`）
     - `runEventsFrom_true_iff_arrow_balanced`（W7：任意长度事件流接受当且仅当存在前置可继续前缀到达 `needsOperand = false ∧ pendingElse = 0` 状态的 `.arrow` 事件；EOF 永不接受）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [when_boundary_kani.rs](../../../crates/lang-frontend/src/parser/engine/when_boundary_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_when_boundary_new_selector_and_control`：直接调用生产 `is_new_selector_after_line_break` 与 `SelectorScanState::step_non_operand_control`，通过 `any_token_kind()` 与编译期无通配符穷举见证 `assert_token_kind_exhaustive` 符号覆盖全部 `WhenEntryBody`、生产 `TokenKind` 的全部变体（含全部 36 个 `Keyword`、43 个 `Symbol`、11 个 `ReservedWord` 及字面量后缀）、任意 `has_line_break`、任意 `needs_operand` 与任意 `pending_else: usize`。
     - `verify_when_boundary_step_invariants`：直接调用生产 `SelectorScanState::step`，符号覆盖任意 `SelectorScanState`（含 `pending_else = 0` 下溢边界与 `usize::MAX` 上溢边界）与全部 10 种 `SelectorEvent`。
     - `verify_when_boundary_run_events_balance_len_6`：直接调用 `run_selector_events`（驱动生产 `SelectorScanState::step`），符号覆盖长度 `0..=6` 的任意 `SelectorEvent` 序列，逐拍核对 `pending_else + else_count == if_count` 与仅在平衡 `.arrow` 处接受。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_when_boundary/**`、`crates/lang-frontend/src/parser/engine/when_boundary.rs`、`crates/lang-frontend/src/parser/engine/when_boundary_kani.rs` 与 `crates/lang-frontend/src/lexer/mod.rs`，并接入 `F6WhenBoundary` 构建、核心公理白名单检查与上述 3 个 Kani harness。

## 2. 检查点

- [x] `F6WhenBoundary.lean` 零告警构建与 7 条定理核心公理白名单本机通过（仅依赖 `propext`、`Quot.sound`）
- [x] `verify_when_boundary_new_selector_and_control`、`verify_when_boundary_step_invariants` 与 `verify_when_boundary_run_events_balance_len_6` 本机 Kani 0.68.0 实跑通过，`when_boundary` 单测与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 与主 CI 实跑验收
