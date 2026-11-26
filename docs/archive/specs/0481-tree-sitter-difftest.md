# SPEC-0481：Tree-sitter 与 `parse_file` 完整文件差分首片（F4b）

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F4b Tree-sitter 完整文件差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@8207b212` |
| 分支 | `feature/spec-0481-tree-sitter-difftest` |

本 Spec 承接 [Parser 分阶段形式化验证计划](../../proposals/parser-formal-verification-plan.md) 的 **F4b** 阶段与
[F4b 差分基线](../../proposals/parser-formal-verification/tree-sitter-diff-baseline.md)。不修改生产 Parser、
Lexer、Guide 或 Tree-sitter 文法，不新增 Cargo 依赖。双宿主 Targeted Tests 显式运行默认故障负对照目标
`parser_tree_sitter_diff`；完整 Tree-sitter CLI 差分保持 `--ignored` 显式运行。Tree-sitter 仅作交叉观察信号，不是第二份语言规范。

## 交付合同

1. **统一完整文件入口与归一结构投影**：两侧均以完整 `.ko` 源码分别经过 `lang_frontend::parser::parse_file`
   与锁定版本 `tree-sitter parse --xml` 解析，提取同构 `FileSummary`（`package` 段、`import` 路径/通配符/别名、
   顶层声明种类与名称列表，以及 `nested_blocks`、`control_blocks`、`lambdas`、`calls`、`when_entries`、
   `while_loops`、`for_loops`、`loops`、`prefixes`、`binaries` 计数），比较接受性与完整结构投影。
2. **故障负对照默认运行**：`fault_controls_detect_structural_and_acceptance_drift` 无需外部 `tree-sitter`
   二进制，随普通 `cargo test -p lang-frontend --test parser_tree_sitter_diff` 运行，证明投影与比较器能检出
   `package`、`import`、顶层声明、各类结构计数和接受/拒绝漂移。
3. **共同子集与 SPEC-0479 实跑差分**：`common_subset_and_spec_0479_differential`（`#[ignore]` 显式开启，缺
   CLI 时失败不静默跳过）覆盖文件头、顶层声明、类族与委托、表达式与运算符、控制流与软关键字、SPEC-0479
   （显式 `block { ... }`、同行无括号尾 lambda、闭合 `(...)` 跨行尾 lambda 及注释保留、`when` 复合条件边界、
   `({ ... })`、`move { ... }`），以及 5 组双方必须共同拒绝的负对照（含块内与 lambda 体内裸 `{ ... }`）。
4. **已知漂移显式归因**：`known_drift_witnesses_match_documented_baseline`（`#[ignore]`）对基线登记的
   `TS-01`、`TS-02`、`TS-03`、`TS-04`、`TS-05`、`TS-07` 各运行至少 1 条具体见证并断言精确分歧方向（`TS-06`
   已由 SPEC-0479 在 `editors/tree-sitter` 关闭并纳入第 3 项共同子集）；若未来任一漂移被修复或改变，测试立即显式暴露。

## 本地运行

```bash
# 1. 默认故障负对照（不依赖外部 CLI）
cargo test --locked -p lang-frontend --test parser_tree_sitter_diff

# 2. 完整 Tree-sitter 二进制差分（指向锁定版 tree-sitter CLI）
KOVEN_TREE_SITTER=/absolute/path/to/tree-sitter \
  cargo test --locked -p lang-frontend --test parser_tree_sitter_diff -- --ignored --nocapture
```

## 检查点

- [x] 完整文件投影合同、故障负对照与共同子集/SPEC-0479/漂移见证实现
- [x] 本机 macOS AArch64 锁定版 Tree-sitter CLI 实跑全量差分通过
- [x] PR CI 验收与归档

**归档记录。** 已随 PR #456（`5ea16bb8`）与 PR #457（`f3b9cc5a`） 合入 `main` 并完成有界交付，本批次按维护者确认收口归档至 `docs/archive/specs/`。
