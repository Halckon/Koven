# SPEC-0520：F6 第二十片 `borrow_result` 与 `declaration_head` 借用返回、局部借用守卫与扩展接收者前瞻双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第二十片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置 Spec | [SPEC-0519](0519-f6-lambda-body-machine-proof.md) |
| 基线 | `main@f7ad5fd7` |
| 分支 | `feature/spec-0519-f6-lambda-body` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0519](0519-f6-lambda-body-machine-proof.md)，
对 [borrow_result.rs](../../../crates/lang-frontend/src/parser/engine/borrow_result.rs) 中的显式返回来源四路分派（`PlainTypeOnly`/`MissingBorrowFrom`/`ParseBorrowSource`/`ParseCarrierSource`）、局部借用前瞻尾判定与合法性合同（仅普通 `borrow val` 合法），以及 [declaration_head.rs](../../../crates/lang-frontend/src/parser/engine/declaration_head.rs) 中的扩展函数接收者前瞻（`.`/`<`/`?`）交付 **Lean 4（模型定理：5 条 `complete finite domain`）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [borrow_result.rs](../../../crates/lang-frontend/src/parser/engine/borrow_result.rs) 中提炼纯栈值内核（`ExplicitResultAction`、`explicit_result_action`、`is_borrow_local_binding_keyword_kind`、`is_borrow_local_start_tail` 与 `borrow_local_contract_is_invalid`），接入 `parse_explicit_result`、`borrow_local_start` 与 `parse_borrow_local`，严格保留原有短路求值（非 `borrow`/`inout` 不前瞻 `peek(1)`，非 `?` 不前瞻 `peek(2)`）与解析行为零变化。
   - 在 [declaration_head.rs](../../../crates/lang-frontend/src/parser/engine/declaration_head.rs) 中提炼 `is_extension_receiver_follow_kind`（及 `#[cfg(any(test, kani))] is_extension_receiver_head_kind`），接入 `parse_function_head` 且保持非标识符短路。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中挂载 `#[cfg(kani)] mod borrow_result_kani;` 与 `#[cfg(test)] mod borrow_result_tests;`，并在 [borrow_result_tests.rs](../../../crates/lang-frontend/src/parser/engine/borrow_result_tests.rs) 补充 3 个单元测试。
2. **轨道 1（Lean 4 模型定理：5 条 `complete finite domain`）**：
   - [formal/parser/f6_borrow_result/F6BorrowResult.lean](../../../formal/parser/f6_borrow_result/F6BorrowResult.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 5 条有限域模型定理（B1–B5，`proved (model, complete finite domain)`）：
     - `explicitResultAction_spec`（B1：四路返回来源分派充要条件，缺失 `from` 诊断当且仅当 `hasBorrow ∧ ¬hasFrom`）
     - `borrowLocalStartTail_spec`（B2：局部借用尾前瞻精确充要条件，覆盖直接 `val`/`var` 与条件 `? val`/`? var`）
     - `borrowLocalContractInvalid_spec`（B3：唯一合法组合为普通 `borrow val`，其余 7 种组合均触发 `INVALID_BORROW_CONTRACT`）
     - `extensionReceiverHead_spec`（B4：扩展函数接收者前瞻精确充要条件为标识符后接 `.`/`<`/`?`）
     - `borrowLocal_and_extensionReceiver_disjoint`（B5：绑定关键字 `val`/`var` 与扩展接收者后继互斥，且单独 `?` 后须接绑定关键字才触发局部借用）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [borrow_result_kani.rs](../../../crates/lang-frontend/src/parser/engine/borrow_result_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_explicit_result_action_and_borrow_local_contract`：直接调用生产 `explicit_result_action` 与 `borrow_local_contract_is_invalid`，覆盖全部布尔组合并校验独立预期值。
     - `verify_borrow_local_start_tail_exhaustive`：直接调用生产 `is_borrow_local_binding_keyword_kind` 与 `is_borrow_local_start_tail`，通过单源自动计数 `#[deny(unreachable_patterns)]` 宏覆盖全部生产 `LexemeKind` 与 `Option<LexemeKind>` 变体。
     - `verify_extension_receiver_head_exhaustive`：直接调用生产 `is_extension_receiver_follow_kind` 与 `is_extension_receiver_head_kind`，覆盖全部生产 `LexemeKind` 变体并校验与绑定关键字互斥。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_borrow_result/**`、`crates/lang-frontend/src/parser/engine/borrow_result.rs`、`crates/lang-frontend/src/parser/engine/borrow_result_kani.rs` 与 `crates/lang-frontend/src/parser/engine/declaration_head.rs`，统一校验全部 **20 份 F6 Lean 4 证明工程（132 条定理）与 53 个 Kani harness**。

## 2. 检查点

- [x] `F6BorrowResult.lean` 零告警构建与 5 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_explicit_result_action_and_borrow_local_contract`、`verify_borrow_local_start_tail_exhaustive` 与 `verify_extension_receiver_head_exhaustive` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`borrow_result_tests`（3/3）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（20 片 / 132 定理 / 53 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
