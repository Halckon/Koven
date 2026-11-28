# SPEC-0519：F6 第十九片 `lambda` 块体循环分派、分号跳过、缺失闭合与 `const val` 守卫双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十九片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0518](../../archive/specs/0518-f6-type-ref-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0518](../../archive/specs/0518-f6-type-ref-machine-proof.md)，
对 [lambda.rs](../../../crates/lang-frontend/src/parser/engine/lambda.rs) 中的 lambda 体循环动作分派（`Stop`/`SkipSemicolon`/`ParseElement`）、连续分号跳过、不支持符号识别（`,`/`->`）、缺失右花括号诊断守卫与不支持形式 `const val` 双词法单元消费内核交付 **Lean 4（模型定理：2 条 `unbounded` + 4 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [lambda.rs](../../../crates/lang-frontend/src/parser/engine/lambda.rs) 中提炼纯栈值内核（`LambdaBodyLoopAction`、`lambda_body_loop_action`、共享生产与验证游标推进的 `skip_lambda_body_semicolons_with`、`is_lambda_body_unsupported_symbol_kind`、`lambda_body_missing_closer_emits_diagnostic` 与 `unsupported_lambda_body_consumes_val_kind`），接入 `parse_lambda_body` 与 `parse_unsupported_lambda_body_form`，严格保留原有短路求值与解析行为零变化，并在 [lambda_tests.rs](../../../crates/lang-frontend/src/parser/engine/lambda_tests.rs) 补充 4 个单元测试（含生产入口 `lambda_body_production_entry_skips_consecutive_semicolons_and_parses_element`）。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod lambda_kani;` 与 `#[cfg(test)] mod lambda_tests;`。
2. **轨道 1（Lean 4 模型定理：2 条 `unbounded` + 4 条 `complete finite domain`）**：
   - [formal/parser/f6_lambda_body/F6LambdaBody.lean](../../../formal/parser/f6_lambda_body/F6LambdaBody.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 6 条定理（L1–L6）：
     - 有限域模型定理（4 条，`proved (model, complete finite domain)`）：
       - `lambdaBodyLoopAction_spec`（L1：三态循环动作 `stop`/`skipSemicolon`/`parseElement` 的完整充要条件）
       - `isLambdaBodyUnsupportedSymbol_spec`（L2：不支持符号当且仅当 `,` 或 `->`）
       - `missingCloserEmitsDiagnostic_spec`（L3：缺失右花括号诊断守卫充要条件）
       - `unsupportedConsumesVal_spec`（L4：不支持形式双词法单元消费当且仅当 `const` 后接 `val`）
     - 无界列表模型定理（2 条，`proved (model, unbounded)`）：
       - `skipLambdaBodySemicolons_stops_at_non_semicolon`（L5：任意长度序列上返回下标 `k ≤ xs.length`，所有前缀均满足非硬停止分号，且 `xs[k]` 非可跳过分号）
       - `skipLambdaBodySemicolons_prepend_semicolons`（L6：前置任意 `m` 个非硬停止分号精确使停止下标平移 `m`）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [lambda_kani.rs](../../../crates/lang-frontend/src/parser/engine/lambda_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_lambda_body_loop_action_and_unsupported_symbol`：直接调用生产 `lambda_body_loop_action` 与 `is_lambda_body_unsupported_symbol_kind`，通过单源自动计数 `#[deny(unreachable_patterns)]` 宏覆盖全部生产 `LexemeKind` 变体，并以独立预期分类核对结果。
     - `verify_lambda_body_missing_closer_and_const_val_guards`：直接调用生产 `lambda_body_missing_closer_emits_diagnostic` 与 `unsupported_lambda_body_consumes_val_kind`，覆盖全部生产 `LexemeKind`、`Option<LexemeKind>` 与布尔组合。
     - `verify_lambda_body_semicolon_skip_len_6`：直接调用生产共享游标推进函数 `skip_lambda_body_semicolons_with`（`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意 `(LexemeKind, bool)` 序列并验证前缀全为可跳过分号、停止位非可跳过分号且下标有界。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_lambda_body/**`、`crates/lang-frontend/src/parser/engine/lambda.rs` 与 `crates/lang-frontend/src/parser/engine/lambda_kani.rs`，统一校验全部 **19 份 F6 Lean 4 证明工程（127 条定理）与 50 个 Kani harness**。

## 2. 检查点

- [x] `F6LambdaBody.lean` 零告警构建与 6 条定理核心公理白名单本机通过（仅依赖 `propext`、`Quot.sound`）
- [x] `verify_lambda_body_loop_action_and_unsupported_symbol`、`verify_lambda_body_missing_closer_and_const_val_guards` 与 `verify_lambda_body_semicolon_skip_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`lambda_tests`（4/4，含生产入口 `lambda_body_production_entry_skips_consecutive_semicolons_and_parses_element`）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（19 片 / 127 定理 / 50 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
