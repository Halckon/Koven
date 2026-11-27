# SPEC-0514：F6 第十四片 `block` 块语句分隔符、换行边界、显式 `block {` 与缺失块根扫描双轨证明

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F6 第十四片局部机器证明与双轨 CI 时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 局部形式化验证（F6 延伸） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@36012f86` |
| 分支 | `feature/spec-0514-f6-block-boundary` |

本 Spec 承接 [F6 局部机器证明前置设计](../../proposals/parser-formal-verification/f6-machine-proof-design.md) 与 [SPEC-0513](../../archive/specs/0513-f6-supertypes-boundary-machine-proof.md)，
对 [block.rs](../../../crates/lang-frontend/src/parser/engine/block.rs) 中块不支持元素识别、块表达式换行续行边界、显式 `block {` 起始守卫、同行语句分隔符要求（含 `while`/`for` 豁免）及缺失块根尾部扫描内核交付 **Lean 4（无界状态与任意长度词法流模型定理）+ Kani（零堆分配生产 Rust 符号检验）** 双轨证明，并接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml)。
生产解析行为与公开 Rust API 零变化，不引入 Cargo 生产依赖，不宣称全输入生产 G4。

## 1. 交付合同

1. **生产零分配内核抽取与零行为变化**：
   - 在 [block.rs](../../../crates/lang-frontend/src/parser/engine/block.rs) 中提炼纯栈值内核（`unsupported_block_element_kind`、`block_expression_line_break_boundary`、`is_explicit_block_start`、`block_element_separator_required`、`step_missing_block_root_end` 与 `#[cfg(any(test, kani))] run_missing_block_root_end`），由 `at_block_expression_line_break`、`parse_missing_block_root`、`explicit_block_start` 与 `check_block_element_separator` 直接调用，保持原有短路求值与解析行为零变化，并补充 5 个单元测试。
   - 在 [engine.rs](../../../crates/lang-frontend/src/parser/engine.rs) 中仅挂载 `#[cfg(kani)] mod block_kani;`。
2. **轨道 1（Lean 4 无界模型定理，`proved (model, unbounded)`）**：
   - [formal/parser/f6_block_boundary/F6BlockBoundary.lean](../../../formal/parser/f6_block_boundary/F6BlockBoundary.lean)（零 Mathlib，固定 `leanprover/lean4:v4.14.0`）证明 7 条无界定理（B1–B7）：
     - `unsupportedBlockElement_spec`（B1：精确刻画全部 9 个不支持块元素关键字）
     - `blockExprLineBreakBoundary_spec`（B2：精确刻画块表达式换行边界当且仅当处于块上下文、存在换行且遇到 `(`/`+`/`-`）
     - `isExplicitBlockStart_spec`（B3：精确刻画显式 `block {` 起始当且仅当标识符为 `block`、后继为 `{` 且同行无换行）
     - `blockElementSeparatorRequired_spec`（B4：精确刻画块元素分隔符要求的完整充要条件）
     - `blockElementSeparatorRequired_while_for_exempt_from_unsupported`（B5：证明 `while`/`for` 虽属不支持元素集但在同行仍要求分隔符，其余不支持关键字被抑制）
     - `runMissingBlockRootEnd_eq_last_content_before_eof`（B6：任意长度词法流上 `runMissingBlockRootEnd` 精确等于首个 `eof` 之前最后一个非 trivia/非 `eof` 词法单元的 `end`）
     - `runMissingBlockRootEnd_ignores_trailing_trivia_and_after_eof`（B7：任意前置流后追加尾部 trivia 与 `eof` 后任意后缀均不改变结果）
3. **轨道 2（Kani 零堆分配生产 Rust 符号检验，`validated (bounded Rust)`）**：
   - [block_kani.rs](../../../crates/lang-frontend/src/parser/engine/block_kani.rs)（`#[cfg(kani)]` 隔离，零堆分配）提供 3 条 harness：
     - `verify_block_unsupported_line_break_and_explicit_start`：直接调用生产 `unsupported_block_element_kind`、`block_expression_line_break_boundary` 与 `is_explicit_block_start`，通过单源宏（`define_exhaustive_unit_enum_any!` / `define_exhaustive_enum_any!` + `#[deny(unreachable_patterns)]`）在编译期同时阻断漏项与重复项，符号覆盖生产 `LexemeKind` 全部变体及 `Option<LexemeKind>`。
     - `verify_block_element_separator_required_matrix`：直接调用生产 `block_element_separator_required`，符号覆盖全部生产 `LexemeKind` 与全部 6 个布尔控制位（64 种组合），并专项校验 `while`/`for` 豁免。
     - `verify_missing_block_root_end_run_len_6`：直接调用 `run_missing_block_root_end`（驱动生产 `step_missing_block_root_end`，`#[kani::unwind(8)]`），符号覆盖长度 `0..=6` 的任意 `(LexemeKind, usize)` 序列。
4. **路径触发轻量 CI 扩展**：
   - [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 同步触发 `formal/parser/f6_block_boundary/**`、`crates/lang-frontend/src/parser/engine/block.rs` 与 `crates/lang-frontend/src/parser/engine/block_kani.rs`，统一校验全部 **14 份 F6 Lean 4 证明工程（95 条定理）与 35 个 Kani harness**。

## 2. 检查点

- [x] `F6BlockBoundary.lean` 零告警构建与 7 条定理核心公理白名单本机通过（仅依赖 `propext`）
- [x] `verify_block_unsupported_line_break_and_explicit_start`、`verify_block_element_separator_required_matrix` 与 `verify_missing_block_root_end_run_len_6` 本机 Kani 0.68.0 实跑通过（3/3 harnesses, 0 failures），`block`（5/5）/ `separator_tests`（13/13）单测、`cargo fmt --check` 与 `cargo clippy --all-targets` 通过
- [ ] 接入 `parser-formal-f6.yml`（14 片 / 95 定理 / 35 harness），待 PR 远端主 CI 与 F6 双轨作业实跑验收后归档
