# SPEC-0503：F6 第七片 `LambdaHeaderIndex` 单遍流式 DFA 等价性与线性步数双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第七片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0502-f6-stops-lattice-proof@e68e05b0` |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0502](0502-f6-stops-lattice-machine-proof.md)，
对 [lambda_trial.rs](../../../crates/lang-frontend/src/parser/lambda_trial.rs) 中的单遍无回溯 `LambdaHeaderIndex` DFA 交付 **Lean 4（无限长流等价性与线性步数定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与全流穷举回归**：
   - [lambda_trial.rs](../../../crates/lang-frontend/src/parser/lambda_trial.rs) 抽取纯单步转移 `step_lambda_trial` 供生产 `LambdaHeaderIndex::new`、Kani 与单测共用（不改变生产索引或诊断行为），并在 `#[cfg(test)]` 穷举验证全部 $6^5 = 7{,}776$ 条长度 5 词素流与独立逐 `{` 前向规约扫描完全一致。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_lambda_trial/F6LambdaTrial.lean](../../../formal/parser/f6_lambda_trial/F6LambdaTrial.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（L1–L5）：`advance_leftBrace_fails`、`specFrom_step_non_trivia`、`singlePass_sound_and_complete`、`singlePass_header_bounds` 与 `singlePass_steps_linear`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - `lambda_trial.rs` 内 `#[cfg(kani)] mod kani_proofs` 提供 `verify_lambda_trial_left_brace_always_fails_active` 与 `verify_lambda_trial_single_pass_eq_spec_len_5`，直接调用生产 `ActiveTrial::advance` 与 `step_lambda_trial` 符号验证状态转移、等价性、边界与线性步数。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_lambda_trial/**` 与 `lambda_trial.rs`，统一校验全部 7 份 F6 Lean 4 证明工程（4 + 14 + 5 + 4 + 8 + 8 + 5 = 48 条定理）与 18 个 Kani harness。

## 2. 检查点

- [x] `F6LambdaTrial.lean` 零告警构建与 5 条定理核心公理白名单本机通过
- [x] `verify_lambda_trial_left_brace_always_fails_active` 与 `verify_lambda_trial_single_pass_eq_spec_len_5` 本机 Kani 0.68.0 实跑通过，`lambda_trial` 穷举单测与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
