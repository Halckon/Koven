# Koven Spec 路线图

本目录依据现行 [v0.8 语言规范](../agent-language-design-guide-v0.8.md) 维护可独立验证、可独立
提交的 Goal；已完成 Spec 保留其实施时适用的 guide 引用。路线图负责排序，Spec 文件负责
定义一次交付；路线图条目本身不等于已批准的 Spec，也不授权实现。

## Goal 与提交工作流

Spec 进入 `in-progress` 前必须已有单份明确确认或有效站立授权作为批准依据，并满足所有前置
Spec `done`、前置 ADR `accepted` 和阻塞项解除。存在有效站立授权时，可以在同一工作流中
按逻辑顺序完成 `approved → in-progress`，不要求中间状态形成独立提交。

后续使用 Goal 时固定按以下顺序推进：

1. 以 `完成 SPEC-NNNN：〈Goal〉；按本 Spec 验收并创建独立提交` 作为 Goal objective。
2. 把 Spec 标为 `in-progress`，按“实施计划”逐项执行；每一步先跑窄检查。
3. 完成测试后运行 Spec 要求的 workspace 基线，并同步 Architecture 当前事实。
4. 填写验证记录、逐项勾选验收，把 Spec 标为 `done`。
5. 只暂存该 Spec 范围，检查 staged diff 后提交；提交信息必须包含 `(SPEC-NNNN)`。
6. 提交成功后再把 Goal 标记为完成。未提交、验收缺失或检查未运行时不得完成 Goal。

一个提交不得混合两个 Spec。一个 Spec 可以有多个提交；含 Rust / 目标语言实现的提交必须
可构建、可测试，workspace 建立前的纯文档提交执行适用的链接、术语和 diff 检查。所有提交
都引用同一 Spec 编号；最终实现提交同时包含验收记录、Architecture 更新和 `done` 状态。
若一个 Spec 无法形成清晰的独立提交边界，应在批准前继续拆分。

Spec 草案、批准和 `in-progress` 状态不要求分别提交；最终实现提交仍必须独占一个 Spec，
并包含 `done` 状态、实际验收记录和 Architecture 更新。新 ADR 的决策正文仍应形成独立文档
提交，但可在首次提交时直接为 `accepted`，不得把 ADR 与依赖它的实现混入同一提交。
简化的是人工确认和重复状态文书，不是行为验收；任何检查只有实际成功后才能记录为通过。

## Phase 0 Spec 队列

Phase 0 已细化为以下实际 Spec。状态以各 Spec 文件为准，并按前置关系依次推进：

