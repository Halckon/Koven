# SPEC-0055: 发布单文档 LSP 诊断

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-0055` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [`guide/00-index.md`](../guide/00-index.md) v0.28；[`guide/06-roadmap.md`](../guide/06-roadmap.md) Phase 6 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0002、0003、0018–0023、0027–0030、0032 `done` |
| 前置 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md) `accepted` |
| 关联 ADR | [ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | 无；多文件 package/import 明确排除，不依赖候选 SPEC-0025 |
| 影响范围 | `lang-lsp`、workspace manifest/lock、Architecture、Roadmap |
| 语言语义变更 | 否 |

## 1. Goal

完成后，编辑器可通过标准 LSP stdio 会话打开或全量更新一份 Koven 文档，并收到由现有
Lexer、Parser、名称、类型与所有权流水线共同产生的版本化诊断；关闭文档会清空诊断。

## 2. 背景

`lang-lsp` 当前只有空 binary。ADR-0003 已决定由 Phase 6 适配层映射 frontend 的同一
`Diagnostic` 模型，但候选队列把全部 LSP 诊断笼统依赖在多文件 SPEC-0025 上。现有 frontend
已经提供完整单文件分析产物，打开的内存 buffer 也不需要 package 到文件系统的解析，因此先
交付单文档诊断能够形成真实工具链闭环，同时不猜测尚未封闭的 import 冲突或跨 package
visibility。

## 3. 范围与需求

- 使用标准 LSP/JSON-RPC stdio transport 完成 `initialize`、`initialized`、`shutdown` 与
  `exit` 生命周期；声明 UTF-16 position encoding 和 full-document text sync。
- 处理 `textDocument/didOpen`、`textDocument/didChange` 与 `textDocument/didClose`。服务端只
  保存客户端已打开文档的 URI、版本和完整文本，不读取磁盘。
- 每次 open/change 都以新 `SourceMap` 运行 `lex → parse_file → resolve_names → check_types →
  check_ownership`；使用显式标准分析环境绑定当前已实现的 builtin、能力、核心容器和
  `error()` identity。
- 合并 Parser（已含 Lexer）、名称、类型和所有权诊断，再通过 frontend 的
  `ordered_diagnostics` 建立确定性全序；不得按阶段拼接顺序或哈希迭代顺序发布。
- 把主 `Span` 映射为零基 UTF-16 LSP `Range`，把 `Severity`、`Ldddd`、message 和 source
  映射到标准字段；label 映射为同文档 related information，note/help 按生产者顺序附加到
  message。内部不变量失败保持服务器错误，不伪造新的 Koven 用户诊断。
- open/change 发布对应文档版本；close 移除内存文本并发布空诊断集合。未知 request 必须返回
  method-not-found，未知 notification 不改变文档状态。
- 使用内存 LSP connection 测试真实消息生命周期，并单独测试 Unicode、CRLF、空范围与
  frontend 各阶段诊断映射。

## 4. 非目标

- 不实现多文件 package/import 展开、workspace 扫描、磁盘监听或跨文件诊断；这些能力仍等待
  SPEC-0025 及其 guide 门禁。
- 不实现跳转定义、completion、hover、semantic token、代码动作、增量 text sync 或并发分析。
- 不定义 Koven 自有的版本化机器诊断 schema，不完成候选 SPEC-0060；LSP 标准消息只是本次
  编辑器适配边界。
- 不新增或改变任何语言错误码、语言语义及 frontend 阶段的接受/拒绝规则。

## 5. 验收标准

- [x] 初始化结果声明 UTF-16 与 full-document open/change/close sync，且不声明未实现能力。
- [x] open/change 对合法源码发布空集合，对 Lexer/Parser、名称、类型和所有权错误发布原有
      `Ldddd`；诊断顺序与 `ordered_diagnostics` 一致，change 携带新版本。
- [x] UTF-8 多字节字符、UTF-16 surrogate pair、LF/CRLF、EOF/空 `Span` 的零基 LSP 范围均有
      精确测试；label、note、help 不丢失或乱序。
- [x] close 发布空集合并移除文档；后续没有 open buffer 时的 change 不产生陈旧诊断。
- [x] shutdown/exit 正常结束；未知 request 返回 method-not-found；畸形或不支持 notification
      不触发 panic，也不污染其他文档状态。
- [x] `lsp-server 0.10.0`、`lsp-types 0.97.0` 与直接 `serde_json` 依赖经 lockfile 固定；只在
      `lang-lsp` 边界使用，不泄漏到 frontend。
- [x] `lang-lsp` Rust 测试和 workspace 标准基线全部通过。
- [x] Architecture 与 Roadmap 只把单文档诊断记为已实现，跨文件诊断和跳转定义继续标为未完成。

## 6. 技术方案与边界

### 6.1 依赖准入

采用成熟的 `lsp-server` transport scaffold 和 `lsp-types` 协议数据模型，不自行实现
`Content-Length` framing、JSON-RPC request lifecycle 或 LSP 数据结构：

| 检查 | `lsp-server 0.10.0` | `lsp-types 0.97.0` |
|---|---|---|
| 适配度 | 提供 stdio/memory connection、初始化与 shutdown helper | 提供 LSP 3.x typed params/capabilities/diagnostics |
| 许可 | MIT OR Apache-2.0 | MIT |
| Rust/feature | edition 2024；workspace Rust 1.96 实际编译验收 | edition 2018；default feature 为空，不启用不稳定 `proposed` |
| 构建时执行 | 无 `build.rs`；自身不是过程宏 | 无 `build.rs`；自身 `forbid(unsafe_code)` |
| 直接依赖面 | crossbeam-channel、log、serde/derive、serde_json | bitflags、fluent-uri、serde/serde_json/serde_repr |
| 隔离 | 只进入 `lang-lsp` | 只进入 `lang-lsp` |

当前需求若最小自研，需要复制 framing、并发 channel、request ID 与 shutdown 状态机，验证和
维护成本高于上述依赖。`serde_json` 作为 direct dependency 只负责 typed params/value 转换，
不把内部 `Diagnostic` 暴露为自定义 JSON schema。最终直接/传递版本由 `Cargo.lock` 固定，
并以 `cargo tree -p lang-lsp` 审阅实际依赖图。

### 6.2 数据流

`lang-lsp` 保持 transport adapter：每个打开文档独立重建 frontend 分析 owner，避免跨版本
`SourceId` 或 analysis identity 混用。LSP range 转换先调用 `SourceMap::position` 取得统一行与
scalar column，再只在 adapter 边界把该列换算为 UTF-16 code unit；不复制 Lexer/Parser 的行
索引或诊断排序逻辑。

## 7. 实施计划

1. [x] 集中声明依赖并建立 server/analysis/diagnostic adapter 模块 → 验证：`cargo check -p lang-lsp`
2. [x] 实现 lifecycle 与 open/change/close 全量同步 → 验证：memory connection 生命周期测试
3. [x] 接入完整单文件 frontend 流水线与 UTF-16 映射 → 验证：各阶段、Unicode/CRLF 精确测试
4. [x] 同步 Spec 验收记录、Architecture 与 Roadmap → 验证：文档和实现一致

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 批准范围、依赖审计与候选队列修正 | `docs(spec): define single-document LSP diagnostics (SPEC-0055)` |
| 2 | transport、诊断适配、测试、Architecture/Roadmap 与最终验收 | `feat(lsp): publish single-document diagnostics (SPEC-0055)` |

## 9. 未决问题

- 无。跨文件分析与跳转定义是明确非目标，不构成本 Spec 阻塞项。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo info lsp-server@0.10.0` | 通过 | 版本 0.10.0；MIT OR Apache-2.0；无声明 MSRV |
| `cargo info lsp-types` | 通过 | 当前版本 0.97.0；MIT；default feature 为空；无声明 MSRV |
| registry manifest/source 审计 | 通过 | 两者无 `build.rs`；生产源码未发现 `unsafe` 块/函数/impl；Serde derive/serde_repr 是实际依赖图中的过程宏 |
| `cargo tree -p lang-lsp` | 通过 | 直接依赖仅 frontend、lsp-server、lsp-types、serde_json；实际版本由 lockfile 固定 |
| `cargo test -p lang-lsp --all-targets` | 通过 | 5 项：分析、UTF-16/detail 映射与两组 memory connection 会话测试 |
| `cargo fmt --all -- --check` | 通过 | workspace 格式基线 |
| `cargo check --workspace --all-targets` | 通过 | workspace 构建检查 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 零 warning |
| `cargo test --workspace --all-targets` | 通过 | 完整 frontend、codegen、CLI、LSP、stdlib 测试矩阵 |
| `cargo build -p lang-cli` | 通过 | CLI 构建基线 |
