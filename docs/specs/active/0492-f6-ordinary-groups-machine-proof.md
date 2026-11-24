# SPEC-0492：F6 第二片 `OrdinaryGroups` 栈计数不变量与 `GroupClosers::confirmed_closer` 水位单调性双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0491-tree-sitter-remaining-drifts@92abcd74` |
| 分支 | `feature/spec-0492-f6-ordinary-groups-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 中的 **候选 B** 与 [SPEC-0490](0490-f6-angle-group-machine-proof.md)，
对 [ordinary_groups.rs](../../../crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs) 中的 `OrdinaryGroups` 栈计数机、`RecoveryBoundaries::include_unclosed` 水位合并以及 `GroupClosers::confirmed_closer` 跳转守卫交付 **Lean 4（无限长算法模型定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入路径触发的 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与穷举回归**：
   - [ordinary_groups.rs](../../../crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs) 仅向同级模块与 `#[cfg(any(test, kani))]` 暴露只读检查与测试构造辅助，不改变任何生产路径行为。
   - [ordinary_group_tests.rs](../../../crates/lang-frontend/src/parser/engine/recovery/ordinary_group_tests.rs) 新增 3 项穷举 Rust 单元测试：覆盖长度 `0..=5` 全部 9,331 组分隔符序列的 `active == stack_kind_counts` 不变量、`include_unclosed` 结合/交换/幂等与非遮蔽律，以及 `confirmed_closer` 停止集反单调性、游标单调性与边界安全。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_ordinary_groups/F6OrdinaryGroups.lean](../../../formal/parser/f6_ordinary_groups/F6OrdinaryGroups.lean)（零 Mathlib，仅用 Lean 4 核心 `Init`，固定 `leanprover/lean4:v4.14.0`）证明：
     - **O1**：`includeUnclosed` 的结合律、交换律、幂等律与子边界非遮蔽性（`includeUnclosed_assoc`/`comm`/`idem` 及 `includeUnclosed_preserves_child_*`）；
     - **O2**：`popUntil_complete_and_wf`（`active.get k > 0` 时必返回同类 opener、计数器永不下溢且保持 `WellFormed`）；
     - **O3**：`visitGroup_preserves_wf`、`visitGroup_close_matches_iff` 与 `runEvents_preserves_wf`（任意长序列保持 `active == countKind` 且闭合匹配当且仅当同类计数 `> 0`）；
     - **O4**：`confirmedCloser_antitone_stops`（停止集扩大不凭空放行）与 `confirmedCloser_monotone_cursor`（游标前移不撤销已确认闭合）；
     - **O5**：`confirmedCloser_boundary_safety`（返回 `some c` 必保证外层已闭合且所有激活水位均 `< cursor`）。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [ordinary_group_kani.rs](../../../crates/lang-frontend/src/parser/engine/recovery/ordinary_group_kani.rs)（`#[cfg(kani)]` 隔离）提供 3 个直接调用生产 Rust 实现的证明入口：`verify_recovery_boundaries_monoid_and_non_shielding`、`verify_ordinary_groups_active_invariant_len_6` 与 `verify_confirmed_closer_monotonicity_and_safety`。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_ordinary_groups/**` 与 `ordinary_group_kani.rs`，统一校验 `F6AngleGroups`（4 条定理）与 `F6OrdinaryGroups`（10 条定理）的零告警构建与核心公理白名单，并运行全部 9 个 Kani harness。

## 2. 证据边界声明

- 本机沙箱已完成 `ordinary_group_tests` 全部 3 项穷举单测（含 9,331 组序列）、`cargo fmt --all -- --check`、`cargo clippy --locked -p lang-frontend --lib -- -D warnings` 与 `scripts/check_docs.py`。
- 因本机沙箱未连接 Lean 4 / Kani 安装目录，Lean 4 10 条定理构建、公理白名单与 Kani 3 个新增 harness 的最终实跑验收以 PR 触发的 `Parser Formal F6 (Lean 4 + Kani)` workflow 结果为准。

## 3. 检查点

- [x] `ordinary_groups.rs` 零行为变化接口、`ordinary_group_tests.rs`（3/3 穷举单测通过）与 `ordinary_group_kani.rs`（3 个 harness）落地
- [x] `formal/parser/f6_ordinary_groups/` Lean 4 零 Mathlib 工程（O1–O5）与 `.github/workflows/parser-formal-f6.yml` 扩展落地
- [ ] PR CI 中 Lean 4（10 条定理）与 Kani（9 个 harness）双轨实跑验收与归档
