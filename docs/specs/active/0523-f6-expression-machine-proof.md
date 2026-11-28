# SPEC-0523：F6 第二十三片 `expression` 前缀运算符、`when` 条件分派、`return` 守卫、括号分组尾与字符串闭合双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十三片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0522](0522-f6-operator-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0522](0522-f6-operator-machine-proof.md)，
对 [expression.rs](../../../crates/lang-frontend/src/parser/engine/expression.rs)（纯内核位于 [expression_kernel.rs](../../../crates/lang-frontend/src/parser/engine/expression_kernel.rs)）中的前缀运算符（`!`/`+`/`-`）、`when` 臂体跨行中缀停止集合、`when` 条件六路分派、`return` 返回值守卫、括号分组非毒化尾部四路分派与字符串/插值缺失闭合防重报守卫交付 **Lean 4（模型定理：5 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [expression_kernel.rs](../../../crates/lang-frontend/src/parser/engine/expression_kernel.rs) 中提炼纯栈值内核（`is_when_body_line_break_infix_stop_kind`、`prefix_operator_for_kind`、`WhenConditionDispatch`、`when_condition_dispatch`、`return_parses_value`、`is_group_declaration_tail_kind`、`GroupNonPoisonTailAction`、`group_non_poison_tail_action` 与 `string_missing_closer_emits_diagnostic`），直接接入 [expression.rs](../../../crates/lang-frontend/src/parser/engine/expression.rs) 的 `parse_expression_bp_inner`、`parse_prefix_inner`、`parse_when_condition`、`parse_return`、`parse_group` 与 `parse_string`，严格保留原有短路求值（`return` 在 `!can_start_expression` 时跳过 `gap_has_line_break`）与解析行为零变化，且 `expression.rs` 物理行数保持低于 1000 行门禁。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `mod expression_kernel;`、`#[cfg(kani)] mod expression_kani;` 与 `#[cfg(test)] mod expression_tests;`，并在 [expression_tests.rs](../../../crates/lang-frontend/src/parser/engine/expression_tests.rs) 补充 4 个单元测试（含生产入口 `parse` / `parse_block` 对 `when` 四种前缀条件与 `return\n x` 跨行截断的端到端验证）。
2. **轨道 1（Lean 4 模型定理：5 条 `complete finite domain`）**：
   - [formal/parser/f6_expression/F6Expression.lean](../../../formal/parser/f6_expression/F6Expression.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条有限域模型定理（E1–E5，`proved (model, complete finite domain)`）：
     - `prefixOperator_and_whenInfixStop_spec`（E1：`!`/`+`/`-` 前缀集合与 `is`/`!is`/`in`/`!in` 跨行停止集合精确充要条件及互斥性）
     - `whenConditionDispatch_spec`（E2：`when` 条件六路分派充要条件，且前缀分派精确等价于 `isWhenBodyLineBreakInfixStop`）
     - `returnParsesValue_spec`（E3：`return` 携带返回值精确条件及跨行必抑制性质）
     - `groupNonPoisonTailAction_spec`（E4：括号分组非毒化尾部四路分派优先级 `AtRightParen > RecoverDeclarationTail > TryConfirmedCloser > NoSkip`）
     - `stringMissingCloserEmitsDiagnostic_spec`（E5：字符串与插值缺失闭合防重报精确守卫）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [expression_kani.rs](../../../crates/lang-frontend/src/parser/engine/expression_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_prefix_operator_and_when_condition_dispatch_exhaustive`：全量覆盖生产 `LexemeKind × bool`，验证前缀运算符 ⊆ 表达式起始、前缀与 `when` 停止集合互斥以及 `when_condition_dispatch` 全量等价。
     - `verify_return_value_and_string_missing_closer_guards`：穷举全部布尔组合验证 `return_parses_value` 与 `string_missing_closer_emits_diagnostic`。
     - `verify_group_non_poison_tail_action_exhaustive`：全量覆盖 `LexemeKind × bool` 与 4 布尔优先级矩阵。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_expression/**`、`crates/lang-frontend/src/parser/engine/expression.rs`、`crates/lang-frontend/src/parser/engine/expression_kernel.rs` 与 `crates/lang-frontend/src/parser/engine/expression_kani.rs`，统一校验全部 **23 份 F6 Lean 4 证明工程（147 条定理）与 62 个 Kani harness**。

## 2. 检查点

- [x] `F6Expression.lean` 零告警构建与 5 条定理核心公理白名单本机通过（零外部公理）
- [x] `verify_prefix_operator_and_when_condition_dispatch_exhaustive`、`verify_return_value_and_string_missing_closer_guards` 与 `verify_group_non_poison_tail_action_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`expression_tests`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（23 片 / 147 定理 / 62 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
