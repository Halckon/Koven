# SPEC-0501：收口 `editors/tree-sitter` 块与 Lambda 同行无分隔符语句漂移（TS-05 闭环）

> **性质**：编辑器文法对齐合同 · **状态**：in-progress · **读取时机**：收口 F4b 漂移基线中 TS-05 剩余的同行无分隔符语句漂移时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 编辑器文法与差分测试；不改变生产 Parser 或语言规范 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@a6d2d51d` |
| 分支 | `feature/spec-0498-f6-group-owners-proof` |

本 Spec 承接 [SPEC-0488](0488-tree-sitter-drift-closure.md)、[SPEC-0491](../../archive/specs/0491-tree-sitter-remaining-drift-closure.md) 与
[F4b 漂移基线](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)，收口基线中最后 1 项开放漂移——**TS-05 块与 Lambda 内相邻语句强制换行或分号分隔**（拒绝如 `fun f() { 1 2 }`、`if (ok) { a() b() }`、`val g = { x() y() }` 等同行无分隔符写法，同时保留空块、尾随分号/换行、多分隔符及跨行块注释分隔）。至此 F4b 基线登记的 **TS-01 至 TS-07 全部 7 项已知漂移彻底闭环**。

## 交付合同

1. **强制分隔符块体规则**：在 [editors/tree-sitter/grammar.js](../../../editors/tree-sitter/grammar.js) 中引入 `_block_separator`（`choice($._newline, ";")`）与 `separatedBody($, elem)`，应用于 `block`、`control_block`、`lambda_expression`、`trailing_lambda_expression` 与 `same_line_trailing_lambda_expression`，使相邻语句之间必须出现至少一个换行（含跨行块注释）或分号。
2. **生成产物与双侧回归**：重新生成 `editors/tree-sitter/src/*`（0 警告、0 未解冲突），在 [test_contract.py](../../../editors/tree-sitter/test/test_contract.py) 与 [parser_tree_sitter_diff.rs](../../../crates/lang-frontend/tests/parser_tree_sitter_diff.rs) 补充块、控制块、lambda 及尾随 lambda 的同行无分隔符拒收与合法分隔对照用例。
3. **漂移基线同步**：更新 [tree-sitter-diff-baseline.md](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md) 登记 TS-05 完整闭环。

## 检查点

- [x] `grammar.js` 与生成产物同步更新并通过 `tree-sitter test`（10/10）与 `test_contract.py`（21/21）
- [x] `parser_tree_sitter_diff`（默认 1/1 + 锁定 CLI `--ignored` 2/2）实跑通过，TS-01–TS-07 全部闭环
- [ ] PR 双宿主、`editors` 与 `Parser Formal F4b` CI 验收与归档
