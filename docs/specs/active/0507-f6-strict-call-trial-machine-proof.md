# SPEC-0507：F6 第十片 `StrictCallTrialIndex` 索引单调性、反向 DP 路径与递归预算双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0502-f6-stops-lattice-proof@71d4d963` |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)与 [SPEC-0506](0506-f6-diagnostic-anchor-machine-proof.md)，
对 [trial.rs](../../../crates/lang-frontend/src/parser/trial.rs) 中 `StrictCallTrialIndex` 的 trivia 跳过反向填充（`raw_to_significant` / `significant_to_raw`）、反向 DP 路径终止表（`path_ends`）、调用点类型实参分类（`classify_call_trial`）与缓存查询统一递归预算校验（`check_recursion_budget`）交付 **Lean 4（无限长模型定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与穷举回归单测**：
   - 将四个纯内核抽离至 [trial/kernel.rs](../../../crates/lang-frontend/src/parser/trial/kernel.rs)（`fill_next_significant`、`resolve_path_end`、`classify_call_trial`、`check_recursion_budget`），[trial.rs](../../../crates/lang-frontend/src/parser/trial.rs) 直接委托调用，保持生产行为完全不变。
   - [trial/kernel_tests.rs](../../../crates/lang-frontend/src/parser/trial/kernel_tests.rs) 穷举全部 $2^6 = 64$ 种 trivia 掩码、全部 $3^6 = 729$ 种 `{Ident, Dot, Other}` 路径序列以及 `MAX_RECURSION_DEPTH` / `usize::MAX` 溢出边界。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_strict_call_trial/F6StrictCallTrial.lean](../../../formal/parser/f6_strict_call_trial/F6StrictCallTrial.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 6 条定理（T1–T6）：`rawToSig_non_trivia_always_some`、`rawToSig_monotone`、`sigToRaw_strict_mono`、`pathEndAt_strict_advance_bounds_and_maximal`、`classifyCallTrial_match_ordering` 与 `checkBudget_iff_and_antitone`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [trial/kernel_kani.rs](../../../crates/lang-frontend/src/parser/trial/kernel_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_raw_to_significant_exact_and_monotone_len_8` 与 `verify_trial_path_ends_and_budget_len_8`，直接调用生产 `trial::kernel` 符号验证 T1–T6 及无溢出预算检查。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_strict_call_trial/**`、`trial.rs`、`trial/kernel.rs` 与 `trial/kernel_kani.rs`，统一校验全部 10 份 F6 Lean 4 证明工程（58 + 6 = 64 条定理）与 24 个 Kani harness。

## 2. 检查点

- [x] `F6StrictCallTrial.lean` 零告警构建与 6 条定理核心公理白名单本机通过
- [x] `verify_raw_to_significant_exact_and_monotone_len_8` 与 `verify_trial_path_ends_and_budget_len_8` 本机 Kani 0.68.0 实跑通过，`trial` 全套单测与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
