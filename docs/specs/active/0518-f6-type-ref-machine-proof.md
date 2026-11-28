# SPEC-0518：F6 第十八片 `type_ref` 函数类型起始、扩展接收者点号停止、`proven_type_closer` 守卫与字符串毒化扫描双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十八片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0517](0517-f6-destructuring-machine-proof.md) |
| 基线 | `main@f2ea2feb`（叠加于 `SPEC-0515`–`0517`） |
| 分支 | `feature/spec-0515-f6-postfix-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0517](0517-f6-destructuring-machine-proof.md)，
对 [type_ref.rs](../../../crates/lang-frontend/src/parser/engine/type_ref.rs) 中的函数类型起始识别、扩展接收者点号停止判定、`proven_type_closer` 边界防窃取守卫、有界参数闭合器过滤与分段字符串类型毒化扫描内核交付 **Lean 4（无界状态与任意长度毒化流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [type_ref.rs](../../../crates/lang-frontend/src/parser/engine/type_ref.rs) 中提炼纯栈值内核（`is_function_type_start_kinds`、`is_extension_receiver_dot_stop`、`is_function_type_right_paren_closer`、`bounded_proven_parameter_closer`、`step_segmented_string_type_poison_end` 与 `#[cfg(any(test, kani))] run_segmented_string_type_poison_end`），接入 `parse_type_ref_inner`、`consume_segmented_string_type_poison`、`parse_qualified_type` 与 `parse_function_type`，严格保留短路求值与零行为变化，并在 [type_ref_tests.rs](../../../crates/lang-frontend/src/parser/engine/type_ref_tests.rs) 补充 3 个单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod type_ref_kani;` 与 `#[cfg(test)] mod type_ref_tests;`。
2. **轨道 1（Lean 4 无界模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_type_ref/F6TypeRef.lean](../../../formal/parser/f6_type_ref/F6TypeRef.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 6 条无界定理（R1–R6）：
     - `isFunctionTypeStart_spec`（R1：函数类型起始当且仅当 `(` 或 `move (`）
     - `isExtensionReceiverDotStop_spec`（R2：扩展接收者点号停止当且仅当处于扩展名模式且后继为 `Ident (`）
     - `isFunctionTypeRightParenCloser_spec`（R3：内层函数类型闭合 `)` 永不窃取外层 `provenTypeCloser`）
     - `boundedProvenParameterCloser_spec`（R4：参数闭合器过滤当且仅当严格小于外层 `provenTypeCloser` 上界）
     - `runStringTypePoisonEnd_stops_at_eof_and_recovery_end`（R5：任意长度流上一旦达到 `recoveryEnd` 或遇 `eof` 即刻停止并忽略后续所有词法单元）
     - `runStringTypePoisonEnd_monotone`（R6：单调递增词法端点的任意长度流上扫描结果单调不减）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [type_ref_kani.rs](../../../crates/lang-frontend/src/parser/engine/type_ref_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_type_ref_start_extension_and_closer_guards`：直接调用生产 `is_function_type_start_kinds`、`is_extension_receiver_dot_stop` 与 `is_function_type_right_paren_closer`，由单源自动计数宏覆盖全部生产 `LexemeKind` / `TokenKind` 变体与任意 `usize` / `Option<usize>`。
     - `verify_bounded_proven_parameter_closer_invariants`：直接调用生产 `bounded_proven_parameter_closer`，覆盖任意 `Option<usize>` 组合。
     - `verify_string_type_poison_scan_len_6`：直接调用 `run_segmented_string_type_poison_end`（`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意 `(LexemeKind, usize)` 序列。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_type_ref/**`、`crates/lang-frontend/src/parser/engine/type_ref.rs` 与 `crates/lang-frontend/src/parser/engine/type_ref_kani.rs`，统一校验全部 **18 份 F6 Lean 4 证明工程（121 条定理）与 47 个 Kani harness**。

## 2. 检查点

- [x] `F6TypeRef.lean` 零告警构建与 6 条定理核心公理白名单本机通过（仅依赖 `propext`、`Quot.sound`）
- [x] `verify_type_ref_start_extension_and_closer_guards`、`verify_bounded_proven_parameter_closer_invariants` 与 `verify_string_type_poison_scan_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`type_ref_tests`（3/3）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（18 片 / 121 定理 / 47 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
