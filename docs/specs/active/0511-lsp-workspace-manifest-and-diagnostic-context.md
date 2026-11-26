# SPEC-0511: `lang-lsp` 显式工作区 manifest 加载与诊断源码片段上下文

> **性质**：Phase 6 工具链增量合同 · **状态**：in-progress · **读取时机**：维护 `lang-lsp` 通用 LSP 客户端多文件支持与诊断可读性时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P6-0511` |
| 所属 Phase | Phase 6 LSP；不改变语言语义、Phase 2–5 或 D10/E2/E3 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 前置合同 | [ADR-0021](../../adr/accepted/0021-lsp-explicit-source-set-protocol.md)、[ADR-0022](../../adr/accepted/0022-minimal-project-manifest-source-discovery.md) |
| 影响范围 | `crates/lang-lsp/`（不新增 Cargo 依赖、不改动 `lang-frontend`/`lang-codegen`） |
| 语言语义变更 | 否 |

## 1. 目标与边界

默认无参数启动的 `lang-lsp` 严格保留 ADR-0021 合同（不读磁盘、仅在 `initializationOptions.koven.sourceSet` 存在时进入多文件模式，否则单文档模式）。
为支持不注入自定义 `koven.sourceSet` 的标准 LSP 客户端（如 OpenIDEA），新增两个显式 opt-in CLI 参数：

1. `--workspace-manifest`：当 `koven.sourceSet` 缺席时，从 LSP `workspaceFolders` / `rootUri` 检查根目录 `project.toml`（`schema = "koven.project"`、`version = 1`、`[project] source-roots`），确定性收集 `.ko` 源码并经同一 `SourceSetConfig` 进入 `UnitSession`；对不在 `source-roots` 内的独立 `.ko` 文件（如 `examples/*.ko`）保留单文档回退分析、定义跳转与格式化。目录无 `project.toml` 时平滑回退单文档模式。
2. `--diagnostic-context`：在 `diagnostic_adapter` 输出 LSP 诊断消息时，若 `primary_span` 对应短单行源码片段且原消息尚未包含该片段，则附加 `` `snippet` ``（例如 ``unresolved name `missingSymbol` ``）。默认关闭，确保 `single_file_oracle` 与既有测试零变动。

## 2. 非目标

- 不改变无参数 `lang-lsp` 的任何行为或 ADR-0021 version 1 wire；
- 不引入文件系统监听（磁盘成员增删仍通过重启会话刷新）；
- 不修改 `lang-frontend` 诊断文案、错误码或 `lang-codegen` 任何代码。

## 3. 验收证据（2026-11-26）

分支 `feature/lsp-workspace-manifest`（基于 `main` `48268b43`，commit `5d49fc45`）：

- `cargo test -p lang-lsp`：60 passed / 0 failed（含 `workspace_manifest` 3 项、`diagnostic_adapter::enrich_context` 1 项与既有 `single_file_oracle`），`formatting_stdio` 1 passed / 0 failed；
- `cargo clippy -p lang-lsp --all-targets -- -D warnings`：0 告警通过；
- 真实多文件工程 `koven-demo`（`project.toml` + `src/demo/main.ko` + `src/demo/math.ko` + `examples/hello.ko`）stdio 实测与 OpenIDEA `0.1.53` 宿主实测：多文件跨文件符号零红线、单文件回退零红线、未解析符号提示 ``unresolved name `missingSymbol` ``。

待办：推送 PR 触发双宿主（macOS + Linux）正式 CI 验收后归档。
