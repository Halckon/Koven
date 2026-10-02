# Koven 架构

> **性质**：当前实现事实入口 · **状态**：current · **读取时机**：定位编译阶段、数据流或实现页面时 · **唯一真源**：代码、测试与本索引导航的页面

Architecture 只描述当前仓库已经落地的结构，不保存决策历史或未来计划。长期理由见
[ADR](../adr/README.md)，未启用方向见 [Proposals](../proposals/README.md)。

## 当前流水线

```text
.ko 源码 → Source/Span → Lexer → Parser/AST → 名称/类型事实 → 所有权事实
         → typed SSA → LLVM IR → object → linker → 本机可执行文件
```

五个 workspace member 保持单向依赖：frontend 提供语言事实，codegen 消费事实并封装 LLVM，
CLI 编排构建/链接/运行，LSP 复用 frontend，lang-std 提供 Koven 标准库源码。

## 当前成熟度

| 领域 | 当前事实 |
|---|---|
| Source、Lexer、Parser | 确定性词法、完整文件组合、表达式/声明/block/lambda/class-family 与恢复路径已接入 |
| 名称与类型 | 单文件及 compilation-unit package/import、名义/泛型/interface、call/member/container 等事实已接入 |
| 所有权 | move/loan/drop/capture、结构移动、容器 element place、跨文件所有权与 validated gate 已接入 |
| SSA / LLVM | owner-aware typed SSA、标量/聚合/容器/closure/String/Rc/nullable/receiver 与多文件 native 路径已接入；宿主支持 AArch64 macOS 和 x86_64 Linux + glibc |
| 标准库 | Koven prelude、`error`、`println(String)`、`Pair`/`Result`、String 与 Rc 核心路径已接入 |
| 工具 | 单文件/本地 project build/run、JSON Lines 诊断、LSP、formatter、TextMate 与 Tree-sitter 已接入 |

## 规范覆盖边界

本地整合已包含 SPEC-0229–0234、0236 的代码或文档切片；各自的实际验证与遗留缺口见
[演进实施账本](../specs/evolution-status.md)。[String.clone](string-clone.md) 已有单/多文件
类型、所有权、SSA 与 Linux native 定向证据。builtin 文本类型仍是 MoveOnly String。
Guide v0.40 新启用的调用处无 Borrow marker、位宽移位屏蔽及只读 deinit 顺序合同尚未全部
闭环：Parser 仍可构造旧实参 Borrow marker；不得由文档启用推断实现或远端 CI 已完成。

## 按实现领域读取

| 修改内容 | 页面 |
|---|---|
| workspace、crate 边界、编译流水线 | [流水线与 Workspace](pipeline-and-workspace.md) |
| Source、Span、Lexer、Parser、AST | [Source 与语法前端](source-and-syntax.md) |
| 名称解析、类型签名和 typed facts | [名称与类型](names-and-types.md) |
| loan、move、capture、drop facts | [所有权](ownership.md) |
- [Receiver 两阶段借用](receiver-borrows.md)：预留、激活、this 身份与直接 SSA/native 消费
| unit planning、typed SSA、LLVM、runtime | [SSA、Codegen 与 Runtime](ssa-codegen-runtime.md) |
| 整数具名位运算与 inv 的 const / typed / native 链路 | [整数位运算](integer-operations.md) |
| String clone 的 intrinsic / loan / 独立 owner 全链路 | [String clone](string-clone.md) |
| Guide 示例当前覆盖与 PR #6 审计更正 | [Guide 验证与更正](guide-conformance.md) |
| Diagnostic、fixture、矩阵与压力测试 | [诊断与测试](diagnostics-and-tests.md) |
| CLI、project、LSP、formatter、编辑器 grammar | [工具链](tooling.md) |
