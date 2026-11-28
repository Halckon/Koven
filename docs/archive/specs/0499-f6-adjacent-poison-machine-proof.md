# SPEC-0499：F6 第四片 `ElsePoisonTail` 缓存可靠与同行花括号保留双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第四片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0498-f6-group-owners-proof@6d254f51` |
| 分支 | `feature/spec-0498-f6-group-owners-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)、[SPEC-0490](0490-f6-angle-group-machine-proof.md)、[SPEC-0492](0492-f6-ordinary-groups-machine-proof.md) 与 [SPEC-0498](0498-f6-group-owners-machine-proof.md)，
对 [adjacent_poison.rs](../../../crates/lang-frontend/src/parser/engine/recovery/adjacent_poison.rs) 中的 `ElsePoisonTail` 缓存捷径（`recover_poison_before_else`）与 `adjacent_poison_tail_boundary` 同行花括号保留交付 **Lean 4（无限长算法模型定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与穷举回归**：
   - [adjacent_poison.rs](../../../crates/lang-frontend/src/parser/engine/recovery/adjacent_poison.rs) 抽取纯内核 `ElsePoisonScan` 与 `scan_poison_before_else_core` 供生产 `recover_poison_before_else`、Kani 与单测共用，不改变生产缓存与恢复语义。
   - [adjacent_poison_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/adjacent_poison_tests.rs) 穷举覆盖长度 `1..=6` 全部 19,530 条词素类别序列与任意起点/游标，核对 `ElsePoisonTail` 拒绝缓存可靠性与 `FoundElse` 边界不变量。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_adjacent_poison/F6AdjacentPoison.lean](../../../formal/parser/f6_adjacent_poison/F6AdjacentPoison.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 4 条定理（E1–E4）：`scanElse_bounds`、`elsePoisonCache_sound`、`scanElse_foundElse_is_else` 与 `scanAdjacentTail_never_same_line_brace`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [adjacent_poison_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/adjacent_poison_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_else_poison_cache_soundness_len_6`，直接调用生产 `scan_poison_before_else_core` 符号验证长度 6 任意序列与游标上的 `ElsePoisonTail` 缓存可靠性。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_adjacent_poison/**`、`adjacent_poison.rs` 与 `adjacent_poison_kani.rs`，统一校验全部 4 份 F6 Lean 4 证明工程（4 + 14 + 5 + 4 = 27 条定理）与 12 个 Kani harness。

## 2. 检查点

- [x] `F6AdjacentPoison.lean` 零告警构建与 4 条定理核心公理白名单本机通过
- [x] `verify_else_poison_cache_soundness_len_6` 本机 Kani 0.68.0 实跑通过，`adjacent_poison_tests` 与 `cargo clippy --all-targets` 通过
- [x] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档

**远端 CI 验收证据。** 实现经 PR #470（`fad5fe41`）合入 `main`，并由 PR #477（`main@a899841c`，验证合并提交 `c5499d8d`）全量复验通过：[双宿主主 CI 证据](https://github.com/Halckon/Koven/actions/runs/45797758722/job/139848568750)、[F6 Lean 4 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703151)（含 `F6AdjacentPoison` 全部 4 条定理，总计 10 片 64 条定理）与 [F6 Kani 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703749)（含 `verify_else_poison_cache_soundness_len_6`，总计 24/24 条）。归档提交本身的门禁另验。
