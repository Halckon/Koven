# SPEC-0498：F6 第三片 `GroupOwners` 词法深度与 `terminal_owner_events` 栈平衡双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第三片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0497-ts05-block-separator@21c52809` |
| 分支 | `feature/spec-0498-f6-group-owners-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md)（放电 §2.2 TCB 前提 1）、[SPEC-0490](0490-f6-angle-group-machine-proof.md) 与 [SPEC-0492](0492-f6-ordinary-groups-machine-proof.md)，
对 [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 中的 `GroupOwners::outside_lexical_symbol` 交付 **Lean 4（无限长算法模型定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与穷举回归**：
   - [recovery.rs](../../../crates/lang-frontend/src/parser/engine/recovery.rs) 仅向 `#[cfg(any(test, kani))]` 暴露 `GroupOwners::lexical_depth` 与 `GroupOwners::next_terminal_index` 只读访问器（总物理行数保持 `<= 1000`）。
   - [group_owner_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/group_owner_tests.rs) 穷举覆盖长度 `1..=5` 全部合法 `String`/`Interpolation`/分隔符与 `TerminalOwnerEvent` 组合，核对深度不变式、终止事件单调性与顶层符号屏蔽。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_group_owners/F6GroupOwners.lean](../../../formal/parser/f6_group_owners/F6GroupOwners.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条定理（G1–G4）：`drainLoop_spec`、`drainTerminals_spec`、`outsideLexicalSymbol_preserves_invariant`、`outsideLexicalSymbol_some_iff_top_level` 与 `runOwners_preserves_bounds`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [group_owner_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/group_owner_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_group_owners_depth_and_filtering_len_4`，直接调用生产 `GroupOwners::outside_lexical_symbol` 符号验证无下溢、无越界与严格过滤等价性。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_group_owners/**` 与 `group_owner_kani.rs`，统一校验全部 3 份 F6 Lean 4 证明工程（4 + 14 + 5 = 23 条定理）与 11 个 Kani harness。

## 2. 检查点

- [x] `F6GroupOwners.lean` 零告警构建与 5 条定理核心公理白名单本机通过
- [x] `verify_group_owners_depth_and_filtering_len_4` 本机 Kani 0.68.0 实跑通过，`group_owner_tests` 与 `cargo clippy --all-targets` 通过
- [x] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档

**远端 CI 验收证据。** 实现经 PR #470（`fad5fe41`）合入 `main`，并由 PR #477（`main@a899841c`，验证合并提交 `c5499d8d`）全量复验通过：[双宿主主 CI 证据](https://github.com/Halckon/Koven/actions/runs/45797758722/job/139848568750)、[F6 Lean 4 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703151)（含 `F6GroupOwners` 全部 5 条定理，总计 10 片 64 条定理）与 [F6 Kani 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703749)（含 `verify_group_owners_depth_and_filtering_len_4`，总计 24/24 条）。归档提交本身的门禁另验。
