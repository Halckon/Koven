# SPEC-0491：收口 `editors/tree-sitter` 剩余已知文法漂移（TS-01 / TS-02 / TS-04）

> **性质**：编辑器文法对齐合同 · **状态**：done · **读取时机**：收口 F4b 剩余 Tree-sitter 已知文法漂移时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 编辑器文法与差分测试；不改变生产 Parser 或语言规范 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@e2e5cc26` |
| 分支 | `feature/spec-0491-tree-sitter-remaining-drifts` |

本 Spec 承接 [SPEC-0481](0481-tree-sitter-difftest.md)、[SPEC-0488](0488-tree-sitter-drift-closure.md) 与
[F4b 漂移基线](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)，在不修改生产 Parser/Lexer、Guide 或新增 Cargo 依赖的前提下，
在 `editors/tree-sitter` 中收口 3 项已登记文法漂移——**TS-01**（拒绝裸 `\r` 与 `\f` 空白，保留合法 LF/CRLF）、
**TS-02**（跨行块注释 `/* ... \n ... */` 作为换行语句分隔符，单行块注释保持水平 `extras`，同时保留运算符后与跨行尾随 lambda 的跨行块注释续行）与
**TS-04**（四组不结合运算符 `..`/`..<`、`in`/`!in`/`is`/`!is`、`<`/`>`/`<=`/`>=`、`==`/`!=` 禁止无括号链式拼接）；
TS-05 中同行无分隔符语句拒绝（如 `fun f() { 1 2 }`）仍作为显式登记的编辑器文法开放项保留。

## 交付合同

1. **TS-01 裸 `\r` 与 `\f` 空白拒绝**：将 `editors/tree-sitter/grammar.js` 的 `extras` 收窄为 `/[ \t]/`（移除 `\r`/`\f`），并同步更新 `editors/tree-sitter/src/scanner.c` 的空白前瞻与 `CROSSLINE_LAMBDA_NL`，使裸 `\r` 与 `\f` 产生词法/语法错误，同时保留合法 LF（`\n`）与 CRLF（`\r\n`）换行。
2. **TS-02 跨行块注释触发换行分隔**：将 `block_comment` 拆分为单行 `block_comment`（留在 `extras`）与必含换行的 `multiline_block_comment`（经 `alias(..., $.block_comment)` 纳入 `_newline`），使 `a/*\n*/-b` 按 Guide 01/06 拆分为前缀语句 `-b`，而 `a/* inline */-b` 保持同行二元减法。
3. **TS-04 四组不结合运算符分层**：按 Guide 04 18 级优先级对 `grammar.js` 表达式操作数完成无循环分层（`_equality_binary`、`_comparison_binary`、`_membership_binary`、`_elvis_binary`、`_to_binary`、`_range_binary`、`_bitwise_or_higher_binary`、`_cast_operand`、`_prefix_operand`、`_postfix_operand`），拒绝所有四组不结合运算符的无括号链式写法（如 `a..b..<c`、`a in b in c`、`a in b is Int`、`a < b <= c`、`a == b != c`），同时保留带括号链式、跨优先级链式（`a in 1..10`、`a < b == true`、`a ?: b to c`）及泛型调用 `f<Int>(a)`（借助 `[$._prefix_operand, $.call_expression]` GLR 消歧）。
4. **双侧验收与基线同步**：重新生成 `editors/tree-sitter/src/*` 并通过 `tree-sitter test`（10/10）与 `test_contract.py`（21/21，含 TS-01/02/04 专项回归）；在 `crates/lang-frontend/tests/parser_tree_sitter_diff.rs` 中将 TS-01/02/04 纳入与 Rust `parse_file` 完全一致的差分断言，并同步更新 [tree-sitter-diff-baseline.md](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)。

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

- [x] `grammar.js`、`src/scanner.c` 与生成产物同步更新并通过语料（10/10）及契约测试（21/21）
- [x] `parser_tree_sitter_diff`（默认 1/1 + 锁定 CLI `--ignored` 2/2）实跑通过，TS-01/TS-02/TS-04 收口闭环（TS-05 同行无分隔符语句拒绝保留为显式登记的开放漂移）
- [x] PR #461（`4d753908`）双宿主 `CI`（run `45385947066`，含 `Editor lexical contracts`）与 `Parser Formal F4b (Tree-sitter Differential)`（run `45385946851`）全绿验收并归档
