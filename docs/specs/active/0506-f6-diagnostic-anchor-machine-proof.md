# SPEC-0506：F6 第九片 `diagnostic_anchor_prefix` 前缀和单调性与区间诊断锚点查询双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第九片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0502-f6-stops-lattice-proof@ae4377ba` |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)与 [SPEC-0505](0505-f6-lexeme-stream-machine-proof.md)，
对 [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 中 `LexicalRecoveryIndex::diagnostic_anchor_prefix` 的前缀和构造与 `has_diagnostic_anchor(start, end)` `O(1)` 区间锚点查询交付 **Lean 4（无限长前缀和单调性与区间等价定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与穷举回归单测**：
   - 将前缀和构造与查询抽离至 [diagnostic_anchor.rs](../../../crates/lang-frontend/src/parser/engine/recovery/diagnostic_anchor.rs)（`finalize_anchor_prefix` 与 `prefix_has_anchor`），[recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 直接委托调用，保持生产行为完全不变且降低 `recovery.rs` 物理行数。
   - [diagnostic_anchor_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/diagnostic_anchor_tests.rs) 穷举全部 $2^6 = 64$ 种布尔标记向量与全部 `(start, end) ∈ [0..=7]^2` 区间（含空表快路径、反序与越界查询）。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_diagnostic_anchor/F6DiagnosticAnchor.lean](../../../formal/parser/f6_diagnostic_anchor/F6DiagnosticAnchor.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（A1–A5）：`prefix_length_and_zero`、`prefix_monotone`、`prefix_get_eq_countPrefix`、`hasAnchor_iff_exists_in_range` 与 `hasAnchor_empty_and_oob_false`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [diagnostic_anchor_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/diagnostic_anchor_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_diagnostic_anchor_prefix_empty_and_oob` 与 `verify_diagnostic_anchor_prefix_exact_range_len_8`，直接调用生产 `finalize_anchor_prefix` 与 `prefix_has_anchor` 符号验证 A1–A5。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_diagnostic_anchor/**`、`diagnostic_anchor.rs` 与 `diagnostic_anchor_kani.rs`，统一校验全部 9 份 F6 Lean 4 证明工程（53 + 5 = 58 条定理）与 22 个 Kani harness。

## 2. 检查点

- [x] `F6DiagnosticAnchor.lean` 零告警构建与 5 条定理核心公理白名单本机通过
- [x] `verify_diagnostic_anchor_prefix_empty_and_oob` 与 `verify_diagnostic_anchor_prefix_exact_range_len_8` 本机 Kani 0.68.0 实跑通过，`diagnostic_anchor_tests` 与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
