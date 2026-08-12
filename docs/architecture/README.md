# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../agent-language-design-guide-v0.4.md`](../agent-language-design-guide-v0.4.md) 定义。

## 当前状态

仓库处于 Phase 0 之前，目前只有工程规则和设计文档：

- 尚无根 `Cargo.toml`、Cargo workspace 或 Rust target；
- 尚无 lexer、parser、AST、诊断、类型检查、所有权检查或 codegen 实现；
- 尚未确定 Rust edition、MSRV、LLVM / `inkwell` 版本、runtime / ABI 和目标平台矩阵。

因此，下面的结构是**现行 guide 要求的计划边界**，不是已实现架构。

## 已批准但未实现的边界

计划中的 workspace member：

- `lang-frontend`
- `lang-codegen`
- `lang-cli`
- `lang-lsp`
- `lang-std`

计划中的编译流水线：

```text
源码 → Lexer → Parser / 索引式 AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 本机可执行文件
```

其中 `lang-frontend` 不得依赖 LLVM / `inkwell`，LLVM 细节应收敛在 codegen 边界。member
的物理目录、library / binary target、测试 harness、`lang-std` bootstrap 与 runtime 布局
仍未确定。

## 更新要求

Phase 0 落地时，应以实际代码替换本页的“未实现”描述，并至少补充：

1. workspace member 依赖图与各自公开边界；
2. source / `Span`、AST、诊断和阶段产物的数据所有权；
3. fixture harness 的归属与执行路径；
4. 已接受 ADR 与实际目录、依赖的对应关系；
5. 仍未决的 bootstrap、runtime、ABI、LLVM 和平台事项。

此后每次实现改变模块关系或数据流时，同一任务必须更新本目录。不要在这里保存决策历史，
也不要把尚未批准的设想写成路线图事实。
