# SPEC-0510：F6 第十一片 `LexicalRecoveryIndex` 词法 owner 栈健全性与 `verified_events` 构造双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十一片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@28076229` |
| 分支 | `feature/spec-0510-f6-lexical-owner-events` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0498](../../archive/specs/0498-f6-group-owners-machine-proof.md)（Slice 3 `GroupOwners`），
对 [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 中 `LexicalRecoveryIndex::new` 的词法 owner 栈验证与 `verified_events` 构造内核（抽取至 [owner_events.rs](../../../crates/lang-frontend/src/parser/engine/recovery/owner_events.rs)）交付 **Lean 4（无界流模型定理）+ Kani（有界生产 Rust 符号检验 + 下游 `GroupOwners` 桥接验证）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **零行为变化内核抽取、按需分配与单元测试**：
   - 将 `LexicalRecoveryIndex::new` 中的词法 owner 栈 LIFO 验证与 `verified_events` 构造循环等价抽取至 [owner_events.rs](../../../crates/lang-frontend/src/parser/engine/recovery/owner_events.rs)，通过私有 `OwnerEventBuffers` 抽象复用核心状态机 `verify_terminal_owner_events_with`：生产包装 `verify_terminal_owner_events` 保持按实际 owner 深度与事件数按需增长（无字符串/无词法错误时零堆分配、保留容量为 0），保持 [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 远低于 1000 行上限；仅在 [source.rs](../../../crates/lang-frontend/src/source.rs) 与 [lexer/mod.rs](../../../crates/lang-frontend/src/lexer/mod.rs) 补充 `#[cfg(any(test, kani))]` 带范围构造辅助。
   - [owner_events_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/owner_events_tests.rs) 覆盖嵌套字符串/插值配对、终止性无效转义合成事件、EOF 级联关闭（`terminal_error_at_eof` 正反对照）、不匹配闭合/错误 opener/未消费事件拒绝、10 万纯标识符流零容量保留验证，以及下游 `GroupOwners::outside_lexical_symbol` 端到端桥接。
2. **轨道 1（Lean 4 无界模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_owner_events/F6OwnerEvents.lean](../../../formal/parser/f6_owner_events/F6OwnerEvents.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 11 条无界定理（O1–O7 族）：
     - `drainDueEvents_balance_and_bounds`：到期事件 LIFO 弹出与 `verified` 增长精确守恒、`nextT` 单调有界；
     - `drainEofOwners_length`：EOF 级联清空栈与输出事件数精确相等；
     - `verifyStep_balance_invariant`：单步 lexeme 转换保持 `owners.length + verified.length + openDelta = owners'.length + verified'.length + closeDelta` 与 `nextT` 有界单调；
     - `verifyLoop_global_balance`：任意长度 lexeme 流全局计数平衡；
     - `verifyTerminalOwnerEvents_eof_closed_balance`：以 `Eof` 结尾且通过验证的流满足 `countOpens = countExplicitCloses + verified.length`（零残留未闭合 owner）；
     - `verifyStep_rejects_unclosed_eof_without_flag`：无 `terminal_error_at_eof` 时未闭合 owner 到达 EOF 必拒绝；
     - `verifyTerminalOwnerEvents_rejects_unconsumed`：存在未消费输入事件时必拒绝；
     - `verifyTerminalOwnerEvents_bounded`：输入事件与 lexeme 范围有界于 `sourceLen` 时，全部输出事件偏移亦满足 `offset ≤ sourceLen`；
     - `drainDueEvents_exact_lifo_match` 与 `drainEofOwners_exact_lifo_match`：逐项约束每条输出关闭事件与被弹出栈顶的 `(kind, opener)` 身份精确一致；
     - `drainDueEvents_rejects_mismatched_top`：任意到期事件若 `kind` 或 `opener` 与栈顶不符必拒绝。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [owner_events_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/owner_events_kani.rs)（`#[cfg(kani)]` 隔离）通过固定容量栈缓冲 `FixedOwnerBuffers` 直接调用核心状态机 `owner_events::verify_terminal_owner_events_with`（薄 `Vec` 包装 `verify_terminal_owner_events` 的按需分配与零容量保留由 `owner_events_tests` 单独覆盖）：
     - `verify_owner_events_soundness_monotonicity_and_group_owners_bridge`：在符号化有界词法流（2 个前缀词法单元 + 1 个 `Eof`，以及 **0 至 2 条**有序终止事件）上验证全局平衡、输出偏移单调且 `<= source_len`，并直接将 `Ok` 产出的 `verified` 回放至生产 `GroupOwners::outside_lexical_symbol`，符号验证 Slice 11 → Slice 3 的 TCB 桥接零下溢、全消费、终止深度为 0；
     - `verify_owner_events_rejects_unclosed_and_unconsumed`：符号验证无 `terminal_error_at_eof` 的未闭合字符串、越过 `source_len` 的未消费事件、错误 `kind` 或错误 `opener` 的到期终止事件以及错误种类的显式闭合均必返回 `Err(ParserInternalError::InvalidLexemeStream)`，并验证终止性无效转义合成事件与 `GroupOwners` 桥接。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步在 `pull_request` 与 `push` 中触发 `formal/parser/f6_owner_events/**`、`owner_events.rs`、`owner_events_kani.rs`、`lexer/mod.rs` 与 `source.rs`，统一校验全部 11 份 F6 Lean 4 证明工程（75 条定理）与 26 个 Kani harness。

## 2. 检查点

- [x] `F6OwnerEvents.lean` 零告警构建与 11 条定理核心公理白名单本机通过
- [x] `owner_events_tests`（含 10 万纯流零容量保留测试）、`cargo clippy --all-targets`、`cargo fmt --check`、`check_rust_sizes` 与本机 Kani 0.68.0 两条 harness 实跑通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
