# SPEC-0508：F6 第六片 `Stops` / `TypeStops` / `DeclarationStops` 边界传播格与硬分隔符不屏蔽双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第六片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@5680c63a`（同步 `main@c4f040ea` 重编号自原 0502） |
| 分支 | `feature/spec-0502-f6-stops-lattice-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0500](../../specs/active/0500-f6-pratt-precedence-machine-proof.md)，
对 [boundary.rs](../../../crates/lang-frontend/src/parser/engine/boundary.rs) 中表达式（`Stops`）、类型（`TypeStops`）与声明恢复（`DeclarationStops`）之间的边界传播同态及硬分隔符不屏蔽性质交付 **Lean 4（完备格代数定理）+ Kani（全位向量生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与全状态穷举回归**：
   - [boundary.rs](../../../crates/lang-frontend/src/parser/engine/boundary.rs) 保持零改动（998 行 `<= 1000`）；仅在 [source.rs](../../../crates/lang-frontend/src/source.rs) 与 [lexer/mod.rs](../../../crates/lang-frontend/src/lexer/mod.rs) 补充 `#[cfg(any(test, kani))]` 构造辅助并在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 挂载 `stops_kani` 与 `stops_tests`。
   - [stops_tests.rs](../../../crates/lang-frontend/src/parser/engine/stops_tests.rs) 穷举覆盖全部 $2^{11} \times 2 = 4{,}096$ 个 `Stops` 状态及 3 种 `ClassifierHeaderOwner`，核对硬分隔符不屏蔽与包含单调性。
2. **轨道 1（Lean 4 模型定理，`proved (model, complete finite lattice)`）**：
   - [formal/parser/f6_stops_lattice/F6StopsLattice.lean](../../../formal/parser/f6_stops_lattice/F6StopsLattice.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 8 条定理（S1–S8）：`containsHard_implies_contains`、`blockExpr_preserves_hard`、`lambdaExpr_preserves_hard`、`stripSoft_preserves_hard`、`typeFromExpr_preserves_hard`、`declFromExpr_monotone_and_preserves_hard`、`supertypeRecovery_preserves_hard_and_owner` 与 `forProvenFnParams_preserves_inner_hard_clears_outer`。
3. **轨道 2（Kani 全位向量生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [stops_kani.rs](../../../crates/lang-frontend/src/parser/engine/stops_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_stops_hard_delimiter_non_shielding` 与 `verify_type_stops_recovery_projections`，直接调用生产 `Stops`、`TypeStops` 与 `DeclarationStops` 方法符号检验全部位向量组合。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_stops_lattice/**`、`boundary.rs` 与 `stops_kani.rs`，统一校验全部 6 份 F6 Lean 4 证明工程（4 + 14 + 5 + 4 + 8 + 8 = 43 条定理）与 16 个 Kani harness。

## 2. 检查点

- [x] `F6StopsLattice.lean` 零告警构建与 8 条定理核心公理白名单本机通过
- [x] `verify_stops_hard_delimiter_non_shielding` 与 `verify_type_stops_recovery_projections` 本机 Kani 0.68.0 实跑通过，`stops_tests` 与 `cargo clippy --all-targets` 通过
- [x] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档

**PR #477 验收证据。** 实现 head `1b683c7f`，基于 `c4f040ea`，实际验证合并提交 `c5499d8d`。[双宿主主 CI 证据](https://github.com/Halckon/Koven/actions/runs/45797758722/job/139848568750)通过。[Lean 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703151)与 [Kani 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703749)均通过；该次验证覆盖 F6 全部 10 片，共 64 条定理和 24/24 个 harness。归档提交本身的门禁另验。
