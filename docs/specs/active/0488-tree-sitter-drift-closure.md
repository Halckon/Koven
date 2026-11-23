# SPEC-0488：收口 `editors/tree-sitter` 已知文法漂移（TS-03 / TS-05 / TS-07）

> **性质**：编辑器文法对齐合同 · **状态**：in-progress · **读取时机**：收口 F4b 已登记的 Tree-sitter 文法漂移时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 编辑器文法与差分测试；不改变生产 Parser 或语言规范 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@5ea16bb8` |
| 分支 | `feature/spec-0488-tree-sitter-drift-closure` |

本 Spec 承接 [SPEC-0481](0481-tree-sitter-difftest.md) 与 [F4b 漂移基线](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)，
在不修改生产 Parser/Lexer、Guide 或新增 Cargo 依赖的前提下，在 `editors/tree-sitter` 中收口 **TS-03**（6 个具名位运算符）、
**TS-05**（块与 lambda 内分号分隔）与 **TS-07**（跨行 `move\n{ ... }`），并将这三类见证从 `known_drift` 迁入 F4b 共同子集双向验证。
剩余 `TS-01`（裸 `\r`）、`TS-02`（块注释内换行分隔）与 `TS-04`（不结合比较链）继续作为显式登记的编辑器文法权衡保留。

## 交付合同

1. **TS-03 具名位运算符**：在 `editors/tree-sitter/grammar.js` 与 `src/scanner.c` 按 Guide 04 优先级（`SHIFT` > `BITWISE_AND` > `BITWISE_XOR` > `BITWISE_OR` > `RANGE`）
   新增 6 个上下文软中缀运算符（`shl`/`shr`/`ushr`/`and`/`xor`/`or`），同时保证它们在声明名、参数名与普通调用位置仍保持普通 `identifier`。
2. **TS-05 块内分号分隔**：在 `block`、`control_block`、`lambda_expression`、`trailing_lambda_expression` 与 `same_line_trailing_lambda_expression`
   中支持 `;` 作为语句分隔符（如 `fun f() { a(); b() }` 与 `{ a(); b() }`）。
3. **TS-07 跨行 `move` 前瞻**：在 `scanner.c` 中对 `move` 启用跨行 trivia 前瞻（同时保持 `block` 的同行约束不变），使 `val f = move\n{ 1 }` 与含注释跨行 `move` 正确识别为 `lambda_expression`。
4. **双侧验收与基线同步**：重新生成 `editors/tree-sitter/src/parser.c` 及元数据并通过 `tree-sitter test` 与 `test_contract.py`；
   在 `crates/lang-frontend/tests/parser_tree_sitter_diff.rs` 中将 `TS-03`/`TS-05`/`TS-07` 迁入 `common_subset_and_spec_0479_differential`，
   同步更新 [tree-sitter-diff-baseline.md](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)。

## 本地运行

```bash
# 1. Tree-sitter 生成一致性、语料与契约测试
npm run generate --prefix editors/tree-sitter
git diff --exit-code -- editors/tree-sitter/src
npm test --prefix editors/tree-sitter

# 2. Rust <-> Tree-sitter 完整文件差分
KOVEN_TREE_SITTER=/absolute/path/to/tree-sitter \
  cargo test --locked -p lang-frontend --test parser_tree_sitter_diff -- --ignored --nocapture
```

## 检查点

- [x] `grammar.js`、`src/scanner.c` 与生成产物同步更新并通过语料（10/10）及契约测试（18/18）
- [x] `parser_tree_sitter_diff`（默认 + 锁定 CLI `--ignored`）与 `tree_sitter_grammar` 实跑通过
- [ ] PR 双宿主与 `editors` CI 验收与归档
