# SPEC-0505：F6 第八片 `validate_lexemes` 词法流连续划分、严格单调与唯一 EOF 契约双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第八片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0502-f6-stops-lattice-proof@dd2a5deb` |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)（放电 §2.2 严格单调前提及四入口公共输入不变量）与 [SPEC-0509](0509-f6-lambda-trial-machine-proof.md)，
对 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中全部 4 个 Parser 入口共用的 `validate_lexemes` 词法流校验器交付 **Lean 4（无限长流划分与严格单调定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与回归单测**：
   - [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 抽取零堆分配单步函数 `validate_lexeme_step` 供生产 `validate_lexemes` 与 `#[cfg(any(test, kani))] validate_lexeme_spans_core` 共用，不改变生产校验语义。
   - [lexeme_stream_tests.rs](../../../crates/lang-frontend/src/parser/engine/lexeme_stream_tests.rs) 覆盖空流、缺 EOF、空洞、重叠、空非 EOF token、非空 EOF、提前 EOF 与长度不匹配等正反例。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_lexeme_stream/F6LexemeStream.lean](../../../formal/parser/f6_lexeme_stream/F6LexemeStream.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（V1–V5）：`valid_nonempty`、`valid_last_is_eof_at_sourceLen`、`valid_non_last_non_eof_and_non_empty`、`valid_contiguous_partition` 与 `valid_starts_strictly_increasing`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [lexeme_stream_kani.rs](../../../crates/lang-frontend/src/parser/engine/lexeme_stream_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_validate_lexeme_stream_rejects_empty_and_missing_eof` 与 `verify_validate_lexeme_stream_partition_len_4`，直接调用生产 `validate_lexeme_spans_core` 符号验证 V1–V5。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_lexeme_stream/**` 与 `lexeme_stream_kani.rs`，统一校验全部 8 份 F6 Lean 4 证明工程（48 + 5 = 53 条定理）与 20 个 Kani harness。

## 2. 检查点

- [x] `F6LexemeStream.lean` 零告警构建与 5 条定理核心公理白名单本机通过
- [x] `verify_validate_lexeme_stream_rejects_empty_and_missing_eof` 与 `verify_validate_lexeme_stream_partition_len_4` 本机 Kani 0.68.0 实跑通过，`lexeme_stream_tests` 与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
