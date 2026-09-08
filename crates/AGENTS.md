# AGENTS.md — Cargo workspace 共享开发规则

根 [AGENTS.md](../AGENTS.md) 继续适用。本文件只增加所有 workspace member 共用的 Rust、依赖和
验证规则；具体领域由子 crate 的 `AGENTS.md` 路由。

## Crate 边界

| Crate | 职责 | 不得承担 |
|---|---|---|
| `lang-frontend` | Source/Span、Lexer、AST/Parser、名称/类型/所有权、格式化、诊断 | LLVM、链接或 CLI 编排 |
| `lang-codegen` | typed SSA、verifier、LLVM、目标文件与 native runtime 边界 | 重新推导语言语义 |
| `lang-cli` | 流水线编排、项目发现、渲染、链接与进程退出 | frontend/codegen 核心算法 |
| `lang-lsp` | 复用 frontend 的诊断、位置适配、source set 与 definition | 复制 Parser/检查器 |
| `lang-std` | Koven 标准库源码与 Cargo 边界 | 用 Rust 重写标准库公共实现 |

依赖保持单向、无环；跨 crate API 才使用 `pub`，其余保持最小可见性。

## 修改规则

- 修改前读取目标模块门面、直接调用方、共享类型、最近单元/集成测试和对应 architecture 页面。
- 匹配既有模块与命名风格；不顺手格式化或重构无关代码。
- 生产 Rust 文件以 1000 物理行为软上限；新增独立职责时优先按领域提取，不机械切片。
- 用户输入路径返回具体错误或结构化 Diagnostic；内部不变量才可使用有依据的 `expect`。
- 库不直接写 stdout/stderr；渲染留给可执行边界。
- 新依赖必须先按 [依赖治理](../docs/development/dependencies.md)审查，并在根 manifest 集中版本。

## 验证

先运行直接行为和最近共享契约套件，再按 [分层验收](../docs/development/testing.md)决定是否升级到
crate、下游或 workspace 全量。禁止并发运行争用同一 Cargo target 的本地门禁；不运行
`cargo clean` 作为普通步骤。
