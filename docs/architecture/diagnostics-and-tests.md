# 诊断与测试基础设施

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Diagnostic、renderer、fixture 或测试矩阵时 · **唯一真源**：代码、测试和已提交 fixture

## 结构化诊断

`lang_frontend::diagnostic` 拥有语言诊断模型。`DiagnosticCodeCatalog` 校验 `Ldddd` 编号和重复项；
`Diagnostic` 必须包含 severity、已注册 code、单行 message 和主 `Span`，并按生产者顺序保存 label、
note 与 help。

`ordered_diagnostics` 先用共同 SourceMap 验证全部 Span，再按 source 名称、范围、severity、code、
message 和 details 建立稳定全序。顺序不依赖 `SourceId`、source 加载顺序或哈希容器迭代。

CLI 提供两个 adapter：

- `diagnostic_renderer` 生成确定性 human 文本；
- `machine_diagnostic_renderer` 生成 versioned JSON Lines，保留 UTF-8 byte range、1-based scalar
  position 和有序 details。

LSP 使用独立 adapter 将 scalar column 转为 UTF-16 range；它不复用 CLI 序列化格式。内部错误、IO
错误和 CLI usage error 不是 `Ldddd` 语言诊断。

对应覆盖位于 frontend 的 `diagnostic_model` integration suite 和 CLI 的
`machine_diagnostic_renderer` 模块测试；命令选择见[开发测试指南](../development/testing.md)。

## Fixture harness

`crates/lang-frontend/tests/fixtures.rs` 是 Cargo 自动发现的 fixture runner。fixture 位于
`crates/lang-frontend/tests/fixtures/`，按 source、lexer 和 parser 领域分 suite。

- 发现器只接受普通 `.ko` 文件，拒绝 symlink、未知扩展、非 UTF-8 path 和空 suite。
- fail fixture 使用同 stem `.diag` sidecar，逐项锁定 code 与 UTF-8 半开字节范围。
- helper 对同一输入重复运行生产 Lexer/Parser，比较完整公开产物并校验 lexeme 覆盖、唯一 EOF、
  source identity、AST/diagnostic Span 和稳定顺序。
- sidecar 是仓库测试格式，不是公共诊断协议。

Cargo 将该 runner 注册为 `fixtures` integration suite。

## Guide 示例门禁

`guide_litmus` 直接从现行 Guide 提取源码，分别运行单文件/unit 前端入口，精确锁定通过、
应拒绝及已知实现缺口；调用脚本为 `scripts/check_guide_litmus.sh`。覆盖和限制见
[当前更正账本](guide-conformance.md)，不把诊断门禁视为 native 验收。

## 测试分层

| 层级 | 位置 | 目的 |
|---|---|---|
| 模块单元测试 | `crates/*/src/**` | 私有状态机、验证器和错误边界 |
| 领域 integration test | `crates/*/tests/*.rs` | 公共入口、AST/fact/diagnostic 契约 |
| 变异与压力矩阵 | `parser_*_matrix.rs`、`lexer_*_matrix.rs` | 恢复、复杂度、确定性和栈边界 |
| native/CLI test | codegen/CLI tests | object、link、run、stdout、原子失败 |
| 编辑器 grammar contract | `editors/*/tests` + frontend tests | TextMate/Tree-sitter 与生产词法/语法边界 |

共享表、诊断编号、public fact 与跨 crate 边界由对应领域 suite 共同覆盖；如何按影响面选择和扩大
验证见[开发测试指南](../development/testing.md)。

## 确定性与失败边界

- 正常用户错误的回归测试重复断言相同 code、Span、detail 和顺序。
- 测试专用 constructor 可以构造不可能的内部产物，用来验证 fail-loud 边界；生产 API 不公开这些
  constructor。

## 本机平台验收

native 夹具按宿主检查 Mach-O AArch64 或 ELF64 x86_64 object 头及平台入口符号。
Linux LLVM IR 插桩测试优先从 `LLVM_SYS_211_PREFIX/bin/clang` 取得 Clang，缺失时回退到
PATH；生产 CLI 链接独立使用 `/usr/bin/cc`。macOS 测试继续使用 `/usr/bin/clang`。

macOS 的 DWARF 调试器测试保留 LLDB 断点/运行断言。Linux 的 DWARF 测试使用匹配的
`llvm-dwarfdump --debug-line` 检查生成 ELF object 中的 `.ko` 文件、行和列，再经系统 C driver
链接并实际运行；该测试不声称 Linux 调试器断点或变量查看已通过。工具前提与未运行平台的
报告规则见[测试指南](../development/testing.md#本机目标与工具前提)。
