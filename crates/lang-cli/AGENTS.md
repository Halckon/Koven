# AGENTS.md — lang-cli

本 crate 负责编译流水线、项目发现、诊断渲染、链接与进程退出，不承载语言阶段核心算法。

## 按任务读取

| 修改内容 | 必读文档 |
|---|---|
| command / renderer | [程序与 project 语义](../../docs/guide/13-program-runtime-standard-library.md)、[工具架构](../../docs/architecture/tooling.md)、[诊断规范](../../docs/development/diagnostics.md) |
| project source set | [名称与文件](../../docs/guide/02-names-files-packages.md)、[工具架构](../../docs/architecture/tooling.md)、[ADR-0022](../../docs/adr/accepted/0022-minimal-project-manifest-source-discovery.md) |
| object / linker | [工具架构](../../docs/architecture/tooling.md)、[ADR-0010](../../docs/adr/accepted/0010-first-native-object-and-linker-contract.md) |
| argv bridge | [程序入口](../../docs/guide/13-program-runtime-standard-library.md)、[ADR-0019](../../docs/adr/accepted/0019-parameterized-process-entry-bridge.md) |

## 边界与验证

- 文件系统、manifest、source set 与进程错误在 CLI 边界转成稳定用户结果；不污染 frontend API。
- 人类诊断、JSON Lines 与 operational error 保持各自协议和 stdout/stderr 边界。
- 构建发布必须失败原子；`run` 的临时产物和 argv 转交遵循现有 project/native 合同。
- 最近测试为 `format_cli`、`project_cli`、`native_cli` 及模块内单元测试；修改项目或 native 命令时
  至少运行对应集成套件和 `cargo build -p lang-cli`。
