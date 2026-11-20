# SPEC-0479: 显式词法块 `block { ... }` 与带括号调用跨行尾 lambda 附着

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收 `block { ... }` 显式词法块、取消独立裸 `{ ... }` 语句与带括号调用跨行尾 lambda 附着时 · **唯一真源**：本 Spec 的范围、边界与验收；语言语义归 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0479` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-19 `#koven-design` 全员共识（Halckon、Claude、Cooper、Grok、Dot、Gemini） |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `docs/guide/03-types-generics.md`、`docs/guide/04-expressions-operators.md`、`docs/guide/06-blocks-control-flow.md`、`docs/guide/07-calls-lambdas-closures.md`、`crates/lang-frontend/`、`editors/tree-sitter/`、全仓 `.ko` 与测试夹具 |
| 语言语义变更 | 是 |

## 1. Goal

按 `#koven-design` 最终共识落地「显式词法块 `block { ... }`（按形状保留，不设全局硬关键字）+ 取消语句位置独立裸 `{ ... }` + 仅允许带圆括号调用跨行附着唯一尾 lambda（含 `f<T>()`）」，同步 D2/D3 规范澄清，并完成 Parser、诊断、Formatter、Tree-sitter 及全仓受影响夹具迁移。

## 2. 冻结的语法与语义合同

### 2.1 显式词法块 `block { ... }`（按形状保留，不设全局硬关键字）

- **取消独立裸 `{ ... }` 执行语句**：在 block/lambda body 语句位置裸写 `{ ... }` 必须明确报语法错并提示改用 `block { ... }`；函数体、`if`/`while`/`for`/`loop`/`when`/`deinit` 等已有语法引导的块不变。
- **按形状识别**：仅当**未限定名 `block` 与 `{` 处于同一行且位于语句起点**时，解析为显式词法块（`Statement::Block`）；它不建闭包、不捕获，完全保留原有词法作用域、`return`/`break`/`continue` 跳转目标与提前 drop 边界。
- **表达式位置明确拒绝**：`val x = block { ... }` 等表达式位置出现未限定名 `block` 同行接 `{` 时，**明确报语法错，绝不回退成普通函数调用**。
- **标识符复用保留**：不将 `block` 设为全局硬关键字；`val block = ...`、`block(...)`、`obj.block { ... }` 均继续合法，同名高阶函数调用可显式写为 `block({ ... })`。

### 2.2 跨行尾 lambda 附着（首版仅限带圆括号调用）

- **跨行准入**：仅当调用已通过圆括号实参列表 `(...)`（含 `f()`、`f(a)`、`f<T>()`、`obj.m()`）闭合后，允许跨换行（LF、CRLF、注释与空行）附着唯一一个尾 lambda `{ ... }`；不得越过 `;`、caller hard stop 或其他 owner。
- **位置统一**：在 block element、变量初始化式（`val x = f()\n{ ... }`）、`return f()\n{ ... }`、单表达式控制体（`if (ok) f()\n{ ... }`）与 `when` 分支中均按同一规则附着；不看返回类型，类型检查失败也不改 AST。
- **同行保留与非目标**：同行无括号调用 `html { ... }`、`f<T> { ... }` 保持不变；**裸名跨行 `f\n{ ... }` 与无圆括号泛型跨行 `f<T>\n{ ... }` 首版均不附着**；每个调用至多一个尾 lambda，第二组 `{ ... }` 报错。

### 2.3 独立 `move { ... }` 与 D2/D3 澄清

- 单独的 `move { ... }` 不与裸执行块混为一谈；即使后续拒绝其作为无用语句，诊断也不得提示直接改成 `block { ... }`（移动捕获与直接执行语义不同）。
- 同步 D2（从 Guide 07 `L0032` 延后清单移除 `return`，回链 Guide 06 最近 callable 边界）与 D3（表达式等待 primary 且下一非 trivia 为 `{` 时提交 `move` lambda，其余保持普通 `Identifier`）。

## 3. 实施计划与验收清单

1. [x] **Guide 同步**：更新 `03`/`04`/`06`/`07` 正文与 EBNF，消除旧裸块与同行限制冲突 → 验证：`python3 scripts/check_docs.py`
2. [x] **Parser 与诊断实现**：在 `lang-frontend` 落地 `block { ... }` 语句识别、`val x = block { ... }` 显式拒绝、取消语句位置裸 `{ ... }`、`parse_call` 跨行尾 lambda 附着（保持 `f<T>\n{}` 不附着）与 `move { ... }` 隔离诊断 → 验证：`parser_block`、`parser_trailing_lambda`、`parser_block_line_continuation`、`parser_lambda`、`parser_return_control`、`parser_expression`
3. [x] **Formatter 与 Tree-sitter 同步**：更新 `lang-frontend` Formatter 输出 `block { ... }` 并保证 SPEC-0477 往返等价；同步 `editors/tree-sitter/grammar.js` 与 `scanner.c` → 验证：`formatting` 目标与 Tree-sitter 契约测试
4. [x] **全仓 `.ko` 与测试夹具迁移**：将 `lang-frontend`、`lang-codegen`、`lang-cli` 中既有独立裸执行块统一迁移为 `block { ... }`（含 `coexisting.rs`、`coexisting_paths.rs` 及 6 个 `.ko` fixture 等） → 验证：全量受影响测试与双宿主 CI
5. [x] **表面语法增量门禁（F5）**：核对规则归属、离线冲突变化、软词/换行谓词、R1–R5 恢复边界与独立正反例

## 4. 验收记录

- 验收提交 `7398e2ee`（PR #453）。本地执行 §3 的六个 Parser 目标和 `formatting`：154 项通过。
- F5 核对：Guide 将显式块归入语句，将跨行附着归入调用；软词仅在未限定名同行接 `{` 时特殊处理，跨行只接已闭合的圆括号调用。既有离线块差分作为回归证据；新增语法由 Parser 与独立 Tree-sitter 正反例覆盖，不将现有离线语料视为新增语法的完整证明。
- 恢复回归覆盖次级 lambda、caller 终止符、未闭合和 lexer 错误的根诊断、owner 归属及共享资源预算。
- [PR CI 第 2 次尝试](https://github.com/Halckon/Koven/actions/runs/44755356932/attempts/2)：文档结构、Tree-sitter 词法合同、macOS/Ubuntu 的 Clippy 和定向测试均通过；双平台 Preview 构建与独立安装通过，`CI Passed` 与 `Code CI Evidence` 均通过。
