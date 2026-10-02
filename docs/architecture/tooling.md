# CLI、LSP、Formatter 与编辑器 Grammar

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 CLI、project、LSP、formatter 或编辑器 grammar 时 · **唯一真源**：对应代码、协议测试和 grammar fixture

## `kovenc` CLI

`lang-cli` 负责 frontend/codegen 编排、诊断输出、链接和进程退出码，不承载语言分析算法。

当前公开路径包括：

- 单文件 `build` / `run`，接收显式 source、entry 和输出选择；
- `format` / `format --check`，结果写 stdout，不原地改文件；
- `--message-format=json` 的版本化 JSON Lines 语言诊断；
- 显式 `project.toml` 的本地 project build/run。

project loader 严格解析 version 1 manifest，验证 source root、logical path、symlink/overlap 和 physical
file identity，再发布不可变、稳定排序的 source-set snapshot。build 注册原 SourceMap 后将其移动进
frontend `analyze_unit_names`，取得拥有源码/语法/环境/名称事实的只读 `UnitNameSnapshot`；
首个非空诊断 gate 仍在完整 names 前缀后。type/ownership validation 继续留宿主，成功后进入
单 object、link 和原子 executable 发布。CLI 不从 cwd 或
祖先目录猜 manifest。typed diagnostics 统一先于 entry 选择；基础 capability 验证成功时沿用
基础 ownership/native，含常量时由 frontend 专用 gate 发布 typed/owned capability，再调用
`emit_native_constant_unit_object`。同一只读 entry shape helper 服务两条已验证路径。

CLI 生产链接按宿主使用 macOS `/usr/bin/clang` 或 Linux `/usr/bin/cc`，通过 `Command`
参数数组链接已有 native object，不调用 shell 或让外部 driver 重新编译 LLVM IR。
链接启动失败与进程失败保留现有结构化错误；受支持目标在 codegen 边界先行校验。

实现入口是 `crates/lang-cli/src/main.rs`、`native_command.rs`、`project/`、`project_build.rs` 和
`project_command.rs`；对应覆盖位于 `native_cli`、`project_cli` 与 `format_cli` integration suites。

## LSP

`lang-lsp` 是标准 stdio server，声明 UTF-16 position encoding、full-document sync、诊断和
`textDocument/definition`。

它有两种明确模式：

- legacy 单文档：每个打开 URI 独立运行 lex → parse → names → types → ownership；
- `koven.sourceSet` version 1：初始化 payload 提供 immutable base sources，open/change 形成 overlay，
  每次候选更新重建共同 compilation-unit snapshot。

source-set 模式不读取磁盘。成功 snapshot 原子替换诊断和 definition facts；内部分析失败保留 last-good
snapshot。诊断按 target URI 分组并稳定发布，definition 直接消费已保存的 name/type target，不重新解析
package/import 或 overload。

`position_adapter` 集中处理 Span 与 UTF-16 的双向映射，拒绝 surrogate pair 中间位置和越界 cursor。
实现入口位于 `crates/lang-lsp/src/analysis.rs`、`source_set.rs`、`unit_session.rs`、`definition.rs` 和
`diagnostic_adapter.rs`；对应覆盖位于 `lang-lsp` 的模块与 integration tests。

## Formatter

`lang_frontend::formatting::format_source` 先运行生产 Lexer 和 file Parser；任一语言诊断都会阻止部分
格式化结果。成功路径复用 lexeme slice，规范 horizontal whitespace 和四空格缩进，同时保持 comment、
string segment 与 LF/CRLF 字节。它不排序声明、不折行、不合并空行。

实现入口是 `crates/lang-frontend/src/formatting.rs`；对应覆盖位于 frontend 的 `formatting` 和 CLI
的 `format_cli` integration suites。

## TextMate

`editors/textmate/syntaxes/koven.tmLanguage.json` 提供 `.ko` 的词法高亮近似；
`editors/textmate/tests/lexical-contract.tsv`、`highlight.ko`、`reserved.ko` 和 `scopes.tsv` 锁定代表性
scope。Node verifier 执行 grammar regex，frontend integration test 用同一数据对照生产 Lexer。
对应覆盖位于 TextMate 的 Node tests 与 frontend 的 `textmate_grammar` integration suite。

## Tree-sitter

`editors/tree-sitter/grammar.js` 是手写 grammar 入口；`src/grammar.json`、`node-types.json` 和
`parser.c` 是锁定 CLI 生成并提交的产物。`src/scanner.c` 集中拒绝硬关键字、未来保留字和解构 `_`。
Tree-sitter 用于编辑器 concrete syntax；生产编译仍只使用 Rust Lexer/Parser。
对应覆盖位于 Tree-sitter 的 Node/corpus tests 与 frontend 的 `tree_sitter_grammar` integration suite。

上述测试的命令与范围选择统一见[开发测试指南](../development/testing.md)。

## CI 工程门禁

当前 workflow 定义 macOS 14 / Ubuntu 24.04 双宿主 check、严格 clippy、核心测试与两个
frontend 定向脚本，单次 fmt；LLVM setup action 统一校验所需工具，CI 汇总策略拒绝必需 job
意外跳过。配置与本地验证不代表远端已运行；实际交付证据见
[SPEC-0239](../archive/specs/0239-linux-ci-gates.md)，使用规则见[测试与分层验收](../development/testing.md)。
