# SPEC-0500：F6 第五片 Pratt 运算符优先级、结合律与非结合链检出双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第五片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0498-f6-group-owners-proof@ad878a0c` |
| 分支 | `feature/spec-0498-f6-group-owners-proof` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0499](0499-f6-adjacent-poison-machine-proof.md)，
对 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs)、[operator.rs](../../../crates/lang-frontend/src/parser/engine/operator.rs) 与 [expression.rs](../../../crates/lang-frontend/src/parser/engine/expression.rs) 中的 Pratt 优先级表、左右结合律及 `NonAssociativeGroup` 链检出逻辑交付 **Lean 4（无限长算法模型定理）+ Kani（有界生产 Rust 符号检验）** 双轨证明，并接入 `.github/workflows/parser-formal-f6.yml`。
不改变生产解析行为或公开 Rust API，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产源码零行为变化与全量运算符对回归**：
   - [operator.rs](../../../crates/lang-frontend/src/parser/engine/operator.rs) 抽取纯查找表 `infix_rule_from_parts` 供生产 `Parser::infix_rule`、Kani 与单测共用，不改变生产规则映射。
   - [pratt_tests.rs](../../../crates/lang-frontend/src/parser/engine/pratt_tests.rs) 穷举覆盖全部 35 个中缀运算符的 $35 \times 35 = 1{,}225$ 组有序对，核对优先级区间、右结合力、非结合组槽位双射与分组结果。
2. **轨道 1（Lean 4 无限长模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_pratt_precedence/F6PrattPrecedence.lean](../../../formal/parser/f6_pratt_precedence/F6PrattPrecedence.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 8 条定理（PR1–PR6）：`rightBp_spec`、`pratt_left_assoc_groups_left`、`pratt_right_assoc_groups_right`、`pratt_higher_prec_binds_tighter_right`、`pratt_higher_prec_binds_tighter_left`、`pratt_non_assoc_chain_detected`、`pratt_distinct_non_assoc_groups_no_chain_error` 与 `pratt_top_level_consumes_all`。
3. **轨道 2（Kani 有界生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [pratt_kani.rs](../../../crates/lang-frontend/src/parser/engine/pratt_kani.rs)（`#[cfg(kani)]` 隔离）提供 `verify_infix_rule_table_invariants` 与 `verify_pratt_precedence_and_non_assoc_3_ops`，直接调用生产 `infix_rule_from_parts` 符号验证全部 35 个中缀运算符及任意二元组合。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_pratt_precedence/**`、`engine.rs`、`operator.rs` 与 `pratt_kani.rs`，统一校验全部 5 份 F6 Lean 4 证明工程（4 + 14 + 5 + 4 + 8 = 35 条定理）与 14 个 Kani harness。

## 2. 检查点

- [x] `F6PrattPrecedence.lean` 零告警构建与 8 条定理核心公理白名单本机通过
- [x] `verify_infix_rule_table_invariants` 与 `verify_pratt_precedence_and_non_assoc_3_ops` 本机 Kani 0.68.0 实跑通过，`pratt_tests` 与 `cargo clippy --all-targets` 通过
- [ ] PR CI 中 `Parser Formal F6 (Lean 4 + Kani)` 实跑验收与归档
