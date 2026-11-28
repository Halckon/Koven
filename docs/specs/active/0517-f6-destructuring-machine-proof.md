# SPEC-0517：F6 第十七片 `destructuring` 局部解构起始、绑定循环防重报状态机与初始化器守卫双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十七片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0516](0516-f6-file-modifiers-machine-proof.md) |
| 基线 | `main@f2ea2feb`（叠加于 `SPEC-0515`–`0516`） |
| 分支 | `feature/spec-0515-f6-postfix-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0516](0516-f6-file-modifiers-machine-proof.md)，
对 [destructuring.rs](../../../crates/lang-frontend/src/parser/engine/destructuring.rs) 中的局部解构前缀识别、绑定列表防双报循环状态机（`has_bindings` / `comma_already_reported_as_empty` / `needs_binding`）与初始化器缺失诊断守卫交付 **Lean 4（无界状态与任意长度事件流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [destructuring.rs](../../../crates/lang-frontend/src/parser/engine/destructuring.rs) 中提炼纯栈值内核（`DestructuringEvent`、`DestructuringExitKind`、`DestructuringLoopState`、`is_local_destructuring_start_kinds`、`is_const_local_destructuring_start_kinds`、`destructuring_exit_emits_missing_binding`、`step_destructuring_loop`、`#[cfg(any(test, kani))] run_destructuring_loop` 与 `destructuring_initializer_missing_diagnostic`），接入 `local_destructuring_start`、`const_local_destructuring_start`、`parse_local_destructuring` 与 `parse_destructuring_initializer`，严格保留短路求值与零行为变化，并在 [destructuring_tests.rs](../../../crates/lang-frontend/src/parser/engine/destructuring_tests.rs) 补充 3 个单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod destructuring_kani;` 与 `#[cfg(test)] mod destructuring_tests;`。
2. **轨道 1（Lean 4 模型定理：1 条 `unbounded` + 5 条 `complete finite domain`）**：
   - [formal/parser/f6_destructuring/F6Destructuring.lean](../../../formal/parser/f6_destructuring/F6Destructuring.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 6 条定理（D1–D6）：
     - 有限域模型定理（5 条，`proved (model, complete finite domain)`）：
       - `isLocalDestructuringStart_spec`（D1：仅限 `LocalDestructuringKeyword`（`val`/`var`）的局部解构前缀充要条件）
       - `isConstLocalDestructuringStart_spec`（D2：`const val (` 三词法单元前缀充要条件）
       - `exitEmitsMissingBinding_spec`（D3：`RightParen` 与 `Boundary` 两类退出点缺失绑定诊断充要条件）
       - `stepDestructuringLoop_preserves_invariant`（D4：单步 `hasBindings` 单调性与 `commaAlreadyReported → needsBinding ∧ hasBindings` 不变式保持）
       - `initializerMissingDiagnostic_spec`（D6：边界处 `boundary && !starts_primary` 缺失初始化器诊断的五元守卫充要条件；非边界非法首记号路径如 `val (x) = /` 在 `destructuring.rs:511–515` 另行发报，不在本守卫范围内）
     - 无界列表模型定理（1 条，`proved (model, unbounded)`）：
       - `runDestructuringLoop_preserves_invariant`（D5：任意长度事件流保持该不变式，且任意前缀追加 `.emptyComma` 后必然置位 `commaAlreadyReported` 并使 `)` 与边界两类退出均不重报缺失绑定）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [destructuring_kani.rs](../../../crates/lang-frontend/src/parser/engine/destructuring_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_destructuring_start_predicates`：直接调用生产 `is_local_destructuring_start_kinds` 与 `is_const_local_destructuring_start_kinds`，通过单源自动计数宏覆盖全部生产 `LexemeKind` / `Keyword` 变体。
     - `verify_destructuring_exit_and_initializer_guards`：直接调用生产 `destructuring_exit_emits_missing_binding` 与 `destructuring_initializer_missing_diagnostic`，覆盖全部退出类型与布尔组合。
     - `verify_destructuring_loop_state_invariant_len_6`：直接调用 `run_destructuring_loop`（`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意事件序列并验证防重报不变式。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_destructuring/**`、`crates/lang-frontend/src/parser/engine/destructuring.rs` 与 `crates/lang-frontend/src/parser/engine/destructuring_kani.rs`，统一校验全部 **17 份 F6 Lean 4 证明工程（115 条定理）与 44 个 Kani harness**。

## 2. 检查点

- [x] `F6Destructuring.lean` 零告警构建与 6 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_destructuring_start_predicates`、`verify_destructuring_exit_and_initializer_guards` 与 `verify_destructuring_loop_state_invariant_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`destructuring_tests`（3/3）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（17 片 / 115 定理 / 44 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
