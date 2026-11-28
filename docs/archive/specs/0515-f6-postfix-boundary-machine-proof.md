# SPEC-0515：F6 第十五片 `postfix` 后缀起始、带类型调用延续、实参边界与 callee 尾逆向扫描双轨证明

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F6 第十五片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0514](0514-f6-block-boundary-machine-proof.md) |
| 基线 | `main@f2ea2feb` |
| 分支 | `feature/spec-0515-f6-postfix-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0514](0514-f6-block-boundary-machine-proof.md)，
对 [postfix.rs](../../../crates/lang-frontend/src/parser/engine/postfix.rs) 中的后缀起始判定、带类型调用后缀延续判定、调用实参边界判定、闭合调用尾 lambda 附着判定及恢复区 callee 尾逆向扫描内核交付 **Lean 4（无界状态与任意长度前缀流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [postfix.rs](../../../crates/lang-frontend/src/parser/engine/postfix.rs) 中提炼纯栈值内核（`is_postfix_start_kind`、`is_recovery_callee_ignored_kind`、`is_recovery_callee_terminal_kind`、`has_recovery_callee_end_in`、`typed_call_suffix_continues_kind`、`call_argument_boundary_decision` 与 `trailing_lambda_attaches_after_call`），由 `is_postfix_start`、`has_recovery_callee_end`、`parse_call`、`call_argument_boundary` 与 `typed_call_suffix_continues` 直接调用，严格保留原有短路求值与解析行为零变化，并在 [postfix_tests.rs](../../../crates/lang-frontend/src/parser/engine/postfix_tests.rs) 补充 5 个单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod postfix_kani;` 与 `#[cfg(test)] mod postfix_tests;`。
2. **轨道 1（Lean 4 模型定理：2 条 `unbounded` + 5 条 `complete finite domain`）**：
   - [formal/parser/f6_postfix_boundary/F6PostfixBoundary.lean](../../../formal/parser/f6_postfix_boundary/F6PostfixBoundary.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 7 条定理（P1–P7）：
     - 有限域模型定理（5 条，`proved (model, complete finite domain)`）：
       - `isPostfixStart_spec`（P1：精确刻画全部 7 个后缀起始符号 `.`/`?.`/`(`/ `[`/`!!`/`?`/`::`）
       - `typedCallSuffixContinues_spec`（P2：仅当后缀为跨行 `{` 时停止带类型调用延续）
       - `callArgumentBoundary_spec`（P3：精确刻画调用实参边界的完整充要条件）
       - `trailingLambdaAttachesAfterCall_spec`（P4：仅当调用已闭合、当前为 `{` 且非后续 `when` 分支时附着尾 lambda）
       - `recoveryCalleeIgnored_and_terminal_disjoint`（P5：恢复区忽略后缀 `trivia`/`?`/`!!` 与有效 callee 终止记号严格不相交）
     - 无界列表模型定理（2 条，`proved (model, unbounded)`）：
       - `hasRecoveryCalleeEnd_iff_last_non_ignored_terminal`（P6：任意长度前缀流上逆向查找首个非忽略记号并检查终止性精确等价于 `hasRecoveryCalleeEnd`）
       - `hasRecoveryCalleeEnd_invariant_under_ignored_suffix`（P7：尾部追加任意长度的 `trivia`/`?`/`!!` 序列均不改变 `hasRecoveryCalleeEnd` 结果）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [postfix_kani.rs](../../../crates/lang-frontend/src/parser/engine/postfix_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_postfix_start_typed_call_and_trailing_lambda`：直接调用生产 `is_postfix_start_kind`、`typed_call_suffix_continues_kind` 与 `trailing_lambda_attaches_after_call`，由单源宏（`define_exhaustive_unit_enum_any!` / `define_exhaustive_enum_any!`）从同一份变体清单自动派生采样上界、采样分支与 `#[deny(unreachable_patterns)]` 无通配符穷举 `match`，符号覆盖全部生产 `LexemeKind` / `TokenKind` 变体。
     - `verify_call_argument_boundary_and_callee_disjointness`：直接调用生产 `call_argument_boundary_decision`、`is_recovery_callee_ignored_kind` 与 `is_recovery_callee_terminal_kind`，覆盖全部 `LexemeKind` 与布尔控制位并断言忽略集与终止集互斥。
     - `verify_recovery_callee_end_reverse_scan_len_6`：直接调用与生产共享的 `has_recovery_callee_end_in`（`#[kani::unwind(8)]`），以独立预期分类清单核对忽略集与终止集，符号覆盖长度 `0..=6` 的任意 `LexemeKind` 序列。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_postfix_boundary/**`、`crates/lang-frontend/src/parser/engine/postfix.rs` 与 `crates/lang-frontend/src/parser/engine/postfix_kani.rs`，统一校验全部 **15 份 F6 Lean 4 证明工程（102 条定理）与 38 个 Kani harness**。

## 2. 检查点

- [x] `F6PostfixBoundary.lean` 零告警构建与 7 条定理核心公理白名单本机通过（仅依赖 `propext`、`Quot.sound`）
- [x] `verify_postfix_start_typed_call_and_trailing_lambda`、`verify_call_argument_boundary_and_callee_disjointness` 与 `verify_recovery_callee_end_reverse_scan_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`postfix_tests`（5/5）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [x] 接入 `parser-formal-f6.yml`（15 片 / 102 定理 / 38 harness），经 PR #496 远端主 CI 与 F6 双轨作业验收合入 `main@f7ad5fd7` 并归档
