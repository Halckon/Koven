# SPEC-0516：F6 第十六片 `file` 声明修饰符顺序状态机与文件头/顶层分隔符双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第十六片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0515](0515-f6-postfix-boundary-machine-proof.md) |
| 基线 | `main@f2ea2feb`（叠加于 `SPEC-0515`） |
| 分支 | `feature/spec-0515-f6-postfix-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0515](0515-f6-postfix-boundary-machine-proof.md)，
对 [file.rs](../../../crates/lang-frontend/src/parser/engine/file.rs) 中的声明修饰符顺序状态机（`visibility → override → receiver` 单调推进与无错长度上界）以及文件头/顶层声明分隔符判定内核交付 **Lean 4（无界状态与任意长度修饰符流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [file.rs](../../../crates/lang-frontend/src/parser/engine/file.rs) 中提炼纯栈值内核（`FileHeaderSeparatorDecision`、`FileRootSeparatorDiagnostic`、`ModifierToken`、`ModifierStepOutcome`、`ModifierScanState`、`file_header_separator_decision`、`file_root_separator_diagnostic`、`step_declaration_modifier` 与 `#[cfg(any(test, kani))] run_declaration_modifiers`），由 `consume_file_header_separator`、`parse_file_roots` 与 `parse_declaration_modifiers` 直接调用，严格保留原有短路求值与解析行为零变化，并在 [file_tests.rs](../../../crates/lang-frontend/src/parser/engine/file_tests.rs) 补充 3 个单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod file_kani;` 与 `#[cfg(test)] mod file_tests;`。
2. **轨道 1（Lean 4 模型定理：2 条 `unbounded` + 5 条 `complete finite domain`）**：
   - [formal/parser/f6_file_modifiers/F6FileModifiers.lean](../../../formal/parser/f6_file_modifiers/F6FileModifiers.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 7 条定理（M1–M7）：
     - 有限域模型定理（5 条，`proved (model, complete finite domain)`）：
       - `fileHeaderSeparatorDecision_spec`（M1：文件头分隔符三态 `satisfied`/`consumeSemicolon`/`missing` 的完整充要条件）
       - `fileRootSeparatorDiagnostic_spec`（M2：顶层声明分隔符三态诊断选择的完整充要条件）
       - `stepModifier_spec`（M3：单步修饰符转移 `stop`/`accept`/`invalid` 的精确充要条件）
       - `stepModifier_monotone`（M4：单步转移三标志位 `hasVisibility`/`sawOverride`/`sawReceiver` 均单调不回退）
       - `stepModifier_accept_increases_rank`（M6：每次 `accept` 严格使三位状态秩 `stateRank` 增 1 且不超过 3）
     - 无界列表模型定理（2 条，`proved (model, unbounded)`）：
       - `runModifiers_monotone`（M5：任意长度修饰符流上三标志位均单调不回退）
       - `runModifiers_zero_errors_len_le_three`（M7：无 `.notRecognized` 提前停止且零错误的任意修饰符流长度必 `≤ 3`）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [file_kani.rs](../../../crates/lang-frontend/src/parser/engine/file_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_file_header_and_root_separator_decisions`：直接调用生产 `file_header_separator_decision` 与 `file_root_separator_diagnostic`，由单源自动计数宏（无手写 `count`，`#[deny(unreachable_patterns)]` 穷举 `match`）覆盖全部生产 `LexemeKind` / `TokenKind` 变体与布尔控制位。
     - `verify_declaration_modifier_step_invariants`：直接调用生产 `step_declaration_modifier`，符号覆盖全部 `ReceiverModifierPolicy`、`ModifierToken`、8 种 `ModifierScanState` 与 `allow_override` 组合，核对单调性、`Stop` 与 `Accept` 充要条件。
     - `verify_declaration_modifier_run_len_6`：直接调用 `run_declaration_modifiers`（`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意 `ModifierToken` 序列，验证无错非停止序列长度 `≤ 3` 且最终秩 `≤ 3`。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_file_modifiers/**`、`crates/lang-frontend/src/parser/engine/file.rs` 与 `crates/lang-frontend/src/parser/engine/file_kani.rs`，统一校验全部 **16 份 F6 Lean 4 证明工程（109 条定理）与 41 个 Kani harness**。

## 2. 检查点

- [x] `F6FileModifiers.lean` 零告警构建与 7 条定理核心公理白名单本机通过（仅依赖 `propext`、`Quot.sound`）
- [x] `verify_file_header_and_root_separator_decisions`、`verify_declaration_modifier_step_invariants` 与 `verify_declaration_modifier_run_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`file_tests`（3/3）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [x] 接入 `parser-formal-f6.yml`（16 片 / 109 定理 / 41 harness），经 PR #496 远端主 CI 与 F6 双轨作业验收合入 `main@f7ad5fd7` 并归档
