# SPEC-0513：F6 第十三片 `supertypes` 委托边界、`deinit` 后缀决策与 Lambda 尾扫描双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第十三片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@d405ed2b`（合入 `main@d9db304a` 后随 PR #490 合并至 `main@36012f86`） |
| 分支 | `feature/spec-0513-f6-supertypes` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0512](0512-f6-when-boundary-machine-proof.md)，
对 [supertypes.rs](../../../crates/lang-frontend/src/parser/engine/class/supertypes.rs) 中超类型列表的直接委托边界判定、委托后缀恢复停止位选择、成员 `deinit` 后缀边界决策与委托尾 lambda 扫描内核交付 **Lean 4（无界状态与任意长度尾事件流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [supertypes.rs](../../../crates/lang-frontend/src/parser/engine/class/supertypes.rs) 中将原内联判定与扫描步进逻辑提炼为纯栈值内核（`DelegationLambdaTailEvent`、`DelegationLambdaTailStep`、`is_direct_delegation_boundary_kind`、`delegation_suffix_extra_stop`、`supertype_deinit_suffix_decision`、`classify_delegation_lambda_token`、`step_delegation_lambda_tail` 与 `#[cfg(any(test, kani))] run_delegation_lambda_tail`），由 `parse_supertype_list`、`at_delegation_boundary`、`at_supertype_suffix_boundary` 与 `delegation_lambda_closer` 直接调用，保持原有短路求值与解析行为零变化，并补充单元测试。
   - 在 [class.rs](../../../crates/lang-frontend/src/parser/engine/class.rs) 中仅挂载 `#[cfg(kani)] mod supertypes_kani;`。
2. **轨道 1（Lean 4 无界模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_supertypes/F6Supertypes.lean](../../../formal/parser/f6_supertypes/F6Supertypes.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 6 条无界定理（T1–T6）：
     - `isDirectDelegationBoundary_spec`（T1：直接委托边界当且仅当 `eof`、`,`、`{`、`}`）
     - `delegationSuffixExtraStop_spec`（T2：委托后缀附加停止位严格按 `invalidTarget > poison > unsupportedSuffix` 优先且三分支互斥）
     - `supertypeDeinitDecision_spec`（T3：成员 `deinit` 后缀决策矩阵精确等价于生产四象限短路语义）
     - `classifyDelegationLambdaToken_spec`（T4：对全部 6 类委托尾 lambda 词法事件 `trivia`、`sameLineClosedCall`、`lineBreakLeftParen`、`unclosedLeftParen`、`outerHeaderDelimiter`、`other` 逐一给出精确充要条件）
     - `runDelegationLambdaTail_iff`（T5：任意长度尾事件流接受当且仅当存在全由 `trivia`/`sameLineClosedCall` 组成的前缀紧接 `outerHeaderDelimiter`）
     - `runDelegationLambdaTail_rejects_breaks_and_eof`（T6：无外层头分隔符的纯可继续流（EOF 截断）永不接受，且首遇换行左括号、未闭合左括号或其它记号即刻拒绝）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [supertypes_kani.rs](../../../crates/lang-frontend/src/parser/engine/class/supertypes_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_supertype_direct_boundary_and_lambda_token_classification`：直接调用生产 `is_direct_delegation_boundary_kind` 与 `classify_delegation_lambda_token`，由单源宏（`define_exhaustive_unit_enum_any!` / `define_exhaustive_enum_any!`）从同一份变体清单同步生成采样数组与 `#[deny(unreachable_patterns)]` 无通配符穷举 `match`，在编译期同时阻断漏项与重复项，符号覆盖生产 `LexemeKind` 全部变体（含全部 `TriviaKind`、`InvalidKind`、`Eof` 及 `TokenKind` 全部 36 个 `Keyword`、43 个 `Symbol`、11 个 `ReservedWord` 与字面量后缀）以及任意 `has_line_break` / `has_call_closer`。
     - `verify_supertype_deinit_and_suffix_stop_decisions`：直接调用生产 `supertype_deinit_suffix_decision` 与 `delegation_suffix_extra_stop`，符号覆盖全部 3 种 `ClassifierHeaderOwner` 与全部布尔组合，并逐对断言三个 `DeclarationStops` 位掩码常量非零、两两按位互斥（`a & b == 0`）且 `in_unsupported_delegation_suffix()` 仅在 `!invalid_target && !poison` 时为真。
     - `verify_delegation_lambda_tail_run_len_6`：直接调用 `run_delegation_lambda_tail`（驱动生产 `step_delegation_lambda_tail`，`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意 `DelegationLambdaTailEvent` 序列。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_supertypes/**`、`crates/lang-frontend/src/parser/engine/class.rs`、`crates/lang-frontend/src/parser/engine/class/supertypes.rs` 与 `crates/lang-frontend/src/parser/engine/class/supertypes_kani.rs`，统一校验全部 **13 份 F6 Lean 4 证明工程（88 条定理）与 32 个 Kani harness**。

## 2. 检查点

- [x] `F6Supertypes.lean` 零告警构建与 6 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_supertype_direct_boundary_and_lambda_token_classification`、`verify_supertype_deinit_and_suffix_stop_decisions` 与 `verify_delegation_lambda_tail_run_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`supertypes`（4/4）/ `supertype_suffix`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [x] 合并树 `73c20bf4`（合入 PR #488 后）接入 `parser-formal-f6.yml`（13 片 / 88 定理 / 32 harness）并经 PR #490 远端主 CI 与 F6 双轨作业验收合入 `main@36012f86`
