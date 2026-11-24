# SPEC-0493：F4b 完整文件 Tree-sitter 差分路径触发轻量 CI 接入

> **性质**：CI 门禁接线合同 · **状态**：in-progress · **读取时机**：将 F4b Tree-sitter 完整文件差分接入持续集成时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 形式化验证 CI 接线（F4b） |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `feature/spec-0492-f6-ordinary-groups-proof@81a0358f` |
| 分支 | `feature/spec-0493-f4b-difftest-ci` |

本 Spec 承接 [SPEC-0481](0481-tree-sitter-difftest.md)、[SPEC-0488](0488-tree-sitter-drift-closure.md) 与 [SPEC-0491](0491-tree-sitter-remaining-drift-closure.md)：
在 TS-01/TS-02/TS-03/TS-04/TS-05（分号部分）/TS-06/TS-07 完成收口后，新增独立的路径触发轻量 workflow [.github/workflows/parser-formal-f4b.yml](../../../.github/workflows/parser-formal-f4b.yml)，
在每次修改 `editors/tree-sitter/**`、`crates/lang-frontend/src/lexer/**`、`crates/lang-frontend/src/parser/**` 或 `crates/lang-frontend/tests/parser_tree_sitter_diff.rs` 时，
使用锁定 `editors/tree-sitter` CLI 实跑 `parser_tree_sitter_diff -- --ignored`，防止编辑器文法与生产 Parser 再次产生静默漂移。
本 Spec 不修改主 `.github/workflows/ci.yml` 的 job 名称与复用合同，对无关 PR 零耗时影响。

## 交付合同

1. **路径触发轻量 workflow**：新增 [.github/workflows/parser-formal-f4b.yml](../../../.github/workflows/parser-formal-f4b.yml)（先以 advisory 独立 workflow 运行），在 `ubuntu-24.04` 上锁定 Node 22 + Rust 1.96.0 与 `Swatinem/rust-cache@v2`。
2. **双阶段真实执行与结果断言**：
   - 先运行 `npm ci --prefix editors/tree-sitter`、`npm run generate`、`git diff --exit-code -- editors/tree-sitter/src` 与 `npm test --prefix editors/tree-sitter`；
   - 再设置 `KOVEN_TREE_SITTER="$PWD/editors/tree-sitter/node_modules/.bin/tree-sitter"` 运行默认 `parser_tree_sitter_diff`（故障对照）及 `-- --ignored`（完整文件双向差分），并断言 `2 passed; 0 failed; 0 ignored`。
3. **不破坏既有 CI 策略测试**：通过 `python3 -m unittest discover -s scripts/tests -v` 与 `python3 scripts/check_docs.py`。

## 检查点

- [x] `.github/workflows/parser-formal-f4b.yml` 落地且与本地 `parser_tree_sitter_diff -- --ignored` 执行命令一致
- [x] `scripts/tests`、`scripts/check_docs.py` 与 `parser_tree_sitter_diff`（默认 + `--ignored`）本地通过
- [ ] PR CI 中 `Parser Formal F4b (Tree-sitter Differential)` 实跑验收与归档
