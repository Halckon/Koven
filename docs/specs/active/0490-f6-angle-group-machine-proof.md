# SPEC-0490：F6 首片 `angle_group_closers` 双轨机器证明（Lean 4 + Kani）与路径触发轻量 CI

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 首片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main`（合入 PR #458 后） |
| 分支 | `docs/parser-f6-proof-slice-design` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)，以候选 A（`angle_group_closers` 8 状态反向 DP 与前向 `OrdinaryGroups::visit` 等价性）作为 F6 首片，采用 **Lean 4（无限长算法模型定理）+ Kani（有界生产 Rust 符号检验）双轨并行**，并通过路径触发的独立轻量 CI workflow 持续校验。不改变公开 Rust API、不引入 Cargo 生产依赖、不宣称已取得全输入生产 G4 证明。

## 1. 交付合同

1. **生产核心无行为变化抽离与穷举回归**：
   - [angle_groups.rs](../../../crates/lang-frontend/src/parser/engine/recovery/angle_groups.rs) 从 [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 原样抽离 `match_angle_group_delimiters`，供生产 `angle_group_closers`、Kani harness 与穷举单测共用。
   - [angle_group_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/angle_group_tests.rs) 在保留原有词法流 `n = 5` 穷举基础上，新增 `match_angle_group_delimiters_agrees_with_ordinary_groups_up_to_len_6`（覆盖长度 `0..=6` 共 55,987 组序列及 P4 `NonZeroUsize` 非零性）。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_angle_groups/F6AngleGroups.lean](../../../formal/parser/f6_angle_groups/F6AngleGroups.lean)（零 Mathlib 依赖，仅用 Lean 4 核心 `Init`，固定 `leanprover/lean4:v4.14.0`）形式化 `OrdinaryGroups` 前向模型与 8 状态反向 DP，证明 P1（边界表有界与无越界）、P2（`dpBoundary_eq_forwardStop` 强归纳主引理）、P3（`dpMatch_eq_forwardMatch` 无限长等价）与 P4（`dpMatch_some_pos`）。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust, n <= 6)`）**：
   - [angle_group_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/angle_group_kani.rs)（`#[cfg(kani)]` 隔离）直接调用生产 `match_angle_group_delimiters` 与 `OrdinaryGroups::visit`，提供 `verify_angle_group_closers_equivalence_bounded` 与 `verify_angle_group_closer_strictly_after_opener_and_nonzero` 两个证明入口。
4. **路径触发的独立轻量 CI（advisory）**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 仅在 `formal/parser/f6_angle_groups/**`、`recovery.rs`、`angle_groups.rs`、`angle_group_kani.rs`、`ordinary_groups.rs` 或自身变更时触发，对其他 PR 零耗时影响。

## 2. 证据边界声明

- 本机未安装 `lean`/`lake` 与 `cargo-kani`；本地仅运行了 Rust 格式化、Clippy、穷举单测（含 `n <= 6`）与文档/CI 策略检查。
- Lean 4 无限长模型证明与 Kani 有界生产检验的真实机器检查结果以后续 PR 触发的 `Parser Formal F6 (Lean 4 + Kani)` workflow 实跑为准；二者均不升级为全输入生产 G4。

## 3. 检查点

- [x] `match_angle_group_delimiters` 抽离与长度 `0..=6` 穷举单测通过
- [x] Lean 4 零 Mathlib 工程与 P1–P4 证明编写完成
- [x] Kani `#[cfg(kani)]` 双 harness 与 `.github/workflows/parser-formal-f6.yml` 落地
- [ ] PR CI 中 Lean 4 与 Kani 双轨实跑验收与归档
