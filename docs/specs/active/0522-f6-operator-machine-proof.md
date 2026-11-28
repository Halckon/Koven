# SPEC-0522：F6 第二十二片 `operator` 非法相邻运算符、表达式/类型起始、毒化层级与中缀规则表双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十二片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0521](0521-f6-declaration-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0521](0521-f6-declaration-machine-proof.md)，
对 [operator.rs](../../../crates/lang-frontend/src/parser/engine/operator.rs) 中的非法相邻双符号（`++`/`--`/`<<`/`>>`/`...`）、表达式与类型起始判定、毒化层级关系、声明尾随毒化守卫以及 31 个中缀运算符规则表（`infix_rule_from_parts`）交付 **Lean 4（模型定理：5 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [operator.rs](../../../crates/lang-frontend/src/parser/engine/operator.rs) 中提炼纯栈值内核（`is_unsupported_adjacent_operator_pair`、`can_start_expression_kind`、`can_start_type_ref_kind`、`is_poison_lexeme_kind` 与 `is_declaration_trailing_poison_decision`），与既有纯生产函数 `infix_rule_from_parts` 共同接入 `unsupported_operator`、`can_start_expression`、`can_start_type_ref`、`is_poison_kind` 与 `is_declaration_trailing_poison`，严格保留短路求值（非毒化且非 `StringStart` 短路 `lexical_recoveries` 查询）与解析行为零变化。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod operator_kani;` 与 `#[cfg(test)] mod operator_tests;`，并在 [operator_tests.rs](../../../crates/lang-frontend/src/parser/engine/operator_tests.rs) 补充 4 个单元测试（含生产解析入口 `parse` 对右结合 `?:` 与类型右操作数 `as`/`is` 的端到端验证）。
2. **轨道 1（Lean 4 模型定理：5 条 `complete finite domain`）**：
   - [formal/parser/f6_operator/F6Operator.lean](../../../formal/parser/f6_operator/F6Operator.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条有限域模型定理（O1–O5，`proved (model, complete finite domain)`）：
     - `unsupportedAdjacentOperatorPair_spec`（O1：相邻且精确匹配 `++`/`--`/`<<`/`>>`/`...` 充要条件）
     - `starter_and_poison_hierarchy_spec`（O2：类型起始 ⊆ 表达式起始、毒化 ⊆ 表达式起始、毒化与类型起始严格互斥）
     - `declarationTrailingPoison_spec`（O3：声明尾随毒化判定充要条件）
     - `infixRule_precedence_and_associativity_spec`（O4：31 个中缀规则全部满足 `1 ≤ prec < 17`，仅 `?:` 与 6 个赋值右结合，非结合组精确落于优先级 `{4, 5, 6, 9}` 且永不右结合）
     - `infixRule_operand_and_soft_keyword_spec`（O5：类型右操作数精确为 `as`/`as?`/`is`/`!is`，标识符匹配当且仅当存在软关键字，非标识符忽略软关键字，毒化词法单元永不形成中缀运算符）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [operator_kani.rs](../../../crates/lang-frontend/src/parser/engine/operator_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_unsupported_adjacent_operator_pair_exhaustive`：全量覆盖 `LexemeKind × LexemeKind × bool`。
     - `verify_expression_type_poison_and_trailing_poison_exhaustive`：全量覆盖 `LexemeKind × bool × bool` 与包含/互斥层级不变量。
     - `verify_infix_rule_from_parts_exhaustive`：全量覆盖 `LexemeKind × Option<SoftInfixKeyword>` 与独立预期规则表、优先级上界、结合性、非结合组及软关键字无关性。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_operator/**` 与 `crates/lang-frontend/src/parser/engine/operator_kani.rs`，统一校验全部 **22 份 F6 Lean 4 证明工程（142 条定理）与 59 个 Kani harness**。

## 2. 检查点

- [x] `F6Operator.lean` 零告警构建与 5 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_unsupported_adjacent_operator_pair_exhaustive`、`verify_expression_type_poison_and_trailing_poison_exhaustive` 与 `verify_infix_rule_from_parts_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`operator_tests`（4/4）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（22 片 / 142 定理 / 59 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
