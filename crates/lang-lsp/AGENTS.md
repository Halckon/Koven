# AGENTS.md — lang-lsp

本 crate 只做协议、文档/session、位置适配和 frontend 产物转换，不复制语言分析。

## 必读入口

- [名称与文件规则](../../docs/guide/02-names-files-packages.md)
- [工具架构](../../docs/architecture/tooling.md)
- [名称/类型实现事实](../../docs/architecture/names-and-types.md)
- [诊断规范](../../docs/development/diagnostics.md)

## 边界与验证

- UTF-16 LSP position 与 byte-based `Span` 的转换集中处理并覆盖 Unicode/越界。
- source-set/session 必须确定、版本感知；过期分析结果不得覆盖较新文档。
- 诊断和 definition 复用 frontend identity、provenance 与排序，不建立第二套 Parser/Resolver。
- 先运行修改模块的单元测试；跨文件 source-set 或 definition 变化再运行整个 `lang-lsp` 测试和
  `cargo check --workspace --all-targets`。