| 顺序 | Spec | Goal | 前置条件 |
|---|---|---|---|
| 1 | [SPEC-0001](./0001-bootstrap-cargo-workspace.md) | 建立可检查的五 member Cargo workspace | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) accepted |
| 2 | [SPEC-0002](./0002-source-span-foundation.md) | 建立统一 source / `Span` 基础设施 | SPEC-0001 done；[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 3 | [SPEC-0003](./0003-structured-diagnostics.md) | 建立稳定、确定性的结构化诊断核心 | SPEC-0002 done；[ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 4 | [SPEC-0004](./0004-indexed-ast-foundation.md) | 建立保留 `Span` 的索引式 AST 基础 | SPEC-0001、0002 done |
| 5 | [SPEC-0005](./0005-language-fixture-harness.md) | 建立真实枚举 `.ko` 文件且零用例失败的 harness | SPEC-0001、0002、0003、0004 done |

Phase 0 的建议依赖关系：

```text
SPEC-0001
    └── SPEC-0002
            ├── SPEC-0003 ──┐
            └── SPEC-0004 ──┴── SPEC-0005
```

## 后续 Spec 候选

下表预留编号、Phase、单一 Goal 和依赖门槛。没有文件链接的候选项尚不是 Spec；只在前置
Phase 接近完成、适用 guide 已明确且必要 ADR 已接受时，才从模板创建文件并补齐可执行
计划。已有链接但尚未批准或仍带阻塞项的文件继续保持 `draft`，不得据此实施。这能避免
长期空壳 Spec 与实现事实漂移。

### Phase 1：Lexer / Parser

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0006](./0006-deterministic-lexer.md) | Lexer 覆盖字面量、标识符、关键字和 trivia（`done`） | 0002、0003、0005 `done`；v0.5 已生效 |
| [0007](./0007-pratt-expression-parser.md) | Pratt parser 覆盖完整表达式优先级（`done`） | 0004、0006 `done`；v0.6 已生效；站立授权已记录 |
| [0008](./0008-declaration-parser.md) | 解析 `val` / `var` / `const val`、函数、泛型与调用点类型实参（`done`） | 0007 `done`；v0.7 已生效；站立授权已记录 |
| [0009](./0009-block-statement-parser.md) | 解析 block / statement 序列与函数 block body（`done`） | 0008 `done`；v0.8 已生效；站立授权已记录 |
| 0010 | 解析 lambda、命名 / 模式实参与解构 | 0007、0008、0009 |
| 0011 | 组合 0007–0010 已有节点为完整文件并实现声明分隔、跨声明恢复与级联抑制 | 0009、0010；不是 Phase 1 全部语法终点 |
| 0012 | 解析 `module` / `import` | 0008；先由新 guide 定义语法 |
| 待编号 | 解析 `if` / `when` / `super` 与 loop-family 控制流 | 新 guide 明确定义；0009；不得占用既有 0012 或 Phase 2 编号 |
| 待编号 | 解析 `value class` / `class` / `interface` / `enum class` / `object` / `companion object` | 新 guide 明确定义；0009；不得占用既有 0012 或 Phase 2 编号 |

### Phase 2：名称与类型检查

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0013 | 完成单文件声明收集、作用域和名称诊断 | 0011 |
| 0014 | 检查基础类型、局部推导、显式返回类型与 `Nothing` | 0013 |
| 0015 | 检查泛型及 class / interface / enum / value class 名义类型 | 0014；待编号 class-family Parser Spec |
| 0016 | 实现 `when` 穷尽性与 smart cast | 0015；待编号控制流 Parser Spec |
| 0017 | 推导条件 `Copyable` 并检查结构化解构类型 | 0014、0015 |
| 0018 | 检查顺序容器的名义类型、元素可存储性、核心构造和索引 place 类型 | 0015、0017；v0.6 生效 |
| 0019 | 检查 `Map` / `MutableMap` 的 key 契约、value 所有权约束和查询结果类型 | 0015；新 guide 明确 key 等价关系、返回所有权与修改 API |
| 0020 | 建立多文件 module / import 名称解析 | 0012、0013；新 guide + module 映射 ADR |
| 0021 | 检查 `object` / `companion object` | 0015；待编号 class-family Parser Spec；先由新 guide 明确成员函数限制 |

### Phase 3：所有权与借用

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0022 | 建立变量所有权状态并检测 use-after-move | 0014、0015 |
| 0023 | 实现条件复制、移动与消费式解构检查 | 0017、0022 |
| 0024 | 检查 `borrow` / `inout` / `own` 冲突并确定 ASAP 析构点 | 0023；新 guide 明确借用与析构规则 |
| 0025 | 检查顺序容器元素 place 的读取、借用、替换与析构所有权规则 | 0018、0024；v0.6 生效 |
| 0026 | 检查 `Map` / `MutableMap` 查询和修改的 key / value 所有权规则 | 0019、0024；新 guide 明确完整 Map 契约 |
| 0027 | 检查 move closure 与 `Shareable` / `Transferable` | 0015、0024；新 guide 明确标记能力推导 |

### Phase 4：SSA、LLVM 与原生 AOT

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0028 | 实现最小 typed SSA IR 与 verifier | 0016、0024；接受 SSA ADR |
| 0029 | 把标量表达式和控制流 lower 到 LLVM | 0028；接受 LLVM / target ADR |
| 0030 | 生成聚合、class 分配和显式 drop / free | 0029、0024；接受 runtime ABI ADR |
| 0031 | 生成顺序容器的单一连续缓冲区基元、边界检查和 drop 路径 | 0018、0025、0030；接受 runtime ABI ADR |
| 0032 | 生成 `Map` / `MutableMap` 查询与修改的 runtime 基元 | 0019、0026、0030；接受 runtime ABI ADR、Map 存储策略 ADR |
| 0033 | 生成捕获闭包环境和无捕获函数指针 | 0029、0027 |
| 0034 | 生成 object、链接 `main` 并把 `error()` 映射到 abort | 0030、0033；接受 linker 决策 |
| 0035 | 生成 DWARF 并用首个支持平台的调试器验收 | 0034；接受 debug mapping ADR |
| 0036 | 提供用户可见 `extern` FFI | 0034；新 guide 定义 FFI 与所有权边界，非 v1 主路径 |

### Phase 5：最小标准库

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0037 | 用编译器构建并运行 `lang-std` 目标语言源码 | 0034；接受 bootstrap / runtime ADR |
| 0038 | 实现 prelude、基础操作和 `error()` | 0037 |
| 0039 | 实现条件可复制的 `Pair` 与 `Result` | 0037、0023、0030 |
| 0040 | 实现独占 `Box` 与共享 `Rc` 所有权类型 | 0037、0023、0030 |
| 0041 | 提供 Array / List / MutableList 的目标语言公共 API 与顺序算法 | 0031、0038、0040 |
| 0042 | 提供 Map / MutableMap 的目标语言公共 API 与键值算法 | 0032、0038、0040；新 guide 明确完整 Map 契约 |
| 0043 | 为顺序容器实现 `map` / `filter` / `reduce` / `forEach` | 0041、0033 |
| 0044 | 实现同步 File / BufferedReader / 标准流 | 0038、0034 |
| 0045 | 实现 thread / channel | 0037、0027 |
| 0046 | 实现目标语言测试发现与断言 runner | 0037；新 guide 定义最小 `@Test` 语法 |

### Phase 6：工具链

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0047 | 定义并解析最小 `project.toml` | 0020；接受 package schema ADR |
| 0048 | 实现依赖解析与确定性 `project.lock` 核心 | 0047；接受解析 / 锁定策略 ADR |
| 0049 | 由 package CLI 编排 manifest、解析与锁定 | 0048 |
| 0050 | 让 LSP 发布 frontend 诊断 | 0020、0016、0003 |
| 0051 | 让 LSP 支持跳转定义 | 0050、0020、0016 |
| 0052 | 实现稳定、幂等的格式化器 | 0011、0006 |
| 0053 | 提供 TextMate grammar 与回归 fixture | 0011、0012 |
| 0054 | 提供 Tree-sitter grammar 与 corpus | 0011、0012 |
| 0055 | 提供版本化机器可读诊断协议 | 0003、0050；接受协议 ADR |
| 0056 | 构建首个支持平台的 compiler + stdlib 发行包 | 0035、0037–0046、0049；接受发布矩阵 ADR |

增量编译不预留在 Phase 0–6 主链中。它依赖稳定 module identity、package lock、SSA 和依赖
图；推荐在 SPEC-0049 完成后另建 Phase 6+ Spec，并先接受缓存键与失效策略 ADR。

## 未决决策的推荐方向

以下是起草后续 guide / ADR 时的默认推荐，不是已经接受的决策；触及对应 Spec 前仍需正式
文档批准。

| 决策 | 推荐方案 | 需要的权威文档 |
|---|---|---|
| `object` / `companion object` | 编译期限制只约束存储状态和初始化式；成员函数体可使用普通 v1 代码，但不能读取或修改运行时单例状态 | 新 guide |
| `module` / `import` | 使用显式、点分层级 module 名和绝对 import；source root 到文件的映射由 package ADR 决定，不从相对路径静默推导语义 | 新 guide + ADR |
| `Shareable` / `Transferable` | v1 与 `Copyable` 一样采用编译器已知的结构化自动推导，不开放手动实现；标准库并发类型的例外逐项写入 Spec | 新 guide |
| 借用与析构 | v1 借用只存在于一次调用的动态期间，不允许存储或返回；ASAP 析构以所有权检查标出的最后一次合法使用为准，分支合流采用保守点 | 新 guide |
| `lang-std` bootstrap / runtime | `.ko` 标准库保持独立真源；最小 ABI 支撑先收敛在 codegen 的私有 runtime 边界，证明需要独立发布后再提新增 crate 的 ADR | ADR |
| SSA | 采用 typed SSA + block parameters，显式表达 move / drop；用 verifier 锁定类型、CFG 与所有权不变量 | ADR |
| LLVM / target / linker | 固定一组经兼容矩阵验证的 LLVM major 与 `inkwell` feature；先支持单一 host target，再扩展 CI 矩阵 | ADR |
| FFI | 不放入 v1 主交付路径；待内部 ABI 稳定后，以受限 C ABI 和显式 `unsafe` / 所有权边界起步 | 新 guide + ADR |
| `@Test` | v1 只定义编译器保留的最小 `@Test`，不顺带实现通用运行时注解或反射 | 新 guide |
| 机器诊断 | 人类可读诊断走 stderr；机器模式使用带 schema version 的 JSON Lines，稳定字段由后续协议 ADR 固定 | 后续 ADR |
| package / lock | `project.toml` 只保留 package、target、dependency 最小字段；`project.lock` 完全由工具生成并确定性排序 | ADR |
| 首发平台 | 先验收开发主机 `aarch64-apple-darwin`，再增加一个 Linux CI target；跨平台承诺以发行 ADR 为准 | ADR |

## 路线图维护规则

- guide 改变 Phase 或语言语义时，先更新 guide，再调整尚未批准的路线图候选。
- 已批准或已完成 Spec 不因路线图重排而改号；需要替代时使用 `superseded` 并建立双向链接。
- ADR 只决定 guide 留白处的长期方案；路线图中的“推荐”不能替代 accepted ADR。
- Architecture 只描述已经落地的事实，不复制本页计划。
