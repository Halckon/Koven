# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) 定义。

## 当前状态

仓库处于 Phase 0 之前，目前只有工程规则和设计文档：

- 尚无根 `Cargo.toml`、Cargo workspace 或 Rust target；
- 尚无 lexer、parser、AST、诊断、类型检查、所有权检查或 codegen 实现；
- LLVM / `inkwell` 版本、runtime / ABI 和目标平台矩阵仍未确定。

因此，下面的结构是**现行 guide 要求的计划边界**，不是已实现架构。

## 已批准但未实现的边界

已由 [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 批准、但尚未创建的 workspace
采用根 virtual workspace、`crates/` 布局、Rust edition 2024，以及 `1.96.0` toolchain pin
和初始 MSRV。五个 member 及 target 形态为：

- `crates/lang-frontend`：Rust library；
- `crates/lang-codegen`：Rust library；
- `crates/lang-cli`：名为 `kovenc` 的 Rust binary；
- `crates/lang-lsp`：Rust binary；
- `crates/lang-std`：最小 Rust library，并以 `koven/**/*.ko` 保存标准库源码真源。

批准的项目内依赖方向为：codegen 依赖 frontend，CLI 依赖 frontend 与 codegen，LSP 只依赖
frontend，`lang-std` 不依赖编译器 crate。Phase 0 不新增 runtime crate；所有 package 在许可
与发布策略确定前保持不可发布。

计划中的编译流水线：

```text
源码 → Lexer → Parser / 索引式 AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 本机可执行文件
```

其中 `lang-frontend` 不得依赖 LLVM / `inkwell`，LLVM 细节应收敛在 codegen 边界。fixture
harness、`lang-std` 后续 bootstrap 流程与 runtime / ABI 布局仍未确定。

## 更新要求

Phase 0 落地时，应以实际代码替换本页的“未实现”描述，并至少补充：

1. workspace member 依赖图与各自公开边界；
2. source / `Span`、AST、诊断和阶段产物的数据所有权；
3. fixture harness 的归属与执行路径；
4. 已接受 ADR 与实际目录、依赖的对应关系；
5. 仍未决的 bootstrap、runtime、ABI、LLVM 和平台事项。

此后每次实现改变模块关系或数据流时，同一任务必须更新本目录。不要在这里保存决策历史，
也不要把尚未批准的设想写成路线图事实。
